//! Explorer: the file tree, largest first. Virtualised: only visible rows
//! are built, so expanding a directory with 100k entries stays smooth.
//!
//! Context stays on screen while scrolling: the ancestors of the first
//! visible rows stick to the top (pushed up as their subtree ends), indent
//! guides show nesting, and the map's current scope is a tinted band.

use std::time::Instant;

use tessera_domain::NodeId;

use super::widgets;
use super::{Ctx, panel_header};
use crate::gfx::icons::Icon;
use crate::state::{Action, Tab, ViewState};
use crate::theme::{self, alpha};
use crate::ui::{Align, Rect, Ui, ease_out};

const INDENT: f32 = 14.0;
/// Most ancestors pinned at once; deeper ones scroll normally.
const MAX_STICKY: usize = 6;
const GLIDE: f32 = 0.24;

/// Visible rows in display order: (node, depth).
pub fn flatten(ctx: &Ctx, view: &mut ViewState) -> Vec<(NodeId, u16)> {
    let mut rows = Vec::with_capacity(256);
    let mut stack: Vec<(NodeId, u16)> = vec![(NodeId::ROOT, 0)];
    while let Some((id, depth)) = stack.pop() {
        let n = ctx.inv.node(id);
        if n.is_removed() {
            continue;
        }
        rows.push((id, depth));
        if n.is_dir() && view.expanded.contains(&id) {
            let kids = view.sorted_children(ctx.inv, id);
            stack.extend(kids.iter().rev().map(|&c| (c, depth + 1)));
        }
    }
    rows
}

/// Index one past the last row of `j`'s subtree.
fn subtree_end(rows: &[(NodeId, u16)], j: usize) -> usize {
    let d = rows[j].1;
    rows[j + 1..].iter().position(|r| r.1 <= d).map_or(rows.len(), |p| j + 1 + p)
}

/// Pinned rows for scroll offset `scroll`: (row index, y relative to the
/// list top). Slot `k` holds the depth-`k` ancestor of the row just below
/// the pinned stack; it slides up when its subtree scrolls out.
pub fn sticky(rows: &[(NodeId, u16)], scroll: f32, max: usize) -> Vec<(usize, f32)> {
    let h = theme::ROW_H;
    let mut out: Vec<(usize, f32)> = Vec::new();
    while out.len() < max {
        let k = out.len();
        let line = scroll + k as f32 * h;
        let i = (line / h) as usize;
        if i >= rows.len() || (rows[i].1 as usize) < k {
            break;
        }
        let mut j = i;
        while rows[j].1 as usize > k {
            j -= 1;
        }
        let has_kids = rows.get(j + 1).is_some_and(|r| r.1 as usize > k);
        if j as f32 * h >= line || !has_kids {
            break;
        }
        let bottom = subtree_end(rows, j) as f32 * h - scroll;
        let y = (k as f32 * h).min(bottom - h);
        out.push((j, y));
        if y < k as f32 * h {
            break; // being pushed out: nothing deeper can stick below it
        }
    }
    out
}

pub fn draw(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, area: Rect, out: &mut Vec<Action>) {
    ui.rect(area, theme::PANEL, 0.0);
    ui.rect(Rect::new(area.right() - 1.0, area.y, 1.0, area.h), theme::LINE_SOFT, 0.0);
    let mut a = area;
    // Where the scan lives; the tree below says the rest.
    let head = panel_header(&mut a);
    ui.icon(head.x, head.cy() - 7.5, 15.0, Icon::Drive, theme::TEXT_3);
    let root = display_path(&ctx.inv.root_path().to_string_lossy());
    ui.label(Rect::new(head.x + 23.0, head.y, head.w - 23.0, head.h), &root, theme::MONO, theme::TEXT_2, Align::Start);

    let list = a.inset_xy(6.0, 0.0);
    let rows = flatten(ctx, view);
    let h = theme::ROW_H;
    let content_h = rows.len() as f32 * h + 8.0;
    let max_scroll = (content_h - list.h).max(0.0);

    // Scrolling: wheel is immediate; jumps (reveal, sticky click) glide.
    if ui.hovered(list) && ui.input.wheel.1 != 0.0 {
        view.tree_scroll -= ui.input.wheel.1;
        view.tree_glide = None;
    }
    if view.reveal_selected
        && let Some(sel) = view.selected
        && let Some(pos) = rows.iter().position(|r| r.0 == sel)
    {
        let y = pos as f32 * h;
        let pinned = (rows[pos].1 as usize).min(MAX_STICKY) as f32 * h;
        let target = if y - pinned < view.tree_scroll {
            Some(y - pinned - h)
        } else if y + h > view.tree_scroll + list.h {
            Some(y + h * 3.0 - list.h)
        } else {
            None
        };
        if let Some(t) = target {
            glide_to(view, t.clamp(0.0, max_scroll));
        }
        view.reveal_selected = false;
    }
    if let Some((from, to, at)) = view.tree_glide {
        let t = at.elapsed().as_secs_f32() / GLIDE;
        view.tree_scroll = from + (to - from) * ease_out(t);
        if t >= 1.0 {
            view.tree_glide = None;
        } else {
            ui.animating = true;
        }
    }
    view.tree_scroll = view.tree_scroll.clamp(0.0, max_scroll);

    let scrollable = content_h > list.h + 1.0;
    let rows_area = Rect { w: list.w - if scrollable { 10.0 } else { 0.0 }, ..list };
    let pinned = sticky(&rows, view.tree_scroll, MAX_STICKY);
    let pinned_h = pinned.iter().map(|p| p.1 + h).fold(0.0, f32::max);
    let ctx_rows = RowCtx::new(ctx, view);

    // Normal rows, clipped below the pinned stack so it also takes input.
    let prev = ui.set_clip(Rect::new(list.x, list.y + pinned_h, list.w, (list.h - pinned_h).max(0.0)));
    let first = (view.tree_scroll / h) as usize;
    let visible = (list.h / h) as usize + 2;
    // The map scope as a band behind its subtree.
    if view.tab == Tab::Map
        && view.focus != NodeId::ROOT
        && let Some(fpos) = rows.iter().position(|r| r.0 == view.focus)
    {
        let end = subtree_end(&rows, fpos);
        let y0 = list.y + fpos as f32 * h - view.tree_scroll;
        let band = Rect::new(rows_area.x, y0, rows_area.w, (end - fpos) as f32 * h);
        ui.rect(band, alpha(theme::ACCENT, 0.045), theme::RADIUS_SM);
    }
    for (i, &(id, depth)) in rows.iter().enumerate().skip(first).take(visible) {
        let r = Rect::new(rows_area.x, list.y + i as f32 * h - view.tree_scroll, rows_area.w, h);
        guides(ui, ctx, view, &ctx_rows, id, depth, r);
        row(ui, ctx, view, &ctx_rows, id, depth, r, false, out);
    }

    // Pinned ancestors on top, deepest drawn first so shallower ones cover
    // it while it is being pushed out.
    ui.set_clip(list);
    for &(j, y) in pinned.iter().rev() {
        let (id, depth) = rows[j];
        let r = Rect::new(rows_area.x, list.y + y, rows_area.w, h);
        ui.rect(Rect::new(list.x - 6.0, r.y, list.w + 12.0, h), theme::PANEL, 0.0);
        guides(ui, ctx, view, &ctx_rows, id, depth, r);
        if row(ui, ctx, view, &ctx_rows, id, depth, r, true, out) {
            glide_to(view, (j as f32 * h - depth as f32 * h).clamp(0.0, max_scroll));
        }
    }
    if pinned_h > 0.0 {
        let y = list.y + pinned_h;
        ui.gradient(Rect::new(list.x - 6.0, y, list.w + 12.0, 8.0), alpha(theme::BG, 0.55), alpha(theme::BG, 0.0), 0.0, true);
        ui.rect(Rect::new(list.x - 6.0, y - 1.0, list.w + 12.0, 1.0), theme::LINE_SOFT, 0.0);
    }
    ui.set_clip(prev);
    if let Some(o) = widgets::scrollbar(ui, list, content_h, view.tree_scroll, &mut view.tree_drag) {
        view.tree_scroll = o;
        view.tree_glide = None;
    }
}

fn glide_to(view: &mut ViewState, to: f32) {
    if (to - view.tree_scroll).abs() > 0.5 {
        view.tree_glide = Some((view.tree_scroll, to, Instant::now()));
    }
}

/// `~` for the home directory.
fn display_path(p: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && p.starts_with(&home) => format!("~{}", &p[home.len()..]),
        _ => p.to_string(),
    }
}

/// Per-frame facts every row needs.
struct RowCtx {
    /// Entries the map pointer is over: (entry, scope).
    hover: Option<(NodeId, NodeId)>,
    /// Ancestors of the map scope (scope first).
    focus_chain: Vec<NodeId>,
    map: bool,
    /// Directory whose guide is lit: the selection if it is an open
    /// folder, else its parent.
    active_guide: Option<NodeId>,
}

impl RowCtx {
    fn new(ctx: &Ctx, view: &ViewState) -> RowCtx {
        RowCtx {
            hover: view.map_hover,
            focus_chain: ctx.inv.ancestors(view.focus).collect(),
            map: view.tab == Tab::Map,
            active_guide: match view.selected {
                Some(s) if ctx.inv.node(s).is_dir() && view.expanded.contains(&s) => Some(s),
                Some(s) => ctx.inv.node(s).parent(),
                None => None,
            },
        }
    }
}

/// Vertical indent guides; the one enclosing the selection is lit.
fn guides(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, rc: &RowCtx, id: NodeId, depth: u16, r: Rect) {
    if depth == 0 {
        return;
    }
    // Ancestors of this row, root first, excluding the row itself.
    let mut chain: Vec<NodeId> = ctx.inv.ancestors(id).skip(1).collect();
    chain.reverse();
    for (k, &anc) in chain.iter().enumerate().skip(1) {
        let x = r.x + 8.0 + k as f32 * INDENT + 6.5;
        let lit = rc.active_guide == Some(anc);
        let color = if lit { alpha(theme::branch(view.family(ctx.inv, anc)), 0.55) } else { theme::LINE };
        ui.rect(Rect::new(x, r.y, 1.0, r.h), color, 0.0);
    }
}

/// Draws one row; returns true when it was clicked (selected).
#[allow(clippy::too_many_arguments)]
fn row(ui: &mut Ui, ctx: &Ctx, view: &mut ViewState, rc: &RowCtx, id: NodeId, depth: u16, r: Rect, pinned: bool, out: &mut Vec<Action>) -> bool {
    let n = ctx.inv.node(id);
    let selected = view.selected == Some(id);
    let marked = ctx.is_marked(id);
    let covered = !marked && ctx.is_covered(id);
    let hover = ui.hovered(r);
    let map_hot = rc.hover.is_some_and(|(e, s)| e == id || s == id);
    let tint = if id == NodeId::ROOT { theme::TEXT_3 } else { theme::branch(view.family(ctx.inv, id)) };
    // On the map tab, rows outside the scope (and not leading to it) recede.
    let outside = rc.map && !rc.focus_chain.contains(&id) && !ctx.inv.ancestors(id).any(|a| a == view.focus);

    if selected {
        ui.rect(r, theme::HOVER, theme::RADIUS_SM);
        ui.rect(Rect::new(r.x, r.y + 6.0, 2.0, r.h - 12.0), tint, 1.0);
    } else if hover || map_hot {
        ui.rect(r, alpha(theme::HOVER, if map_hot { 0.9 } else { 0.6 }), theme::RADIUS_SM);
    }

    let indent = 8.0 + depth as f32 * INDENT;
    // Share of parent, drawn as a faint bar along the bottom.
    if !pinned {
        let parent_size = n.parent().map_or(n.size(), |p| ctx.inv.node(p).size()).max(1);
        let share = n.size() as f32 / parent_size as f32;
        if share > 0.0 {
            let bar = Rect::new(r.x + indent + 14.0, r.bottom() - 3.0, (r.w - indent - 22.0) * share, 2.0);
            ui.rect(bar, alpha(tint, if outside { 0.15 } else { 0.35 }), 1.0);
        }
    }

    let mut c = r;
    c.take_left(indent);
    let chev = c.take_left(14.0);
    if n.is_dir() && (n.child_count() > 0 || !n.is_scanned()) {
        let open = view.expanded.contains(&id);
        let (h, click) = ui.interact(chev);
        ui.icon_in(chev, 12.0, if open { Icon::ChevronDown } else { Icon::ChevronRight }, if h { theme::TEXT } else { theme::TEXT_3 });
        if click {
            out.push(Action::ToggleExpand(id));
        }
    }
    let icon = c.take_left(21.0);
    // Folders wear their family colour; files their content's, quieter.
    let icon_tint = if n.is_dir() { tint } else { theme::mix(theme::category(n.category()), theme::TEXT_3, 0.25) };
    let icon_tint = if outside { alpha(icon_tint, 0.45) } else { icon_tint };
    ui.icon(icon.x, icon.cy() - 7.5, 15.0, widgets::kind_icon(n), icon_tint);

    let toggle = c.take_right(24.0);
    let size = c.take_right(72.0);
    let ink = if marked {
        theme::RECLAIM
    } else if covered {
        alpha(theme::RECLAIM, 0.7)
    } else if outside {
        theme::TEXT_3
    } else if n.is_dir() {
        theme::TEXT
    } else {
        theme::TEXT_2
    };
    if n.is_scanned() || !n.is_dir() {
        let size_ink = if marked {
            theme::RECLAIM
        } else if outside {
            theme::TEXT_3
        } else {
            theme::TEXT_2
        };
        widgets::size_text(ui, size, n.size(), theme::NUM, size_ink, Align::End);
    } else {
        widgets::spinner(ui, size.right() - 22.0, size.cy(), theme::TEXT_3);
    }
    let mut name = c;
    name.take_right(8.0);
    // Why a folder has no size: a glyph, named on hover.
    let flag = if n.is_protected() {
        Some((Icon::Lock, "Privacy-protected: not scanned"))
    } else if n.is_failed() {
        Some((Icon::Alert, "Unreadable: permission denied"))
    } else if n.is_boundary() {
        Some((Icon::Drive, "Another volume: not followed"))
    } else {
        None
    };
    if let Some((glyph, why)) = flag {
        let fr = name.take_right(20.0);
        ui.icon_in(fr, 13.0, glyph, theme::TEXT_3);
        if ui.hovered(fr) {
            widgets::tip(ui, fr, why);
        }
    }
    // The map scope carries the map glyph.
    if rc.map && id == view.focus {
        let m = name.take_right(20.0);
        ui.icon_in(m, 13.0, Icon::Map, theme::ACCENT);
    }
    ui.label(name, ctx.inv.name(id), if selected { theme::BODY_STRONG } else { theme::BODY }, ink, Align::Start);

    if id != NodeId::ROOT && widgets::mark_toggle(ui, toggle, marked, covered, hover || selected) {
        out.push(Action::ToggleMark(id));
        return false;
    }
    let mut clicked = false;
    if ui.double_clicked(r) && n.is_dir() {
        out.push(Action::Focus(id));
        out.push(Action::SetTab(Tab::Map));
    } else if ui.clicked(r) && !chev.contains(ui.input.mouse) {
        out.push(Action::Select(id));
        clicked = true;
    }
    if ui.right_clicked(r) && id != NodeId::ROOT {
        out.push(Action::ToggleMark(id));
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(depths: &[u16]) -> Vec<(NodeId, u16)> {
        depths.iter().enumerate().map(|(i, &d)| (NodeId(i as u32), d)).collect()
    }

    #[test]
    fn sticky_pins_ancestors_and_pushes_them_out() {
        let h = theme::ROW_H;
        // 0 root, 1..=6 a (1) with kids, 7 b (1) with kids
        let r = rows(&[0, 1, 2, 2, 2, 2, 2, 1, 2, 2]);
        assert!(sticky(&r, 0.0, 6).is_empty());
        // Scrolled two rows: root and `a` pinned.
        let s = sticky(&r, 2.0 * h, 6);
        assert_eq!(s.iter().map(|p| p.0).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(s[1].1, h);
        // Near the end of `a`'s subtree it slides up under root.
        let s = sticky(&r, 5.5 * h, 6);
        assert_eq!(s[1].0, 1);
        assert!(s[1].1 < h && s[1].1 > 0.0);
    }
}
