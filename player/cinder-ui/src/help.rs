//! Menu ▸ Help & controls (design handoff 5i) — one list, values only.
//!
//! ```text
//!   Help & controls
//!   THE WAY BACK TO SONY
//!   1  Switch on with the cable in
//!   2  POWER while the Sony logo shows     Once or twice, no PC needed
//!   GETTING AROUND
//!   Now Playing                        TAP THE CLOCK
//!   ...
//!   AGAIN
//!   Replay the introduction                         ›
//! ```
//!
//! Until this screen, Help was the seven-page first-run introduction opened again from the Menu:
//! paged, so finding "how do I get back to Sony's player" meant tapping through Welcome and What's
//! inside to reach it, and it was not there at all. The introduction still runs once on the first
//! start, and the last row here replays it; this is the reference you come back to.
//!
//! A row is what you want to do and, on the right, how. EVERY ROW IS SOMETHING THAT EXISTS, pinned
//! the same way `onboarding.rs` pins its rows — the gestures to `nav::tap` and the shell's edge
//! swipes, the ladder to `docs/RECOVERY.md`. The pull-down panel is listed only when it is switched
//! on, because a gesture that does nothing is not help.
//!
//! The list scrolls, so the layout is ONE function, [`parts`], that both `render` and the hit test
//! read.

use crate::canvas::{Canvas, H};
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;

/// One row: a numbered lead or none, what it does, a second line or none, and how.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Line {
    pub lead: &'static str,
    pub title: &'static str,
    pub sub: &'static str,
    pub how: &'static str,
}

const fn l(title: &'static str, how: &'static str) -> Line {
    Line {
        lead: "",
        title,
        sub: "",
        how,
    }
}

/// The ladder, in the order to try it (`docs/RECOVERY.md`, rungs 0, 0b and 1, and Settings).
const WAY_BACK: [Line; 4] = [
    Line {
        lead: "1",
        title: "Switch on with the cable in",
        sub: "Not on the first start after an install",
        how: "",
    },
    Line {
        lead: "2",
        title: "POWER while the Sony logo shows",
        sub: "Once or twice, no PC needed",
        how: "",
    },
    Line {
        lead: "3",
        title: "Four failed starts in a row",
        sub: "Cinder hands over by itself",
        how: "",
    },
    Line {
        lead: "4",
        title: "Boot to stock, in Settings",
        sub: "For one start",
        how: "",
    },
];
const GETTING_AROUND: [Line; 5] = [
    l("Now Playing", "TAP THE CLOCK"),
    l("Menu", "TAP \u{2261}"),
    l("Shelf", "TAP THE BOOKMARK"),
    l("Back", "ARROW \u{b7} LEFT EDGE"),
    l("The song from a list", "BOTTOM STRIP"),
];
const ANY_SONG: [Line; 3] = [
    l("Play from there", "TAP"),
    l("Play next", "SWIPE \u{2190}"),
    l("Add to Up Next", "SWIPE \u{2192}"),
];
const UP_NEXT: [Line; 7] = [
    l("Jump to a song", "TAP"),
    l("Move a song", "DRAG \u{2261}"),
    l("Pick up a song", "HOLD"),
    l("Remove a song", "SWIPE"),
    l("Shuffle the list", "MIX"),
    l("Empty what is left", "CLEAR"),
    l("Keep it as a playlist", "SAVE"),
];
const NOW_PLAYING: [Line; 5] = [
    l("Cover \u{b7} spectrum \u{b7} level", "SWIPE THE ART"),
    l("Previous \u{b7} next track", "SWIPE BELOW IT"),
    l("Track info", "TAP THE TITLE"),
    l("Repeat all \u{b7} album \u{b7} one", "TAP REPEAT"),
    Line {
        lead: "",
        title: "Stop after this song",
        sub: "Settings or the pull-down panel",
        how: "SLEEP: SONG",
    },
];
const ANYWHERE: [Line; 2] = [
    l("The Shelf", "SWIPE UP"),
    l("Scroll a list", "SWIPE \u{2195}"),
];
/// Listed only while Settings ▸ Pull-down panel is on.
const PULL_DOWN: Line = l("Pull-down panel", "SWIPE DOWN");
const BUTTONS: [Line; 6] = [
    l("Play \u{b7} pause", "PLAY"),
    l("Previous \u{b7} next", "\u{25c1} \u{25b7}"),
    l("Volume", "VOL + \u{2212}"),
    l("Screen on \u{b7} off", "POWER"),
    l("Power off \u{b7} restart", "HOLD POWER"),
    Line {
        lead: "",
        title: "Lock the screen",
        sub: "Volume, play and skip still work",
        how: "HOLD SWITCH",
    },
];

/// A single-line row is shorter than the kit's two-line row: this is a reference list, and 28
/// rows at 64 px would be three screens of scrolling for one line each.
const LINE_H: i32 = 52;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Label(&'static str),
    Line(Line),
    Replay,
}

/// What a tap can land on. Only the last row does anything: every other row is a fact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Replay,
}

const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The whole list, as `(part, content-space top, height)`. `pull_down` = the panel is switched on.
fn parts(pull_down: bool) -> Vec<(Part, i32, i32)> {
    let mut out = Vec::new();
    let mut y = 0;
    let mut push = |p: Part, h: i32| {
        out.push((p, y, h));
        y += h;
    };
    let anywhere: Vec<Line> = ANYWHERE
        .iter()
        .copied()
        .chain(pull_down.then_some(PULL_DOWN))
        .collect();
    let sections: [(&'static str, &[Line]); 7] = [
        ("THE WAY BACK TO SONY", &WAY_BACK),
        ("GETTING AROUND", &GETTING_AROUND),
        ("ON ANY SONG", &ANY_SONG),
        ("IN UP NEXT", &UP_NEXT),
        ("NOW PLAYING", &NOW_PLAYING),
        ("ANYWHERE", &anywhere),
        ("BUTTONS", &BUTTONS),
    ];
    for (label, lines) in sections {
        push(Part::Label(label), kit::SECTION_H);
        for line in lines {
            push(
                Part::Line(*line),
                if line.sub.is_empty() {
                    LINE_H
                } else {
                    kit::ROW_H
                },
            );
        }
    }
    push(Part::Label("AGAIN"), kit::SECTION_H);
    push(Part::Replay, LINE_H);
    out
}

pub fn content_height(pull_down: bool) -> i32 {
    parts(pull_down).last().map_or(0, |&(_, y, h)| y + h)
}

pub fn max_scroll(pull_down: bool) -> i32 {
    (content_height(pull_down) + 8 - (H as i32 - TOP)).max(0)
}

/// What is under screen point `(x, y)` at scroll `scroll`.
pub fn item_at(_x: i32, y: i32, scroll: i32, pull_down: bool) -> Option<Item> {
    if y < TOP {
        return None;
    }
    let cy = y - TOP + scroll;
    parts(pull_down)
        .into_iter()
        .find(|&(_, top, h)| (top..top + h).contains(&cy))
        .and_then(|(p, _, _)| (p == Part::Replay).then_some(Item::Replay))
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, scroll: i32, pull_down: bool) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Help & controls", None);
    c.set_clip_y(TOP, H as i32);
    for (p, top, h) in parts(pull_down) {
        let y = TOP + top - scroll;
        if y + h < TOP || y >= H as i32 {
            continue;
        }
        match p {
            Part::Label(l) => {
                kit::section_label(c, t, f, y, l, None);
            }
            Part::Line(line) => {
                let trail = if line.how.is_empty() {
                    Trail::None
                } else {
                    Trail::Value(line.how)
                };
                kit::row(
                    c,
                    t,
                    f,
                    y,
                    h,
                    &Row::new(line.title)
                        .lead(line.lead)
                        .sub(line.sub)
                        .trail(trail),
                );
            }
            Part::Replay => {
                kit::row(
                    c,
                    t,
                    f,
                    y,
                    h,
                    &Row::new("Replay the introduction").trail(Trail::Open("")),
                );
            }
        }
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the replay row is a target, it answers where it is drawn, and it is reachable at the
    /// bottom of the scroll.
    #[test]
    fn only_the_replay_row_answers_and_it_is_reachable() {
        for pull in [false, true] {
            for (p, top, h) in parts(pull) {
                let got = item_at(240, TOP + top + h / 2, 0, pull);
                assert_eq!(got.is_some(), p == Part::Replay, "{p:?}");
            }
            let deep = max_scroll(pull);
            assert!(deep > 0, "the list is taller than the glass");
            let (_, top, h) = *parts(pull).last().unwrap();
            let y = TOP + top - deep + h / 2;
            assert!(
                y < H as i32,
                "the replay row is off the bottom at full scroll"
            );
            assert_eq!(item_at(240, y, deep, pull), Some(Item::Replay));
        }
    }

    /// The pull-down gesture is listed only while the panel is switched on.
    #[test]
    fn the_pull_down_row_follows_the_setting() {
        let has = |pull| {
            parts(pull)
                .iter()
                .any(|(p, _, _)| *p == Part::Line(PULL_DOWN))
        };
        assert!(!has(false));
        assert!(has(true));
    }

    /// Every character on this page is in the bundled face that draws it. The arrows and `≡` are
    /// mono-only, so they live in the right-hand column, which is mono; a symbol no bundled face
    /// has would draw as a box, and the fallback chain never loads for one (`text::resolve`).
    #[test]
    fn every_character_is_in_the_face_that_draws_it() {
        let load = |name: &str| {
            let path = format!("{}/assets/fonts/{name}", env!("CARGO_MANIFEST_DIR"));
            fontdue::Font::from_bytes(
                std::fs::read(path).unwrap(),
                fontdue::FontSettings::default(),
            )
            .unwrap()
        };
        let (mono, sans) = (
            load("JetBrainsMono-Regular.ttf"),
            load("HankenGrotesk-SemiBold.ttf"),
        );
        for (p, _, _) in parts(true) {
            let Part::Line(line) = p else { continue };
            for ch in line.how.chars() {
                assert!(
                    mono.lookup_glyph_index(ch) != 0,
                    "mono lacks {ch:?} in {:?}",
                    line.how
                );
            }
            for ch in line
                .title
                .chars()
                .chain(line.sub.chars())
                .chain(line.lead.chars())
            {
                assert!(
                    ch.is_ascii() || sans.lookup_glyph_index(ch) != 0,
                    "sans lacks {ch:?} in {:?}",
                    line.title
                );
            }
        }
    }

    /// The ladder is numbered in order, one to four, because the order is the advice.
    #[test]
    fn the_way_back_is_numbered_in_order() {
        let leads: Vec<&str> = WAY_BACK.iter().map(|l| l.lead).collect();
        assert_eq!(leads, ["1", "2", "3", "4"]);
    }
}
