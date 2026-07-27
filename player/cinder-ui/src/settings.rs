//! Settings — interactive. Up/Down move the cursor; Select acts on the focused row. Rows:
//! DISPLAY (Theme, Visualiser type, Visualiser animation, Screen-off, Brightness),
//! SYSTEM (Storage, Database, Battery care, USB mode), ABOUT (Firmware, Model).
//! Live rows: Theme, Visualiser type, Visualiser, Sleep timer, Brightness (DISPLAY), Battery care
//! and USB mode (SYSTEM). Screen-off timer and Database are drawn but NOT wired (they show "—" —
//! see the dead-UI audit in cinder-home/STATUS.md); Firmware/Model are static info.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;

/// Number of selectable rows (for nav cursor clamping). Keep in sync with the rows below.
pub const ROWS: usize = 12;
/// The actionable rows: Theme / Visualiser / Visualiser animation / Sleep timer (DISPLAY) +
/// Battery care (SYSTEM).
pub const ROW_THEME: usize = 0;
pub const ROW_VIZ: usize = 1;
pub const ROW_VIZ_ANIM: usize = 2;
pub const ROW_SLEEP: usize = 3;
pub const ROW_BATTERY: usize = 8;
pub const ROW_BRIGHTNESS: usize = 5;
pub const ROW_USB_MODE: usize = 9; // tapping enters USB mass-storage (file transfer to a PC)

const RH: i32 = 56;

/// Firmware/build label shown on the Settings "Firmware" row. The `dev` feature (development
/// channel, built from the same tree) makes the two builds visually distinguishable on-device.
#[cfg(feature = "dev")]
pub const FIRMWARE_LABEL: &str = "CINDER DEV · RUST";
#[cfg(not(feature = "dev"))]
pub const FIRMWARE_LABEL: &str = "CINDER 1.0 · RUST";

/// Current settings values to display.
pub struct SettingsView<'a> {
    pub night: bool,
    pub viz_name: &'a str,
    pub viz_on: bool,
    pub usb_dac: bool,
    pub battery_care: bool,
    pub storage: &'a str,
    pub sleep: &'a str,
    /// Brightness label, e.g. "3 / 5" (nav formats it from its 1..5 level).
    pub brightness: &'a str,
}

/// Which selectable row is at touch-y `y`? Mirrors `render`'s vertical layout exactly: header
/// ends at 91, each section eyebrow consumes +14 (gap) then +24, and every row is `RH` tall.
/// Returns the row index (0..ROWS) or None (tapped a gap/eyebrow).
pub fn row_at(y: i32) -> Option<usize> {
    let mut yy = 91 + 24; // after the DISPLAY eyebrow
    for r in 0..6 {
        if y >= yy && y < yy + RH {
            return Some(r);
        }
        yy += RH;
    }
    yy += 14 + 24; // SYSTEM eyebrow
    for r in 6..10 {
        if y >= yy && y < yy + RH {
            return Some(r);
        }
        yy += RH;
    }
    yy += 14 + 24; // ABOUT eyebrow
    for r in 10..12 {
        if y >= yy && y < yy + RH {
            return Some(r);
        }
        yy += RH;
    }
    None
}

fn eyebrow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    text::draw(c, f, 22.0, (y + 14) as f32, label, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    y + 24
}

/// A label/value row; highlights when selected. Returns the next y.
fn srow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str, value: &str, chevron: bool) -> i32 {
    let cy = y + RH / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    let vx = if chevron { 438.0 } else { 458.0 };
    right(c, f, vx, (cy + 4) as f32, value, &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.04));
    if chevron {
        icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
    }
    hline(c, y + RH, t.line);
    y + RH
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, v: &SettingsView) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Settings", None);

    let mut y = eyebrow(c, t, f, y0, "DISPLAY");

    // Row 0: Theme — Day/Night segmented control (highlighted when selected)
    if sel == ROW_THEME {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    hline(c, y, t.line);
    let cy = y + RH / 2;
    let lc = if sel == ROW_THEME { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, "Theme", &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    let segs = [("DAY", !v.night), ("NIGHT", v.night)];
    let sh = 26;
    let mut widths = [0i32; 2];
    for (i, (label, _)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.1);
        widths[i] = text::measure(f, label, &st) as i32 + 26;
    }
    let total = widths[0] + widths[1] + 8;
    let mut sx = 458 - total;
    for (i, (label, on)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 12.0, if *on { t.acc_ink } else { t.dim }, 0.1);
        if *on {
            fill_rect(c, sx, cy - sh / 2, widths[i], sh, t.acc);
        }
        stroke_rect(c, sx, cy - sh / 2, widths[i], sh, if *on { t.acc } else { t.line }, 1);
        text::draw(c, f, (sx + 13) as f32, (cy + 4) as f32, label, &st);
        sx += widths[i] + 8;
    }
    hline(c, y + RH, t.line);
    y += RH;

    // Rows 1-2: the visualiser options (live). ROW_VIZ_ANIM is the master ON/OFF (hides the
    // visualiser entirely on Now Playing when off); ROW_VIZ picks which type to show when on.
    y = srow(c, t, f, y, sel == ROW_VIZ, "Visualiser type", v.viz_name, false);
    y = srow(c, t, f, y, sel == ROW_VIZ_ANIM, "Visualiser", if v.viz_on { "ON" } else { "OFF" }, false);
    // Row 3: Sleep timer (live) — pauses playback after N min. Shows the live remaining when running.
    y = srow(c, t, f, y, sel == ROW_SLEEP, "Sleep timer", v.sleep, false);
    // Row 4 is NOT WIRED — still drawn (it marks where a real stock feature goes) but it must not
    // pretend: the old "30 SEC" was an invented number on a row that does nothing when tapped, so
    // it read as a setting the user had chosen. "—" is the honest value.
    y = srow(c, t, f, y, sel == 4, "Screen-off timer", "—", false);
    // Row 5: brightness is live — tapping cycles 1..5 and the shell writes the backlight node.
    y = srow(c, t, f, y, sel == ROW_BRIGHTNESS, "Brightness", v.brightness, false);

    y = eyebrow(c, t, f, y + 14, "SYSTEM");
    // Storage shows the real statvfs value (no chevron — it's a live info row, not a drill-in).
    y = srow(c, t, f, y, sel == 6, "Storage", v.storage, false);
    // Database: no chevron. The chevron is this screen's affordance for "tapping does something"
    // (USB mode has one and acts), and this row has no arm in settings_activate — so a chevron here
    // promised a rebuild that never ran.
    y = srow(c, t, f, y, sel == 7, "Database", "—", false);
    // Battery care = Sony "Itawari" charging (caps ~90%). Live On/Off toggle (no chevron — it acts
    // in place), wired to PowerMgrServiceClient::EnableItawariCharging via the shell.
    y = srow(c, t, f, y, sel == ROW_BATTERY, "Battery care", if v.battery_care { "ON · 90%" } else { "OFF" }, false);
    y = srow(c, t, f, y, sel == 9, "USB mode", if v.usb_dac { "DAC" } else { "MASS STORAGE" }, true);

    y = eyebrow(c, t, f, y + 14, "ABOUT");
    y = srow(c, t, f, y, sel == 10, "Firmware", FIRMWARE_LABEL, false);
    let _ = srow(c, t, f, y, sel == 11, "Model", "SONY NW-A55", false);
}
