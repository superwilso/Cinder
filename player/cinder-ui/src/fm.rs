//! FM Radio — the real thing, wired to the Si4708.
//!
//! WHAT THE HARDWARE FORCED. Two of Sony's own primitives are unusable here, and both were
//! checked against a station that was audible at the time (analysis/RE_fm_tuner.md):
//!
//!   * `GetSignalLevel` returns 1 at EVERY frequency in the band. It is not a signal meter, so
//!     there is no bar to draw and nothing to seek on.
//!   * `StartAutoTuning` returns inside 100 ms having found nothing, in both directions.
//!
//! So a station list has to be MEASURED from the audio, and that costs ~0.45 s per 100 kHz step —
//! about 90 s for the whole band. That is far too slow to sit behind a SEEK button, which is why
//! this screen has an explicit SCAN that fills the preset row, and why ◀ ▶ then jump between the
//! stations that scan FOUND rather than sweeping live. The slow thing happens once, on purpose,
//! with the user watching; the fast thing happens per tap.
//!
//! ANTENNA. The headphone cable is the aerial — with an empty jack every frequency is noise. The
//! footer says so, because "the radio is broken" and "nothing is plugged in" look identical.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty};
use crate::Canvas;

/// Band edges in kHz. The tuner VALIDATES its own range — an out-of-band `SetFrequency` is
/// rejected and the previous value kept — so these are for the dial's geometry, not for safety.
pub const MIN_KHZ: i32 = 87500;
pub const MAX_KHZ: i32 = 108000;
/// One tap of the fine buttons. 100 kHz is the European raster and the step the scanner uses.
pub const STEP_KHZ: i32 = 100;
/// How many stations the preset row shows.
pub const PRESETS: usize = 6;

// ── layout: one source for render AND hit test ───────────────────────────────────────────────
const DIAL_X0: i32 = 30;
const DIAL_W: i32 = 420;
const DIAL_Y: i32 = 285;
const BTN_Y: i32 = 336;
const BTN_H: i32 = 44;
const PRESET_Y0: i32 = 418;
const PRESET_W: i32 = 138;
const PRESET_H: i32 = 52;
const PRESET_COLS: [i32; 3] = [22, 170, 318];
const SCAN_Y: i32 = 620;
const SCAN_H: i32 = 52;
const POWER: (i32, i32, i32, i32) = (352, 46, 106, 34);

/// The four transport buttons, laid out as a uniform row so the hit test needs no font metrics.
const BTN_LABELS: [&str; 4] = ["\u{2212}0.1", "\u{25C0}", "\u{25B6}", "+0.1"];
const BTN_W: i32 = 96;
const BTN_GAP: i32 = 12;
fn btn_x(i: usize) -> i32 {
    let total = BTN_W * 4 + BTN_GAP * 3;
    240 - total / 2 + i as i32 * (BTN_W + BTN_GAP)
}

/// What a tap on this screen means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Power,
    /// Fine tune by ±100 kHz.
    Step(i32),
    /// Jump to the previous / next station the scan found.
    Prev,
    Next,
    /// Preset slot — tune to it.
    Preset(usize),
    /// Run a band scan (slow; the shell shows progress).
    Scan,
    /// Tap on the dial — tune straight to that frequency.
    Dial(i32),
}

/// Frequency under dial x, snapped to the raster.
fn dial_khz(x: i32) -> i32 {
    let f = ((x - DIAL_X0).max(0) as f32 / DIAL_W as f32).min(1.0);
    let raw = MIN_KHZ + (f * (MAX_KHZ - MIN_KHZ) as f32) as i32;
    ((raw + STEP_KHZ / 2) / STEP_KHZ) * STEP_KHZ
}

fn dial_x(khz: i32) -> i32 {
    let f = (khz - MIN_KHZ) as f32 / (MAX_KHZ - MIN_KHZ) as f32;
    DIAL_X0 + (f.clamp(0.0, 1.0) * DIAL_W as f32) as i32
}

pub fn hit(x: i32, y: i32) -> Option<Hit> {
    let (px, py, pw, ph) = POWER;
    if (px..px + pw).contains(&x) && (py..py + ph).contains(&y) {
        return Some(Hit::Power);
    }
    // The dial is a real target, not decoration: a 40 px band around the line.
    if (DIAL_Y - 22..DIAL_Y + 22).contains(&y) && (DIAL_X0..DIAL_X0 + DIAL_W).contains(&x) {
        return Some(Hit::Dial(dial_khz(x)));
    }
    if (BTN_Y..BTN_Y + BTN_H).contains(&y) {
        for i in 0..4 {
            let bx = btn_x(i);
            if (bx..bx + BTN_W).contains(&x) {
                return Some(match i {
                    0 => Hit::Step(-STEP_KHZ),
                    1 => Hit::Prev,
                    2 => Hit::Next,
                    _ => Hit::Step(STEP_KHZ),
                });
            }
        }
        return None;
    }
    for i in 0..PRESETS {
        let cx = PRESET_COLS[i % 3];
        let cy = PRESET_Y0 + (i / 3) as i32 * (PRESET_H + 10);
        if (cx..cx + PRESET_W).contains(&x) && (cy..cy + PRESET_H).contains(&y) {
            return Some(Hit::Preset(i));
        }
    }
    if (SCAN_Y..SCAN_Y + SCAN_H).contains(&y) && (22..458).contains(&x) {
        return Some(Hit::Scan);
    }
    None
}

/// What the screen draws.
pub struct Fm {
    pub khz: i32,
    pub playing: bool,
    /// Stations the last scan found, in kHz. Empty until the user scans.
    pub stations: [i32; PRESETS],
    pub n_stations: usize,
    /// A scan is running — the row shows progress instead of stations.
    pub scanning: bool,
    /// 0..=100 while scanning.
    pub scan_pct: u8,
    /// Is anything in the headphone jack? Without it there is no aerial and no radio.
    pub antenna: bool,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, s: &Fm) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "FM Radio", None);

    // power pill
    let (px, py, pw, ph) = POWER;
    if s.playing {
        fill_rect(c, px, py, pw, ph, t.acc);
    }
    stroke_rect(c, px, py, pw, ph, if s.playing { t.acc } else { t.line }, 1);
    center(c, f, (px + pw / 2) as f32, (py + ph / 2 + 4) as f32,
           if s.playing { "ON" } else { "OFF" },
           &sty(Family::Mono, Weight::Regular, 12.0,
                if s.playing { t.acc_ink } else { t.dim }, 0.14));

    // big frequency readout
    let fstr = format!("{:.1}", s.khz as f32 / 1000.0);
    let ink = if s.playing { t.ink } else { t.faint };
    let fs = sty(Family::Mono, Weight::Light, 86.0, ink, -0.03);
    let ms = sty(Family::Mono, Weight::Regular, 19.0, t.dim, 0.0);
    let fw = text::measure(f, &fstr, &fs);
    let mw = text::measure(f, "MHz", &ms);
    let start = 240.0 - (fw + 9.0 + mw) / 2.0;
    text::draw(c, f, start, 205.0, &fstr, &fs);
    text::draw(c, f, start + fw + 9.0, 205.0, "MHz", &ms);

    // dial
    fill_rect(c, DIAL_X0, DIAL_Y, DIAL_W, 1, t.line);
    let mut mhz = 88;
    while mhz <= 106 {
        let x = dial_x(mhz * 1000);
        fill_rect(c, x, DIAL_Y - 10, 1, 20, t.line);
        center(c, f, x as f32, (DIAL_Y + 24) as f32, &format!("{mhz}"),
               &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        mhz += 2;
    }
    // found stations as ticks ON the dial — the scan's result, visible where it means something
    for i in 0..s.n_stations {
        let x = dial_x(s.stations[i]);
        fill_rect(c, x, DIAL_Y - 16, 1, 8, t.acc);
    }
    let nx = dial_x(s.khz);
    fill_rect(c, nx - 1, DIAL_Y - 20, 2, 40, if s.playing { t.acc } else { t.dim });

    // transport row
    let bs = sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.08);
    for (i, l) in BTN_LABELS.iter().enumerate() {
        let bx = btn_x(i);
        stroke_rect(c, bx, BTN_Y, BTN_W, BTN_H, t.line, 1);
        center(c, f, (bx + BTN_W / 2) as f32, (BTN_Y + BTN_H / 2 + 4) as f32, l, &bs);
    }

    // presets = what the scan found
    let cap = if s.n_stations == 0 { "PRESETS — RUN A SCAN" } else { "STATIONS FOUND" };
    text::draw(c, f, 22.0, (PRESET_Y0 - 10) as f32, cap,
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    for i in 0..PRESETS {
        let cx = PRESET_COLS[i % 3];
        let cy = PRESET_Y0 + (i / 3) as i32 * (PRESET_H + 10);
        let has = i < s.n_stations;
        let active = has && s.stations[i] == s.khz;
        if active {
            fill_rect(c, cx, cy, PRESET_W, PRESET_H, t.acc);
        }
        stroke_rect(c, cx, cy, PRESET_W, PRESET_H,
                    if active { t.acc } else { t.line }, 1);
        let col = if active { t.acc_ink } else if has { t.dim } else { t.faint };
        let label = if has { format!("{:.1}", s.stations[i] as f32 / 1000.0) } else { "—".into() };
        center(c, f, (cx + PRESET_W / 2) as f32, (cy + 24) as f32, &label,
               &sty(Family::Mono, Weight::Regular, 17.0, col, 0.0));
        center(c, f, (cx + PRESET_W / 2) as f32, (cy + 40) as f32, &format!("P{}", i + 1),
               &sty(Family::Mono, Weight::Regular, 10.0, col, 0.14));
    }

    // scan button / progress
    let scan_label = if s.scanning {
        format!("SCANNING… {}%", s.scan_pct)
    } else {
        "SCAN THE BAND".to_string()
    };
    stroke_rect(c, 22, SCAN_Y, 436, SCAN_H, if s.scanning { t.acc } else { t.line }, 1);
    if s.scanning {
        // fill proportional to progress — the only honest progress bar is one driven by real work
        let w = 436 * s.scan_pct.min(100) as i32 / 100;
        fill_rect(c, 22, SCAN_Y, w, SCAN_H, t.row_sel);
    }
    center(c, f, 240.0, (SCAN_Y + SCAN_H / 2 + 5) as f32, &scan_label,
           &sty(Family::Mono, Weight::Regular, 14.0, if s.scanning { t.acc } else { t.dim }, 0.10));
    center(c, f, 240.0, (SCAN_Y + SCAN_H + 20) as f32,
           "takes about a minute — the signal meter cannot do it",
           &sty(Family::Sans, Weight::Regular, 11.0, t.faint, 0.0));

    hline(c, 740, t.line);
    let note = if s.antenna {
        "ANTENNA: HEADPHONE CABLE"
    } else {
        "NO AERIAL — PLUG IN WIRED HEADPHONES"
    };
    center(c, f, 240.0, 768.0, note,
           &sty(Family::Mono, Weight::Regular, 11.0,
                if s.antenna { t.faint } else { t.acc }, 0.08));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every control must be reachable, and the dial must map back to the frequency it draws.
    #[test]
    fn the_dial_round_trips() {
        for khz in (MIN_KHZ..=MAX_KHZ).step_by(500) {
            let x = dial_x(khz);
            let back = dial_khz(x);
            assert!((back - khz).abs() <= STEP_KHZ,
                    "dial round trip {khz} -> x{x} -> {back}");
        }
    }

    #[test]
    fn every_button_is_hittable() {
        assert_eq!(hit(btn_x(0) + 10, BTN_Y + 10), Some(Hit::Step(-STEP_KHZ)));
        assert_eq!(hit(btn_x(1) + 10, BTN_Y + 10), Some(Hit::Prev));
        assert_eq!(hit(btn_x(2) + 10, BTN_Y + 10), Some(Hit::Next));
        assert_eq!(hit(btn_x(3) + 10, BTN_Y + 10), Some(Hit::Step(STEP_KHZ)));
        assert_eq!(hit(POWER.0 + 5, POWER.1 + 5), Some(Hit::Power));
        assert_eq!(hit(240, SCAN_Y + 10), Some(Hit::Scan));
        for i in 0..PRESETS {
            let cx = PRESET_COLS[i % 3];
            let cy = PRESET_Y0 + (i / 3) as i32 * (PRESET_H + 10);
            assert_eq!(hit(cx + 5, cy + 5), Some(Hit::Preset(i)));
        }
    }

    /// The gaps between controls must do nothing, or a fat finger retunes the radio by accident.
    #[test]
    fn gaps_are_not_targets() {
        assert_eq!(hit(btn_x(0) - 6, BTN_Y + 10), None, "left of the first button");
        assert_eq!(hit(240, BTN_Y + BTN_H + 4), None, "below the button row");
        assert_eq!(hit(5, PRESET_Y0 + 5), None, "left margin of the preset grid");
    }

    /// Tuning past the band edge clamps rather than wrapping — the tuner rejects out-of-band
    /// values and keeps the previous one, so a wrap would silently do nothing.
    #[test]
    fn the_dial_clamps_at_both_edges() {
        assert_eq!(dial_khz(DIAL_X0 - 50), MIN_KHZ);
        assert_eq!(dial_khz(DIAL_X0 + DIAL_W + 50), MAX_KHZ);
    }
}
