//! Top bar, footer, toasts and the commit confirmation.

use tessera_app::CommitState;
use tessera_domain::ByteSize;

use super::Ctx;
use super::widgets::{self, ButtonKind};
use crate::gfx::icons::Icon;
use crate::platform;
use crate::state::{Action, Tab, ViewState};
use crate::theme::{self, alpha};
use crate::ui::{Align, Rect, Ui, ease_out};

pub fn top_bar(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, r: Rect, out: &mut Vec<Action>) {
    ui.rect(r, theme::PANEL, 0.0);
    ui.rect(Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0), theme::LINE_SOFT, 0.0);
    let mut row = r;
    row.take_left(theme::TOPBAR_LEAD);

    // Brand: three overlapping cells, the app's own visual language.
    let brand = row.take_left(theme::SIDEBAR_W - theme::TOPBAR_LEAD);
    let (bx, by) = (brand.x + 2.0, brand.cy());
    ui.rect(Rect::new(bx, by - 8.0, 10.0, 10.0), theme::ACCENT, 3.5);
    ui.rect(Rect::new(bx + 7.0, by - 4.0, 10.0, 10.0), alpha(theme::BRANCH[2], 0.9), 3.5);
    ui.rect(Rect::new(bx + 2.0, by + 1.0, 7.0, 7.0), alpha(theme::RECLAIM, 0.95), 2.5);
    ui.text(bx + 26.0, by - theme::TITLE.line_height() / 2.0, "Tessera", theme::TITLE, theme::TEXT);

    // Right cluster: icon buttons, then the scan state beside them.
    let mut right = row;
    right.take_right(10.0);
    let open = right.take_right(32.0);
    let open = Rect::new(open.x, open.cy() - 16.0, 32.0, 32.0);
    right.take_right(4.0);
    let rescan = right.take_right(32.0);
    let rescan = Rect::new(rescan.x, rescan.cy() - 16.0, 32.0, 32.0);
    if widgets::icon_button(ui, open, Icon::FolderOpen, platform::OPEN_FOLDER_TIP, true) {
        out.push(Action::OpenFolder);
    }
    if widgets::icon_button(ui, rescan, Icon::Refresh, platform::RESCAN_TIP, !ctx.scanning) {
        out.push(Action::Rescan);
    }
    right.take_right(14.0);
    let status = right.take_right(260.0);
    scan_status(ui, ctx, status);

    // Centre: the view switch.
    let seg = row.take_left(196.0).inset_xy(0.0, 11.0);
    let sel = if view.tab == Tab::Overview { 0 } else { 1 };
    if let Some(i) = widgets::segmented(ui, seg, &[(Icon::Overview, "Overview"), (Icon::Map, "Map")], sel) {
        out.push(Action::SetTab(if i == 0 { Tab::Overview } else { Tab::Map }));
    }

    // Empty bar space drags the window where the bar is the titlebar.
    let hot = [open, rescan, seg];
    if platform::INTEGRATED_TITLEBAR && ui.input.pressed && ui.hovered(r) && !hot.iter().any(|h| h.contains(ui.input.mouse)) {
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

/// One quiet line: a state glyph, then what was scanned and how fast.
fn scan_status(ui: &mut Ui, ctx: &Ctx, r: Rect) {
    let entries = ctx.stats.entries.load(std::sync::atomic::Ordering::Relaxed);
    let secs = ctx.stats.elapsed().as_secs_f32();
    let line = if ctx.scanning { format!("Scanning · {} items", widgets::count(entries)) } else { format!("{} items · {:.1}s", widgets::count(entries), secs) };
    let w = ui.measure(&line, theme::NUM);
    let x = r.right() - w;
    if ctx.scanning {
        widgets::spinner(ui, x - 28.0, r.cy(), theme::ACCENT);
    } else {
        ui.icon(x - 21.0, r.cy() - 7.5, 15.0, Icon::Check, theme::SAFE);
    }
    ui.text(x, r.cy() - theme::NUM.line_height() / 2.0, &line, theme::NUM, theme::TEXT_3);
}

/// What you can do right now, as keycaps.
pub fn footer(ui: &mut Ui, ctx: &Ctx, view: &ViewState, r: Rect) {
    ui.rect(r, theme::PANEL, 0.0);
    ui.rect(Rect::new(r.x, r.y, r.w, 1.0), theme::LINE_SOFT, 0.0);
    let hints: &[(&str, &str)] = match (view.tab, view.selected.is_some()) {
        (Tab::Overview, _) if view.list.is_some() => &[("Esc", "back to overview"), ("Space", "mark"), ("Enter", "open in map")],
        (Tab::Map, _) => &[("Click", "open"), ("Scroll", "in / out"), ("Esc", "up"), (platform::KEY_SELECT_CLICK, "select"), ("Right-click", "mark")],
        (Tab::Overview, true) => &[("Space", "mark"), ("Enter", "open in map"), ("↑↓←→", "navigate")],
        (Tab::Overview, false) => &[("↓", "navigate"), (platform::KEY_MAP_TAB, "map")],
    };
    let mut x = r.x + theme::GUTTER;
    let cy = r.cy();
    for (k, v) in hints {
        let kw = ui.measure(k, theme::CAPTION) + 10.0;
        let kr = Rect::new(x, cy - 8.5, kw, 17.0);
        ui.rect_border(kr, theme::RAISED, theme::LINE, 1.0, 4.0);
        ui.label(kr, k, theme::CAPTION, theme::TEXT_2, Align::Center);
        x += kw + 6.0;
        x += ui.text(x, cy - 8.0, v, theme::SMALL, theme::TEXT_3) + 16.0;
    }
    // Right: provenance of the numbers on screen.
    let mut info = format!("{} · on-disk sizes", ctx.reader);
    if let Some(l) = &ctx.layout
        && view.tab == Tab::Map
    {
        info = format!("{} cells in {:.0} ms · {}", l.cells.len(), l.elapsed.as_secs_f32() * 1000.0, info);
    }
    ui.label(Rect::new(r.right() - 420.0 - theme::GUTTER, r.y, 420.0, r.h), &info, theme::SMALL, theme::TEXT_3, Align::End);
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
    let icon = if t.ok { Icon::Check } else { Icon::Alert };
    ui.icon(r.x + 12.0, r.cy() - 7.5, 15.0, icon, alpha(if t.ok { theme::SAFE } else { theme::RECLAIM }, a));
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
    if widgets::button_icon(ui, go, Some(Icon::Trash), if running { "Moving…" } else { "Move to Trash" }, ButtonKind::Reclaim, !running) {
        out.push(Action::Commit);
    }
    ui.layer = 0;
}
