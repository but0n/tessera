//! The map: a nested Voronoi treemap of the focused directory (ADR-0004).
//! Area is proportional to bytes on disk; nesting is containment; colour
//! families follow the hierarchy (ADR-0011), so the eye can trace a subtree.
//! Clicking opens a cell's top-level folder with a zoom (ADR-0010).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tessera_app::cartography::{CellKind, MapCell, MapLayout, MapRequest};
use tessera_domain::{ByteSize, NodeId};
use tessera_voronoi::{Polygon, Vec2};

use super::Ctx;
use super::widgets;
use crate::gfx::cell_flags;
use crate::state::{Action, FADE_DURATION, Transition, ViewState, ZOOM_DURATION, ZOOM_PATIENCE, ZoomDir};
use crate::theme::{self, Tone, alpha};
use crate::ui::{Align, Rect, Ui, ease_in_out, ease_out};

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
    a.take_top(66.0);
    Rect::new(a.x + theme::GUTTER, a.y, a.w - 2.0 * theme::GUTTER, a.h - theme::GUTTER)
}

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    let mut a = area;
    header(ui, ctx, view, a.take_top(66.0).inset_xy(theme::GUTTER + 4.0, 0.0), out);
    let map_rect = map_rect(area);
    ui.rect(map_rect.inset(-1.0), theme::PANEL, theme::RADIUS);

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
        if t < 1.0 || waiting {
            transition(ui, ctx, view, &tr, current.as_deref(), t.min(1.0), map_rect);
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
    // Zoom from 98.5% → 100% as it fades in; also absorbs window resizes.
    let k = 1.0 + 0.015 * (1.0 - opacity as f64);
    let full = Camera::full(&layout);
    let cam = Camera { x: full.w * (1.0 - k) / 2.0, y: full.h * (1.0 - k) / 2.0, w: full.w * k, h: full.h * k };
    let s = Screen::new(map_rect, cam);
    let interactive = !stale && age >= FADE_DURATION;
    let hovered = if interactive && ui.hovered(s.rect) { layout.hit(s.to_layout(ui.input.mouse)) } else { None };
    cells(ui, ctx, view, &layout, &s, Fade::all(opacity), hovered);
    if stale {
        widgets::spinner(ui, map_rect.cx() - 10.0, map_rect.cy(), theme::TEXT);
    }
    if let Some(i) = hovered {
        interact(ui, ctx, view, &layout, &s, i, out);
    }
}

/// One frame of a scope change at progress `t` ∈ [0, 1].
fn transition(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, tr: &Transition, new: Option<&MapLayout>, t: f32, map_rect: Rect) {
    let e = ease_in_out(t);
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
                let from = Screen::new(map_rect, Camera::full(old)).rect_of(b);
                let s = Screen { rect: lerp_rect(from, map_rect, e), cam: Camera::full(new), clip: map_rect };
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
            let to = Screen::new(map_rect, Camera::full(new)).rect_of(b);
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

fn header(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect, out: &mut Vec<Action>) {
    let n = ctx.inv.node(view.focus);
    let mut row = r;
    if n.parent().is_some() {
        let up = row.take_left(30.0);
        if widgets::icon_button(ui, Rect::new(up.x, up.cy() - 15.0, 30.0, 30.0), "←", false) {
            out.push(Action::FocusUp);
        }
        row.take_left(10.0);
    }
    let legend_w = 380.0;
    let mut title = row;
    let legend = title.take_right(legend_w);
    ui.label(Rect::new(title.x, title.cy() - 20.0, title.w, 22.0), ctx.inv.name(view.focus), theme::TITLE, theme::TEXT, Align::Start);
    let sub = format!("{} · {} files · {} folders · area ∝ size on disk", ByteSize(n.size()), widgets::count(n.files() as u64), widgets::count(n.dirs() as u64));
    ui.label(Rect::new(title.x, title.cy() + 3.0, title.w, 18.0), &sub, theme::SMALL, theme::TEXT_3, Align::Start);

    // Legend: the visual vocabulary of the map, spelled out.
    let items: [(&str, u8); 4] = [("Folder", 0), ("File", 1), ("Small items", 2), ("Marked", 3)];
    let mut x = legend.right();
    for (label, kind) in items.iter().rev() {
        let w = ui.measure(label, theme::SMALL) + 22.0;
        x -= w + 10.0;
        let sw = Rect::new(x, legend.cy() - 6.0, 12.0, 12.0);
        match kind {
            0 => {
                ui.rect(sw, theme::tone(0, Tone::Frame(1), 0), 3.5);
                ui.rect(sw.inset(3.0), theme::tone(0, Tone::Folder, 0), 1.5);
            }
            1 => ui.rect(sw, theme::tone(0, Tone::File, 0), 3.5),
            2 => ui.rect(sw, theme::tone(0, Tone::Rest, 0), 3.5),
            _ => ui.stripes(sw, theme::mix(theme::tone(0, Tone::File, 0), theme::RECLAIM, 0.3), theme::RECLAIM, 3.5),
        }
        ui.text(x + 18.0, legend.cy() - 8.0, label, theme::SMALL, theme::TEXT_2);
    }
}

struct LabelCand {
    idx: usize,
    center: Vec2,
    inradius: f64,
    area: f64,
    alpha: f32,
    hue: u16,
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
        let gap = if c.depth == 1 { 1.75 } else { 1.15 };
        let want_r = (if c.depth == 1 { 7.0 } else { 4.5 }) * zoom.min(3.0);
        let inr = poly.inradius_at_centroid();
        let r = want_r.min(inr * 0.3);
        let mut shape = poly.inset(gap + r);
        if shape.is_empty() {
            shape = poly.inset(gap * 0.5);
            if shape.is_empty() {
                continue;
            }
        }

        let tone = match c.kind {
            CellKind::Others { .. } => Tone::Rest,
            CellKind::Dir if c.has_children => Tone::Frame(c.depth),
            CellKind::Dir => Tone::Folder,
            CellKind::File => Tone::File,
        };
        let color = theme::tone(fam[i], tone, c.rank);
        let light = if matches!(tone, Tone::Frame(_) | Tone::Rest) { 0.45 } else { 1.0 };
        let others = matches!(c.kind, CellKind::Others { .. });
        let mut flags = 0;
        if hovered == Some(i) && !others {
            flags |= cell_flags::HOVER;
        }
        if scope == Some(i) {
            flags |= cell_flags::SCOPE;
        }
        if !others {
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

        let label_r = shape.inradius_at_centroid() + r;
        if o > 0.8 && label_r >= 15.0 && (c.depth == 1 || !c.has_children) {
            labels.push(LabelCand { idx: i, center: shape.centroid(), inradius: label_r, area: poly.area(), alpha: smoothstep(0.8, 1.0, o), hue: fam[i] });
        }
    }
    place_labels(ui, ctx, layout, labels, s.clip, scope);
    ui.set_clip(prev);
}

fn place_labels(ui: &mut Ui, ctx: &Ctx, layout: &MapLayout, mut cands: Vec<LabelCand>, bounds: Rect, scope: Option<usize>) {
    ui.layer = 1;
    cands.sort_by(|a, b| {
        let (ca, cb) = (&layout.cells[a.idx], &layout.cells[b.idx]);
        ca.depth.cmp(&cb.depth).then(b.area.total_cmp(&a.area))
    });
    let mut placed: Vec<Rect> = Vec::new();
    for cand in cands {
        let opacity = cand.alpha;
        let c: &MapCell = &layout.cells[cand.idx];
        let name: String = match c.kind {
            CellKind::Others { count } => format!("{} small items", widgets::count(count as u64)),
            _ => ctx.inv.name(c.node).to_owned(),
        };
        let (cx, cy) = (cand.center.x as f32, cand.center.y as f32);
        if c.has_children {
            // Frame label: a pill floating over the children.
            let style = if c.depth == 1 { theme::SMALL_STRONG } else { theme::CAPTION };
            let size = ByteSize(c.size).to_string();
            let max_name = (cand.inradius as f32 * 2.2).min(220.0);
            let name = ui.text.elide(&name, &style, max_name);
            let wn = ui.measure(&name, style);
            let ws = ui.measure(&size, theme::NUM);
            let w = wn + ws + 26.0;
            // Keep pills fully inside the map even for cells on its edge.
            let x = (cx - w / 2.0).clamp(bounds.x + 6.0, (bounds.right() - w - 6.0).max(bounds.x + 6.0));
            let y = (cy - 12.0).clamp(bounds.y + 6.0, bounds.bottom() - 30.0);
            let r = Rect::new(x, y, w, 24.0);
            if placed.iter().any(|p| p.overlaps(&r)) {
                continue;
            }
            ui.shadow(r, alpha(theme::BG, 0.5 * opacity), 12.0, 8.0);
            if scope == Some(cand.idx) {
                ui.rect_border(r, alpha(theme::RAISED, 0.96 * opacity), alpha(theme::TEXT, 0.35 * opacity), 1.0, 12.0);
            } else {
                ui.rect(r, alpha(theme::BG, 0.78 * opacity), 12.0);
            }
            ui.text(r.x + 10.0, r.cy() - style.line_height() / 2.0, &name, style, alpha(theme::TEXT, opacity));
            ui.text(r.x + 16.0 + wn, r.cy() - theme::NUM.line_height() / 2.0, &size, theme::NUM, alpha(theme::branch(cand.hue), opacity));
            placed.push(r.inset(-4.0));
        } else {
            // Leaf label: set directly on the cell, size beneath when it fits.
            let max_w = (cand.inradius as f32 * 1.8).max(10.0);
            let two_lines = cand.inradius >= 26.0;
            let style = if c.depth == 1 && cand.inradius >= 40.0 { theme::BODY_STRONG } else { theme::SMALL_STRONG };
            let mut name = ui.text.elide(&name, &style, max_w);
            if let CellKind::Others { count } = c.kind
                && name.ends_with('…')
            {
                name = format!("+{}", widgets::count(count as u64));
            }
            // A label reduced to a stub says nothing; leave the cell bare.
            let full = match c.kind {
                CellKind::Others { .. } => name.chars().count(),
                _ => ctx.inv.name(c.node).chars().count(),
            };
            if name.ends_with('…') && name.chars().count() < full.min(6) + 1 {
                continue;
            }
            if ui.measure(&name, style) > max_w + 1.0 {
                continue;
            }
            let wn = ui.measure(&name, style);
            let lh = style.line_height();
            let h = if two_lines { lh + 15.0 } else { lh };
            let r = Rect::new(cx - wn / 2.0, cy - h / 2.0, wn, h);
            if placed.iter().any(|p| p.overlaps(&r)) {
                continue;
            }
            ui.text(r.x, r.y, &name, style, alpha(theme::TEXT, 0.95 * opacity));
            if two_lines {
                let size = ByteSize(c.size).to_string();
                let ws = ui.measure(&size, theme::NUM);
                ui.text(cx - ws / 2.0, r.y + lh, &size, theme::NUM, alpha(theme::TEXT, 0.6 * opacity));
            }
            placed.push(r.inset(-3.0));
        }
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
    if enters || !others {
        ui.cursor = winit::window::CursorIcon::Pointer;
    }
    // A click must start and end in the same scope (no accidental drags).
    let same = layout.hit(s.to_layout(ui.input.press_pos)).map(|j| scope_of(layout, j)) == Some(sc);
    if ui.clicked(s.rect) && same {
        if ui.input.mods.alt_key() {
            if !others {
                out.push(Action::Select(c.node));
            }
        } else if enters {
            out.push(Action::Focus(scope.node));
        } else if scope.kind == CellKind::File {
            out.push(Action::Select(scope.node));
        }
    }
    if ui.right_clicked(s.rect) && !others {
        out.push(Action::ToggleMark(c.node));
    }
    let tint = theme::branch(if others { view.hue(ctx.inv, layout.focus) } else { view.family(ctx.inv, c.node) });
    tooltip(ui, ctx, c, tint, enters.then_some(scope.node));
}

fn tooltip(ui: &mut Ui, ctx: &Ctx, c: &MapCell, tint: theme::Rgba, opens: Option<NodeId>) {
    ui.layer = 2;
    let (mx, my) = ui.input.mouse;
    let w = 300.0;
    let others = matches!(c.kind, CellKind::Others { .. });
    let node = ctx.inv.node(c.node);
    // Content + padding + the hint strip.
    let h = if others { 66.0 } else { 88.0 } + 24.0 + 36.0;
    let screen = ui.screen();
    let x = if mx + 18.0 + w > screen.right() - 8.0 { mx - 18.0 - w } else { mx + 18.0 };
    let y = (my + 18.0).min(screen.bottom() - h - 8.0);
    let r = Rect::new(x, y, w, h);
    ui.shadow(r, alpha(theme::BG, 0.7), theme::RADIUS, 18.0);
    ui.rect_border(r, alpha(theme::RAISED, 0.97), theme::LINE, 1.0, theme::RADIUS);
    let mut a = r.inset_xy(14.0, 12.0);
    let title = if others { "Small items".to_string() } else { ctx.inv.name(c.node).to_string() };
    let t = a.take_top(20.0);
    widgets::dot(ui, t.x + 4.0, t.cy(), 8.0, tint);
    ui.label(Rect::new(t.x + 14.0, t.y, t.w - 14.0, t.h), &title, theme::BODY_STRONG, theme::TEXT, Align::Start);
    if others {
        let CellKind::Others { count } = c.kind else { unreachable!() };
        footer_hint(ui, r, ctx, opens, false);
        ui.label(a.take_top(18.0), &format!("{} entries in {} too small to draw", widgets::count(count as u64), ctx.inv.name(c.node)), theme::SMALL, theme::TEXT_2, Align::Start);
        let mut sz = a.take_top(28.0);
        let share = sz.take_right(120.0);
        widgets::size_text(ui, sz, c.size, theme::NUM_BIG, theme::TEXT, Align::Start);
        ui.label(share, &format!("{} of parent", widgets::percent(c.share)), theme::SMALL, theme::TEXT_3, Align::End);
        ui.layer = 0;
        return;
    }
    ui.label(a.take_top(16.0), &ctx.inv.relative_path(c.node), theme::MONO, theme::TEXT_3, Align::Start);
    a.take_top(6.0);
    let mut sz = a.take_top(28.0);
    let share = sz.take_right(120.0);
    widgets::size_text(ui, sz, node.size(), theme::NUM_BIG, if ctx.is_marked(c.node) { theme::RECLAIM } else { theme::TEXT }, Align::Start);
    let parent = node.parent().map_or(node.size(), |p| ctx.inv.node(p).size()).max(1);
    ui.label(share, &format!("{} of parent", widgets::percent(node.size() as f64 / parent as f64)), theme::SMALL, theme::TEXT_3, Align::End);
    if node.is_dir() {
        let line = format!("{} files · {} folders", widgets::count(node.files() as u64), widgets::count(node.dirs() as u64));
        let mut l = a.take_top(18.0);
        if node.hint().is_some() {
            let bw = ui.measure(node.hint().label(), theme::CAPTION) + 14.0;
            let b = l.take_right(bw);
            widgets::badge(ui, b.x, b.cy(), node.hint().label(), theme::hint(node.hint()));
        }
        ui.label(l, &line, theme::SMALL, theme::TEXT_2, Align::Start);
    } else {
        ui.label(a.take_top(18.0), node.category().label(), theme::SMALL, theme::TEXT_2, Align::Start);
    }
    footer_hint(ui, r, ctx, opens, true);
    ui.layer = 0;
}

/// Bottom strip of the tooltip: what a click does here.
fn footer_hint(ui: &mut Ui, r: Rect, ctx: &Ctx, opens: Option<NodeId>, markable: bool) {
    let strip = Rect::new(r.x + 1.0, r.bottom() - 31.0, r.w - 2.0, 30.0);
    ui.rect(Rect::new(strip.x, strip.y, strip.w, 1.0), theme::LINE, 0.0);
    let mut row = strip.inset_xy(13.0, 0.0);
    let text = match opens {
        Some(id) => format!("Click to open {}", ctx.inv.name(id)),
        None if markable => "Click to select".to_string(),
        None => String::new(),
    };
    if markable {
        let w = ui.measure("Right-click to mark", theme::SMALL);
        let m = row.take_right(w);
        ui.label(m, "Right-click to mark", theme::SMALL, theme::TEXT_3, Align::End);
        row.take_right(10.0);
    }
    if opens.is_some() {
        widgets::dot(ui, row.x + 3.0, row.cy(), 5.0, theme::TEXT_2);
        row.take_left(12.0);
    }
    ui.label(row, &text, theme::SMALL_STRONG, theme::TEXT_2, Align::Start);
}
