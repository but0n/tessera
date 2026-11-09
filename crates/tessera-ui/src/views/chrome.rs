//! Top bar, footer, toasts and the commit confirmation.

use tessera_app::CommitState;
use tessera_domain::{ByteSize, NodeId};

use super::Ctx;
use super::widgets::{self, ButtonKind};
use crate::state::{Action, Tab, ViewState};
use crate::theme::{self, alpha};
use crate::ui::{Align, Rect, Ui, ease_out};

pub fn top_bar(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect, out: &mut Vec<Action>) {
    ui.rect(r, theme::PANEL, 0.0);
    ui.rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), theme::LINE_SOFT, 0.0);
    let mut row = r;
    row.take_left(theme::TRAFFIC_LIGHTS_W);

    // Brand: three overlapping cells, the app's own visual language.
    let brand = row.take_left(theme::SIDEBAR_W - theme::TRAFFIC_LIGHTS_W);
    let (bx, by) = (brand.x + 2.0, brand.cy());
    ui.rect(Rect::new(bx, by - 8.0, 10.0, 10.0), theme::ACCENT, 3.5);
    ui.rect(Rect::new(bx + 7.0, by - 4.0, 10.0, 10.0), alpha(theme::BRANCH[2], 0.9), 3.5);
    ui.rect(Rect::new(bx + 2.0, by + 1.0, 7.0, 7.0), alpha(theme::RECLAIM, 0.95), 2.5);
    ui.text(bx + 26.0, by - theme::TITLE.line_height() / 2.0, "Tessera", theme::TITLE, theme::TEXT);

    // Right cluster.
    let mut right = row;
    right.take_right(4.0);
    let open = right.take_right(76.0).inset_xy(0.0, 11.0);
    right.take_right(8.0);
    let rescan = right.take_right(78.0).inset_xy(0.0, 11.0);
    if widgets::button(ui, open, "Open…", ButtonKind::Secondary, true) {
        out.push(Action::OpenFolder);
    }
    if widgets::button(ui, rescan, "Rescan", ButtonKind::Secondary, true) {
        out.push(Action::Rescan);
    }
    right.take_right(12.0);
    let status = right.take_right(250.0);
    scan_status(ui, ctx, status);

    // Centre: view switch, then the breadcrumb of what the map shows.
    let seg = row.take_left(176.0).inset_xy(0.0, 11.0);
    let sel = if view.tab == Tab::Overview { 0 } else { 1 };
    if let Some(i) = widgets::segmented(ui, seg, &["Overview", "Map"], sel) {
        out.push(Action::SetTab(if i == 0 { Tab::Overview } else { Tab::Map }));
    }
    row.take_left(18.0);
    let crumbs = row.inset_xy(0.0, 12.0);
    let used = breadcrumb(ui, ctx, view, crumbs, out);

    // Empty bar space drags the window (the titlebar is transparent).
    let hot = [open, rescan, seg, Rect { w: used, ..crumbs }];
    if ui.input.pressed && ui.hovered(r) && !hot.iter().any(|h| h.contains(ui.input.mouse)) {
        out.push(Action::DragWindow);
    }

    // Indeterminate progress shimmer along the bottom edge while scanning.
    if ctx.scanning {
        let t = ui.time();
        let w = r.w * 0.28;
        let x = r.x + ((t * 0.55).fract() * 1.4 - 0.2) * r.w;
        let line = Rect::new(x, r.bottom() - 2.0, w, 2.0);
        ui.set_clip(r);
        ui.gradient(line, alpha(theme::ACCENT, 0.0), theme::ACCENT, 1.0, false);
        ui.gradient(Rect::new(x + w, line.y, w * 0.4, 2.0), theme::ACCENT, alpha(theme::ACCENT, 0.0), 1.0, false);
        ui.set_clip(ui.screen());
        ui.animating = true;
    }
}

fn breadcrumb(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect, out: &mut Vec<Action>) -> f32 {
    let mut chain: Vec<NodeId> = ctx.inv.ancestors(view.focus).collect();
    chain.reverse();
    // Keep the tail visible: drop leading segments that would not fit.
    let sep = "/";
    let sep_w = ui.measure(sep, theme::BODY) + 12.0;
    let widths: Vec<f32> = chain.iter().map(|&id| ui.measure(ctx.inv.name(id), theme::BODY_STRONG).min(180.0) + 14.0).collect();
    let mut start = 0;
    while start + 1 < chain.len() && widths[start..].iter().sum::<f32>() + sep_w * (chain.len() - start) as f32 + 20.0 > r.w {
        start += 1;
    }
    let mut x = r.x;
    if start > 0 {
        x += ui.text(x, r.cy() - 9.0, "…", theme::BODY, theme::TEXT_3) + 6.0;
    }
    for (i, &id) in chain.iter().enumerate().skip(start) {
        if i > start || start > 0 {
            ui.text(x, r.cy() - 9.0, sep, theme::BODY, theme::TEXT_3);
            x += sep_w - 6.0;
        }
        let last = i + 1 == chain.len();
        let seg = Rect::new(x - 6.0, r.y, widths[i], r.h);
        let (hover, click) = ui.interact(seg);
        if hover && !last {
            ui.rect(seg, theme::HOVER, theme::RADIUS_SM);
        }
        let style = if last { theme::BODY_STRONG } else { theme::BODY };
        ui.label(seg.inset_xy(6.0, 0.0), ctx.inv.name(id), style, if last { theme::TEXT } else { theme::TEXT_2 }, Align::Start);
        if click {
            out.push(Action::Focus(id));
            out.push(Action::SetTab(Tab::Map));
        }
        x += widths[i];
    }
    x - r.x
}

fn scan_status(ui: &mut Ui, ctx: &Ctx, r: Rect) {
    let entries = ctx.stats.entries.load(std::sync::atomic::Ordering::Relaxed);
    let secs = ctx.stats.elapsed().as_secs_f32();
    let (head, color) = if ctx.scanning { ("Scanning", theme::ACCENT) } else { ("Scanned", theme::SAFE) };
    let line1 = format!("{} items · {:.1}s", widgets::count(entries), secs);
    let x = r.right() - ui.measure(&line1, theme::NUM).max(ui.measure(head, theme::CAPTION) + 16.0);
    if ctx.scanning {
        widgets::spinner(ui, x - 26.0, r.cy(), theme::ACCENT);
    } else {
        widgets::dot(ui, x - 12.0, r.cy() - 7.0, 6.0, color);
    }
    widgets::caption(ui, x, r.cy() - 15.0, head, color);
    ui.text(x, r.cy() - 1.0, &line1, theme::NUM, theme::TEXT_2);
}

pub fn footer(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect) {
    ui.rect(r, theme::PANEL, 0.0);
    ui.rect(Rect::new(r.x, r.y, r.w, 1.0), theme::LINE_SOFT, 0.0);
    let hints: &[(&str, &str)] = match (view.tab, view.selected.is_some()) {
        (Tab::Map, _) => &[("Click", "open folder"), ("⌥ Click", "select"), ("Right-click", "mark"), ("Esc", "up one level"), ("⌘1", "overview")],
        (Tab::Overview, true) => &[("Space", "mark selection"), ("Enter", "open in map"), ("↑↓←→", "navigate tree"), ("⌘2", "map")],
        (Tab::Overview, false) => &[("↓", "start navigating"), ("⌘2", "map"), ("⌘R", "rescan"), ("⌘O", "open folder")],
    };
    let mut x = r.x + theme::GUTTER;
    let cy = r.cy();
    for (k, v) in hints {
        let kw = ui.measure(k, theme::CAPTION) + 10.0;
        let kr = Rect::new(x, cy - 8.0, kw, 16.0);
        ui.rect_border(kr, theme::RAISED, theme::LINE, 1.0, 4.0);
        ui.label(kr, k, theme::CAPTION, theme::TEXT_2, Align::Center);
        x += kw + 6.0;
        x += ui.text(x, cy - 8.0, v, theme::SMALL, theme::TEXT_3) + 16.0;
    }
    // Right: provenance of the numbers on screen.
    let mut info = format!("{} · on-disk sizes, hard links counted once", ctx.reader);
    if let Some(l) = &ctx.layout
        && view.tab == Tab::Map
    {
        info = format!("{} cells · solved in {:.0} ms · {}", l.cells.len(), l.elapsed.as_secs_f32() * 1000.0, info);
    }
    ui.label(Rect::new(r.right() - 520.0 - theme::GUTTER, r.y, 520.0, r.h), &info, theme::SMALL, theme::TEXT_3, Align::End);
}

pub fn toast(ui: &mut Ui, view: &mut ViewState, area: Rect) {
    let Some(t) = &view.toast else { return };
    let age = t.at.elapsed().as_secs_f32();
    if age > 4.5 {
        view.toast = None;
        return;
    }
    let a = ease_out(age / 0.25) * (1.0 - ease_out((age - 4.0) / 0.5));
    ui.layer = 2;
    let w = ui.measure(&t.text, theme::BODY_STRONG) + 44.0;
    let r = Rect::new(area.cx() - w / 2.0, area.bottom() - 64.0 + (1.0 - a) * 12.0, w, 38.0);
    ui.shadow(r, alpha(theme::BG, 0.6 * a), 19.0, 14.0);
    ui.rect_border(r, alpha(theme::RAISED, a), alpha(theme::LINE, a), 1.0, 19.0);
    widgets::dot(ui, r.x + 18.0, r.cy(), 7.0, alpha(if t.ok { theme::SAFE } else { theme::RECLAIM }, a));
    ui.label(Rect::new(r.x + 30.0, r.y, r.w - 30.0, r.h), &t.text.clone(), theme::BODY_STRONG, alpha(theme::TEXT, a), Align::Start);
    ui.layer = 0;
    ui.animating = true;
}

/// Modal confirmation for moving marked items to the Trash.
pub fn confirm(ui: &mut Ui, ctx: &Ctx, screen: Rect, out: &mut Vec<Action>) {
    ui.layer = 2;
    ui.rect(screen, alpha(theme::BG, 0.66), 0.0);
    let n = ctx.plan.len();
    let total = ctx.plan.total(ctx.inv);
    let shown = n.min(6);
    let h = 196.0 + shown as f32 * 26.0;
    let r = Rect::new(screen.cx() - 230.0, screen.cy() - h / 2.0, 460.0, h);
    ui.shadow(r, alpha(theme::BG, 0.8), theme::RADIUS + 4.0, 30.0);
    ui.rect_border(r, theme::RAISED, theme::LINE, 1.0, theme::RADIUS + 4.0);
    let mut c = r.inset(22.0);
    let title = format!("Move {} item{} to the Trash?", n, if n == 1 { "" } else { "s" });
    ui.label(c.take_top(24.0), &title, theme::TITLE, theme::TEXT, Align::Start);
    c.take_top(6.0);
    let body = format!("Frees about {}. Items stay in the Trash until you empty it.", ByteSize(total));
    ui.label(c.take_top(20.0), &body, theme::BODY, theme::TEXT_2, Align::Start);
    c.take_top(12.0);
    let list = c.take_top(shown as f32 * 26.0 + 8.0);
    ui.rect(list, theme::PANEL, theme::RADIUS_SM);
    let mut l = list.inset_xy(12.0, 4.0);
    for &id in ctx.plan.items().iter().take(shown) {
        let row = l.take_top(26.0);
        let mut row2 = row;
        let size = row2.take_right(80.0);
        widgets::size_text(ui, size, ctx.inv.node(id).size(), theme::NUM, theme::RECLAIM, Align::End);
        ui.label(row2, &ctx.inv.relative_path(id), theme::MONO, theme::TEXT_2, Align::Start);
    }
    if n > shown {
        ui.label(c.take_top(22.0), &format!("and {} more", n - shown), theme::SMALL, theme::TEXT_3, Align::Start);
    }
    let mut buttons = Rect::new(r.x + 22.0, r.bottom() - 22.0 - 34.0, r.w - 44.0, 34.0);
    let go = buttons.take_right(150.0);
    buttons.take_right(10.0);
    let cancel = buttons.take_right(96.0);
    let running = matches!(ctx.commit, CommitState::Running { .. });
    if widgets::button(ui, cancel, "Cancel", ButtonKind::Secondary, !running) {
        out.push(Action::CancelCommit);
    }
    if widgets::button(ui, go, if running { "Moving…" } else { "Move to Trash" }, ButtonKind::Reclaim, !running) {
        out.push(Action::Commit);
    }
    ui.layer = 0;
}
