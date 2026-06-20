//! USB-DAC — ported from cinder-proto-screens4.jsx `CUsbDac`. Toggle header;
//! when active shows the PC → Walkman → headphones path + a signal info box;
//! footer reports charging behaviour. (USB-DAC→LDAC routing comes in Phase E.)

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty, toggle};
use crate::Canvas;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, on: bool, eq_preset: &str, dsee: bool) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    crate::chrome::header(c, t, f, "USB-DAC", None);
    toggle(c, t, 424, 56, 34, 18, 12, on);

    if on {
        icons::usb(c, 240.0, 250.0, 44.0, t.acc);
        center(c, f, 240.0, 312.0, "USB-DAC active", &sty(Family::Sans, Weight::Bold, 22.0, t.ink, 0.0));
        center(c, f, 240.0, 340.0, "PC → NW-A55 → HEADPHONES", &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.1));
        // info box
        let (bx, by, bw, bh) = (60, 372, 360, 120);
        fill_rect(c, bx, by, bw, bh, t.panel);
        stroke_rect(c, bx, by, bw, bh, t.line, 1);
        let lines = [
            "INPUT  : PCM 24BIT / 96.0 KHZ".to_string(),
            "SOURCE : DESKTOP-7F3K (USB)".to_string(),
            format!("DSP    : EQ {}{}", eq_preset, if dsee { " · DSEE HX" } else { "" }),
            "OUTPUT : 3.5MM UNBALANCED".to_string(),
        ];
        let ls = sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.04);
        for (i, ln) in lines.iter().enumerate() {
            text::draw(c, f, (bx + 22) as f32, (by + 26 + i as i32 * 24) as f32, ln, &ls);
        }
    } else {
        icons::usb(c, 240.0, 320.0, 44.0, t.faint);
        center(c, f, 240.0, 382.0, "USB-DAC is off", &sty(Family::Sans, Weight::Bold, 19.0, t.dim, 0.0));
        center(c, f, 240.0, 412.0, "Turn on, then connect to a computer — the", &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
        center(c, f, 240.0, 432.0, "Walkman becomes its sound card.", &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
    }

    hline(c, 740, t.line);
    center(c, f, 240.0, 770.0, "CHARGING WHILE IN DAC MODE: ON", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.1));
}
