//! Minimal immediate-mode UI core: geometry, input snapshot, draw lists.
//!
//! Views are plain functions that read state, emit primitives and return
//! [`crate::state::Action`]s; they never mutate the domain directly.

use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::CursorIcon;

use crate::gfx::text::{TextItem, TextStyle, TextSystem};
use crate::gfx::{Cell, LAYERS, Quad};
use crate::theme::Rgba;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w * 0.5
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h * 0.5
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.right() && p.1 >= self.y && p.1 < self.bottom()
    }
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_xy(d, d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    pub fn take_top(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.x, self.y, self.w, h.min(self.h));
        self.y += r.h;
        self.h -= r.h;
        r
    }
    pub fn take_bottom(&mut self, h: f32) -> Rect {
        let h = h.min(self.h);
        self.h -= h;
        Rect::new(self.x, self.y + self.h, self.w, h)
    }
    pub fn take_left(&mut self, w: f32) -> Rect {
        let r = Rect::new(self.x, self.y, w.min(self.w), self.h);
        self.x += r.w;
        self.w -= r.w;
        r
    }
    pub fn take_right(&mut self, w: f32) -> Rect {
        let w = w.min(self.w);
        self.w -= w;
        Rect::new(self.x + self.w, self.y, w, self.h)
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        Rect::new(x, y, (self.right().min(o.right()) - x).max(0.0), (self.bottom().min(o.bottom()) - y).max(0.0))
    }
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    fn clip4(&self) -> [f32; 4] {
        [self.x, self.y, self.right(), self.bottom()]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub mouse: (f32, f32),
    pub inside: bool,
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub press_pos: (f32, f32),
    pub double: bool,
    pub right_pressed: bool,
    /// Mouse "back" button.
    pub back_pressed: bool,
    pub wheel: (f32, f32),
    pub keys: Vec<Key>,
    pub mods: ModifiersState,
    pub last_click: Option<(std::time::Instant, (f32, f32))>,
}

impl Input {
    pub fn end_frame(&mut self) {
        self.pressed = false;
        self.released = false;
        self.double = false;
        self.right_pressed = false;
        self.back_pressed = false;
        self.wheel = (0.0, 0.0);
        self.keys.clear();
    }

    pub fn key(&self, k: NamedKey) -> bool {
        self.keys.contains(&Key::Named(k))
    }

    pub fn char(&self, c: &str) -> bool {
        self.keys.iter().any(|x| matches!(x, Key::Character(s) if s.eq_ignore_ascii_case(c)))
    }
}

#[derive(Default)]
pub struct Layer {
    pub quads: Vec<Quad>,
    pub cells: Vec<Cell>,
    pub texts: Vec<TextItem>,
}

/// Everything the renderer needs for one frame.
pub struct Frame {
    pub size: (f32, f32),
    pub scale: f32,
    pub time: f32,
    pub layers: [Layer; LAYERS],
    pub verts: Vec<[f32; 2]>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

pub struct Ui<'a> {
    pub input: &'a Input,
    pub text: &'a mut TextSystem,
    pub frame: Frame,
    pub layer: usize,
    clip: Rect,
    /// Input is only delivered to this layer and above (modal dialogs).
    pub input_floor: usize,
    pub cursor: CursorIcon,
    /// Something is animating; keep redrawing.
    pub animating: bool,
}

impl<'a> Ui<'a> {
    pub fn new(input: &'a Input, text: &'a mut TextSystem, size: (f32, f32), scale: f32, time: f32) -> Self {
        text.begin_frame(scale);
        let full = Rect::new(0.0, 0.0, size.0, size.1);
        Ui {
            input,
            text,
            frame: Frame { size, scale, time, layers: Default::default(), verts: Vec::with_capacity(4096) },
            layer: 0,
            clip: full,
            input_floor: 0,
            cursor: CursorIcon::Default,
            animating: false,
        }
    }

    pub fn screen(&self) -> Rect {
        Rect::new(0.0, 0.0, self.frame.size.0, self.frame.size.1)
    }

    pub fn time(&self) -> f32 {
        self.frame.time
    }

    pub fn set_clip(&mut self, r: Rect) -> Rect {
        std::mem::replace(&mut self.clip, r)
    }

    // ---------------------------------------------------------- input ---

    fn accepts_input(&self) -> bool {
        self.layer >= self.input_floor
    }

    pub fn hovered(&self, r: Rect) -> bool {
        self.accepts_input() && self.input.inside && r.contains(self.input.mouse) && self.clip.contains(self.input.mouse)
    }

    pub fn clicked(&self, r: Rect) -> bool {
        self.input.released && self.hovered(r) && r.contains(self.input.press_pos)
    }

    pub fn double_clicked(&self, r: Rect) -> bool {
        self.input.double && self.hovered(r)
    }

    pub fn right_clicked(&self, r: Rect) -> bool {
        self.input.right_pressed && self.hovered(r)
    }

    /// Hover + pointer cursor for clickable things.
    pub fn interact(&mut self, r: Rect) -> (bool, bool) {
        let h = self.hovered(r);
        if h {
            self.cursor = CursorIcon::Pointer;
        }
        (h, self.clicked(r))
    }

    // --------------------------------------------------------- drawing ---

    fn layer_mut(&mut self) -> &mut Layer {
        &mut self.frame.layers[self.layer]
    }

    pub fn quad(&mut self, q: Quad) {
        let mut q = q;
        q.clip = self.clip.intersect(&Rect::new(q.clip[0], q.clip[1], q.clip[2] - q.clip[0], q.clip[3] - q.clip[1])).clip4();
        self.layer_mut().quads.push(q);
    }

    pub fn rect(&mut self, r: Rect, color: Rgba, radius: f32) {
        self.quad(Quad { rect: [r.x, r.y, r.w, r.h], color, color2: color, border: [0.0; 4], params: [radius, 0.0, 0.0, 0.0], clip: [-1e5, -1e5, 1e5, 1e5] });
    }

    pub fn rect_border(&mut self, r: Rect, fill: Rgba, border: Rgba, width: f32, radius: f32) {
        self.quad(Quad { rect: [r.x, r.y, r.w, r.h], color: fill, color2: fill, border, params: [radius, width, 0.0, 0.0], clip: [-1e5, -1e5, 1e5, 1e5] });
    }

    pub fn gradient(&mut self, r: Rect, a: Rgba, b: Rgba, radius: f32, vertical: bool) {
        self.quad(Quad {
            rect: [r.x, r.y, r.w, r.h],
            color: a,
            color2: b,
            border: [0.0; 4],
            params: [radius, 0.0, 0.0, if vertical { 2.0 } else { 0.0 }],
            clip: [-1e5, -1e5, 1e5, 1e5],
        });
    }

    pub fn stripes(&mut self, r: Rect, a: Rgba, b: Rgba, radius: f32) {
        self.quad(Quad { rect: [r.x, r.y, r.w, r.h], color: a, color2: b, border: [0.0; 4], params: [radius, 0.0, 0.0, 1.0], clip: [-1e5, -1e5, 1e5, 1e5] });
    }

    pub fn shadow(&mut self, r: Rect, color: Rgba, radius: f32, blur: f32) {
        self.quad(Quad { rect: [r.x, r.y, r.w, r.h], color, color2: color, border: [0.0; 4], params: [radius, 0.0, blur, 0.0], clip: [-1e5, -1e5, 1e5, 1e5] });
    }

    /// Convex polygon cell; `pts` must have positive signed area.
    #[allow(clippy::too_many_arguments)]
    pub fn cell(&mut self, pts: &[[f32; 2]], color: Rgba, accent: Rgba, radius: f32, flags: u32, light: f32, opacity: f32) {
        if pts.len() < 3 {
            return;
        }
        let start = self.frame.verts.len() as u32;
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        self.frame.verts.extend_from_slice(pts);
        let clip = self.clip.clip4();
        self.layer_mut().cells.push(Cell {
            bbox: [lo[0], lo[1], hi[0], hi[1]],
            color,
            accent,
            range: [start, pts.len() as u32],
            params: [radius, flags as f32, light, opacity],
            clip,
        });
    }

    // ------------------------------------------------------------ text ---

    pub fn measure(&mut self, s: &str, style: TextStyle) -> f32 {
        self.text.measure(s, &style)
    }

    /// Draws `s` with its top-left at `(x, y)`; returns its width.
    pub fn text(&mut self, x: f32, y: f32, s: &str, style: TextStyle, color: Rgba) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let w = self.text.measure(s, &style);
        let clip = self.clip;
        self.layer_mut().texts.push(TextItem { text: s.to_owned(), style, pos: (x, y), color, clip });
        w
    }

    /// Draws `s` inside `r`, elided to fit, aligned horizontally, centred
    /// vertically. Returns the drawn width.
    pub fn label(&mut self, r: Rect, s: &str, style: TextStyle, color: Rgba, align: Align) -> f32 {
        if r.w <= 2.0 || s.is_empty() {
            return 0.0;
        }
        let s = self.text.elide(s, &style, r.w);
        let w = self.text.measure(&s, &style);
        let x = match align {
            Align::Start => r.x,
            Align::Center => r.cx() - w * 0.5,
            Align::End => r.right() - w,
        };
        let y = r.cy() - style.line_height() * 0.5;
        self.text(x, y, &s, style, color)
    }

    pub fn finish(self) -> (Frame, CursorIcon, bool) {
        (self.frame, self.cursor, self.animating)
    }
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}
