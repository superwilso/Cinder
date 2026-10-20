//! Sound ▸ Advanced ▸ DAC EQ — five filters in the codec chip itself.
//!
//! WHAT THIS IS. The CXD3778GF has its own five-biquad EQ, loaded by the kernel from the "tone
//! control table" and re-applied on every output, amp, jack and resume change. Sony uses it only to
//! correct its own noise-cancelling headphones; for every other pair it is loaded flat. Cinder
//! fills the ordinary-headphone slots with this screen's five bands, through the setuid
//! `cinder-voltable eq` helper, which computes the coefficients itself from five whole numbers.
//! Decode and kernel trace: `analysis/RE_codec_tone_table.md`; the table builder and its self-test:
//! `cinder-home/src/codec_eq.h`, `cinder-home/tools/codeceq_selftest.cpp`.
//!
//! HOW IT DIFFERS FROM THE EQUALIZER AND TONE CONTROL. Those run in Sony's DSP, on the CPU, and
//! apply to Bluetooth as well. This one runs in the DAC after all of them, on the wired output
//! only, and is in the path with either of them.
//!
//! ASYMMETRIC RANGE, ON PURPOSE: -12 dB to +6 dB. The helper never lets the curve rise above 0 dB —
//! a boost is paid for by lowering everything else by the same amount — so a big boost buys mostly
//! a quieter player. Cuts are free.
//!
//! Source Direct flattens it (the shell applies a flat table while Direct is on): "bypasses every
//! effect" has to include this one.

use crate::canvas::W;
use crate::eq::band_db;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, disc, fill_rect, hline, right, sty};
use crate::Canvas;

/// Band order is the helper's argument order (`cinder-voltable eq G1..G5`), and the frequencies are
/// fixed in `codec_eq.h`: a low shelf at 100 Hz, peaks at 400 Hz, 1.5 kHz and 4 kHz, a high shelf
/// at 10 kHz.
pub const BANDS: usize = 5;
pub const BAND_NAMES: [&str; BANDS] = ["BASS", "400", "1.5K", "4K", "TREBLE"];

/// Raw limits, HALF-decibels — the same unit as the Equalizer and Tone Control, and the helper's
/// own `CODEC_EQ_GAIN_MIN`/`_MAX`. Keep the three in step: the helper REJECTS a value outside its
/// range (rc 2) rather than clamping it.
pub const BAND_MIN: i8 = -24;
pub const BAND_MAX: i8 = 12;
/// One tap = 1.0 dB, like the other two band editors.
pub const BAND_STEP: i8 = 2;

// ── Layout, shared by render and the hit test ────────────────────────────────────────────────

/// The slider field. The zero line is NOT the middle: the range is 12 dB down and 6 up, so the
/// line sits a third of the way down and a raw step is the same height either side of it.
pub const FIELD_TOP: i32 = 250;
pub const FIELD_BOTTOM: i32 = 570;
/// Pixels of travel for the whole range, inside a 10 px margin at each end.
const TRAVEL: i32 = FIELD_BOTTOM - FIELD_TOP - 20;
const RANGE: i32 = BAND_MAX as i32 - BAND_MIN as i32;
/// The zero line.
pub const FIELD_ZERO: i32 = FIELD_TOP + 10 + BAND_MAX as i32 * TRAVEL / RANGE;
const BAND_X0: i32 = 15;
const BAND_SLOT: i32 = 90;

/// Where the knob for raw value `v` sits.
pub fn knob_y(v: i8) -> i32 {
    FIELD_ZERO - v as i32 * TRAVEL / RANGE
}

/// Centre x of band `i`.
pub fn band_center_x(i: usize) -> i32 {
    BAND_X0 + i as i32 * BAND_SLOT + BAND_SLOT / 2
}

/// The band gain a finger at `y` is asking for — the inverse of [`knob_y`], snapped to
/// [`BAND_STEP`] and clamped to the range.
pub fn value_at_y(y: i32) -> i8 {
    let raw = (FIELD_ZERO - y) as f32 * RANGE as f32 / TRAVEL as f32;
    let snapped = (raw / BAND_STEP as f32).round() as i32 * BAND_STEP as i32;
    snapped.clamp(BAND_MIN as i32, BAND_MAX as i32) as i8
}

/// Which band column is under `x`, if any.
pub fn band_at(x: i32) -> Option<usize> {
    let i = (x - BAND_X0).div_euclid(BAND_SLOT);
    (0..BANDS as i32).contains(&i).then_some(i as usize)
}

/// Footer controls.
pub const FOOTER_TOP: i32 = 700;
const FOOTER_H: i32 = 60;

/// Did this tap land on Reset (the footer's left half)?
pub fn reset_at(x: i32, y: i32) -> bool {
    (FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) && x < W as i32 / 2
}

/// Did this tap land on "Fit my 10-band" (the footer's right half)?
pub fn fit_at(x: i32, y: i32) -> bool {
    (FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) && x >= W as i32 / 2
}

/// The "Use on the headphone jack" switch row, under the two lines of text.
pub const JACK_ROW_TOP: i32 = crate::chrome::HEADER_BOTTOM + 66;
pub const JACK_ROW_H: i32 = 64;
pub fn jack_row_at(y: i32) -> bool {
    (JACK_ROW_TOP..JACK_ROW_TOP + JACK_ROW_H).contains(&y)
}

/// The 10-band curve brought to the DAC's five bands: each DAC band takes the mean of the 10-band
/// bands nearest it (100 Hz shelf: 32/64/125; 400: 250/500; 1.5k: 1k/2k; 4k: 4k; 10k shelf:
/// 8k/16k), snapped to the editor's 1 dB step and clamped to the DAC's range. Both are half-dB.
pub fn fit(eq10: [i8; 10]) -> [i8; BANDS] {
    const GROUPS: [&[usize]; BANDS] = [&[0, 1, 2], &[3, 4], &[5, 6], &[7], &[8, 9]];
    let mut out = [0i8; BANDS];
    for (o, g) in out.iter_mut().zip(GROUPS) {
        let mean = g.iter().map(|&i| eq10[i] as f32).sum::<f32>() / g.len() as f32;
        let snapped = (mean / BAND_STEP as f32).round() as i32 * BAND_STEP as i32;
        *o = snapped.clamp(BAND_MIN as i32, BAND_MAX as i32) as i8;
    }
    out
}

/// Clamp a raw gain into the range — for values arriving from the settings file, which any PC can
/// write.
pub fn clamp(v: i8) -> i8 {
    v.clamp(BAND_MIN, BAND_MAX)
}

/// What the screen draws.
pub struct DacEq {
    pub bands: [i8; BANDS],
    /// Source Direct is on, so the shell is holding the DAC EQ flat.
    pub direct: bool,
    /// This is the jack's equalizer (Sony's 10-band held flat there).
    pub jack: bool,
    /// The screen it was opened from, for the header's back label.
    pub back: &'static str,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, d: &DacEq, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, crate::i18n::text("DAC EQ"), Some(d.back));
    hline(c, y0, t.line);

    const AVAIL: f32 = 436.0;
    let mut line = |dy: i32, s: &str, st: crate::text::TextStyle| {
        let s = crate::widgets::fit(f, s, &st, AVAIL);
        text::draw(c, f, 22.0, (y0 + dy) as f32, &s, &st);
    };
    let (msg, col) = if d.direct {
        (
            crate::i18n::text("Source Direct is on — held flat until it is off"),
            t.acc,
        )
    } else {
        (
            crate::i18n::text("In the DAC chip, after every other effect"),
            t.faint,
        )
    };
    line(26, msg, sty(Family::Sans, Weight::Regular, 13.0, col, 0.0));
    line(
        46,
        crate::i18n::text("Wired headphones only. A boost lowers everything else."),
        sty(Family::Sans, Weight::Regular, 12.0, t.faint, 0.0),
    );
    let jsub = if d.jack {
        "The jack's equalizer \u{b7} the 10-band stays for Bluetooth"
    } else {
        crate::i18n::text("Off: the 10-band runs on the jack too, on the processor")
    };
    crate::kit::row(
        c,
        t,
        f,
        JACK_ROW_TOP,
        JACK_ROW_H,
        &crate::kit::Row::new(crate::i18n::text("Use on the headphone jack"))
            .sub(jsub)
            .trail(crate::kit::Trail::Switch(d.jack)),
    );

    // ── the slider field ────────────────────────────────────────────────────────────────────
    let (sy, by) = (FIELD_TOP, FIELD_BOTTOM);
    let mid = FIELD_ZERO;
    let mut dx = BAND_X0;
    while dx < BAND_X0 + BAND_SLOT * BANDS as i32 {
        fill_rect(c, dx, mid, 6, 1, t.line);
        dx += 12;
    }
    let live = !d.direct;
    let ink_fill = if live { t.acc } else { t.faint };
    for i in 0..BANDS {
        let bx = band_center_x(i);
        let ky = knob_y(d.bands[i]);
        fill_rect(c, bx - 1, sy, 2, by - sy, t.line);
        let (fy, fh) = if ky < mid {
            (ky, mid - ky)
        } else {
            (mid, ky - mid)
        };
        fill_rect(c, bx - 1, fy, 2, fh, ink_fill);
        let on = i == sel;
        if on {
            disc(c, bx, ky, 30, t.ink);
        }
        disc(c, bx, ky, 24, t.bg);
        disc(c, bx, ky, if on { 18 } else { 15 }, ink_fill);

        let dbl = match band_db(d.bands[i]) {
            v if v == 0.0 => "0".to_string(),
            v if v.fract() == 0.0 => format!("{v:+.0}"),
            v => format!("{v:+.1}"),
        };
        let dbcol = if on {
            t.ink
        } else if d.bands[i] != 0 {
            ink_fill
        } else {
            t.faint
        };
        center(
            c,
            f,
            bx as f32,
            (sy - 10) as f32,
            &dbl,
            &sty(
                Family::Mono,
                Weight::Regular,
                if on { 14.0 } else { 13.0 },
                dbcol,
                0.0,
            ),
        );
        center(
            c,
            f,
            bx as f32,
            (by + 26) as f32,
            BAND_NAMES[i],
            &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.0),
        );
    }
    center(
        c,
        f,
        (W / 2) as f32,
        (by + 52) as f32,
        crate::i18n::text("Bass 100 Hz · treble 10 kHz · +6 to −12 dB"),
        &sty(Family::Sans, Weight::Regular, 12.0, t.faint, 0.0),
    );

    // ── footer ──────────────────────────────────────────────────────────────────────────────
    let fy = FOOTER_TOP;
    hline(c, fy, t.line);
    let fcy = (fy + FOOTER_H / 2) as f32;
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
        crate::i18n::text("Fit my 10-band curve"),
        &sty(Family::Sans, Weight::SemiBold, 16.0, t.acc, 0.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat curve fits flat; a bass boost lands in the bass shelf; nothing leaves the range.
    #[test]
    fn fitting_the_10_band_curve() {
        assert_eq!(fit([0; 10]), [0; BANDS]);
        assert_eq!(fit([12, 12, 12, 0, 0, 0, 0, 0, 0, 0]), [12, 0, 0, 0, 0]);
        assert_eq!(
            fit([0, 0, 0, 0, 0, 0, 0, -20, -20, -20]),
            [0, 0, 0, -20, -20]
        );
        assert_eq!(fit([20; 10]), [BAND_MAX; BANDS], "boosts clamp at +6 dB");
        assert_eq!(
            fit([3, 3, 3, 0, 0, 0, 0, 0, 0, 0])[0] % BAND_STEP,
            0,
            "snapped to 1 dB"
        );
    }

    #[test]
    fn every_band_is_hittable_across_its_column() {
        for b in 0..BANDS {
            let left = BAND_X0 + BAND_SLOT * b as i32;
            assert_eq!(band_at(left), Some(b), "left edge of band {b}");
            assert_eq!(
                band_at(left + BAND_SLOT - 1),
                Some(b),
                "right edge of band {b}"
            );
        }
        assert_eq!(band_at(BAND_X0 - 1), None, "left gutter");
        assert_eq!(
            band_at(BAND_X0 + BAND_SLOT * BANDS as i32),
            None,
            "right gutter"
        );
        assert!(
            BAND_X0 + BAND_SLOT * BANDS as i32 <= W as i32,
            "last column runs off the panel"
        );
    }

    /// Both ends of the range keep the knob (and so its label above) inside the field.
    #[test]
    fn full_scale_knobs_stay_inside_the_field() {
        for v in [BAND_MIN, BAND_MAX] {
            let y = knob_y(v);
            assert!(
                y > FIELD_TOP && y < FIELD_BOTTOM,
                "knob at {v} leaves the field ({y})"
            );
        }
        assert!(knob_y(BAND_MAX) < FIELD_ZERO && FIELD_ZERO < knob_y(BAND_MIN));
    }

    /// The knob sits under the finger: every reachable value survives render -> hit test.
    #[test]
    fn value_at_y_round_trips_through_the_knob_placement() {
        let mut v = BAND_MIN;
        while v <= BAND_MAX {
            assert_eq!(value_at_y(knob_y(v)), v, "round trip failed at {v}");
            v += BAND_STEP;
        }
    }

    #[test]
    fn dragging_past_the_field_clamps() {
        assert_eq!(value_at_y(FIELD_TOP - 300), BAND_MAX);
        assert_eq!(value_at_y(FIELD_BOTTOM + 300), BAND_MIN);
    }

    /// The step has to land exactly on both ends, or one end is unreachable by tapping.
    #[test]
    fn the_step_reaches_both_ends() {
        assert_eq!(BAND_MAX % BAND_STEP, 0);
        assert_eq!(BAND_MIN % BAND_STEP, 0);
    }

    /// The helper's range, restated: it rejects anything outside -24..=12 (codec_eq.h), so the UI
    /// must never be able to produce such a value.
    #[test]
    fn the_range_is_the_helpers() {
        assert_eq!((BAND_MIN, BAND_MAX), (-24, 12));
        assert_eq!(clamp(i8::MIN), BAND_MIN);
        assert_eq!(clamp(i8::MAX), BAND_MAX);
        assert_eq!(band_db(BAND_MAX), 6.0);
        assert_eq!(band_db(BAND_MIN), -12.0);
    }

    #[test]
    fn reset_is_the_left_half_of_the_footer() {
        assert!(reset_at(20, FOOTER_TOP + 10));
        assert!(!reset_at(400, FOOTER_TOP + 10));
        assert!(!reset_at(20, FOOTER_TOP - 10));
    }
}
