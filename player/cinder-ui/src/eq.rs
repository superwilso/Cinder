//! Equalizer — ported from cinder-proto-screens3.jsx `CEq`. Preset pills
//! (FLAT/ROCK/JAZZ/A1/A2), a 10-band slider field with a zero line, accent
//! deviation fills + knobs, dB + Hz labels, and a Reset / Save footer.

use crate::canvas::W;
use crate::data::{EQ_BANDS, EQ_PRESETS};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

fn disc(c: &mut Canvas, cx: i32, cy: i32, d: u32, col: embedded_graphics::pixelcolor::Rgb888) {
    Circle::with_center(Point::new(cx, cy), d)
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, bands: &[i8; 10], preset: &str, sel: usize) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Equalizer", None);
    // header-right pill: CUSTOM <preset>
    let plabel = format!("CUSTOM {}", preset);
    let ps = sty(Family::Mono, Weight::Regular, 10.0, t.acc, 0.12);
    let pw = text::measure(f, &plabel, &ps) as i32 + 18;
    stroke_rect(c, 458 - pw, 52, pw, 26, t.acc, 1);
    text::draw(c, f, (458 - pw + 9) as f32, 69.0, &plabel, &ps);

    // preset pills row
    let mut px = 22;
    let py = y0 + 6;
    let ph = 30;
    for (name, _) in EQ_PRESETS {
        let on = name == preset;
        let st = sty(Family::Mono, Weight::Regular, 10.0, if on { t.acc_ink } else { t.dim }, 0.08);
        let w = text::measure(f, name, &st) as i32 + 26;
        if on {
            fill_rect(c, px, py, w, ph, t.acc);
        }
        stroke_rect(c, px, py, w, ph, if on { t.acc } else { t.line }, 1);
        text::draw(c, f, (px + 13) as f32, (py + ph / 2 + 4) as f32, name, &st);
        px += w + 8;
    }

    // slider field
    let (sy, by) = (170, 470);
    let mid = (sy + by) / 2;
    let span = (by - sy) / 2 - 10;
    // zero line (dashed)
    let mut dx = 30;
    while dx < 450 {
        fill_rect(c, dx, mid, 5, 1, t.line);
        dx += 11;
    }
    let slot = 420 / 10;
    for i in 0..10 {
        let bx = 30 + i as i32 * slot + slot / 2;
        let db = bands[i] as i32;
        let knob_y = mid - db * span / 10;
        // vertical guide
        fill_rect(c, bx - 1, sy, 2, by - sy, t.line);
        // deviation fill (mid → knob)
        let (fy, fh) = if knob_y < mid { (knob_y, mid - knob_y) } else { (mid, knob_y - mid) };
        fill_rect(c, bx - 1, fy, 2, fh, t.acc);
        let on = i == sel;
        // knob: bg ring + accent core (selected band gets a brighter, larger highlight ring)
        if on {
            disc(c, bx, knob_y, 22, t.ink);
        }
        disc(c, bx, knob_y, 16, t.bg);
        disc(c, bx, knob_y, if on { 12 } else { 10 }, t.acc);
        // dB label above (brighter on the selected band)
        let dbl = if db > 0 { format!("+{}", db) } else { format!("{}", db) };
        let dbcol = if on { t.ink } else if db != 0 { t.acc } else { t.faint };
        crate::widgets::center(c, f, bx as f32, (sy - 6) as f32, &dbl, &sty(Family::Mono, Weight::Regular, if on { 10.0 } else { 9.0 }, dbcol, 0.0));
        // Hz label below
        crate::widgets::center(c, f, bx as f32, (by + 22) as f32, EQ_BANDS[i], &sty(Family::Mono, Weight::Regular, 9.0, t.dim, 0.0));
    }

    // footer
    let fy = 740;
    hline(c, fy, t.line);
    let fcy = (fy + 60 / 2) as f32;
    text::draw(c, f, 22.0, fcy + 4.0, "Reset", &sty(Family::Sans, Weight::SemiBold, 14.0, t.dim, 0.0));
    right(c, f, 458.0, fcy + 4.0, "Save Sound Preset", &sty(Family::Sans, Weight::Bold, 14.0, t.acc, 0.0));
    let _ = W;
}
