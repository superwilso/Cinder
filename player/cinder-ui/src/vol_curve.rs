//! Sound ▸ Advanced ▸ Volume curve — the owner's own headphone volume curve.
//!
//! WHAT IT EDITS. Sony's volume table gives each of the 121 volume steps one row of codec register
//! values. This screen picks, for volume 0, 20, 40 … 120, WHICH of Sony's rows that step plays; the
//! steps between follow a straight line. The setuid `cinder-voltable curve stock P0 … P120` builds
//! the table from the player's own one (`cinder-home/src/vol_curve.h`), so nothing is ever played
//! that Sony's curve does not already contain, and the maximum cannot rise above Sony's.
//!
//! WHY. The stock A50 curve has dead steps: nothing changes from 100 to 120, and the analogue stage
//! sits still from 40 to 60 (`analysis/RE_volume_pop.md`). "Use every step" spreads 0..120 over
//! Sony's rows 0..100, so every press changes the level and the steps near the top are finer.
//!
//! A point can never be set below the one before it or above the one after: a curve that goes down
//! would make the volume control run backwards, and the helper refuses one anyway.

use crate::canvas::W;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, disc, fill_rect, hline, right, sty};
use crate::Canvas;

/// Volume 0, 20, … 120: the helper's argument order.
pub const POINTS: usize = 7;
/// Sony's rows, and the volume numbers, run 0..=120.
pub const MAX: u8 = 120;
/// Every step plays itself: Sony's own curve.
pub const STRAIGHT: [u8; POINTS] = [0, 20, 40, 60, 80, 100, 120];
/// 0..120 over Sony's rows 0..100: no dead steps at the top.
pub const EVERY_STEP: [u8; POINTS] = [0, 17, 33, 50, 67, 83, 100];

pub const FIELD_TOP: i32 = 250;
pub const FIELD_BOTTOM: i32 = 600;
const TRAVEL: i32 = FIELD_BOTTOM - FIELD_TOP - 20;
const X0: i32 = 16;
const SLOT: i32 = 64;

/// Where the knob for row `v` sits.
pub fn knob_y(v: u8) -> i32 {
    FIELD_BOTTOM - 10 - v as i32 * TRAVEL / MAX as i32
}

/// The row a finger at `y` is asking for, clamped to 0..=120.
pub fn value_at_y(y: i32) -> u8 {
    let raw = ((FIELD_BOTTOM - 10 - y) as f32 * MAX as f32 / TRAVEL as f32).round() as i32;
    raw.clamp(0, MAX as i32) as u8
}

pub fn point_x(i: usize) -> i32 {
    X0 + i as i32 * SLOT + SLOT / 2
}

/// Which point's column is under `x`.
pub fn point_at(x: i32) -> Option<usize> {
    let i = (x - X0).div_euclid(SLOT);
    (0..POINTS as i32).contains(&i).then_some(i as usize)
}

/// Set point `i` to `v`, held between its neighbours so the curve never goes down.
pub fn set(pts: &mut [u8; POINTS], i: usize, v: u8) {
    let lo = if i > 0 { pts[i - 1] } else { 0 };
    let hi = if i + 1 < POINTS { pts[i + 1] } else { MAX };
    pts[i] = v.clamp(lo, hi);
}

/// A curve from the settings file, which any PC can write: Some only when it is one the helper
/// will take.
pub fn valid(pts: &[u8; POINTS]) -> bool {
    pts.iter().all(|&p| p <= MAX) && pts.windows(2).all(|w| w[0] <= w[1])
}

/// The "Use this curve" switch row.
pub const SWITCH_TOP: i32 = crate::chrome::HEADER_BOTTOM + 66;
pub const SWITCH_H: i32 = 64;
pub fn switch_at(y: i32) -> bool {
    (SWITCH_TOP..SWITCH_TOP + SWITCH_H).contains(&y)
}

pub const FOOTER_TOP: i32 = 700;
const FOOTER_H: i32 = 60;
/// Reset: the footer's left half.
pub fn reset_at(x: i32, y: i32) -> bool {
    (FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) && x < W as i32 / 2
}
/// Use every step: the right half.
pub fn every_at(x: i32, y: i32) -> bool {
    (FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) && x >= W as i32 / 2
}

pub struct VolCurve {
    pub pts: [u8; POINTS],
    /// The curve is in use (written to the codec and applied at every start).
    pub on: bool,
    /// `cinder-voltable` is installed: the volume-curve option.
    pub available: bool,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &VolCurve, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(
        c,
        t,
        f,
        crate::i18n::text("Volume curve"),
        Some(crate::i18n::text("Advanced")),
    );
    hline(c, y0, t.line);
    let st = sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0);
    let (msg, col) = if v.available {
        (
            crate::i18n::text("Which of Sony's steps each volume number plays"),
            t.faint,
        )
    } else {
        (
            crate::i18n::text("Needs the installer's volume curve option"),
            t.acc,
        )
    };
    let s = crate::widgets::fit(
        f,
        msg,
        &sty(Family::Sans, Weight::Regular, 13.0, col, 0.0),
        436.0,
    );
    text::draw(
        c,
        f,
        22.0,
        (y0 + 26) as f32,
        &s,
        &sty(Family::Sans, Weight::Regular, 13.0, col, 0.0),
    );
    let s = crate::widgets::fit(
        f,
        crate::i18n::text("Headphone jack only. Never louder than Sony's maximum."),
        &st,
        436.0,
    );
    text::draw(c, f, 22.0, (y0 + 46) as f32, &s, &st);
    crate::kit::row(
        c,
        t,
        f,
        SWITCH_TOP,
        SWITCH_H,
        &crate::kit::Row::new(crate::i18n::text("Use this curve"))
            .sub(crate::i18n::text("Off: the curve chosen in the installer"))
            .trail(crate::kit::Trail::Switch(v.on)),
    );

    // The straight line (Sony's own curve) under the knobs, for comparison.
    let ink = if v.on && v.available { t.acc } else { t.faint };
    for i in 0..POINTS - 1 {
        let (xa, xb) = (point_x(i), point_x(i + 1));
        let (ya, yb) = (knob_y(v.pts[i]), knob_y(v.pts[i + 1]));
        let (sa, sb) = (knob_y(STRAIGHT[i]), knob_y(STRAIGHT[i + 1]));
        for x in xa..xb {
            let k = (x - xa) as f32 / (xb - xa) as f32;
            fill_rect(
                c,
                x,
                (sa as f32 + (sb - sa) as f32 * k) as i32,
                1,
                1,
                t.line,
            );
            fill_rect(
                c,
                x,
                (ya as f32 + (yb - ya) as f32 * k) as i32 - 1,
                1,
                3,
                ink,
            );
        }
    }
    for i in 0..POINTS {
        let (bx, ky) = (point_x(i), knob_y(v.pts[i]));
        let on = i == sel;
        if on {
            disc(c, bx, ky, 26, t.ink);
        }
        disc(c, bx, ky, 20, t.bg);
        disc(c, bx, ky, if on { 15 } else { 12 }, ink);
        center(
            c,
            f,
            bx as f32,
            (FIELD_TOP - 10) as f32,
            &v.pts[i].to_string(),
            &sty(
                Family::Mono,
                Weight::Regular,
                13.0,
                if on { t.ink } else { t.dim },
                0.0,
            ),
        );
        center(
            c,
            f,
            bx as f32,
            (FIELD_BOTTOM + 26) as f32,
            &(i * 20).to_string(),
            &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.0),
        );
    }
    center(
        c,
        f,
        (W / 2) as f32,
        (FIELD_BOTTOM + 52) as f32,
        crate::i18n::text("Volume number below · Sony's step above"),
        &sty(Family::Sans, Weight::Regular, 12.0, t.faint, 0.0),
    );

    let fcy = (FOOTER_TOP + FOOTER_H / 2) as f32;
    hline(c, FOOTER_TOP, t.line);
    text::draw(
        c,
        f,
        22.0,
        fcy + 4.0,
        crate::i18n::text("Reset"),
        &sty(Family::Sans, Weight::SemiBold, 16.0, t.dim, 0.0),
    );
    right(
        c,
        f,
        458.0,
        fcy + 4.0,
        crate::i18n::text("Use every step"),
        &sty(Family::Sans, Weight::SemiBold, 16.0, t.acc, 0.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_stays_between_its_neighbours() {
        let mut p = STRAIGHT;
        set(&mut p, 3, 10);
        assert_eq!(p[3], 40, "not below the point before");
        set(&mut p, 3, 119);
        assert_eq!(p[3], 80, "not above the point after");
        set(&mut p, 6, 200);
        assert_eq!(p[6], MAX);
        assert!(valid(&p) && valid(&EVERY_STEP) && !valid(&[0, 30, 20, 60, 80, 100, 120]));
    }

    #[test]
    fn the_knob_sits_under_the_finger() {
        for v in 0..=MAX {
            assert_eq!(value_at_y(knob_y(v)), v);
        }
        assert_eq!(value_at_y(FIELD_TOP - 300), MAX);
        assert_eq!(value_at_y(FIELD_BOTTOM + 300), 0);
        assert!(knob_y(MAX) > FIELD_TOP && knob_y(0) < FIELD_BOTTOM);
    }

    #[test]
    fn every_column_is_hittable_and_on_the_panel() {
        for i in 0..POINTS {
            assert_eq!(point_at(point_x(i)), Some(i));
        }
        assert_eq!(point_at(X0 - 1), None);
        assert!(X0 + SLOT * POINTS as i32 <= W as i32);
    }
}
