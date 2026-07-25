//! Text engine: fontdue rasteriser + bundled OFL fonts (Hanken Grotesk sans,
//! JetBrains Mono mono). embedded-graphics' built-in fonts are bitmap-only, so
//! we rasterise glyph coverage with fontdue and alpha-blend onto the Canvas.
//! `tracking` is letter-spacing in em (multiplied by px size), per the design.

use crate::canvas::Canvas;
use embedded_graphics::pixelcolor::Rgb888;
use fontdue::{Font, FontSettings, Metrics};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Family {
    Sans,
    Mono,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Weight {
    Light,
    Regular,
    SemiBold,
    Bold,
    ExtraBold,
}

/// Glyph cache key: which font (family<<3 | weight), the char, and the size quantised to 0.25px.
/// (Sizes used are a small fixed set, so the cache stays small + bounded.)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u8,
    ch: char,
    size_q: u32,
}

pub struct FontSet {
    sans_regular: Font,
    sans_semibold: Font,
    sans_bold: Font,
    sans_extrabold: Font,
    mono_light: Font,
    mono_regular: Font,
    mono_bold: Font,
    // Rasterising a glyph allocates + does real work; the UI re-draws the same glyphs every frame
    // (especially while scrolling / the visualiser animates), so we cache (metrics, coverage
    // bitmap) per glyph. Single-threaded access (render runs under the cinder-ffi mutex), so a
    // RefCell suffices. Bounded: ~ASCII × a handful of sizes × weights.
    glyph_cache: RefCell<HashMap<GlyphKey, (Metrics, std::sync::Arc<Vec<u8>>)>>,
}

impl FontSet {
    pub fn load() -> Self {
        let f = |b: &[u8]| Font::from_bytes(b, FontSettings::default()).expect("font parse");
        FontSet {
            sans_regular: f(include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf")),
            sans_semibold: f(include_bytes!("../assets/fonts/HankenGrotesk-SemiBold.ttf")),
            sans_bold: f(include_bytes!("../assets/fonts/HankenGrotesk-Bold.ttf")),
            sans_extrabold: f(include_bytes!("../assets/fonts/HankenGrotesk-ExtraBold.ttf")),
            mono_light: f(include_bytes!("../assets/fonts/JetBrainsMono-Light.ttf")),
            mono_regular: f(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
            mono_bold: f(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
            glyph_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Stable font index for the cache key (matches `pick`'s selection).
    fn font_index(fam: Family, w: Weight) -> u8 {
        match fam {
            Family::Sans => match w {
                Weight::Light | Weight::Regular => 0,
                Weight::SemiBold => 1,
                Weight::Bold => 2,
                Weight::ExtraBold => 3,
            },
            Family::Mono => match w {
                Weight::Light => 4,
                Weight::Regular | Weight::SemiBold => 5,
                Weight::Bold | Weight::ExtraBold => 6,
            },
        }
    }

    /// Cached rasterisation: returns the glyph metrics + an Rc to its coverage bitmap. Misses
    /// rasterise once and insert; hits are a hashmap lookup (no allocation/raster work).
    fn glyph(&self, fam: Family, w: Weight, ch: char, size: f32) -> (Metrics, std::sync::Arc<Vec<u8>>) {
        let key = GlyphKey { font: Self::font_index(fam, w), ch, size_q: (size * 4.0) as u32 };
        if let Some(hit) = self.glyph_cache.borrow().get(&key) {
            return (hit.0, hit.1.clone());
        }
        let (m, bitmap) = self.pick(fam, w).rasterize(ch, size);
        let rc = std::sync::Arc::new(bitmap);
        self.glyph_cache.borrow_mut().insert(key, (m, rc.clone()));
        (m, rc)
    }

    fn pick(&self, fam: Family, w: Weight) -> &Font {
        match fam {
            Family::Sans => match w {
                Weight::Light | Weight::Regular => &self.sans_regular,
                Weight::SemiBold => &self.sans_semibold,
                Weight::Bold => &self.sans_bold,
                Weight::ExtraBold => &self.sans_extrabold,
            },
            Family::Mono => match w {
                Weight::Light => &self.mono_light,
                Weight::Regular | Weight::SemiBold => &self.mono_regular,
                Weight::Bold | Weight::ExtraBold => &self.mono_bold,
            },
        }
    }
}

pub struct TextStyle {
    pub fam: Family,
    pub weight: Weight,
    pub size: f32,
    pub color: Rgb888,
    pub tracking: f32, // em
}

/// Pixel width of `s` rendered with `st` (advances + tracking).
pub fn measure(fonts: &FontSet, s: &str, st: &TextStyle) -> f32 {
    let font = fonts.pick(st.fam, st.weight);
    let track = st.tracking * st.size;
    s.chars()
        .map(|ch| font.metrics(ch, st.size).advance_width + track)
        .sum()
}

/// Draw `s` with its baseline at (`x`, `baseline`). Returns the pen x after.
pub fn draw(canvas: &mut Canvas, fonts: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle) -> f32 {
    let track = st.tracking * st.size;
    let mut pen = x;
    for ch in s.chars() {
        let (m, bitmap) = fonts.glyph(st.fam, st.weight, ch, st.size); // cached rasterise
        let gx0 = (pen + m.xmin as f32).round() as i32;
        // fontdue: bitmap top is `ymin + height` above the baseline.
        let gy0 = (baseline - (m.height as f32 + m.ymin as f32)).round() as i32;
        for gy in 0..m.height {
            for gx in 0..m.width {
                let a = bitmap[gy * m.width + gx];
                if a > 0 {
                    canvas.blend(gx0 + gx as i32, gy0 + gy as i32, st.color, a);
                }
            }
        }
        pen += m.advance_width + track;
    }
    pen
}
