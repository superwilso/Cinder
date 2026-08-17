//! Sound ▸ Advanced ▸ Tone Control — Sony's SECOND tone system.
//!
//! WHY THIS EXISTS SEPARATELY FROM THE EQUALIZER. Sony's own manual is explicit: the Equalizer and
//! the Tone Control are ALTERNATIVES whose settings are saved separately. `SetSelectUsingEq` picks
//! which one is actually in the signal path, and the ordinals were settled on device on
//! 2026-08-17 by reading the sound service's own `UpdateProcCond … isproc is N` log rather than by
//! ear:
//!
//! ```text
//!     EqType 0 -> nothing in the path      2 -> the 10-band Equalizer
//!            1 -> the 6-band EQ            3 -> Tone Control
//! ```
//!
//! Three bands, ±10 dB, and that is deliberately all: `SetToneCenterFreq` exists and echoes 0..7,
//! but it has no dB twin and no recovered frequency list, so a centre-frequency picker here would
//! be a control with numbers Cinder made up. It is left out until the Hz are read out of the
//! service's own `FS = [..] FREQ = [..]` log during playback.
//!
//! UNITS, MEASURED not assumed (`cinder-probe --tone`, 2026-08-17). The raw value the service
//! takes is in HALF-DECIBELS and the useful range is ±20 raw = ±10 dB — identical to the 10-band:
//!
//! ```text
//!     set  -20 -> dB -10.0      set  20 -> dB  10.0
//!     set  -24 -> dB   0.0      set  24 -> dB   0.0      <- NOT clamped: silently ZEROED
//! ```
//!
//! That second row is the reason [`BAND_MAX`] is a hard limit rather than a suggestion. Feed the
//! service a value past the end of its table and it does not clamp to the edge — it drops the band
//! to flat, so an over-enthusiastic "+12 dB" would read as a boost in the UI and be silence in the
//! DSP. Same trap as the 10-band, same guard.

use crate::canvas::W;
use crate::eq::band_db;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, right, sty};
use crate::Canvas;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

/// Catalogue order, and it IS the enum order: the sound service logs `eqtone,type=N` with N in
/// {0,1,2} as these three are written, so BASS=0, MIDDLE=1, TREBLE=2 is measured, not guessed.
pub const BAND_NAMES: [&str; 3] = ["BASS", "MIDDLE", "TREBLE"];
pub const BANDS: usize = 3;

/// Raw limits, in the same HALF-DECIBEL units as the 10-band. See the module note: past this the
/// service zeroes the band instead of clamping it.
pub const BAND_MAX: i8 = 20;
/// One tap = 1.0 dB, matching the Equalizer screen so the two feel like the same control.
pub const BAND_STEP: i8 = 2;

// ── Layout, shared by render and the hit test ────────────────────────────────────────────────
// Single source for both, for the reason the Sound screen learned the hard way: a hit test with
// its own copy of the geometry drifts, and a band you cannot land on is a band that isn't there.

/// The slider field.
pub const FIELD_TOP: i32 = 250;
pub const FIELD_BOTTOM: i32 = 570;
/// The zero line — and therefore the boundary between "tap to raise" and "tap to lower".
pub const FIELD_MID: i32 = (FIELD_TOP + FIELD_BOTTOM) / 2;
const BAND_X0: i32 = 45;
const BAND_SLOT: i32 = 130;

/// Centre x of band `i` (where its guide, knob and labels are drawn).
pub fn band_center_x(i: usize) -> i32 {
    BAND_X0 + i as i32 * BAND_SLOT + BAND_SLOT / 2
}

/// The band gain a finger at `y` is asking for — inverse of the knob placement in `render`,
/// snapped to [`BAND_STEP`]. Same helper as the Equalizer's, over this screen's own geometry;
/// see `eq::value_at_y` for why it is derived from the renderer's numbers rather than its own.
pub fn value_at_y(y: i32) -> i8 {
    let span = (FIELD_BOTTOM - FIELD_TOP) / 2 - 10;
    let raw = (FIELD_MID - y) * BAND_MAX as i32 / span.max(1);
    let snapped = (raw as f32 / BAND_STEP as f32).round() as i32 * BAND_STEP as i32;
    snapped.clamp(-(BAND_MAX as i32), BAND_MAX as i32) as i8
}

/// Which band column is under `x` within the field, if any.
pub fn band_at(x: i32) -> Option<usize> {
    let i = (x - BAND_X0).div_euclid(BAND_SLOT);
    (0..BANDS as i32).contains(&i).then_some(i as usize)
}

/// Footer controls.
pub const FOOTER_TOP: i32 = 700;
const FOOTER_H: i32 = 60;

/// Did this tap land on Reset? (The right half is a status label, not a button — the settings are
/// written as they change, exactly as on the Equalizer screen.)
pub fn reset_at(x: i32, y: i32) -> bool {
    (FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) && x < W as i32 / 2
}

fn disc(c: &mut Canvas, cx: i32, cy: i32, d: u32, col: embedded_graphics::pixelcolor::Rgb888) {
    Circle::with_center(Point::new(cx, cy), d)
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

/// What the screen draws.
pub struct Tone {
    pub bands: [i8; BANDS],
    /// Is Tone Control switched on? Off means every band below is stored and inert, which the
    /// screen says rather than pretending otherwise.
    pub on: bool,
    /// Name of whatever upstream control is bypassing the whole chain, if any (Source Direct,
    /// ClearAudio+). Same banner idea as the Advanced screen it is pushed from.
    pub overridden_by: Option<&'static str>,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, tc: &Tone, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Tone Control", Some("Advanced"));
    hline(c, y0, t.line);

    // ── the state line ──────────────────────────────────────────────────────────────────────
    // Three things can make these sliders inaudible and only one of them is on this screen, so
    // say which. Ordered by which you would have to undo FIRST.
    let (msg, col) = if let Some(who) = tc.overridden_by {
        (format!("{who} is on — nothing here is in the path"), t.acc)
    } else if !tc.on {
        ("Tone Control is off — switch it on in Advanced".to_string(), t.dim)
    } else {
        ("In the path, in place of the Equalizer".to_string(), t.faint)
    };
    text::draw(c, f, 22.0, (y0 + 26) as f32, &msg,
               &sty(Family::Sans, Weight::Regular, 13.0, col, 0.0));
    text::draw(c, f, 22.0, (y0 + 46) as f32,
               "Sony saves the two separately; only one is ever applied.",
               &sty(Family::Sans, Weight::Regular, 12.0, t.faint, 0.0));

    // ── the slider field ────────────────────────────────────────────────────────────────────
    let (sy, by) = (FIELD_TOP, FIELD_BOTTOM);
    let mid = FIELD_MID;
    let span = (by - sy) / 2 - 10;
    // Zero line, dashed, drawn the full width so the three columns read as one control.
    let mut dx = BAND_X0;
    while dx < BAND_X0 + BAND_SLOT * BANDS as i32 {
        fill_rect(c, dx, mid, 6, 1, t.line);
        dx += 12;
    }
    // A control that is not in the path is drawn in the faint ink rather than hidden: the values
    // are still yours, they are just not doing anything yet.
    let live = tc.on && tc.overridden_by.is_none();
    let ink_fill = if live { t.acc } else { t.faint };
    for i in 0..BANDS {
        let bx = band_center_x(i);
        let db = tc.bands[i] as i32;
        let knob_y = mid - db * span / BAND_MAX as i32;
        fill_rect(c, bx - 1, sy, 2, by - sy, t.line);
        let (fy, fh) = if knob_y < mid { (knob_y, mid - knob_y) } else { (mid, knob_y - mid) };
        fill_rect(c, bx - 1, fy, 2, fh, ink_fill);
        let on = i == sel;
        if on {
            disc(c, bx, knob_y, 30, t.ink);
        }
        disc(c, bx, knob_y, 24, t.bg);
        disc(c, bx, knob_y, if on { 18 } else { 15 }, ink_fill);

        // The REAL decibels. Printing the raw half-dB number would claim twice the boost the DSP
        // applies — the exact mistake the 10-band screen shipped with for a month.
        let dbl = match band_db(tc.bands[i]) {
            v if v == 0.0 => "0".to_string(),
            v if v.fract() == 0.0 => format!("{v:+.0}"),
            v => format!("{v:+.1}"),
        };
        let dbcol = if on { t.ink } else if db != 0 { ink_fill } else { t.faint };
        center(c, f, bx as f32, (sy - 10) as f32, &dbl,
               &sty(Family::Mono, Weight::Regular, if on { 14.0 } else { 13.0 }, dbcol, 0.0));
        center(c, f, bx as f32, (by + 26) as f32, BAND_NAMES[i],
               &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.0));
    }
    center(c, f, (W / 2) as f32, (by + 52) as f32, "dB   ·   tap above or below the line",
           &sty(Family::Sans, Weight::Regular, 12.0, t.faint, 0.0));

    // ── footer ──────────────────────────────────────────────────────────────────────────────
    let fy = FOOTER_TOP;
    hline(c, fy, t.line);
    let fcy = (fy + FOOTER_H / 2) as f32;
    text::draw(c, f, 22.0, fcy + 4.0, "Reset",
               &sty(Family::Sans, Weight::SemiBold, 16.0, t.dim, 0.0));
    right(c, f, 458.0, fcy + 4.0, "Saved automatically",
          &sty(Family::Sans, Weight::Regular, 14.0, t.faint, 0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every column must be hittable across its whole width, and the gutters must not be. A band
    /// you cannot land on is a band that does not exist on a device with no d-pad.
    #[test]
    fn every_band_is_hittable_across_its_column() {
        for b in 0..BANDS {
            let left = BAND_X0 + BAND_SLOT * b as i32;
            assert_eq!(band_at(left), Some(b), "left edge of band {b}");
            assert_eq!(band_at(left + BAND_SLOT / 2), Some(b), "middle of band {b}");
            assert_eq!(band_at(left + BAND_SLOT - 1), Some(b), "right edge of band {b}");
        }
        assert_eq!(band_at(BAND_X0 - 1), None, "left gutter");
        assert_eq!(band_at(BAND_X0 + BAND_SLOT * BANDS as i32), None, "right gutter");
    }

    /// The drawn knob for the maximum value must stay inside the field. The dB label sits above
    /// the knob, so a knob that reaches the top edge puts its own label off-screen.
    #[test]
    fn full_scale_knob_stays_inside_the_field() {
        let span = (FIELD_BOTTOM - FIELD_TOP) / 2 - 10;
        for &v in &[BAND_MAX, -BAND_MAX] {
            let knob_y = FIELD_MID - v as i32 * span / BAND_MAX as i32;
            assert!(knob_y > FIELD_TOP, "knob at {v} rides over the top of the field");
            assert!(knob_y < FIELD_BOTTOM, "knob at {v} rides under the bottom of the field");
        }
    }

    /// The step must divide the range exactly, or the top of the slider is unreachable by tapping.
    #[test]
    fn the_step_reaches_the_end_of_the_range() {
        assert_eq!(BAND_MAX % BAND_STEP, 0);
        let mut v: i8 = 0;
        for _ in 0..(BAND_MAX / BAND_STEP) {
            v = (v + BAND_STEP).min(BAND_MAX);
        }
        assert_eq!(v, BAND_MAX);
    }

    /// Raw is half-decibels — the whole reason the label is computed rather than printed raw.
    #[test]
    fn the_label_is_real_decibels() {
        assert_eq!(band_db(BAND_MAX), 10.0);
        assert_eq!(band_db(-BAND_MAX), -10.0);
        assert_eq!(band_db(BAND_STEP), 1.0);
    }

    /// The drag mapping must agree with the render: a value put through the knob placement and
    /// read back through `value_at_y` has to come out the same, or the knob does not sit under
    /// the finger.
    #[test]
    fn value_at_y_round_trips_through_the_knob_placement() {
        let span = (FIELD_BOTTOM - FIELD_TOP) / 2 - 10;
        let mut v = -BAND_MAX;
        while v <= BAND_MAX {
            let knob_y = FIELD_MID - v as i32 * span / BAND_MAX as i32;
            assert_eq!(value_at_y(knob_y), v, "round trip failed at {v}");
            v += BAND_STEP;
        }
    }

    /// Past the ends of the field the value clamps rather than running away — the service ZEROES
    /// an out-of-range gain, so an unclamped drag off the top would silence the band.
    #[test]
    fn dragging_past_the_field_clamps() {
        assert_eq!(value_at_y(FIELD_TOP - 200), BAND_MAX);
        assert_eq!(value_at_y(FIELD_BOTTOM + 200), -BAND_MAX);
    }

    /// Reset is the left half of the footer only; the right half is a status label.
    #[test]
    fn reset_is_the_left_half_of_the_footer() {
        assert!(reset_at(20, FOOTER_TOP + 10));
        assert!(!reset_at(400, FOOTER_TOP + 10));
        assert!(!reset_at(20, FOOTER_TOP - 10));
    }
}
