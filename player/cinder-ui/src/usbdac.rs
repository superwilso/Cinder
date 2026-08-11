//! USB-DAC — the Walkman as a USB sound card. Cinder's difference from stock is that **Bluetooth
//! does not have to be disconnected** to use it: stock shows a "disconnect Bluetooth" overlay and
//! tears the link down, and Cinder simply does not.
//!
//! THE OUTPUT DEPENDS ON WHAT IS CONNECTED, and this screen has been wrong about it in both
//! directions before, so the rule is: say only what the engine actually does. With headphones
//! connected the capture is bridged to them (`ldac_start` in cinder-home, handshake + PCM over
//! `BtTransmitterService`'s socket — proven on device 2026-08-11, tone audible, byte rate matching
//! a live A2DP stream). With nothing connected it renders to the jack. The `ldac` flag below is
//! exactly that condition, so it selects the copy for both cases.
//!
//! Earlier versions of this file claimed "LDAC + 3.5MM, both outputs live" (never true — it is one
//! or the other) and then "audio goes to the 3.5 mm jack, not over Bluetooth" (true only for the
//! hours the bridge was disabled). Neither claim was measured at the time; this one was.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty, toggle};
use crate::Canvas;

/// `on` = USB-DAC engaged. `ldac` = a Bluetooth device is connected, which is also the condition
/// under which the engine bridges the USB capture to it; `codec`/`bt_device` describe that link,
/// and `eq_preset`/`dsee` fill the DSP line.
/// The ON/OFF switch in the header, as drawn (`toggle` at 424,56 34×18 plus its ON/OFF label).
const TOGGLE: (i32, i32, i32, i32) = (424, 56, 34, 18);

/// True if `(x, y)` is on the USB-DAC switch.
///
/// This screen used to toggle on a tap *anywhere*, which meant a stray touch while reading the
/// panel silently engaged USB-DAC — a disruptive action that reconfigures the USB gadget and drops
/// the PC's connection to the device.
/// switches the USB gadget mode. Now only the switch toggles it. The target is padded out to ≥44px in
/// both directions (the drawn switch is only 34×18) so it stays easy to hit.
pub fn hit_toggle(x: i32, y: i32) -> bool {
    let (tx, ty, tw, th) = TOGGLE;
    let (cx, cy) = (tx + tw / 2, ty + th / 2);
    // Include the "ON"/"OFF" label to the left of the switch — it reads as part of the control.
    (cx - 60..cx + 34).contains(&x) && (cy - 22..cy + 22).contains(&y)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, on: bool, ldac: bool, codec: &str,
              bt_device: Option<&str>, eq_preset: &str, dsee: bool,
              fmt: Option<(u32, u32, u32)>, negotiated: Option<&str>) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "USB-DAC", None);
    let onoff = if on { "ON" } else { "OFF" };
    crate::widgets::right(c, f, 416.0, 65.0, onoff, &sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc } else { t.faint }, 0.12));
    let (tx, ty, tw, th) = TOGGLE;
    toggle(c, t, tx, ty, tw, th, 12, on);

    if on {
        icons::usb(c, 240.0, 232.0, 40.0, t.acc);
        center(c, f, 240.0, 292.0, "USB-DAC active", &sty(Family::Sans, Weight::Bold, 24.0, t.ink, 0.0));
        // One path or the other, never both: the bridge takes the capture PCM for the whole
        // session, so when it runs the jack gets nothing. `ldac` is the engine's own condition for
        // choosing, so the two stay in step by construction rather than by comment.
        // NAME THE OUTPUT ONLY IF IT IS KNOWN. `codec` is the user's preference and A2DP
        // negotiates, so a sink that cannot do LDAC lands on SBC with the preference unchanged —
        // printing it here would be the third false claim this screen has made about its own
        // output. `negotiated` comes from GetSoundStatus and is None until its enumerators are tied
        // to a real headphone, so until then the honest label is the transport, not the codec.
        let out_name = negotiated.unwrap_or("BLUETOOTH");
        let path = if ldac { format!("PC → NW-A55 → {}", out_name) }
                   else { "PC → NW-A55 → 3.5MM".to_string() };
        center(c, f, 240.0, 320.0, &path,
               &sty(Family::Mono, Weight::Regular, 13.0, t.acc, 0.1));

        // info box
        // Height tracks the line count: this was 148 for four lines and now holds three, since the
        // fabricated INPUT rate and SOURCE hostname were removed rather than replaced.
        let (bx, by, bw, bh) = (50, 352, 380, 112);
        fill_rect(c, bx, by, bw, bh, t.panel);
        stroke_rect(c, bx, by, bw, bh, t.line, 1);
        let out = if ldac { format!("OUTPUT : {}", out_name) }
                  else { "OUTPUT : 3.5MM UNBALANCED".to_string() };
        // This line used to read "INPUT : PCM 24BIT / 96.0 KHZ" as a hardcoded literal, next to a
        // "SOURCE : DESKTOP-7F3K (USB)" that named a PC which does not exist. Both were invented,
        // and when the format was finally measured (2026-08-11) the real stream was 32-bit at
        // 44.1 kHz — the fake numbers were not even close. So the line went generic, and now it is
        // live: `fmt` is Sony's own stream_info_t, carried across the FFI by the engine's GetStatus
        // poll. `None` means nothing is streaming, and the generic line is the honest answer then.
        let input = match fmt {
            Some((rate, bits, _)) if rate > 0 => {
                let khz = rate as f32 / 1000.0;
                if bits > 0 {
                    format!("INPUT  : PCM {}BIT / {:.1} KHZ", bits, khz)
                } else {
                    format!("INPUT  : PCM {:.1} KHZ", khz)
                }
            }
            _ => "INPUT  : USB AUDIO CLASS 2".to_string(),
        };
        let lines = [
            input,
            format!("DSP    : EQ {}{}", eq_preset, if dsee { " · DSEE HX" } else { "" }),
            out,
        ];
        let ls = sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.04);
        for (i, ln) in lines.iter().enumerate() {
            text::draw(c, f, (bx + 22) as f32, (by + 28 + i as i32 * 26) as f32, ln, &ls);
        }

        // The thing Cinder does that stock refuses to: stock shows a "disconnect Bluetooth" overlay
        // on entering DAC mode and tears the link down. Here the link stays up and the USB audio
        // goes out over it.
        if ldac {
            let dev = bt_device.unwrap_or("Bluetooth");
            center(c, f, 240.0, 542.0,
                   &format!("Playing to {} — {} requested.", dev, codec),
                   &sty(Family::Sans, Weight::SemiBold, 15.0, t.acc, 0.0));
            center(c, f, 240.0, 564.0, "Stock makes you disconnect Bluetooth first. This does not.",
                   &sty(Family::Sans, Weight::Regular, 14.0, t.faint, 0.0));
        } else {
            center(c, f, 240.0, 542.0, "Connect Bluetooth headphones to send USB audio to them.",
                   &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        }
    } else {
        icons::usb(c, 240.0, 300.0, 44.0, t.faint);
        center(c, f, 240.0, 362.0, "USB-DAC is off", &sty(Family::Sans, Weight::Bold, 21.0, t.dim, 0.0));
        center(c, f, 240.0, 396.0, "Turn on to use the Walkman as a USB sound card.", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        center(c, f, 240.0, 418.0, "Plays out of the 3.5 mm jack, or over Bluetooth", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
        center(c, f, 240.0, 438.0, "headphones if any are connected.", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    }

    hline(c, 740, t.line);
    center(c, f, 240.0, 770.0, "CHARGING WHILE IN DAC MODE: ON", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}
