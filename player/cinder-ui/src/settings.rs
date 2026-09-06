//! Settings — interactive. Up/Down move the cursor; Select acts on the focused row. Rows:
//! DISPLAY (Theme, Accent, Visualiser style, Cover visualiser, Sleep, Screen-off, Brightness),
//! SYSTEM (Storage, Database, Battery care, USB mode, Boot to stock, Restart, Power off),
//! ABOUT (Firmware, Model).
//! Live rows: Theme, Accent, Visualiser style, Cover visualiser, Sleep, Screen-off, Brightness
//! (DISPLAY), Battery care and USB mode (SYSTEM). Database is drawn but NOT wired (shows "—" — see
//! the dead-UI audit in cinder-home/STATUS.md); Firmware/Model are static info.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::{Accent, Theme};
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;

/// Number of selectable rows (for nav cursor clamping). Keep in sync with the rows below.
pub const ROWS: usize = 19;
/// The actionable rows: Theme / Accent / UI scale / Visualiser / Sleep timer (DISPLAY) +
/// Battery care (SYSTEM).
pub const ROW_THEME: usize = 0;
/// Accent colour — six swatches, tap one directly (Select cycles).
pub const ROW_ACCENT: usize = 1;
/// UI text scale — a real slider (tap a stop, or drag it). See `ui_scale_idx_at`.
pub const ROW_UI_SCALE: usize = 2;
/// Visualiser — a CHEVRON into its own screen (`vizset`), not a control in place.
///
/// This was two rows, "Visualiser style" and "Cover visualiser", and they were the only two things
/// about the analyser a user could change. Everything else — how magnitudes map to bar height, the
/// dB window, how fast the bars chase the audio, whether peaks are marked, the analyzer's own
/// averaging window and frame rate — was a constant in the source. Nine controls do not belong in
/// a scrolling list of unrelated preferences, and they especially do not belong somewhere you
/// cannot see what they do: the screen they moved to has a live preview at the top.
pub const ROW_VIZ: usize = 3;
pub const ROW_SLEEP: usize = 4;
pub const ROW_SCREEN_OFF: usize = 5;
pub const ROW_BRIGHTNESS: usize = 6;
/// Auto power-off: shut the device down after N minutes of no input AND nothing playing. Sony has
/// this (sid_4118 AutoShutdownSetting) and Cinder did not, so a paused device with the screen dark
/// ran until the battery was flat. Defaults to OFF — powering a device down by itself is the kind
/// of behaviour that has to be asked for.
pub const ROW_AUTO_OFF: usize = 7;
pub const ROW_STORAGE: usize = 8;
pub const ROW_DATABASE: usize = 9;
pub const ROW_BATTERY: usize = 10;
/// Date & time. Sony has this and Cinder did not — the status-bar clock was read-only, so a
/// drifting RTC or a flat battery left no way back to a correct time short of booting stock. The
/// row drills into `clockset`; the shell writes both clocks through the setuid `cinder-clock`
/// helper, because nothing in vendor/sony/lib exposes a clock setter and cinder-home is uid 100.
pub const ROW_CLOCK: usize = 11;
pub const ROW_USB_MODE: usize = 12; // tapping enters USB mass-storage (file transfer to a PC)
/// Boot to stock: arms a ONE-SHOT return to Sony's player, then restarts. Two taps (the row asks
/// for confirmation first) because it reboots the device.
pub const ROW_BOOT_STOCK: usize = 13;
/// Restart and Power off. Both go through the confirmation modal — they take the device away
/// mid-song, and the two-tap row used by Boot to stock is too easy to arm by accident for that.
pub const ROW_RESTART: usize = 14;
pub const ROW_POWER_OFF: usize = 15;
/// Reset every preference to its default. Sony has this (sid_4106 "Reset Settings") and it is the
/// only way out of a settings state you cannot see your way back from — a wrong UI scale, a dark
/// theme at brightness 1, an EQ you have lost track of. Behind the confirmation modal, because it
/// throws away work; it does NOT touch the library, what is playing, or the shelf pins.
pub const ROW_RESET: usize = 16;
/// ABOUT — static info rows, but they still take the cursor, so they need names like the rest.
pub const ROW_FIRMWARE: usize = 17;
pub const ROW_MODEL: usize = 18;

const RH: i32 = 56;
/// How many rows sit under each section eyebrow. DISPLAY | SYSTEM | ABOUT — the single source both
/// `content_height` and `row_at` read, so a row added to one can't be missed by the other.
const SECTIONS: [usize; 3] = [7, 10, 2];

/// Accent swatch geometry. Shared by the render AND `accent_hit` so a tap can never land on a
/// different swatch than the one drawn under the finger (the class of bug the 07-26 input sweep
/// found six times). Swatches are right-aligned to the same 458 edge every other value uses.
const SW: i32 = 30; // swatch edge
const SW_GAP: i32 = 6;
const SW_RIGHT: i32 = 458;
fn swatch_x(i: usize) -> i32 {
    let total = Accent::COUNT as i32 * SW + (Accent::COUNT as i32 - 1) * SW_GAP;
    SW_RIGHT - total + i as i32 * (SW + SW_GAP)
}

/// Firmware/build label shown on the Settings "Firmware" row. The `dev` feature (development
/// channel, built from the same tree) makes the two builds visually distinguishable on-device.
#[cfg(feature = "dev")]
pub const FIRMWARE_LABEL: &str = "CINDER DEV · RUST";
#[cfg(not(feature = "dev"))]
pub const FIRMWARE_LABEL: &str = "CINDER 1.0 · RUST";

/// Current settings values to display.
pub struct SettingsView<'a> {
    pub night: bool,
    /// The Visualiser row's value: style and cover size in one line, since the row is now a
    /// chevron into the screen that owns both.
    pub viz_name: &'a str,
    /// Visualiser size label: OFF / EDGE / FLOOR / VEIL / FULL.
    pub usb_dac: bool,
    pub battery_care: bool,
    /// One-line summary for the Device row, e.g. "99% · 34.4 °C". Formatted by `nav` with the same
    /// helpers the Device screen uses, so the row and the screen can never disagree about a number.
    pub device: &'a str,
    pub storage: &'a str,
    /// Settings ▸ Database value: "N tracks", "Empty", or "Rescanning…" while a scan is out.
    pub database: &'a str,
    pub sleep: &'a str,
    /// Brightness label, e.g. "3 / 5" (nav formats it from its 1..5 level).
    pub brightness: &'a str,
    /// Idle screen-off label, e.g. "OFF" / "30 SEC" / "2 MIN".
    pub screen_off: &'a str,
    /// Auto power-off label, e.g. "OFF" / "30 MIN".
    pub auto_off: &'a str,
    /// Boot-to-stock row value: normally "SONY", or the confirm prompt once armed.
    pub boot_stock: &'a str,
    /// The live clock, shown as the Date & time row's value — so the row is also where you notice
    /// the time is wrong. Formatted by the caller (nav) from the same string the status bar uses.
    pub clock: &'a str,
    /// The selected accent — which swatch gets the ring, and the name shown beside them.
    pub accent: Accent,
}

/// Total height of the row content, from the top of the screen to the bottom of the last row.
/// Exceeds the 800px panel, which is why this screen scrolls.
pub fn content_height() -> i32 {
    // header, then each section: eyebrow (24) + its rows, with a 14px gap before every eyebrow
    // after the first.
    let mut h = LIST_TOP;
    for (i, n) in SECTIONS.iter().enumerate() {
        if i > 0 {
            h += 14;
        }
        h += 24 + *n as i32 * RH;
    }
    h
}

/// How far this screen can scroll. 0 would mean everything fits (it doesn't).
pub fn max_scroll_px() -> i32 {
    (content_height() + 8 - crate::canvas::H as i32).max(0)
}

/// Which selectable row is at touch-y `y`, given the current `scroll` offset? Mirrors `render`'s
/// vertical layout exactly: header ends at 91, each section eyebrow consumes +14 (gap) then +24,
/// and every row is `RH` tall. Returns the row index (0..ROWS) or None (tapped a gap/eyebrow).
pub fn row_at(y: i32, scroll: i32) -> Option<usize> {
    row_span(scroll).find(|(_, top)| y >= *top && y < *top + RH).map(|(r, _)| r)
}

/// Every row as `(index, screen-y of its top)`, in order — the one place the vertical layout is
/// expressed. `render` walks the same section table, so the two cannot drift.
fn row_span(scroll: i32) -> impl Iterator<Item = (usize, i32)> {
    let mut out = Vec::with_capacity(ROWS);
    let mut yy = LIST_TOP - scroll;
    let mut r = 0;
    for (i, n) in SECTIONS.iter().enumerate() {
        if i > 0 {
            yy += 14;
        }
        yy += 24; // section eyebrow
        for _ in 0..*n {
            out.push((r, yy));
            r += 1;
            yy += RH;
        }
    }
    out.into_iter()
}

/// Screen-y of the top of the row list (before scrolling). Exposed for tests and for nav's
/// "keep the cursor visible" arithmetic — the layout constant lives here, not in the caller.
pub const LIST_TOP: i32 = 91;

/// Content-space top y of row `r` (i.e. at scroll 0, measured from LIST_TOP).
pub fn row_top_px(r: usize) -> i32 {
    row_span(0).find(|(i, _)| *i == r).map(|(_, top)| top - 91).unwrap_or(0)
}

/// Which accent swatch is under `(x, y)`, if any. Returns an index into `Accent::ALL`.
/// Checked BEFORE `row_at` by the navigator, so tapping a swatch picks that colour directly
/// instead of advancing the cycle by one — six taps to reach the last accent is not a picker.
pub fn accent_hit(x: i32, y: i32, scroll: i32) -> Option<usize> {
    let (_, top) = row_span(scroll).find(|(r, _)| *r == ROW_ACCENT)?;
    // Full row height vertically: the swatch is 30px inside a 56px row, and a near-miss above or
    // below should still land on the colour the finger was clearly aiming at.
    if y < top || y >= top + RH {
        return None;
    }
    (0..Accent::COUNT).find(|i| {
        let sx = swatch_x(*i);
        // Half the gap on each side counts as the swatch, so there is no dead strip between them.
        x >= sx - SW_GAP / 2 && x < sx + SW + SW_GAP / 2
    })
}

fn eyebrow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    text::draw(c, f, 22.0, (y + 14) as f32, label, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    y + 24
}

// ── UI scale slider ─────────────────────────────────────────────────────────────────────────
// A real slider (track + detents + knob), not a value that cycles on tap: tapping anywhere on the
// track jumps to that stop, and dragging scrubs live (nav routes the gesture through
// `App::scrub_*`). Left/Right on the buttons step one stop.
// The track stops short of the right edge to reserve a gutter for the "NNN%" readout.
const SLIDER_X0: i32 = 176;
const SLIDER_W: i32 = 196;

/// Map a tap/drag x on the UI-scale row to a `text::SCALE_STEPS` index. x is clamped, so grabbing
/// past either end pins to the min/max stop rather than doing nothing.
pub fn ui_scale_idx_at(x: i32) -> usize {
    let n = text::SCALE_STEPS.len() as i32;
    let dx = (x - SLIDER_X0).clamp(0, SLIDER_W);
    // Round to the nearest stop so the knob lands under the finger.
    ((dx * (n - 1) * 2 + SLIDER_W) / (SLIDER_W * 2)).clamp(0, n - 1) as usize
}

/// The UI-scale row. Returns the next y, like `srow`.
fn slider_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str) -> i32 {
    let cy = y + RH / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));

    let n = text::SCALE_STEPS.len() as i32;
    let idx = text::scale_idx() as i32;
    fill_rect(c, SLIDER_X0, cy - 1, SLIDER_W, 2, t.line);
    for i in 0..n {
        let x = SLIDER_X0 + i * SLIDER_W / (n - 1);
        fill_rect(c, x - 1, cy - 4, 2, 8, if i <= idx { t.acc } else { t.line });
    }
    let kx = SLIDER_X0 + idx * SLIDER_W / (n - 1);
    fill_rect(c, SLIDER_X0, cy - 1, kx - SLIDER_X0, 2, t.acc);
    fill_rect(c, kx - 7, cy - 9, 14, 18, t.acc);
    // The readout is drawn at a CONSTANT pixel size: it is the control that SETS the scale, so
    // letting it grow with the scale both crowds the knob at 140% and makes this one row the
    // widest thing on the screen. Compensating here keeps the slider's geometry identical at
    // every stop — and `ui_scale_idx_at` is scale-independent to match.
    let unscaled = 14.0 * 100.0 / text::scale_pct() as f32;
    right(c, f, 458.0, (cy + 4) as f32, &format!("{}%", text::scale_pct()),
          &sty(Family::Mono, Weight::Regular, unscaled, t.faint, 0.04));
    hline(c, y + RH, t.line);
    y + RH
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

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, scroll: i32, v: &SettingsView) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Settings", None);
    // Content is taller than the panel, so it scrolls. Rows are drawn shifted up by `scroll`;
    // row_at applies the same shift, so the hit test can't drift from the render.
    //
    // CLIP TO BELOW THE HEADER. Without this the scrolled rows keep painting upward past
    // HEADER_BOTTOM and cover the "Settings" title and the back chevron — the header is drawn
    // above, so whatever is drawn after simply wins. The library lists have always clipped; this
    // screen gained pixel scrolling later and did not, which is why it was the one that showed it.
    // (The status bar is safe either way: the navigator draws it AFTER every screen.)
    let y0 = y0 - scroll;
    c.set_clip_y(crate::chrome::HEADER_BOTTOM, crate::canvas::H as i32);

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

    // Row 1: Accent — all six swatches at once, the selected one ringed. Showing every choice is
    // the point: a "next colour" row makes you cycle blind through five wrong answers to see the
    // sixth, and on a touch device there is room to just offer them. Tapping a swatch selects it.
    if sel == ROW_ACCENT {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    let cy = y + RH / 2;
    let lc = if sel == ROW_ACCENT { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, "Accent", &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    for (i, a) in Accent::ALL.iter().enumerate() {
        let sx = swatch_x(i);
        let sy = cy - SW / 2;
        // Through the theme's dim: a swatch is a raw palette entry, not a theme colour, so at
        // night it would otherwise be the brightest thing on a deliberately dark screen.
        fill_rect(c, sx, sy, SW, SW, t.scale_color(a.swatch(t.night)));
        if *a == v.accent {
            // The ring is drawn in ink, not in the accent: on BONE the swatch already *is* near-ink,
            // so an accent-coloured ring would vanish on exactly one of the six.
            stroke_rect(c, sx - 3, sy - 3, SW + 6, SW + 6, t.ink, 2);
        }
    }
    hline(c, y + RH, t.line);
    y += RH;

    // Row 2: UI text scale. One global multiplier on every TextStyle size, applied by BOTH
    // text::measure and text::draw — which is what keeps truncation, centring and right-alignment
    // exact at any stop. Row heights and tap targets are deliberately NOT scaled, so no hit test
    // can drift out of step with the render.
    y = slider_row(c, t, f, y, sel == ROW_UI_SCALE, "UI scale");

    // Row 3: the visualiser, as one chevron. The value carries the two facts the old pair of rows
    // showed — which style, and how much of the cover it takes — so the row still answers the
    // question without being opened.
    y = srow(c, t, f, y, sel == ROW_VIZ, "Visualiser", v.viz_name, true);
    // Row 3: Sleep timer (live) — pauses playback after N min. Shows the live remaining when running.
    y = srow(c, t, f, y, sel == ROW_SLEEP, "Sleep timer", v.sleep, false);
    // Row 4: idle screen-off (live). Defaults to OFF, so the panel never blanks on its own unless
    // the user picks a duration.
    y = srow(c, t, f, y, sel == ROW_SCREEN_OFF, "Screen-off timer", v.screen_off, false);
    // Row 5: brightness is live — tapping cycles 1..5 and the shell writes the backlight node.
    y = srow(c, t, f, y, sel == ROW_BRIGHTNESS, "Brightness", v.brightness, false);

    y = eyebrow(c, t, f, y + 14, "SYSTEM");
    // Auto power-off. Distinct from the Screen-off timer above it: that one blanks the panel and
    // keeps playing, this one shuts the device down — and only when nothing is playing, so it can
    // never cut a track off.
    y = srow(c, t, f, y, sel == ROW_AUTO_OFF, "Auto power off", v.auto_off, false);
    // Storage shows the real statvfs value (no chevron — it's a live info row, not a drill-in).
    y = srow(c, t, f, y, sel == ROW_STORAGE, "Storage", v.storage, false);
    // Database: CHEVRON, because tapping now does something. The row used to be drawn without one
    // and with a "—" value, deliberately: it had no arm in settings_activate, and a chevron would
    // have promised a rebuild that never ran. It runs now — it asks Sony's MediaStore to rescan the
    // music tree, which is the thing Cinder could never do and the reason new music stayed
    // invisible and deleted music stayed listed. The value carries the library size, so the row
    // also answers "did it work" without leaving the screen.
    y = srow(c, t, f, y, sel == ROW_DATABASE, "Database", v.database, true);
    // Device: chevron into the hardware's vital signs — battery, die temperatures, CPU clock,
    // memory, storage, uptime. It used to be a toggle in place for Sony's "Itawari" charging and
    // nothing else, which made the status-bar level the only hardware fact Cinder ever showed. The
    // value carries the two numbers people open it for, so the row usually answers the question
    // without being opened at all.
    y = srow(c, t, f, y, sel == ROW_BATTERY, "Device", v.device, true);
    // Chevron: it drills into the clock editor. The value is the live clock, so the row doubles as
    // the place you notice the time is wrong.
    y = srow(c, t, f, y, sel == ROW_CLOCK, "Date & time", v.clock, true);
    y = srow(c, t, f, y, sel == ROW_USB_MODE, "USB mode", if v.usb_dac { "DAC" } else { "MASS STORAGE" }, true);
    // Boot to stock: the only way back to Sony's player that needs no USB cable. Chevron, because
    // it acts. The value doubles as the confirmation prompt (see nav: first tap arms, second goes).
    y = srow(c, t, f, y, sel == ROW_BOOT_STOCK, "Boot to stock", v.boot_stock, true);

    y = srow(c, t, f, y, sel == ROW_RESTART, "Restart", "", true);
    y = srow(c, t, f, y, sel == ROW_POWER_OFF, "Power off", "", true);
    // Chevron, because it acts — behind the confirmation modal. The value names the SCOPE, since
    // "reset" on a music player could just as easily mean the library or the whole device.
    y = srow(c, t, f, y, sel == ROW_RESET, "Reset settings", "PREFERENCES", true);

    y = eyebrow(c, t, f, y + 14, "ABOUT");
    // Named, not literal. These were `sel == 14` and `sel == 15` while ROW_POWER_OFF was 14 — so
    // selecting Power off ALSO highlighted Firmware, and Model could never be highlighted at all.
    // Two hardcoded indices that stopped matching the section table the moment a row was added.
    y = srow(c, t, f, y, sel == ROW_FIRMWARE, "Firmware", FIRMWARE_LABEL, false);
    let _ = srow(c, t, f, y, sel == ROW_MODEL, "Model", "SONY NW-A55", false);
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::FontSet;

    fn view<'a>(accent: Accent) -> SettingsView<'a> {
        SettingsView {
            night: false,
            viz_name: "BARS · VEIL",
            usb_dac: false,
            battery_care: true,
            device: "78% · 34.4 °C",
            database: "3,424 tracks", storage: "12.4 / 58 GB",
            sleep: "30 MIN",
            brightness: "4 / 5",
            screen_off: "OFF",
            auto_off: "OFF",
            boot_stock: "SONY", clock: "17 Aug · 09:01",
            accent,
        }
    }

    /// Scrolling must never repaint the header. The rows move UP as you scroll, and without a clip
    /// they carry on past HEADER_BOTTOM and cover the "Settings" title and the back chevron — the
    /// header is drawn first, so anything drawn after it simply wins. Reported from the device
    /// 2026-07-28 ("the top bars get covered up when scrolling").
    ///
    /// The check is a pixel diff of the whole header band against an unscrolled frame, so it will
    /// catch any future control that reaches up there, not just the rows that did.
    #[test]
    fn scrolling_never_paints_over_the_header() {
        // Renders text, so it must not run concurrently with a test that moves the UI scale.
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut base = Canvas::new();
        render(&mut base, &t, &f, 0, 0, &view(Accent::Amber));

        for scroll in [1, 17, 60, max_scroll_px() / 2, max_scroll_px()] {
            let mut c = Canvas::new();
            render(&mut c, &t, &f, 0, scroll, &view(Accent::Amber));
            for y in 0..crate::chrome::HEADER_BOTTOM {
                for x in 0..crate::canvas::W {
                    let i = y as usize * crate::canvas::W + x;
                    assert_eq!(
                        c.buf[i], base.buf[i],
                        "scroll {scroll} painted over the header at ({x},{y})"
                    );
                }
            }
        }
    }

    /// …and the clip must be released again, or every later screen in the same frame would inherit
    /// it. The navigator draws the status bar and the Now Playing bar after the screen.
    #[test]
    fn the_clip_is_released_after_rendering() {
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut c = Canvas::new();
        render(&mut c, &t, &f, 0, max_scroll_px(), &view(Accent::Amber));
        // A full-screen fill must reach row 0 again; if the clip leaked, the top band stays put.
        c.fill(t.acc);
        assert_eq!(c.buf[0], crate::canvas::to_u32(t.acc), "clip band leaked out of settings::render");
    }

    /// Scrolling still has to actually move the rows — otherwise the test above passes trivially.
    #[test]
    fn scrolling_moves_the_content() {
        let _scale = crate::text::scale_guard();
        let t = Theme::day();
        let f = FontSet::load();
        let mut a = Canvas::new();
        render(&mut a, &t, &f, 0, 0, &view(Accent::Amber));
        let mut b = Canvas::new();
        render(&mut b, &t, &f, 0, max_scroll_px(), &view(Accent::Amber));
        let band = (crate::chrome::HEADER_BOTTOM as usize)..(crate::canvas::H);
        let differing = band
            .flat_map(|y| (0..crate::canvas::W).map(move |x| y * crate::canvas::W + x))
            .filter(|&i| a.buf[i] != b.buf[i])
            .count();
        assert!(differing > 5000, "scrolling barely changed the list ({differing} px)");
    }
}
