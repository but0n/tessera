//! Vector icons (ADR-0012). Each icon is a few SVG path strings on a 24-unit
//! grid, stroked at a uniform weight like the type, rasterised on demand
//! with tiny-skia at the exact physical size and cached in glyphon's atlas.
//! Icons are masks, so they take any token colour.

use glyphon::{ContentType, RasterizeCustomGlyphRequest, RasterizedCustomGlyph};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Icon {
    Folder,
    FolderFill,
    FolderOpen,
    File,
    Video,
    Image,
    Audio,
    Archive,
    Code,
    Document,
    Data,
    Package,
    Drive,
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    ArrowLeft,
    Refresh,
    Trash,
    Check,
    Close,
    Overview,
    Map,
    Reveal,
    Lock,
    Alert,
    Link,
    Flag,
    Mouse,
    Layers,
    Dots,
    Recycle,
}

impl Icon {
    pub const ALL: [Icon; 32] = [
        Icon::Folder,
        Icon::FolderFill,
        Icon::FolderOpen,
        Icon::File,
        Icon::Video,
        Icon::Image,
        Icon::Audio,
        Icon::Archive,
        Icon::Code,
        Icon::Document,
        Icon::Data,
        Icon::Package,
        Icon::Drive,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::ChevronLeft,
        Icon::ArrowLeft,
        Icon::Refresh,
        Icon::Trash,
        Icon::Check,
        Icon::Close,
        Icon::Overview,
        Icon::Map,
        Icon::Reveal,
        Icon::Lock,
        Icon::Alert,
        Icon::Link,
        Icon::Flag,
        Icon::Mouse,
        Icon::Layers,
        Icon::Dots,
        Icon::Recycle,
    ];

    pub fn from_id(id: u16) -> Option<Icon> {
        Icon::ALL.get(id as usize).copied()
    }
}

/// One path of an icon: stroked outline or solid fill.
enum Part {
    S(&'static str),
    F(&'static str),
}
use Part::{F, S};

const FOLDER: &str = "M3 7.5a2 2 0 0 1 2-2h3.6a2 2 0 0 1 1.4.6L11.4 7.5H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z";
const FILE: &str = "M7 3h7l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z";
const FOLD: &str = "M14 3v4a1 1 0 0 0 1 1h4";

fn parts(icon: Icon) -> &'static [Part] {
    match icon {
        Icon::Folder => &[S(FOLDER)],
        Icon::FolderFill => &[F(FOLDER)],
        Icon::FolderOpen => &[
            S("M4.5 19.5a1.5 1.5 0 0 1-1.5-1.5V6.5A1.5 1.5 0 0 1 4.5 5h3.4l2 2H17a1.5 1.5 0 0 1 1.5 1.5V10"),
            S("M4.5 19.5h12.6a1.5 1.5 0 0 0 1.4-1L21 11H7.6a1.5 1.5 0 0 0-1.4 1z"),
        ],
        Icon::File => &[S(FILE), S(FOLD)],
        Icon::Video => &[
            S("M5.5 5h13A2.5 2.5 0 0 1 21 7.5v9a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5v-9A2.5 2.5 0 0 1 5.5 5z"),
            F("M10 9.2v5.6a.6.6 0 0 0 .9.5l4.6-2.8a.6.6 0 0 0 0-1l-4.6-2.8a.6.6 0 0 0-.9.5z"),
        ],
        Icon::Image => &[
            S("M5.5 4h13A2.5 2.5 0 0 1 21 6.5v11a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5v-11A2.5 2.5 0 0 1 5.5 4z"),
            S("M3.5 17l5-5 4.5 4.5"),
            S("M12 15.5l3-3 5.5 5.5"),
            F("M15.5 7.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 1 0 0-3z"),
        ],
        Icon::Audio => &[S("M9 18V5.5l11-2.5v12.5"), S("M3.5 18a2.75 2.75 0 1 0 5.5 0 2.75 2.75 0 1 0-5.5 0z"), S("M14.5 15.5a2.75 2.75 0 1 0 5.5 0 2.75 2.75 0 1 0-5.5 0z")],
        Icon::Archive => {
            &[S("M4 4h16a1 1 0 0 1 1 1v2.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"), S("M4.5 8.5V19a1.5 1.5 0 0 0 1.5 1.5h12a1.5 1.5 0 0 0 1.5-1.5V8.5"), S("M10 12.5h4")]
        }
        Icon::Code => &[S("M8 7l-5 5 5 5"), S("M16 7l5 5-5 5"), S("M13.5 4.5l-3 15")],
        Icon::Document => &[S(FILE), S(FOLD), S("M9 13h6"), S("M9 16.5h6"), S("M9 9.5h1.5")],
        Icon::Data => &[S("M4 6a8 3 0 1 0 16 0 8 3 0 1 0-16 0z"), S("M4 6v12a8 3 0 0 0 16 0V6"), S("M4 12a8 3 0 0 0 16 0")],
        Icon::Package => &[S("M12 2.8l8 4.6v9.2l-8 4.6-8-4.6V7.4z"), S("M4.3 7.6L12 12l7.7-4.4"), S("M12 12v9")],
        Icon::Drive => &[
            S("M5 13h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2z"),
            S("M3.4 14.2L6.6 5.4A1.5 1.5 0 0 1 8 4.5h8a1.5 1.5 0 0 1 1.4.9l3.2 8.8"),
            F("M16.5 15.5a1 1 0 1 0 0 2 1 1 0 1 0 0-2z"),
        ],
        Icon::ChevronRight => &[S("M9.5 6l6 6-6 6")],
        Icon::ChevronDown => &[S("M6 9.5l6 6 6-6")],
        Icon::ChevronLeft => &[S("M14.5 6l-6 6 6 6")],
        Icon::ArrowLeft => &[S("M19 12H5"), S("M11 6l-6 6 6 6")],
        Icon::Refresh => &[S("M18.5 15.75A7.5 7.5 0 1 1 18.5 8.25"), S("M19.5 4.4l-1 3.85-3.9-1.05")],
        Icon::Trash => &[
            S("M4 6.5h16"),
            S("M9 6.5V4.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"),
            S("M6 6.5l.9 12.6A1.5 1.5 0 0 0 8.4 20.5h7.2a1.5 1.5 0 0 0 1.5-1.4L18 6.5"),
            S("M10 10.5v6"),
            S("M14 10.5v6"),
        ],
        Icon::Check => &[S("M5 12.5l4.5 4.5L19 7.5")],
        Icon::Close => &[S("M6.5 6.5l11 11"), S("M17.5 6.5l-11 11")],
        Icon::Overview => &[
            S("M5 3.5h4a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 11V5A1.5 1.5 0 0 1 5 3.5z"),
            S("M15 3.5h4A1.5 1.5 0 0 1 20.5 5v2A1.5 1.5 0 0 1 19 8.5h-4A1.5 1.5 0 0 1 13.5 7V5A1.5 1.5 0 0 1 15 3.5z"),
            S("M15 11.5h4a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-6a1.5 1.5 0 0 1 1.5-1.5z"),
            S("M5 15.5h4a1.5 1.5 0 0 1 1.5 1.5v2A1.5 1.5 0 0 1 9 20.5H5A1.5 1.5 0 0 1 3.5 19v-2A1.5 1.5 0 0 1 5 15.5z"),
        ],
        // A Voronoi tessellation in a rounded frame: the app's own shape.
        Icon::Map => {
            &[S("M6.5 3.5h11a3 3 0 0 1 3 3v11a3 3 0 0 1-3 3h-11a3 3 0 0 1-3-3v-11a3 3 0 0 1 3-3z"), S("M3.5 13.5L9.5 11 12 3.5"), S("M9.5 11l4.5 4.5 6.5-2.5"), S("M14 15.5l-1 5")]
        }
        Icon::Reveal => &[S("M14 3.5h6.5V10"), S("M20.5 3.5L11 13"), S("M18 14v4a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 4 18V9a2.5 2.5 0 0 1 2.5-2.5h4")],
        Icon::Lock => &[S("M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z"), S("M8 11V8a4 4 0 0 1 8 0v3")],
        Icon::Alert => &[S("M3 12a9 9 0 1 0 18 0 9 9 0 1 0-18 0z"), S("M12 7.5v5"), F("M12 15a1.15 1.15 0 1 0 0 2.3 1.15 1.15 0 1 0 0-2.3z")],
        Icon::Link => &[S("M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1"), S("M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1")],
        Icon::Flag => &[S("M5 21V4"), S("M5 4.5h11.5l-2.5 4 2.5 4H5")],
        Icon::Mouse => &[S("M12 3a6 6 0 0 1 6 6v6a6 6 0 0 1-12 0V9a6 6 0 0 1 6-6z"), S("M12 7v3.5")],
        Icon::Layers => &[S("M12 3.5l8.5 4.5-8.5 4.5-8.5-4.5z"), S("M3.5 12.5l8.5 4.5 8.5-4.5"), S("M3.5 16.5l8.5 4.5 8.5-4.5")],
        Icon::Dots => &[
            F("M6 6a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
            F("M12 6a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
            F("M18 6a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
            F("M6 14.8a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
            F("M12 14.8a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
            F("M18 14.8a1.6 1.6 0 1 0 0 3.2 1.6 1.6 0 1 0 0-3.2z"),
        ],
        Icon::Recycle => &[S("M20 12a8 8 0 0 1-13.66 5.66"), S("M4 12a8 8 0 0 1 13.66-5.66"), S("M18 3v4h-4"), S("M6 21v-4h4")],
    }
}

/// Stroke weight on the 24-unit grid; thinner at large sizes so icons keep
/// the optical weight of the text beside them.
fn weight(px: f32) -> f32 {
    if px <= 16.0 {
        1.9
    } else if px <= 24.0 {
        1.75
    } else {
        1.5
    }
}

/// Glyphon callback: rasterises `req` into an 8-bit coverage mask.
pub fn rasterize(req: RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph> {
    let icon = Icon::from_id(req.id)?;
    let (w, h) = (req.width as u32, req.height as u32);
    let mut pixmap = Pixmap::new(w.max(1), h.max(1))?;
    draw(&mut pixmap, icon, w as f32 / req.scale.max(0.01));
    let data = pixmap.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    Some(RasterizedCustomGlyph { data, content_type: ContentType::Mask })
}

/// Draws `icon` filling `pixmap`; `logical` is its size in points.
pub fn draw(pixmap: &mut Pixmap, icon: Icon, logical: f32) {
    let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
    let t = Transform::from_scale(w / 24.0, h / 24.0);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let stroke = Stroke { width: weight(logical), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    for part in parts(icon) {
        match part {
            S(d) => {
                if let Some(p) = parse(d) {
                    pixmap.stroke_path(&p, &paint, &stroke, t, None);
                }
            }
            F(d) => {
                if let Some(p) = parse(d) {
                    pixmap.fill_path(&p, &paint, FillRule::Winding, t, None);
                }
            }
        }
    }
}

/// Parses SVG path data (M L H V C S Q A Z, absolute and relative).
fn parse(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut nums = Numbers { s: d.as_bytes(), i: 0 };
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut last_ctrl: Option<(f32, f32)> = None;
    let mut cmd = b'M';
    loop {
        nums.skip_ws();
        let Some(&c) = nums.s.get(nums.i) else { break };
        if c.is_ascii_alphabetic() {
            cmd = c;
            nums.i += 1;
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (nums.next()? + ox, nums.next()? + oy);
                pb.move_to(x, y);
                (cx, cy, sx, sy) = (x, y, x, y);
                last_ctrl = None;
            }
            b'L' => {
                let (x, y) = (nums.next()? + ox, nums.next()? + oy);
                pb.line_to(x, y);
                (cx, cy) = (x, y);
                last_ctrl = None;
            }
            b'H' => {
                cx = nums.next()? + ox;
                pb.line_to(cx, cy);
                last_ctrl = None;
            }
            b'V' => {
                cy = nums.next()? + oy;
                pb.line_to(cx, cy);
                last_ctrl = None;
            }
            b'C' => {
                let v: Vec<f32> = (0..6).map(|_| nums.next()).collect::<Option<_>>()?;
                let p = [v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy, v[4] + ox, v[5] + oy];
                pb.cubic_to(p[0], p[1], p[2], p[3], p[4], p[5]);
                last_ctrl = Some((p[2], p[3]));
                (cx, cy) = (p[4], p[5]);
            }
            b'S' => {
                let v: Vec<f32> = (0..4).map(|_| nums.next()).collect::<Option<_>>()?;
                let (c1x, c1y) = last_ctrl.map_or((cx, cy), |(lx, ly)| (2.0 * cx - lx, 2.0 * cy - ly));
                let p = [v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy];
                pb.cubic_to(c1x, c1y, p[0], p[1], p[2], p[3]);
                last_ctrl = Some((p[0], p[1]));
                (cx, cy) = (p[2], p[3]);
            }
            b'Q' => {
                let v: Vec<f32> = (0..4).map(|_| nums.next()).collect::<Option<_>>()?;
                pb.quad_to(v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy);
                (cx, cy) = (v[2] + ox, v[3] + oy);
                last_ctrl = None;
            }
            b'A' => {
                let (rx, ry, rot) = (nums.next()?, nums.next()?, nums.next()?);
                let (large, sweep) = (nums.flag()?, nums.flag()?);
                let (x, y) = (nums.next()? + ox, nums.next()? + oy);
                arc(&mut pb, (cx, cy), (rx, ry), rot, large, sweep, (x, y));
                (cx, cy) = (x, y);
                last_ctrl = None;
            }
            b'Z' => {
                pb.close();
                (cx, cy) = (sx, sy);
                last_ctrl = None;
            }
            _ => return None,
        }
    }
    pb.finish()
}

struct Numbers<'a> {
    s: &'a [u8],
    i: usize,
}

impl Numbers<'_> {
    fn skip_ws(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace() || *c == b',') {
            self.i += 1;
        }
    }
    fn next(&mut self) -> Option<f32> {
        self.skip_ws();
        let start = self.i;
        let mut seen_dot = false;
        let mut seen_digit = false;
        if matches!(self.s.get(self.i), Some(b'-' | b'+')) {
            self.i += 1;
        }
        while let Some(&c) = self.s.get(self.i) {
            if c.is_ascii_digit() {
                seen_digit = true;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
            } else {
                break;
            }
            self.i += 1;
        }
        if !seen_digit {
            self.i = start;
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
    /// Arc flags may be written without separators ("a2 2 0 011 1").
    fn flag(&mut self) -> Option<bool> {
        self.skip_ws();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// SVG elliptical arc → cubic Béziers (SVG spec, appendix F.6).
fn arc(pb: &mut PathBuilder, p0: (f32, f32), r: (f32, f32), rot_deg: f32, large: bool, sweep: bool, p1: (f32, f32)) {
    let (mut rx, mut ry) = (r.0.abs() as f64, r.1.abs() as f64);
    let (x0, y0, x1, y1) = (p0.0 as f64, p0.1 as f64, p1.0 as f64, p1.1 as f64);
    if rx < 1e-9 || ry < 1e-9 || ((x0 - x1).abs() < 1e-9 && (y0 - y1).abs() < 1e-9) {
        pb.line_to(p1.0, p1.1);
        return;
    }
    let (sin, cos) = (rot_deg as f64).to_radians().sin_cos();
    let (dx, dy) = ((x0 - x1) / 2.0, (y0 - y1) / 2.0);
    let (x1p, y1p) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut k = (num / den).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let (cxp, cyp) = (k * rx * y1p / ry, -k * ry * x1p / rx);
    let (cx, cy) = (cos * cxp - sin * cyp + (x0 + x1) / 2.0, sin * cxp + cos * cyp + (y0 + y1) / 2.0);
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        let s = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
        s * a.clamp(-1.0, 1.0).acos()
    };
    let t1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let tau = std::f64::consts::TAU;
    if !sweep && dt > 0.0 {
        dt -= tau;
    } else if sweep && dt < 0.0 {
        dt += tau;
    }
    let segs = (dt.abs() / (std::f64::consts::FRAC_PI_2)).ceil().max(1.0) as usize;
    let step = dt / segs as f64;
    let alpha = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (cx + rx * c * cos - ry * s * sin, cy + rx * c * sin + ry * s * cos)
    };
    let deriv = |t: f64| {
        let (s, c) = t.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    let mut t = t1;
    for _ in 0..segs {
        let (a, b) = (point(t), point(t + step));
        let (da, db) = (deriv(t), deriv(t + step));
        pb.cubic_to((a.0 + alpha * da.0) as f32, (a.1 + alpha * da.1) as f32, (b.0 - alpha * db.0) as f32, (b.1 - alpha * db.1) as f32, b.0 as f32, b.1 as f32);
        t += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses_and_inks() {
        for icon in Icon::ALL {
            assert_eq!(Icon::from_id(icon as u16), Some(icon));
            for p in parts(icon) {
                let (S(d) | F(d)) = p;
                assert!(parse(d).is_some(), "{icon:?}: {d}");
            }
            let mut pm = Pixmap::new(32, 32).unwrap();
            draw(&mut pm, icon, 16.0);
            let ink: u32 = pm.data().as_chunks::<4>().0.iter().map(|p| p[3] as u32).sum();
            assert!(ink > 255 * 8, "{icon:?} draws something");
        }
    }

    /// `TESSERA_ICON_SHEET=/tmp/icons.png cargo test -p tessera-ui icon_sheet -- --ignored`
    #[test]
    #[ignore]
    fn icon_sheet() {
        let Ok(out) = std::env::var("TESSERA_ICON_SHEET") else { return };
        let (cell, sizes) = (56u32, [16.0f32, 24.0, 48.0]);
        let mut sheet = Pixmap::new(cell * Icon::ALL.len() as u32, cell * sizes.len() as u32).unwrap();
        sheet.fill(tiny_skia::Color::from_rgba8(16, 17, 20, 255));
        for (row, &px) in sizes.iter().enumerate() {
            for (i, &icon) in Icon::ALL.iter().enumerate() {
                let mut pm = Pixmap::new(px as u32, px as u32).unwrap();
                draw(&mut pm, icon, px);
                let off = ((cell as f32 - px) / 2.0) as i32;
                sheet.draw_pixmap(i as i32 * cell as i32 + off, row as i32 * cell as i32 + off, pm.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
            }
        }
        let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
        let mut enc = png::Encoder::new(&mut f, sheet.width(), sheet.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(sheet.data()).unwrap();
    }
}
