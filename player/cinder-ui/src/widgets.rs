//! Small shared draw helpers used across screens — flat fills, 1px hairlines,
//! stroked borders, text alignment, and the Cinder toggle switch (`CToggle` /
//! the header on/off switches in cinder-proto-screens3/4.jsx).

use crate::canvas::{Canvas, W};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

/// Full-width 1px hairline at row `y`.
pub fn hline(c: &mut Canvas, y: i32, col: Rgb888) {
    fill_rect(c, 0, y, W as i32, 1, col);
}

/// Stroked (outline) rectangle.
pub fn stroke_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888, weight: u32) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_stroke(col, weight))
        .draw(c)
        .ok();
}

pub fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

/// Draw text right-aligned so it ends at `xr`.
pub fn right(c: &mut Canvas, f: &FontSet, xr: f32, baseline: f32, s: &str, st: &TextStyle) {
    let w = text::measure(f, s, st);
    text::draw(c, f, xr - w, baseline, s, st);
}

/// Draw text horizontally centred on `cx`.
pub fn center(c: &mut Canvas, f: &FontSet, cx: f32, baseline: f32, s: &str, st: &TextStyle) {
    let w = text::measure(f, s, st);
    text::draw(c, f, cx - w / 2.0, baseline, s, st);
}

/// Truncate `s` (with a trailing ellipsis) so it fits within `max_w` px.
pub fn fit(f: &FontSet, s: &str, st: &TextStyle, max_w: f32) -> String {
    if text::measure(f, s, st) <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let trial = format!("{}{}…", out, ch);
        if text::measure(f, &trial, st) > max_w {
            break;
        }
        out.push(ch);
    }
    format!("{}…", out)
}

/// Cinder toggle switch: a 1px box with a square knob, accent when on.
pub fn toggle(c: &mut Canvas, t: &Theme, x: i32, y: i32, w: i32, h: i32, knob: i32, on: bool) {
    stroke_rect(c, x, y, w, h, if on { t.acc } else { t.line }, 1);
    let inset = (h - knob) / 2;
    let kx = if on { x + w - inset - knob } else { x + inset };
    let ky = y + inset;
    fill_rect(c, kx, ky, knob, knob, if on { t.acc } else { t.faint });
}

/// Static visualiser bar strip (`FBars`): deterministic heights, every 4th bar
/// accent. Bars are bottom-aligned within the (x,y,w,h) box.
pub fn bars(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, n: i32, gap: i32, seed: f32, acc: Rgb888, dim: Rgb888) {
    let bw = ((w - gap * (n - 1)) / n).max(1);
    for i in 0..n {
        let v = 0.18 + 0.82 * (i as f32 * 1.93 + seed * 2.7).sin().abs();
        let bh = ((v * h as f32).round() as i32).max(2);
        let bx = x + i * (bw + gap);
        let col = if i % 4 == 0 { acc } else { dim };
        fill_rect(c, bx, y + h - bh, bw, bh, col);
    }
}

/// A mono-caption "pill": bordered box, accent fill when `on`. Returns its width.
pub fn pill(c: &mut Canvas, f: &FontSet, t: &Theme, x: i32, y: i32, h: i32, label: &str, on: bool) -> i32 {
    let st = sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc_ink } else { t.dim }, 0.08);
    let tw = text::measure(f, label, &st);
    let w = tw as i32 + 24;
    if on {
        fill_rect(c, x, y, w, h, t.acc);
    }
    stroke_rect(c, x, y, w, h, if on { t.acc } else { t.line }, 1);
    text::draw(c, f, (x + 12) as f32, (y + h / 2 + 4) as f32, label, &st);
    w
}
