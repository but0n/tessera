//! Reusable controls. All take a rect and return what happened.

use tessera_domain::size::group_digits;
use tessera_domain::{ByteSize, Category, EntryKind, Node};

use crate::gfx::icons::Icon;
use crate::gfx::text::TextStyle;
use crate::theme::{self, Rgba, alpha, mix};
use crate::ui::{Align, Rect, Ui};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    /// Filled vermilion: the single destructive call to action (commit).
    Reclaim,
    /// Vermilion-tinted: adds to the plan, deletes nothing.
    Mark,
    Secondary,
}

pub fn button(ui: &mut Ui, r: Rect, label: &str, kind: ButtonKind, enabled: bool) -> bool {
    button_icon(ui, r, None, label, kind, enabled)
}

/// Button with a leading icon; the pair is centred as one unit.
pub fn button_icon(ui: &mut Ui, r: Rect, icon: Option<Icon>, label: &str, kind: ButtonKind, enabled: bool) -> bool {
    let (hover, click) = if enabled { ui.interact(r) } else { (false, false) };
    let (fill, border, ink) = match kind {
        ButtonKind::Reclaim if enabled => (if hover { mix(theme::RECLAIM, theme::TEXT, 0.12) } else { theme::RECLAIM }, theme::RECLAIM, theme::ON_RECLAIM),
        ButtonKind::Reclaim | ButtonKind::Mark if !enabled => (alpha(theme::RECLAIM, 0.08), alpha(theme::RECLAIM, 0.0), alpha(theme::RECLAIM, 0.45)),
        ButtonKind::Reclaim | ButtonKind::Mark => (alpha(theme::RECLAIM, if hover { 0.2 } else { 0.12 }), alpha(theme::RECLAIM, if hover { 0.7 } else { 0.4 }), theme::RECLAIM),
        ButtonKind::Secondary => (if hover { theme::HOVER } else { theme::RAISED }, theme::LINE, if enabled { theme::TEXT } else { theme::TEXT_3 }),
    };
    ui.rect_border(r, fill, border, if kind == ButtonKind::Reclaim { 0.0 } else { 1.0 }, theme::RADIUS_SM + 1.0);
    let Some(icon) = icon else {
        ui.label(r.inset_xy(10.0, 0.0), label, theme::BODY_STRONG, ink, Align::Center);
        return click;
    };
    let size = theme::ICON;
    let w = (ui.measure(label, theme::BODY_STRONG) + size + 7.0).min(r.w - 20.0);
    let x = r.cx() - w / 2.0;
    ui.icon(x, r.cy() - size / 2.0, size, icon, ink);
    ui.label(Rect::new(x + size + 7.0, r.y, w - size - 7.0, r.h), label, theme::BODY_STRONG, ink, Align::Start);
    click
}

/// Square, borderless button holding one icon; `tip` names it on hover.
pub fn icon_button(ui: &mut Ui, r: Rect, icon: Icon, tip_text: &str, enabled: bool) -> bool {
    let (hover, click) = if enabled { ui.interact(r) } else { (false, false) };
    if hover {
        ui.rect(r, theme::HOVER, theme::RADIUS_SM);
    }
    let ink = if !enabled {
        alpha(theme::TEXT_3, 0.5)
    } else if hover {
        theme::TEXT
    } else {
        theme::TEXT_2
    };
    ui.icon_in(r, theme::ICON, icon, ink);
    if hover && !tip_text.is_empty() {
        tip(ui, r, tip_text);
    }
    click
}

/// Small label under `anchor`, naming an icon-only control.
pub fn tip(ui: &mut Ui, anchor: Rect, text: &str) {
    let layer = std::mem::replace(&mut ui.layer, 2);
    let w = ui.measure(text, theme::SMALL) + 16.0;
    let screen = ui.screen();
    let x = (anchor.cx() - w / 2.0).clamp(screen.x + 6.0, screen.right() - w - 6.0);
    let r = Rect::new(x, anchor.bottom() + 6.0, w, 22.0);
    ui.shadow(r, alpha(theme::BG, 0.6), theme::RADIUS_SM, 8.0);
    ui.rect_border(r, theme::RAISED, theme::LINE, 1.0, theme::RADIUS_SM);
    ui.label(r, text, theme::SMALL, theme::TEXT, Align::Center);
    ui.layer = layer;
}

/// The icon for an entry: its kind, or for files its content category.
pub fn kind_icon(n: &Node) -> Icon {
    match n.kind() {
        EntryKind::Dir => Icon::FolderFill,
        EntryKind::Symlink => Icon::Link,
        EntryKind::Other => Icon::File,
        EntryKind::File => category_icon(n.category()),
    }
}

pub fn category_icon(c: Category) -> Icon {
    match c {
        Category::Video => Icon::Video,
        Category::Image => Icon::Image,
        Category::Audio => Icon::Audio,
        Category::Archive => Icon::Archive,
        Category::Code => Icon::Code,
        Category::Document => Icon::Document,
        Category::Data => Icon::Data,
        Category::Binary => Icon::Package,
        Category::Folder => Icon::FolderFill,
        Category::Other => Icon::File,
    }
}

/// Segmented control; returns the index clicked.
pub fn segmented(ui: &mut Ui, r: Rect, options: &[(Icon, &str)], selected: usize) -> Option<usize> {
    ui.rect_border(r, theme::PANEL, theme::LINE, 1.0, theme::RADIUS_SM + 2.0);
    let w = r.w / options.len() as f32;
    let mut out = None;
    for (i, o) in options.iter().enumerate() {
        let cell = Rect::new(r.x + w * i as f32, r.y, w, r.h).inset(3.0);
        let (hover, click) = ui.interact(cell);
        if i == selected {
            ui.shadow(cell, alpha(theme::BG, 0.5), theme::RADIUS_SM, 3.0);
            ui.rect(cell, theme::HOVER, theme::RADIUS_SM);
        } else if hover {
            ui.rect(cell, alpha(theme::HOVER, 0.5), theme::RADIUS_SM);
        }
        let (icon, label) = *o;
        let ink = if i == selected { theme::TEXT } else { theme::TEXT_2 };
        let size = 14.0;
        let w = ui.measure(label, theme::SMALL_STRONG) + size + 6.0;
        let x = cell.cx() - w / 2.0;
        ui.icon(x, cell.cy() - size / 2.0, size, icon, if i == selected { theme::ACCENT } else { ink });
        ui.text(x + size + 6.0, cell.cy() - theme::SMALL_STRONG.line_height() / 2.0, label, theme::SMALL_STRONG, ink);
        if click {
            out = Some(i);
        }
    }
    out
}

/// Round mark toggle. `visible` lets rows show it only on hover.
pub fn mark_toggle(ui: &mut Ui, r: Rect, marked: bool, covered: bool, visible: bool) -> bool {
    let (hover, click) = ui.interact(r);
    if !(visible || marked || covered || hover) {
        return false;
    }
    let d = r.w.min(r.h).min(16.0);
    let c = Rect::new(r.cx() - d / 2.0, r.cy() - d / 2.0, d, d);
    if marked {
        ui.rect(c, theme::RECLAIM, d / 2.0);
        ui.icon_in(c, d - 4.0, Icon::Check, theme::ON_RECLAIM);
    } else if covered {
        ui.rect_border(c, alpha(theme::RECLAIM, 0.15), alpha(theme::RECLAIM, 0.6), 1.0, d / 2.0);
    } else {
        ui.rect_border(c, if hover { alpha(theme::RECLAIM, 0.12) } else { alpha(theme::BG, 0.0) }, if hover { theme::RECLAIM } else { theme::TEXT_3 }, 1.2, d / 2.0);
    }
    click
}

/// Uppercase caption used for every section title.
pub fn caption(ui: &mut Ui, x: f32, y: f32, text: &str, color: Rgba) -> f32 {
    ui.text(x, y, &text.to_uppercase(), theme::CAPTION, color)
}

/// Tinted pill with a short label.
pub fn badge(ui: &mut Ui, x: f32, cy: f32, text: &str, color: Rgba) -> f32 {
    let w = ui.measure(text, theme::CAPTION) + 12.0;
    let r = Rect::new(x, cy - 9.0, w, 18.0);
    ui.rect(r, alpha(color, 0.14), 9.0);
    ui.label(r, text, theme::CAPTION, color, Align::Center);
    w
}

pub fn dot(ui: &mut Ui, cx: f32, cy: f32, d: f32, color: Rgba) {
    ui.rect(Rect::new(cx - d / 2.0, cy - d / 2.0, d, d), color, d / 2.0);
}

/// Horizontal meter: track plus a fill of `frac`.
pub fn meter(ui: &mut Ui, r: Rect, frac: f32, color: Rgba) {
    ui.rect(r, theme::LINE_SOFT, r.h / 2.0);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        ui.rect(Rect::new(r.x, r.y, (r.w * f).max(r.h), r.h), color, r.h / 2.0);
    }
}

/// "12.4 GB" with the unit set smaller and dimmer than the number.
pub fn size_text(ui: &mut Ui, r: Rect, bytes: u64, style: TextStyle, color: Rgba, align: Align) -> f32 {
    let (n, u) = ByteSize(bytes).parts();
    let unit_style = TextStyle { size: (style.size * 0.72).max(10.0), weight: 500, ..style };
    let wn = ui.measure(&n, style);
    let wu = ui.measure(u, unit_style);
    let gap = (style.size * 0.18).max(2.0);
    let total = wn + gap + wu;
    let x = match align {
        Align::Start => r.x,
        Align::Center => r.cx() - total / 2.0,
        Align::End => r.right() - total,
    };
    let y = r.cy() - style.line_height() / 2.0;
    ui.text(x, y, &n, style, color);
    // Share the number's baseline (≈ line centre + 0.35 em).
    let yu = y + (style.line_height() - unit_style.line_height()) * 0.5 + 0.35 * (style.size - unit_style.size);
    ui.text(x + wn + gap, yu, u, unit_style, mix(color, theme::BG, 0.35));
    total
}

pub fn count(n: u64) -> String {
    group_digits(n)
}

pub fn percent(f: f64) -> String {
    let p = f * 100.0;
    if p <= 0.0 {
        "0%".into()
    } else if p < 0.1 {
        "<0.1%".into()
    } else if p < 10.0 {
        format!("{p:.1}%")
    } else {
        format!("{p:.0}%")
    }
}

/// Card surface used by every dashboard block.
pub fn card(ui: &mut Ui, r: Rect) {
    ui.rect_border(r, theme::PANEL, theme::LINE_SOFT, 1.0, theme::RADIUS);
}

/// Three pulsing dots.
pub fn spinner(ui: &mut Ui, x: f32, cy: f32, color: Rgba) {
    let t = ui.time();
    for i in 0..3 {
        let phase = (t * 5.0 - i as f32 * 0.7).sin() * 0.5 + 0.5;
        dot(ui, x + i as f32 * 7.0 + 2.5, cy, 4.0, alpha(color, 0.3 + 0.7 * phase));
    }
    ui.animating = true;
}

/// Scrollbar for a region scrolled by `offset`. The thumb can be dragged
/// (`drag` holds the offset at drag start) and the track clicked to page.
/// Returns the new offset when the user moved it.
pub fn scrollbar(ui: &mut Ui, r: Rect, content_h: f32, offset: f32, drag: &mut Option<f32>) -> Option<f32> {
    if content_h <= r.h + 1.0 {
        *drag = None;
        return None;
    }
    let range = content_h - r.h;
    let th = (r.h * r.h / content_h).max(28.0);
    let ty = r.y + (r.h - th) * (offset / range).clamp(0.0, 1.0);
    let track = Rect::new(r.right() - 11.0, r.y, 11.0, r.h);
    let thumb = Rect::new(track.x, ty, track.w, th);
    let hot = ui.hovered(track) || drag.is_some();
    let mut out = None;
    if ui.input.pressed && ui.hovered(track) {
        if thumb.contains(ui.input.mouse) {
            *drag = Some(offset);
        } else {
            let page = if ui.input.mouse.1 < ty { -r.h } else { r.h } * 0.9;
            out = Some((offset + page).clamp(0.0, range));
        }
    }
    if let Some(start) = *drag {
        if ui.input.down {
            let dy = ui.input.mouse.1 - ui.input.press_pos.1;
            out = Some((start + dy * range / (r.h - th).max(1.0)).clamp(0.0, range));
        } else {
            *drag = None;
        }
    }
    let w = if hot { 6.0 } else { 3.0 };
    ui.rect(Rect::new(r.right() - 3.0 - w, ty, w, th), alpha(theme::TEXT, if hot { 0.3 } else { 0.14 }), w / 2.0);
    out
}
