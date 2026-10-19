//! Sound ▸ Advanced — the rest of Sony's effect surface.
//!
//! WHY A SUB-SCREEN. The Sound screen keeps the everyday rows; everything else Sony's chain offers
//! is here, in one place. Since 2026-10-19 that includes VPT Surround, DC Phase and the Vinyl
//! Processor, which used to unfold under a "More effects" row on Sound — a second hiding place
//! beside this one. They are the controls you set once for a pair of headphones, not the ones you
//! reach for per album. The list scrolls.
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

/// Row ids. IDENTITIES, not positions: the screen draws them in [`order`].
pub const ROWS: usize = 12;
pub const ROW_CLEAR_PHASE: usize = 0;
pub const ROW_DSEE_AI: usize = 1;
pub const ROW_DSEE_CUSTOM: usize = 2;
pub const ROW_VINYL_TYPE: usize = 3;
pub const ROW_TONE: usize = 4;
pub const ROW_TONE_BANDS: usize = 5;
/// The codec's linear headphone amp instead of Sony's S-Master class-D. Hardware, not an effect:
/// Direct mode and ClearAudio+ do not take it out of the path, so it is never dimmed.
pub const ROW_HP_AMP: usize = 6;
/// The route to the codec's own EQ (`dac_eq.rs`). Not dimmed under ClearAudio+, which does not
/// reach the DAC; Direct mode holds it flat, and the row says so rather than greying out.
pub const ROW_DAC_EQ: usize = 7;
/// Sony's Clear Bass. Drawn only on a player whose sound library has the six-band EQ in its
/// chain (Walkman One after `cinder-chain.sh set clearbass`; `analysis/RE_clear_bass.md`).
pub const ROW_CLEAR_BASS: usize = 8;
/// From the Sound screen, 2026-10-19. Their state and toggles are the Sound screen's own
/// (`sound::ROW_VPT` and friends); only where they are drawn moved.
pub const ROW_VPT: usize = 9;
pub const ROW_DC: usize = 10;
pub const ROW_VINYL: usize = 11;

/// Clear Bass steps. The index times two is the gain handed to the six-band's band 0. Measured at
/// the jack 2026-10-11: +10 is about +7 dB from 35 to 90 Hz.
pub const CLEAR_BASS: [&str; 6] = ["Off", "+2", "+4", "+6", "+8", "+10"];

/// Top-to-bottom order: the effects (Clear Bass beside DC Phase, whose place it takes in the
/// chain), then the tone stage, then the two hardware rows. The Clear Bass row exists only where
/// the firmware can run it.
pub fn order(clear_bass: bool) -> Vec<usize> {
    let mut v = vec![ROW_VPT, ROW_DC];
    if clear_bass {
        v.push(ROW_CLEAR_BASS);
    }
    v.extend([
        ROW_VINYL,
        ROW_VINYL_TYPE,
        ROW_CLEAR_PHASE,
        ROW_DSEE_AI,
        ROW_DSEE_CUSTOM,
        ROW_TONE,
        ROW_TONE_BANDS,
        ROW_HP_AMP,
        ROW_DAC_EQ,
    ]);
    v
}

/// Row pitch and list top — SINGLE SOURCE for the render below and `nav`'s hit test. The banner
/// sits BELOW the rows so it cannot shift them: a row that moves when an unrelated toggle flips is
/// how a hit test drifts out of step with what is drawn.
pub const ROW_H: i32 = crate::scale::SETTING_ROW_H;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
/// The banner under the rows.
const BANNER_H: i32 = 64;

/// Which row id is under `y` at scroll `scroll`, or None outside the list.
pub fn row_at(y: i32, clear_bass: bool, scroll: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let i = ((y - TOP + scroll) / ROW_H) as usize;
    order(clear_bass).get(i).copied()
}

/// Screen-y of row `r`'s top at scroll `scroll` (TOP when it is not drawn).
pub fn row_top(r: usize, clear_bass: bool, scroll: i32) -> i32 {
    let i = order(clear_bass).iter().position(|&x| x == r).unwrap_or(0);
    TOP + ROW_H * i as i32 - scroll
}

/// How far the list scrolls.
pub fn max_scroll(clear_bass: bool) -> i32 {
    (TOP + ROW_H * order(clear_bass).len() as i32 + BANNER_H - crate::canvas::H as i32).max(0)
}

/// DSEE HX Custom modes, catalogue order — the index IS the value handed to `SetDseeHxCustomMode`.
/// Same provisional-label caveat as `nav::VPT_MODES`: catalogue order is almost certainly enum
/// order, but only listening settles it.
pub const DSEE_MODES: [&str; 5] = [
    "Standard",
    "Female Vocal",
    "Male Vocal",
    "Percussion",
    "Strings",
];

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
    /// Is DSEE HX itself on? Its switch is on the Sound screen, and both rows below are only in the
    /// path while it is — `SetDseeHxCustomMode` tunes the upscaler, it does not start one, and
    /// `SetDseeAi` selects which upscaler runs.
    ///
    /// The two rows underneath this comment used to claim nothing about their parent, while their
    /// immediate neighbours — Vinyl Character and the Tone band editor — both say "… is off" when
    /// theirs is. Same dependency, same screen, three lines apart, opposite honesty.
    pub dsee_hx: bool,
    /// "Off", or one of [`DSEE_MODES`].
    pub dsee_custom: &'static str,
    /// One of [`VINYL_TYPES`]. Only in the path while the Vinyl Processor itself is on, which is
    /// on the Sound screen — said so in the row's subtitle rather than hiding the row, because
    /// "set it up, then switch it on" is a reasonable order to do things in.
    pub vinyl_type: &'static str,
    pub vinyl_on: bool,
    /// "Off" or the VPT room; "Off" or the DC Phase filter (the Sound screen's values).
    pub vpt: &'static str,
    pub dcphase: &'static str,
    pub tone_control: bool,
    /// The linear headphone amp is selected (experimental, off by default).
    pub hp_linear: bool,
    /// The DAC EQ has at least one band off zero.
    pub dac_eq_on: bool,
    /// One of [`CLEAR_BASS`], or None where the firmware has no six-band EQ in its chain.
    pub clear_bass: Option<&'static str>,
    /// Name of whatever upstream control is currently overriding the rest of the chain, if any.
    pub overridden_by: Option<&'static str>,
    /// Walkman One's sound library: its effect chain has no `clearphase` and no `dseeai`
    /// (analysis/RE_sound_service.md section 3), so those two rows can do nothing.
    pub w1_chain: bool,
}

/// Is this row one of the ones an upstream override hides? Source Direct itself never dims — it is
/// the thing doing the overriding, so greying it out would strand the user with no way back.
fn dimmed(a: &Advanced, row: usize) -> bool {
    (a.overridden_by.is_some() && row != ROW_HP_AMP && row != ROW_DAC_EQ) || not_here(a, row)
}

/// A row whose filter is not in this player's effect chain at all.
pub fn not_here(a: &Advanced, row: usize) -> bool {
    (a.w1_chain && matches!(row, ROW_CLEAR_PHASE | ROW_DSEE_AI | ROW_VPT))
        || (row == ROW_DC && a.clear_bass.is_some())
}

const NOT_HERE: &str = "Not in Walkman One's sound library";

/// A row drawn dim, for when something upstream has taken it out of the path. Same geometry as
/// `sound::row` — it just draws the label in the faint ink so the screen reads as "these are not
/// doing anything right now" at a glance rather than after reading a footnote.
fn dim_row(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    sel: bool,
    label: &str,
    desc: &str,
) -> i32 {
    let cy = y + ROW_H / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, ROW_H, t.row_sel);
    }
    text::draw(
        c,
        f,
        22.0,
        (cy - 3) as f32,
        label,
        &sty(
            Family::Sans,
            Weight::SemiBold,
            crate::scale::ROW,
            t.faint,
            0.0,
        ),
    );
    text::draw(
        c,
        f,
        22.0,
        (cy + 15) as f32,
        desc,
        &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0),
    );
    hline(c, y + ROW_H, t.line);
    cy
}

fn a_row(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    a: &Advanced,
    y: i32,
    sel: bool,
    row: usize,
    label: &str,
    desc: &str,
) -> i32 {
    if dimmed(a, row) {
        dim_row(c, t, f, y, sel, label, desc)
    } else {
        crate::sound::row(c, t, f, y, sel, label, desc)
    }
}

/// What a row ends in.
enum End {
    Switch(bool),
    Pill(&'static str),
    Open,
}

/// One row's title, subtitle and trail.
fn spec(a: &Advanced, r: usize) -> (&'static str, String, End) {
    let s = |x: &str| x.to_string();
    match r {
        ROW_VPT => (
            "VPT Surround",
            s(if not_here(a, r) {
                NOT_HERE
            } else {
                "Studio \u{b7} Club \u{b7} Concert Hall \u{b7} Matrix"
            }),
            End::Pill(if not_here(a, r) { "Off" } else { a.vpt }),
        ),
        ROW_DC => (
            "DC Phase Linearizer",
            s(if not_here(a, r) {
                "Not on this player \u{b7} Clear Bass has its place"
            } else {
                "Analog-amp low-frequency phase"
            }),
            End::Pill(if not_here(a, r) { "Off" } else { a.dcphase }),
        ),
        ROW_VINYL => (
            "Vinyl Processor",
            s("Tonearm resonance, surface noise"),
            End::Switch(a.vinyl_on),
        ),
        ROW_CLEAR_PHASE => (
            "Clear Phase",
            s(if a.w1_chain {
                NOT_HERE
            } else {
                "Correct headphone phase response"
            }),
            End::Switch(a.clear_phase),
        ),
        // DSEE AI. SAYS WHAT IS KNOWN ABOUT IT, which is less than the other rows on this screen.
        //
        // `SetDseeAi`/`IsDseeAiOn` are exported by libEffectCtrlDmp and the write lands — and this
        // project has already learned once, expensively, that neither of those is evidence: high
        // gain accepted its write, read back 1, persisted, and the codec ignored it because the A50
        // output stage lacks the hardware (STATUS.md: "a mixer control accepting a write is not
        // evidence the feature works"). Not removed, because unlike high gain nobody has measured
        // it inert; the honest state is "we do not know", and a subtitle can say that.
        ROW_DSEE_AI => (
            "DSEE AI",
            s(if a.w1_chain {
                NOT_HERE
            } else if a.dsee_hx {
                "Upscaling by source analysis \u{2014} UNVERIFIED on this hardware"
            } else {
                "Upscaling by source analysis \u{2014} DSEE HX is off"
            }),
            End::Switch(a.dsee_ai),
        ),
        // Tuning the upscaler needs an upscaler.
        ROW_DSEE_CUSTOM => (
            "DSEE HX Custom",
            s(if a.dsee_hx {
                "Tune the upscaler to the material"
            } else {
                "DSEE HX is off"
            }),
            End::Pill(a.dsee_custom),
        ),
        ROW_VINYL_TYPE => (
            "Vinyl Character",
            s(if a.vinyl_on {
                "Character of the vinyl emulation"
            } else {
                "Vinyl Processor is off"
            }),
            End::Pill(a.vinyl_type),
        ),
        ROW_TONE => (
            "Tone Control",
            s("Bass / mid / treble \u{2014} alternative to the EQ"),
            End::Switch(a.tone_control),
        ),
        // The band editor. A ROUTE, not a setting. Reachable while Tone Control is off, because
        // "set it up, then switch it on" is a reasonable order to do things in.
        ROW_TONE_BANDS => (
            "Adjust bands",
            s(if a.tone_control {
                "Three bands, \u{b1}10 dB"
            } else {
                "Three bands \u{2014} Tone Control is off"
            }),
            End::Open,
        ),
        // The linear amp. Says "experimental" and what it costs up front: it has only been
        // measured into a line input, and it gives up the class-D amp's efficiency by an amount
        // this device cannot measure.
        ROW_HP_AMP => (
            "Linear headphone amp",
            s(if a.hp_linear {
                "Experimental \u{2014} about 3 dB louder, may cost battery"
            } else {
                "Experimental \u{2014} off uses Sony's S-Master amp"
            }),
            End::Switch(a.hp_linear),
        ),
        // The DAC EQ. A route. Source Direct holds it flat, and says so here instead of dimming.
        ROW_DAC_EQ => (
            "DAC EQ",
            s(if a.source_direct {
                "Five bands in the DAC chip \u{2014} flat in Direct mode"
            } else if a.dac_eq_on {
                "Five bands in the DAC chip \u{2014} on"
            } else {
                "Five bands in the DAC chip \u{2014} flat"
            }),
            End::Open,
        ),
        // Clear Bass. Sony's six-band EQ is a third alternative beside the 10-band EQ and Tone
        // Control (measured 2026-10-11), so the row says what it displaces.
        _ => {
            let level = a.clear_bass.unwrap_or(CLEAR_BASS[0]);
            (
                "Clear Bass",
                s(if level == CLEAR_BASS[0] {
                    "Sony's bass boost \u{2014} takes the place of the EQ"
                } else {
                    "On \u{2014} the EQ and Tone Control are out of the path"
                }),
                End::Pill(level),
            )
        }
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, a: &Advanced, sel: usize, scroll: i32) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Advanced", Some("Sound"));
    debug_assert_eq!(y0, TOP, "advanced list top drifted from the hit test");
    hline(c, y0, t.line);
    let chev = sty(
        Family::Sans,
        Weight::SemiBold,
        crate::scale::ROW,
        t.dim,
        0.0,
    );
    let rows = order(a.clear_bass.is_some());
    c.set_clip_y(TOP, crate::canvas::H as i32);
    for (i, &r) in rows.iter().enumerate() {
        let y = y0 + ROW_H * i as i32 - scroll;
        if y >= crate::canvas::H as i32 || y + ROW_H <= TOP {
            continue;
        }
        let (title, desc, end) = spec(a, r);
        let cy = a_row(c, t, f, a, y, sel == r, r, title, &desc);
        match end {
            End::Switch(on) => toggle(c, t, 418, cy - 11, 40, 22, 14, on),
            End::Pill(v) => crate::sound::value_pill(c, f, t, 458, cy, v),
            End::Open => crate::widgets::right(c, f, 458.0, (cy + 7) as f32, "\u{203A}", &chev),
        }
    }

    // ── the override banner ─────────────────────────────────────────────────────────────────
    // Below the rows, so nothing above it moves when it appears.
    let by = y0 + ROW_H * rows.len() as i32 - scroll + 18;
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
        line(
            14,
            &format!("{who} is on"),
            sty(Family::Sans, Weight::SemiBold, 13.0, t.acc, 0.0),
        );
        line(
            34,
            "The effects above are not in the path.",
            sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0),
        );
    } else {
        line(
            14,
            "Set these once for a pair of headphones.",
            sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0),
        );
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Advanced {
        Advanced {
            source_direct: false,
            clear_phase: false,
            dsee_ai: false,
            dsee_hx: false,
            dsee_custom: "Off",
            vinyl_type: VINYL_TYPES[0],
            vinyl_on: false,
            vpt: "Off",
            dcphase: "Off",
            tone_control: false,
            hp_linear: false,
            dac_eq_on: false,
            clear_bass: None,
            overridden_by: None,
            w1_chain: false,
        }
    }

    /// On Walkman One's library the two rows with no filter behind them dim; the rest do not.
    #[test]
    fn walkman_one_dims_only_the_missing_filters() {
        let a = Advanced {
            w1_chain: true,
            ..sample()
        };
        for r in 0..ROWS {
            assert_eq!(
                dimmed(&a, r),
                matches!(r, ROW_CLEAR_PHASE | ROW_DSEE_AI | ROW_VPT),
                "row {r}"
            );
        }
    }

    /// The hit test must agree with the drawn geometry for every row, at the top and the bottom
    /// of the scroll, with and without Clear Bass — this screen is touch-only and a row you cannot
    /// land on is a row that does not exist.
    #[test]
    fn every_row_is_hittable_at_its_own_band() {
        for cb in [false, true] {
            assert_eq!(order(cb).len(), ROWS - 1 + usize::from(cb));
            for scroll in [0, max_scroll(cb)] {
                for r in order(cb) {
                    let top = row_top(r, cb, scroll);
                    if top < TOP {
                        continue;
                    }
                    assert_eq!(row_at(top, cb, scroll), Some(r), "top edge of row {r}");
                    assert_eq!(
                        row_at(top + ROW_H - 1, cb, scroll),
                        Some(r),
                        "bottom of {r}"
                    );
                }
                assert_eq!(row_at(TOP - 1, cb, scroll), None, "above the list");
            }
            let last = *order(cb).last().unwrap();
            assert!(
                row_top(last, cb, max_scroll(cb)) + ROW_H + BANNER_H <= crate::canvas::H as i32,
                "scrolled to the end, the last row and the banner are on the glass"
            );
        }
    }

    /// An upstream mode dims every effect row; the two hardware rows stay live.
    #[test]
    fn an_override_dims_the_effects_and_not_the_hardware_rows() {
        let a = Advanced {
            overridden_by: Some("Direct mode"),
            ..sample()
        };
        for r in 0..ROWS {
            if r == ROW_HP_AMP || r == ROW_DAC_EQ {
                continue;
            }
            assert!(dimmed(&a, r), "row {r} should be dimmed while overridden");
        }
    }

    /// The amp row is hardware, not an effect: a bypass of the effect chain leaves it in the path,
    /// so it must never be drawn as if it were switched out.
    #[test]
    fn the_amp_row_never_dims() {
        for who in ["Source Direct", "ClearAudio+"] {
            let a = Advanced {
                overridden_by: Some(who),
                ..sample()
            };
            assert!(!dimmed(&a, ROW_HP_AMP), "dimmed under {who}");
        }
    }

    /// The DAC EQ row is the way in to a screen that explains its own state, and ClearAudio+ does not
    /// reach the DAC at all, so it never dims.
    #[test]
    fn the_dac_eq_row_never_dims() {
        for who in ["Source Direct", "ClearAudio+"] {
            let a = Advanced {
                overridden_by: Some(who),
                ..sample()
            };
            assert!(!dimmed(&a, ROW_DAC_EQ), "dimmed under {who}");
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
