//! Headless rendering to PNG: `tessera <root> --snapshot out.png [--view map]`.
//! Used for visual review and docs; exercises the exact same frame code.

use std::path::Path;
use std::time::{Duration, Instant};

use tessera_app::Workspace;
use tessera_app::cartography::Cartographer;
use tessera_domain::NodeId;

use crate::app::{App, build_frame};
use crate::gfx::Renderer;
use crate::gfx::text::Fonts;
use crate::state::Tab;
use crate::theme;
use crate::ui::Input;

pub struct Options {
    pub out: std::path::PathBuf,
    pub tab: Tab,
    pub size: (u32, u32),
    pub scale: f32,
    /// Select and mark a few entries to show those states.
    pub demo: bool,
    /// Focus the map on this child of the root.
    pub focus: Option<String>,
    pub hover: Option<(f32, f32)>,
    /// Click here once the map has settled (exercises the real input path).
    pub click: Option<(f32, f32)>,
    /// Capture this long after the click (mid-transition frames).
    pub after_ms: u64,
    /// Select the entry at this path below the root (reveals it in the tree).
    pub select: Option<String>,
    /// Scroll the tree by this many pixels after revealing.
    pub tree_scroll: f32,
    /// Scroll the map at (x, y) by dy points, spread over a few frames.
    pub scroll: Option<(f32, f32, f32)>,
}

pub fn run(ws: Workspace, carto: Cartographer, opts: Options) {
    let mut app = App::new(ws, carto);
    let t = Instant::now();
    while !app.ws.session.is_finished() {
        std::thread::sleep(Duration::from_millis(10));
    }
    eprintln!("scan: {:.2?}", t.elapsed());

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(device, queue, format, Fonts::load(&theme::TEXT_STYLES));

    app.view.tab = opts.tab;
    {
        let (ws, view) = (&app.ws, &mut app.view);
        let inv = ws.session.inventory().read();
        if let Some(name) = &opts.focus
            && let Some(c) = inv.children(NodeId::ROOT).find(|&c| inv.name(c) == name)
        {
            view.focus = c;
        }
    }
    if let Some(rel) = &opts.select {
        let (ws, view) = (&app.ws, &mut app.view);
        let inv = ws.session.inventory().read();
        let mut id = NodeId::ROOT;
        for part in rel.split('/') {
            match inv.children(id).find(|&c| inv.name(c) == part) {
                Some(c) => id = c,
                None => break,
            }
        }
        view.selected = Some(id);
        view.reveal(&inv, id);
        if inv.node(id).is_dir() {
            view.expanded.insert(id);
        }
    }
    if opts.demo {
        let (ws, view) = (&mut app.ws, &mut app.view);
        let ins = ws.session.insights();
        let inv = ws.session.inventory().clone();
        let kids = inv.read().children_by_size(view.focus);
        for &id in ins.reclaimable.iter().take(2) {
            ws.toggle_mark(id);
        }
        if let Some(&first) = kids.first().filter(|_| view.selected.is_none()) {
            view.selected = Some(first);
            view.reveal(&inv.read(), first);
        }
        if let Some(&second) = kids.get(1) {
            ws.toggle_mark(second);
        }
    }

    let (w, h) = opts.size;
    let logical = (w as f32 / opts.scale, h as f32 / opts.scale);
    let mut input = Input::default();
    if let Some(p) = opts.hover {
        input.mouse = p;
        input.inside = true;
    }
    // Frame 1 requests the map layout; wait for it, then let fades finish.
    let frame = |app: &mut App, input: &Input, renderer: &mut Renderer| {
        let mut actions = Vec::new();
        let t = app.start.elapsed().as_secs_f32();
        let (f, _, _) = build_frame(&app.ws, &app.carto, &mut app.view, input, renderer, logical, opts.scale, t, &mut actions);
        for a in actions {
            eprintln!("action: {a:?}");
            app.apply(a);
        }
        f
    };
    let settle = |app: &App| {
        let t = Instant::now();
        while app.carto.is_busy() && t.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(5));
        }
        t.elapsed()
    };
    frame(&mut app, &input, &mut renderer);
    eprintln!("layout: {:.2?}", settle(&app));
    // Second frame starts the fade-in; the third captures the settled state.
    frame(&mut app, &input, &mut renderer);
    std::thread::sleep(Duration::from_millis(350));
    let mut out = frame(&mut app, &input, &mut renderer);
    if opts.tree_scroll != 0.0 {
        app.view.tree_scroll += opts.tree_scroll;
        out = frame(&mut app, &input, &mut renderer);
    }
    if let Some(p) = opts.click {
        let clicked = Instant::now();
        input.mouse = p;
        input.inside = true;
        input.pressed = true;
        input.down = true;
        input.press_pos = p;
        frame(&mut app, &input, &mut renderer);
        input.end_frame();
        input.down = false;
        input.released = true;
        frame(&mut app, &input, &mut renderer);
        input.end_frame();
        frame(&mut app, &input, &mut renderer);
        eprintln!("relayout: {:.2?}", settle(&app));
        frame(&mut app, &input, &mut renderer);
        let wait = Duration::from_millis(opts.after_ms).saturating_sub(clicked.elapsed());
        std::thread::sleep(wait);
        out = frame(&mut app, &input, &mut renderer);
        if let Some(tr) = &app.view.transition {
            let t = tr.progress(Instant::now());
            eprintln!("transition {:?} at t={t:.2}", tr.dir);
        }
    }
    if let Some((x, y, dy)) = opts.scroll {
        let scrolled = Instant::now();
        input.mouse = (x, y);
        input.inside = true;
        frame(&mut app, &input, &mut renderer);
        let steps = 6;
        for _ in 0..steps {
            input.wheel.1 = dy / steps as f32;
            out = frame(&mut app, &input, &mut renderer);
            input.end_frame();
            std::thread::sleep(Duration::from_millis(16));
        }
        if app.view.transition.is_some() {
            frame(&mut app, &input, &mut renderer);
            eprintln!("relayout: {:.2?}", settle(&app));
            frame(&mut app, &input, &mut renderer);
            let wait = Duration::from_millis(opts.after_ms).saturating_sub(scrolled.elapsed());
            std::thread::sleep(wait);
            out = frame(&mut app, &input, &mut renderer);
            if let Some(tr) = &app.view.transition {
                let t = tr.progress(Instant::now());
                eprintln!("transition {:?} at t={t:.2} (pre {:.2})", tr.dir, tr.pre);
            }
        } else {
            eprintln!("lean: {:.2}", app.view.gesture.progress());
        }
    }
    let frame = out;

    let device = renderer.device.clone();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let tv = tex.create_view(&wgpu::TextureViewDescriptor::default());
    renderer.render(&frame, &tv, (w, h), theme::BG);

    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    renderer.queue.submit(Some(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = buf.slice(..).get_mapped_range().expect("mapped");
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let s = (y * row) as usize;
        pixels.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    write_png(&opts.out, w, h, &pixels);
    eprintln!("wrote {}", opts.out.display());
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(rgba).expect("png data");
}
