//! Sound ▸ Presets — sounds the owner has saved, and the A/B compare's tools.
//!
//! A PRESET is a whole `nav::SoundSetup` (the 10-band EQ, every Sony effect Cinder sets, Tone
//! Control, the balance) kept under a number. Loading one replaces the side of the A/B compare that
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

/// Saved slots. Four fill the glass under the save row with the compare tools below them;
/// ponytail: more needs the screen to scroll.
pub const MAX: usize = 4;

/// What a tap lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Save the live sound as a new preset.
    Save,
    /// Load preset `i` into the live side of the compare.
    Load(usize),
    /// The Delete trail of preset `i`: the first tap arms it, the second deletes.
    Delete(usize),
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
        "Saved with the sound, and different on A and B",
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

/// The Delete trail's width: the right edge of a preset row.
const DELETE_W: i32 = 120;

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

/// What is under `(x, y)` with `n` presets saved.
pub fn hit(x: i32, y: i32, n: usize) -> Option<Hit> {
    parts(n).into_iter().find_map(|(p, top, h)| match p {
        Part::Row(Hit::Load(i)) if (top..top + h).contains(&y) => {
            Some(if x >= crate::canvas::W as i32 - DELETE_W {
                Hit::Delete(i)
            } else {
                Hit::Load(i)
            })
        }
        Part::Row(hit) if (top..top + h).contains(&y) => Some(hit),
        _ => None,
    })
}

/// Vertical centre of a control, for tests and the sim recipe.
pub fn centre(h: Hit, n: usize) -> i32 {
    let key = match h {
        Hit::Delete(i) => Hit::Load(i),
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
    for (part, top, h) in parts(n) {
        match part {
            Part::Label(l) => {
                kit::section_label(c, t, f, top, l, None);
            }
            Part::Row(Hit::Save) => {
                let sub = if n >= MAX {
                    "All four are in use \u{b7} delete one to save another".to_string()
                } else {
                    crate::ui_format!("Keeps what {} sounds like now, as Preset {}", p.live, n + 1)
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
                let title = crate::ui_format!("Preset {}", i + 1);
                let trail = if p.armed == Some(i) {
                    crate::i18n::text("SURE?")
                } else {
                    crate::i18n::text("DELETE")
                };
                kit::row(
                    c,
                    t,
                    f,
                    top,
                    h,
                    &Row::new(&title).sub(&p.saved[i]).trail(Trail::Value(trail)),
                );
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
            Part::Row(Hit::Delete(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every control answers at its centre, Delete only at the right edge, at every count.
    #[test]
    fn every_control_hits_itself() {
        for n in 0..=MAX {
            let mut hits = vec![Hit::Save, Hit::Copy];
            hits.extend((0..n).map(Hit::Load));
            hits.extend(FOLLOWS.iter().map(|(b, _, _)| Hit::Follow(*b)));
            for h in hits {
                let y = centre(h, n);
                assert!(
                    y > crate::chrome::HEADER_BOTTOM && y < crate::canvas::H as i32,
                    "{h:?} n={n}"
                );
                assert_eq!(hit(100, y, n), Some(h), "n={n}");
            }
            for i in 0..n {
                assert_eq!(
                    hit(crate::canvas::W as i32 - 10, centre(Hit::Load(i), n), n),
                    Some(Hit::Delete(i))
                );
            }
        }
    }

    #[test]
    fn the_full_screen_fits_the_glass() {
        let (_, top, h) = *parts(MAX).last().unwrap();
        assert!(top + h <= crate::canvas::H as i32);
    }
}
