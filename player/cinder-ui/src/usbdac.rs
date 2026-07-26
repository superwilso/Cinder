//! USB-DAC — the Walkman as a USB sound card, with Cinder's headline routing: the incoming USB
//! audio plays to the 3.5 mm jack AND streams out over Bluetooth/LDAC at the same time (stock blocks
//! this and makes you disconnect BT). The header toggle engages it; the shell starts the LDAC bridge
//! and forces UAC mode without tearing down Bluetooth. Codec comes from the device-wide BT pref.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, fit, hline, stroke_rect, sty, toggle};
use crate::Canvas;

/// `on` = USB-DAC engaged. `ldac` = audio is also going out over BT/LDAC (a device is connected).
/// `codec`/`bt_device` describe that BT path; `eq_preset`/`dsee` fill the DSP line.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, on: bool, ldac: bool, codec: &str,
              bt_device: Option<&str>, eq_preset: &str, dsee: bool) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    crate::chrome::header(c, t, f, "USB-DAC", None);
    let onoff = if on { "ON" } else { "OFF" };
    crate::widgets::right(c, f, 416.0, 65.0, onoff, &sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc } else { t.faint }, 0.12));
    toggle(c, t, 424, 56, 34, 18, 12, on);

    if on {
        icons::usb(c, 240.0, 232.0, 40.0, t.acc);
        center(c, f, 240.0, 292.0, "USB-DAC active", &sty(Family::Sans, Weight::Bold, 24.0, t.ink, 0.0));
        let path = if ldac { "PC → NW-A55 → LDAC + 3.5MM" } else { "PC → NW-A55 → 3.5MM" };
        center(c, f, 240.0, 320.0, path, &sty(Family::Mono, Weight::Regular, 13.0, t.acc, 0.1));

        // info box
        let (bx, by, bw, bh) = (50, 352, 380, 148);
        fill_rect(c, bx, by, bw, bh, t.panel);
        stroke_rect(c, bx, by, bw, bh, t.line, 1);
        let out = if ldac {
            let dev = bt_device.unwrap_or("BT");
            fit(f, &format!("OUTPUT : {} → {}  +  3.5MM", codec, dev),
                &sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.04), (bw - 44) as f32)
        } else {
            "OUTPUT : 3.5MM UNBALANCED".to_string()
        };
        let lines = [
            "INPUT  : PCM 24BIT / 96.0 KHZ".to_string(),
            "SOURCE : DESKTOP-7F3K (USB)".to_string(),
            format!("DSP    : EQ {}{}", eq_preset, if dsee { " · DSEE HX" } else { "" }),
            out,
        ];
        let ls = sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.04);
        for (i, ln) in lines.iter().enumerate() {
            text::draw(c, f, (bx + 22) as f32, (by + 28 + i as i32 * 26) as f32, ln, &ls);
        }

        // the key behavioural difference from stock
        if ldac {
            center(c, f, 240.0, 542.0, "Bluetooth stays connected — both outputs are live.",
                   &sty(Family::Sans, Weight::SemiBold, 15.0, t.acc, 0.0));
        } else {
            center(c, f, 240.0, 542.0, &format!("Connect Bluetooth to also stream over {}.", codec),
                   &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        }
    } else {
        icons::usb(c, 240.0, 300.0, 44.0, t.faint);
        center(c, f, 240.0, 362.0, "USB-DAC is off", &sty(Family::Sans, Weight::Bold, 21.0, t.dim, 0.0));
        center(c, f, 240.0, 396.0, "Turn on to use the Walkman as a USB sound card.", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        center(c, f, 240.0, 418.0, "Audio also streams to Bluetooth (LDAC) when connected —", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        center(c, f, 240.0, 438.0, "no need to disconnect.", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    }

    hline(c, 740, t.line);
    center(c, f, 240.0, 770.0, "CHARGING WHILE IN DAC MODE: ON", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}
