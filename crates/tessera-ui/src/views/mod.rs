//! Screen composition. The layout itself is the legend (ADR-0008):
//!
//! ```text
//! ┌──────────── top bar: where am I · what am I looking at · scan state ───┐
//! │ Explorer (tree) │  Overview  /  Map (the centre stage)  │ Inspector    │
//! │ what exists     │  how big things are                   │ what it is,  │
//! │                 │                                       │ what to free │
//! └──────────── footer: what you can do right now ─────────────────────────┘
//! ```

mod chrome;
mod inspector;
mod map;
mod overview;
mod sidebar;
pub mod widgets;

use std::sync::Arc;

use parking_lot::RwLock;
use tessera_app::CommitState;
use tessera_app::cartography::{Cartographer, MapLayout};
use tessera_app::ports::VolumeStats;
use tessera_app::scan::ScanStats;
use tessera_domain::{Insights, Inventory, NodeId, ReclaimPlan};
use winit::keyboard::NamedKey;

use crate::state::{Action, Tab, ViewState};
use crate::theme;
use crate::ui::{Rect, Ui};

/// Read-only view of everything a frame may show.
pub struct Ctx<'a> {
    pub inv: &'a Inventory,
    pub inv_arc: &'a Arc<RwLock<Inventory>>,
    pub plan: &'a ReclaimPlan,
    pub insights: &'a Insights,
    pub stats: &'a ScanStats,
    pub volume: Option<VolumeStats>,
    pub scanning: bool,
    pub skips_protected: bool,
    pub reader: &'static str,
    pub carto: &'a Cartographer,
    pub layout: Option<Arc<MapLayout>>,
    pub commit: &'a CommitState,
}

impl Ctx<'_> {
    pub fn is_marked(&self, id: NodeId) -> bool {
        self.plan.is_marked(id)
    }
    /// Marked through an ancestor (not itself).
    pub fn is_covered(&self, id: NodeId) -> bool {
        !self.plan.is_empty() && self.plan.covering(self.inv, id).is_some_and(|c| c != id)
    }
}

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, out: &mut Vec<Action>) {
    let screen = ui.screen();
    ui.rect(screen, theme::BG, 0.0);
    let mut body = screen;
    let top = body.take_top(theme::TOPBAR_H);
    let foot = body.take_bottom(theme::FOOTER_H);
    let side = body.take_left(theme::SIDEBAR_W);
    let insp = body.take_right(theme::INSPECTOR_W);

    if view.confirm {
        ui.input_floor = 2;
    }
    keyboard(ui, ctx, view, out);
    map::request_layout(ui, ctx, view, map::map_rect(body));

    chrome::top_bar(ui, ctx, view, top, out);
    sidebar::draw(ui, ctx, view, side, out);
    match view.tab {
        Tab::Overview => overview::draw(ui, ctx, view, body, out),
        Tab::Map => map::draw(ui, ctx, view, body, out),
    }
    inspector::draw(ui, ctx, view, insp, out);
    chrome::footer(ui, ctx, view, foot);
    chrome::toast(ui, view, body);
    if view.confirm {
        chrome::confirm(ui, ctx, screen, out);
    }
}

fn keyboard(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, out: &mut Vec<Action>) {
    let i = ui.input;
    if view.confirm {
        if i.key(NamedKey::Escape) {
            out.push(Action::CancelCommit);
        }
        if i.key(NamedKey::Enter) {
            out.push(Action::Commit);
        }
        return;
    }
    let cmd = i.mods.super_key();
    if cmd && i.char("1") {
        out.push(Action::SetTab(Tab::Overview));
    }
    if cmd && i.char("2") {
        out.push(Action::SetTab(Tab::Map));
    }
    if cmd && i.char("r") {
        out.push(Action::Rescan);
    }
    if cmd && i.char("o") {
        out.push(Action::OpenFolder);
    }
    if cmd && i.key(NamedKey::Backspace) && !ctx.plan.is_empty() {
        out.push(Action::AskCommit);
    }
    if cmd {
        return;
    }
    if ((i.key(NamedKey::Escape) || i.key(NamedKey::Backspace)) && view.tab == Tab::Map) || i.back_pressed {
        out.push(Action::FocusUp);
    }
    let Some(sel) = view.selected else {
        if i.key(NamedKey::ArrowDown) {
            out.push(Action::Select(NodeId::ROOT));
        }
        return;
    };
    if i.key(NamedKey::Space) || i.char("m") {
        out.push(Action::ToggleMark(sel));
    }
    if i.key(NamedKey::Enter) && ctx.inv.node(sel).is_dir() {
        out.push(Action::Focus(sel));
        out.push(Action::SetTab(Tab::Map));
    }
    if i.key(NamedKey::ArrowRight) && ctx.inv.node(sel).is_dir() && !view.expanded.contains(&sel) {
        out.push(Action::ToggleExpand(sel));
    }
    if i.key(NamedKey::ArrowLeft) {
        if view.expanded.contains(&sel) && sel != NodeId::ROOT {
            out.push(Action::ToggleExpand(sel));
        } else if let Some(p) = ctx.inv.node(sel).parent() {
            out.push(Action::Select(p));
        }
    }
    let page = 12;
    let step: Option<isize> = if i.key(NamedKey::ArrowDown) {
        Some(1)
    } else if i.key(NamedKey::ArrowUp) {
        Some(-1)
    } else if i.key(NamedKey::PageDown) {
        Some(page)
    } else if i.key(NamedKey::PageUp) {
        Some(-page)
    } else if i.key(NamedKey::End) {
        Some(isize::MAX / 2)
    } else if i.key(NamedKey::Home) {
        Some(-isize::MAX / 2)
    } else {
        None
    };
    if let Some(step) = step {
        let rows = sidebar::flatten(ctx, view);
        if let Some(pos) = rows.iter().position(|r| r.0 == sel) {
            let next = (pos as isize).saturating_add(step).clamp(0, rows.len() as isize - 1) as usize;
            out.push(Action::Select(rows[next].0));
        }
    }
}

/// Splits `r` into a header strip and the rest, with the house padding.
pub fn panel_header(r: &mut Rect) -> Rect {
    r.take_top(44.0).inset_xy(theme::GUTTER, 0.0)
}
