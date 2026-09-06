//! Settings ▸ Visualiser — the analyser's own screen.
//!
//! WHY A SCREEN. The visualiser had two rows on the Settings list (style, and how much of the
//! cover it takes) and nothing else was adjustable, because nothing else existed: one hardcoded
//! band table, one hardcoded scaling curve, one hardcoded pair of smoothing constants. Everything
//! added here is a knob a desktop analyser has always had — how the magnitudes map to height, over
//! what dB window, how fast the bars chase the audio, whether peaks are marked — and each one is a
//! genuinely different display, not a preference between two spellings of the same one. Nine rows
//! is more than the Settings list had room for, and a cycling row buried in a scrolling list is
//! also the wrong shape for something you tune BY LOOKING AT IT.
//!
//! Which is what the preview at the top is for: it draws the real spectrum, with the real settings,
//! at the real frame rate, so a change to Response or Range is visible in the same glance that made
//! it. It is the same `viz::draw` the Now Playing pages call — not a mock-up of one.
//!
//! WHAT IS NOT HERE, and why: a band-COUNT row. Sony's AudioAnalyzerService allocates its level
//! detectors once, in its constructor, from a hardcoded twelve-entry list, and `SetPassband` only
//! re-assigns the vector — a thirteenth band has nothing to run in. Twelve is the ceiling for any
//! client, this one included, and a row offering 24 or 36 real bands would be offering a number the
//! hardware cannot produce. The columns you see are interpolated across those twelve; the Curve row
//! is honest about being an interpolation choice.
//!
//! The obvious way around the ceiling was tried and MEASURED, not assumed: alternate two twelve-band
//! tables frame by frame and interleave them into 24. `SetPassband` does apply to a live stream and
//! costs 1–7 ms, so the idea is not blocked by the API — it is blocked by physics. Alternating two
//! deliberately unmistakable tables (all twelve bands at 100 Hz, versus all twelve at 8 kHz) on
//! device separates them by 5.5x at a 500 ms dwell, 2.5x at 250 ms, and **1.1x at 100 ms** — i.e.
//! by the time the alternation is fast enough for a display, the two tables report the same thing.
//! A bandpass needs several cycles of its own centre frequency before its level means anything
//! (~Q/f, which at the bottom of the range is over 100 ms), and the level detector averages on top
//! of that. Twenty-four bands would cost roughly 2 Hz for a complete frame against 20–60 Hz for
//! twelve. The ceiling stands, and the twelve are placed and Q'd to be worth having instead.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty, toggle};
use crate::Canvas;

pub const ROWS: usize = 9;
pub const ROW_STYLE: usize = 0;
pub const ROW_COVER: usize = 1;
pub const ROW_SCALE: usize = 2;
pub const ROW_RANGE: usize = 3;
pub const ROW_RESPONSE: usize = 4;
pub const ROW_CURVE: usize = 5;
pub const ROW_PEAKS: usize = 6;
pub const ROW_WINDOW: usize = 7;
pub const ROW_RATE: usize = 8;

/// Row pitch, the preview's height, and the list top — SINGLE SOURCE for the render below and for
/// `nav`'s hit test. 91 (header) + 132 (preview) + 9 × 64 = 799, so the screen fits the panel
/// exactly and never scrolls: a scroll offset is the thing that has already drifted a hit test out
/// of step with a render once in this codebase.
pub const ROW_H: i32 = 64;
pub const PREVIEW_H: i32 = 132;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM + PREVIEW_H;

/// Which row is under `y`, or None above the list (the preview is not a control).
pub fn row_at(y: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / ROW_H) as usize;
    (r < ROWS).then_some(r)
}

/// What the screen draws. Values are pre-formatted by `nav` from the same index tables the rows
/// cycle, so this module never has to know what the options ARE.
#[derive(Clone, Copy)]
pub struct VizSet<'a> {
    pub style: &'a str,
    pub cover: &'a str,
    pub scale: &'a str,
    pub range: &'a str,
    pub response: &'a str,
    pub curve: &'a str,
    pub peaks: bool,
    pub window: &'a str,
    pub rate: &'a str,
    /// Live levels for the preview, exactly as Now Playing gets them. `None` when no analyzer is
    /// feeding us — the preview then draws the synthetic motion, and says so.
    pub levels: Option<&'a [f32]>,
    pub peak_marks: Option<&'a [f32]>,
    pub seed: f32,
    pub kind: crate::viz::VizKind,
}

fn vrow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str, value: &str) -> i32 {
    let cy = y + ROW_H / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, ROW_H, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    right(c, f, 458.0, (cy + 4) as f32, value, &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.04));
    hline(c, y + ROW_H, t.line);
    y + ROW_H
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &VizSet, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Visualiser", None);

    // ── the preview ──────────────────────────────────────────────────────────────────────────
    // Inset by the same 24 px the Now Playing visualiser uses, so what you tune is the width you
    // get. Drawn opaque (255/255) whatever the cover size is set to: this is the signal, not the
    // overlay, and fading it here would make Response and Range harder to judge.
    let px = 24;
    let pw = crate::canvas::W as i32 - px * 2;
    let ph = PREVIEW_H - 28;
    let py = y0 + 8;
    fill_rect(c, px, py, pw, ph, t.row_sel);
    crate::viz::draw_with_peaks(
        c, px + 4, py + 4, pw - 8, ph - 8, 36, 3, v.seed, v.kind, t.acc, t.line, v.levels,
        v.peak_marks, 255, 255,
    );
    // One caption, and it earns its line: with no analyzer running the bars are synthetic, and a
    // preview that silently shows made-up motion is the exact kind of thing this project keeps
    // taking out of screens.
    let cap = if v.levels.is_some() { "LIVE" } else { "NO SIGNAL — DEMO MOTION" };
    text::draw(c, f, px as f32, (py + ph + 15) as f32, cap,
               &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.06));

    // ── the rows ─────────────────────────────────────────────────────────────────────────────
    let mut y = TOP;
    y = vrow(c, t, f, y, sel == ROW_STYLE, "Style", v.style);
    y = vrow(c, t, f, y, sel == ROW_COVER, "On the cover", v.cover);
    y = vrow(c, t, f, y, sel == ROW_SCALE, "Level scale", v.scale);
    y = vrow(c, t, f, y, sel == ROW_RANGE, "Range", v.range);
    y = vrow(c, t, f, y, sel == ROW_RESPONSE, "Response", v.response);
    y = vrow(c, t, f, y, sel == ROW_CURVE, "Curve", v.curve);
    // A toggle, not a cycling value: markers are on or they are not.
    {
        let cy = y + ROW_H / 2;
        if sel == ROW_PEAKS {
            fill_rect(c, 0, y, crate::canvas::W as i32, ROW_H, t.row_sel);
        }
        let lc = if sel == ROW_PEAKS { t.acc } else { t.ink };
        text::draw(c, f, 22.0, (cy + 5) as f32, "Peak markers",
                   &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
        toggle(c, t, 418, cy - 11, 40, 22, 14, v.peaks);
        hline(c, y + ROW_H, t.line);
        y += ROW_H;
    }
    y = vrow(c, t, f, y, sel == ROW_WINDOW, "Time window", v.window);
    let _ = vrow(c, t, f, y, sel == ROW_RATE, "Frame rate", v.rate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_fits_the_panel() {
        // No scroll offset exists on this screen, so the last row must land on the panel.
        assert!(TOP + ROWS as i32 * ROW_H <= crate::canvas::H as i32,
                "rows run past the panel: {}", TOP + ROWS as i32 * ROW_H);
    }

    #[test]
    fn every_row_is_hittable_and_the_preview_is_not() {
        assert_eq!(row_at(crate::chrome::HEADER_BOTTOM + 4), None, "preview must not select a row");
        for r in 0..ROWS {
            let mid = TOP + r as i32 * ROW_H + ROW_H / 2;
            assert_eq!(row_at(mid), Some(r));
        }
        assert_eq!(row_at(TOP + ROWS as i32 * ROW_H + 1), None);
    }
}
