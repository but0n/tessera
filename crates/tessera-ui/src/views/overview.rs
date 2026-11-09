//! Overview: the whole story on one page, top to bottom —
//! where the disk is → what it holds → what is safe to free.

use tessera_domain::{ByteSize, NodeId};
use tessera_voronoi::{Polygon, Settings, Vec2};

use super::Ctx;
use super::widgets::{self, ButtonKind};
use crate::gfx::cell_flags;
use crate::state::{Action, Tab, ViewState};
use crate::theme::{self, alpha, mix};
use crate::ui::{Align, Rect, Ui};

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    if ui.hovered(area) {
        view.overview_scroll -= ui.input.wheel.1;
    }
    let w = (area.w - 56.0).min(1120.0);
    let x0 = area.x + (area.w - w) / 2.0;
    let prev = ui.set_clip(area);
    let mut y = area.y + 24.0 - view.overview_scroll;

    if ctx.skips_protected {
        notice(ui, Rect::new(x0, y, w, 52.0), out);
        y += 52.0 + 14.0;
    }
    volume(ui, ctx, Rect::new(x0, y, w, 160.0));
    y += 160.0 + 14.0;
    kpis(ui, ctx, Rect::new(x0, y, w, 98.0));
    y += 98.0 + 14.0;
    let left_w = (w * 0.52).floor();
    composition(ui, ctx, view, Rect::new(x0, y, left_w, 336.0));
    regenerable(ui, ctx, view, Rect::new(x0 + left_w + 14.0, y, w - left_w - 14.0, 336.0), out);
    y += 336.0 + 14.0;
    let rows = ctx.insights.largest_files.len().min(12);
    let lh = 58.0 + rows.max(1) as f32 * 34.0;
    largest(ui, ctx, view, Rect::new(x0, y, w, lh), out);
    y += lh + 28.0;

    let content_h = y + view.overview_scroll - area.y;
    view.overview_scroll = view.overview_scroll.clamp(0.0, (content_h - area.h).max(0.0));
    ui.set_clip(prev);
    if let Some(o) = widgets::scrollbar(ui, area, content_h, view.overview_scroll, &mut view.overview_drag) {
        view.overview_scroll = o;
    }
}

fn notice(ui: &mut Ui, r: Rect, out: &mut Vec<Action>) {
    ui.rect_border(r, alpha(theme::ACCENT, 0.07), alpha(theme::ACCENT, 0.25), 1.0, theme::RADIUS);
    let mut c = r.inset_xy(18.0, 0.0);
    let b = c.take_right(190.0).inset_xy(0.0, 10.0);
    widgets::dot(ui, c.x + 4.0, c.cy(), 8.0, theme::ACCENT);
    ui.label(
        Rect::new(c.x + 18.0, c.y, c.w - 18.0, c.h),
        "Privacy-protected folders (Mail, Messages, app containers…) are listed but not measured.",
        theme::BODY,
        theme::TEXT_2,
        Align::Start,
    );
    if widgets::button(ui, b, "Grant Full Disk Access…", ButtonKind::Secondary, true) {
        out.push(Action::OpenPrivacySettings);
    }
}

fn volume(ui: &mut Ui, ctx: &Ctx, r: Rect) {
    widgets::card(ui, r);
    let mut c = r.inset_xy(22.0, 20.0);
    let head = c.take_top(16.0);
    widgets::caption(ui, head.x, head.y, "Volume", theme::TEXT_3);
    c.take_top(8.0);

    let scanned = ctx.inv.total_size();
    let marked = ctx.plan.total(ctx.inv);
    let Some(v) = ctx.volume.filter(|v| v.total > 0) else {
        widgets::size_text(ui, c.take_top(40.0), scanned, theme::DISPLAY, theme::TEXT, Align::Start);
        return;
    };
    let used = v.used();
    let line = c.take_top(40.0);
    let wn = widgets::size_text(ui, line, used, theme::DISPLAY, theme::TEXT, Align::Start);
    ui.text(line.x + wn + 10.0, line.cy() - 4.0, &format!("used of {}", ByteSize(v.total)), theme::BODY, theme::TEXT_3);
    let fr = Rect::new(line.right() - 220.0, line.y, 220.0, line.h);
    widgets::size_text(ui, fr, v.available, theme::NUM_BIG, theme::SAFE, Align::End);
    ui.label(Rect::new(fr.x, head.y, fr.w, head.h), "FREE", theme::CAPTION, alpha(theme::SAFE, 0.8), Align::End);
    c.take_top(12.0);

    // Capacity bar: stacked pills — other data, this scan, marked; track = free.
    let bar = c.take_top(14.0);
    let total = v.total as f32;
    let scan_w = bar.w * (scanned.min(used) as f32 / total);
    let used_w = bar.w * (used as f32 / total);
    ui.rect(bar, theme::LINE_SOFT, 7.0);
    ui.rect(Rect::new(bar.x, bar.y, used_w.max(14.0), bar.h), alpha(theme::NEUTRAL, 0.55), 7.0);
    ui.rect(Rect::new(bar.x, bar.y, scan_w.max(14.0), bar.h), theme::ACCENT, 7.0);
    if marked > 0 {
        let mw = (bar.w * marked as f32 / total).max(10.0);
        ui.stripes(Rect::new(bar.x, bar.y, mw, bar.h), mix(theme::ACCENT, theme::RECLAIM, 0.35), theme::RECLAIM, 7.0);
    }
    c.take_top(12.0);

    let legend = c.take_top(18.0);
    let items = [
        ("This scan", scanned.min(used), theme::ACCENT),
        ("Other data", used.saturating_sub(scanned), alpha(theme::NEUTRAL, 0.55)),
        ("Free", v.available, theme::LINE),
        ("Marked", marked, theme::RECLAIM),
    ];
    let mut x = legend.x;
    for (label, bytes, color) in items {
        widgets::dot(ui, x + 4.0, legend.cy(), 8.0, color);
        x += 14.0;
        x += ui.text(x, legend.cy() - 8.0, label, theme::SMALL, theme::TEXT_2) + 6.0;
        x += ui.text(x, legend.cy() - 8.0, &ByteSize(bytes).to_string(), theme::NUM, theme::TEXT_3) + 22.0;
    }
}

fn kpis(ui: &mut Ui, ctx: &Ctx, r: Rect) {
    let root = ctx.inv.node(NodeId::ROOT);
    let secs = ctx.stats.elapsed().as_secs_f64().max(1e-3);
    let entries = ctx.stats.entries.load(std::sync::atomic::Ordering::Relaxed);
    let marked = ctx.plan.total(ctx.inv);
    let tiles: [(&str, String, String, theme::Rgba); 4] = [
        ("Scanned", ByteSize(root.size()).to_string(), format!("in {:.1}s · {}/s", secs, widgets::count((entries as f64 / secs) as u64)), theme::TEXT),
        ("Files", widgets::count(root.files() as u64), format!("in {} folders", widgets::count(root.dirs() as u64)), theme::TEXT),
        (
            "Regenerable",
            ByteSize(ctx.insights.reclaimable_bytes).to_string(),
            format!("{} caches & build outputs", ctx.insights.reclaimable.len()),
            theme::hint(tessera_domain::ReclaimHint::BuildArtifacts),
        ),
        ("Marked", ByteSize(marked).to_string(), format!("{} items · nothing deleted yet", ctx.plan.len()), if marked > 0 { theme::RECLAIM } else { theme::TEXT_3 }),
    ];
    let gap = 14.0;
    let tw = (r.w - gap * 3.0) / 4.0;
    for (i, (cap, value, sub, color)) in tiles.iter().enumerate() {
        let t = Rect::new(r.x + i as f32 * (tw + gap), r.y, tw, r.h);
        widgets::card(ui, t);
        let mut c = t.inset_xy(18.0, 16.0);
        let h = c.take_top(14.0);
        widgets::caption(ui, h.x, h.y, cap, theme::TEXT_3);
        c.take_top(6.0);
        let v = c.take_top(30.0);
        ui.label(v, value, theme::NUM_BIG, *color, Align::Start);
        ui.label(c.take_top(16.0), sub, theme::SMALL, theme::TEXT_3, Align::Start);
    }
}

fn composition(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect) {
    widgets::card(ui, r);
    let mut c = r.inset_xy(22.0, 20.0);
    let head = c.take_top(16.0);
    widgets::caption(ui, head.x, head.y, "Composition by kind", theme::TEXT_3);
    ui.label(head, "area ∝ bytes", theme::SMALL, theme::TEXT_3, Align::End);
    c.take_top(12.0);
    let cats = &ctx.insights.categories;
    if cats.is_empty() {
        ui.label(c, "Waiting for data…", theme::BODY, theme::TEXT_3, Align::Center);
        return;
    }
    let d = c.h.min(c.w * 0.4).floor();
    let circle = Rect::new(c.x, c.y + (c.h - d) / 2.0, d, d);

    // Solve (cached): a circular Voronoi treemap of category totals.
    let key = (ctx.insights.generation, d as u32);
    if view.composition.key != key || view.composition.cells.len() != cats.len() {
        let container = Polygon::circle(Vec2::new(0.0, 0.0), d as f64 / 2.0, 72);
        let values: Vec<f64> = cats.iter().map(|s| s.bytes as f64).collect();
        let warm = (view.composition.sites.len() == values.len()).then_some(view.composition.sites.as_slice());
        let sol = tessera_voronoi::solve(&container, &values, warm, 11, &Settings { max_iterations: 150, convergence: 0.005 });
        view.composition.cells = sol.cells;
        view.composition.sites = sol.sites;
        view.composition.key = key;
    }

    let mut legend = c;
    legend.take_left(d + 28.0);
    let total: u64 = cats.iter().map(|s| s.bytes).sum();
    let row_h = 26.0;
    let mut hover_cat: Option<usize> = None;
    let mut ly = legend.y + (legend.h - row_h * cats.len() as f32) / 2.0;
    for (i, s) in cats.iter().enumerate() {
        let row = Rect::new(legend.x, ly, legend.w, row_h);
        ly += row_h;
        if ui.hovered(row) {
            hover_cat = Some(i);
            ui.rect(row.inset_xy(-8.0, 1.0), theme::HOVER, theme::RADIUS_SM);
        }
        let cat = s.category.unwrap_or(tessera_domain::Category::Other);
        let mut rr = row;
        widgets::dot(ui, rr.x + 4.0, rr.cy(), 8.0, theme::category(cat));
        rr.take_left(16.0);
        let size = rr.take_right(64.0);
        let pct = rr.take_right(42.0);
        ui.label(rr, cat.short_label(), theme::BODY, theme::TEXT, Align::Start);
        ui.label(pct, &widgets::percent(s.bytes as f64 / total.max(1) as f64), theme::NUM, theme::TEXT_3, Align::End);
        widgets::size_text(ui, size, s.bytes, theme::NUM, theme::TEXT_2, Align::End);
    }

    let center = Vec2::new(circle.cx() as f64, circle.cy() as f64);
    let mouse = ui.input.mouse;
    let mut pts: Vec<[f32; 2]> = Vec::new();
    for (i, cell) in view.composition.cells.iter().enumerate() {
        let r = (cell.inradius_at_centroid() * 0.3).min(5.0);
        let mut shape = cell.inset(1.5 + r);
        if shape.is_empty() {
            shape = cell.inset(0.8);
            if shape.is_empty() {
                continue;
            }
        }
        let cat = cats[i].category.unwrap_or(tessera_domain::Category::Other);
        let hovered_here = cell.contains(Vec2::new(mouse.0 as f64 - center.x, mouse.1 as f64 - center.y)) && ui.hovered(circle);
        if hovered_here {
            hover_cat = Some(i);
        }
        pts.clear();
        pts.extend(shape.pts.iter().map(|p| [(p.x + center.x) as f32, (p.y + center.y) as f32]));
        let flags = if hover_cat == Some(i) { cell_flags::HOVER } else { 0 };
        ui.cell(&pts, theme::category_fill(cat), theme::RECLAIM, r as f32, flags, 1.0, 1.0);
        let inr = shape.inradius_at_centroid();
        if inr > 22.0 {
            let cc = shape.centroid();
            let pct = widgets::percent(cats[i].bytes as f64 / total.max(1) as f64);
            ui.layer = 1;
            let wn = ui.measure(&pct, theme::SMALL_STRONG);
            ui.text((cc.x + center.x) as f32 - wn / 2.0, (cc.y + center.y) as f32 - 8.0, &pct, theme::SMALL_STRONG, alpha(theme::TEXT, 0.92));
            ui.layer = 0;
        }
    }
    let _ = hover_cat;
}

fn regenerable(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect, out: &mut Vec<Action>) {
    widgets::card(ui, r);
    let mut c = r.inset_xy(22.0, 20.0);
    let head = c.take_top(16.0);
    widgets::caption(ui, head.x, head.y, "Regenerable", theme::TEXT_3);
    let summary = format!("{} in {} places", ByteSize(ctx.insights.reclaimable_bytes), ctx.insights.reclaimable.len());
    ui.label(head, &summary, theme::SMALL, theme::TEXT_3, Align::End);
    c.take_top(10.0);
    if ctx.insights.reclaimable.is_empty() {
        let msg = if ctx.scanning { "Looking for caches and build outputs…" } else { "No caches or build outputs found" };
        ui.label(c.take_top(40.0), msg, theme::BODY, theme::TEXT_3, Align::Start);
        return;
    }
    let rows = ((c.h) / 42.0).floor() as usize;
    for &id in ctx.insights.reclaimable.iter().filter(|&&id| !ctx.inv.node(id).is_removed()).take(rows) {
        let row = c.take_top(42.0);
        item_row(ui, ctx, view, id, row, true, out);
    }
}

fn largest(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect, out: &mut Vec<Action>) {
    widgets::card(ui, r);
    let mut c = r.inset_xy(22.0, 20.0);
    let head = c.take_top(16.0);
    widgets::caption(ui, head.x, head.y, "Largest files", theme::TEXT_3);
    c.take_top(8.0);
    for &id in ctx.insights.largest_files.iter().filter(|&&id| !ctx.inv.node(id).is_removed()).take(12) {
        let row = c.take_top(34.0);
        item_row(ui, ctx, view, id, row, false, out);
    }
}

/// Shared list row: identity on the left, size and mark toggle on the right.
fn item_row(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, id: NodeId, row: Rect, two_line: bool, out: &mut Vec<Action>) {
    let n = ctx.inv.node(id);
    let marked = ctx.is_marked(id);
    let covered = !marked && ctx.is_covered(id);
    let hover = ui.hovered(row);
    let selected = view.selected == Some(id);
    if selected || hover {
        ui.rect(row.inset_xy(-10.0, 1.0), if selected { theme::HOVER } else { alpha(theme::HOVER, 0.6) }, theme::RADIUS_SM);
    }
    let mut c = row;
    let toggle = c.take_right(26.0);
    let size = c.take_right(84.0);
    c.take_right(12.0);
    let color = if n.is_dir() { theme::hint(n.hint()) } else { theme::category(n.category()) };
    if n.is_dir() {
        widgets::folder(ui, c.x, c.cy(), color);
    } else {
        widgets::dot(ui, c.x + 5.0, c.cy(), 7.0, color);
    }
    c.take_left(22.0);
    let ink = if marked { theme::RECLAIM } else { theme::TEXT };
    let parent = n.parent().map(|p| ctx.inv.relative_path(p)).unwrap_or_default();
    if two_line {
        let mut l1 = Rect::new(c.x, c.y + 3.0, c.w, 20.0);
        if n.hint().is_some() {
            let bw = ui.measure(n.hint().label(), theme::CAPTION) + 14.0;
            let b = l1.take_right(bw);
            widgets::badge(ui, b.x, b.cy(), n.hint().label(), color);
            l1.take_right(8.0);
        }
        ui.label(l1, ctx.inv.name(id), theme::BODY_STRONG, ink, Align::Start);
        ui.label(Rect::new(c.x, c.y + 22.0, c.w, 16.0), &parent, theme::MONO, theme::TEXT_3, Align::Start);
    } else {
        let name_w = (c.w * 0.42).max(120.0);
        let name = c.take_left(name_w);
        ui.label(name, ctx.inv.name(id), theme::BODY, ink, Align::Start);
        c.take_left(12.0);
        ui.label(c, &parent, theme::MONO, theme::TEXT_3, Align::Start);
    }
    widgets::size_text(ui, size, n.size(), theme::NUM, if marked { theme::RECLAIM } else { theme::TEXT_2 }, Align::End);
    if widgets::mark_toggle(ui, toggle, marked, covered, true) {
        out.push(Action::ToggleMark(id));
    } else if ui.double_clicked(row) {
        let target = if n.is_dir() { id } else { n.parent().unwrap_or(NodeId::ROOT) };
        out.push(Action::Select(id));
        out.push(Action::Focus(target));
        out.push(Action::SetTab(Tab::Map));
    } else if ui.clicked(row) {
        out.push(Action::Select(id));
    }
}
