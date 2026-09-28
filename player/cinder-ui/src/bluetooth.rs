//! Bluetooth — on/off, the connected device, and the **transmit codec** selector (the device-wide
//! preference used for both normal BT playback and the USB-DAC→LDAC bridge). Codecs this hardware
//! can transmit: LDAC · aptX HD · aptX · SBC (AAC is receive-only, excluded). When LDAC is the
//! choice, a sound-quality sub-row (Auto/990/660/330) appears. Geometry lives in `hit()` so the
//! navigator and the renderer can't drift.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, stroke_rect, sty};
use crate::Canvas;

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
    /// Bluetooth fine volume: how far one AVRCP step is subdivided, as a label ("OFF", "±2 dB").
    ///
    /// AVRCP's step on this firmware is 4 units of 127 — about 2 dB — and there is no finer command
    /// to send: absolute volume is inert here (measured on a WH-1000XM4 and a set of CMF buds, both
    /// of which report every relative step and ignore every absolute write). So the finer steps are
    /// made at the SOURCE, by attenuating through the 10-band EQ in half-dB units, and this row is
    /// how much of a step that trim is allowed to cover.
    pub fine_volume: &'a str,
    /// The radio's paired devices, so this screen can CONNECT to one directly.
    ///
    /// They used to live only on the separate Devices screen, behind "Pair new device" — a button
    /// whose label says you are about to pair something new, which is the wrong door for "put my
    /// headphones back on". Reconnecting to a known device is the commonest thing anyone does here,
    /// so it is now the body of the screen.
    pub paired: &'a [crate::pairing::PairedDevice],
    /// The HCI capture is running (THIS DEVICE ▸ Debug log). Never persisted: every boot starts
    /// with it off, because it is a growing file in RAM.
    pub debug_log: bool,
}

// ---- layout (shared by render + hit) ----
const CARD_Y: i32 = 92;
const CARD_H: i32 = 86;
const DISC: (i32, i32, i32, i32) = (348, 136, 104, 34); // x,y,w,h

// ---- main screen: the paired list is the content ----
const PAIRED_Y0: i32 = 212;
const PAIRED_RH: i32 = crate::scale::TRACK_ROW_H;
/// Rows shown before the list is cut off. Five fills the space between the card and the footer
/// without pushing either off; a longer pairing history is managed on the Devices screen, which is
/// where forgetting lives anyway.
pub const PAIRED_SHOWN: usize = 5;
/// THIS DEVICE (handoff 2g): its section label, then "Sound quality ›" — the codec page.
const THIS_DEVICE_Y: i32 = PAIRED_Y0 + PAIRED_SHOWN as i32 * PAIRED_RH + 6;
const ADV_Y: i32 = THIS_DEVICE_Y + crate::kit::SECTION_H;
const ADV_H: i32 = crate::kit::ROW_H;
/// THIS DEVICE ▸ Debug log (`docs/PLAN_community_2026-09-23.md` B6): a switch that records the
/// radio's HCI traffic, so a "won't connect" report can arrive with the evidence.
const DEBUG_Y: i32 = ADV_Y + ADV_H;
const DEBUG_H: i32 = crate::kit::ROW_H;
/// The PAIRED DEVICES label, whose right half is "PAIR NEW" (handoff 2g: a list's own action lives
/// in its section label, as CLEAR does on Up Next). It sits straight under the connected card.
const PAIRED_LABEL_Y: i32 = CARD_Y + CARD_H;

// ---- codec page: Bluetooth ▸ Sound quality (handoff 5l) ----

/// The LDAC rows in the order the handoff draws them — best sound first, Auto last — as indices
/// into [`QUALITIES`] (the persisted order, which Auto leads and which cannot move).
const LDAC_ORDER: [usize; 4] = [1, 2, 3, 0];
/// Sony's own names for the four LDAC modes, by [`QUALITIES`] index.
const LDAC_NAMES: [&str; 4] = ["Best effort", "Sound quality priority", "Standard", "Connection priority"];

/// One band of the Sound quality page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Strip,
    Label(&'static str),
    /// The transmit codec chips.
    Codecs,
    /// LDAC row `k` in display order (see [`LDAC_ORDER`]).
    Ldac(usize),
    Enhanced,
    /// "Fine volume" — the source-side vernier, directly under Enhanced Mode because it is the row
    /// you reach for when Enhanced Mode has failed to give you the volume resolution you wanted.
    /// (It usually has: absolute volume is inert on this firmware, measured on two sinks.)
    Fine,
}

/// The page, top to bottom, as `(part, top, height)` in screen pixels. `render_codec` draws it and
/// `hit_codec` reads it, so a band cannot be drawn in one place and answer taps in another. The
/// LDAC section exists only while LDAC is the chosen codec.
fn codec_parts(ldac: bool) -> Vec<(Part, i32, i32)> {
    use crate::kit::{CHIP_H, ROW_H, SECTION_H, STRIP_H};
    let mut out = Vec::new();
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut push = |p: Part, h: i32| {
        out.push((p, y, h));
        y += h;
    };
    push(Part::Strip, STRIP_H);
    push(Part::Label("TRANSMIT CODEC"), SECTION_H);
    push(Part::Codecs, CHIP_H + 8);
    if ldac {
        push(Part::Label("LDAC"), SECTION_H);
        for k in 0..LDAC_ORDER.len() {
            push(Part::Ldac(k), ROW_H);
        }
    }
    push(Part::Label("VOLUME CONTROL"), SECTION_H);
    push(Part::Enhanced, ROW_H);
    push(Part::Fine, ROW_H);
    out
}

fn codec_part_centre(ldac: bool, want: Part) -> i32 {
    codec_parts(ldac).into_iter().find(|&(p, _, _)| p == want).map_or(0, |(_, top, h)| top + h / 2)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BtHit {
    None,
    Toggle,
    Disconnect,
    Codec(usize),
    Quality(usize),
    /// "Use Enhanced Mode" toggled — the absolute-volume switch.
    Enhanced,
    /// "Fine volume" cycled — how far one AVRCP step is subdivided by the source-side trim.
    FineVolume,
    Pair,
    /// A paired device row — connect to it, or hang up if it is the connected one. Which of the
    /// two is the caller's call, from its own `connected` flag, so geometry stays geometry.
    PairedRow(usize),
    /// Open the codec / volume-control page.
    Advanced,
    /// The Debug log switch.
    DebugLog,
}

/// Geometry accessors, so tests and any future caller ask the layout where a control is rather
/// than repeating a pixel that silently rots when the screen moves.
pub fn advanced_row() -> (i32, i32, i32, i32) {
    (0, ADV_Y, crate::canvas::W as i32, ADV_H)
}
/// Vertical centre of the Debug log row.
pub fn debug_row_y() -> i32 {
    DEBUG_Y + DEBUG_H / 2
}
/// Centre of codec chip `i` on the Sound quality page.
pub fn codec_chip(i: usize) -> (i32, i32) {
    let (x, w) = crate::kit::chip_span(i, CODECS.len());
    (x + w / 2, codec_parts(false)[2].1 + crate::kit::CHIP_H / 2)
}
/// Vertical centre of the LDAC row for quality `q` (an index into [`QUALITIES`]). Only drawn, and
/// only tappable, while LDAC is the chosen codec.
pub fn quality_row_y(q: usize) -> i32 {
    let k = LDAC_ORDER.iter().position(|&i| i == q).unwrap_or(0);
    codec_part_centre(true, Part::Ldac(k))
}
/// Vertical centre of the Enhanced Mode row. It moves up when the LDAC section is hidden.
pub fn enhanced_row_y(ldac: bool) -> i32 {
    codec_part_centre(ldac, Part::Enhanced)
}
/// Vertical centre of the Fine volume row.
pub fn fine_row_y(ldac: bool) -> i32 {
    codec_part_centre(ldac, Part::Fine)
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
    if (DEBUG_Y..DEBUG_Y + DEBUG_H).contains(&y) {
        return BtHit::DebugLog;
    }
    if crate::kit::section_action_hit(PAIRED_LABEL_Y, x, y) {
        return BtHit::Pair;
    }
    BtHit::None
}

/// Map a tap on the Sound quality page. `codec_is_ldac` gates the LDAC rows (only shown for LDAC).
/// The whole of a row is its target, the switch's row included: nothing else shares the band.
pub fn hit_codec(x: i32, y: i32, on: bool, codec_is_ldac: bool) -> BtHit {
    if !on {
        return BtHit::None;
    }
    for (p, top, h) in codec_parts(codec_is_ldac) {
        if !(top..top + h).contains(&y) {
            continue;
        }
        return match p {
            Part::Codecs => crate::kit::chip_at(CODECS.len(), top, x, y).map_or(BtHit::None, BtHit::Codec),
            Part::Ldac(k) => BtHit::Quality(LDAC_ORDER[k]),
            Part::Enhanced => BtHit::Enhanced,
            Part::Fine => BtHit::FineVolume,
            Part::Strip | Part::Label(_) => BtHit::None,
        };
    }
    BtHit::None
}

/// The Sound quality strip: what the link is actually doing. A2DP picks the codec while
/// connecting and falls back without telling anyone, so when the live codec is not the one asked
/// for, the strip says both — the one thing the codec chips below, which are only a request,
/// cannot say.
fn codec_strip(bt: &Bt) -> String {
    let want = CODECS[(bt.codec_sel as usize).min(CODECS.len() - 1)].0;
    if !bt.on {
        return "BLUETOOTH IS OFF \u{b7} TURN IT ON TO CHANGE THESE".into();
    }
    let Some(name) = bt.connected else {
        return "NOTHING CONNECTED \u{b7} THESE APPLY TO THE NEXT LINK".into();
    };
    let name = name.to_uppercase();
    match bt.link_codec {
        Some(raw) => {
            let live = link_codec_label(raw);
            // Only LDAC's enumerator is known (see `link_codec_name`), so a raw byte proves a
            // fallback only from LDAC, or when it names a codec that is not the one asked for.
            let fell_back = match link_codec_name(raw) {
                Some(n) => n != want,
                None => want == CODECS[LDAC as usize].0,
            };
            if fell_back {
                format!("{name} \u{b7} {live} \u{b7} ASKED FOR {}", want.to_uppercase())
            } else {
                format!("{name} \u{b7} {live}")
            }
        }
        None => format!("{name} \u{b7} CONNECTED"),
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, bt: &Bt) {
    c.fill(t.bg);
    let _y0 = crate::chrome::header(c, t, f, "Bluetooth", None);
    // Header right slot: the kit's switch (handoff: a header's right slot is a caption, a switch,
    // an icon or nothing). Sits low in the header band, away from the status strip the user kept
    // catching, and near the middle of its much larger touch target (see `hit`).
    crate::kit::switch(c, t, crate::kit::RIGHT - crate::kit::SWITCH_W, 58, bt.on);

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
        // Fitted to end 12 px short of Disconnect. A name is whatever the headphones advertise,
        // and a long one ran straight through the button and off the card — 898 px of it clipped
        // off the panel for a 42-character name (`tests/ui_overflow.rs`, hostile Bluetooth state).
        let (dx, dy, dw, dh) = DISC;
        let nst = sty(Family::Sans, Weight::Bold, 24.0, t.ink, 0.0);
        text::draw(c, f, 40.0, (CARD_Y + 52) as f32, &crate::widgets::fit(f, name, &nst, (dx - 12 - 40) as f32), &nst);
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
    crate::kit::section_label(c, t, f, PAIRED_LABEL_Y, &head, bt.on.then_some("PAIR NEW"));
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
            let nst = sty(Family::Sans, Weight::SemiBold, crate::scale::ROW,
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

    // THIS DEVICE — how this player sends to headphones: the codec page, one row rather than a
    // quarter of the screen. Codec and Enhanced Mode are set once and then never touched, so they
    // were paying for prime screen space with the thing people actually came for. The row still
    // SHOWS what is in use, because that is the part worth glancing at. (Handoff 2g also draws a
    // "Sound profile" row here — which A/B setup this output switches to — and that waits on
    // per-output profiles, docs/PLAN_redesign_2026-09.md.)
    crate::kit::section_label(c, t, f, THIS_DEVICE_Y, "THIS DEVICE", None);
    let live = bt.link_codec.map(link_codec_label);
    let want = CODECS[(bt.codec_sel as usize).min(CODECS.len() - 1)].0;
    let detail = match live {
        // What the link NEGOTIATED, when that differs from the request — the radio falls back
        // silently and this row is now the only place that discrepancy is visible from.
        Some(l) if !l.eq_ignore_ascii_case(want) => format!("{want} requested \u{b7} {l} in use"),
        Some(l) => l,
        None => want.to_string(),
    };
    // The value is the LDAC bitrate when LDAC is the request (the handoff's "990"), otherwise the
    // codec itself.
    let value = if bt.codec_sel == LDAC {
        QUALITIES[(bt.ldac_quality as usize).min(QUALITIES.len() - 1)].to_uppercase()
    } else {
        want.to_uppercase()
    };
    crate::kit::row(c, t, f, ADV_Y, ADV_H,
        &crate::kit::Row::new("Sound quality").sub(&detail).trail(crate::kit::Trail::Open(&value)));
    let dsub = if bt.debug_log { "Recording · switch off to save it to the drive" } else { "Record the radio's traffic for a bug report" };
    crate::kit::row(c, t, f, DEBUG_Y, DEBUG_H,
        &crate::kit::Row::new("Debug log").sub(dsub).trail(crate::kit::Trail::Switch(bt.debug_log)));

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

/// Bluetooth ▸ Sound quality (handoff 5l), reached from THIS DEVICE on the Bluetooth screen. These
/// controls were the top half of that screen; they are configured once and then ignored, so they
/// were the wrong thing to give the most reachable space to.
///
/// Drawn on the kit from [`codec_parts`]: a strip with what the link is doing, the codec as chips,
/// LDAC's four modes as rows under Sony's own names, and VOLUME CONTROL. While the radio is off the
/// page is inert, the rows are drawn in `faint` and nothing is marked chosen.
pub fn render_codec(c: &mut Canvas, t: &Theme, f: &FontSet, bt: &Bt) {
    use crate::kit::{self, Row, Trail};
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Sound quality", None);
    let ldac = bt.codec_sel == LDAC;
    let mut bottom = 0;
    for (p, top, h) in codec_parts(ldac) {
        bottom = top + h;
        match p {
            Part::Strip => {
                kit::strip(c, t, f, top, &codec_strip(bt));
            }
            Part::Label(l) => {
                kit::section_label(c, t, f, top, l, None);
            }
            Part::Codecs => {
                let names: Vec<&str> = CODECS.iter().map(|(n, _)| *n).collect();
                let sel = bt.on.then_some((bt.codec_sel as usize).min(CODECS.len() - 1));
                kit::chips(c, &shown(t, bt.on), f, top, &names, sel);
            }
            Part::Ldac(k) => {
                let q = LDAC_ORDER[k];
                let chosen = bt.on && bt.ldac_quality as usize == q;
                let sub = if q == 0 { "Adapts the rate to the link" } else { "" };
                let value = QUALITIES[q].to_uppercase();
                let r = Row::new(LDAC_NAMES[q]).sub(sub).trail(Trail::Value(&value)).sel(chosen);
                row_or_off(c, t, f, top, h, &r, bt.on);
            }
            Part::Enhanced => {
                // Sony's own name for the AVRCP absolute-volume switch. With it on, a volume step
                // sends the headphone the level to sit at (SetCurrentVolume); with it off, the
                // player sends VOLUME_UP/VOLUME_DOWN key events instead and sinks like the CMF Buds
                // answer each one with their own feedback beep. Sony gates SetCurrentVolume on this
                // preference internally ("Not control absolute volume mode"), so the shell must set
                // it — reading IsSupportedAbsoluteVolume alone is not enough.
                let sub = if !bt.enhanced_supported {
                    "Not supported by the connected device"
                } else if bt.enhanced {
                    "Sets the headphone's level \u{b7} no button beep"
                } else {
                    "Sends key presses \u{b7} on if volume won't change"
                };
                let r = Row::new("Use Enhanced Mode").sub(sub).trail(Trail::Switch(bt.on && bt.enhanced));
                row_or_off(c, t, f, top, h, &r, bt.on);
            }
            Part::Fine => {
                // A cycling value rather than a switch, because "how much finer" is the actual
                // question — and the honest answer depends on the sink, whose step size can be
                // measured in AVRCP units but not in dB. The trim rides on the EQ, so a curve
                // already at the service's floor leaves nothing to trim with.
                let sub = if bt.fine_volume == "OFF" {
                    "One Bluetooth step per press (~2 dB)"
                } else {
                    "Splits each Bluetooth step, via the EQ"
                };
                let value = bt.fine_volume.to_uppercase();
                let r = Row::new("Fine volume").sub(sub).trail(Trail::Value(&value));
                row_or_off(c, t, f, top, h, &r, bt.on);
            }
        }
    }
    // What the chips and rows apply to, once, under them: the codec and the LDAC rate are the
    // device's, not the headphone's, and the USB-DAC → LDAC bridge sends with them too.
    let st = sty(Family::Mono, Weight::Regular, crate::scale::CAPTION, t.faint, 0.04);
    let note = crate::widgets::fit(f, "Used everywhere, USB-DAC to LDAC included.", &st,
                                   (kit::RIGHT - kit::LEFT) as f32);
    text::draw(c, f, kit::LEFT as f32, (bottom + 28) as f32, &note, &st);
}

/// The theme a Sound quality control is drawn in: as it is, or — while the radio is off — with its
/// text in `faint`, so an inert page does not look like one that will answer.
fn shown(t: &Theme, on: bool) -> Theme {
    let mut s = *t;
    if !on {
        s.ink = t.faint;
        s.dim = t.faint;
    }
    s
}

fn row_or_off(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32, r: &crate::kit::Row, on: bool) {
    crate::kit::row(c, &shown(t, on), f, y, h, &r.sel(r.sel && on));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bt(on: bool, connected: Option<&str>, codec_sel: u8, link_codec: Option<u8>) -> Bt<'_> {
        Bt {
            on,
            connected,
            link_known: true,
            codec_sel,
            ldac_quality: 0,
            enhanced: true,
            enhanced_supported: true,
            connecting: false,
            busy_phase: 0.0,
            link_codec,
            fine_volume: "OFF",
            paired: &[],
            debug_log: false,
        }
    }

    /// The hit test reads the layout the render draws: every band answers at its centre, with the
    /// LDAC section and without it, and nothing answers while the radio is off.
    #[test]
    fn every_sound_quality_band_answers_where_it_is_drawn() {
        for ldac in [true, false] {
            for (p, top, h) in codec_parts(ldac) {
                let got = hit_codec(240, top + h / 2, true, ldac);
                let want = match p {
                    Part::Ldac(k) => BtHit::Quality(LDAC_ORDER[k]),
                    Part::Enhanced => BtHit::Enhanced,
                    Part::Fine => BtHit::FineVolume,
                    Part::Codecs => BtHit::Codec(2), // x 240: half the gap left of the third chip is its
                    Part::Strip | Part::Label(_) => BtHit::None,
                };
                assert_eq!(got, want, "{p:?} (ldac {ldac})");
                assert_eq!(hit_codec(240, top + h / 2, false, ldac), BtHit::None, "{p:?} answered with the radio off");
            }
            let (_, top, h) = *codec_parts(ldac).last().unwrap();
            assert!(top + h < crate::canvas::H as i32 - 40, "the page runs off the glass (ldac {ldac})");
        }
        for i in 0..CODECS.len() {
            let (x, y) = codec_chip(i);
            assert_eq!(hit_codec(x, y, true, true), BtHit::Codec(i));
        }
        for q in 0..QUALITIES.len() {
            assert_eq!(hit_codec(240, quality_row_y(q), true, true), BtHit::Quality(q));
        }
        assert_eq!(hit_codec(240, enhanced_row_y(false), true, false), BtHit::Enhanced);
        assert_eq!(hit_codec(240, fine_row_y(true), true, true), BtHit::FineVolume);
    }

    /// The strip names the live codec, and says so when it is not the one asked for — the silent
    /// A2DP fallback this page exists to expose.
    #[test]
    fn the_strip_says_when_the_link_fell_back() {
        const LDAC_RAW: u8 = 0x02;
        const OTHER_RAW: u8 = 0x05;
        assert_eq!(codec_strip(&bt(true, Some("WH-1000XM5"), LDAC, Some(LDAC_RAW))), "WH-1000XM5 · LDAC");
        assert_eq!(
            codec_strip(&bt(true, Some("WH-1000XM5"), LDAC, Some(OTHER_RAW))),
            "WH-1000XM5 · CODEC 0x05 · ASKED FOR LDAC"
        );
        // An unknown byte while SBC was asked for may well BE SBC: no claim either way.
        assert_eq!(codec_strip(&bt(true, Some("Buds"), 3, Some(OTHER_RAW))), "BUDS · CODEC 0x05");
        assert_eq!(codec_strip(&bt(true, Some("Buds"), 3, None)), "BUDS · CONNECTED");
        assert!(codec_strip(&bt(true, None, LDAC, None)).starts_with("NOTHING CONNECTED"));
        assert!(codec_strip(&bt(false, Some("Buds"), LDAC, Some(LDAC_RAW))).starts_with("BLUETOOTH IS OFF"));
    }
}
