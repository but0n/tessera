//! Text shaping cache and glyphon renderers (one per layer).

use std::collections::HashMap;

use glyphon::cosmic_text::{FeatureTag, FontFeatures};
use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, CustomGlyph, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
    Weight, Wrap,
};

use super::LAYERS;
use crate::ui::{Frame, Rect};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    Sans,
    /// Sans with tabular figures: numbers that line up and don't jitter.
    Num,
    Mono,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: u16,
    pub font: Font,
}

impl TextStyle {
    pub const fn new(size: f32, weight: u16, font: Font) -> Self {
        TextStyle { size, weight, font }
    }
    pub fn line_height(&self) -> f32 {
        (self.size * 1.3).round()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    size: u32,
    weight: u16,
    font: Font,
}

impl Key {
    fn new(text: &str, s: &TextStyle) -> Key {
        Key { text: text.to_owned(), size: (s.size * 100.0) as u32, weight: s.weight, font: s.font }
    }
}

struct Shaped {
    buffer: Buffer,
    width: f32,
    last_used: u64,
}

pub struct TextItem {
    pub text: String,
    pub style: TextStyle,
    pub pos: (f32, f32),
    pub color: [f32; 4],
    pub clip: Rect,
}

/// A vector icon drawn through the text atlas (see [`super::icons`]).
pub struct IconItem {
    pub icon: super::icons::Icon,
    pub pos: (f32, f32),
    pub size: f32,
    pub color: [f32; 4],
    pub clip: Rect,
}

/// The system's fonts, loaded and warmed up. Building this is pure CPU work
/// (no GPU), so it can run on a background thread while the window and the
/// device are being created.
pub struct Fonts {
    system: FontSystem,
    sans: String,
    mono: String,
}

impl Fonts {
    /// Loads the font database and shapes a line in each of `styles`. A
    /// font is only instantiated the first time one of its weights is
    /// shaped, which takes tens of milliseconds; doing it here keeps that
    /// out of the first frame each style appears in.
    pub fn load(styles: &[TextStyle]) -> Fonts {
        let mut db = glyphon::fontdb::Database::new();
        db.load_system_fonts();
        let family_of = |file: &str| {
            db.faces()
                .find(|f| matches!(&f.source, glyphon::fontdb::Source::File(p) | glyphon::fontdb::Source::SharedFile(p, _) if p.file_name().is_some_and(|n| n == file)))
                .map(|f| f.families[0].0.clone())
        };
        let sans = family_of("SFNS.ttf").unwrap_or_else(|| "Helvetica Neue".into());
        let mono = family_of("SFNSMono.ttf").unwrap_or_else(|| "Menlo".into());
        db.set_sans_serif_family(sans.clone());
        db.set_monospace_family(mono.clone());
        let locale = std::env::var("LANG").ok().and_then(|l| l.split('.').next().map(|s| s.replace('_', "-"))).unwrap_or_else(|| "en-US".into());
        let mut fonts = Fonts { system: FontSystem::new_with_locale_and_db(locale, db), sans, mono };
        for style in styles {
            // Letters, figures and the symbols the chrome is set in.
            fonts.shape("Ag 0.5 · … ⌘ ⌥ ↑↓←→", style, 1.0);
        }
        fonts
    }

    /// `text` set on one line in `style`, at `scale` physical pixels per point.
    fn shape(&mut self, text: &str, style: &TextStyle, scale: f32) -> Buffer {
        let mut buffer = Buffer::new(&mut self.system, Metrics::new(style.size * scale, style.line_height() * scale));
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, None);
        let family = match style.font {
            Font::Mono => Family::Name(&self.mono),
            _ => Family::Name(&self.sans),
        };
        let mut attrs = Attrs::new().family(family).weight(Weight(style.weight));
        if style.font == Font::Num {
            let mut f = FontFeatures::new();
            f.enable(FeatureTag::new(b"tnum"));
            attrs = attrs.font_features(f);
        }
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.system, false);
        buffer
    }
}

pub struct TextSystem {
    fonts: Fonts,
    /// Empty buffer hosting icon glyphs.
    blank: Buffer,
    swash: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderers: Vec<TextRenderer>,
    cache: HashMap<Key, Shaped>,
    elided: HashMap<(Key, u32), String>,
    scale: f32,
    frame: u64,
}

impl TextSystem {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, mut fonts: Fonts) -> Self {
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::with_color_mode(device, queue, &cache, format, ColorMode::Web);
        let renderers = (0..LAYERS).map(|_| TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None)).collect();
        let blank = Buffer::new(&mut fonts.system, Metrics::new(1.0, 1.0));
        TextSystem { fonts, blank, swash: SwashCache::new(), atlas, viewport, renderers, cache: HashMap::new(), elided: HashMap::new(), scale: 1.0, frame: 0 }
    }

    pub fn begin_frame(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.cache.clear();
            self.elided.clear();
            self.scale = scale;
        }
        self.frame += 1;
        if self.frame.is_multiple_of(120) {
            let f = self.frame;
            self.cache.retain(|_, s| f - s.last_used < 240);
            if self.elided.len() > 4096 {
                self.elided.clear();
            }
        }
    }

    fn shaped(&mut self, text: &str, style: &TextStyle) -> &mut Shaped {
        let key = Key::new(text, style);
        let frame = self.frame;
        let scale = self.scale;
        if !self.cache.contains_key(&key) {
            let buffer = self.fonts.shape(text, style, scale);
            let width = buffer.layout_runs().map(|r| r.line_w).fold(0.0, f32::max) / scale;
            self.cache.insert(key.clone(), Shaped { buffer, width, last_used: frame });
        }
        let s = self.cache.get_mut(&key).unwrap();
        s.last_used = frame;
        s
    }

    /// Logical width of `text` set in `style`.
    pub fn measure(&mut self, text: &str, style: &TextStyle) -> f32 {
        self.shaped(text, style).width
    }

    /// `text` shortened with a trailing ellipsis to fit `max_w`.
    pub fn elide(&mut self, text: &str, style: &TextStyle, max_w: f32) -> String {
        if self.measure(text, style) <= max_w {
            return text.to_owned();
        }
        let key = (Key::new(text, style), max_w as u32);
        if let Some(s) = self.elided.get(&key) {
            return s.clone();
        }
        let ell = self.measure("…", style);
        let scale = self.scale;
        let budget = (max_w - ell).max(0.0) * scale;
        let shaped = self.shaped(text, style);
        let mut cut = 0;
        for run in shaped.buffer.layout_runs() {
            for g in run.glyphs {
                if g.x + g.w <= budget && !run.rtl {
                    cut = cut.max(g.end);
                }
            }
        }
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let out = format!("{}…", text[..cut].trim_end());
        self.elided.insert(key, out.clone());
        out
    }

    /// `text` shortened in the middle to fit `max_w`, keeping its end (where
    /// file names differ: take numbers, extensions). `None` if fewer than
    /// `min_chars` characters would survive.
    pub fn elide_middle(&mut self, text: &str, style: &TextStyle, max_w: f32, min_chars: usize) -> Option<String> {
        if self.measure(text, style) <= max_w {
            return Some(text.to_owned());
        }
        let chars: Vec<char> = text.chars().collect();
        let build = |k: usize| {
            let tail = k * 2 / 5;
            let head = k - tail;
            let mut s: String = chars[..head].iter().collect::<String>().trim_end().to_owned();
            s.push('…');
            s.extend(&chars[chars.len() - tail..]);
            s
        };
        // Largest k (characters kept) that fits.
        let (mut lo, mut hi) = (0usize, chars.len().saturating_sub(1));
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.measure(&build(mid), style) <= max_w { lo = mid } else { hi = mid - 1 }
        }
        (lo >= min_chars).then(|| build(lo))
    }

    /// Makes sure everything referenced by `frame` is shaped (call before prepare).
    pub fn ensure(&mut self, text: &str, style: &TextStyle) {
        self.shaped(text, style);
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame, phys: (u32, u32)) {
        for l in &frame.layers {
            for t in &l.texts {
                self.ensure(&t.text, &t.style);
            }
        }
        self.viewport.update(queue, Resolution { width: phys.0, height: phys.1 });
        let scale = self.scale;
        let TextSystem { fonts: Fonts { system: fonts, .. }, blank, swash, atlas, viewport, renderers, cache, .. } = self;
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let color = |c: [f32; 4]| Color::rgba(to8(c[0]), to8(c[1]), to8(c[2]), to8(c[3]));
        let bounds = |r: &Rect| TextBounds {
            left: (r.x * scale).floor() as i32,
            top: (r.y * scale).floor() as i32,
            right: ((r.x + r.w) * scale).ceil() as i32,
            bottom: ((r.y + r.h) * scale).ceil() as i32,
        };
        for (li, layer) in frame.layers.iter().enumerate() {
            // One glyph per area, since every icon carries its own clip.
            let glyphs: Vec<[CustomGlyph; 1]> = layer
                .icons
                .iter()
                .map(|i| {
                    // Snap the box to whole physical pixels so strokes stay crisp.
                    let px = (i.size * scale).round() / scale;
                    [CustomGlyph {
                        id: i.icon as u16,
                        left: (i.pos.0 * scale).round() / scale,
                        top: (i.pos.1 * scale).round() / scale,
                        width: px,
                        height: px,
                        color: Some(color(i.color)),
                        snap_to_physical_pixel: true,
                        metadata: 0,
                    }]
                })
                .collect();
            let icon_areas = layer.icons.iter().zip(&glyphs).map(|(i, g)| TextArea {
                buffer: blank,
                left: 0.0,
                top: 0.0,
                scale,
                bounds: bounds(&i.clip),
                default_color: color(i.color),
                custom_glyphs: g,
            });
            let areas: Vec<TextArea> = layer
                .texts
                .iter()
                .filter_map(|t| {
                    let shaped = cache.get(&Key::new(&t.text, &t.style))?;
                    Some(TextArea {
                        buffer: &shaped.buffer,
                        left: (t.pos.0 * scale).round(),
                        top: (t.pos.1 * scale).round(),
                        scale: 1.0,
                        bounds: bounds(&t.clip),
                        default_color: color(t.color),
                        custom_glyphs: &[],
                    })
                })
                .chain(icon_areas)
                .collect();
            let _ = renderers[li].prepare_with_custom(device, queue, fonts, atlas, viewport, areas, swash, super::icons::rasterize);
        }
    }

    pub fn render(&self, layer: usize, pass: &mut wgpu::RenderPass<'_>) {
        let _ = self.renderers[layer].render(&self.atlas, &self.viewport, pass);
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}
