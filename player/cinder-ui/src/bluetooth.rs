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
/// The codec A2DP actually NEGOTIATED on the live link, as reported by
/// `BtTransmitterService::GetSoundStatus`. This is Sony's own `BtSoundCodec` enum, NOT the
/// Bluetooth assigned-numbers codec ID — 0x02 there would be MPEG-2/4 AAC, and it is not.
///
/// Measured on device 2026-08-17 with a WH-1000XM4: the player requested LDAC, the peer advertised
/// `ldac support:1` with both aptX flags clear, and the link reported 0x02 for the whole session.
/// The other enumerators are NOT known yet, so anything else is shown as its raw byte rather than
/// guessed at — a wrong codec label is worse than an honest hex value on a screen whose entire
/// purpose is telling you what you are actually listening to.
pub fn link_codec_name(raw: u8) -> Option<&'static str> {
    match raw {
        0x02 => Some("LDAC"),
        _ => None,
    }
}

/// Label for the negotiated codec: its name if we know the enumerator, else the raw byte.
pub fn link_codec_label(raw: u8) -> String {
    link_codec_name(raw).map_or_else(|| format!("CODEC 0x{raw:02X}"), |n| n.to_string())
}

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
    /// A connect attempt is in flight (name if known). This screen used to have NO in-flight state
    /// at all: tapping connect on the Devices screen came straight back here, which still read
    /// "No device connected" for however many seconds the link took — indistinguishable from the
    /// attempt having failed outright.
    pub connecting: bool,
    /// Spinner phase in seconds, advanced by nav while `connecting`.
    pub busy_phase: f32,
    /// The codec the LINK actually negotiated (raw `BtSoundCodec`), pushed by the shell from
    /// `GetSoundStatus`. `None` = nothing connected, or the service wrote nothing.
    ///
    /// Distinct from `codec_sel`, which is only what the user ASKED for. They disagree whenever a
    /// sink doesn't support the requested codec and A2DP falls back — which the radio does
    /// silently, and which is exactly the thing this screen existed to not tell you.
    pub link_codec: Option<u8>,
    /// The radio's paired devices, so this screen can CONNECT to one directly.
    ///
    /// They used to live only on the separate Devices screen, behind "Pair new device" — a button
    /// whose label says you are about to pair something new, which is the wrong door for "put my
    /// headphones back on". Reconnecting to a known device is the commonest thing anyone does here,
    /// so it is now the body of the screen.
    pub paired: &'a [crate::pairing::PairedDevice],
}

// ---- layout (shared by render + hit) ----
const CARD_Y: i32 = 92;
const CARD_H: i32 = 86;
const DISC: (i32, i32, i32, i32) = (348, 136, 104, 34); // x,y,w,h

// ---- main screen: the paired list is the content ----
const PAIRED_Y0: i32 = 212;
const PAIRED_RH: i32 = 62;
/// Rows shown before the list is cut off. Five fills the space between the card and the footer
/// without pushing either off; a longer pairing history is managed on the Devices screen, which is
/// where forgetting lives anyway.
pub const PAIRED_SHOWN: usize = 5;
const ADV_Y: i32 = 548; // "Audio quality ›" — the codec page
const ADV_H: i32 = 60;
const PAIR_Y: i32 = 640;

// ---- codec page (its own screen now) ----
const CODEC_Y0: i32 = 150;
const CODEC_RH: i32 = 56;
const QUAL_Y: i32 = 420;
const QUAL_H: i32 = 40;
const ENH_Y: i32 = 556; // "Use Enhanced Mode" row (absolute volume)
const ENH_H: i32 = 64;

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
    /// A paired device row — connect to it, or hang up if it is the connected one. Which of the
    /// two is the caller's call, from its own `connected` flag, so geometry stays geometry.
    PairedRow(usize),
    /// Open the codec / volume-control page.
    Advanced,
}

/// Geometry accessors, so tests and any future caller ask the layout where a control is rather
/// than repeating a pixel that silently rots when the screen moves.
pub fn advanced_row() -> (i32, i32, i32, i32) {
    (0, ADV_Y, crate::canvas::W as i32, ADV_H)
}
/// Vertical centre of codec row `i` on the codec page.
pub fn codec_row_y(i: usize) -> i32 {
    CODEC_Y0 + i as i32 * CODEC_RH + CODEC_RH / 2
}
/// Vertical centre of the LDAC quality chip strip.
pub fn quality_row_y() -> i32 {
    QUAL_Y + QUAL_H / 2
}
/// Vertical centre of the Enhanced Mode row.
pub fn enhanced_row_y() -> i32 {
    ENH_Y + ENH_H / 2
}

/// Map a tap on the BLUETOOTH screen. The codec controls moved to their own page, so this now
/// answers only: the radio switch, the connected card, a paired device, and the two footer rows.
pub fn hit(x: i32, y: i32, on: bool, paired: usize) -> BtHit {
    // Header ON/OFF toggle. Three sizes so far: a 72x34 strip hugging the switch graphic (too
    // small), then the full header band from `STATUS_H` to `HEADER_BOTTOM` at x>=356, and now this.
    //
    // The second version was still wrong, and in a way that made a MISS worse than a miss. It began
    // at exactly `STATUS_H`, sharing an edge with the status strip — and `chrome::status_hit`
    // claims every y below that line with "anywhere else along the strip → the Menu". So a tap two
    // pixels high did not fail to toggle Bluetooth, it navigated away to the Menu. Reported
    // 2026-08-19 as hitting the top bar by accident.
    //
    // So: a deliberate DEAD BAND under the status strip, and the rest of the growth downward into
    // the empty space beside the connected card, which has no other target above `DISC`. A tap in
    // the gap now does nothing at all, which is the right outcome for a near miss — losing your
    // place is worse than having to tap again.
    // The dead band now lives in `chrome::STATUS_DEAD_H`, where every header control benefits from
    // it, so this starts at STATUS_H again and simply grows DOWNWARD into the empty space beside
    // the connected card — which has no other target above `DISC`.
    const TOGGLE_BOTTOM: i32 = 128;
    const TOGGLE_LEFT: i32 = 336;
    if (crate::chrome::STATUS_H..TOGGLE_BOTTOM).contains(&y) && x >= TOGGLE_LEFT {
        return BtHit::Toggle;
    }
    if !on {
        return BtHit::None; // everything else is inert while BT is off
    }
    let (dx, dy, dw, dh) = DISC;
    if (dy..dy + dh).contains(&y) && (dx..dx + dw).contains(&x) {
        return BtHit::Disconnect;
    }
    let rows = paired.min(PAIRED_SHOWN) as i32;
    if (PAIRED_Y0..PAIRED_Y0 + rows * PAIRED_RH).contains(&y) {
        return BtHit::PairedRow(((y - PAIRED_Y0) / PAIRED_RH) as usize);
    }
    if (ADV_Y..ADV_Y + ADV_H).contains(&y) {
        return BtHit::Advanced;
    }
    if (PAIR_Y..PAIR_Y + 52).contains(&y) {
        return BtHit::Pair;
    }
    BtHit::None
}

/// Map a tap on the CODEC page. `codec_is_ldac` gates the quality chips (only shown for LDAC).
pub fn hit_codec(x: i32, y: i32, on: bool, codec_is_ldac: bool) -> BtHit {
    if !on {
        return BtHit::None;
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
    // Nudged down from y=56/65 so the graphic sits further from the status strip the user kept
    // catching, and nearer the middle of its (much larger) touch target. Still inside the header
    // band, so it stays level enough with the title to read as part of the header.
    right(c, f, 416.0, 71.0, onoff, &sty(Family::Mono, Weight::Regular, 12.0, if bt.on { t.acc } else { t.faint }, 0.12));
    crate::widgets::toggle(c, t, 424, 62, 34, 18, 12, bt.on);

    // connected card (or empty state)
    if bt.on && bt.connected.is_some() {
        let name = bt.connected.unwrap();
        fill_rect(c, 22, CARD_Y, 436, CARD_H, t.panel);
        stroke_rect(c, 22, CARD_Y, 436, CARD_H, t.line, 1);
        // The tag carries the NEGOTIATED codec when the link reports one. This is the answer to
        // "am I actually getting LDAC" — the radio falls back silently, so the codec radio list
        // below (which is only a request) can say LDAC while the link is running something else.
        let tag = match bt.link_codec.map(link_codec_label) {
            Some(codec) => format!("CONNECTED · {}", codec.to_uppercase()),
            None => "CONNECTED".to_string(),
        };
        text::draw(c, f, 40.0, (CARD_Y + 24) as f32, &tag, &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));
        // There used to be a "HP BATT 60%" readout here. It was hardcoded, and it cannot be made
        // real on this firmware: the entire BT stack exposes exactly one battery API — AVRCP's
        // coarse 5-state BtBatteryStatus, via BtTransmitterService::ChangeBatteryStatus and
        // BtMwAvrcpSrcRequestCurrentBatteryStatus — and it runs the OTHER WAY, the Walkman
        // announcing its own level to the sink. There is no BLE Battery Service (0x180F) client, no
        // iPhoneAccEv, and no percentage string anywhere in libBtMw / libBtCompIf /
        // libBtTransmitterService / either BLE service. HFP exists only in Hands-Free-unit role
        // (receiver mode) with nothing battery-shaped attached.
        //
        // So the number was not a placeholder waiting to be wired — it was unwireable. A confident
        // "60%" on a stranger's headphones is worse than no reading at all, so the slot is empty.
        text::draw(c, f, 40.0, (CARD_Y + 52) as f32, name, &sty(Family::Sans, Weight::Bold, 24.0, t.ink, 0.0));
        let (dx, dy, dw, dh) = DISC;
        stroke_rect(c, dx, dy, dw, dh, t.line, 1);
        center(c, f, (dx + dw / 2) as f32, (dy + dh / 2 + 4) as f32, "Disconnect", &sty(Family::Sans, Weight::SemiBold, 14.0, t.dim, 0.0));
    } else if bt.on && bt.connecting {
        // In flight. A solid card rather than the dashed empty state, because something IS
        // happening — and a moving spinner, because a connect can take several seconds and the
        // difference between "trying" and "failed" has to be visible without waiting it out.
        fill_rect(c, 22, CARD_Y, 436, CARD_H, t.panel);
        stroke_rect(c, 22, CARD_Y, 436, CARD_H, t.line, 1);
        text::draw(c, f, 64.0, (CARD_Y + 24) as f32, "CONNECTING",
                   &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));
        text::draw(c, f, 64.0, (CARD_Y + 52) as f32, "Linking to device…",
                   &sty(Family::Sans, Weight::Regular, 18.0, t.dim, 0.0));
        crate::widgets::spinner(c, 42, CARD_Y + CARD_H / 2, 9, 3, bt.busy_phase, t.acc);
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

    // PAIRED DEVICES — the body of the screen, and tappable.
    //
    // Reconnecting to headphones you already own is the commonest reason anyone opens this screen,
    // and until now it was the one thing you could not do from it: the list lived behind a button
    // labelled "Pair new device". The codec radio list that used to occupy this space is a
    // set-once preference and has moved to its own page.
    let cap = if bt.on { t.acc } else { t.faint };
    // SAY SO WHEN THIS IS ONLY PART OF THE LIST. This is a summary — the complete surface, with
    // FORGET and a page turn, is Devices — but a list that quietly stops at five looks like the
    // whole truth, and for a while it WAS the whole truth in the sense that nothing else could
    // reach past it either (see pairing::MAX_PAIRED). Naming the count costs one line and makes
    // "Pair new device", which is the route to the rest, an obvious next step rather than a
    // guess.
    let head = if bt.on && bt.paired.len() > PAIRED_SHOWN {
        format!("PAIRED DEVICES · {} OF {}", PAIRED_SHOWN, bt.paired.len())
    } else {
        "PAIRED DEVICES".to_string()
    };
    text::draw(c, f, 22.0, 198.0, &head, &sty(Family::Mono, Weight::Regular, 11.0, cap, 0.18));
    if !bt.on {
        // Nothing here is actionable with the radio off, and greyed rows invite taps that do
        // nothing. Say why the list is empty instead of showing a dead one.
        center(c, f, 240.0, (PAIRED_Y0 + 40) as f32, "Turn Bluetooth on to see your devices",
               &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    } else if bt.paired.is_empty() {
        center(c, f, 240.0, (PAIRED_Y0 + 40) as f32, "No paired devices yet",
               &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    } else {
        for (i, d) in bt.paired.iter().take(PAIRED_SHOWN).enumerate() {
            let y = PAIRED_Y0 + i as i32 * PAIRED_RH;
            let cy = y + PAIRED_RH / 2;
            if d.connected {
                fill_rect(c, 0, y, crate::canvas::W as i32, PAIRED_RH, t.row_sel);
            }
            let icol = if d.connected { t.acc } else { t.dim };
            icons::bt(c, 38.0, cy as f32, 16.0, icol);
            let nst = sty(Family::Sans, Weight::SemiBold, 18.0,
                          if d.connected { t.acc } else { t.ink }, 0.0);
            // Leave room for the right-hand status word rather than running under it.
            crate::widgets::draw_fit(c, f, 64.0, (cy - 2) as f32, &d.name, &nst, 360.0);
            let sub = if d.connected { "CONNECTED" } else { d.kind.as_str() };
            let scol = if d.connected { t.acc } else { t.faint };
            crate::widgets::draw_fit(c, f, 64.0, (cy + 16) as f32, sub,
                                     &sty(Family::Mono, Weight::Regular, 11.0, scol, 0.06), 360.0);
            crate::widgets::right(c, f, 458.0, (cy + 4) as f32,
                                  if d.connected { "\u{2022}" } else { "CONNECT" },
                                  &sty(Family::Mono, Weight::Regular, 11.0,
                                       if d.connected { t.acc } else { t.dim }, 0.1));
            crate::widgets::hline(c, y + PAIRED_RH, t.line);
        }
    }

    // AUDIO QUALITY — the codec page, one row rather than a quarter of the screen.
    //
    // Codec and Enhanced Mode are set once and then never touched, so they were paying for prime
    // screen space with the thing people actually came for. The row still SHOWS the current codec,
    // because that is the part worth glancing at.
    crate::widgets::hline(c, ADV_Y, t.line);
    let acol = if bt.on { t.ink } else { t.faint };
    text::draw(c, f, 22.0, (ADV_Y + 26) as f32, "Audio quality",
               &sty(Family::Sans, Weight::SemiBold, 17.0, acol, 0.0));
    let live = bt.link_codec.map(link_codec_label);
    let want = CODECS[(bt.codec_sel as usize).min(CODECS.len() - 1)].0;
    let detail = match live {
        // What the link NEGOTIATED, when that differs from the request — the radio falls back
        // silently and this row is now the only place that discrepancy is visible from.
        Some(l) if !l.eq_ignore_ascii_case(want) => format!("{want} requested \u{b7} {l} in use"),
        Some(l) => l,
        None => want.to_string(),
    };
    text::draw(c, f, 22.0, (ADV_Y + 46) as f32, &detail,
               &sty(Family::Mono, Weight::Regular, 11.0, if bt.on { t.dim } else { t.faint }, 0.04));
    crate::widgets::right(c, f, 458.0, (ADV_Y + 36) as f32, "\u{203a}",
                          &sty(Family::Sans, Weight::Regular, 22.0, t.dim, 0.0));
    crate::widgets::hline(c, ADV_Y + ADV_H, t.line);

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

/// The codec / volume-control page, reached from "Audio quality" on the Bluetooth screen. These
/// controls were the top half of that screen; they are configured once and then ignored, so they
/// were the wrong thing to give the most reachable space to.
pub fn render_codec(c: &mut Canvas, t: &Theme, f: &FontSet, bt: &Bt) {
    c.fill(t.bg);
    let _y0 = crate::chrome::header(c, t, f, "Audio quality", None);
    // TRANSMIT CODEC — list with the active one selected (greyed while BT is off)
    let body = if bt.on { t.ink } else { t.faint };
    let subc = if bt.on { t.dim } else { t.faint };
    text::draw(c, f, 22.0, (CODEC_Y0 - 20) as f32, "TRANSMIT CODEC", &sty(Family::Mono, Weight::Regular, 11.0, if bt.on { t.acc } else { t.faint }, 0.18));
    for (i, (name, sub)) in CODECS.iter().enumerate() {
        let y = CODEC_Y0 + i as i32 * CODEC_RH;
        let cy = y + CODEC_RH / 2;
        let active = bt.on && bt.codec_sel as usize == i;
        radio(c, 38, cy, active, t);
        let ncol = if active { t.acc } else { body };
        text::draw(c, f, 64.0, (cy - 2) as f32, name, &sty(Family::Sans, Weight::SemiBold, 18.0, ncol, 0.0));
        text::draw(c, f, 64.0, (cy + 15) as f32, sub, &sty(Family::Mono, Weight::Regular, 11.0, subc, 0.04));
        // "LIVE" marks the codec the link actually negotiated, which is not necessarily the one
        // selected: A2DP picks during connection setup and falls back without telling anyone. A
        // selected row with no LIVE tag while something is connected means the request lost.
        if bt.link_codec.and_then(link_codec_name) == Some(*name) {
            crate::widgets::right(c, f, 458.0, (cy + 4) as f32, "LIVE",
                                  &sty(Family::Mono, Weight::Bold, 11.0, t.acc, 0.18));
        }
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

}
