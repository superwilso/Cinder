//! Settings ▸ Device — what the hardware is actually doing right now.
//!
//! WHY IT EXISTS. This started as a Battery screen, because "Battery care" was a single Settings
//! row with an ON/OFF value and the status-bar percentage was the only battery fact Cinder ever
//! showed. It turned out the same trip through sysfs that answers "how full is it" also answers
//! "how hot is it", "what clock is the CPU at", "how much music space is left" — every one of them
//! a world-readable file this app was already allowed to read and simply never did. So the screen
//! is the device's own vital signs, with the battery first because that is what people open it for.
//!
//! WHERE EVERY NUMBER COMES FROM, and what is deliberately absent:
//!
//!   * `/sys/class/power_supply/battery/` — capacity, status, health, voltage_now. World-readable,
//!     no helper. Four facts and no more: there is NO fuel gauge on this platform, so there is no
//!     current, no cycle count and no cell thermistor. A battery screen that showed "3 h 20 m
//!     remaining" here would be inventing it.
//!   * `/proc/regmon/bq24262/` — the charger IC, root-only, read by the setuid `cinder-battery`
//!     helper. Only its STATUS register is decoded; see DECODING DISCIPLINE below.
//!   * `/sys/class/thermal/thermal_zone{0,1,2}/` — three sensors, and they are DIE temperatures,
//!     not a battery temperature: `mtktscpu` (the SoC), `mtktspmic` (the power IC) and `mtktsabb`
//!     (the analog block). They are labelled for what they measure. The PMIC one runs several
//!     degrees above the others even at idle, which is normal and is why it is not passed off as
//!     an ambient or cell reading.
//!   * `/sys/devices/system/cpu/` — clock, governor and how many of the two cores are actually
//!     online. This SoC hotplugs the second core aggressively under the `hotplug` governor, so
//!     "1 of 2" is the normal idle state and not a fault.
//!   * `/proc/meminfo`, `/proc/uptime`, `statvfs` — memory, uptime, storage.
//!
//! No GPU clock: `/sys/module/mali` and the Mali debugfs expose utilisation and memory but no
//! frequency node, and the GPU present path is default-off anyway (measured 4.7x slower than the
//! software one), so a GPU row would read zero and mean nothing.
//!
//! WHAT BATTERY CARE ACTUALLY DOES HERE, measured on device 2026-08-26 across 123 samples from
//! `tools/battery_track.sh`. Sony calls it Itawari charging and every description of it — Cinder's
//! own Settings row included — says it caps the charge at 90%.
//!
//! With care ON, charging **terminates at ~4.09 V**. The highest reading in the whole sample set is
//! 4.0932 V, and the 53 samples that report `Full` top out at 4.0870 V. A normal full charge on
//! this chemistry is ~4.20 V, so the cell is being held about 0.11 V short of full — which is the
//! entire point of the feature, and it is working.
//!
//! But the gauge scales against that capped ceiling, so the protected state reports **100%**, not
//! 90%. The old "90%" on the Settings row was a number the user was never going to see, on a screen
//! whose whole job is showing real readings.
//!
//! (An earlier note here said it "never reaches Full". That was drawn from a 48-second window and
//! was wrong: it does reach `Full` at 100%. The ceiling voltage is the durable finding.)
//!
//! DECODING DISCIPLINE. Only the charger's STATUS register is decoded, because only STATUS has been
//! checked against something independent: STAT=001 while sysfs said `Charging`. The other six
//! registers are real and readable, but this project has no bq24262 datasheet, and a
//! plausible-looking bit split for the current limit or the regulation voltage would be a number
//! the UI invented. They go in the footer as raw hex, labelled raw.
//!
//! LAYOUT IS ONE LIST. `items()` builds every section header and row once, and BOTH `render` and
//! the hit test walk it. This screen scrolls and has exactly one control on it, so a second copy of
//! the vertical layout is precisely how a tap would land on a different row than the one under the
//! finger — the class of bug the 07-26 input sweep found six times.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty, toggle};
use crate::Canvas;

/// Sentinel for "this reading is not available". A plain int rather than an Option so the whole
/// view stays Copy and the C ABI can pass it straight through.
pub const UNKNOWN: i32 = i32::MIN;

/// Row pitch, section-header height, and where the list starts under the hero readout.
pub const ROW_H: i32 = 54;
pub const SECTION_H: i32 = 38;
pub const TOP: i32 = 300;

/// Everything the screen draws. Pushed by the shell; the UI has no filesystem of its own.
#[derive(Clone, Copy, Default)]
pub struct DeviceView<'a> {
    // ── battery ───────────────────────────────────────────────────────────────────────────────
    /// 0..100 from sysfs `capacity`.
    pub percent: u8,
    /// sysfs `status`, verbatim: Charging / Discharging / Full / Not charging / Unknown.
    pub status: &'a str,
    /// sysfs `health`, verbatim.
    pub health: &'a str,
    /// sysfs `voltage_now` in millivolts, or `UNKNOWN`.
    pub millivolts: i32,
    /// Sony's Itawari charge limit. The one control on this screen.
    pub care: bool,
    /// bq24262 STATUS bits 6:4 — 0 ready, 1 charging, 2 done, 3 fault. -1 if the helper is absent.
    pub chg_state: i32,
    /// bq24262 STATUS bits 2:0 — fault code, 0 = none. -1 if unknown.
    pub chg_fault: i32,
    /// Raw charger registers as hex, for the footer. Empty when the helper is not installed.
    pub charger_raw: &'a str,

    // ── temperatures, millidegrees C ──────────────────────────────────────────────────────────
    pub temp_cpu: i32,
    pub temp_pmic: i32,
    pub temp_abb: i32,

    // ── processor ─────────────────────────────────────────────────────────────────────────────
    /// Current and maximum clock in kHz, or `UNKNOWN`.
    pub cpu_khz: i32,
    pub cpu_max_khz: i32,
    /// Cores currently online, and how many the package has.
    pub cores_online: i32,
    pub cores_total: i32,
    /// cpufreq governor name, e.g. "hotplug".
    pub governor: &'a str,

    // ── memory and storage ────────────────────────────────────────────────────────────────────
    /// From /proc/meminfo, in kB.
    pub mem_total_kb: i32,
    pub mem_avail_kb: i32,
    /// The music volume and the app-data volume, in MB.
    pub music_total_mb: i32,
    pub music_free_mb: i32,
    pub data_free_mb: i32,

    // ── system ────────────────────────────────────────────────────────────────────────────────
    /// Seconds since boot, or `UNKNOWN`.
    pub uptime_s: i32,
    /// Kernel release, e.g. "3.10.26".
    pub kernel: &'a str,
    /// Cinder's own build label — the same string the Settings Firmware row shows.
    pub firmware: &'a str,
}

/// One entry in the single layout list. `Section` is a header; `Row` is a readout, and `toggle`
/// marks the one row that is also a control.
pub enum Item {
    Section(&'static str),
    Row { label: String, value: String, toggle: bool },
}

fn row(label: &str, value: String) -> Item {
    Item::Row { label: label.to_string(), value, toggle: false }
}

/// THE layout. Both `render` and `care_row_y` walk this, so the drawn position of the toggle and
/// its hit target come from one place and cannot drift apart.
pub fn items(v: &DeviceView) -> Vec<Item> {
    vec![
        Item::Section("BATTERY"),
        Item::Row { label: "Battery care".into(), value: String::new(), toggle: true },
        row("Voltage", volts_label(v.millivolts)),
        row("Health", text_or_dash(v.health, true)),
        row("Charger", charger_label(v.chg_state, v.chg_fault)),

        Item::Section("TEMPERATURE"),
        row("CPU", temp_label(v.temp_cpu)),
        row("Power IC", temp_label(v.temp_pmic)),
        row("Analog block", temp_label(v.temp_abb)),

        Item::Section("PROCESSOR"),
        row("Clock", clock_label(v.cpu_khz)),
        row("Maximum", clock_label(v.cpu_max_khz)),
        row("Cores online", cores_label(v.cores_online, v.cores_total)),
        row("Governor", text_or_dash(v.governor, true)),

        Item::Section("MEMORY"),
        row("RAM", mem_label(v.mem_total_kb, v.mem_avail_kb)),
        row("Free", if v.mem_avail_kb <= 0 { "—".into() } else { format!("{} MB", v.mem_avail_kb / 1024) }),

        Item::Section("STORAGE"),
        row("Music", size_pair(v.music_total_mb, v.music_free_mb)),
        row("Music free", mb_label(v.music_free_mb)),
        row("App data free", mb_label(v.data_free_mb)),

        Item::Section("SYSTEM"),
        row("Uptime", uptime_label(v.uptime_s)),
        row("Kernel", text_or_dash(v.kernel, false)),
        row("Firmware", text_or_dash(v.firmware, false)),
    ]
}

/// Height of one item.
fn item_h(it: &Item) -> i32 {
    match it {
        Item::Section(_) => SECTION_H,
        Item::Row { .. } => ROW_H,
    }
}

/// Total drawn height, hero included. Drives `max_scroll_px`.
pub fn content_height(v: &DeviceView) -> i32 {
    TOP + items(v).iter().map(item_h).sum::<i32>() + FOOTER_H
}

/// Room left below the footer for the small print.
const FOOTER_H: i32 = 96;

/// How far this screen can scroll. 0 means it all fits.
pub fn max_scroll_px(v: &DeviceView) -> i32 {
    (content_height(v) - crate::canvas::H as i32).max(0)
}

/// Screen-y of the battery-care row's top at this scroll offset, or None if it has no toggle row.
pub fn care_row_y(v: &DeviceView, scroll: i32) -> Option<i32> {
    let mut y = TOP - scroll;
    for it in items(v) {
        if let Item::Row { toggle: true, .. } = it {
            return Some(y);
        }
        y += item_h(&it);
    }
    None
}

/// Did this tap land on the battery-care row? The ONLY interactive target on the screen — every
/// other row is a reading, and a tap on one is deliberately inert rather than selecting something
/// that cannot act.
pub fn hit_care(v: &DeviceView, y: i32, scroll: i32) -> bool {
    match care_row_y(v, scroll) {
        Some(top) => y >= top && y < top + ROW_H,
        None => false,
    }
}

// ── formatting, all unit-tested ───────────────────────────────────────────────────────────────

/// Millivolts to three places: 4093 -> "4.093 V".
///
/// Three places rather than two on purpose: the interesting question on this device is where the
/// charge tops out, and 4.09 vs 4.10 is exactly the distinction two places would lose.
pub fn volts_label(mv: i32) -> String {
    if mv == UNKNOWN || mv < 0 {
        return "—".into();
    }
    format!("{}.{:03} V", mv / 1000, mv % 1000)
}

/// Millidegrees to one decimal: 38196 -> "38.2 °C".
pub fn temp_label(mdeg: i32) -> String {
    if mdeg == UNKNOWN {
        return "—".into();
    }
    let tenths = if mdeg >= 0 { (mdeg + 50) / 100 } else { -((-mdeg + 50) / 100) };
    format!("{}.{} °C", tenths / 10, (tenths % 10).abs())
}

/// kHz to MHz: 1300000 -> "1300 MHz".
pub fn clock_label(khz: i32) -> String {
    if khz == UNKNOWN || khz <= 0 {
        return "—".into();
    }
    format!("{} MHz", khz / 1000)
}

/// "1 OF 2". Hotplug takes the second core offline at idle, so this is normal, not a fault.
pub fn cores_label(online: i32, total: i32) -> String {
    if online <= 0 || total <= 0 {
        return "—".into();
    }
    format!("{} OF {}", online, total)
}

/// Used-of-total in MB, computed from what /proc/meminfo actually gives.
pub fn mem_label(total_kb: i32, avail_kb: i32) -> String {
    if total_kb <= 0 {
        return "—".into();
    }
    let used = (total_kb - avail_kb.max(0)).max(0);
    format!("{} / {} MB", used / 1024, total_kb / 1024)
}

/// Plain megabytes, switching to GB once it would need five digits — "54.1 GB" reads where
/// "55296 MB" does not.
pub fn mb_label(mb: i32) -> String {
    if mb == UNKNOWN || mb < 0 {
        return "—".into();
    }
    if mb >= 10000 {
        return format!("{}.{} GB", mb / 1024, (mb % 1024) * 10 / 1024);
    }
    format!("{} MB", mb)
}

/// "54.1 / 55.9 GB" — used against total, from total and free.
pub fn size_pair(total_mb: i32, free_mb: i32) -> String {
    if total_mb <= 0 {
        return "—".into();
    }
    let used = (total_mb - free_mb.max(0)).max(0);
    format!("{} / {}", mb_label(used), mb_label(total_mb))
}

/// Seconds to "3d 4h", "4h 12m", "12m", "48s" — always two units at most.
pub fn uptime_label(s: i32) -> String {
    if s == UNKNOWN || s < 0 {
        return "—".into();
    }
    let (d, h, m) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{}d {}h", d, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else if m > 0 {
        format!("{}m", m)
    } else {
        format!("{}s", s)
    }
}

/// The charger's own account of itself. Only STATUS is decoded — see the module header. A fault is
/// named as a code, not as a guess at what the code means.
pub fn charger_label(state: i32, fault: i32) -> String {
    if fault > 0 {
        return format!("FAULT {}", fault);
    }
    match state {
        0 => "READY".into(),
        1 => "CHARGING".into(),
        2 => "CHARGE DONE".into(),
        3 => "FAULT".into(),
        _ => "—".into(),
    }
}

/// A string reading, or a dash when the device could not tell us.
///
/// Found by `every_row_has_a_value_even_when_nothing_could_be_read`: `health` was rendered as
/// `v.health.to_uppercase()`, so an unreadable sysfs file drew an EMPTY value column — which reads
/// as a rendering fault rather than as a missing reading. Every string row goes through here.
pub fn text_or_dash(s: &str, upper: bool) -> String {
    if s.trim().is_empty() {
        return "—".into();
    }
    if upper { s.to_uppercase() } else { s.to_string() }
}

pub fn percent_label(pct: u8) -> String {
    format!("{}%", pct.min(100))
}

/// Is the cell taking charge? From sysfs, because that is the source always present.
pub fn is_charging(status: &str) -> bool {
    status.eq_ignore_ascii_case("charging")
}

// ── render ────────────────────────────────────────────────────────────────────────────────────

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &DeviceView, scroll: i32) {
    c.fill(t.bg);
    let head_bottom = crate::chrome::header(c, t, f, "Device", None);
    // Clip below the header for the same reason Settings does: this screen scrolls, and content
    // sliding up must stop at the header rather than painting over it.
    c.set_clip_y(head_bottom, crate::canvas::H as i32);

    // ── hero ──────────────────────────────────────────────────────────────────────────────────
    // The status word is printed VERBATIM, not mapped to friendlier wording: "Not charging" and
    // "Discharging" are different states — cable in but charge suspended, versus no cable — and
    // collapsing them hides the one that explains a battery that is not filling up.
    let charging = is_charging(v.status);
    let hy = 190 - scroll;
    text::draw(c, f, 22.0, hy as f32, &percent_label(v.percent),
               &sty(Family::Sans, Weight::Bold, 76.0, t.ink, -0.02));
    text::draw(c, f, 24.0, (hy + 32) as f32, &v.status.to_uppercase(),
               &sty(Family::Mono, Weight::Regular, 13.0, if charging { t.acc } else { t.dim }, 0.12));
    let bw = 436;
    stroke_rect(c, 22, hy + 56, bw, 16, t.line, 1);
    let fillw = (bw - 4) * (v.percent.min(100) as i32) / 100;
    if fillw > 0 {
        fill_rect(c, 24, hy + 58, fillw, 12, if charging { t.acc } else { t.dim });
    }

    // ── the list ──────────────────────────────────────────────────────────────────────────────
    let mut y = TOP - scroll;
    for it in items(v) {
        match &it {
            Item::Section(name) => {
                text::draw(c, f, 22.0, (y + 26) as f32, name,
                           &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16));
            }
            Item::Row { label, value, toggle: is_toggle } => {
                let cy = y + ROW_H / 2;
                text::draw(c, f, 22.0, (cy + 5) as f32, label,
                           &sty(Family::Sans, Weight::SemiBold, 19.0, t.ink, 0.0));
                if *is_toggle {
                    toggle(c, t, 424, cy - 9, 34, 18, 12, v.care);
                } else {
                    right(c, f, 458.0, (cy + 4) as f32, value,
                          &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.04));
                }
                hline(c, y + ROW_H, t.line);
            }
        }
        y += item_h(&it);
    }

    // ── footer ────────────────────────────────────────────────────────────────────────────────
    // The honest small print, and it scrolls with the content rather than floating: this device has
    // no fuel gauge, so there is no current reading and no cycle count, and saying so is more useful
    // than leaving the reader to wonder why a battery section omits them.
    let foot = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1);
    const FOOT_W: f32 = 436.0;
    y += 18;
    text::draw(c, f, 22.0, y as f32,
               &crate::widgets::fit(f, "NO FUEL GAUGE — NO CURRENT, NO CYCLE COUNT.", &foot, FOOT_W),
               &foot);
    y += 22;
    let l2 = if v.charger_raw.is_empty() {
        "CHARGER DETAIL NEEDS THE CINDER-BATTERY HELPER.".to_string()
    } else {
        format!("BQ24262 RAW {}", v.charger_raw)
    };
    text::draw(c, f, 22.0, y as f32, &crate::widgets::fit(f, &l2, &foot, FOOT_W), &foot);
    if v.care {
        y += 22;
        text::draw(c, f, 22.0, y as f32,
                   &crate::widgets::fit(f, "CARE ON: TOPS OUT ~4.09 V, NOT 4.2 V. GAUGE SAYS 100%.",
                                        &foot, FOOT_W), &foot);
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DeviceView<'static> {
        DeviceView {
            percent: 99, status: "Charging", health: "Good", millivolts: 4093, care: true,
            chg_state: 1, chg_fault: 0, charger_raw: "10 AC 78",
            temp_cpu: 34400, temp_pmic: 39365, temp_abb: 34400,
            cpu_khz: 1300000, cpu_max_khz: 1300000, cores_online: 1, cores_total: 2,
            governor: "hotplug",
            mem_total_kb: 467512, mem_avail_kb: 159772,
            music_total_mb: 56320, music_free_mb: 1024, data_free_mb: 13,
            uptime_s: 711, kernel: "3.10.26", firmware: "CINDER DEV · RUST",
        }
    }

    #[test]
    fn volts_keeps_three_places_so_409_and_410_stay_distinct() {
        assert_eq!(volts_label(4093), "4.093 V");
        assert_ne!(volts_label(4091), volts_label(4100));
    }

    #[test]
    fn an_unreadable_reading_prints_a_dash_rather_than_a_zero() {
        // A zero here reads as a real measurement — a flat cell, a stopped clock, an empty disk.
        assert_eq!(volts_label(UNKNOWN), "—");
        assert_eq!(temp_label(UNKNOWN), "—");
        assert_eq!(clock_label(UNKNOWN), "—");
        assert_eq!(clock_label(0), "—");
        assert_eq!(uptime_label(UNKNOWN), "—");
        assert_eq!(mb_label(UNKNOWN), "—");
        assert_eq!(mem_label(0, 0), "—");
        assert_eq!(cores_label(0, 0), "—");
        assert_eq!(charger_label(-1, -1), "—");
    }

    #[test]
    fn temperature_rounds_to_a_tenth() {
        assert_eq!(temp_label(38196), "38.2 °C");
        assert_eq!(temp_label(34400), "34.4 °C");
        assert_eq!(temp_label(-1400), "-1.4 °C");
    }

    #[test]
    fn clocks_and_cores_read_the_way_the_hardware_reports_them() {
        assert_eq!(clock_label(1300000), "1300 MHz");
        assert_eq!(clock_label(598000), "598 MHz");
        // Hotplug parks the second core at idle. "1 OF 2" is the normal state, not a fault.
        assert_eq!(cores_label(1, 2), "1 OF 2");
        assert_eq!(cores_label(2, 2), "2 OF 2");
    }

    #[test]
    fn big_volumes_switch_to_gb_because_five_digits_of_mb_do_not_read() {
        assert_eq!(mb_label(512), "512 MB");
        assert_eq!(mb_label(9999), "9999 MB");
        assert_eq!(mb_label(56320), "55.0 GB");
        // The music volume: 55 GB total with 1 GB left is the state that matters, and it has to be
        // legible at a glance.
        assert_eq!(size_pair(56320, 1024), "54.0 GB / 55.0 GB");
    }

    #[test]
    fn uptime_never_shows_more_than_two_units() {
        assert_eq!(uptime_label(48), "48s");
        assert_eq!(uptime_label(711), "11m");
        assert_eq!(uptime_label(15120), "4h 12m");
        assert_eq!(uptime_label(273600), "3d 4h");
    }

    #[test]
    fn a_fault_code_outranks_the_state_word() {
        // STAT can still read "charging" with a fault bit set; the fault is what needs seeing.
        assert_eq!(charger_label(1, 3), "FAULT 3");
        assert_eq!(charger_label(1, 0), "CHARGING");
        assert_eq!(charger_label(2, 0), "CHARGE DONE");
    }

    #[test]
    fn only_charging_counts_as_charging() {
        assert!(is_charging("Charging"));
        // "Not charging" CONTAINS "charging" — a substring test gets this wrong, and it is exactly
        // the state that explains a cable plugged in and doing nothing.
        assert!(!is_charging("Not charging"));
        assert!(!is_charging("Discharging"));
        assert!(!is_charging("Full"));
    }

    #[test]
    fn percent_is_clamped_so_a_bad_gauge_read_cannot_draw_past_full() {
        assert_eq!(percent_label(220), "100%");
    }

    #[test]
    fn the_care_row_hit_follows_the_scroll_it_is_drawn_at() {
        let v = sample();
        let top = care_row_y(&v, 0).expect("there is a care row");
        assert!(hit_care(&v, top + ROW_H / 2, 0));
        assert!(!hit_care(&v, top - 1, 0));
        assert!(!hit_care(&v, top + ROW_H, 0));
        // Scrolled, the target must move with the drawing by exactly the same amount — one layout
        // walked twice, not two layouts that happen to agree at rest.
        let s = 120;
        assert_eq!(care_row_y(&v, s), Some(top - s));
        assert!(hit_care(&v, top - s + ROW_H / 2, s));
        assert!(!hit_care(&v, top + ROW_H / 2, s));
    }

    #[test]
    fn exactly_one_row_is_a_control() {
        let v = sample();
        let toggles = items(&v).iter()
            .filter(|it| matches!(it, Item::Row { toggle: true, .. }))
            .count();
        assert_eq!(toggles, 1, "the care switch is the only thing on this screen that acts");
    }

    #[test]
    fn the_content_is_taller_than_the_panel_so_the_screen_must_scroll() {
        let v = sample();
        assert!(content_height(&v) > crate::canvas::H as i32,
                "if this ever fits, drop the scrolling rather than leaving dead code");
        assert!(max_scroll_px(&v) > 0);
    }

    #[test]
    fn a_missing_string_reading_is_a_dash_not_a_blank() {
        assert_eq!(text_or_dash("", true), "—");
        assert_eq!(text_or_dash("   ", false), "—");
        assert_eq!(text_or_dash("Good", true), "GOOD");
        assert_eq!(text_or_dash("3.10.26", false), "3.10.26");
    }

    #[test]
    fn every_row_has_a_value_even_when_nothing_could_be_read() {
        // The all-unknown device: no helper, unreadable sysfs. Every row must still render a
        // string, because a blank value column looks like a rendering fault rather than a
        // missing reading.
        let v = DeviceView {
            status: "", health: "", millivolts: UNKNOWN, charger_raw: "",
            chg_state: -1, chg_fault: -1,
            temp_cpu: UNKNOWN, temp_pmic: UNKNOWN, temp_abb: UNKNOWN,
            cpu_khz: UNKNOWN, cpu_max_khz: UNKNOWN, governor: "",
            uptime_s: UNKNOWN, kernel: "", firmware: "",
            ..DeviceView::default()
        };
        for it in items(&v) {
            if let Item::Row { label, value, toggle } = it {
                if toggle {
                    continue; // the switch draws itself
                }
                assert!(!value.is_empty(), "row {label:?} rendered an empty value");
            }
        }
    }
}
