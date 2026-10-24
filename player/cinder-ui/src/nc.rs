//! Sound ▸ Noise cancelling — Sony's NC, for Sony's NC earphones.
//!
//! Reached from a Sound row that exists only while a 5-pin plug is in the jack (the codec's
//! `jack status se` reads `5pin`): those earphones carry the microphones NC listens through, and
//! nothing else on this jack does. Which of the three models it is cannot be read from the plug;
//! stock asks, and so does this screen. analysis/RE_noise_cancel.md.
//!
//! Each choice is one ALSA item the shell writes by name (`noise cancel mode`,
//! `noise cancel headphone type`), exactly as Sony's NcServiceImpl does.
//!
//! Layout is ONE function ([`parts`]) that both render and hit test walk, so they cannot drift.

use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::sty;
use crate::Canvas;

const NOTE: [&str; 2] = [
    "Sony's noise cancelling works only with these earphones.",
    "The plug cannot say which model it is, so pick yours.",
];

/// `(title, subtitle, noise cancel mode item)`. `airplane` is in the driver but not in Sony's
/// A50 menus (`MakeDriverParams` never writes it), so not here either.
pub const MODES: [(&str, &str, &str); 5] = [
    ("Off", "", "off"),
    ("Automatic", "Picks the setting for where you are", "ainc"),
    ("Bus or train", "", "train"),
    ("Office", "", "office"),
    (
        "Ambient sound",
        "Lets voices and announcements through",
        "ambient1",
    ),
];
pub const MODE_OFF: usize = 0;
/// Ambient sound: IER-NW500N only (Sony's `AsmCapable`), and only where Sony's ambient filter is
/// on the player — Walkman One deleted it.
pub const MODE_AMBIENT: usize = 4;

/// `(name, noise cancel headphone type item)`, Sony's `HeadphoneModel` order.
pub const MODELS: [(&str, &str); 3] = [
    ("IER-NW500N", "nw500n"),
    ("MDR-NW750N", "nw750n"),
    ("MDR-NC31", "nc31"),
];
pub const MODEL_NW500N: usize = 0;

/// What a tap lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Mode(usize),
    Model(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Label(&'static str),
    Row(Hit),
}

/// Can mode `m` run with these earphones on this player?
pub fn mode_ok(m: usize, model: usize, ambient_ok: bool) -> bool {
    m != MODE_AMBIENT || (ambient_ok && model == MODEL_NW500N)
}

fn parts(model: usize, ambient_ok: bool) -> Vec<(Part, i32, i32)> {
    let mut v = Vec::new();
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut push = |p: Part, h: i32, v: &mut Vec<(Part, i32, i32)>| {
        v.push((p, y, h));
        y += h;
    };
    push(Part::Label("MODE"), kit::SECTION_H, &mut v);
    for m in (0..MODES.len()).filter(|&m| mode_ok(m, model, ambient_ok)) {
        push(Part::Row(Hit::Mode(m)), kit::ROW_H, &mut v);
    }
    push(Part::Label("YOUR EARPHONES"), kit::SECTION_H, &mut v);
    for i in 0..MODELS.len() {
        push(Part::Row(Hit::Model(i)), kit::ROW_H, &mut v);
    }
    v
}

/// What is under `y`.
pub fn hit(y: i32, model: usize, ambient_ok: bool) -> Option<Hit> {
    parts(model, ambient_ok)
        .into_iter()
        .find_map(|(p, top, h)| match p {
            Part::Row(hit) if (top..top + h).contains(&y) => Some(hit),
            _ => None,
        })
}

/// Vertical centre of a control, for tests.
pub fn centre(h: Hit, model: usize, ambient_ok: bool) -> i32 {
    parts(model, ambient_ok)
        .into_iter()
        .find_map(|(p, top, ph)| (p == Part::Row(h)).then_some(top + ph / 2))
        .unwrap_or(0)
}

/// What the screen draws.
pub struct Nc {
    pub mode: usize,
    pub model: usize,
    pub ambient_ok: bool,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, n: &Nc) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Noise cancelling", Some("Sound"));
    let mut bottom = crate::chrome::HEADER_BOTTOM;
    for (part, top, h) in parts(n.model, n.ambient_ok) {
        match part {
            Part::Label(l) => {
                kit::section_label(c, t, f, top, l, None);
            }
            Part::Row(Hit::Mode(m)) => {
                let (title, sub, _) = MODES[m];
                let row = Row::new(title).sub(sub).trail(Trail::None).sel(n.mode == m);
                kit::row(c, t, f, top, h, &row);
            }
            Part::Row(Hit::Model(i)) => {
                let row = Row::new(MODELS[i].0).trail(Trail::None).sel(n.model == i);
                kit::row(c, t, f, top, h, &row);
            }
        }
        bottom = top + h;
    }
    let st = sty(
        Family::Sans,
        Weight::Regular,
        crate::scale::SECONDARY,
        t.dim,
        0.0,
    );
    // Wrapped, not cut, at a large text size: nothing sits under the note.
    let mut y = (bottom + 28) as f32;
    for line in NOTE {
        let n = crate::widgets::draw_wrap2(
            c,
            f,
            kit::LEFT as f32,
            y,
            crate::i18n::text(line),
            &st,
            kit::RIGHT as f32,
            20.0,
        );
        y += n as f32 * text::scaled(20.0) + 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_hits_itself_and_fits() {
        for model in 0..MODELS.len() {
            for amb in [false, true] {
                let mut hits: Vec<Hit> = (0..MODES.len())
                    .filter(|&m| mode_ok(m, model, amb))
                    .map(Hit::Mode)
                    .collect();
                hits.extend((0..MODELS.len()).map(Hit::Model));
                for h in hits {
                    assert_eq!(hit(centre(h, model, amb), model, amb), Some(h));
                }
                let (_, top, h) = *parts(model, amb).last().unwrap();
                assert!(top + h + 80 <= crate::canvas::H as i32);
            }
        }
    }

    #[test]
    fn ambient_needs_the_nw500n_and_its_filter() {
        assert!(mode_ok(MODE_AMBIENT, MODEL_NW500N, true));
        assert!(!mode_ok(MODE_AMBIENT, MODEL_NW500N, false));
        assert!(!mode_ok(MODE_AMBIENT, 1, true));
        assert!(mode_ok(1, 2, false));
    }
}
