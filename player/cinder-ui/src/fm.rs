//! FM Radio — ported from cinder-proto-screens4.jsx `CFm`. Big frequency
//! readout, a tuning dial with ticks + needle, tune/seek buttons, a 3×2 preset
//! grid, and the antenna note.

use crate::data::FM_PRESETS;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty};
use crate::Canvas;

const MIN: f32 = 76.0;
const MAX: f32 = 108.0;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, freq: f32) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    crate::chrome::header(c, t, f, "FM Radio", Some("STEREO"));

    // big frequency + MHz, centred as a block
    let fstr = format!("{:.1}", freq);
    let fs = sty(Family::Mono, Weight::Light, 86.0, t.ink, -0.03);
    let ms = sty(Family::Mono, Weight::Regular, 19.0, t.dim, 0.0);
    let fw = text::measure(f, &fstr, &fs);
    let mw = text::measure(f, "MHz", &ms);
    let start = 240.0 - (fw + 9.0 + mw) / 2.0;
    text::draw(c, f, start, 205.0, &fstr, &fs);
    text::draw(c, f, start + fw + 9.0, 205.0, "MHz", &ms);

    // dial
    let (dx0, dw) = (30, 420);
    let line_y = 285;
    fill_rect(c, dx0, line_y, dw, 1, t.line);
    let mut tf = 80;
    while tf <= 106 {
        let x = dx0 + ((tf as f32 - MIN) / (MAX - MIN) * dw as f32) as i32;
        fill_rect(c, x, 275, 1, 20, t.line);
        center(c, f, x as f32, 309.0, &format!("{}", tf), &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        tf += 2;
    }
    let nx = dx0 + ((freq - MIN) / (MAX - MIN) * dw as f32) as i32;
    fill_rect(c, nx - 1, 265, 2, 40, t.acc);

    // tune / seek buttons
    let labels = ["−0.1", "SEEK −", "SEEK +", "+0.1"];
    let bs = sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.08);
    let mut widths = [0i32; 4];
    let mut total = 0;
    for (i, l) in labels.iter().enumerate() {
        widths[i] = text::measure(f, l, &bs) as i32 + 32;
        total += widths[i];
    }
    total += 12 * 3;
    let mut bx = 240 - total / 2;
    let by = 336;
    let bh = 44;
    for (i, l) in labels.iter().enumerate() {
        stroke_rect(c, bx, by, widths[i], bh, t.line, 1);
        center(c, f, (bx + widths[i] / 2) as f32, (by + bh / 2 + 4) as f32, l, &bs);
        bx += widths[i] + 12;
    }

    // presets — HOLD TO SAVE
    text::draw(c, f, 22.0, 408.0, "PRESETS — HOLD TO SAVE", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    let cols = [22, 170, 318];
    let cw = 138;
    let ch = 52;
    for (i, fp) in FM_PRESETS.iter().enumerate() {
        let cxp = cols[i % 3];
        let cyp = 418 + (i / 3) as i32 * (ch + 10);
        let active = (fp - freq).abs() < 0.05;
        if active {
            fill_rect(c, cxp, cyp, cw, ch, t.acc);
        }
        stroke_rect(c, cxp, cyp, cw, ch, if active { t.acc } else { t.line }, 1);
        let col = if active { t.acc_ink } else { t.dim };
        center(c, f, (cxp + cw / 2) as f32, (cyp + 24) as f32, &format!("{:.1}", fp), &sty(Family::Mono, Weight::Regular, 17.0, col, 0.0));
        center(c, f, (cxp + cw / 2) as f32, (cyp + 40) as f32, &format!("P{}", i + 1), &sty(Family::Mono, Weight::Regular, 10.0, col, 0.14));
    }

    hline(c, 740, t.line);
    center(c, f, 240.0, 768.0, "ANTENNA: HEADPHONE CABLE — WIRED HEADPHONES REQUIRED.", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.08));
}
