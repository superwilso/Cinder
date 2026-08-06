//! Pairing flow — ported from cinder-proto-screens4.jsx `CPairing`. NFC card,
//! discoverable-device list (revealed as they're "found"), per-row PAIR action,
//! and a hold-to-pair tip footer.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{bars, center, fill_rect, hline, stroke_rect, sty};
use crate::Canvas;

pub struct Discoverable {
    pub name: &'static str,
    pub kind: &'static str,
}

pub const DISCOVERABLE: &[Discoverable] = &[
    Discoverable { name: "WH-1000XM5", kind: "Headphones · LDAC capable" },
    Discoverable { name: "JBL Flip 6", kind: "Speaker · AAC" },
    Discoverable { name: "Soundcore Q45", kind: "Headphones · LDAC capable" },
];

/// `found` = how many devices have appeared (0..=3); `pairing` = index mid-pair.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, found: usize, pairing: Option<usize>) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f);
    crate::chrome::header(c, t, f, "Pair new", Some("SCANNING…"));

    // NFC card
    let (cx, cy0, cw, ch) = (22, 100, 436, 72);
    fill_rect(c, cx, cy0, cw, ch, t.panel);
    stroke_rect(c, cx, cy0, cw, ch, t.line, 1);
    icons::rx(c, 46.0, (cy0 + ch / 2) as f32, 22.0, t.dim);
    text::draw(c, f, 76.0, (cy0 + 30) as f32, "One-touch NFC", &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
    text::draw(c, f, 76.0, (cy0 + 50) as f32, "Touch an NFC device to the rear panel to pair", &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));

    // DISCOVERABLE · n
    let mut y = cy0 + ch + 18;
    text::draw(c, f, 22.0, (y + 14) as f32, &format!("DISCOVERABLE · {}", found), &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    y += 28;

    if found == 0 {
        bars(c, 160, y + 16, 160, 20, 16, 4, 9.0, t.acc, t.line);
        center(c, f, 240.0, (y + 58) as f32, "LISTENING FOR DEVICES…", &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));
    } else {
        let rh = 62;
        for d in DISCOVERABLE.iter().take(found) {
            let cy = y + rh / 2;
            icons::bt(c, 34.0, cy as f32, 16.0, t.dim);
            text::draw(c, f, 58.0, (cy - 2) as f32, d.name, &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0));
            text::draw(c, f, 58.0, (cy + 15) as f32, d.kind, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.06));
            let is_pairing = pairing == DISCOVERABLE.iter().position(|x| x.name == d.name);
            let plabel = if is_pairing { "PAIRING…" } else { "PAIR" };
            let pcol = if is_pairing { t.faint } else { t.acc };
            let ps = sty(Family::Mono, Weight::Regular, 12.0, pcol, 0.1);
            let pw = text::measure(f, plabel, &ps) as i32 + 24;
            stroke_rect(c, 458 - pw, cy - 13, pw, 26, pcol, 1);
            text::draw(c, f, (458 - pw + 12) as f32, (cy + 4) as f32, plabel, &ps);
            hline(c, y + rh, t.line);
            y += rh;
        }
    }

    // tip footer
    hline(c, 736, t.line);
    text::draw(c, f, 22.0, 760.0, "TIP: HOLD YOUR HEADPHONES' POWER BUTTON ~7s", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
    text::draw(c, f, 22.0, 776.0, "UNTIL THE LED BLINKS BLUE.", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}
