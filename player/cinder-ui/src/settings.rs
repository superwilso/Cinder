//! Settings — interactive. Up/Down move the cursor; Select acts on the focused row.
//!
//! TWO LENGTHS. The screen opens as ten rows that fit without scrolling: the things changed in an
//! ordinary week (Display, Brightness, Screen-off timer, Sleep timer, Shuffle, Date & time, Device,
//! USB mode, Power off) and a last row, More settings. That row unfolds the rest below it, in
//! place — LIBRARY, POWER SAVING, SYSTEM, ABOUT — and folds it away again. Nothing was removed and
//! no row changed what it does; the set-once rows no longer stand between a thumb and Brightness.
//! Every row acts except Storage, Firmware and Model, which are information.
//!
//! Drawn with the redesign kit (`kit.rs`): 64 px rows, 34 px section labels. Theme, palette,
//! accent, text size and the visualiser moved to Settings ▸ Display (`display.rs`, handoff 5k) —
//! the things you change weekly no longer share a column with Restart and Reset.

use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::Canvas;

/// Number of selectable rows (for nav cursor clamping). Keep in sync with the rows below.
pub const ROWS: usize = 22;
/// Rows shown while the list is folded: the everyday ones and `ROW_MORE` itself.
pub const EVERYDAY: usize = 10;
/// More settings: unfolds every row after it, in place, and folds them away again.
pub const ROW_MORE: usize = 9;

/// How many rows are on the screen: all of them, or the everyday ones.
pub fn rows(more: bool) -> usize {
    if more { ROWS } else { EVERYDAY }
}
/// Display ▸ — palette, accent, night, the volume readout, text size and the visualiser.
pub const ROW_DISPLAY: usize = 0;
pub const ROW_BRIGHTNESS: usize = 1;
pub const ROW_SCREEN_OFF: usize = 2;
/// The pull-down panel (`quick.rs`): a swipe down from the status bar opens brightness, Bluetooth,
/// night and the sleep timer. Asked for on the r/walkman thread; OFF by default, because the owner
/// uses the Shelf and a new gesture must never arrive unasked (`docs/PLAN_community_2026-09-23.md`
/// B1). With it off there is no gesture and nothing else on any screen changes.
pub const ROW_QUICK: usize = 15;
/// Volume limit — a safe-listening cap on the 3.5 mm level.
///
/// The CAP is Sony's, not ours: `VolumeService` reports an AVLS threshold per output device
/// (63/120 on this unit, `adapt=1`), and it is a real limiter — measured on device 2026-09-07,
/// with it enabled a request for 91 came back 63. What Cinder does NOT do is switch Sony's flag
/// on, because Sony enforces inside `VolumeAdlerOut::SetVolume` and Cinder writes the mixer
/// directly; the flag would be a control that accepts a write and changes nothing, which is
/// exactly what "High gain output" turned out to be (see sound.rs). So the shell reads Sony's
/// number and clamps in its own `apply_volume`.
pub const ROW_VOLUME_LIMIT: usize = 14;
pub const ROW_SLEEP: usize = 3;
/// What the shuffle button deals: SONGS (every track at random, as always), ALBUMS (whole albums
/// at random, each in its own order) or ARTISTS. `shuffle.rs`. SONGS by default, so the button
/// does what it has always done until the owner picks otherwise.
pub const ROW_SHUFFLE: usize = 4;
/// Ignore "The" in artists — sort "The Beatles" among the B's in the Artists tab, the Albums tab's
/// artist groups and Songs by artist (and file it under B on the rail). OFF by default: artists
/// sort as written. Titles and album names keep their "The" either way. See `collate`.
pub const ROW_IGNORE_THE: usize = 10;
pub const ROW_DATABASE: usize = 11;
/// Auto power-off: shut the device down after N minutes of no input AND nothing playing. Sony has
/// this (sid_4118 AutoShutdownSetting) and Cinder did not, so a paused device with the screen dark
/// ran until the battery was flat. 30 MIN by default since 2026-10-04 (it was OFF): the owner's rule
/// is that no state may drain a player nobody is using. OFF is one tap away.
pub const ROW_AUTO_OFF: usize = 12;
/// Bluetooth auto off: switch the radio off after ten minutes of a dark screen with nothing
/// playing over it. The radio (and a link carrying nothing) costs power for as long as it is up,
/// and nothing but the user ever switched it off. ON by default since 2026-10-04, like Auto power
/// off. It does not come back by itself.
pub const ROW_BT_IDLE_OFF: usize = 13;
pub const ROW_STORAGE: usize = 16;
pub const ROW_BATTERY: usize = 6;
/// Date & time. Sony has this and Cinder did not — the status-bar clock was read-only, so a
/// drifting RTC or a flat battery left no way back to a correct time short of booting stock. The
/// row drills into `clockset`; the shell writes both clocks through the setuid `cinder-clock`
/// helper, because nothing in vendor/sony/lib exposes a clock setter and cinder-home is uid 100.
pub const ROW_CLOCK: usize = 5;
pub const ROW_USB_MODE: usize = 7; // tapping enters USB mass-storage (file transfer to a PC)
/// Boot to stock: arms a ONE-SHOT return to Sony's player, then restarts. Two taps (the row asks
/// for confirmation first) because it reboots the device.
pub const ROW_BOOT_STOCK: usize = 17;
/// Restart and Power off. Both go through the confirmation modal — they take the device away
/// mid-song, and the two-tap row used by Boot to stock is too easy to arm by accident for that.
pub const ROW_RESTART: usize = 18;
pub const ROW_POWER_OFF: usize = 8;
/// Reset every preference to its default. Sony has this (sid_4106 "Reset Settings") and it is the
/// only way out of a settings state you cannot see your way back from — a wrong UI scale, a dark
/// theme at brightness 1, an EQ you have lost track of. Behind the confirmation modal, because it
/// throws away work; it does NOT touch the library, what is playing, or the shelf pins.
pub const ROW_RESET: usize = 19;
/// ABOUT — static info rows, but they still take the cursor, so they need names like the rest.
pub const ROW_FIRMWARE: usize = 20;
pub const ROW_MODEL: usize = 21;

const RH: i32 = kit::ROW_H;
/// Section labels and how many rows sit under each — the single source both `content_height` and
/// `row_at` read, so a row added to one can't be missed by the other.
/// The first group is the everyday list and has no label: a list that fits the screen needs none.
const SECTIONS: [(&str, usize); 5] =
    [("", EVERYDAY), ("LIBRARY", 2), ("POWER SAVING", 2), ("SYSTEM", 6), ("ABOUT", 2)];
/// Height of a section's label; the unlabelled first group has none.
fn label_h(label: &str) -> i32 {
    if label.is_empty() { 0 } else { kit::SECTION_H }
}

/// The Display row's second line: what is behind it (handoff 2c).
pub const DISPLAY_SUB: &str = "Palette · accent · volume display";
/// The Shuffle row's second line: which control the setting is about.
pub const SHUFFLE_SUB: &str = "What the shuffle button deals";
/// The More settings row's second line: what unfolds.
pub const MORE_SUB: &str = "Library · power saving · system";
/// The pull-down panel row's second line: where the gesture starts and what it opens.
pub const QUICK_SUB: &str = "Swipe down from the top of the screen";
/// The Bluetooth auto off row's second line: when it acts.
pub const BT_IDLE_OFF_SUB: &str = "After 10 min silent with the screen off";

/// The released version, in ONE place. `tools/release.sh` rewrites this line when it bumps the
/// tag, the same way it rewrites `installer/Cargo.toml` — so what the player shows on its own
/// screen and what the release is called cannot drift apart. It is a macro rather than a `const`
/// because `concat!` takes literals only.
// One line, and rustfmt must leave it so: `tools/release.sh` finds and rewrites it by its shape.
#[macro_export]
#[rustfmt::skip]
macro_rules! cinder_version { () => { "0.3.15" } }

/// Firmware/build label shown on the Settings "Firmware" row. The `dev` feature (development
/// channel, built from the same tree) makes the two builds visually distinguishable on-device.
///
/// It carries the VERSION as of 0.3.12. Until then the stable label read "CINDER 1.0 · RUST" —
/// a number that was never released and never changed, so a player could not say which build was
/// on it, and neither could a bug report.
#[cfg(feature = "dev")]
pub const FIRMWARE_LABEL: &str = concat!("CINDER ", cinder_version!(), " · DEV");
#[cfg(not(feature = "dev"))]
pub const FIRMWARE_LABEL: &str = concat!("CINDER ", cinder_version!());

/// Current settings values to display.
pub struct SettingsView<'a> {
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
    /// Bluetooth auto off: the switch.
    pub bt_idle_off: bool,
    /// Boot-to-stock row value: normally "SONY", or the confirm prompt once armed.
    pub boot_stock: &'a str,
    /// The live clock, shown as the Date & time row's value — so the row is also where you notice
    /// the time is wrong. Formatted by the caller (nav) from the same string the status bar uses.
    pub clock: &'a str,
    /// Volume limit on/off. A toggle, not a value: the cap itself is Sony's and is read live by
    /// the shell, so there is no number here for the user to pick.
    pub volume_limit: bool,
    /// Ignore "The" at the start of artist names when sorting (`ROW_IGNORE_THE`).
    pub ignore_the: bool,
    /// The pull-down panel is switched on (`ROW_QUICK`).
    pub quick: bool,
    /// What shuffle deals (`ROW_SHUFFLE`): "SONGS", "ALBUMS" or "ARTISTS".
    pub shuffle_by: &'a str,
    /// More settings is unfolded (`ROW_MORE`).
    pub more: bool,
}

/// Total height of the row content, from the top of the screen to the bottom of the last row.
/// Exceeds the 800px panel, which is why this screen scrolls.
pub fn content_height(more: bool) -> i32 {
    row_span(0, more).last().map_or(LIST_TOP, |(_, top)| top + RH)
}

/// How far this screen can scroll. 0 would mean everything fits (it doesn't).
pub fn max_scroll_px(more: bool) -> i32 {
    (content_height(more) + 8 - crate::canvas::H as i32).max(0)
}

/// Which selectable row is at touch-y `y`, given the current `scroll` offset? Mirrors `render`'s
/// vertical layout exactly: header ends at 91, each section label is `kit::SECTION_H`, and every
/// row is `RH` tall. Returns the row index (0..ROWS) or None (tapped a gap/eyebrow).
pub fn row_at(y: i32, scroll: i32, more: bool) -> Option<usize> {
    row_span(scroll, more).find(|(_, top)| y >= *top && y < *top + RH).map(|(r, _)| r)
}

/// Every row as `(index, screen-y of its top)`, in order — the one place the vertical layout is
/// expressed. `render` walks the same section table, so the two cannot drift.
fn row_span(scroll: i32, more: bool) -> impl Iterator<Item = (usize, i32)> {
    let mut out = Vec::with_capacity(ROWS);
    let mut yy = LIST_TOP - scroll;
    let mut r = 0;
    for (label, n) in SECTIONS.iter() {
        if r >= rows(more) {
            break;
        }
        yy += label_h(label);
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
    row_span(0, true).find(|(i, _)| *i == r).map(|(_, top)| top - 91).unwrap_or(0)
}

/// The trailing value of row `r`.
fn trail<'a>(r: usize, v: &'a SettingsView) -> Trail<'a> {
    match r {
        ROW_DISPLAY => Trail::Open(""),
        ROW_BRIGHTNESS => Trail::Value(v.brightness),
        ROW_SCREEN_OFF => Trail::Value(v.screen_off),
        ROW_QUICK => Trail::Switch(v.quick),
        // The volume limit is a value row rather than a switch because the interesting half is
        // WHOSE limit it is: "SAFE LEVEL" says a cap is in force without inventing a number the
        // user did not choose, and the number is Sony's per-output AVLS threshold, read live.
        ROW_VOLUME_LIMIT => Trail::Value(if v.volume_limit { "SAFE LEVEL" } else { "OFF" }),
        ROW_SLEEP => Trail::Value(v.sleep),
        ROW_SHUFFLE => Trail::Value(v.shuffle_by),
        ROW_IGNORE_THE => Trail::Value(if v.ignore_the { "ON" } else { "OFF" }),
        // Database: CHEVRON, because tapping does something — it asks Sony's MediaStore to rescan
        // the music tree. The value carries the library size, so the row also answers "did it work".
        ROW_DATABASE => Trail::Open(v.database),
        ROW_AUTO_OFF => Trail::Value(v.auto_off),
        ROW_BT_IDLE_OFF => Trail::Switch(v.bt_idle_off),
        // Storage shows the real statvfs value (no chevron — it's a live info row, not a drill-in).
        ROW_STORAGE => Trail::Value(v.storage),
        // Device: chevron into the hardware's vital signs. The value carries the two numbers people
        // open it for, so the row usually answers the question without being opened at all.
        ROW_BATTERY => Trail::Open(v.device),
        // The value is the live clock, so the row doubles as the place you notice it is wrong.
        ROW_CLOCK => Trail::Open(v.clock),
        ROW_USB_MODE => Trail::Open(if v.usb_dac { "DAC" } else { "MASS STORAGE" }),
        // The value doubles as the confirmation prompt (see nav: first tap arms, second goes).
        ROW_BOOT_STOCK => Trail::Open(v.boot_stock),
        ROW_RESTART | ROW_POWER_OFF => Trail::Open(""),
        ROW_MORE => Trail::Value(if v.more { "HIDE" } else { "SHOW" }),
        // Names the SCOPE, since "reset" on a music player could just as easily mean the library.
        ROW_RESET => Trail::Open("PREFERENCES"),
        ROW_FIRMWARE => Trail::Value(FIRMWARE_LABEL),
        _ => Trail::Value("SONY NW-A55"),
    }
}

fn title(r: usize) -> &'static str {
    match r {
        ROW_DISPLAY => "Display",
        ROW_BRIGHTNESS => "Brightness",
        ROW_SCREEN_OFF => "Screen-off timer",
        ROW_QUICK => "Pull-down panel",
        ROW_VOLUME_LIMIT => "Volume limit",
        ROW_SLEEP => "Sleep timer",
        ROW_SHUFFLE => "Shuffle",
        ROW_IGNORE_THE => "Ignore \"The\" in artists",
        ROW_DATABASE => "Database",
        ROW_AUTO_OFF => "Auto power off",
        ROW_BT_IDLE_OFF => "Bluetooth auto off",
        ROW_STORAGE => "Storage",
        ROW_BATTERY => "Device",
        ROW_CLOCK => "Date & time",
        ROW_USB_MODE => "USB mode",
        ROW_BOOT_STOCK => "Boot to stock",
        ROW_RESTART => "Restart",
        ROW_POWER_OFF => "Power off",
        ROW_MORE => "More settings",
        ROW_RESET => "Reset settings",
        ROW_FIRMWARE => "Firmware",
        _ => "Model",
    }
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
    let mut y = y0 - scroll;
    c.set_clip_y(crate::chrome::HEADER_BOTTOM, crate::canvas::H as i32);
    let mut r = 0usize;
    for (label, n) in SECTIONS {
        if r >= rows(v.more) {
            break;
        }
        if !label.is_empty() {
            y = kit::section_label(c, t, f, y, label, None);
        }
        for _ in 0..n {
            let sub = match r {
                ROW_DISPLAY => DISPLAY_SUB,
                ROW_MORE => MORE_SUB,
                ROW_QUICK => QUICK_SUB,
                ROW_SHUFFLE => SHUFFLE_SUB,
                ROW_BT_IDLE_OFF => BT_IDLE_OFF_SUB,
                _ => "",
            };
            y = kit::row(c, t, f, y, RH, &Row::new(title(r)).sub(sub).trail(trail(r, v)).sel(sel == r));
            r += 1;
        }
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::FontSet;

    fn view<'a>() -> SettingsView<'a> {
        SettingsView {
            volume_limit: false,
            usb_dac: false,
            battery_care: true,
            device: "78% · 34.4 °C",
            database: "3,424 tracks", storage: "12.4 / 58 GB",
            sleep: "30 MIN",
            brightness: "4 / 5",
            screen_off: "OFF",
            auto_off: "OFF",
            bt_idle_off: false,
            boot_stock: "SONY", clock: "17 Aug · 09:01",
            ignore_the: false,
            quick: false,
            shuffle_by: "SONGS",
            more: true,
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
        render(&mut base, &t, &f, 0, 0, &view());

        for scroll in [1, 17, 60, max_scroll_px(true) / 2, max_scroll_px(true)] {
            let mut c = Canvas::new();
            render(&mut c, &t, &f, 0, scroll, &view());
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
        render(&mut c, &t, &f, 0, max_scroll_px(true), &view());
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
        render(&mut a, &t, &f, 0, 0, &view());
        let mut b = Canvas::new();
        render(&mut b, &t, &f, 0, max_scroll_px(true), &view());
        let band = (crate::chrome::HEADER_BOTTOM as usize)..(crate::canvas::H);
        let differing = band
            .flat_map(|y| (0..crate::canvas::W).map(move |x| y * crate::canvas::W + x))
            .filter(|&i| a.buf[i] != b.buf[i])
            .count();
        assert!(differing > 5000, "scrolling barely changed the list ({differing} px)");
    }
}

#[cfg(test)]
mod volume_limit_tests {
    use super::*;

    /// The section table and ROWS are two statements of the same number, and `row_span` walks the
    /// table while every caller indexes by ROWS. A row added to one and not the other silently
    /// drops off the bottom of the screen or hands out a row index nothing renders.
    #[test]
    fn the_section_table_accounts_for_every_row() {
        assert_eq!(SECTIONS.iter().map(|(_, n)| n).sum::<usize>(), ROWS,
                   "SECTIONS {SECTIONS:?} does not add up to ROWS {ROWS}");
    }

    /// Every row must be reachable by a tap at scroll 0 or at the bottom of the scroll range —
    /// including the last one, which is the one a miscounted section table loses first.
    #[test]
    fn every_row_including_the_new_one_is_hittable() {
        for r in 0..ROWS {
            let top = row_top_px(r) + LIST_TOP;
            let scroll = (top + RH / 2 - (crate::canvas::H as i32 / 2)).clamp(0, max_scroll_px(true));
            let y = top - scroll + RH / 2;
            assert_eq!(row_at(y, scroll, true), Some(r), "row {r} not hittable at y={y} scroll={scroll}");
        }
    }

    /// Folded, the list is the everyday rows only, ends on More settings, and fits the panel
    /// without scrolling; unfolded, every row is there.
    #[test]
    fn folded_is_the_everyday_rows_and_fits_the_screen() {
        assert_eq!(ROW_DISPLAY, 0);
        assert_eq!(ROW_MORE + 1, EVERYDAY);
        assert_eq!(max_scroll_px(false), 0, "the folded list must not scroll");
        let last = LIST_TOP + EVERYDAY as i32 * RH - RH / 2;
        assert_eq!(row_at(last, 0, false), Some(ROW_MORE));
        assert_eq!(row_at(last + RH, 0, false), None, "nothing below More settings while folded");
        assert_eq!(row_at(last + RH + kit::SECTION_H, 0, true), Some(ROW_IGNORE_THE));
    }
}
