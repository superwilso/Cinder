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

/// The fallbacks whose parsed size is measured in tens of megabytes (Japanese, Korean, Traditional
/// Chinese). At most one of these is ever resident — see `fallback`.
const CJK_FALLBACKS: [usize; 3] = [1, 2, 3];

/// Every distinct bundled font, as (family, weight) pairs that `pick` maps to different files.
/// Order matters only for looks — the first one carrying the glyph wins — so the regular weights
/// come first and the display weights last.
const BUNDLED_ORDER: [(Family, Weight); 7] = [
    (Family::Sans, Weight::Regular),
    (Family::Mono, Weight::Regular),
    (Family::Sans, Weight::SemiBold),
    (Family::Sans, Weight::Bold),
    (Family::Mono, Weight::Bold),
    (Family::Mono, Weight::Light),
    (Family::Sans, Weight::ExtraBold),
];

/// Is `ch` in a script that fallback #`i` is actually for?
///
/// The chain is five fonts totalling ~18 MB on disk and ~250 MB parsed, and fontdue parses eagerly
/// — so "try them all and see" is not a lookup, it is an allocation storm. Each font is therefore
/// only opened for the scripts it exists to provide. Anything not listed (arrows, geometric shapes,
/// dingbats, emoji, currency, maths) resolves to `.notdef` without touching the disk.
fn fallback_covers_script(i: usize, ch: char) -> bool {
    let c = ch as u32;
    match i {
        // SST-Roman: Latin-1/Extended, Greek, Cyrillic — Sony's own proportional corporate face.
        0 => (0x0080..=0x024F).contains(&c) || (0x0370..=0x03FF).contains(&c) || (0x0400..=0x052F).contains(&c),
        // SSTJpPro: Japanese — kana, CJK punctuation, unified ideographs, compatibility, fullwidth.
        1 => (0x3000..=0x30FF).contains(&c) || (0x3400..=0x9FFF).contains(&c)
            || (0xF900..=0xFAFF).contains(&c) || (0xFF00..=0xFFEF).contains(&c),
        // NotoSansKR: Hangul jamo, compatibility jamo, extended-A/B, syllables.
        2 => (0x1100..=0x11FF).contains(&c) || (0x3130..=0x318F).contains(&c)
            || (0xA960..=0xA97F).contains(&c) || (0xAC00..=0xD7FF).contains(&c),
        // DFPGothic: Traditional Chinese — only reached for ideographs SSTJpPro did not have.
        3 => (0x3400..=0x9FFF).contains(&c) || (0xF900..=0xFAFF).contains(&c),
        // NotoSansThai.
        4 => (0x0E00..=0x0E7F).contains(&c),
        _ => false,
    }
}

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
    /// Times a lookup has gone past both bundled families into the device chain, and which
    /// characters did it. Diagnostic only (see `chain_walks`/`chain_chars`); the regression test
    /// asserts the count is zero for UI chrome and prints the set when it is not.
    chain_walks: std::cell::Cell<u32>,
    chain_chars: RefCell<std::collections::BTreeSet<char>>,
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
            chain_walks: std::cell::Cell::new(0),
            chain_chars: RefCell::new(std::collections::BTreeSet::new()),
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
        // ONE HEAVY FACE AT A TIME. fontdue parses every outline at load: measured on device,
        // SSTJpPro-Regular alone is +82 MB of RSS, and NotoSansKR and DFPGothicPW5 are the same
        // order. The device has 467 MB total with ~120 MB free, so a library holding Japanese AND
        // Korean tags would load two of them and reach the OOM killer — which for cinder-home is a
        // reboot, not an error. The first CJK-class face to load therefore wins for the session;
        // the others are marked Absent and their scripts render .notdef. A wrong glyph is a
        // cosmetic bug. A reboot is not.
        if CJK_FALLBACKS.contains(&i) {
            let taken = {
                let slots = self.fallbacks.borrow();
                CJK_FALLBACKS.iter().any(|&j| j != i && matches!(slots[j], Slot::Ready(_)))
            };
            if taken {
                self.fallbacks.borrow_mut()[i] = Slot::Absent;
                return None;
            }
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

    /// Pick the font that actually has `ch`, plus its cache id. Order: the requested font, then the
    /// OTHER BUNDLED FAMILY, then the device chain — and the device chain only for scripts the
    /// fallback in question actually covers. If nothing has it, returns the primary so the caller
    /// still gets `.notdef` metrics and the text keeps its shape.
    ///
    /// ── WHY THE ORDER AND THE GATE EXIST (2026-08-19, a device OOM) ─────────────────────────
    /// This used to go straight from "the requested font lacks it" to loading Sony's fonts. Two
    /// things made that fatal on a 467 MB device:
    ///
    ///   1. **The sibling bundled family was never tried.** JetBrains Mono carries the arrows and
    ///      geometric shapes Hanken Grotesk lacks (▸ ◁ ▷ ↕ ▶ ◀ ─ ≡ ⇒) *and* full Cyrillic. A
    ///      `Family::Sans` run containing `▸` therefore skipped a font already in memory and went
    ///      to disk.
    ///   2. **Nothing gated the chain by script.** fontdue parses every glyph outline at load:
    ///      measured on device, SSTJpPro-Regular costs **+82 MB of RSS** and the whole chain about
    ///      **250 MB**. So one character that no font covers loaded all five fonts and the kernel
    ///      killed the app:
    ///
    ///      ```text
    ///      Out of memory: Kill process 1700 (cinder-probe) score 514
    ///      Killed process 1700 (cinder-probe) total-vm:265164kB, anon-rss:251472kB
    ///      ```
    ///
    ///      In cinder-home that is not a crash, it is a REBOOT — appmgr reboots the device when its
    ///      foreground app dies. The reported symptom was "the device crashes on the 3rd page of
    ///      the welcome screens": that page, alone in the UI, draws `Settings ▸ Theme` in Sans.
    ///
    /// The gate is deliberately strict: a symbol, arrow or dingbat NEVER loads a fallback. Those
    /// are chrome, they belong in the bundled fonts, and a `.notdef` box is a cosmetic bug where
    /// loading 250 MB to look for one is a reboot.
    fn resolve(&self, fam: Family, w: Weight, ch: char) -> (u8, &Font) {
        let id = Self::font_index(fam, w);
        let primary = self.pick(fam, w);
        // Space has no outline in some fonts but is never "missing" — skip the chain for it, and
        // for ASCII generally, which is the overwhelmingly common case and always covered.
        if (ch as u32) < 0x80 || primary.lookup_glyph_index(ch) != 0 {
            return (id, primary);
        }
        for i in 0..FALLBACK_FONTS.len() {
            if !fallback_covers_script(i, ch) {
                continue;
            }
            self.chain_walks.set(self.chain_walks.get() + 1);
            self.chain_chars.borrow_mut().insert(ch);
            if let Some(f) = self.fallback(i) {
                if f.lookup_glyph_index(ch) != 0 {
                    // ids 16.. are the fallbacks, so they can't collide with `font_index`'s 0..6.
                    return (16 + i as u8, f);
                }
            }
        }
        // Last, the other bundled family (and its other weights) — already in memory, so this is a
        // lookup rather than a load. It comes AFTER the chain on purpose: Cyrillic and Greek exist
        // in JetBrains Mono, but Sony's proportional SST-Roman is the right face for a sans run,
        // and this is the tier that catches what the chain is not for — the arrows and geometric
        // shapes (▸ ◁ ▷ ↕ ▶ ◀ ─ ≡ ⇒) that only the mono face carries.
        for (f2, w2) in BUNDLED_ORDER {
            if f2 == fam && w2 == w {
                continue;
            }
            let alt = self.pick(f2, w2);
            if alt.lookup_glyph_index(ch) != 0 {
                return (Self::font_index(f2, w2), alt);
            }
        }
        (id, primary)
    }

    /// How many times a glyph lookup has reached the DEVICE font chain (i.e. past both bundled
    /// families). Zero for anything the bundled fonts cover. Tests assert on it; nothing else
    /// reads it.
    pub fn chain_walks(&self) -> u32 {
        self.chain_walks.get()
    }

    /// The distinct characters that reached the device chain.
    pub fn chain_char_list(&self) -> Vec<char> {
        self.chain_chars.borrow().iter().copied().collect()
    }

    /// The distinct characters that reached the device chain, formatted for a test failure.
    pub fn chain_chars(&self) -> String {
        self.chain_chars
            .borrow()
            .iter()
            .map(|c| format!("U+{:04X} {c:?}", *c as u32))
            .collect::<Vec<_>>()
            .join(", ")
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

/// DIAGNOSTIC hook for `cinder-probe --fontchain`: resolve `ch` through the real chain (loading
/// whatever fallback that takes) and rasterise it at a typical UI size, returning the font id that
/// answered — `u8::MAX` if nothing covered it and the primary's `.notdef` was used. Kept next to
/// `resolve` so it cannot drift from what the renderer actually does.
pub fn probe_glyph(fonts: &FontSet, ch: char) -> u8 {
    let (id, font) = fonts.resolve(Family::Sans, Weight::Regular, ch);
    let covered = font.lookup_glyph_index(ch) != 0;
    let _ = font.rasterize(ch, 16.0);
    if covered { id } else { u8::MAX }
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
