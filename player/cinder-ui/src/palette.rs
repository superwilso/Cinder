//! Palettes: Cinder's colours as a text file instead of compiled-in constants.
//!
//! A palette is the six neutral colours for day and for night, plus — only if it needs one — an
//! accent of its own. It is the first, data-only layer of a swappable UI (`docs/PLAN_skins.md`):
//! anyone can write one in a text editor, copy it into `cinder_palettes/` on the player, and pick
//! it under Settings ▸ Palette. It changes colours and nothing else. Layout is what skins are for.
//!
//! ```text
//! # slate.palette — every key is optional and falls back to Cinder's own value
//! name      = Slate
//! day.bg    = #0e1116
//! night.ink = #8a93a0
//! ```
//!
//! **Night values are written before the night dim**, the way `theme.rs` writes the built-in ones:
//! night mode scales them by `Theme::NIGHT_DIM_PCT` on the way to the panel. Cinder's numbers copied
//! into a file therefore reproduce Cinder exactly — `palettes/cinder.palette` is that file, and a
//! test holds it to it.
//!
//! **A palette that would be hard to read is refused**, with the reason logged, instead of loaded.
//! This device has one screen and a finger: a palette that puts text on a background it cannot be
//! read against leaves nothing to read your way back out with. The rules are contrast floors set a
//! margin below what Cinder itself measures (see [`DAY`], [`NIGHT`] and the calibration test), so
//! they turn away the unreadable and nothing Cinder already does.

use crate::theme::{Accent, AccentTokens, Neutrals, Theme, Tokens, CINDER};
use embedded_graphics::pixelcolor::{Rgb888, RgbColor};

/// The built-in palette's id — what the settings file stores when no palette file is chosen. A
/// file may not use it: the built-in is always present and always first, so a file of that name
/// could only ever be a duplicate that hides itself.
pub const BUILTIN_ID: &str = "cinder";
/// The built-in palette's name on the Settings row.
pub const BUILTIN_NAME: &str = "Cinder";
/// The folder palettes are read from, next to the settings file (so `/contents/cinder_palettes`).
pub const DIR_NAME: &str = "cinder_palettes";
/// File extension, compared case-insensitively — FAT keeps whatever case the PC wrote.
pub const EXTENSION: &str = "palette";
/// A palette is a few hundred bytes. Anything past this is not one, and the folder is read on the
/// render thread.
pub const MAX_BYTES: u64 = 16 * 1024;
/// How many palettes load. Each one is a tap to cycle past, on a device with no keyboard.
pub const MAX_FILES: usize = 32;
/// Longest display name, in characters. Fits the Settings value column at the largest UI scale.
pub const MAX_NAME: usize = 16;
/// Longest id (the file name without `.palette`).
pub const MAX_ID: usize = 32;

/// The neutral keys, in `Neutrals` field order. Each is written `day.<key>` and `night.<key>`.
pub const NEUTRAL_KEYS: [&str; 6] = ["bg", "panel", "line", "ink", "dim", "faint"];
/// The six accent keys: all of them, or none.
pub const ACCENT_KEYS: [&str; 6] = [
    "day.accent",
    "day.accent_ink",
    "day.row_select",
    "night.accent",
    "night.accent_ink",
    "night.row_select",
];

/// A loaded palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// The file name without the extension, lowercased: `Slate.palette` is `slate`. This is what
    /// the settings file stores.
    pub id: String,
    /// What Settings shows: the file's `name =`, or the id when it has none.
    pub name: String,
    pub tokens: Tokens,
}

impl Palette {
    /// Cinder's own palette, as a `Palette`.
    pub fn builtin() -> Palette {
        Palette { id: BUILTIN_ID.to_string(), name: BUILTIN_NAME.to_string(), tokens: CINDER }
    }

    /// Parse and check one palette file. `id` is the file name without its extension.
    ///
    /// Every problem is reported, not just the first: someone fixing a palette by copying it back
    /// and forth over USB should not have to discover its errors one round trip at a time.
    pub fn parse(id: &str, body: &str) -> Result<Palette, Vec<String>> {
        let mut errs = Vec::new();
        if !valid_id(id) {
            errs.push(format!(
                "`{id}` cannot be a palette name: use lowercase letters, digits, - and _ (at most {MAX_ID})"
            ));
        } else if id == BUILTIN_ID {
            errs.push(format!("`{BUILTIN_ID}` is the built-in palette — rename the file"));
        }
        let mut tokens = CINDER;
        let mut name = None;
        let mut seen: Vec<&str> = Vec::new();
        let mut accent: [Option<u32>; 6] = [None; 6];
        for (i, raw) in body.lines().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                errs.push(format!("line {n}: expected `key = value`"));
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            if seen.contains(&k) {
                errs.push(format!("line {n}: `{k}` is set twice"));
                continue;
            }
            seen.push(k);
            if k == "name" {
                let len = v.chars().count();
                if len == 0 || len > MAX_NAME || v.chars().any(char::is_control) {
                    errs.push(format!("line {n}: `name` must be 1 to {MAX_NAME} characters"));
                } else {
                    name = Some(v.to_string());
                }
                continue;
            }
            let Some(slot) = key_slot(k) else {
                errs.push(format!("line {n}: unknown key `{k}`"));
                continue;
            };
            let Some(colour) = parse_colour(v) else {
                errs.push(format!("line {n}: `{k}` wants a colour like #1a2b3c, got `{v}`"));
                continue;
            };
            match slot {
                Slot::Neutral { night, field } => {
                    let set = if night { &mut tokens.night } else { &mut tokens.day };
                    *neutral_mut(set, field) = colour;
                }
                Slot::Accent(i) => accent[i] = Some(colour),
            }
        }
        match accent {
            [Some(da), Some(di), Some(ds), Some(na), Some(ni), Some(ns)] => {
                tokens.accent = Some((
                    AccentTokens { acc: da, acc_ink: di, row_sel: ds },
                    AccentTokens { acc: na, acc_ink: ni, row_sel: ns },
                ));
            }
            [None, None, None, None, None, None] => {}
            _ => {
                let missing: Vec<&str> = ACCENT_KEYS
                    .iter()
                    .zip(accent.iter())
                    .filter(|(_, v)| v.is_none())
                    .map(|(k, _)| *k)
                    .collect();
                errs.push(format!(
                    "an accent of its own needs all six accent keys — missing {}",
                    missing.join(", ")
                ));
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        let problems = problems(&tokens);
        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(Palette { id: id.to_string(), name: name.unwrap_or_else(|| id.to_string()), tokens })
    }
}

enum Slot {
    Neutral { night: bool, field: usize },
    Accent(usize),
}

fn key_slot(k: &str) -> Option<Slot> {
    if let Some(i) = ACCENT_KEYS.iter().position(|a| *a == k) {
        return Some(Slot::Accent(i));
    }
    let (mode, field) = k.split_once('.')?;
    let night = match mode {
        "day" => false,
        "night" => true,
        _ => return None,
    };
    NEUTRAL_KEYS.iter().position(|f| *f == field).map(|field| Slot::Neutral { night, field })
}

fn neutral_mut(n: &mut Neutrals, field: usize) -> &mut u32 {
    match field {
        0 => &mut n.bg,
        1 => &mut n.panel,
        2 => &mut n.line,
        3 => &mut n.ink,
        4 => &mut n.dim,
        _ => &mut n.faint,
    }
}

/// `#1a2b3c` or `1a2b3c`, and nothing else. A palette is read by people as often as by the player,
/// and one spelling reads more easily than four.
fn parse_colour(v: &str) -> Option<u32> {
    let hex = v.strip_prefix('#').unwrap_or(v);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

// ── Readability ─────────────────────────────────────────────────────────────────────────────

/// Contrast floors for one mode, as WCAG ratios: `(L1 + 0.05) / (L2 + 0.05)`, from 1.0 to 21.0.
#[derive(Clone, Copy, Debug)]
pub struct Floors {
    /// Primary text, against the background and against panels.
    pub ink: f32,
    /// Secondary text.
    pub dim: f32,
    /// Tertiary text: mono values, captions.
    pub faint: f32,
    /// The accent against the background and the row wash, and the ink drawn on the accent.
    pub accent: f32,
}

/// Day. Ink and dim are WCAG AA (4.5 for text, 3.0 for large text and controls). Cinder measures
/// 15.9 / 6.2 / 2.9, and its weakest accent (crimson) 4.2 against its own row wash.
pub const DAY: Floors = Floors { ink: 4.5, dim: 3.0, faint: 1.8, accent: 3.0 };
/// Night. WCAG's numbers do not apply here: night exists to emit as little light as a readable
/// screen can, and Cinder's own night measures 2.26 / 1.54 / 1.25, with accents down to 1.28. These
/// floors sit a margin under that, so any night at least as legible as Cinder's passes and one that
/// is not is refused.
pub const NIGHT: Floors = Floors { ink: 1.9, dim: 1.35, faint: 1.12, accent: 1.2 };
/// The brightest a night background may be, as relative luminance — about `#272727`. Night mode is
/// for a dark room.
pub const NIGHT_BG_MAX: f32 = 0.02;

/// Everything about `t` that would make the UI hard to read, one sentence each. Empty = fine.
///
/// Measured on the colours as they reach the panel, after the night dim. A palette without an
/// accent of its own is checked against EVERY built-in accent, because the picker offers all six.
pub fn problems(t: &Tokens) -> Vec<String> {
    let mut out = Vec::new();
    for night in [false, true] {
        let mode = if night { "night" } else { "day" };
        let f = if night { NIGHT } else { DAY };
        // The neutrals do not depend on the accent, so any accent resolves them.
        let th = t.theme(night, Accent::Amber);
        let ink = contrast(th.ink, th.bg);
        let dim = contrast(th.dim, th.bg);
        let faint = contrast(th.faint, th.bg);
        floor(&mut out, format!("{mode}.ink on {mode}.bg"), ink, f.ink);
        floor(&mut out, format!("{mode}.dim on {mode}.bg"), dim, f.dim);
        floor(&mut out, format!("{mode}.faint on {mode}.bg"), faint, f.faint);
        floor(&mut out, format!("{mode}.ink on {mode}.panel"), contrast(th.ink, th.panel), f.ink);
        if dim >= ink {
            out.push(format!(
                "{mode}.dim stands out as much as {mode}.ink ({dim:.2} vs {ink:.2}) — dim is for secondary text"
            ));
        }
        if faint >= dim {
            out.push(format!("{mode}.faint stands out as much as {mode}.dim ({faint:.2} vs {dim:.2})"));
        }
        if night && luminance(th.bg) > NIGHT_BG_MAX {
            out.push(format!(
                "night.bg is too bright for night mode (luminance {:.3}, at most {NIGHT_BG_MAX})",
                luminance(th.bg)
            ));
        }

        let pinned = t.accent.is_some();
        let accents: &[Accent] = if pinned { &[Accent::Amber] } else { &Accent::ALL };
        type Measure = fn(&Theme) -> f32;
        let checks: [(&str, &str, Measure); 3] = [
            ("accent", "bg", |th| contrast(th.acc, th.bg)),
            ("accent_ink", "accent", |th| contrast(th.acc_ink, th.acc)),
            ("accent", "row_select", |th| contrast(th.acc, th.row_sel)),
        ];
        for (what, on, measure) in checks {
            // The worst accent in scope, so one sentence covers all six.
            let worst = accents
                .iter()
                .map(|&a| (a, measure(&t.theme(night, a))))
                .min_by(|x, y| x.1.total_cmp(&y.1));
            let Some((a, got)) = worst else { continue };
            if got >= f.accent {
                continue;
            }
            if pinned {
                out.push(format!(
                    "{mode}.{what} on {mode}.{on}: contrast {got:.2}, needs at least {:.2}",
                    f.accent
                ));
            } else {
                out.push(format!(
                    "{mode}: the built-in {} accent measures {got:.2} for {what} on {on} (needs {:.2}) — \
                     give the palette an accent of its own (all six accent keys)",
                    a.name(),
                    f.accent
                ));
            }
        }
    }
    out
}

fn floor(out: &mut Vec<String>, what: String, got: f32, need: f32) {
    if got < need {
        out.push(format!("{what}: contrast {got:.2}, needs at least {need:.2}"));
    }
}

fn channel(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.040_45 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance, 0.0 (black) to 1.0 (white).
pub fn luminance(c: Rgb888) -> f32 {
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG contrast ratio between two colours, 1.0 to 21.0, in either order.
pub fn contrast(a: Rgb888, b: Rgb888) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

// ── The folder ──────────────────────────────────────────────────────────────────────────────

/// `slate.palette` → `slate`. Hidden files are not palettes: macOS writes a `._slate.palette`
/// beside every file it copies onto a FAT volume, and that is metadata, not a second palette.
pub fn palette_stem(file: &str) -> Option<&str> {
    let (stem, ext) = file.rsplit_once('.')?;
    (ext.eq_ignore_ascii_case(EXTENSION) && !stem.is_empty() && !stem.starts_with('.')).then_some(stem)
}

/// Turn what a palette folder holds into the loaded set, in cycle order, plus one line per file
/// that did not load and why.
///
/// `files` is `(file name, contents or the read error)`, in whatever order the directory listed
/// them. Anything that is not a `.palette` is ignored without comment. The result is sorted by id,
/// so the cycle does not depend on the order FAT happens to return entries in.
pub fn load_files(files: Vec<(String, Result<String, String>)>) -> (Vec<Palette>, Vec<String>) {
    let mut entries: Vec<(String, String, Result<String, String>)> = files
        .into_iter()
        .filter_map(|(file, body)| {
            let id = palette_stem(&file)?.to_ascii_lowercase();
            Some((id, file, body))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut list: Vec<Palette> = Vec::new();
    let mut skipped = Vec::new();
    for (id, file, body) in entries {
        if list.iter().any(|p| p.id == id) {
            skipped.push(format!("{file}: another file already uses the name `{id}`"));
            continue;
        }
        if list.len() == MAX_FILES {
            skipped.push(format!("{file}: only the first {MAX_FILES} palettes are loaded"));
            continue;
        }
        match body {
            Err(e) => skipped.push(format!("{file}: could not be read ({e})")),
            Ok(body) => match Palette::parse(&id, &body) {
                Ok(p) => list.push(p),
                Err(problems) => skipped.push(format!("{file}: {}", problems.join("; "))),
            },
        }
    }
    (list, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE: &str = include_str!("../palettes/cinder.palette");
    const SLATE: &str = include_str!("../palettes/slate.palette");
    const PAPER: &str = include_str!("../palettes/paper.palette");

    fn ok(id: &str, body: &str) -> Palette {
        Palette::parse(id, body).unwrap_or_else(|e| panic!("{id} should load:\n  {}", e.join("\n  ")))
    }

    fn refused(id: &str, body: &str) -> Vec<String> {
        Palette::parse(id, body).expect_err("should have been refused")
    }

    /// The template IS Cinder. If it drifts, every palette started from it starts wrong.
    #[test]
    fn the_reference_file_reproduces_cinder_exactly() {
        let p = ok("reference", REFERENCE);
        assert_eq!(p.tokens, CINDER);
        assert_eq!(p.name, "Cinder");
    }

    #[test]
    fn every_shipped_palette_loads() {
        ok("slate", SLATE);
        assert!(ok("paper", PAPER).tokens.accent.is_some(), "paper is light, so it must bring its own accent");
    }

    /// The floors are calibrated against Cinder: its own palette, with every accent the picker
    /// offers, has to pass them. If this fails, a floor was set above what the design itself does.
    #[test]
    fn cinder_passes_its_own_rules_with_every_accent() {
        assert_eq!(problems(&CINDER), Vec::<String>::new());
    }

    #[test]
    fn missing_keys_inherit_cinder() {
        let p = ok("almost", "day.bg = #101010\n");
        assert_eq!(p.tokens.day.bg, 0x101010);
        assert_eq!(p.tokens.day.ink, CINDER.day.ink);
        assert_eq!(p.tokens.night, CINDER.night);
        assert_eq!(p.tokens.accent, None);
        assert_eq!(p.name, "almost", "no name means the id");
    }

    #[test]
    fn a_pinned_accent_ignores_the_picker() {
        let p = ok("paper", PAPER);
        for a in Accent::ALL {
            assert_eq!(p.tokens.theme(false, a), p.tokens.theme(false, Accent::Amber));
            assert_eq!(p.tokens.theme(true, a), p.tokens.theme(true, Accent::Amber));
        }
    }

    #[test]
    fn syntax_errors_name_their_line_and_all_of_them_are_reported() {
        let e = refused(
            "broken",
            "name = Fine\nday.backround = #000000\nday.ink = pink\nnot a line\nday.dim = #111111\nday.dim = #222222\n",
        );
        let all = e.join("\n");
        assert!(all.contains("line 2: unknown key `day.backround`"), "{all}");
        assert!(all.contains("line 3: `day.ink` wants a colour"), "{all}");
        assert!(all.contains("line 4: expected `key = value`"), "{all}");
        assert!(all.contains("line 6: `day.dim` is set twice"), "{all}");
    }

    #[test]
    fn colours_take_one_spelling() {
        assert_eq!(parse_colour("#1a2B3c"), Some(0x1a2b3c));
        assert_eq!(parse_colour("1a2b3c"), Some(0x1a2b3c));
        for bad in ["#123", "#1234567", "rgb(1,2,3)", "#gg0000", ""] {
            assert_eq!(parse_colour(bad), None, "{bad}");
        }
    }

    #[test]
    fn accent_keys_are_all_or_nothing() {
        let e = refused("half", "day.accent = #ff0000\nday.accent_ink = #000000\n");
        let all = e.join("\n");
        assert!(all.contains("missing day.row_select, night.accent, night.accent_ink, night.row_select"), "{all}");
    }

    #[test]
    fn unreadable_text_is_refused() {
        let e = refused("murk", "day.ink = #141210\n");
        assert!(e.iter().any(|l| l.starts_with("day.ink on day.bg")), "{e:?}");
    }

    #[test]
    fn dim_may_not_outshine_ink() {
        let e = refused("inverted", "day.ink = #95908a\nday.dim = #ece7df\n");
        assert!(e.iter().any(|l| l.contains("day.dim stands out as much as day.ink")), "{e:?}");
    }

    #[test]
    fn a_bright_night_is_refused() {
        let e = refused("glare", "night.bg = #808080\n");
        assert!(e.iter().any(|l| l.starts_with("night.bg is too bright")), "{e:?}");
    }

    /// A light background with the built-in accents — tuned against near-black — has to be told what
    /// to do about it, not merely refused.
    #[test]
    fn a_light_palette_without_an_accent_is_told_to_bring_one() {
        let body: Vec<&str> =
            PAPER.lines().filter(|l| !l.contains("accent") && !l.contains("row_select")).collect();
        let e = refused("paperish", &body.join("\n"));
        assert!(e.iter().any(|l| l.contains("accent of its own")), "{e:?}");
    }

    #[test]
    fn the_builtin_id_and_unusable_ids_are_refused() {
        assert!(refused("cinder", "").join("\n").contains("built-in"));
        assert!(Palette::parse("Has Space", "").is_err());
        assert!(Palette::parse("", "").is_err());
        assert!(Palette::parse(&"x".repeat(MAX_ID + 1), "").is_err());
    }

    #[test]
    fn a_folder_loads_sorted_and_deduplicated_and_says_what_it_skipped() {
        let files = vec![
            ("zeta.palette".to_string(), Ok("name = Zeta\n".to_string())),
            ("Alpha.PALETTE".to_string(), Ok(String::new())),
            ("alpha.palette".to_string(), Ok(String::new())),
            ("notes.txt".to_string(), Ok("ignored".to_string())),
            ("._alpha.palette".to_string(), Ok("macOS metadata".to_string())),
            ("broken.palette".to_string(), Ok("day.ink = #0d0c0b\n".to_string())),
            ("gone.palette".to_string(), Err("I/O error".to_string())),
            ("cinder.palette".to_string(), Ok(REFERENCE.to_string())),
        ];
        let (list, skipped) = load_files(files);
        let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "zeta"]);
        let all = skipped.join("\n");
        assert!(all.contains("alpha.palette: another file already uses the name `alpha`"), "{all}");
        assert!(all.contains("broken.palette: day.ink on day.bg"), "{all}");
        assert!(all.contains("gone.palette: could not be read (I/O error)"), "{all}");
        assert!(all.contains("cinder.palette: `cinder` is the built-in palette"), "{all}");
        assert_eq!(skipped.len(), 4, "{all}");
    }

    #[test]
    fn only_the_first_max_files_load() {
        let files = (0..MAX_FILES + 3).map(|i| (format!("p{i:02}.palette"), Ok(String::new()))).collect();
        let (list, skipped) = load_files(files);
        assert_eq!(list.len(), MAX_FILES);
        assert_eq!(skipped.len(), 3);
    }
}
