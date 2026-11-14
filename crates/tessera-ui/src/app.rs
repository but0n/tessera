//! Window, event loop and the per-frame cycle:
//! input → build UI (views emit actions) → render → apply actions.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tessera_app::Workspace;
use tessera_app::cartography::{Cartographer, CellKind, MapLayout};
use tessera_domain::{ByteSize, MarkOutcome, NodeId};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{CursorIcon, Window, WindowId};

use crate::gfx::Renderer;
use crate::gfx::text::Fonts;
use crate::state::{Action, Tab, Transition, ViewState, ZoomDir};
use crate::theme;
use crate::ui::{Frame, Input, Ui};
use crate::views::{self, Ctx};

/// User event: some background service has news (scan progress, a new map
/// layout, a finished commit).
#[derive(Debug, Clone, Copy)]
pub struct Wake;

struct Gfx {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    // Dropped last: the surface borrows the window.
    window: Arc<Window>,
}

pub struct App {
    pub ws: Workspace,
    pub carto: Cartographer,
    pub view: ViewState,
    input: Input,
    gfx: Option<Gfx>,
    /// Fonts being loaded in the background while the window comes up.
    pub fonts: Option<JoinHandle<Fonts>>,
    pub start: Instant,
    cursor: CursorIcon,
}

impl App {
    pub fn new(ws: Workspace, carto: Cartographer) -> App {
        App { ws, carto, view: ViewState::default(), input: Input::default(), gfx: None, fonts: None, start: Instant::now(), cursor: CursorIcon::Default }
    }

    fn request_redraw(&self) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn redraw(&mut self) {
        if let Some(report) = self.ws.poll() {
            let freed = ByteSize(report.freed);
            if report.failed.is_empty() {
                self.view.toast(format!("Moved {} item{} to the Trash · freed {}", report.trashed.len(), if report.trashed.len() == 1 { "" } else { "s" }, freed), true);
            } else {
                self.view.toast(format!("Freed {} · {} item(s) could not be moved: {}", freed, report.failed.len(), report.failed[0].1), false);
            }
            self.fix_dangling();
        }
        let Some(gfx) = self.gfx.as_mut() else { return };
        // Acquire the target first. Building a frame consumes input (scroll
        // deltas move things), so a frame that cannot be shown must not be
        // built: its input would be applied again by the retry.
        let texture = match gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated => {
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
                gfx.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                gfx.surface = gfx.instance.create_surface(gfx.window.clone()).expect("surface");
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
                gfx.window.request_redraw();
                return;
            }
            _ => {
                gfx.window.request_redraw();
                return;
            }
        };
        let scale = gfx.window.scale_factor() as f32;
        let logical = (gfx.config.width as f32 / scale, gfx.config.height as f32 / scale);
        let time = self.start.elapsed().as_secs_f32();
        let mut actions = Vec::new();
        let (frame, cursor, animating) = build_frame(&self.ws, &self.carto, &mut self.view, &self.input, &mut gfx.renderer, logical, scale, time, &mut actions);

        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        gfx.renderer.render(&frame, &view, (gfx.config.width, gfx.config.height), theme::BG);
        gfx.window.pre_present_notify();
        gfx.renderer.queue.present(texture);

        if cursor != self.cursor {
            gfx.window.set_cursor(cursor);
            self.cursor = cursor;
        }
        self.input.end_frame();
        let busy = animating || !self.ws.session.is_finished() || !actions.is_empty() || self.carto.is_busy();
        for a in actions {
            self.apply(a);
        }
        if busy {
            self.request_redraw();
        }
    }

    /// Points focus/selection at live nodes after entries were removed.
    fn fix_dangling(&mut self) {
        let inv = self.ws.session.inventory().read();
        let focus = self.view.focus;
        self.view.focus = inv.ancestors(focus).find(|&a| !inv.node(a).is_removed()).unwrap_or(NodeId::ROOT);
        if let Some(s) = self.view.selected
            && inv.node(s).is_removed()
        {
            self.view.selected = None;
        }
    }

    /// Changes the map scope, choosing the transition that keeps the user
    /// oriented: zoom in to a descendant, out to an ancestor, else fade.
    fn focus(&mut self, id: NodeId) {
        let view = &mut self.view;
        if id == view.focus {
            return;
        }
        let inv_arc = self.ws.session.inventory().clone();
        let inv = inv_arc.read();
        let old = self.carto.latest().filter(|l| l.focus == view.focus && l.is_from(&inv_arc));
        if let Some(old) = old
            && view.tab == Tab::Map
        {
            let (dir, pivot) = if inv.is_ancestor(view.focus, id) {
                // Deepest drawn ancestor-or-self of the target.
                let drawn = inv.ancestors(id).find(|&a| old.cells.iter().any(|c| c.node == a && c.kind != CellKind::File && !matches!(c.kind, CellKind::Others { .. })));
                match drawn {
                    Some(p) if p != view.focus => (ZoomDir::In, p),
                    _ => (ZoomDir::Fade, id),
                }
            } else if inv.is_ancestor(id, view.focus) {
                let child = inv.ancestors(view.focus).find(|&a| inv.node(a).parent() == Some(id)).unwrap_or(view.focus);
                (ZoomDir::Out, child)
            } else {
                (ZoomDir::Fade, id)
            };
            let now = Instant::now();
            if dir == ZoomDir::Out {
                // Coming back out: point at the folder just left.
                view.arrival = Some((pivot, None));
            }
            let pre = if dir == ZoomDir::Fade { 0.0 } else { std::mem::take(&mut view.gesture.handoff) };
            view.transition = Some(Transition { old, dir, pivot, start: (dir == ZoomDir::In).then_some(now), asked: now, pre });
        } else {
            // Nothing to animate from. A transition still in flight was aimed
            // at the scope being left; it must not play out over this one.
            view.transition = None;
        }
        view.gesture.handoff = 0.0;
        view.focus = id;
        view.selected = Some(id);
        drop(inv);
    }

    pub fn apply(&mut self, a: Action) {
        let view = &mut self.view;
        match a {
            Action::Select(id) => {
                view.selected = Some(id);
                view.reveal(&self.ws.session.inventory().read(), id);
            }
            Action::Focus(id) => self.focus(id),
            Action::FocusUp => {
                let parent = self.ws.session.inventory().read().get(self.view.focus).and_then(|n| n.parent());
                if let Some(p) = parent {
                    self.focus(p);
                }
            }
            Action::ToggleMark(id) => {
                let name = self.ws.session.inventory().read().name(id).to_owned();
                match self.ws.toggle_mark(id) {
                    MarkOutcome::Marked { absorbed } if absorbed > 0 => view.toast(format!("Marked {name} · replaces {absorbed} mark(s) inside it"), true),
                    MarkOutcome::CoveredBy(a) => {
                        let an = self.ws.session.inventory().read().name(a).to_owned();
                        view.toast(format!("Already included: {an} is marked"), false)
                    }
                    MarkOutcome::Rejected => view.toast("The scan root itself can't be marked", false),
                    _ => {}
                }
            }
            Action::Unmark(id) => self.ws.plan.unmark(id),
            Action::ToggleExpand(id) => {
                if !view.expanded.remove(&id) {
                    view.expanded.insert(id);
                }
            }
            Action::SetTab(t) => view.tab = t,
            Action::ShowList(l) => {
                view.list = l;
                view.list_scroll = 0.0;
                view.list_drag = None;
            }
            Action::AskCommit => view.confirm = !self.ws.plan.is_empty(),
            Action::CancelCommit => view.confirm = false,
            Action::Commit => {
                self.ws.commit();
                view.confirm = false;
            }
            Action::Rescan => {
                self.ws.rescan(None);
                view.reset();
            }
            Action::OpenFolder => {
                let start = self.ws.root();
                if let Some(dir) = rfd::FileDialog::new().set_directory(start).pick_folder() {
                    self.ws.rescan(Some(dir));
                    view.reset();
                }
            }
            Action::Reveal(id) => self.ws.reveal(id),
            Action::OpenPrivacySettings => self.ws.open_privacy_settings(),
            Action::DragWindow => {
                if let Some(g) = &self.gfx {
                    let _ = g.window.drag_window();
                }
                self.input.down = false;
            }
        }
    }
}

/// Builds one frame. Shared by the windowed app and headless snapshots.
#[allow(clippy::too_many_arguments)]
pub fn build_frame(
    ws: &Workspace,
    carto: &Cartographer,
    view: &mut ViewState,
    input: &Input,
    renderer: &mut Renderer,
    logical: (f32, f32),
    scale: f32,
    time: f32,
    actions: &mut Vec<Action>,
) -> (Frame, CursorIcon, bool) {
    let inv_arc = ws.session.inventory().clone();
    let inv = inv_arc.read();
    let insights = ws.session.insights();
    let layout: Option<Arc<MapLayout>> = carto.latest().filter(|l| l.is_from(&inv_arc));
    let commit = ws.commit_state();
    let ctx = Ctx {
        inv: &inv,
        inv_arc: &inv_arc,
        plan: &ws.plan,
        insights: &insights,
        stats: ws.session.stats(),
        volume: ws.volume,
        scanning: !ws.session.is_finished(),
        skips_protected: ws.skips_protected(),
        reader: ws.session.reader_name(),
        carto,
        layout,
        commit: &commit,
    };
    let mut ui = Ui::new(input, &mut renderer.text, logical, scale, time);
    views::draw(&mut ui, &ctx, view, actions);
    ui.finish()
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("Tessera").with_inner_size(LogicalSize::new(1480.0, 920.0)).with_min_inner_size(LogicalSize::new(1120.0, 700.0));
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs.with_titlebar_transparent(true).with_title_hidden(true).with_fullsize_content_view(true);
        }
        #[cfg(target_os = "linux")]
        {
            // The app id: what compositors, docks and window rules match on.
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs = attrs.with_name("tessera", "tessera");
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(event_loop.owned_display_handle())));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
        let caps = surface.get_capabilities(&adapter);
        // Non-sRGB target: blend in sRGB space like design tools do (ADR-0008).
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) { wgpu::CompositeAlphaMode::Opaque } else { caps.alpha_modes[0] },
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let fonts = self.fonts.take().and_then(|h| h.join().ok()).unwrap_or_else(|| Fonts::load(&theme::TEXT_STYLES));
        let renderer = Renderer::new(device, queue, format, fonts);
        self.gfx = Some(Gfx { instance, surface, config, renderer, window });
        self.request_redraw();
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: Wake) {
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        let scale = gfx.window.scale_factor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gfx.config.width = size.width.max(1);
                gfx.config.height = size.height.max(1);
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(scale);
                self.input.mouse = (p.x, p.y);
                self.input.inside = true;
            }
            WindowEvent::CursorLeft { .. } => self.input.inside = false,
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    let now = Instant::now();
                    let m = self.input.mouse;
                    self.input.double = self.input.last_click.is_some_and(|(t, p)| now - t < Duration::from_millis(380) && (p.0 - m.0).abs() < 5.0 && (p.1 - m.1).abs() < 5.0);
                    self.input.last_click = if self.input.double { None } else { Some((now, m)) };
                    self.input.down = true;
                    self.input.pressed = true;
                    self.input.press_pos = m;
                }
                (MouseButton::Left, ElementState::Released) => {
                    self.input.down = false;
                    self.input.released = true;
                }
                (MouseButton::Right, ElementState::Pressed) => self.input.right_pressed = true,
                (MouseButton::Back, ElementState::Pressed) => self.input.back_pressed = true,
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.input.wheel_notch = true;
                        (x * 36.0, y * 36.0)
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        let l = p.to_logical::<f32>(scale);
                        (l.x, l.y)
                    }
                };
                self.input.wheel.0 += dx;
                self.input.wheel.1 += dy;
            }
            WindowEvent::PinchGesture { delta, .. } => self.input.pinch += delta as f32,
            WindowEvent::ModifiersChanged(m) => self.input.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.input.keys.push(event.logical_key.clone());
            }
            _ => return,
        }
        self.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tessera_app::Services;
    use tessera_app::cartography::{MapParams, MapRequest};
    use tessera_domain::ScanPolicy;
    use tessera_infra::{SampleDesktop, SampleReader, SampleTrash, SampleVolume, sample_root};

    /// The sample tree, scanned, with the root's map solved.
    fn app() -> App {
        let waker: tessera_app::scan::Waker = Arc::new(|| {});
        let services = Services { reader: Arc::new(SampleReader), trash: Arc::new(SampleTrash), volume: Arc::new(SampleVolume), desktop: Arc::new(SampleDesktop) };
        let ws = Workspace::open(sample_root(), services, ScanPolicy::default(), waker.clone());
        let mut app = App::new(ws, Cartographer::new(MapParams::default(), waker));
        let wait = |done: &dyn Fn() -> bool| {
            let t = Instant::now();
            while !done() {
                assert!(t.elapsed() < Duration::from_secs(20), "timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        wait(&|| app.ws.session.is_finished());
        app.carto.request(MapRequest { inventory: app.ws.session.inventory().clone(), focus: NodeId::ROOT, width: 900.0, height: 600.0 });
        wait(&|| !app.carto.is_busy());
        app.view.tab = Tab::Map;
        app
    }

    #[test]
    fn refocus_before_the_layout_arrives_drops_the_transition() {
        let mut app = app();
        let layout = app.carto.latest().expect("root layout");
        let dir = layout.cells.iter().find(|c| c.depth == 1 && c.kind == CellKind::Dir).expect("a folder").node;
        app.apply(Action::Focus(dir));
        let tr = app.view.transition.as_ref().expect("zooms in");
        assert!(tr.dir == ZoomDir::In && tr.pivot == dir);
        // Back out before any layout of `dir` exists: there is nothing to
        // zoom out of, and the zoom into `dir` must not go on playing.
        app.apply(Action::FocusUp);
        assert_eq!(app.view.focus, NodeId::ROOT);
        assert!(app.view.transition.is_none());
    }
}
