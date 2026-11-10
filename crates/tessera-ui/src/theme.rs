//! Design tokens (ADR-0008). Every colour, size and type style in the UI
//! comes from here; views never hard-code visual constants.

use tessera_domain::{Category, ReclaimHint};

use crate::gfx::text::{Font, TextStyle};

/// sRGB, straight alpha. The surface is non-sRGB so blending matches
/// design tools and browsers.
pub type Rgba = [f32; 4];

pub const fn hex(v: u32) -> Rgba {
    [((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0, 1.0]
}

pub const fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

// Every colour below is specified in OKLCH (perceptual lightness, chroma,
// hue) and baked to sRGB hex, so equal steps *look* equal (ADR-0011).
// Neutrals share one cool hue (265°) at very low chroma.

// Surfaces, darkest to lightest (L 0.155 → 0.245).
pub const BG: Rgba = hex(0x0B0C0F);
pub const PANEL: Rgba = hex(0x101114);
pub const RAISED: Rgba = hex(0x17191D);
pub const HOVER: Rgba = hex(0x1E2025);
pub const LINE: Rgba = hex(0x25282D);
pub const LINE_SOFT: Rgba = hex(0x191B1F);

// Ink (L 0.945 / 0.73 / 0.555).
pub const TEXT: Rgba = hex(0xEEEDEA);
pub const TEXT_2: Rgba = hex(0xA4A8AF);
pub const TEXT_3: Rgba = hex(0x6F737C);

// Semantics. Each hue has exactly one meaning.
/// Vermilion: "will be removed". Nothing else uses it.
pub const RECLAIM: Rgba = hex(0xF36D42);
/// Ink on a vermilion fill.
pub const ON_RECLAIM: Rgba = hex(0x2A130C);
/// Interaction accent: focus, scope, progress, the scan itself.
pub const ACCENT: Rgba = hex(0x78AEF2);
/// Done / free space.
pub const SAFE: Rgba = hex(0x6CCEA1);
/// Uncategorised content.
pub const NEUTRAL: Rgba = hex(0x6E7279);

/// Branch hues for the map, ordered so neighbours in size order contrast.
/// All sit at the same lightness and chroma, and skip the vermilion band
/// (hue 10°–60°), so no branch can be mistaken for "marked".
pub const BRANCH_HUES: [f32; 8] = [250.0, 78.0, 172.0, 300.0, 140.0, 212.0, 338.0, 108.0];
/// `BRANCH_HUES` at L 0.74, C 0.085: the branch's "ink" (labels, dots, bars).
pub const BRANCH: [Rgba; 8] = [hex(0x81AFDF), hex(0xC9A46C), hex(0x6FBDA5), hex(0xB39FD9), hex(0x8EB885), hex(0x65B9CA), hex(0xCF96BE), hex(0xB0AF6F)];

pub fn branch(i: u16) -> Rgba {
    BRANCH[i as usize % BRANCH.len()]
}

/// How a map cell is drawn; each tone is a fixed lightness step, so a
/// cell's role reads the same in every colour family.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Tone {
    /// A folder showing its contents, at nesting depth `d` (1 = outermost).
    Frame(u8),
    /// A folder too small to show contents.
    Folder,
    File,
    /// "Small items" aggregate: a muted folder tone (it is drawn dotted).
    Rest,
}

/// Fill for a map cell of `branch`, in OKLCH so every family has the same
/// visual weight. `rank` adds a small lightness variation between siblings.
pub fn tone(branch: u16, t: Tone, rank: u16) -> Rgba {
    let h = BRANCH_HUES[branch as usize % BRANCH_HUES.len()];
    let j = (rank % 3) as f32 * 0.022;
    let (l, c) = match t {
        Tone::Frame(d) => {
            let k = d.saturating_sub(1).min(4) as f32;
            (0.255 + 0.032 * k, 0.026 + 0.008 * k)
        }
        Tone::Folder => (0.475 - j, 0.066),
        Tone::File => (0.56 - j, 0.078),
        Tone::Rest => (0.36, 0.028),
    };
    oklch(l, c, h)
}

/// OKLCH → sRGB, clamped to gamut.
pub fn oklch(l: f32, c: f32, h_deg: f32) -> Rgba {
    let (sin, cos) = h_deg.to_radians().sin_cos();
    let (a, b) = (c * cos, c * sin);
    let l_ = l + 0.396_337_8 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    let enc = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
    };
    [enc(lin[0]), enc(lin[1]), enc(lin[2]), 1.0]
}

pub fn category(c: Category) -> Rgba {
    match c {
        Category::Video => BRANCH[0],
        Category::Image => BRANCH[4],
        Category::Archive => BRANCH[1],
        Category::Audio => BRANCH[3],
        Category::Data => BRANCH[5],
        Category::Code => BRANCH[2],
        Category::Binary => BRANCH[7],
        Category::Document => BRANCH[6],
        Category::Other | Category::Folder => NEUTRAL,
    }
}

/// Category colour as a map-cell fill (same tone as a file cell).
pub fn category_fill(c: Category) -> Rgba {
    let i = BRANCH.iter().position(|b| *b == category(c));
    match i {
        Some(i) => tone(i as u16, Tone::File, 0),
        None => oklch(0.42, 0.01, 265.0),
    }
}

pub fn hint(h: ReclaimHint) -> Rgba {
    match h {
        ReclaimHint::Dependencies => BRANCH[3],
        ReclaimHint::BuildArtifacts => BRANCH[1],
        ReclaimHint::Cache => BRANCH[5],
        ReclaimHint::Trash => TEXT_2,
        ReclaimHint::None => TEXT_3,
    }
}

// Type scale (SF Pro via the system font; tabular figures for numbers).
pub const DISPLAY: TextStyle = TextStyle::new(30.0, 600, Font::Num);
pub const TITLE: TextStyle = TextStyle::new(17.0, 600, Font::Sans);
pub const BODY: TextStyle = TextStyle::new(13.0, 400, Font::Sans);
pub const BODY_STRONG: TextStyle = TextStyle::new(13.0, 590, Font::Sans);
pub const SMALL: TextStyle = TextStyle::new(11.5, 400, Font::Sans);
pub const SMALL_STRONG: TextStyle = TextStyle::new(11.5, 600, Font::Sans);
pub const CAPTION: TextStyle = TextStyle::new(10.5, 600, Font::Sans);
pub const NUM: TextStyle = TextStyle::new(12.0, 500, Font::Num);
pub const NUM_STRONG: TextStyle = TextStyle::new(13.0, 600, Font::Num);
pub const NUM_BIG: TextStyle = TextStyle::new(20.0, 600, Font::Num);
pub const MONO: TextStyle = TextStyle::new(11.0, 400, Font::Mono);

// Map labels: one style per nesting level, so the level reads in the type.
pub const MAP_L1: TextStyle = TextStyle::new(14.5, 650, Font::Sans);
pub const MAP_L1_NUM: TextStyle = TextStyle::new(11.5, 560, Font::Num);
pub const MAP_L2: TextStyle = TextStyle::new(12.0, 590, Font::Sans);
pub const MAP_L2_NUM: TextStyle = TextStyle::new(10.5, 500, Font::Num);
pub const MAP_L3: TextStyle = TextStyle::new(10.5, 500, Font::Sans);

/// Every style above. Their fonts are warmed up at startup so that no
/// style's first use stalls a frame; list new styles here.
pub const TEXT_STYLES: [TextStyle; 16] =
    [DISPLAY, TITLE, BODY, BODY_STRONG, SMALL, SMALL_STRONG, CAPTION, NUM, NUM_STRONG, NUM_BIG, MONO, MAP_L1, MAP_L1_NUM, MAP_L2, MAP_L2_NUM, MAP_L3];

// Metrics.
/// Default icon size (points); icons sit on a 24-unit grid.
pub const ICON: f32 = 15.0;
pub const TOPBAR_H: f32 = 52.0;
pub const FOOTER_H: f32 = 28.0;
pub const SIDEBAR_W: f32 = 318.0;
pub const INSPECTOR_W: f32 = 304.0;
pub const ROW_H: f32 = 26.0;
pub const GUTTER: f32 = 16.0;
pub const RADIUS: f32 = 10.0;
pub const RADIUS_SM: f32 = 6.0;
/// Space reserved for the macOS traffic lights in the transparent titlebar.
pub const TRAFFIC_LIGHTS_W: f32 = 78.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The baked branch inks must stay in sync with their OKLCH definition.
    #[test]
    fn branch_inks_match_oklch() {
        for (hex, h) in BRANCH.iter().zip(BRANCH_HUES) {
            let c = oklch(0.74, 0.085, h);
            for k in 0..3 {
                assert!((hex[k] - c[k]).abs() < 2.0 / 255.0, "hue {h}: {hex:?} vs {c:?}");
            }
        }
    }
}
