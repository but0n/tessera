//! Inspector: what the selection is, and the reclaim queue below it.

use tessera_app::CommitState;
use tessera_domain::{EntryKind, NodeId};

use super::widgets::{self, ButtonKind};
use super::{Ctx, panel_header};
use crate::state::{Action, Tab, ViewState};
use crate::theme::{self, alpha};
use crate::ui::{Align, Rect, Ui};

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    ui.rect(area, theme::PANEL, 0.0);
    ui.rect(Rect::new(area.x, area.y, 1.0, area.h), theme::LINE_SOFT, 0.0);
    let mut a = area;

    // The queue grows from the bottom; the selection takes what is left.
    let queue_rows = ctx.plan.len().min(7) as f32;
    let queue_h = 150.0 + queue_rows * 30.0 + if ctx.plan.is_empty() { 34.0 } else { 0.0 };
    let queue = a.take_bottom(queue_h);
    selection(ui, ctx, view, a, out);
    reclaim_queue(ui, ctx, queue, out);
}

fn selection(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    let mut a = area;
    let head = panel_header(&mut a);
    widgets::caption(ui, head.x, head.cy() - 7.0, "Inspector", theme::TEXT_3);
    let mut c = a.inset_xy(theme::GUTTER, 4.0);

    let Some(id) = view.selected.filter(|&id| ctx.inv.get(id).is_some_and(|n| !n.is_removed())) else {
        let r = c.take_top(64.0);
        ui.rect_border(r, alpha(theme::RAISED, 0.4), theme::LINE_SOFT, 1.0, theme::RADIUS);
        ui.label(r.inset(14.0), "Select anything in the tree or map", theme::BODY, theme::TEXT_3, Align::Center);
        return;
    };
    let n = ctx.inv.node(id);
    let tint = if id == NodeId::ROOT { theme::TEXT_2 } else { theme::branch(view.family(ctx.inv, id)) };

    // Identity.
    let title = c.take_top(26.0);
    let sw = Rect::new(title.x, title.cy() - 6.0, 12.0, 12.0);
    if n.is_dir() {
        widgets::folder(ui, sw.x, sw.cy(), tint);
    } else {
        widgets::dot(ui, sw.cx(), sw.cy(), 10.0, tint);
    }
    ui.label(Rect::new(title.x + 22.0, title.y, title.w - 22.0, title.h), ctx.inv.name(id), theme::TITLE, theme::TEXT, Align::Start);
    let rel = ctx.inv.relative_path(id);
    ui.label(c.take_top(18.0), &rel, theme::MONO, theme::TEXT_3, Align::Start);
    c.take_top(14.0);

    // Magnitude.
    let big = c.take_top(40.0);
    widgets::size_text(ui, big, n.size(), theme::DISPLAY, if ctx.is_marked(id) { theme::RECLAIM } else { theme::TEXT }, Align::Start);
    c.take_top(10.0);
    let parent = n.parent().map(|p| ctx.inv.node(p).size()).unwrap_or(n.size()).max(1);
    let total = ctx.inv.total_size().max(1);
    for (label, frac) in [("of parent", n.size() as f64 / parent as f64), ("of scan", n.size() as f64 / total as f64)] {
        let mut r = c.take_top(22.0);
        let pct = r.take_right(56.0);
        ui.label(pct, &widgets::percent(frac), theme::NUM, theme::TEXT_2, Align::End);
        let lab = r.take_left(70.0);
        ui.label(lab, label, theme::SMALL, theme::TEXT_3, Align::Start);
        widgets::meter(ui, Rect::new(r.x, r.cy() - 2.0, r.w - 10.0, 4.0), frac as f32, tint);
    }
    c.take_top(10.0);

    // Facts.
    let kind = match n.kind() {
        EntryKind::Dir => "Folder".to_string(),
        EntryKind::File => n.category().label().to_string(),
        EntryKind::Symlink => "Symbolic link".to_string(),
        EntryKind::Other => "Special file".to_string(),
    };
    let mut facts: Vec<(&str, String)> = vec![("Kind", kind)];
    if n.is_dir() {
        facts.push(("Contains", format!("{} files · {} folders", widgets::count(n.files() as u64), widgets::count(n.dirs() as u64))));
    }
    if n.is_shared() {
        facts.push(("Hard link", "bytes counted at first link".into()));
    }
    for (k, v) in facts {
        let mut r = c.take_top(22.0);
        let kr = r.take_left(70.0);
        ui.label(kr, k, theme::SMALL, theme::TEXT_3, Align::Start);
        ui.label(r, &v, theme::SMALL, theme::TEXT_2, Align::Start);
    }

    // Advice.
    let note: Option<(&str, &str, theme::Rgba)> = if n.hint().is_some() {
        Some((n.hint().label(), n.hint().explain(), theme::hint(n.hint())))
    } else if n.is_protected() {
        Some(("Protected", "Not scanned. Grant Full Disk Access to include it.", theme::TEXT_2))
    } else if n.is_failed() {
        Some(("Unreadable", "Permission denied while scanning.", theme::TEXT_2))
    } else if n.is_boundary() {
        Some(("Other volume", "A mount point; scans stay on one volume.", theme::TEXT_2))
    } else {
        None
    };
    if let Some((title, body, color)) = note {
        c.take_top(10.0);
        let r = c.take_top(54.0);
        ui.rect(r, alpha(color, 0.08), theme::RADIUS_SM + 2.0);
        ui.rect(Rect::new(r.x, r.y + 8.0, 2.0, r.h - 16.0), color, 1.0);
        widgets::caption(ui, r.x + 14.0, r.y + 10.0, title, color);
        ui.label(Rect::new(r.x + 14.0, r.y + 26.0, r.w - 22.0, 18.0), body, theme::SMALL, theme::TEXT_2, Align::Start);
    }

    // Actions.
    c.take_top(16.0);
    if id != NodeId::ROOT {
        let marked = ctx.is_marked(id);
        let covered = ctx.is_covered(id);
        let r = c.take_top(34.0);
        let label = if marked {
            "Unmark"
        } else if covered {
            "Included via parent"
        } else {
            "Mark for removal"
        };
        if widgets::button(ui, r, label, if marked { ButtonKind::Secondary } else { ButtonKind::Mark }, !covered) {
            out.push(Action::ToggleMark(id));
        }
        c.take_top(8.0);
    }
    let mut r = c.take_top(32.0);
    let half = (r.w - 8.0) / 2.0;
    let left = r.take_left(half);
    r.take_left(8.0);
    if n.is_dir() && widgets::button(ui, left, "Open in Map", ButtonKind::Secondary, true) {
        out.push(Action::Focus(id));
        out.push(Action::SetTab(Tab::Map));
    }
    if widgets::button(ui, if n.is_dir() { r } else { Rect { w: r.right() - left.x, ..left } }, "Reveal in Finder", ButtonKind::Secondary, true) {
        out.push(Action::Reveal(id));
    }
}

fn reclaim_queue(ui: &mut Ui, ctx: &Ctx, area: Rect, out: &mut Vec<Action>) {
    ui.rect(Rect::new(area.x, area.y, area.w, 1.0), theme::LINE_SOFT, 0.0);
    let mut c = area.inset_xy(theme::GUTTER, 14.0);
    let head = c.take_top(18.0);
    let n = ctx.plan.len();
    widgets::caption(ui, head.x, head.y + 2.0, "Reclaim", theme::RECLAIM);
    ui.label(head, &format!("{} marked", n), theme::SMALL, theme::TEXT_3, Align::End);
    let total = ctx.plan.total(ctx.inv);
    let big = c.take_top(42.0);
    widgets::size_text(ui, big, total, theme::DISPLAY, if n > 0 { theme::RECLAIM } else { theme::TEXT_3 }, Align::Start);

    if n == 0 {
        ui.label(c.take_top(20.0), "Mark items to plan what to free.", theme::SMALL, theme::TEXT_3, Align::Start);
        ui.label(c.take_top(16.0), "Nothing is deleted until you confirm.", theme::SMALL, theme::TEXT_3, Align::Start);
    }
    c.take_top(4.0);
    let items = ctx.plan.items();
    let shown = items.len().min(7);
    for &id in items.iter().rev().take(shown) {
        let r = c.take_top(30.0);
        let hover = ui.hovered(r);
        if hover {
            ui.rect(r.inset_xy(-6.0, 1.0), theme::HOVER, theme::RADIUS_SM);
        }
        let mut row = r;
        let x = row.take_right(22.0);
        let size = row.take_right(70.0);
        ui.stripes(Rect::new(row.x, row.cy() - 5.0, 10.0, 10.0), alpha(theme::RECLAIM, 0.25), theme::RECLAIM, 3.0);
        row.take_left(18.0);
        ui.label(row, ctx.inv.name(id), theme::BODY, theme::TEXT, Align::Start);
        widgets::size_text(ui, size, ctx.inv.node(id).size(), theme::NUM, theme::TEXT_2, Align::End);
        if widgets::icon_button(ui, x.inset_xy(0.0, 4.0), "×", false) {
            out.push(Action::Unmark(id));
        } else if ui.clicked(r) {
            out.push(Action::Select(id));
        }
    }
    if items.len() > shown {
        ui.label(c.take_top(18.0), &format!("+ {} more", items.len() - shown), theme::SMALL, theme::TEXT_3, Align::Start);
    }
    let btn = Rect::new(area.x + theme::GUTTER, area.bottom() - 14.0 - 36.0, area.w - 2.0 * theme::GUTTER, 36.0);
    let running = matches!(ctx.commit, CommitState::Running { .. });
    let label = if running { "Moving to Trash…".to_string() } else { "Move to Trash…".to_string() };
    if widgets::button(ui, btn, &label, ButtonKind::Reclaim, n > 0 && !running) {
        out.push(Action::AskCommit);
    }
}
