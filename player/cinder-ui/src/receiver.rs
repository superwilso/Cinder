//! BT Receiver — the Walkman as a Bluetooth SINK: play from a phone or a PC, and the Walkman becomes
//! the DAC and amp for the wired headphones plugged into it.
//!
//! EXPERIMENTAL, AND THIS SCREEN SAYS SO. The chain the shell runs is Sony's own, read out of
//! HgrmMediaPlayerApp's `BtReceiver*` states and `BtPlayerModel` (2026-09-29): release the music
//! player, `EnterFuncMode(A2dpSink)`, `BtPlayerService::RequestStartConnectWait`, make the radio
//! discoverable, then `StartSound` once a device attaches. On the reference player a PC found it
//! and reached the pairing code with exactly that; a finished pairing and sound at the jack have
//! not been seen yet. `analysis/G_bt_nfc/RE_findings.md`, "Receiver, 2026-09-29".
//!
//! The pairing code appears ON THIS SCREEN (the prompt used to draw only on Devices, and a PC
//! pairing to the Walkman sat on its PIN until it gave up). Leaving the screen turns the mode off,
//! so there is never a Walkman stuck as a speaker with nothing on screen to say so.

use crate::kit::{self, Row, Trail};
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{hline, sty};
use crate::Canvas;

/// What the shell last reported, plus the user's switch.
pub struct Rx<'a> {
    pub on: bool,
    /// 0 off, 1 waiting for a device, 2 a device is attached, 3 audio is arriving. From
    /// BtPlayerService's AVSNK status, read on the device 2026-09-29: 1 idle, 2 waiting,
    /// 4 connected, 5 streaming.
    pub phase: u8,
    pub peer: &'a str,
    /// Raw values from BtPlayerService (GetTrackCodec / GetTrackFreq / GetBitrate). The codec and
    /// frequency enumerators are not decoded, so they are shown as numbers.
    pub codec: u32,
    pub freq: u32,
    pub bitrate: u32,
    /// The Bluetooth radio is on — the mode cannot start without it.
    pub radio: bool,
}

pub const SWITCH_Y: i32 = crate::chrome::HEADER_BOTTOM + 16;
pub const SWITCH_H: i32 = 76;
const STATUS_Y: i32 = SWITCH_Y + SWITCH_H + 28;

/// Did this tap land on the switch row?
pub fn switch_hit(_x: i32, y: i32) -> bool {
    (SWITCH_Y..SWITCH_Y + SWITCH_H).contains(&y)
}

/// The status lines for a state, most important first. Split out so it is testable.
pub fn status_lines(rx: &Rx) -> Vec<String> {
    if !rx.radio && !rx.on {
        return vec!["Bluetooth is off. Turn it on first.".into()];
    }
    match (rx.on, rx.phase) {
        (false, _) => vec![
            "Off.".into(),
            "Turn it on, then pair from your phone or PC:".into(),
            "look for this Walkman in its Bluetooth list.".into(),
        ],
        (true, 0) | (true, 1) => vec![
            "Waiting for a device.".into(),
            "Pair or connect from your phone or PC now.".into(),
            "If a code appears, check it matches, then YES.".into(),
        ],
        (true, 2) => vec![
            format!("Connected{}", if rx.peer.is_empty() { String::new() } else { format!(": {}", rx.peer) }),
            "Start playing on the other device.".into(),
        ],
        (true, _) => vec![
            format!("Playing{}", if rx.peer.is_empty() { String::new() } else { format!(" from {}", rx.peer) }),
            // Raw codec byte (Sony's enum; 0x02 is LDAC on the transmitter side, 0x03 was what a
            // Windows PC sent — not decoded yet). GetBitrate read 1 throughout a working stream, so
            // it is not a kbps figure and is left off.
            if rx.freq >= 8000 {
                format!("codec 0x{:02x} · {:.1} kHz", rx.codec, rx.freq as f32 / 1000.0)
            } else {
                format!("codec 0x{:02x}", rx.codec)
            },
        ],
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, rx: &Rx) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "BT Receiver", Some("EXPERIMENTAL"));

    let sub = if rx.on { "On · your music is paused" } else { "Play from a phone or PC into this Walkman" };
    kit::row(c, t, f, SWITCH_Y, SWITCH_H,
             &Row::new("Receiver mode").sub(sub).trail(Trail::Switch(rx.on)));

    const AVAIL: f32 = 436.0;
    let mut y = STATUS_Y as f32;
    for (i, line) in status_lines(rx).iter().enumerate() {
        let st = if i == 0 {
            sty(Family::Sans, Weight::SemiBold, 19.0, if rx.on { t.ink } else { t.dim }, 0.0)
        } else {
            sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0)
        };
        let s = crate::widgets::fit(f, line, &st, AVAIL);
        crate::text::draw(c, f, 22.0, y, &s, &st);
        y += if i == 0 { 30.0 } else { 22.0 };
    }

    let notes = [
        "Sound comes out of the headphone jack.",
        "Bluetooth headphones do not reconnect while it is on.",
        "Leaving this page turns it off and gives the music back.",
    ];
    let nst = sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0);
    let mut ny = 600.0;
    hline(c, 580, t.line);
    for n in notes {
        let s = crate::widgets::fit(f, n, &nst, AVAIL);
        crate::text::draw(c, f, 22.0, ny, &s, &nst);
        ny += 22.0;
    }
    hline(c, 740, t.line);
    crate::widgets::center(c, f, 240.0, 770.0, "EXPERIMENTAL · NOT YET HEARD AT THE JACK",
                           &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rx(on: bool, phase: u8) -> Rx<'static> {
        Rx { on, phase, peer: "ARTHURS-PC", codec: 3, freq: 48000, bitrate: 1, radio: true }
    }

    #[test]
    fn every_state_says_what_to_do_next() {
        assert!(status_lines(&rx(false, 0))[0].starts_with("Off"));
        assert!(status_lines(&rx(true, 1))[1].contains("Pair or connect"));
        assert_eq!(status_lines(&rx(true, 2))[0], "Connected: ARTHURS-PC");
        assert!(status_lines(&rx(true, 3))[0].starts_with("Playing from ARTHURS-PC"));
        assert_eq!(status_lines(&rx(true, 3))[1], "codec 0x03 · 48.0 kHz");
    }

    #[test]
    fn a_radio_that_is_off_is_named_before_anything_else() {
        let r = Rx { radio: false, ..rx(false, 0) };
        assert!(status_lines(&r)[0].contains("Bluetooth is off"));
    }

    #[test]
    fn the_switch_row_is_its_own_target() {
        assert!(switch_hit(240, SWITCH_Y + 1));
        assert!(switch_hit(240, SWITCH_Y + SWITCH_H - 1));
        assert!(!switch_hit(240, SWITCH_Y + SWITCH_H + 5));
    }
}
