//! Sound ▸ Advanced — the rest of Sony's effect surface.
//!
//! WHY A SUB-SCREEN. The Sound screen is full: six effect rows at 64 px, a 132 px balance slider,
//! and the signal-path footer at y=700. Sony exposes far more than fits there, and making that
//! list scroll would put a scroll offset into the one screen whose render and hit test have already
//! drifted apart once. A pushed screen is additive — `ClockSet`, `Folders` and `TrackInfo` are the
//! precedent — and it also says something true about these controls: they are the ones you set once
//! for a pair of headphones, not the ones you reach for per album.
//!
//! WHAT IS AND IS NOT HERE. Everything on this screen is a control Sony's own firmware exposes and
//! Cinder did not. Deliberately absent:
//!   * ClearPhase Speaker / Wmport — the symbols exist, but they describe hardware an A55 does not
//!     have (same story as `smaster btl`). A row that cannot do anything is the exact class of lie
//!     this screen's neighbours have had cleaned out of them twice.
//!   * Tone Control's CENTRE FREQUENCIES. The three band gains got their own editor on 2026-08-17
//!     (`tone.rs`, reached from the row below the on/off); the centre frequencies did not, because
//!     `SetToneCenterFreq` echoes 0..7 with no dB twin and no recovered frequency list, so the
//!     picker would be showing numbers Cinder invented.
//!   * The 6-band EQ and Sony's named presets — those belong next to the EQ screen, not here.
//!
//! THE OVERRIDE BANNER is the point of the screen as much as the rows are. ClearAudio+ overrides
//! the manual EQ and DSP outright, and Source Direct bypasses the whole chain — measured on device,
//! and the reason a whole evening went into "why can I not hear VPT". When either is on, the rows
//! it hides are drawn dim and the banner says which one is doing it.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, sty, toggle};
use crate::Canvas;

/// Rows: Source Direct, Clear Phase, DSEE AI, DSEE HX Custom, Vinyl character, Tone Control,
/// and the route to the Tone Control band editor.
pub const ROWS: usize = 7;
pub const ROW_SOURCE_DIRECT: usize = 0;
pub const ROW_CLEAR_PHASE: usize = 1;
pub const ROW_DSEE_AI: usize = 2;
pub const ROW_DSEE_CUSTOM: usize = 3;
pub const ROW_VINYL_TYPE: usize = 4;
pub const ROW_TONE: usize = 5;
pub const ROW_TONE_BANDS: usize = 6;

/// Row pitch and list top — SINGLE SOURCE for the render below and `nav`'s hit test. The banner
/// sits BELOW the rows so it cannot shift them: a row that moves when an unrelated toggle flips is
/// how a hit test drifts out of step with what is drawn.
pub const ROW_H: i32 = 64;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// Which row is under `y`, or None if the point is outside the list.
pub fn row_at(y: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / ROW_H) as usize;
    (r < ROWS).then_some(r)
}

/// DSEE HX Custom modes, catalogue order — the index IS the value handed to `SetDseeHxCustomMode`.
/// Same provisional-label caveat as `nav::VPT_MODES`: catalogue order is almost certainly enum
/// order, but only listening settles it.
pub const DSEE_MODES: [&str; 5] = ["Standard", "Female Vocal", "Male Vocal", "Percussion", "Strings"];

/// Vinylizer characters, catalogue order — the index IS the value for `SetVinylizerType`.
///
/// Worth wiring for its own sake: the device read back type=7 for this, which is not a member of a
/// four-value enum at all. Nothing had ever set it, so it was whatever the service happened to hold.
pub const VINYL_TYPES: [&str; 4] = ["Standard", "Turntable", "Arm Resonance", "Surface Noise"];

/// What the screen draws. Every field is Copy so a preview or a test can spin off a variant with
/// struct-update syntax.
#[derive(Clone, Copy)]
pub struct Advanced {
    pub source_direct: bool,
    pub clear_phase: bool,
    pub dsee_ai: bool,
    /// "Off", or one of [`DSEE_MODES`].
    pub dsee_custom: &'static str,
    /// One of [`VINYL_TYPES`]. Only in the path while the Vinyl Processor itself is on, which is
    /// on the Sound screen — said so in the row's subtitle rather than hiding the row, because
    /// "set it up, then switch it on" is a reasonable order to do things in.
    pub vinyl_type: &'static str,
    pub vinyl_on: bool,
    pub tone_control: bool,
    /// Name of whatever upstream control is currently overriding the rest of the chain, if any.
    pub overridden_by: Option<&'static str>,
}

/// Is this row one of the ones an upstream override hides? Source Direct itself never dims — it is
/// the thing doing the overriding, so greying it out would strand the user with no way back.
fn dimmed(a: &Advanced, row: usize) -> bool {
    a.overridden_by.is_some() && row != ROW_SOURCE_DIRECT
}

/// A row drawn dim, for when something upstream has taken it out of the path. Same geometry as
/// `sound::row` — it just draws the label in the faint ink so the screen reads as "these are not
/// doing anything right now" at a glance rather than after reading a footnote.
fn dim_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str, desc: &str) -> i32 {
    let cy = y + ROW_H / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, ROW_H, t.row_sel);
    }
    text::draw(c, f, 22.0, (cy - 3) as f32, label,
               &sty(Family::Sans, Weight::SemiBold, 18.0, t.faint, 0.0));
    text::draw(c, f, 22.0, (cy + 15) as f32, desc,
               &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
    hline(c, y + ROW_H, t.line);
    cy
}

fn a_row(
    c: &mut Canvas, t: &Theme, f: &FontSet, a: &Advanced, y: i32, sel: bool, row: usize,
    label: &str, desc: &str,
) -> i32 {
    if dimmed(a, row) {
        dim_row(c, t, f, y, sel, label, desc)
    } else {
        crate::sound::row(c, t, f, y, sel, label, desc)
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, a: &Advanced, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Advanced", Some("Sound"));
    debug_assert_eq!(y0, TOP, "advanced list top drifted from the hit test");
    hline(c, y0, t.line);

    let rh = ROW_H;

    let cy = a_row(c, t, f, a, y0, sel == 0, ROW_SOURCE_DIRECT,
                   "Source Direct", "Shortest path — bypasses every effect");
    toggle(c, t, 418, cy - 11, 40, 22, 14, a.source_direct);

    let cy = a_row(c, t, f, a, y0 + rh, sel == 1, ROW_CLEAR_PHASE,
                   "Clear Phase", "Correct headphone phase response");
    toggle(c, t, 418, cy - 11, 40, 22, 14, a.clear_phase);

    let cy = a_row(c, t, f, a, y0 + rh * 2, sel == 2, ROW_DSEE_AI,
                   "DSEE AI", "Upscaling with real-time source analysis");
    toggle(c, t, 418, cy - 11, 40, 22, 14, a.dsee_ai);

    let cy = a_row(c, t, f, a, y0 + rh * 3, sel == 3, ROW_DSEE_CUSTOM,
                   "DSEE HX Custom", "Tune the upscaler to the material");
    crate::sound::value_pill(c, f, t, 458, cy, a.dsee_custom);

    let vdesc = if a.vinyl_on { "Character of the vinyl emulation" } else { "Vinyl Processor is off" };
    let cy = a_row(c, t, f, a, y0 + rh * 4, sel == 4, ROW_VINYL_TYPE, "Vinyl Character", vdesc);
    crate::sound::value_pill(c, f, t, 458, cy, a.vinyl_type);

    let cy = a_row(c, t, f, a, y0 + rh * 5, sel == 5, ROW_TONE,
                   "Tone Control", "Bass / mid / treble — alternative to the EQ");
    toggle(c, t, 418, cy - 11, 40, 22, 14, a.tone_control);

    // The band editor. A ROUTE, not a setting — the same shape as the Sound screen's own
    // "Advanced ›" row. It stays reachable while Tone Control is off, because "set it up, then
    // switch it on" is a reasonable order to do things in — the Vinyl Character row above says
    // the same thing about the Vinyl Processor.
    let bdesc = if a.tone_control { "Three bands, ±10 dB" } else { "Three bands — Tone Control is off" };
    let cy = a_row(c, t, f, a, y0 + rh * 6, sel == 6, ROW_TONE_BANDS, "Adjust bands", bdesc);
    let chev = sty(Family::Sans, Weight::SemiBold, 20.0, t.dim, 0.0);
    crate::widgets::right(c, f, 458.0, (cy + 7) as f32, "\u{203A}", &chev);

    // ── the override banner ─────────────────────────────────────────────────────────────────
    // Below the rows, so nothing above it moves when it appears.
    let by = y0 + rh * ROWS as i32 + 18;
    // Every line here goes through `fit`: the banner carries a control's NAME, so its width is not
    // something this file gets to assume, and at 140% UI scale the one-line version ran 156 px past
    // the margin — caught by tests/ui_overflow.rs, which is why it sweeps this screen with the
    // banner up rather than only in its default state.
    const AVAIL: f32 = 436.0; // 22 px gutter each side
    let mut line = |dy: i32, s: &str, st: crate::text::TextStyle| {
        let s = crate::widgets::fit(f, s, &st, AVAIL);
        text::draw(c, f, 22.0, (by + dy) as f32, &s, &st);
    };
    if let Some(who) = a.overridden_by {
        line(14, &format!("{who} is on"),
             sty(Family::Sans, Weight::SemiBold, 13.0, t.acc, 0.0));
        line(34, "The effects above are not in the path.",
             sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
    } else {
        line(14, "Set these once for a pair of headphones.",
             sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Advanced {
        Advanced {
            source_direct: false, clear_phase: false, dsee_ai: false,
            dsee_custom: "Off", vinyl_type: VINYL_TYPES[0], vinyl_on: false,
            tone_control: false, overridden_by: None,
        }
    }

    /// The hit test must agree with the drawn geometry for every row, including the edges — this
    /// screen is touch-only and a row you cannot land on is a row that does not exist.
    #[test]
    fn every_row_is_hittable_at_its_own_band() {
        for r in 0..ROWS {
            let top = TOP + ROW_H * r as i32;
            assert_eq!(row_at(top), Some(r), "top edge of row {r}");
            assert_eq!(row_at(top + ROW_H / 2), Some(r), "middle of row {r}");
            assert_eq!(row_at(top + ROW_H - 1), Some(r), "bottom edge of row {r}");
        }
        assert_eq!(row_at(TOP - 1), None, "above the list");
        assert_eq!(row_at(TOP + ROW_H * ROWS as i32), None, "below the list");
    }

    /// Source Direct is the control DOING the overriding, so it must never be dimmed — greying it
    /// out would leave the user looking at the one switch that undoes the state, drawn as if it
    /// were inert.
    #[test]
    fn source_direct_never_dims_itself() {
        let a = Advanced { overridden_by: Some("Source Direct"), ..sample() };
        assert!(!dimmed(&a, ROW_SOURCE_DIRECT));
        for r in 1..ROWS {
            assert!(dimmed(&a, r), "row {r} should be dimmed while overridden");
        }
    }

    /// With nothing overriding, nothing is dim.
    #[test]
    fn nothing_dims_when_the_chain_is_live() {
        let a = sample();
        for r in 0..ROWS {
            assert!(!dimmed(&a, r));
        }
    }
}
