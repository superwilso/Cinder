//! Sound ▸ Presets — sounds the owner has saved, and the A/B compare's tools.
//!
//! A PRESET is a whole `nav::SoundSetup` (the 10-band EQ, every Sony effect Cinder sets, Tone
//! Control, the balance) kept under a number, and a name if it was given one (the Search keyboard,
//! on Save and from the row's RENAME). Loading one replaces the side of the A/B compare that
//! is live. The compare itself is the Sound screen's A/B control: two sides to switch between while
//! listening, nothing more (the owner, 2026-10-18: "A/B was supposed to allow people to compare
//! settings and work out which they preferred, not to be sound profiles").
//!
//! ALSO SAVED (each Off by default): mono, the linear amp and the DAC EQ. Off, one value holds on
//! both sides of the compare and through a preset load; on, they travel with the sound.
//!
//! Layout is ONE function ([`parts`]) that both render and hit test walk, so they cannot drift.

use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fit, sty};
use crate::Canvas;

/// Saved slots. Past four the screen scrolls; the number is a guard on the settings file, not a
/// layout limit.
pub const MAX: usize = 12;

/// What a tap lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Save the live sound as a new preset.
    Save,
    /// Load preset `i` into the live side of the compare.
    Load(usize),
    /// The Delete trail of preset `i`: the first tap arms it, the second deletes.
    Delete(usize),
    /// The Rename trail of preset `i`: the keyboard, seeded with its name.
    Rename(usize),
    /// Copy the live side over the other one.
    Copy,
    /// Switch mono / linear amp / DAC EQ into or out of what a sound carries (`nav::FOLLOW_*`).
    Follow(u8),
}

/// The ALSO SAVED rows: `(bit, title, what it means)`.
pub const FOLLOWS: [(u8, &str, &str); 3] = [
    (
        crate::nav::FOLLOW_MONO,
        "Mono",
        "Saved with the sound, per A and B",
    ),
    (
        crate::nav::FOLLOW_AMP,
        "Linear amp",
        "3.5 mm only \u{b7} saved with the sound",
    ),
    (
        crate::nav::FOLLOW_DAC_EQ,
        "DAC EQ",
        "3.5 mm only \u{b7} saved with the sound",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Label(&'static str),
    Row(Hit),
    Empty,
}

/// The Delete trail's width: the right edge of a preset row. Rename sits in the same width before it.
const DELETE_W: i32 = 100;
const TRAIL_W: i32 = DELETE_W * 2;

/// What preset `i` is called: its name, or "Preset N" when it has none.
pub fn name(names: &[String], i: usize) -> String {
    match names.get(i).map(|n| n.trim()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => crate::ui_format!("Preset {}", i + 1),
    }
}

/// A typed name as it is kept: one line, at most 32 characters. Empty means "Preset N".
pub fn clean_name(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(32)
        .collect::<String>()
        .trim()
        .to_string()
}

fn parts(n: usize) -> Vec<(Part, i32, i32)> {
    let mut v = Vec::new();
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut push = |p: Part, h: i32, v: &mut Vec<(Part, i32, i32)>| {
        v.push((p, y, h));
        y += h;
    };
    push(Part::Row(Hit::Save), kit::ROW_H, &mut v);
    push(
        Part::Label(crate::i18n::text("SAVED")),
        kit::SECTION_H,
        &mut v,
    );
    if n == 0 {
        push(Part::Empty, kit::ROW_H, &mut v);
    }
    for i in 0..n.min(MAX) {
        push(Part::Row(Hit::Load(i)), kit::ROW_H, &mut v);
    }
    push(
        Part::Label(crate::i18n::text("A/B COMPARE")),
        kit::SECTION_H,
        &mut v,
    );
    push(Part::Row(Hit::Copy), kit::ROW_H, &mut v);
    push(
        Part::Label(crate::i18n::text("ALSO SAVED")),
        kit::SECTION_H,
        &mut v,
    );
    for (bit, _, _) in FOLLOWS {
        push(Part::Row(Hit::Follow(bit)), kit::ROW_H, &mut v);
    }
    v
}

/// How far the list scrolls with `n` presets saved.
pub fn max_scroll(n: usize) -> i32 {
    let (_, top, h) = *parts(n).last().expect("the list is never empty");
    (top + h - crate::canvas::H as i32).max(0)
}

/// What is under `(x, y)` with `n` presets saved, the list scrolled `scroll` px.
pub fn hit(x: i32, y: i32, n: usize, scroll: i32) -> Option<Hit> {
    if y < crate::chrome::HEADER_BOTTOM {
        return None;
    }
    let y = y + scroll;
    let w = crate::canvas::W as i32;
    parts(n).into_iter().find_map(|(p, top, h)| match p {
        Part::Row(Hit::Load(i)) if (top..top + h).contains(&y) => Some(if x >= w - DELETE_W {
            Hit::Delete(i)
        } else if x >= w - TRAIL_W {
            Hit::Rename(i)
        } else {
            Hit::Load(i)
        }),
        Part::Row(hit) if (top..top + h).contains(&y) => Some(hit),
        _ => None,
    })
}

/// Vertical centre of a control, unscrolled, for tests and the sim recipe.
pub fn centre(h: Hit, n: usize) -> i32 {
    let key = match h {
        Hit::Delete(i) | Hit::Rename(i) => Hit::Load(i),
        other => other,
    };
    parts(n)
        .into_iter()
        .find_map(|(p, top, ph)| (p == Part::Row(key)).then_some(top + ph / 2))
        .unwrap_or(0)
}

/// What the screen draws.
pub struct Presets<'a> {
    /// One line per saved preset: what it holds ("EQ · DSEE HX").
    pub saved: &'a [String],
    /// Their names, by index; empty for "Preset N".
    pub names: &'a [String],
    /// How far the list is scrolled.
    pub scroll: i32,
    /// The preset whose Delete has been tapped once.
    pub armed: Option<usize>,
    /// The live side of the compare, "A" or "B".
    pub live: &'a str,
    /// `nav::FOLLOW_*` bits.
    pub follow: u8,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, p: &Presets) {
    c.fill(t.bg);
    crate::chrome::header(
        c,
        t,
        f,
        crate::i18n::text("Presets"),
        Some(crate::i18n::text("Sound")),
    );
    let other = if p.live == "A" { "B" } else { "A" };
    let n = p.saved.len();
    c.set_clip_y(crate::chrome::HEADER_BOTTOM, crate::canvas::H as i32);
    for (part, top, h) in parts(n) {
        let top = top - p.scroll;
        if top + h <= crate::chrome::HEADER_BOTTOM || top >= crate::canvas::H as i32 {
            continue;
        }
        match part {
            Part::Label(l) => {
                kit::section_label(c, t, f, top, l, None);
            }
            Part::Row(Hit::Save) => {
                let sub = if n >= MAX {
                    crate::ui_format!("All {} are in use \u{b7} delete one to save another", MAX)
                } else {
                    crate::ui_format!("Saves how {} sounds now, under a name", p.live)
                };
                kit::row(
                    c,
                    t,
                    f,
                    top,
                    h,
                    &Row::new(crate::i18n::text("Save current sound"))
                        .sub(&sub)
                        .trail(Trail::Tag(crate::i18n::text("SAVE"))),
                );
            }
            Part::Empty => {
                let st = sty(
                    Family::Sans,
                    Weight::Regular,
                    crate::scale::SECONDARY,
                    t.dim,
                    0.0,
                );
                let w = (kit::RIGHT - kit::LEFT) as f32;
                let s = fit(
                    f,
                    crate::i18n::text("Nothing saved yet. Save the sound you have now."),
                    &st,
                    w,
                );
                text::draw(c, f, kit::LEFT as f32, (top + h / 2 + 5) as f32, &s, &st);
            }
            Part::Row(Hit::Load(i)) => {
                let title = name(p.names, i);
                kit::row(
                    c,
                    t,
                    f,
                    top,
                    h,
                    &Row::new(&title)
                        .sub(&p.saved[i])
                        .trail(Trail::Reserve(TRAIL_W)),
                );
                // Two mono words in the reserved width, each centred in its own tap zone.
                let w = crate::canvas::W as i32;
                let armed = p.armed == Some(i);
                for (word, right, col) in [
                    (crate::i18n::text("RENAME"), w - DELETE_W, t.dim),
                    (
                        crate::i18n::text(if armed { "SURE?" } else { "DELETE" }),
                        w,
                        if armed { t.acc } else { t.dim },
                    ),
                ] {
                    let st = sty(
                        Family::Mono,
                        Weight::Regular,
                        crate::scale::SECONDARY,
                        col,
                        0.0,
                    );
                    let s = fit(f, word, &st, (DELETE_W - 12) as f32);
                    let tw = text::measure(f, &s, &st);
                    let x = right as f32 - DELETE_W as f32 / 2.0 - tw / 2.0;
                    text::draw(c, f, x, (top + h / 2 + 5) as f32, &s, &st);
                }
            }
            Part::Row(Hit::Copy) => {
                let title = crate::ui_format!("Copy {} to {other}", p.live, other = other);
                let sub = crate::ui_format!(
                    "{other} starts as {}, to change one thing and compare",
                    p.live,
                    other = other
                );
                kit::row(c, t, f, top, h, &Row::new(&title).sub(&sub));
            }
            Part::Row(Hit::Follow(bit)) => {
                if let Some((_, title, sub)) = FOLLOWS.iter().find(|(b, _, _)| *b == bit) {
                    kit::row(
                        c,
                        t,
                        f,
                        top,
                        h,
                        &Row::new(title)
                            .sub(sub)
                            .trail(Trail::Switch(p.follow & bit != 0)),
                    );
                }
            }
            Part::Row(Hit::Delete(_) | Hit::Rename(_)) => {}
        }
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every control answers at its centre, scrolled into view; Rename and Delete only in their
    /// zones at the right edge; at every count.
    #[test]
    fn every_control_hits_itself() {
        let glass = crate::chrome::HEADER_BOTTOM..crate::canvas::H as i32;
        for n in 0..=MAX {
            let mut hits = vec![Hit::Save, Hit::Copy];
            hits.extend((0..n).map(Hit::Load));
            hits.extend(FOLLOWS.iter().map(|(b, _, _)| Hit::Follow(*b)));
            let w = crate::canvas::W as i32;
            for h in hits {
                let y = centre(h, n);
                // The least scroll that brings it on the glass, as a finger would.
                let scroll = (y + 40 - crate::canvas::H as i32).clamp(0, max_scroll(n));
                assert!(
                    glass.contains(&(y - scroll)),
                    "{h:?} n={n} cannot be reached"
                );
                assert_eq!(hit(100, y - scroll, n, scroll), Some(h), "n={n}");
                if let Hit::Load(i) = h {
                    assert_eq!(hit(w - 10, y - scroll, n, scroll), Some(Hit::Delete(i)));
                    assert_eq!(
                        hit(w - DELETE_W - 10, y - scroll, n, scroll),
                        Some(Hit::Rename(i))
                    );
                }
            }
        }
        assert_eq!(hit(100, 10, 3, 0), None, "the header is not a row");
    }

    #[test]
    fn four_fit_and_more_scroll() {
        assert_eq!(max_scroll(4), 0, "the screen as it was: no scroll");
        assert!(max_scroll(MAX) > 0);
    }

    #[test]
    fn names_fall_back_to_their_number_and_are_kept_to_one_line() {
        let names = vec![String::new(), "Bassy".to_string()];
        assert_eq!(name(&names, 0), "Preset 1");
        assert_eq!(name(&names, 1), "Bassy");
        assert_eq!(name(&names, 5), "Preset 6");
        assert_eq!(clean_name("  Late\tnight\n "), "Latenight");
        assert_eq!(clean_name(&"x".repeat(40)).len(), 32);
    }
}
