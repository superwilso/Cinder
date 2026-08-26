//! Settings ▸ Battery — everything this device will actually tell you about its own cell.
//!
//! WHY A SCREEN AND NOT A ROW. "Battery care" was a single Settings row with an ON/OFF value and
//! nothing else, which meant the only battery fact Cinder ever showed was the status-bar
//! percentage. That percentage is also the least trustworthy number available: it is a gauge
//! estimate, it moves in steps, and it says nothing about whether the thing is charging, how hot
//! it is, or what voltage it has actually reached. The row is now a chevron into here.
//!
//! WHAT IS SHOWN AND WHERE IT COMES FROM. Two sources, and the screen keeps them separate on
//! purpose:
//!
//!   * `/sys/class/power_supply/battery/` — capacity, status, health, voltage_now. World-readable,
//!     always present, no helper needed. Four facts and no more: there is NO fuel gauge on this
//!     platform, so there is no current, no cycle count, and no battery temperature. Anything
//!     claiming otherwise on this screen would be invented.
//!   * `/proc/regmon/bq24262/` — the charger IC, read by the setuid `cinder-battery` helper. Its
//!     STATUS register carries the charge state machine and the fault code.
//!
//! Temperature is the board, NOT the cell — `thermal_zone1` is `mtktspmic`, the PMIC's own sensor.
//! It is labelled "board" for that reason. Calling it a battery temperature would be a guess
//! dressed as a measurement, and it reads several degrees above ambient even when idle.
//!
//! WHAT BATTERY CARE ACTUALLY DOES HERE, measured on device 2026-08-26. Sony calls it Itawari
//! charging and every description of it, Cinder's own Settings row included, says it caps the
//! charge at 90%. On this unit, with care ON, the charge plateaus at **4.093 V** and sits there
//! indefinitely: `status` stays `Charging`, the charger's STAT field stays 001, and the level never
//! reaches Full. A normal full charge on this chemistry is ~4.20 V, so the cell really is being
//! held well short of full — which is the whole point, and it is working.
//!
//! But the gauge reports that plateau as **99%**, not 90%. It appears to scale against the capped
//! ceiling rather than the cell's true one. So the "90%" in the old Settings row was a number the
//! user was never going to see, on a screen whose entire job is to show real readings. The row says
//! CARE ON now, and the footer here says what the cap actually looks like — a voltage that stops
//! climbing, not a percentage that stops at ninety.
//!
//! DECODING DISCIPLINE. Only the STATUS register is decoded, because only STATUS has been checked
//! against something independent: STAT=001 while sysfs says `Charging`, on device 2026-08-26. The
//! other six registers are real and readable, but this project has no bq24262 datasheet, and a
//! plausible-looking bit split for the current limit or the regulation voltage would be a number
//! the UI invented. They are shown as raw hex in the footer, labelled raw, where they are useful
//! for diagnosis and cannot be mistaken for a reading.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty, toggle};
use crate::Canvas;

/// Rows. Only the first is interactive; the rest are readouts.
pub const ROWS: usize = 5;
pub const ROW_CARE: usize = 0;
pub const ROW_VOLTAGE: usize = 1;
pub const ROW_TEMP: usize = 2;
pub const ROW_HEALTH: usize = 3;
pub const ROW_CHARGER: usize = 4;

/// Row pitch and list top — SINGLE SOURCE for `render` and `row_at`, so a tap can never land on a
/// different row than the one drawn under the finger.
pub const ROW_H: i32 = 58;
pub const TOP: i32 = 340;

/// Sentinel for "this reading is not available". Used rather than Option so the whole view stays
/// Copy and the C ABI can pass it through as a plain int.
pub const UNKNOWN: i32 = i32::MIN;

/// Which row is under `y`, or None outside the list.
pub fn row_at(y: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / ROW_H) as usize;
    (r < ROWS).then_some(r)
}

/// What the screen draws.
#[derive(Clone, Copy)]
pub struct BatteryView<'a> {
    /// 0..100 from sysfs `capacity`.
    pub percent: u8,
    /// sysfs `status`, verbatim: Charging / Discharging / Full / Not charging / Unknown.
    pub status: &'a str,
    /// sysfs `health`, verbatim: Good / Overheat / ...
    pub health: &'a str,
    /// sysfs `voltage_now`, in millivolts. `UNKNOWN` if unreadable.
    pub millivolts: i32,
    /// `thermal_zone1` (mtktspmic), in millidegrees C. `UNKNOWN` if unreadable. Board, not cell.
    pub milli_degc: i32,
    /// Sony's Itawari charging limit (PowerMgrServiceClient). The one control on this screen.
    pub care: bool,
    /// bq24262 STATUS bits 6:4 — 0 ready, 1 charging, 2 charge done, 3 fault. -1 if the helper
    /// is not installed or could not read the chip.
    pub chg_state: i32,
    /// bq24262 STATUS bits 2:0 — the fault code; 0 is "no fault". -1 if unknown.
    pub chg_fault: i32,
    /// The raw charger register line for the footer, e.g. "10 AC 78 46 10 04 18". Empty when the
    /// helper is absent, which is a supported configuration rather than an error.
    pub charger_raw: &'a str,
}

/// Is the cell taking charge right now? Read from sysfs rather than the charger, because sysfs is
/// the source that is always present.
pub fn is_charging(status: &str) -> bool {
    status.eq_ignore_ascii_case("charging")
}

/// Format millivolts as volts to three places: 4091 -> "4.091 V". `UNKNOWN` -> "—".
///
/// Three places rather than two on purpose: the interesting question on this device is what the
/// charge actually tops out at, and 4.09 vs 4.10 is exactly the distinction two places would lose.
pub fn volts_label(mv: i32) -> String {
    if mv == UNKNOWN || mv < 0 {
        return "—".to_string();
    }
    format!("{}.{:03} V", mv / 1000, mv % 1000)
}

/// Format millidegrees as one decimal place: 38196 -> "38.2 °C". `UNKNOWN` -> "—".
pub fn temp_label(mdeg: i32) -> String {
    if mdeg == UNKNOWN {
        return "—".to_string();
    }
    // Round to the nearest tenth, away from zero, so -0.04 does not print as "-0.0".
    let tenths = if mdeg >= 0 { (mdeg + 50) / 100 } else { -((-mdeg + 50) / 100) };
    format!("{}.{} °C", tenths / 10, (tenths % 10).abs())
}

/// The charger's own account of what it is doing. Deliberately only STATUS is decoded — see the
/// module header. A fault is named as a code, not a guess at what the code means.
pub fn charger_label(state: i32, fault: i32) -> String {
    if fault > 0 {
        return format!("FAULT {}", fault);
    }
    match state {
        0 => "READY".to_string(),
        1 => "CHARGING".to_string(),
        2 => "CHARGE DONE".to_string(),
        3 => "FAULT".to_string(),
        _ => "—".to_string(),
    }
}

/// The percentage as its own line. Kept separate from `render` so a test can assert the string
/// without a canvas.
pub fn percent_label(pct: u8) -> String {
    format!("{}%", pct.min(100))
}

/// One readout row: label left, value right, hairline under. Mirrors `settings::srow` so the two
/// screens do not look like different products, minus the selection highlight — nothing on this
/// screen except the toggle responds to a tap, so drawing a selection would promise otherwise.
fn brow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str, value: &str) -> i32 {
    let cy = y + ROW_H / 2;
    text::draw(c, f, 22.0, (cy + 5) as f32, label,
               &sty(Family::Sans, Weight::SemiBold, 19.0, t.ink, 0.0));
    right(c, f, 458.0, (cy + 4) as f32, value,
          &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.04));
    hline(c, y + ROW_H, t.line);
    y + ROW_H
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &BatteryView) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Battery", None);

    // ── the hero readout ──────────────────────────────────────────────────────────────────────
    // The percentage large, the sysfs status word under it. The status word is shown verbatim
    // rather than mapped to friendlier wording: "Not charging" and "Discharging" are different
    // states (cable in but charge suspended, versus no cable) and collapsing them would hide the
    // one that actually explains a battery that is not filling up.
    let charging = is_charging(v.status);
    text::draw(c, f, 22.0, 200.0, &percent_label(v.percent),
               &sty(Family::Sans, Weight::Bold, 84.0, t.ink, -0.02));
    text::draw(c, f, 24.0, 236.0, &v.status.to_uppercase(),
               &sty(Family::Mono, Weight::Regular, 13.0,
                    if charging { t.acc } else { t.dim }, 0.12));

    // A plain level bar. Fills with the accent while charging so the screen answers "is it going
    // up?" without reading a word.
    let bw = 436;
    stroke_rect(c, 22, 262, bw, 18, t.line, 1);
    let fillw = (bw - 4) * (v.percent.min(100) as i32) / 100;
    if fillw > 0 {
        fill_rect(c, 24, 264, fillw, 14, if charging { t.acc } else { t.dim });
    }

    // ── rows ──────────────────────────────────────────────────────────────────────────────────
    let mut y = TOP;

    // Battery care is the only control here. Sony calls it Itawari charging; it holds the cell
    // short of a full charge to slow ageing. The value says the cap so the row is self-explaining.
    let cy = y + ROW_H / 2;
    text::draw(c, f, 22.0, (cy + 5) as f32, "Battery care",
               &sty(Family::Sans, Weight::SemiBold, 19.0, t.ink, 0.0));
    toggle(c, t, 424, cy - 9, 34, 18, 12, v.care);
    hline(c, y + ROW_H, t.line);
    y += ROW_H;

    y = brow(c, t, f, y, "Voltage", &volts_label(v.millivolts));
    // "Board" not "Battery": thermal_zone1 is the PMIC sensor. There is no cell thermistor exposed.
    y = brow(c, t, f, y, "Board temperature", &temp_label(v.milli_degc));
    y = brow(c, t, f, y, "Health", &v.health.to_uppercase());
    let _ = brow(c, t, f, y, "Charger", &charger_label(v.chg_state, v.chg_fault));

    // ── footer ────────────────────────────────────────────────────────────────────────────────
    // The honest small print. This device has no fuel gauge, so there is no current reading and no
    // cycle count to show, and saying so is more useful than leaving the reader to wonder why a
    // battery screen omits them.
    hline(c, 700, t.line);
    let foot = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1);
    // Every footer line goes through `fit`: at 11 px mono the panel holds about 55 characters, and
    // a line that runs past 458 is simply cut off mid-word by the rasteriser rather than wrapped.
    // Truncating with an ellipsis is the honest failure — it at least looks deliberate.
    const FOOT_W: f32 = 436.0;
    text::draw(c, f, 22.0, 726.0,
               &crate::widgets::fit(f, "NO FUEL GAUGE — NO CURRENT, NO CYCLE COUNT.", &foot, FOOT_W),
               &foot);
    let line2 = if v.charger_raw.is_empty() {
        "CHARGER DETAIL NEEDS THE CINDER-BATTERY HELPER.".to_string()
    } else {
        format!("BQ24262 RAW {}", v.charger_raw)
    };
    text::draw(c, f, 22.0, 748.0, &crate::widgets::fit(f, &line2, &foot, FOOT_W), &foot);
    // What the cap looks like in practice, because it does NOT look like the number every
    // description of this feature quotes. See the module header: measured, care on, the charge
    // stops climbing at ~4.09 V and the gauge calls that 99%.
    if v.care {
        text::draw(c, f, 22.0, 770.0,
                   &crate::widgets::fit(f, "CARE ON: HOLDS NEAR 4.09 V, GAUGE STILL READS ~99%.",
                                        &foot, FOOT_W), &foot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volts_keeps_three_places_so_410_and_409_stay_distinct() {
        assert_eq!(volts_label(4091), "4.091 V");
        assert_eq!(volts_label(4100), "4.100 V");
        assert_eq!(volts_label(3999), "3.999 V");
        // The whole point of three places: these must not collapse to the same string.
        assert_ne!(volts_label(4091), volts_label(4100));
    }

    #[test]
    fn an_unreadable_reading_prints_a_dash_rather_than_a_zero() {
        // A zero here would read as a real measurement — a flat cell, or absolute zero.
        assert_eq!(volts_label(UNKNOWN), "—");
        assert_eq!(temp_label(UNKNOWN), "—");
        assert_eq!(charger_label(-1, -1), "—");
    }

    #[test]
    fn temperature_rounds_to_a_tenth() {
        assert_eq!(temp_label(38196), "38.2 °C");
        assert_eq!(temp_label(33000), "33.0 °C");
        assert_eq!(temp_label(32949), "32.9 °C");
        assert_eq!(temp_label(-1400), "-1.4 °C");
    }

    #[test]
    fn a_fault_code_outranks_the_state_word() {
        // STAT can still read "charging" while a fault bit is set; the fault is the thing the user
        // needs to see, so it wins.
        assert_eq!(charger_label(1, 3), "FAULT 3");
        assert_eq!(charger_label(1, 0), "CHARGING");
        assert_eq!(charger_label(2, 0), "CHARGE DONE");
    }

    #[test]
    fn only_charging_counts_as_charging() {
        assert!(is_charging("Charging"));
        assert!(is_charging("charging"));
        // "Not charging" contains "charging" — a substring test would get this wrong, and it is
        // exactly the state that explains a cable that is plugged in and doing nothing.
        assert!(!is_charging("Not charging"));
        assert!(!is_charging("Discharging"));
        assert!(!is_charging("Full"));
    }

    #[test]
    fn every_row_is_reachable_and_the_list_stops_where_it_is_drawn() {
        for r in 0..ROWS {
            let mid = TOP + r as i32 * ROW_H + ROW_H / 2;
            assert_eq!(row_at(mid), Some(r), "row {r} not hit at its own midpoint");
        }
        assert_eq!(row_at(TOP - 1), None, "a tap above the list must miss");
        assert_eq!(row_at(TOP + ROWS as i32 * ROW_H), None, "a tap past the last row must miss");
    }

    #[test]
    fn percent_is_clamped_so_a_bad_gauge_read_cannot_draw_past_full() {
        assert_eq!(percent_label(0), "0%");
        assert_eq!(percent_label(100), "100%");
        assert_eq!(percent_label(220), "100%");
    }
}
