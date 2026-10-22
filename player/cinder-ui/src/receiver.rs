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
    /// Raw values from BtPlayerService (GetTrackCodec / GetTrackFreq / GetBitrate). The receiver
    /// codec label decodes only the stock-confirmed SBC/AAC/LDAC values; other values stay raw.
    pub codec: u32,
    pub freq: u32,
    pub bitrate: u32,
    /// The Bluetooth radio is on — the mode cannot start without it.
    pub radio: bool,
    /// Receiver-only capability preference: 0 Auto, 1 prefer LDAC, 2 prefer AAC, 3 prefer SBC.
    pub codec_pref: u8,
}

pub const SWITCH_Y: i32 = crate::chrome::HEADER_BOTTOM + 16;
pub const SWITCH_H: i32 = 76;
const STATUS_Y: i32 = SWITCH_Y + SWITCH_H + 28;
pub const CODEC_LABEL_Y: i32 = STATUS_Y + 90;
pub const CODEC_CHIPS_Y: i32 = CODEC_LABEL_Y + 21;
const CODEC_COLS: usize = 2;
const CODEC_PITCH: i32 = crate::kit::CHIP_H + 8;
const CODEC_HINT_Y: i32 = CODEC_CHIPS_Y + CODEC_PITCH + crate::kit::CHIP_H + 19;
const LDAC_HINT_Y: i32 = CODEC_HINT_Y + 21;
const CODEC_PREFS: [&str; 4] = ["Auto", "LDAC or SBC", "AAC or SBC", "SBC only"];

/// "Use Sony's receiver": restart into the stock player with its receiver open. Below the status
/// lines (at most three, ending by y≈290) and well clear of the notes rule at 580.
pub const SONY_Y: i32 = 489;
pub const SONY_H: i32 = kit::ROW_H;

/// Did this tap land on the "Use Sony's receiver" row?
pub fn sony_hit(_x: i32, y: i32) -> bool {
    (SONY_Y..SONY_Y + SONY_H).contains(&y)
}

/// Did this tap land on the switch row?
pub fn switch_hit(_x: i32, y: i32) -> bool {
    (SWITCH_Y..SWITCH_Y + SWITCH_H).contains(&y)
}

/// Centre of one receiver codec preference chip, shared by rendering and navigation tests.
pub fn codec_pref_chip(index: usize) -> (i32, i32) {
    let index = index.min(CODEC_PREFS.len() - 1);
    let (x, w) = crate::kit::chip_span(index % CODEC_COLS, CODEC_COLS);
    let row = (index / CODEC_COLS) as i32;
    (
        x + w / 2,
        CODEC_CHIPS_Y + row * CODEC_PITCH + crate::kit::CHIP_H / 2,
    )
}

/// Which receiver codec preference chip was tapped, if any.
pub fn codec_pref_hit(x: i32, y: i32) -> Option<u8> {
    for row in 0..2 {
        let top = CODEC_CHIPS_Y + row * CODEC_PITCH;
        if let Some(col) = crate::kit::chip_at(CODEC_COLS, top, x, y) {
            return Some((row as usize * CODEC_COLS + col) as u8);
        }
    }
    None
}

/// The receiver's BtPlayerService codec enum is distinct from the transmitter's BtSoundCodec.
/// Stock HgrmMediaPlayerApp maps 1 = SBC, 3 = AAC, 4 = LDAC. Preserve raw 2 (MP3) and any other
/// values as numbers; only the three supported receiver choices are presented in the UI.
pub fn codec_label(raw: u32) -> String {
    match raw {
        1 => "SBC".into(),
        3 => "AAC".into(),
        4 => "LDAC".into(),
        _ => format!("CODEC 0x{raw:02X}"),
    }
}

/// The status lines for a state, most important first. Split out so it is testable.
/// Receiver service codec values are independent of the transmitter codec selectors.
pub fn rx_codec_label(raw: u8) -> String {
    match raw {
        1 => "SBC".into(),
        3 => "AAC".into(),
        4 => "LDAC".into(),
        _ => format!("CODEC 0x{raw:02X}"),
    }
}

pub fn status_lines(rx: &Rx) -> Vec<String> {
    if !rx.radio && !rx.on {
        return vec![crate::i18n::text("Bluetooth is off. Turn it on first.").into()];
    }
    match (rx.on, rx.phase) {
        (false, _) => vec![
            crate::i18n::text("Off.").into(),
            crate::i18n::text("Turn it on, then pair from your phone or PC:").into(),
            crate::i18n::text("look for this Walkman in its Bluetooth list.").into(),
        ],
        (true, 0) | (true, 1) => vec![
            crate::i18n::text("Waiting for a device.").into(),
            crate::i18n::text("Pair or connect from your phone or PC now.").into(),
            crate::i18n::text("If a code appears, check it matches, then YES.").into(),
        ],
        (true, 2) => vec![
            crate::ui_format!(
                "Connected{}",
                if rx.peer.is_empty() {
                    String::new()
                } else {
                    format!(": {}", rx.peer)
                }
            ),
            crate::i18n::text("Start playing on the other device.").into(),
        ],
        (true, _) => vec![
            crate::ui_format!(
                "Playing{}",
                if rx.peer.is_empty() {
                    String::new()
                } else {
                    crate::ui_format!(" from {}", rx.peer)
                }
            ),
            // This is BtPlayerService's receiver codec enum, distinct from the transmitter's
            // BtSoundCodec. GetBitrate read 1 throughout a working stream, so it is not a kbps
            // figure and is left off.
            if rx.freq >= 8000 {
                format!(
                    "{} · {:.1} kHz",
                    codec_label(rx.codec),
                    rx.freq as f32 / 1000.0
                )
            } else {
                codec_label(rx.codec)
            },
        ],
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, rx: &Rx) {
    c.fill(t.bg);
    crate::chrome::header(
        c,
        t,
        f,
        crate::i18n::text("BT Receiver"),
        Some(crate::i18n::text("EXPERIMENTAL")),
    );

    let sub = if rx.on {
        crate::i18n::text("On · your music is paused")
    } else {
        crate::i18n::text("Play from a phone or PC into this Walkman")
    };
    kit::row(
        c,
        t,
        f,
        SWITCH_Y,
        SWITCH_H,
        &Row::new(crate::i18n::text("Receiver mode"))
            .sub(sub)
            .trail(Trail::Switch(rx.on)),
    );

    // The fallback: Sony's own receiver, via a restart. Here and not in Settings because this is
    // where someone finds out Cinder's does not pair with their phone.
    hline(c, SONY_Y - 1, t.line);
    kit::row(
        c,
        t,
        f,
        SONY_Y,
        SONY_H,
        &Row::new(crate::i18n::text("Use Sony's receiver"))
            .sub(crate::i18n::text("Restarts into the Sony player"))
            .trail(Trail::Open("")),
    );

    const AVAIL: f32 = 436.0;
    let mut y = STATUS_Y as f32;
    for (i, line) in status_lines(rx).iter().enumerate() {
        let st = if i == 0 {
            sty(
                Family::Sans,
                Weight::SemiBold,
                19.0,
                if rx.on { t.ink } else { t.dim },
                0.0,
            )
        } else {
            sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0)
        };
        let s = crate::widgets::fit(f, line, &st, AVAIL);
        crate::text::draw(c, f, 22.0, y, &s, &st);
        y += if i == 0 { 30.0 } else { 22.0 };
    }

    let label_style = sty(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION,
        t.faint,
        0.6,
    );
    crate::text::draw(
        c,
        f,
        22.0,
        CODEC_LABEL_Y as f32,
        crate::i18n::text("BT RECEIVE CODEC"),
        &label_style,
    );
    for (row, labels) in CODEC_PREFS.chunks(CODEC_COLS).enumerate() {
        let first = row * CODEC_COLS;
        let selected = rx.codec_pref.min((CODEC_PREFS.len() - 1) as u8) as usize;
        let selected = (first..first + labels.len())
            .contains(&selected)
            .then(|| selected - first);
        kit::chips_in(
            c,
            t,
            f,
            CODEC_CHIPS_Y + row as i32 * CODEC_PITCH,
            labels,
            selected,
            CODEC_COLS,
        );
    }
    let hint_style = sty(
        Family::Sans,
        Weight::Regular,
        crate::scale::CAPTION,
        t.faint,
        0.0,
    );
    let hint = crate::widgets::fit(
        f,
        crate::i18n::text("Your phone picks from what is allowed."),
        &hint_style,
        AVAIL,
    );
    crate::text::draw(c, f, 22.0, CODEC_HINT_Y as f32, &hint, &hint_style);
    if rx.codec_pref <= 1 {
        let hint = crate::widgets::fit(
            f,
            crate::i18n::text("LDAC bitrate and sample rate are set on your phone."),
            &hint_style,
            AVAIL,
        );
        crate::text::draw(c, f, 22.0, LDAC_HINT_Y as f32, &hint, &hint_style);
    }

    let notes = [
        crate::i18n::text("Sound comes out of the headphone jack."),
        crate::i18n::text("Bluetooth headphones do not reconnect while it is on."),
        crate::i18n::text("Leaving this page turns it off and gives the music back."),
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
    crate::widgets::center(
        c,
        f,
        240.0,
        770.0,
        crate::i18n::text("EXPERIMENTAL · TESTED WITH WINDOWS AND IPHONE"),
        &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rx(on: bool, phase: u8) -> Rx<'static> {
        Rx {
            on,
            phase,
            peer: "ARTHURS-PC",
            codec: 3,
            freq: 48000,
            bitrate: 1,
            radio: true,
            codec_pref: 0,
        }
    }

    #[test]
    fn receiver_codec_values_are_independent_from_transmitter_values() {
        assert_eq!(rx_codec_label(1), "SBC");
        assert_eq!(rx_codec_label(3), "AAC");
        assert_eq!(rx_codec_label(4), "LDAC");
        assert_eq!(rx_codec_label(2), "CODEC 0x02");
        assert_eq!(rx_codec_label(255), "CODEC 0xFF");
    }

    #[test]
    fn every_state_says_what_to_do_next() {
        assert!(status_lines(&rx(false, 0))[0].starts_with("Off"));
        assert!(status_lines(&rx(true, 1))[1].contains("Pair or connect"));
        assert_eq!(status_lines(&rx(true, 2))[0], "Connected: ARTHURS-PC");
        assert!(status_lines(&rx(true, 3))[0].starts_with("Playing from ARTHURS-PC"));
        assert_eq!(status_lines(&rx(true, 3))[1], "AAC · 48.0 kHz");
    }

    #[test]
    fn a_radio_that_is_off_is_named_before_anything_else() {
        let r = Rx {
            radio: false,
            ..rx(false, 0)
        };
        assert!(status_lines(&r)[0].contains("Bluetooth is off"));
    }

    #[test]
    fn the_sony_row_does_not_overlap_the_switch() {
        assert!(sony_hit(240, SONY_Y + 1));
        assert!(!switch_hit(240, SONY_Y + 1));
        assert!(!sony_hit(240, SWITCH_Y + 1));
        assert!(SONY_Y + SONY_H < 580, "clear of the notes");
    }

    #[test]
    fn the_switch_row_is_its_own_target() {
        assert!(switch_hit(240, SWITCH_Y + 1));
        assert!(switch_hit(240, SWITCH_Y + SWITCH_H - 1));
        assert!(!switch_hit(240, SWITCH_Y + SWITCH_H + 5));
    }

    #[test]
    fn receiver_codec_values_do_not_reuse_transmitter_enum() {
        assert_eq!(codec_label(3), "AAC");
        assert_eq!(codec_label(4), "LDAC");
        assert_eq!(codec_label(1), "SBC");
        assert_eq!(codec_label(2), "CODEC 0x02");
        assert_eq!(codec_label(0x17), "CODEC 0x17");
    }

    #[test]
    fn receiver_codec_chips_have_distinct_hits_and_fit_before_sony_entry() {
        for pref in 0..4 {
            let (x, y) = codec_pref_chip(pref);
            assert_eq!(codec_pref_hit(x, y), Some(pref as u8));
        }
        assert!(CODEC_LABEL_Y > STATUS_Y);
        assert!(CODEC_HINT_Y < SONY_Y);
        assert!(LDAC_HINT_Y < SONY_Y);
        assert!(SONY_Y + SONY_H < 580, "clear of the notes section");
    }
}
