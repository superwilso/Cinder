//! Quick settings — the pull-down panel (`docs/PLAN_community_2026-09-23.md` B1).
//!
//! ```text
//!   ┌──────────── status bar ─────────────┐   swipe down from here
//!   BRIGHTNESS
//!   [ 1 ][ 2 ][ 3 ][ 4 ][ 5 ]
//!   Bluetooth   WH-1000XM5              (●)
//!   Night       Dims everything after dark ( )
//!   SLEEP TIMER
//!   [Off][ 15 ][ 30 ][ 45 ][ 60 ]
//!   ───────────── (grab bar) ─────────────
//!   (the screen behind, dimmed: tap it to close)
//! ```
//!
//! Asked for on the r/walkman thread (a Shanling-style pull-down: brightness, Bluetooth, the sleep
//! timer without digging through Settings). **OPTIONAL, and OFF by default** — the owner uses the
//! Shelf and does not want a new gesture arriving unasked. It is switched on by Settings ▸ Pull-down
//! panel; with it off the shell never hands this module a contact, and no screen changes.
//!
//! It is its own overlay with its own state and hit test. It does not borrow the Shelf's file,
//! state or geometry, so the Shelf's code and behaviour are untouched by it.
//!
//! Every control is an action that already exists: brightness is `Action::BrightnessChanged`, the
//! switch is `Action::BtToggle`, night is `Action::ThemeChanged`, the chips are
//! `Action::SleepTimer`. There is no new shell work behind the panel beyond the gesture itself.
//!
//! The panel has no BLE remote toggle, although the request named one: Cinder has no remote
//! support to switch (the RMT-NWS20 is in `docs/PLAN_redesign_2026-09.md` Part E4).

use crate::canvas::{Canvas, H, W};
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline};

/// The sleep-timer presets, in minutes. The same six `nav` cycles through on the Settings row. The
/// last is not a length: [`SLEEP_END_OF_SONG`] pauses when the playing song ends ("stop after
/// current", `docs/SPEC_queue_v2.md` §4), which the shell times from the song's own position.
pub const SLEEP_PRESETS: [u32; 6] = [0, 15, 30, 45, 60, SLEEP_END_OF_SONG];
/// The "End of song" preset. Never a countdown, so it cannot collide with a real length.
pub const SLEEP_END_OF_SONG: u32 = u32::MAX;
const SLEEP_LABELS: [&str; 6] = ["Off", "15", "30", "45", "60", "Song"];
const LEVELS: [&str; 5] = ["1", "2", "3", "4", "5"];

/// The sheet hangs from the bottom of the status bar, which stays visible above it.
pub const TOP: i32 = crate::chrome::STATUS_H;
const BRIGHT_CHIPS: i32 = TOP + kit::SECTION_H;
pub const ROW_BT: i32 = BRIGHT_CHIPS + kit::CHIP_H + 10;
/// The rows below Bluetooth move down one when the Wi-Fi row is there. `wifi` is passed rather than
/// read so the tests can check both layouts; the no-argument forms read the install option.
fn night_at(wifi: bool) -> i32 {
    ROW_BT + kit::ROW_H() * (1 + i32::from(wifi))
}
fn sleep_chips_at(wifi: bool) -> i32 {
    night_at(wifi) + kit::ROW_H() + kit::SECTION_H
}
fn bottom_at(wifi: bool) -> i32 {
    sleep_chips_at(wifi) + kit::CHIP_H + 26
}
fn wifi_row() -> bool {
    crate::wifi::installed()
}

#[allow(non_snake_case)]
pub fn ROW_NIGHT() -> i32 {
    night_at(wifi_row())
}

#[allow(non_snake_case)]
fn SLEEP_LABEL() -> i32 {
    ROW_NIGHT() + kit::ROW_H()
}

#[allow(non_snake_case)]
fn SLEEP_CHIPS() -> i32 {
    sleep_chips_at(wifi_row())
}

/// The sheet's bottom edge. Below it the screen shows through, dimmed, and a tap there closes.
#[allow(non_snake_case)]
pub fn BOTTOM() -> i32 {
    bottom_at(wifi_row())
}

/// What the panel shows. Built by `nav`.
pub struct QuickView<'a> {
    /// 1..5 (a stored 0 draws as no chip selected).
    pub brightness: u8,
    pub bt_on: bool,
    /// The connected device's name, for the Bluetooth row's second line.
    pub bt_device: Option<&'a str>,
    pub night: bool,
    /// Index into [`SLEEP_PRESETS`].
    pub sleep_idx: usize,
    /// The Wi-Fi row: its switch, and the second line (the network, or the state). Drawn only
    /// with the Wi-Fi install option.
    pub wifi_on: bool,
    pub wifi_sub: &'a str,
    /// Settings ▸ Menu icons: the rows carry their icons here too (#53).
    pub icons: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickHit {
    /// A brightness level, 1..=5.
    Brightness(u8),
    /// The Bluetooth row's switch: on or off.
    Bluetooth,
    /// The rest of the Bluetooth row, its name: opens the Bluetooth page (#51).
    BluetoothPage,
    /// The Wi-Fi row's switch, and the rest of it (opens Settings ▸ Wi-Fi), as for Bluetooth.
    Wifi,
    WifiPage,
    Night,
    /// An index into [`SLEEP_PRESETS`].
    Sleep(usize),
    /// On the sheet but on nothing: swallowed, so a near miss never reaches the screen behind.
    Sheet,
    /// Below the sheet: close the panel.
    Outside,
}

/// Left of this the Bluetooth row opens the page; from here right it is the switch. The switch is
/// 40 px wide at the right edge, so this leaves a thumb's width of slack around it.
const SWITCH_ZONE_X: i32 = kit::RIGHT - kit::SWITCH_W - 80;

/// Which control is under `(x, y)`. Pure geometry, so the hit test and the render cannot drift.
pub fn hit(x: i32, y: i32) -> QuickHit {
    hit_at(x, y, wifi_row())
}

fn hit_at(x: i32, y: i32, wifi: bool) -> QuickHit {
    if y >= bottom_at(wifi) {
        return QuickHit::Outside;
    }
    if let Some(i) = kit::chip_at(LEVELS.len(), BRIGHT_CHIPS, x, y) {
        return QuickHit::Brightness(i as u8 + 1);
    }
    if (ROW_BT..ROW_BT + kit::ROW_H()).contains(&y) {
        return if x >= SWITCH_ZONE_X {
            QuickHit::Bluetooth
        } else {
            QuickHit::BluetoothPage
        };
    }
    if wifi && (ROW_BT + kit::ROW_H()..night_at(true)).contains(&y) {
        return if x >= SWITCH_ZONE_X {
            QuickHit::Wifi
        } else {
            QuickHit::WifiPage
        };
    }
    let night = night_at(wifi);
    if (night..night + kit::ROW_H()).contains(&y) {
        return QuickHit::Night;
    }
    if let Some(i) = kit::chip_at(SLEEP_LABELS.len(), sleep_chips_at(wifi), x, y) {
        return QuickHit::Sleep(i);
    }
    QuickHit::Sheet
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &QuickView) {
    // Everything under the panel is covered on purpose (see `Canvas::begin_layer`).
    c.begin_layer();
    // The screen behind, below the sheet, dimmed to under half: still recognisably the place you
    // were, and plainly not the thing to tap.
    for y in BOTTOM()..H as i32 {
        for px in &mut c.buf[y as usize * W..(y as usize + 1) * W] {
            let r = ((*px >> 16) & 0xff) * 40 / 100;
            let g = ((*px >> 8) & 0xff) * 40 / 100;
            let b = (*px & 0xff) * 40 / 100;
            *px = (r << 16) | (g << 8) | b;
        }
    }
    fill_rect(c, 0, TOP, W as i32, BOTTOM() - TOP, t.bg);

    kit::section_label(c, t, f, TOP, crate::i18n::text("BRIGHTNESS"), None);
    let level = (1..=5)
        .contains(&v.brightness)
        .then(|| v.brightness as usize - 1);
    kit::chips(c, t, f, BRIGHT_CHIPS, &LEVELS, level);

    let bt_sub = match (v.bt_on, v.bt_device) {
        (false, _) => crate::i18n::text("Off"),
        (true, Some(name)) => name,
        (true, None) => crate::i18n::text("Not connected"),
    };
    kit::row(
        c,
        t,
        f,
        ROW_BT,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Bluetooth"))
            .sub(bt_sub)
            .icon(v.icons.then_some(crate::icons::bt as kit::Icon))
            .trail(Trail::Switch(v.bt_on)),
    );
    if wifi_row() {
        kit::row(
            c,
            t,
            f,
            ROW_BT + kit::ROW_H(),
            kit::ROW_H(),
            &Row::new(crate::i18n::text("Wi-Fi"))
                .sub(v.wifi_sub)
                .icon(v.icons.then_some(crate::icons::wifi as kit::Icon))
                .trail(Trail::Switch(v.wifi_on)),
        );
    }
    kit::row(
        c,
        t,
        f,
        ROW_NIGHT(),
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Night"))
            .sub(crate::i18n::text("Dims everything after dark"))
            .icon(v.icons.then_some(crate::icons::moon as kit::Icon))
            .trail(Trail::Switch(v.night)),
    );

    kit::section_label(
        c,
        t,
        f,
        SLEEP_LABEL(),
        crate::i18n::text("SLEEP TIMER"),
        None,
    );
    kit::chips(
        c,
        t,
        f,
        SLEEP_CHIPS(),
        &SLEEP_LABELS,
        Some(v.sleep_idx.min(SLEEP_LABELS.len() - 1)),
    );

    // The grab bar says the sheet came from the top and goes back there; the hairline is its edge.
    fill_rect(c, W as i32 / 2 - 20, BOTTOM() - 12, 40, 4, t.ctrl());
    hline(c, BOTTOM() - 1, t.line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every control answers at its centre, and the regions do not overlap.
    #[test]
    fn every_control_answers_where_it_is_drawn() {
        for i in 0..5 {
            let (x, w) = kit::chip_span(i, 5);
            assert_eq!(
                hit(x + w / 2, BRIGHT_CHIPS + kit::CHIP_H / 2),
                QuickHit::Brightness(i as u8 + 1)
            );
        }
        for i in 0..SLEEP_PRESETS.len() {
            let (x, w) = kit::chip_span(i, SLEEP_PRESETS.len());
            assert_eq!(
                hit(x + w / 2, SLEEP_CHIPS() + kit::CHIP_H / 2),
                QuickHit::Sleep(i)
            );
        }
        assert_eq!(
            hit(240, ROW_BT + 30),
            QuickHit::BluetoothPage,
            "the name opens the page"
        );
        assert_eq!(
            hit(kit::RIGHT - kit::SWITCH_W / 2, ROW_BT + 30),
            QuickHit::Bluetooth,
            "the switch switches"
        );
        assert_eq!(hit(240, ROW_NIGHT() + 30), QuickHit::Night);
        assert_eq!(
            hit(240, TOP + 10),
            QuickHit::Sheet,
            "the section label is not a control"
        );
        assert_eq!(hit(240, BOTTOM()), QuickHit::Outside);
        assert_eq!(hit(240, H as i32 - 1), QuickHit::Outside);
    }

    /// With the Wi-Fi option the Wi-Fi row sits under Bluetooth and everything below moves down
    /// with it, hit test and drawing alike.
    #[test]
    fn the_wifi_row_answers_under_bluetooth_and_the_rest_moves_down() {
        let y = ROW_BT + kit::ROW_H() + 30;
        assert_eq!(
            hit_at(240, y, false),
            QuickHit::Night,
            "no Wi-Fi: Night is there"
        );
        assert_eq!(
            hit_at(240, y, true),
            QuickHit::WifiPage,
            "the name opens the page"
        );
        assert_eq!(
            hit_at(kit::RIGHT - kit::SWITCH_W / 2, y, true),
            QuickHit::Wifi
        );
        assert_eq!(hit_at(240, night_at(true) + 30, true), QuickHit::Night);
        assert_eq!(hit_at(240, ROW_BT + 30, true), QuickHit::BluetoothPage);
        for i in 0..SLEEP_PRESETS.len() {
            let (x, w) = kit::chip_span(i, SLEEP_PRESETS.len());
            assert_eq!(
                hit_at(x + w / 2, sleep_chips_at(true) + kit::CHIP_H / 2, true),
                QuickHit::Sleep(i)
            );
        }
        assert_eq!(hit_at(240, bottom_at(true), true), QuickHit::Outside);
        assert!(
            bottom_at(true) < H as i32 * 3 / 5,
            "still a panel: {}",
            bottom_at(true)
        );
    }

    /// The sheet leaves most of the screen showing, so it reads as a panel and not a page.
    #[test]
    fn the_sheet_is_a_panel_not_a_page() {
        assert!(
            bottom_at(false) < H as i32 / 2,
            "the sheet runs to {}",
            bottom_at(false)
        );
        assert_eq!(SLEEP_PRESETS.len(), SLEEP_LABELS.len());
    }
}
