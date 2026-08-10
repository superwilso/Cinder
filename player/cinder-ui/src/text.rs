//! Text engine: fontdue rasteriser + bundled OFL fonts (Hanken Grotesk sans,
//! JetBrains Mono mono). embedded-graphics' built-in fonts are bitmap-only, so
//! we rasterise glyph coverage with fontdue and alpha-blend onto the Canvas.
//! `tracking` is letter-spacing in em (multiplied by px size), per the design.

use crate::canvas::Canvas;
use embedded_graphics::pixelcolor::Rgb888;
use fontdue::{Font, FontSettings, Metrics};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

// ── UI text scale (Settings ▸ UI scale) ─────────────────────────────────────────────────────
// One global multiplier applied to every `TextStyle::size`, by BOTH `measure` and `draw`. Doing
// it here (rather than scaling the call sites) is what keeps truncation, centring and
// right-alignment exact at any scale: the two functions can never disagree about a glyph's width.
// Row heights and hit-test geometry are deliberately NOT scaled — the panel is 480×800 and the
// tap targets are tuned to it, so scaling layout would silently desync every hit test. What the
// slider gives you is bigger (or denser) TYPE inside the same, already-correct rows.

/// Scale steps, in percent. Discrete on purpose: the glyph cache is keyed on rasterised pixel
/// size, so a continuous scale would grow it without bound. Seven stops still read as a slider.
pub const SCALE_STEPS: [u32; 7] = [80, 90, 100, 110, 120, 130, 140];
/// Index into `SCALE_STEPS` for the native (100%) size — the default.
pub const SCALE_DEFAULT_IDX: usize = 2;

static SCALE_PCT: AtomicU32 = AtomicU32::new(100);

/// Set the global UI text scale in percent (clamped to the `SCALE_STEPS` range).
pub fn set_scale_pct(pct: u32) {
    let lo = SCALE_STEPS[0];
    let hi = SCALE_STEPS[SCALE_STEPS.len() - 1];
    SCALE_PCT.store(pct.clamp(lo, hi), Ordering::Relaxed);
}

/// The global UI text scale in percent (100 = native).
pub fn scale_pct() -> u32 {
    SCALE_PCT.load(Ordering::Relaxed)
}

/// Nearest `SCALE_STEPS` index for the current scale (for the Settings slider knob).
pub fn scale_idx() -> usize {
    let cur = scale_pct();
    SCALE_STEPS
        .iter()
        .enumerate()
        .min_by_key(|(_, &s)| s.abs_diff(cur))
        .map(|(i, _)| i)
        .unwrap_or(SCALE_DEFAULT_IDX)
}

/// Apply `SCALE_STEPS[idx]` (index clamped into range).
pub fn set_scale_idx(idx: usize) {
    set_scale_pct(SCALE_STEPS[idx.min(SCALE_STEPS.len() - 1)]);
}

/// Test-only serialisation for the scale.
///
/// `SCALE_PCT` is process-global — there is exactly one UI per process on the device, so that is
/// the right shape for production. But `cargo test` runs tests on several threads, so a test that
/// sets 140% will corrupt any concurrent test that measures or renders TEXT, wherever it lives.
/// (Observed: `settings::tests::scrolling_never_paints_over_the_header` passed alone and failed in
/// the full run.) Every test that changes the scale — or depends on it, which means every test
/// that renders — takes this ONE crate-wide lock, and the guard restores 100% on the way out even
/// if the test panics.
#[cfg(test)]
pub fn scale_guard() -> ScaleGuard {
    static SCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let g = ScaleGuard(SCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner()));
    set_scale_pct(100);
    g
}

#[cfg(test)]
pub struct ScaleGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl Drop for ScaleGuard {
    fn drop(&mut self) {
        set_scale_pct(100);
    }
}

#[inline]
fn scaled(size: f32) -> f32 {
    size * scale_pct() as f32 / 100.0
}

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

/// Device font fallback chain, tried in order for any codepoint the bundled fonts lack.
///
/// Hanken Grotesk and JetBrains Mono are Latin-focused: Hanken has **no Cyrillic, Greek, CJK or
/// Thai at all**, and neither has CJK. On a Walkman that matters — a library with Japanese,
/// Chinese, Korean, Russian or Thai tags renders every one of those characters as a `.notdef`
/// box. (Advance widths stay non-zero, so layout doesn't collapse; it's purely wrong glyphs.)
///
/// Sony already ships full coverage on the device for its own Qt UI, so we borrow it rather than
/// bloating the binary. Nothing is redistributed — these are read at runtime from the device's
/// own `/system`, and the paths simply don't exist on a host, where the chain is a no-op.
/// Coverage verified against the extracted rootfs (see `tests/font_coverage.rs`):
///   SST-Roman          Cyrillic, Greek, Latin-ext — Sony's own corporate face  (87 KB)
///   SSTJpPro-Regular   JP kana+kanji, Hans, ♪♥ symbols                         (2.9 MB)
///   NotoSansKR-Regular Hangul (+ JP/Hans)                                      (4.5 MB)
///   DFPGothicPW5       Traditional Chinese / BIG5-HK                           (10 MB)
///   NotoSansThai       Thai                                                    (34 KB)
///
/// **Order is not arbitrary.** `SSTJpPro` also covers Cyrillic and Greek, but as *full-width*
/// glyphs (16 px advance at 16 px, vs ~9.6 proportional) — a Russian title resolved to it renders
/// v e r y   s p a c e d   o u t. `SST-Roman` is proportional, tiny, and is the typeface Sony's
/// own UI uses, so it goes first and the CJK faces only ever get scripts they are actually right
/// for. Each is loaded ONLY when a glyph misses, so a Latin-only library pays for none of it.
///
/// Note `SSTUI-Roman.ttf` is deliberately absent: fontdue cannot parse it (see
/// `analysis/RE_sony_fonts.md`). `SSTUI-Bold.ttf` parses fine — the pair are not interchangeable.
const FALLBACK_FONTS: &[&str] = &[
    "SST-Roman.otf",
    "SSTJpPro-Regular.otf",
    "NotoSansKR-Regular.otf",
    "DFPGothicPW5-BIG5HK-SONY-20140613.ttf",
    "NotoSansThai-Regular.ttf",
];

/// Where those live on the device. Overridable so the host harness and tests can point at the
/// extracted rootfs and render real non-Latin text without a device.
const FALLBACK_DIR: &str = "/system/vendor/sony/lib/fonts";

/// Lazily-resolved fallback slot. Fonts live for the process lifetime (leaked once), which is
/// what lets `resolve` hand back a plain reference out of a `RefCell`.
enum Slot {
    Untried,
    Absent,
    Ready(&'static Font),
}

pub struct FontSet {
    sans_regular: Font,
    sans_semibold: Font,
    sans_bold: Font,
    sans_extrabold: Font,
    mono_light: Font,
    mono_regular: Font,
    mono_bold: Font,
    fallbacks: RefCell<Vec<Slot>>,
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
            fallbacks: RefCell::new((0..FALLBACK_FONTS.len()).map(|_| Slot::Untried).collect()),
            glyph_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Fallback #`i`, loading it on first demand. A missing/unparseable file is remembered as
    /// `Absent` so a device without Sony's fonts costs exactly one failed `read` per font, ever.
    fn fallback(&self, i: usize) -> Option<&'static Font> {
        if let Slot::Ready(f) = self.fallbacks.borrow()[i] {
            return Some(f);
        }
        if matches!(self.fallbacks.borrow()[i], Slot::Absent) {
            return None;
        }
        let dir = std::env::var("CINDER_FONT_DIR").unwrap_or_else(|_| FALLBACK_DIR.to_string());
        let loaded = std::fs::read(format!("{dir}/{}", FALLBACK_FONTS[i]))
            .ok()
            .and_then(|b| Font::from_bytes(b, FontSettings::default()).ok())
            // Leak: at most 4 fonts, loaded once, alive until exit anyway. Buys a 'static ref so
            // `resolve` can return it alongside a `&self`-bound primary font.
            .map(|f| &*Box::leak(Box::new(f)));
        self.fallbacks.borrow_mut()[i] = match loaded {
            Some(f) => Slot::Ready(f),
            None => Slot::Absent,
        };
        loaded
    }

    /// Pick the font that actually has `ch`, plus its cache id. Falls back through the device
    /// chain when the bundled font lacks the codepoint; if nothing has it, returns the primary so
    /// the caller still gets `.notdef` metrics and the text keeps its shape.
    fn resolve(&self, fam: Family, w: Weight, ch: char) -> (u8, &Font) {
        let id = Self::font_index(fam, w);
        let primary = self.pick(fam, w);
        // Space has no outline in some fonts but is never "missing" — skip the chain for it, and
        // for ASCII generally, which is the overwhelmingly common case and always covered.
        if (ch as u32) < 0x80 || primary.lookup_glyph_index(ch) != 0 {
            return (id, primary);
        }
        for i in 0..FALLBACK_FONTS.len() {
            if let Some(f) = self.fallback(i) {
                if f.lookup_glyph_index(ch) != 0 {
                    // ids 16.. are the fallbacks, so they can't collide with `font_index`'s 0..6.
                    return (16 + i as u8, f);
                }
            }
        }
        (id, primary)
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
        let (font_id, font) = self.resolve(fam, w, ch);
        let key = GlyphKey { font: font_id, ch, size_q: (size * 4.0) as u32 };
        if let Some(hit) = self.glyph_cache.borrow().get(&key) {
            return (hit.0, hit.1.clone());
        }
        let (m, bitmap) = font.rasterize(ch, size);
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
/// MUST resolve per-char exactly as `draw` does. A fallback glyph is typically full-width where
/// the primary's `.notdef` is half — measuring against the primary while drawing from the
/// fallback would silently desync every truncation, centring and right-alignment on the screen.
pub fn measure(fonts: &FontSet, s: &str, st: &TextStyle) -> f32 {
    let size = scaled(st.size);
    let track = st.tracking * size;
    s.chars()
        .map(|ch| {
            let (_, font) = fonts.resolve(st.fam, st.weight, ch);
            font.metrics(ch, size).advance_width + track
        })
        .sum()
}

/// Draw `s` with its baseline at (`x`, `baseline`). Returns the pen x after.
pub fn draw(canvas: &mut Canvas, fonts: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle) -> f32 {
    let size = scaled(st.size);
    let track = st.tracking * size;
    let mut pen = x;
    for ch in s.chars() {
        let (m, bitmap) = fonts.glyph(st.fam, st.weight, ch, size); // cached rasterise
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
