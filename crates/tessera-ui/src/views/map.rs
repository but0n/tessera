//! The map: a nested Voronoi treemap of the focused directory (ADR-0004).
//! Area is proportional to bytes on disk; nesting is containment; colour
//! families follow the hierarchy (ADR-0011), so the eye can trace a subtree.
//! Clicking or scrolling opens a cell's top-level folder with a zoom, and
//! scrolling back (or Esc) returns (ADR-0010).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tessera_app::cartography::{CellKind, MapCell, MapLayout, MapRequest};
use tessera_domain::{ByteSize, NodeId};
use tessera_voronoi::{Polygon, Vec2};

use super::Ctx;
use super::widgets;
use crate::gfx::cell_flags;
use crate::gfx::icons::Icon;
use crate::state::{Action, FADE_DURATION, Transition, ViewState, ZOOM_DURATION, ZOOM_PATIENCE, ZoomDir};
use crate::theme::{self, Tone, alpha};
use crate::ui::{Align, Rect, Ui, ease_in_out, ease_out};

const HEADER_H: f32 = 52.0;
/// Scroll distance (points) that makes one step through the hierarchy.
const SCROLL_STEP: f32 = 120.0;
/// Pinch magnification that makes one step.
const PINCH_STEP: f32 = 0.35;
/// Input pause that ends a gesture (and releases its latch).
const QUIET: Duration = Duration::from_millis(220);
/// How far the camera leans in (fraction of the zoom) while a scroll
/// builds up, and how far the map shrinks while scrolling out.
const LEAN_IN: f32 = 0.14;
const LEAN_OUT: f32 = 0.05;
/// Length of the arrival pulse (s).
const PULSE: f32 = 1.1;

/// Which part of a layout is on screen.
#[derive(Copy, Clone)]
struct Camera {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Camera {
    fn full(l: &MapLayout) -> Camera {
        Camera { x: 0.0, y: 0.0, w: l.width, h: l.height }
    }

    /// Smallest camera with the layout's aspect ratio that contains `b`.
    fn fit(l: &MapLayout, b: [f64; 4]) -> Camera {
        let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
        let aspect = l.width / l.height;
        let (w, h) = if bw / bh.max(1e-6) > aspect { (bw, bw / aspect) } else { (bh * aspect, bh) };
        Camera { x: (b[0] + b[2]) / 2.0 - w / 2.0, y: (b[1] + b[3]) / 2.0 - h / 2.0, w, h }
    }

    fn lerp(a: Camera, b: Camera, t: f64) -> Camera {
        Camera { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, w: a.w + (b.w - a.w) * t, h: a.h + (b.h - a.h) * t }
    }
}

/// A layout placed on screen: `cam` (layout space) maps onto `rect`.
struct Screen {
    rect: Rect,
    cam: Camera,
    clip: Rect,
}

impl Screen {
    fn new(rect: Rect, cam: Camera) -> Screen {
        Screen { rect, cam, clip: rect }
    }
    fn to_screen(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.rect.x as f64 + (p.x - self.cam.x) * self.rect.w as f64 / self.cam.w, self.rect.y as f64 + (p.y - self.cam.y) * self.rect.h as f64 / self.cam.h)
    }
    fn to_layout(&self, p: (f32, f32)) -> Vec2 {
        Vec2::new(self.cam.x + (p.0 - self.rect.x) as f64 * self.cam.w / self.rect.w as f64, self.cam.y + (p.1 - self.rect.y) as f64 * self.cam.h / self.rect.h as f64)
    }
    fn rect_of(&self, b: [f64; 4]) -> Rect {
        let (a, c) = (self.to_screen(Vec2::new(b[0], b[1])), self.to_screen(Vec2::new(b[2], b[3])));
        Rect::new(a.x as f32, a.y as f32, (c.x - a.x) as f32, (c.y - a.y) as f32)
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.w + (b.w - a.w) * t, a.h + (b.h - a.h) * t)
}

fn smoothstep(a: f32, b: f32, t: f32) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn lean_out(full: Camera, p: f32) -> Camera {
    let k = 1.0 + (LEAN_OUT * p) as f64;
    Camera { x: full.x - full.w * (k - 1.0) / 2.0, y: full.y - full.h * (k - 1.0) / 2.0, w: full.w * k, h: full.h * k }
}

fn bbox(c: &MapCell) -> [f64; 4] {
    let (min, max) = c.polygon.bbox();
    [min.x, min.y, max.x, max.y]
}

/// Index of the drawable (non-aggregate) cell for `node`.
fn cell_of(l: &MapLayout, node: NodeId) -> Option<usize> {
    l.cells.iter().position(|c| c.node == node && !matches!(c.kind, CellKind::Others { .. }))
}

/// The depth-1 ancestor of cell `i`: what a click on `i` enters.
fn scope_of(l: &MapLayout, mut i: usize) -> usize {
    while let Some(p) = l.cells[i].parent {
        i = p as usize;
    }
    i
}

/// Opacity of a layout's cells: those inside the `sub` subtree vs. the rest.
#[derive(Copy, Clone)]
struct Fade {
    sub: Option<usize>,
    inside: f32,
    outside: f32,
}

impl Fade {
    fn all(o: f32) -> Fade {
        Fade { sub: None, inside: o, outside: o }
    }
}

/// Where the map sits inside the centre area (also used to pre-solve the
/// layout while the overview is showing).
pub fn map_rect(area: Rect) -> Rect {
    let mut a = area;
    a.take_top(HEADER_H);
    Rect::new(a.x + theme::GUTTER, a.y, a.w - 2.0 * theme::GUTTER, a.h - theme::GUTTER)
}

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    let mut a = area;
    header(ui, ctx, view, a.take_top(HEADER_H).inset_xy(theme::GUTTER, 0.0), out);
    let map_rect = map_rect(area);
    ui.rect(map_rect, theme::PANEL, theme::RADIUS);

    let now = Instant::now();
    let current = ctx.layout.clone().filter(|l| l.focus == view.focus);
    view.map_hover = None;

    // 1. Scope transition.
    if let Some(mut tr) = view.transition.take() {
        if tr.start.is_none() && (current.is_some() || now - tr.asked > ZOOM_PATIENCE) {
            tr.start = Some(now);
        }
        let t = tr.start.map_or(0.0, |s| (now - s).as_secs_f32() / ZOOM_DURATION);
        let waiting = current.is_none() && now - tr.asked < ZOOM_PATIENCE * 4;
        hold_gesture(ui, view, map_rect);
        if t < 1.0 || waiting {
            let e = tr.pre + (1.0 - tr.pre) * ease_in_out(t.min(1.0));
            transition(ui, ctx, view, &tr, current.as_deref(), t.min(1.0), e, map_rect);
            if current.is_none() && t >= 1.0 {
                widgets::spinner(ui, map_rect.cx() - 10.0, map_rect.cy(), theme::TEXT);
            }
            view.transition = Some(tr);
            ui.animating = true;
            return;
        }
        // Landed: show the new layout as is, no extra fade.
        if let Some(l) = &current {
            view.shown = Some((Arc::as_ptr(l) as usize, l.focus, now - Duration::from_secs(1)));
        }
    }

    // 2. Steady state, fading in when a new scope first appears.
    let Some(layout) = current.or_else(|| ctx.layout.clone()) else {
        widgets::spinner(ui, map_rect.cx() - 10.0, map_rect.cy(), theme::TEXT_3);
        return;
    };
    let stale = layout.focus != view.focus;
    let ptr = Arc::as_ptr(&layout) as usize;
    match view.shown {
        Some((_, f, at)) if f == layout.focus => view.shown = Some((ptr, f, at)),
        _ => view.shown = Some((ptr, layout.focus, now)),
    }
    let age = view.shown.map_or(1.0, |s| (now - s.2).as_secs_f32());
    let mut opacity = ease_out(age / FADE_DURATION);
    if age < FADE_DURATION {
        ui.animating = true;
    }
    if stale {
        opacity *= 0.35;
    }
    if layout.cells.is_empty() {
        let msg = if ctx.scanning { "Waiting for data…" } else { "Nothing to show here" };
        ui.label(map_rect, msg, theme::BODY, theme::TEXT_3, Align::Center);
        return;
    }
    // The arrival pulse starts when the new scope is first on screen.
    if !stale && let Some((id, start)) = view.arrival {
        match start {
            None => view.arrival = Some((id, Some(now))),
            Some(t) if (now - t).as_secs_f32() > PULSE => view.arrival = None,
            _ => ui.animating = true,
        }
    }
    let full = Camera::full(&layout);
    let probe = Screen::new(map_rect, full);
    let interactive = !stale && age >= FADE_DURATION;
    let hovered = if interactive && ui.hovered(map_rect) { layout.hit(probe.to_layout(ui.input.mouse)) } else { None };
    let enter = hovered.map(|i| scope_of(&layout, i)).filter(|&sc| layout.cells[sc].kind == CellKind::Dir);
    let can_up = ctx.inv.node(view.focus).parent().is_some();

    // Scroll / pinch: lean toward the next level, step once it is reached.
    let lean = if interactive { gesture(ui, view, &layout, hovered, enter, can_up, map_rect, out) } else { 0.0 };
    // Zoom from 98.5% → 100% as it fades in; also absorbs window resizes.
    let k = 1.0 + 0.015 * (1.0 - opacity as f64);
    let mut cam = Camera { x: full.w * (1.0 - k) / 2.0, y: full.h * (1.0 - k) / 2.0, w: full.w * k, h: full.h * k };
    if lean > 0.0
        && let Some(sc) = enter
    {
        cam = Camera::lerp(cam, Camera::fit(&layout, bbox(&layout.cells[sc])), (LEAN_IN * lean) as f64);
    } else if lean < 0.0 {
        cam = lean_out(cam, -lean);
    }
    let s = Screen::new(map_rect, cam);
    let hovered = if lean == 0.0 { hovered } else { hovered.filter(|_| lean > 0.0) };
    cells(ui, ctx, view, &layout, &s, Fade::all(opacity), hovered);
    if stale {
        widgets::spinner(ui, map_rect.cx() - 10.0, map_rect.cy(), theme::TEXT);
    }
    if let Some(i) = hovered {
        interact(ui, ctx, view, &layout, &s, i, out);
    }
}

/// Feeds scroll and pinch into the view's gesture; returns the lean in
/// [-1, 1] to preview. Steps fire at ±1: into `enter`, or up a level.
#[allow(clippy::too_many_arguments)]
fn gesture(ui: &mut Ui, view: &mut ViewState, layout: &MapLayout, hovered: Option<usize>, enter: Option<usize>, can_up: bool, map_rect: Rect, out: &mut Vec<Action>) -> f32 {
    let now = Instant::now();
    let g = &mut view.gesture;
    let dt = g.last_frame.map_or(0.0, |t| (now - t).as_secs_f32().min(0.1));
    g.last_frame = Some(now);
    let input = ui.input;
    let mut delta = 0.0;
    if ui.hovered(map_rect) {
        delta = input.wheel.1 / SCROLL_STEP + input.pinch / PINCH_STEP;
        if input.wheel_notch && input.wheel.1 != 0.0 {
            // A mouse wheel notch is a deliberate, discrete step.
            delta = input.wheel.1.signum();
        }
    }
    if delta != 0.0 {
        let quiet = g.last_input.is_none_or(|t| now - t > QUIET);
        let reversed = g.acc != 0.0 && g.acc.signum() != delta.signum();
        if quiet || reversed {
            g.latched = false;
            if reversed {
                g.acc = 0.0;
            }
        }
        g.last_input = Some(now);
        if !g.latched {
            g.acc += delta;
        }
    } else if g.last_input.is_none_or(|t| now - t > Duration::from_millis(90)) {
        // Released short of a step: spring back.
        g.acc *= (-dt * 14.0).exp();
        if g.acc.abs() < 0.002 {
            g.acc = 0.0;
        }
    }
    // Nowhere to go: resist like a rubber band.
    let open = if g.acc > 0.0 { enter.is_some() } else { can_up };
    if !open {
        g.acc = g.acc.clamp(-0.22, 0.22);
    }
    if g.acc != 0.0 {
        ui.animating = true;
    }
    if open && g.acc.abs() >= 1.0 {
        let dir = g.acc.signum();
        g.latched = true;
        g.acc = 0.0;
        g.handoff = if dir > 0.0 { LEAN_IN } else { 0.0 };
        match enter {
            Some(sc) if dir > 0.0 => {
                out.push(Action::Focus(layout.cells[sc].node));
                view.arrival = hovered.map(|h| (layout.cells[h].node, None));
            }
            _ => out.push(Action::FocusUp),
        }
        return dir * if dir > 0.0 { 1.0 } else { 0.0 };
    }
    let p = g.acc.clamp(-1.0, 1.0);
    // Ease so the first bit of a scroll is clearly felt.
    p.signum() * (1.0 - (1.0 - p.abs()).powi(2))
}

/// While a transition runs, input only keeps the gesture's latch alive.
fn hold_gesture(ui: &Ui, view: &mut ViewState, map_rect: Rect) {
    if ui.hovered(map_rect) && (ui.input.wheel.1 != 0.0 || ui.input.pinch != 0.0) {
        view.gesture.last_input = Some(Instant::now());
    }
    view.gesture.acc = 0.0;
    view.gesture.last_frame = None;
}

/// One frame of a scope change at clock `t` ∈ [0, 1]; `e` is the eased
/// camera progress (which may start part-way, after a scroll preview).
#[allow(clippy::too_many_arguments)]
fn transition(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, tr: &Transition, new: Option<&MapLayout>, t: f32, e: f32, map_rect: Rect) {
    let old = &*tr.old;
    match tr.dir {
        ZoomDir::In => {
            // The camera dives into the pivot cell of the old layout; the new
            // layout grows out of that cell's bounds to fill the map.
            let Some(p) = cell_of(old, tr.pivot) else { return fade(ui, ctx, view, old, new, t, map_rect) };
            let b = bbox(&old.cells[p]);
            let cam = Camera::lerp(Camera::full(old), Camera::fit(old, b), e as f64);
            let inside = if new.is_some() { 1.0 - smoothstep(0.35, 0.8, t) } else { 1.0 };
            cells(ui, ctx, view, old, &Screen::new(map_rect, cam), Fade { sub: Some(p), inside, outside: 1.0 - smoothstep(0.0, 0.55, t) }, None);
            if let Some(new) = new {
                let from = Screen::new(map_rect, cam).rect_of(b);
                let s = Screen { rect: lerp_rect(from, map_rect, smoothstep(0.0, 1.0, e)), cam: Camera::full(new), clip: map_rect };
                cells(ui, ctx, view, new, &s, Fade::all(smoothstep(0.25, 0.75, t)), None);
            }
        }
        ZoomDir::Out => {
            // Mirror image: the new (parent) layout's camera pulls back from
            // the pivot cell while the old layout shrinks into it.
            let Some(new) = new else { return fade(ui, ctx, view, old, None, t, map_rect) };
            let Some(p) = cell_of(new, tr.pivot) else { return fade(ui, ctx, view, old, Some(new), t, map_rect) };
            let b = bbox(&new.cells[p]);
            let cam = Camera::lerp(Camera::fit(new, b), Camera::full(new), e as f64);
            cells(ui, ctx, view, new, &Screen::new(map_rect, cam), Fade { sub: Some(p), inside: smoothstep(0.3, 0.8, t), outside: smoothstep(0.05, 0.6, t) }, None);
            let to = Screen::new(map_rect, cam).rect_of(b);
            let s = Screen { rect: lerp_rect(map_rect, to, e), cam: Camera::full(old), clip: map_rect };
            cells(ui, ctx, view, old, &s, Fade::all(1.0 - smoothstep(0.3, 0.75, t)), None);
        }
        ZoomDir::Fade => fade(ui, ctx, view, old, new, t, map_rect),
    }
}

fn fade(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, old: &MapLayout, new: Option<&MapLayout>, t: f32, map_rect: Rect) {
    let k = 1.0 + 0.03 * t as f64;
    let full = Camera::full(old);
    let cam = Camera { x: full.w * (1.0 - 1.0 / k) / 2.0, y: full.h * (1.0 - 1.0 / k) / 2.0, w: full.w / k, h: full.h / k };
    cells(ui, ctx, view, old, &Screen::new(map_rect, cam), Fade::all(1.0 - smoothstep(0.0, 0.6, t)), None);
    if let Some(new) = new {
        cells(ui, ctx, view, new, &Screen::new(map_rect, Camera::full(new)), Fade::all(smoothstep(0.3, 1.0, t)), None);
    }
}

pub fn request_layout(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect) {
    let size = (r.w.round() as u32, r.h.round() as u32);
    if size.0 < 64 || size.1 < 64 {
        return;
    }
    let generation = ctx.inv.generation();
    let due = match view.last_request {
        None => true,
        Some((f, g, s, at)) => {
            let age = at.elapsed();
            let owed = s != size || g != generation;
            if owed {
                ui.animating = true;
            }
            f != view.focus || (s != size && age > Duration::from_millis(70)) || (g != generation && age > Duration::from_millis(if ctx.scanning { 900 } else { 100 }))
        }
    };
    if due {
        ctx.carto.request(MapRequest { inventory: ctx.inv_arc.clone(), focus: view.focus, width: size.0 as f64, height: size.1 as f64 });
        view.last_request = Some((view.focus, generation, size, Instant::now()));
    }
}

/// Back button, the path to the scope (each step clickable), and the
/// scope's totals on the right.
fn header(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect, out: &mut Vec<Action>) {
    let n = ctx.inv.node(view.focus);
    let mut row = r;
    let up = row.take_left(28.0);
    if widgets::icon_button(ui, Rect::new(up.x, up.cy() - 14.0, 28.0, 28.0), Icon::ArrowLeft, "Up one level  Esc", n.parent().is_some()) {
        out.push(Action::FocusUp);
    }
    row.take_left(8.0);

    // Totals: size, then what it is made of.
    let counts = format!("{} files · {} folders", widgets::count(n.files() as u64), widgets::count(n.dirs() as u64));
    let cw = ui.measure(&counts, theme::SMALL);
    let totals = row.take_right(cw + 96.0);
    ui.label(Rect { x: totals.right() - cw, w: cw, ..totals }, &counts, theme::SMALL, theme::TEXT_3, Align::End);
    widgets::size_text(ui, Rect { w: totals.w - cw - 12.0, ..totals }, n.size(), theme::NUM_STRONG, theme::TEXT_2, Align::End);
    row.take_right(12.0);
    breadcrumb(ui, ctx, view, row, out);
}

fn breadcrumb(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect, out: &mut Vec<Action>) {
    let mut chain: Vec<NodeId> = ctx.inv.ancestors(view.focus).collect();
    chain.reverse();
    let sep = 18.0;
    let style = |last: bool| if last { theme::TITLE } else { theme::BODY };
    let widths: Vec<f32> = chain.iter().enumerate().map(|(i, &id)| ui.measure(ctx.inv.name(id), style(i + 1 == chain.len())).min(220.0) + 12.0).collect();
    // Keep the tail visible: fold leading steps that would not fit.
    let root_w = 22.0;
    let mut start = 0;
    while start + 1 < chain.len() && root_w + widths[start..].iter().sum::<f32>() + sep * (chain.len() - start) as f32 + 24.0 > r.w {
        start += 1;
    }
    let mut x = r.x;
    let cy = r.cy();
    ui.icon(x, cy - 8.0, 16.0, Icon::Drive, theme::TEXT_3);
    x += root_w;
    if start > 0 {
        x += ui.text(x, cy - 9.0, "…", theme::BODY, theme::TEXT_3) + 4.0;
        ui.icon(x, cy - 6.0, 12.0, Icon::ChevronRight, theme::TEXT_3);
        x += sep;
    }
    for (i, &id) in chain.iter().enumerate().skip(start) {
        let last = i + 1 == chain.len();
        let seg = Rect::new(x - 6.0, cy - 13.0, widths[i], 26.0);
        let (hover, click) = if last { (false, false) } else { ui.interact(seg) };
        if hover {
            ui.rect(seg, theme::HOVER, theme::RADIUS_SM);
        }
        let ink = if last || hover { theme::TEXT } else { theme::TEXT_2 };
        ui.label(seg.inset_xy(6.0, 0.0), ctx.inv.name(id), style(last), ink, Align::Start);
        if click {
            out.push(Action::Focus(id));
        }
        x += widths[i];
        if !last {
            ui.icon(x - 3.0, cy - 6.0, 12.0, Icon::ChevronRight, theme::TEXT_3);
            x += sep - 6.0;
        }
    }
}

struct LabelCand {
    idx: usize,
    center: Vec2,
    inradius: f64,
    area: f64,
    alpha: f32,
    hue: u16,
    /// On the path to the followed entry: placed before everything else.
    first: bool,
}

/// Draws the cells of `layout` through `s`. `hovered` is the cell under the
/// pointer; its scope (depth-1 ancestor) is lifted to show what a click
/// would open.
fn cells(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, layout: &MapLayout, s: &Screen, fade: Fade, hovered: Option<usize>) {
    let prev = ui.set_clip(s.clip);
    ui.layer = 0;
    let zoom = (s.rect.w as f64 / s.cam.w).max(1.0);

    let in_sub: Vec<bool> = match fade.sub {
        Some(root) => {
            let mut v = vec![false; layout.cells.len()];
            for (i, c) in layout.cells.iter().enumerate() {
                v[i] = i == root || c.parent.is_some_and(|p| v[p as usize]);
            }
            v
        }
        None => Vec::new(),
    };
    let scope = hovered.map(|i| scope_of(layout, i));
    if let (Some(h), Some(sc)) = (hovered, scope) {
        view.map_hover = Some((layout.cells[h].node, layout.cells[sc].node));
    }

    // Colour family per cell: depth-1 cells carry their own hue, deeper
    // cells inherit it.
    let focus_hue = view.hue(ctx.inv, layout.focus);
    let mut fam: Vec<u16> = Vec::with_capacity(layout.cells.len());
    for c in &layout.cells {
        let h = match (c.parent, c.kind) {
            (Some(p), _) => fam[p as usize],
            (None, CellKind::Others { .. }) => focus_hue,
            (None, _) => view.hue(ctx.inv, c.node),
        };
        fam.push(h);
    }

    // The entry the user is following and its ancestors: labelled first.
    let anchor = view.arrival.map(|a| a.0).or(view.selected);
    let chain: Vec<NodeId> = anchor.map(|a| ctx.inv.ancestors(a).collect()).unwrap_or_default();
    let pulse = match view.arrival {
        Some((id, Some(t))) => {
            let age = t.elapsed().as_secs_f32();
            Some((id, smoothstep(0.0, 0.12, age) * (1.0 - smoothstep(0.15, PULSE, age))))
        }
        _ => None,
    };

    let mut labels: Vec<LabelCand> = Vec::new();
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(32);
    for (i, c) in layout.cells.iter().enumerate() {
        let o = if in_sub.get(i).copied().unwrap_or(false) {
            fade.inside
        } else if fade.sub.is_some() {
            fade.outside
        } else {
            fade.inside
        };
        if o <= 0.01 {
            continue;
        }
        let poly = Polygon { pts: c.polygon.pts.iter().map(|&p| s.to_screen(p)).collect() };
        let (min, max) = poly.bbox();
        if max.x < s.clip.x as f64 || min.x > s.clip.right() as f64 || max.y < s.clip.y as f64 || min.y > s.clip.bottom() as f64 {
            continue;
        }
        // Grout between cells thins with depth; corners round off in
        // proportion to the cell, so big cells read as soft pebbles.
        let gap = match c.depth {
            1 => 1.6,
            2 => 1.1,
            _ => 0.8,
        } * zoom.min(2.0);
        // Concentric: a child's radius is about its parent's minus the
        // padding between them, so no corner pokes out of a rounded parent.
        let want_r = match c.depth {
            1 => 13.0,
            2 => 8.5,
            _ => 6.0,
        } * zoom.min(3.0);
        let inr = poly.inradius_at_centroid();
        let mut r = want_r.min(inr * 0.55);
        let mut shape = poly.inset(gap + r);
        if shape.is_empty() {
            // Too thin to round: draw it square rather than overlap.
            r = 0.0;
            shape = poly.inset(gap * 0.5);
            if shape.is_empty() {
                continue;
            }
        }

        let others = matches!(c.kind, CellKind::Others { .. });
        let tone = match c.kind {
            CellKind::Others { .. } => Tone::Rest,
            CellKind::Dir if c.has_children => Tone::Frame(c.depth),
            CellKind::Dir => Tone::Folder,
            CellKind::File => Tone::File,
        };
        let color = theme::tone(fam[i], tone, c.rank);
        let light = if matches!(tone, Tone::Frame(_)) { 0.6 } else { 1.0 };
        let mut flags = 0;
        if others {
            flags |= cell_flags::OTHERS;
        }
        if hovered == Some(i) && !others {
            flags |= cell_flags::HOVER;
        }
        if scope == Some(i) && layout.cells[i].kind == CellKind::Dir {
            flags |= cell_flags::SCOPE;
        }
        if others {
            // Small items share their folder's fate.
            if ctx.is_marked(c.node) || ctx.is_covered(c.node) {
                flags |= cell_flags::COVERED;
            }
        } else {
            if view.selected == Some(c.node) {
                flags |= cell_flags::SELECTED;
            }
            if ctx.is_marked(c.node) {
                flags |= cell_flags::MARKED;
            } else if ctx.is_covered(c.node) {
                flags |= cell_flags::COVERED;
            }
        }
        pts.clear();
        pts.extend(shape.pts.iter().map(|p| [p.x as f32, p.y as f32]));
        ui.cell(&pts, color, theme::RECLAIM, r as f32, flags, light, o);
        if let Some((id, k)) = pulse
            && id == c.node
            && !others
            && k > 0.0
        {
            ui.cell(&pts, theme::TEXT, theme::RECLAIM, r as f32, 0, 0.0, 0.3 * k * o);
        }

        let label_r = shape.inradius_at_centroid() + r;
        // Folders are labelled at every depth, outermost first: the name on
        // screen is the folder a click opens, and it keeps its label as the
        // zoom brings it closer.
        if o > 0.8 && label_r >= 14.0 && !others {
            let first = chain.contains(&c.node);
            labels.push(LabelCand { idx: i, center: shape.centroid(), inradius: label_r, area: poly.area(), alpha: smoothstep(0.8, 1.0, o), hue: fam[i], first });
        }
    }
    place_labels(ui, ctx, layout, labels, s.clip);
    ui.set_clip(prev);
}

/// Labels sized to their cell: big cells get big type and their size, small
/// ones just a name, the smallest nothing (the tooltip has it).
///
/// A folder whose largest child covers most of it is, to the eye, that
/// child: the folder shows only as a thin rim. Such chains get one label at
/// the region they share. It names the cell actually seen, with the folders
/// around it as a small path above (`aurora-engine ›` over `target`), so neither
/// name is lost and none is misplaced. Folders showing several children
/// float their name on a soft halo.
fn place_labels(ui: &mut Ui, ctx: &Ctx, layout: &MapLayout, mut cands: Vec<LabelCand>, bounds: Rect) {
    ui.layer = 1;
    cands.sort_by(|a, b| {
        let (ca, cb) = (&layout.cells[a.idx], &layout.cells[b.idx]);
        b.first.cmp(&a.first).then(ca.depth.cmp(&cb.depth)).then(b.area.total_cmp(&a.area))
    });
    let n = layout.cells.len();
    // Dominant child: fills its parent, which then shows only as a rim.
    let mut dominant: Vec<Option<usize>> = vec![None; n];
    for (i, c) in layout.cells.iter().enumerate() {
        if let Some(p) = c.parent
            && !matches!(c.kind, CellKind::Others { .. })
            && c.polygon.area() >= 0.7 * layout.cells[p as usize].polygon.area()
        {
            dominant[p as usize] = Some(i);
        }
    }
    let mut absorbed = vec![false; n];
    for d in dominant.iter().flatten() {
        absorbed[*d] = true;
    }
    let mut cand_of: Vec<Option<usize>> = vec![None; n];
    for (k, c) in cands.iter().enumerate() {
        cand_of[c.idx] = Some(k);
    }

    let mut placed: Vec<Rect> = Vec::new();
    for head in &cands {
        // Chain members are labelled through their head.
        if absorbed[head.idx] {
            continue;
        }
        let mut chain = vec![head.idx];
        while let Some(d) = dominant[*chain.last().unwrap()] {
            chain.push(d);
        }
        let end = *chain.last().unwrap();
        let cand = cand_of[end].map_or(head, |k| &cands[k]);
        let opacity = head.alpha.min(cand.alpha);
        let c: &MapCell = &layout.cells[end];
        let top = &layout.cells[head.idx];
        let full_name = ctx.inv.name(c.node);
        // Deeper labels step down a size so the hierarchy reads in type too.
        let room = cand.inradius as f32 * if top.depth == 1 { 1.0 } else { 0.78 };
        let (style, with_size) = if room >= 68.0 {
            (theme::MAP_L, true)
        } else if room >= 38.0 {
            (theme::MAP_M, true)
        } else if room >= 17.0 {
            (theme::MAP_S, false)
        } else {
            continue;
        };
        let (cx, cy) = (cand.center.x as f32, cand.center.y as f32);
        let frame = c.has_children || chain.len() > 1;
        let max_w = cand.inradius as f32 * if frame { 2.4 } else { 1.8 };
        let name = ui.text.elide(full_name, &style, max_w);
        // A label cut down to a stub says nothing; leave the cell bare
        // (folders keep a shorter stub, they are worth naming).
        let (kept, total) = (name.chars().count().saturating_sub(1), full_name.chars().count());
        if name.ends_with('…') && (kept < total.min(6) || (!frame && c.kind == CellKind::File && (kept as f32) < total as f32 * 0.7)) {
            continue;
        }
        // The folders this cell fills, outermost first.
        let path = (chain.len() > 1 && room >= 30.0).then(|| {
            let names: Vec<&str> = chain[..chain.len() - 1].iter().map(|&i| ctx.inv.name(layout.cells[i].node)).collect();
            ui.text.elide(&format!("{} ›", names.join(" › ")), &theme::MAP_PATH, max_w)
        });
        let wn = ui.measure(&name, style);
        let wp = path.as_ref().map_or(0.0, |p| ui.measure(p, theme::MAP_PATH));
        let size = ByteSize(c.size).to_string();
        let ws = if with_size { ui.measure(&size, theme::MAP_NUM) } else { 0.0 };
        let lh = style.line_height();
        let ph = if path.is_some() { theme::MAP_PATH.line_height() } else { 0.0 };
        let h = ph + if with_size { lh + 14.0 } else { lh };
        let w = wn.max(ws).max(wp);
        let x = (cx - w / 2.0).clamp(bounds.x + 8.0, (bounds.right() - w - 8.0).max(bounds.x + 8.0));
        let y = (cy - h / 2.0).clamp(bounds.y + 6.0, bounds.bottom() - h - 6.0);
        let r = Rect::new(x, y, w, h);
        if placed.iter().any(|p| p.overlaps(&r)) {
            continue;
        }
        if frame {
            // Over busy content: a soft dark halo, no hard pill.
            let halo = if c.has_children { if top.depth == 1 { 0.5 } else { 0.38 } } else { 0.28 };
            ui.shadow(r.inset_xy(-10.0, -5.0), alpha(theme::BG, halo * opacity), 16.0, 12.0);
        }
        let mut y = r.y;
        if let Some(p) = &path {
            // A top-level folder keeps its family colour in the path.
            let ink = if top.depth == 1 { theme::branch(head.hue) } else { alpha(theme::TEXT, 0.62) };
            ui.text(r.cx() - wp / 2.0, y, p, theme::MAP_PATH, alpha(ink, ink[3] * opacity));
            y += ph;
        }
        let ink = if frame { theme::TEXT } else { alpha(theme::TEXT, 0.94) };
        ui.text(r.cx() - wn / 2.0, y, &name, style, alpha(ink, ink[3] * opacity));
        if with_size {
            let size_ink = if c.has_children { theme::branch(cand.hue) } else { alpha(theme::TEXT, 0.58) };
            ui.text(r.cx() - ws / 2.0, y + lh, &size, theme::MAP_NUM, alpha(size_ink, size_ink[3] * opacity));
        }
        // Folder names claim more room, so nested ones never stack up.
        placed.push(if frame { r.inset_xy(-18.0, -14.0) } else { r.inset(-4.0) });
    }
    ui.layer = 0;
}

/// Pointer over cell `i`. A click opens its scope (the depth-1 folder it
/// lies in): the map always descends one level, so the hierarchy unfolds
/// predictably. ⌥-click selects the exact entry; right-click marks it.
fn interact(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, layout: &MapLayout, s: &Screen, i: usize, out: &mut Vec<Action>) {
    let c = &layout.cells[i];
    let others = matches!(c.kind, CellKind::Others { .. });
    let sc = scope_of(layout, i);
    let scope = &layout.cells[sc];
    let enters = scope.kind == CellKind::Dir;
    let alt = ui.input.mods.alt_key();
    // The cursor says what a click does: zoom in, or pick.
    if enters && !alt {
        ui.cursor = winit::window::CursorIcon::ZoomIn;
    } else if !others {
        ui.cursor = winit::window::CursorIcon::Pointer;
    }
    // A click must start and end in the same scope (no accidental drags).
    let same = layout.hit(s.to_layout(ui.input.press_pos)).map(|j| scope_of(layout, j)) == Some(sc);
    if ui.clicked(s.rect) && same {
        if alt {
            if !others {
                out.push(Action::Select(c.node));
            }
        } else if enters {
            // Open one level, and keep what was clicked selected so it
            // stays identifiable (ring, label, pulse) in the new view.
            out.push(Action::Focus(scope.node));
            out.push(Action::Select(c.node));
            view.arrival = Some((c.node, None));
        } else if scope.kind == CellKind::File {
            out.push(Action::Select(scope.node));
        }
    }
    if ui.right_clicked(s.rect) && !others {
        out.push(Action::ToggleMark(c.node));
    }
    let tint = theme::branch(if others { view.hue(ctx.inv, layout.focus) } else { view.family(ctx.inv, c.node) });
    tooltip(ui, ctx, c, tint, (enters && !alt).then_some(scope.node));
}

/// Hover card: what this is, how big, and (in the footer) what a click opens.
fn tooltip(ui: &mut Ui, ctx: &Ctx, c: &MapCell, tint: theme::Rgba, opens: Option<NodeId>) {
    ui.layer = 2;
    let (mx, my) = ui.input.mouse;
    let w = 280.0;
    let others = matches!(c.kind, CellKind::Others { .. });
    let node = ctx.inv.node(c.node);
    let strip = if opens.is_some() { 32.0 } else { 0.0 };
    let h = 104.0 + strip;
    let screen = ui.screen();
    let x = if mx + 18.0 + w > screen.right() - 8.0 { mx - 18.0 - w } else { mx + 18.0 };
    let y = (my + 18.0).min(screen.bottom() - h - 8.0);
    let r = Rect::new(x, y, w, h);
    ui.shadow(r, alpha(theme::BG, 0.7), theme::RADIUS, 18.0);
    ui.rect_border(r, alpha(theme::RAISED, 0.97), theme::LINE, 1.0, theme::RADIUS);
    let mut a = r.inset_xy(14.0, 13.0);

    // Title row: icon, name, size.
    let mut t = a.take_top(20.0);
    let (icon, title, bytes) = match c.kind {
        CellKind::Others { count } => (Icon::Dots, format!("{} small items", widgets::count(count as u64)), c.size),
        _ => (widgets::kind_icon(node), ctx.inv.name(c.node).to_string(), node.size()),
    };
    ui.icon(t.x, t.cy() - 8.0, 16.0, icon, tint);
    t.take_left(24.0);
    let size_w = widgets::size_text(ui, t, bytes, theme::NUM_STRONG, if ctx.is_marked(c.node) && !others { theme::RECLAIM } else { theme::TEXT }, Align::End);
    t.take_right(size_w + 10.0);
    ui.label(t, &title, theme::BODY_STRONG, theme::TEXT, Align::Start);
    a.take_top(4.0);
    let path = if others { format!("in {}", ctx.inv.relative_path(c.node)) } else { ctx.inv.relative_path(c.node) };
    ui.label(a.take_top(16.0), &path, theme::MONO, theme::TEXT_3, Align::Start);
    a.take_top(12.0);

    // Share of the parent, as a bar.
    let share = if others {
        c.share
    } else {
        let parent = node.parent().map_or(node.size(), |p| ctx.inv.node(p).size()).max(1);
        node.size() as f64 / parent as f64
    };
    let mut m = a.take_top(14.0);
    let pct = format!("{} of parent", widgets::percent(share));
    let pw = ui.measure(&pct, theme::SMALL);
    ui.label(m.take_right(pw), &pct, theme::SMALL, theme::TEXT_3, Align::End);
    widgets::meter(ui, Rect::new(m.x, m.cy() - 2.0, m.w - 12.0, 4.0), share as f32, tint);
    a.take_top(8.0);

    // Composition, or kind; plus a hint badge.
    let mut l = a.take_top(18.0);
    let line = if others {
        "too small to draw individually".to_string()
    } else if node.is_dir() {
        format!("{} files · {} folders", widgets::count(node.files() as u64), widgets::count(node.dirs() as u64))
    } else {
        node.category().label().to_string()
    };
    if !others && node.hint().is_some() {
        let bw = ui.measure(node.hint().label(), theme::CAPTION) + 14.0;
        let b = l.take_right(bw);
        widgets::badge(ui, b.x, b.cy(), node.hint().label(), theme::hint(node.hint()));
    }
    ui.label(l, &line, theme::SMALL, theme::TEXT_2, Align::Start);

    if let Some(id) = opens {
        let strip = Rect::new(r.x + 1.0, r.bottom() - strip, r.w - 2.0, strip - 1.0);
        ui.rect(Rect::new(strip.x, strip.y, strip.w, 1.0), theme::LINE, 0.0);
        let mut row = strip.inset_xy(13.0, 0.0);
        ui.icon(row.x, row.cy() - 7.0, 14.0, Icon::Mouse, theme::TEXT_3);
        row.take_left(22.0);
        ui.label(row, &format!("Click or scroll to open {}", ctx.inv.name(id)), theme::SMALL, theme::TEXT_2, Align::Start);
    }
    ui.layer = 0;
}
