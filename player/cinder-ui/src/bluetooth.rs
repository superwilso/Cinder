//! Bluetooth — on/off, the connected device, and the **transmit codec** selector (the device-wide
//! preference used for both normal BT playback and the USB-DAC→LDAC bridge). Codecs this hardware
//! can transmit: LDAC · aptX HD · aptX · SBC (AAC is receive-only, excluded). When LDAC is the
//! choice, a sound-quality sub-row (Auto/990/660/330) appears. Geometry lives in `hit()` so the
//! navigator and the renderer can't drift.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, right, stroke_rect, sty};
use crate::Canvas;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

/// Transmit codecs, in display order. Index = the persisted `bt_codec` value.
pub const CODECS: [(&str, &str); 4] = [
    ("LDAC", "Up to 990 kbps · Hi-Res"),
    ("aptX HD", "576 kbps · 24-bit"),
    ("aptX", "352 kbps · low latency"),
    ("SBC", "Universal · always works"),
];
/// LDAC sound-quality tiers. Index = the persisted `bt_ldac_quality` value.
pub const QUALITIES: [&str; 4] = ["Auto", "990", "660", "330"];

pub const LDAC: u8 = 0; // codec index whose quality sub-row is shown

pub struct Bt<'a> {
    pub on: bool,
    pub connected: Option<&'a str>,
    /// Does the shell actually KNOW the link state on this firmware? False = no detector was
    /// found, so we say so rather than claiming "No device connected" (which would be a guess) —
    /// and certainly rather than the hard-coded "WH-1000XM5 · CONNECTED" this screen used to show
    /// whenever the on/off toggle happened to be on.
    pub link_known: bool,
    pub codec_sel: u8,    // index into CODECS
    pub ldac_quality: u8, // index into QUALITIES (only meaningful when codec_sel == LDAC)
    /// Sony's "Use Enhanced Mode" (firmware message 230077, helped by 230079 "Select this check
    /// box if you cannot change the volume"). It is the AVRCP **absolute-volume** switch:
    /// `BtTransmitterService::SetControlAbsoluteVolume`. On, the player sends the headphone the
    /// level it should sit at; off, it sends VOLUME_UP/VOLUME_DOWN key events, which many sinks
    /// answer with their own volume beep.
    pub enhanced: bool,
    /// Does the CONNECTED sink accept absolute volume (`IsSupportedAbsoluteVolume`)? Pushed by the
    /// shell. False = the row still shows, but says the sink can't do it rather than pretending.
    pub enhanced_supported: bool,
}

// ---- layout (shared by render + hit) ----
const CARD_Y: i32 = 92;
const CARD_H: i32 = 86;
const DISC: (i32, i32, i32, i32) = (348, 136, 104, 34); // x,y,w,h
const CODEC_Y0: i32 = 212;
const CODEC_RH: i32 = 44;
const QUAL_Y: i32 = 420;
const QUAL_H: i32 = 40;
const ENH_Y: i32 = 556; // "Use Enhanced Mode" row (absolute volume)
const ENH_H: i32 = 64;
const PAIR_Y: i32 = 700;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BtHit {
    None,
    Toggle,
    Disconnect,
    Codec(usize),
    Quality(usize),
    /// "Use Enhanced Mode" toggled — the absolute-volume switch.
    Enhanced,
    Pair,
}

/// Map a tap to a Bluetooth action. `codec_is_ldac` gates the quality chips (only shown for LDAC).
pub fn hit(x: i32, y: i32, on: bool, codec_is_ldac: bool) -> BtHit {
    // header on/off toggle works regardless of state
    if (48..82).contains(&y) && x > 408 {
        return BtHit::Toggle;
    }
    if !on {
        return BtHit::None; // everything else is inert while BT is off
    }
    let (dx, dy, dw, dh) = DISC;
    if (dy..dy + dh).contains(&y) && (dx..dx + dw).contains(&x) {
        return BtHit::Disconnect;
    }
    if (CODEC_Y0..CODEC_Y0 + 4 * CODEC_RH).contains(&y) {
        return BtHit::Codec(((y - CODEC_Y0) / CODEC_RH) as usize);
    }
    if codec_is_ldac && (QUAL_Y..QUAL_Y + QUAL_H).contains(&y) {
        let i = ((x - 22).max(0) / 109).min(3) as usize;
        return BtHit::Quality(i);
    }
    // The whole row is the target, not just the switch — it is a 34x18 graphic and this panel has
    // no other tappable thing on that band.
    if (ENH_Y..ENH_Y + ENH_H).contains(&y) {
        return BtHit::Enhanced;
    }
    if (PAIR_Y..PAIR_Y + 52).contains(&y) {
        return BtHit::Pair;
    }
    BtHit::None
}

// A small radio indicator: filled accent disc when selected, hollow ring otherwise.
fn radio(c: &mut Canvas, cx: i32, cy: i32, on: bool, t: &Theme) {
    if on {
        Circle::with_center(Point::new(cx, cy), 16)
            .into_styled(PrimitiveStyle::with_fill(t.acc))
            .draw(c)
            .ok();
        Circle::with_center(Point::new(cx, cy), 6)
            .into_styled(PrimitiveStyle::with_fill(t.acc_ink))
            .draw(c)
            .ok();
    } else {
        Circle::with_center(Point::new(cx, cy), 16)
            .into_styled(PrimitiveStyle::with_stroke(t.line, 1))
            .draw(c)
            .ok();
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, bt: &Bt) {
    c.fill(t.bg);
    let _y0 = crate::chrome::header(c, t, f, "Bluetooth", None);
    // header right: ON/OFF + toggle
    let onoff = if bt.on { "ON" } else { "OFF" };
    right(c, f, 416.0, 65.0, onoff, &sty(Family::Mono, Weight::Regular, 12.0, if bt.on { t.acc } else { t.faint }, 0.12));
    crate::widgets::toggle(c, t, 424, 56, 34, 18, 12, bt.on);

    // connected card (or empty state)
    if bt.on && bt.connected.is_some() {
        let name = bt.connected.unwrap();
        fill_rect(c, 22, CARD_Y, 436, CARD_H, t.panel);
        stroke_rect(c, 22, CARD_Y, 436, CARD_H, t.line, 1);
        text::draw(c, f, 40.0, (CARD_Y + 24) as f32, "CONNECTED", &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));
        right(c, f, 440.0, (CARD_Y + 24) as f32, "HP BATT 60%", &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.1));
        text::draw(c, f, 40.0, (CARD_Y + 52) as f32, name, &sty(Family::Sans, Weight::Bold, 24.0, t.ink, 0.0));
        let (dx, dy, dw, dh) = DISC;
        stroke_rect(c, dx, dy, dw, dh, t.line, 1);
        center(c, f, (dx + dw / 2) as f32, (dy + dh / 2 + 4) as f32, "Disconnect", &sty(Family::Sans, Weight::SemiBold, 14.0, t.dim, 0.0));
    } else {
        let mut dx = 22;
        while dx < 458 {
            fill_rect(c, dx, CARD_Y + 28, 5, 1, t.line);
            fill_rect(c, dx, CARD_Y + CARD_H - 8, 5, 1, t.line);
            dx += 11;
        }
        let msg = if !bt.on {
            "Bluetooth is off"
        } else if bt.link_known {
            "No device connected"
        } else {
            "Link state unavailable on this firmware"
        };
        center(c, f, 240.0, (CARD_Y + CARD_H / 2 + 4) as f32, msg, &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    }

    // TRANSMIT CODEC — list with the active one selected (greyed while BT is off)
    let body = if bt.on { t.ink } else { t.faint };
    let subc = if bt.on { t.dim } else { t.faint };
    text::draw(c, f, 22.0, 198.0, "TRANSMIT CODEC", &sty(Family::Mono, Weight::Regular, 11.0, if bt.on { t.acc } else { t.faint }, 0.18));
    for (i, (name, sub)) in CODECS.iter().enumerate() {
        let y = CODEC_Y0 + i as i32 * CODEC_RH;
        let cy = y + CODEC_RH / 2;
        let active = bt.on && bt.codec_sel as usize == i;
        radio(c, 38, cy, active, t);
        let ncol = if active { t.acc } else { body };
        text::draw(c, f, 64.0, (cy - 2) as f32, name, &sty(Family::Sans, Weight::SemiBold, 18.0, ncol, 0.0));
        text::draw(c, f, 64.0, (cy + 15) as f32, sub, &sty(Family::Mono, Weight::Regular, 11.0, subc, 0.04));
        crate::widgets::hline(c, y + CODEC_RH, t.line);
    }

    // LDAC QUALITY chips — only when LDAC is the active codec
    if bt.on && bt.codec_sel == LDAC {
        text::draw(c, f, 22.0, (QUAL_Y - 10) as f32, "LDAC SOUND QUALITY", &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));
        for (i, q) in QUALITIES.iter().enumerate() {
            let x = 22 + i as i32 * 109;
            let on = bt.ldac_quality as usize == i;
            if on {
                fill_rect(c, x, QUAL_Y, 103, QUAL_H, t.acc);
            } else {
                stroke_rect(c, x, QUAL_Y, 103, QUAL_H, t.line, 1);
            }
            let col = if on { t.acc_ink } else { t.dim };
            center(c, f, (x + 51) as f32, (QUAL_Y + QUAL_H / 2 + 4) as f32, q, &sty(Family::Sans, Weight::SemiBold, 15.0, col, 0.0));
        }
        // Centred text has no fixed edge to truncate against, so it overflows BOTH sides once the
        // UI scale grows it — at 140% this caption ran off the panel at each end and read as
        // "…s the bitrate to the link. Used everywhere, incl…". Fit it to the panel width first.
        let cst = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.04);
        let cap = crate::widgets::fit(f, "Auto adapts the bitrate to the link. Used everywhere, incl. USB-DAC.",
                                      &cst, (crate::canvas::W as f32) - 44.0);
        center(c, f, 240.0, (QUAL_Y + QUAL_H + 22) as f32, &cap, &cst);
    }

    // ENHANCED MODE — Sony's own name for the AVRCP absolute-volume switch. With it on, a volume
    // step sends the headphone the level to sit at (SetCurrentVolume); with it off, the player
    // sends VOLUME_UP/VOLUME_DOWN key events instead and sinks like the CMF Buds answer each one
    // with their own feedback beep. Sony gates SetCurrentVolume on this preference internally
    // ("Not control absolute volume mode"), so the shell must set it — reading
    // IsSupportedAbsoluteVolume alone is not enough.
    text::draw(c, f, 22.0, (ENH_Y - 12) as f32, "VOLUME CONTROL",
               &sty(Family::Mono, Weight::Regular, 11.0, if bt.on { t.acc } else { t.faint }, 0.18));
    crate::widgets::hline(c, ENH_Y, t.line);
    let enh_on = bt.on && bt.enhanced;
    let tcol = if bt.on { t.ink } else { t.faint };
    let tst = sty(Family::Sans, Weight::SemiBold, 17.0, tcol, 0.0);
    let sst = sty(Family::Mono, Weight::Regular, 11.0, if bt.on { t.dim } else { t.faint }, 0.04);
    // 22 → the switch's left edge (422) less a gap; both strings truncate rather than run under it.
    let avail = (422 - 22 - 14) as f32;
    text::draw(c, f, 22.0, (ENH_Y + 26) as f32,
               &crate::widgets::fit(f, "Use Enhanced Mode", &tst, avail), &tst);
    let sub = if !bt.enhanced_supported {
        "Not supported by the connected device"
    } else if bt.enhanced {
        "Sets the headphone's level directly \u{b7} no button beep"
    } else {
        "Sends volume key presses \u{b7} turn on if volume won't change"
    };
    text::draw(c, f, 22.0, (ENH_Y + 46) as f32, &crate::widgets::fit(f, sub, &sst, avail), &sst);
    crate::widgets::toggle(c, t, 422, ENH_Y + 20, 34, 18, 12, enh_on);
    crate::widgets::hline(c, ENH_Y + ENH_H, t.line);

    // pair new device + NFC hint
    fill_rect(c, 22, PAIR_Y, 436, 52, if bt.on { t.acc } else { t.line });
    let plabel_col = if bt.on { t.acc_ink } else { t.faint };
    icons::bt(c, 178.0, (PAIR_Y + 26) as f32, 17.0, plabel_col);
    text::draw(c, f, 196.0, (PAIR_Y + 31) as f32, "Pair new device", &sty(Family::Sans, Weight::Bold, 17.0, plabel_col, 0.0));
    // Footer: an NFC hint on the left and the Receiver-mode link on the right, on ONE baseline.
    // Both were drawn at fixed x, so at 140% "…TO REAR PANEL" ran straight through "RECEIVER
    // MODE ›". The link keeps its width (it names a destination); the hint gives way.
    icons::rx(c, 30.0, 776.0, 14.0, t.faint);
    crate::widgets::row_pair(
        c, f, 46.0, 458.0, 780.0,
        "NFC · TOUCH DEVICE TO REAR PANEL", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.08),
        "RECEIVER MODE \u{203a}", &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.08),
        14.0,
    );
}
