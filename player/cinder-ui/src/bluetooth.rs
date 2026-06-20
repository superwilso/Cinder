//! Bluetooth — ported from cinder-proto-screens4.jsx `CBtScreen`. Header on/off
//! toggle, a connected card (or empty state), the paired-device list, and the
//! "Pair new device" action with NFC hint + receiver-mode link.

use crate::data::PAIRED;
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, right, stroke_rect, sty, toggle};
use crate::Canvas;

pub struct Bt {
    pub on: bool,
    pub connected: Option<&'static str>,
    pub codec: &'static str,
}

fn dashed_box(c: &mut Canvas, t: &Theme, x: i32, y: i32, w: i32, h: i32) {
    // top + bottom dashes
    let mut dx = x;
    while dx < x + w {
        fill_rect(c, dx, y, 5, 1, t.line);
        fill_rect(c, dx, y + h, 5, 1, t.line);
        dx += 11;
    }
    let mut dy = y;
    while dy < y + h {
        fill_rect(c, x, dy, 1, 5, t.line);
        fill_rect(c, x + w, dy, 1, 5, t.line);
        dy += 11;
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, bt: &Bt) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let _y0 = crate::chrome::header(c, t, f, "Bluetooth", None);
    // header right: ON/OFF + toggle
    let onoff = if bt.on { "ON" } else { "OFF" };
    right(c, f, 416.0, 65.0, onoff, &sty(Family::Mono, Weight::Regular, 10.0, if bt.on { t.acc } else { t.faint }, 0.12));
    toggle(c, t, 424, 56, 34, 18, 12, bt.on);

    let mut y;
    if bt.on && bt.connected.is_some() {
        let name = bt.connected.unwrap();
        let (cx, cy0, cw, ch) = (22, 100, 436, 138);
        fill_rect(c, cx, cy0, cw, ch, t.panel);
        stroke_rect(c, cx, cy0, cw, ch, t.line, 1);
        text::draw(c, f, 40.0, 122.0, "CONNECTED", &sty(Family::Mono, Weight::Regular, 9.0, t.acc, 0.18));
        right(c, f, 440.0, 122.0, "HP BATT 60%", &sty(Family::Mono, Weight::Regular, 9.0, t.dim, 0.1));
        text::draw(c, f, 40.0, 154.0, name, &sty(Family::Sans, Weight::Bold, 23.0, t.ink, 0.0));
        let codecln = format!("{} · 96 kHz · Sound quality preferred", bt.codec);
        text::draw(c, f, 40.0, 174.0, &codecln, &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.04));
        // buttons
        let by = 188;
        stroke_rect(c, 40, by, 198, 44, t.line, 1);
        center(c, f, 139.0, (by + 28) as f32, "Disconnect", &sty(Family::Sans, Weight::SemiBold, 13.0, t.dim, 0.0));
        stroke_rect(c, 246, by, 198, 44, t.line, 1);
        center(c, f, 345.0, (by + 28) as f32, &format!("Quality · {}", bt.codec), &sty(Family::Sans, Weight::SemiBold, 13.0, t.dim, 0.0));
        y = cy0 + ch;
    } else {
        dashed_box(c, t, 22, 100, 436, 64);
        let msg = if bt.on { "No device connected" } else { "Bluetooth is off" };
        center(c, f, 240.0, 138.0, msg, &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
        y = 164;
    }

    // paired list
    text::draw(c, f, 22.0, (y + 34) as f32, "PAIRED DEVICES — TAP TO CONNECT", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    y += 46;
    let rh = 58;
    for d in PAIRED {
        let cy = y + rh / 2;
        icons::bt(c, 34.0, cy as f32, 16.0, t.dim);
        let active = bt.connected == Some(d.name);
        let ncol = if active { t.acc } else { t.ink };
        text::draw(c, f, 58.0, (cy - 2) as f32, d.name, &sty(Family::Sans, Weight::SemiBold, 15.0, ncol, 0.0));
        text::draw(c, f, 58.0, (cy + 15) as f32, d.kind, &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.06));
        // CONNECT / ACTIVE pill
        let plabel = if active { "ACTIVE" } else { "CONNECT" };
        let pcol = if active { t.faint } else { t.acc };
        let ps = sty(Family::Mono, Weight::Regular, 10.0, pcol, 0.1);
        let pw = text::measure(f, plabel, &ps) as i32 + 24;
        stroke_rect(c, 458 - pw, cy - 13, pw, 26, pcol, 1);
        text::draw(c, f, (458 - pw + 12) as f32, (cy + 4) as f32, plabel, &ps);
        hline(c, y + rh, t.line);
        y += rh;
    }

    // pair new device + NFC hint
    let by = 700;
    fill_rect(c, 22, by, 436, 52, if bt.on { t.acc } else { t.line });
    let plabel_col = if bt.on { t.acc_ink } else { t.faint };
    icons::bt(c, 178.0, (by + 26) as f32, 17.0, plabel_col);
    text::draw(c, f, 196.0, (by + 31) as f32, "Pair new device", &sty(Family::Sans, Weight::Bold, 15.0, plabel_col, 0.0));
    icons::rx(c, 30.0, 776.0, 14.0, t.faint);
    text::draw(c, f, 46.0, 780.0, "NFC · TOUCH DEVICE TO REAR PANEL", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.08));
    right(c, f, 458.0, 780.0, "RECEIVER MODE ›", &sty(Family::Mono, Weight::Regular, 9.0, t.dim, 0.08));
}
