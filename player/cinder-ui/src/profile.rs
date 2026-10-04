//! Sound profiles per output (design handoff 2a / 2b, and the Sound profile row of 2g).
//!
//! A PROFILE is one of the two A/B sound setups (`nav::SoundSetup`): the 10-band EQ, every Sony
//! effect Cinder can set (Sound and Sound ▸ Advanced), the Tone Control bands and the balance. Each
//! OUTPUT — the headphone jack, Bluetooth, USB-DAC — remembers which of the two it uses, and when
//! the route changes the player switches to that output's profile by itself. The Sound screen's
//! A/B control still works the way it did: it picks the profile for the output that is live now.
//!
//! WHAT IS NOT IN A PROFILE UNLESS THE OWNER PUTS IT THERE (ALSO PER PROFILE, each Off by default
//! — the owner's call of 2026-10-04: "have it as an option"):
//!   * MONO — an accessibility need, not a tuning (see `nav::App::mono`). Off, it holds on both
//!     sides of any comparison. On, a profile can be mono (one Bluetooth speaker) and the other not.
//!   * The linear headphone amp and the DAC EQ — hardware stages of the 3.5 mm output only, driven
//!     through a codec register and a codec table reload. They cannot reach Bluetooth. On, a route
//!     change that switches profile may rewrite the codec's tone table (the shell only does so when
//!     the curve really differs).
//!
//! The Profiles screen is this module's other half: which profile each output uses, and a way to
//! start B from a copy of A. Layout is ONE function ([`parts`]) that both the render and the hit
//! test walk, so they cannot drift apart.

use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fit, sty};
use crate::Canvas;

/// Where the audio is going. The order is the persisted order of [`crate::nav::App::profile_map`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// The 3.5 mm jack (also the output while nothing else is).
    Jack,
    /// A Bluetooth sink is connected and the player is transmitting to it.
    Bluetooth,
    /// USB-DAC mode: a PC is the source, and the player renders it to the jack or bridges it to a
    /// connected Bluetooth sink. One output for both, because the SOURCE is what makes it different.
    UsbDac,
}

impl Output {
    pub const ALL: [Output; 3] = [Output::Jack, Output::Bluetooth, Output::UsbDac];

    pub fn idx(self) -> usize {
        match self {
            Output::Jack => 0,
            Output::Bluetooth => 1,
            Output::UsbDac => 2,
        }
    }

    /// Row title on the Profiles screen.
    pub fn label(self) -> &'static str {
        match self {
            Output::Jack => "Headphone jack",
            Output::Bluetooth => "Bluetooth",
            Output::UsbDac => "USB-DAC",
        }
    }

    /// Mono caption form, for strips and toasts.
    pub fn short(self) -> &'static str {
        match self {
            Output::Jack => "3.5 MM",
            Output::Bluetooth => "BLUETOOTH",
            Output::UsbDac => "USB-DAC",
        }
    }

    /// The settings-file key holding this output's profile letter. Part of the on-disk format.
    pub fn key(self) -> &'static str {
        match self {
            Output::Jack => "profile_jack",
            Output::Bluetooth => "profile_bt",
            Output::UsbDac => "profile_usb",
        }
    }
}

/// Which output is live. USB-DAC wins over Bluetooth: in DAC mode the PC is the source even when
/// the bridge is sending it to headphones, and that is the case a separate profile exists for.
pub fn output_for(usb_dac: bool, bt_route: bool) -> Output {
    if usb_dac {
        Output::UsbDac
    } else if bt_route {
        Output::Bluetooth
    } else {
        Output::Jack
    }
}

/// Profile letters, indexed by setup index (0 = A, 1 = B).
pub const LETTERS: [&str; 2] = ["A", "B"];

pub fn letter(idx: usize) -> &'static str {
    LETTERS[idx & 1]
}

/// Read a profile letter from the settings file: `a`/`b` (either case). Anything else is `None`,
/// so a hand-edited line cannot select a profile that does not exist.
pub fn parse_letter(s: &str) -> Option<usize> {
    match s.trim() {
        "a" | "A" => Some(0),
        "b" | "B" => Some(1),
        _ => None,
    }
}

/// The Sound screen's Profile row subtitle: the live output first, then any output that uses the
/// OTHER profile — the one fact that is not visible from the letter in the title.
/// "3.5 mm now · Bluetooth uses B".
pub fn summary(map: [usize; 3], live: Output) -> String {
    let now = match live {
        Output::Jack => "3.5 mm",
        Output::Bluetooth => "Bluetooth",
        Output::UsbDac => "USB-DAC",
    };
    let mine = map[live.idx()] & 1;
    let others: Vec<String> = Output::ALL
        .iter()
        .filter(|o| **o != live && map[o.idx()] & 1 != mine)
        .map(|o| format!("{} uses {}", o.label(), letter(map[o.idx()])))
        .collect();
    if others.is_empty() {
        format!("{now} now \u{b7} every output uses {}", letter(mine))
    } else {
        format!("{now} now \u{b7} {}", others.join(" \u{b7} "))
    }
}

// ── the Profiles screen ─────────────────────────────────────────────────────────────────────────

/// What a tap on the Profiles screen lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Flip this output between A and B.
    Output(Output),
    /// Copy the live profile over the other one.
    Copy,
    /// Switch one of mono / linear amp / DAC EQ into or out of the profiles (`nav::FOLLOW_*`).
    Follow(u8),
}

/// The ALSO PER PROFILE rows: `(bit, title, what it means)`.
pub const FOLLOWS: [(u8, &str, &str); 3] = [
    (crate::nav::FOLLOW_MONO, "Mono", "Each profile has its own mono switch"),
    (crate::nav::FOLLOW_AMP, "Linear amp", "3.5 mm only \u{b7} each profile picks its amp"),
    (crate::nav::FOLLOW_DAC_EQ, "DAC EQ", "3.5 mm only \u{b7} each profile has its curve"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Strip,
    Label(&'static str),
    Row(Hit),
    Note,
}

const NOTE_H: i32 = 64;

/// The screen, top to bottom: `(part, top, height)`. The single source for render and hit test.
fn parts() -> Vec<(Part, i32, i32)> {
    let mut v = Vec::new();
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut push = |p: Part, h: i32, v: &mut Vec<(Part, i32, i32)>| {
        v.push((p, y, h));
        y += h;
    };
    push(Part::Strip, kit::STRIP_H, &mut v);
    push(Part::Label("EACH OUTPUT USES"), kit::SECTION_H, &mut v);
    for o in Output::ALL {
        push(Part::Row(Hit::Output(o)), kit::ROW_H, &mut v);
    }
    push(Part::Label("PROFILES"), kit::SECTION_H, &mut v);
    push(Part::Row(Hit::Copy), kit::ROW_H, &mut v);
    push(Part::Label("ALSO PER PROFILE"), kit::SECTION_H, &mut v);
    for (bit, _, _) in FOLLOWS {
        push(Part::Row(Hit::Follow(bit)), kit::ROW_H, &mut v);
    }
    push(Part::Note, NOTE_H, &mut v);
    v
}

/// Which control is under `y`. Rows are full width, so x does not matter.
pub fn hit(_x: i32, y: i32) -> Option<Hit> {
    parts().into_iter().find_map(|(p, top, h)| match p {
        Part::Row(hit) if (top..top + h).contains(&y) => Some(hit),
        _ => None,
    })
}

/// Vertical centre of a control, for tests and the sim recipe.
pub fn centre(h: Hit) -> i32 {
    parts()
        .into_iter()
        .find_map(|(p, top, ph)| (p == Part::Row(h)).then_some(top + ph / 2))
        .unwrap_or(0)
}

/// What the Profiles screen draws.
#[derive(Clone, Copy, Debug)]
pub struct Profiles {
    /// Profile index per output, in [`Output::ALL`] order.
    pub map: [usize; 3],
    /// The output that is live now.
    pub live: Output,
    /// `nav::FOLLOW_*` bits: which optional members travel with a profile.
    pub follow: u8,
}

fn output_sub(o: Output, live: bool) -> &'static str {
    match (o, live) {
        (Output::Jack, true) => "Live now \u{b7} the 3.5 mm output",
        (Output::Jack, false) => "The 3.5 mm output",
        (Output::Bluetooth, true) => "Live now \u{b7} while headphones are connected",
        (Output::Bluetooth, false) => "While headphones are connected",
        // Whether Sony's effect chain reaches the PC's audio is not measured yet (DEVICE_CHECKLIST
        // §26.6). The row still does what it says — it picks what is live in DAC mode — so it stays,
        // but it says what is not known rather than implying the effects are heard.
        (Output::UsbDac, true) => "Live now \u{b7} effects on PC audio unverified",
        (Output::UsbDac, false) => "In DAC mode \u{b7} effects on PC audio unverified",
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, p: &Profiles) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Sound profiles", None);
    let live_letter = letter(p.map[p.live.idx()]);
    let other_letter = letter(1 - (p.map[p.live.idx()] & 1));
    for (part, top, h) in parts() {
        match part {
            Part::Strip => {
                let s = format!("LIVE: {} \u{b7} PROFILE {live_letter}", p.live.short());
                kit::strip(c, t, f, top, &s);
            }
            Part::Label(l) => {
                kit::section_label(c, t, f, top, l, None);
            }
            Part::Row(Hit::Output(o)) => {
                let live = o == p.live;
                let r = Row::new(o.label())
                    .sub(output_sub(o, live))
                    .trail(Trail::Value(letter(p.map[o.idx()])))
                    .sel(live);
                kit::row(c, t, f, top, h, &r);
            }
            Part::Row(Hit::Copy) => {
                let title = format!("Copy {live_letter} to {other_letter}");
                let sub = format!("{other_letter} becomes the same as {live_letter}, to tune from there");
                kit::row(c, t, f, top, h, &Row::new(&title).sub(&sub));
            }
            Part::Row(Hit::Follow(bit)) => {
                if let Some((_, title, sub)) = FOLLOWS.iter().find(|(b, _, _)| *b == bit) {
                    kit::row(c, t, f, top, h, &Row::new(title).sub(sub).trail(Trail::Switch(p.follow & bit != 0)));
                }
            }
            Part::Note => {
                // What a profile HOLDS, said once. Without it the letters are just letters.
                let st = sty(Family::Sans, Weight::Regular, crate::scale::SECONDARY, t.dim, 0.0);
                let w = (kit::RIGHT - kit::LEFT) as f32;
                let l1 = fit(f, "A profile is the EQ, every effect and the balance.", &st, w);
                let l2 = fit(f, "Tap an output to switch its letter.", &st, w);
                text::draw(c, f, kit::LEFT as f32, (top + 26) as f32, &l1, &st);
                text::draw(c, f, kit::LEFT as f32, (top + 50) as f32, &l2, &st);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_dac_outranks_bluetooth_outranks_the_jack() {
        assert_eq!(output_for(false, false), Output::Jack);
        assert_eq!(output_for(false, true), Output::Bluetooth);
        assert_eq!(output_for(true, false), Output::UsbDac);
        assert_eq!(output_for(true, true), Output::UsbDac, "DAC mode bridging to BT is still DAC mode");
    }

    #[test]
    fn letters_round_trip_and_junk_is_refused() {
        for i in 0..2 {
            assert_eq!(parse_letter(letter(i)), Some(i));
            assert_eq!(parse_letter(&letter(i).to_lowercase()), Some(i));
        }
        for junk in ["", "c", "2", "AB", "-1"] {
            assert_eq!(parse_letter(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn the_summary_names_only_outputs_on_the_other_profile() {
        assert_eq!(summary([0, 0, 0], Output::Jack), "3.5 mm now \u{b7} every output uses A");
        assert_eq!(summary([0, 1, 0], Output::Jack), "3.5 mm now \u{b7} Bluetooth uses B");
        assert_eq!(summary([0, 1, 1], Output::Bluetooth), "Bluetooth now \u{b7} Headphone jack uses A");
    }

    /// Every output row and the copy row can be hit at its centre, and the rows do not overlap.
    #[test]
    fn every_control_hits_itself() {
        let mut seen = Vec::new();
        for h in Output::ALL
            .iter()
            .map(|o| Hit::Output(*o))
            .chain([Hit::Copy])
            .chain(FOLLOWS.iter().map(|(b, _, _)| Hit::Follow(*b)))
        {
            let y = centre(h);
            assert!(y > crate::chrome::HEADER_BOTTOM && y < crate::canvas::H as i32);
            assert_eq!(hit(240, y), Some(h));
            seen.push(y);
        }
        seen.dedup();
        assert_eq!(seen.len(), 7);
        // The strip and the section labels are not targets.
        assert_eq!(hit(240, crate::chrome::HEADER_BOTTOM + 4), None);
    }

    #[test]
    fn the_screen_fits_the_glass() {
        let (_, top, h) = *parts().last().unwrap();
        assert!(top + h <= crate::canvas::H as i32);
    }
}
