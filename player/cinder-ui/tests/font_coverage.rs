//! Font coverage guards.
//!
//! Two failure modes this locks down, both of which render as `.notdef` boxes on the device and
//! are invisible in a host render unless you happen to test the right string:
//!
//! 1. **A UI literal using a codepoint the chosen family lacks.** The bundled fonts are
//!    Latin-focused; Hanken Grotesk has no `▶` (U+25B6), for one. Adding `"▶ NOW"` to a
//!    `Family::Sans` label would silently draw a box.
//! 2. **Library metadata in a non-Latin script.** Hanken has no Cyrillic, Greek, CJK or Thai at
//!    all — so on a Walkman, a Japanese or Russian album title is a row of boxes. That is what
//!    the device-font fallback chain in `text.rs` exists to fix.

use cinder_ui::text::{measure, Family, FontSet, TextStyle, Weight};
use embedded_graphics::pixelcolor::Rgb888;
use fontdue::{Font, FontSettings};

/// Every non-ASCII codepoint that appears in a string literal anywhere in `cinder-ui/src`.
/// Regenerate after adding one:
///   grep -oP '"[^"]*"' src/*.rs | grep -oP '[^\x00-\x7F]' | sort -u
const UI_CHARS: &[char] = &[
    '\u{00B7}', // · separator, used everywhere
    '\u{00D7}', // × remove/close
    '\u{2014}', // — em dash
    '\u{2026}', // … ellipsis (widgets::fit truncation)
    '\u{203A}', // › disclosure
    '\u{2192}', // → signal-path arrow
    '\u{2212}', // − true minus (FM tuning)
    '\u{25B6}', // ▶ now-playing marker
];

/// `▶` exists in JetBrains Mono but NOT in any Hanken Grotesk weight. It is drawn in
/// `Family::Mono` (`up_next.rs`), which is why it works. Keep it that way — if you need a
/// play triangle in Sans, use the vector `icons::play`, not this codepoint.
const MONO_ONLY: &[char] = &['\u{25B6}'];

fn load(name: &str) -> Font {
    let b = std::fs::read(format!("assets/fonts/{name}")).expect("bundled font present");
    Font::from_bytes(b, FontSettings::default()).expect("bundled font parses")
}

const SANS: &[&str] = &[
    "HankenGrotesk-Regular.ttf",
    "HankenGrotesk-SemiBold.ttf",
    "HankenGrotesk-Bold.ttf",
    "HankenGrotesk-ExtraBold.ttf",
];
const MONO: &[&str] = &[
    "JetBrainsMono-Light.ttf",
    "JetBrainsMono-Regular.ttf",
    "JetBrainsMono-Bold.ttf",
];

#[test]
fn every_ui_char_exists_in_the_family_that_draws_it() {
    for name in MONO {
        let f = load(name);
        for &c in UI_CHARS {
            assert!(f.lookup_glyph_index(c) != 0, "{name} lacks U+{:04X} {c:?}", c as u32);
        }
    }
    for name in SANS {
        let f = load(name);
        for &c in UI_CHARS.iter().filter(|c| !MONO_ONLY.contains(c)) {
            assert!(f.lookup_glyph_index(c) != 0, "{name} lacks U+{:04X} {c:?}", c as u32);
        }
    }
    // Pin the exception itself, so deleting the `▶` literal (or gaining Hanken coverage) shows up
    // here as a stale rule rather than quietly widening what's allowed.
    for name in SANS {
        assert_eq!(load(name).lookup_glyph_index('\u{25B6}'), 0, "{name} gained U+25B6 — drop MONO_ONLY");
    }
}

fn sty(size: f32) -> TextStyle {
    TextStyle { fam: Family::Sans, weight: Weight::Regular, size, color: Rgb888::new(0, 0, 0), tracking: 0.0 }
}

/// The extracted stock rootfs stands in for the device's `/system`.
const ROOTFS_FONTS: &str = "../../analysis/binwalk/6.bin/_6.bin.extracted/ext-root/vendor/sony/lib/fonts";

/// Both halves live in ONE test on purpose: `CINDER_FONT_DIR` is process-global, and cargo runs
/// tests in parallel threads, so splitting them lets one test's `set_var` land in the middle of
/// another's `FontSet::load()` and silently invalidate the comparison.
#[test]
fn device_font_fallback() {
    // Japanese, Traditional Chinese, Korean, Russian, Thai — all realistic tag content.
    let samples = ["君の名は", "周杰倫", "방탄소년단", "Чайковский", "ลาบ"];

    // Without the chain (host default: no /system), everything is .notdef. Layout survives —
    // advances stay non-zero — but the glyphs are wrong, which is the bug.
    std::env::set_var("CINDER_FONT_DIR", "/nonexistent-font-dir");
    let bare = FontSet::load();
    let bare_widths: Vec<f32> = samples.iter().map(|s| measure(&bare, s, &sty(16.0))).collect();
    for (s, w) in samples.iter().zip(&bare_widths) {
        assert!(*w > 0.0, "{s}: zero width would break every truncation/centring calc");
    }

    // ASCII must never touch the chain — measured here while it is provably unavailable.
    let ascii = "Hello World 123";
    let ascii_bare = measure(&bare, ascii, &sty(16.0));

    if !std::path::Path::new(ROOTFS_FONTS).is_dir() {
        eprintln!("skip: extracted rootfs fonts absent (run `make phase2`)");
        return;
    }
    std::env::set_var("CINDER_FONT_DIR", ROOTFS_FONTS);

    // Each script resolves — but each in its OWN FontSet. At most one CJK-class face (Japanese,
    // Korean, Traditional Chinese) is resident per FontSet since 2026-08-19: fontdue parses every
    // outline at load, one of those faces costs +82 MB of RSS on device, and loading two reaches
    // the OOM killer — which for cinder-home means appmgr reboots the device. See
    // `CJK_FALLBACKS` in text.rs.
    for (s, bare_w) in samples.iter().zip(&bare_widths) {
        let one = FontSet::load();
        let w = measure(&one, s, &sty(16.0));
        // The width must CHANGE, not necessarily grow. It grows for CJK/Thai, where .notdef is
        // roughly half-width and the real glyph is full-width. It SHRINKS for Cyrillic: `resolve`
        // now falls back to the other bundled family when the device chain has nothing for a
        // codepoint, and JetBrains Mono carries Cyrillic — so "bare" is real monospaced Cyrillic
        // (16 px/char) and the chain replaces it with Sony's proportional SST-Roman, which is
        // narrower. Either direction proves the chain engaged.
        assert!(
            (w - *bare_w).abs() > 0.5,
            "{s}: no fallback engaged (width {w} vs bare {bare_w})"
        );
    }

    // The cap itself: one FontSet, two different CJK scripts. The first wins; the second keeps
    // .notdef metrics rather than pulling a second 80 MB face into a 467 MB device.
    let shared = FontSet::load();
    let jp = measure(&shared, samples[0], &sty(16.0));
    let kr = measure(&shared, samples[2], &sty(16.0));
    assert!(
        (jp - bare_widths[0]).abs() > 0.5,
        "the FIRST CJK script asked for must resolve (Japanese: {jp} vs bare {})",
        bare_widths[0]
    );
    assert!(
        (kr - bare_widths[2]).abs() < 0.5,
        "a SECOND CJK-class face was loaded into the same FontSet (Korean: {kr} vs bare {}) — \
         that is the pair that OOM-kills the app on device",
        bare_widths[2]
    );

    let full = FontSet::load();
    let _ = measure(&full, samples[3], &sty(16.0));   // Cyrillic -> SST-Roman, the cheap face
    assert!(
        full.chain_walks() > 0,
        "the device chain was never consulted for {:?} — the script gate in \
         `fallback_covers_script` is too tight",
        samples[3]
    );
    assert_eq!(
        ascii_bare,
        measure(&full, ascii, &sty(16.0)),
        "the fallback chain changed ASCII metrics — it must not"
    );
}
