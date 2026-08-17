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
/// The gutter both helpers keep clear of the panel edges — the same 22px every screen's left
/// margin uses, so clamped text lines up with the rest of the layout instead of touching the glass.
const EDGE: f32 = 22.0;

/// Draw text ending at `xr`.
///
/// The string is fitted to the space between the left gutter and `xr` first. Right-aligning by
/// measuring puts the START x at `xr - w`, so an over-long run silently walks off the LEFT edge —
/// nothing on this device scrolls sideways, so those pixels are gone. It is size-dependent, which
/// is why it only ever showed up at 130-140% UI scale.
pub fn right(c: &mut Canvas, f: &FontSet, xr: f32, baseline: f32, s: &str, st: &TextStyle) {
    let s = fit(f, s, st, (xr - EDGE).max(0.0));
    let w = text::measure(f, &s, st);
    text::draw(c, f, xr - w, baseline, &s, st);
}

/// Draw LEFT-aligned text that stops at `right`, ellipsising if it would not fit.
///
/// For static copy at a literal x. Such a line is only safe at ONE text size — the x and the
/// string are fixed but the glyphs grow with the UI-scale slider — so at 140% a caption laid out
/// to look comfortable at 100% runs off the panel and the tail is silently discarded. Returns the
/// end x, like `text::draw`.
pub fn draw_fit(c: &mut Canvas, f: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle,
                right: f32) -> f32 {
    let s = fit(f, s, st, (right - x).max(0.0));
    text::draw(c, f, x, baseline, &s, st)
}

/// Draw text horizontally centred on `cx`.
///
/// Fitted to the panel first: centred text wider than the screen overflows BOTH edges at once, and
/// the ellipsis is far better than losing the ends of the sentence. Same size-dependence as
/// `right` — these are the two helpers where the x is computed from the measured width rather than
/// being a layout constant, so they are exactly where a text-scale change turns into overflow.
pub fn center(c: &mut Canvas, f: &FontSet, cx: f32, baseline: f32, s: &str, st: &TextStyle) {
    // Symmetric about cx, so the fitted run stays centred on the anchor rather than drifting.
    let half = (cx - EDGE).min(crate::canvas::W as f32 - EDGE - cx).max(0.0);
    let s = fit(f, s, st, half * 2.0);
    let w = text::measure(f, &s, st);
    text::draw(c, f, cx - w / 2.0, baseline, &s, st);
}

/// Truncate `s` (with a trailing ellipsis) so it fits within `max_w` px.
/// Draw a LEFT label and a RIGHT value on the same baseline without ever letting them collide.
///
/// The right item is measured first and keeps its full width (it is the value — truncating
/// "FLAC 24-bit / 96.0 kHz" to "FLAC 24-bit / 96.0…" would be worse than shortening the artist);
/// the left item is then `fit()` into whatever is left, minus `gap`.
///
/// This exists because a fixed left x plus a fixed right edge is only safe at ONE text size. With
/// the UI-scale slider both runs grow, and at 140% the Now Playing artist ran straight through the
/// codec string in the middle of the line. Anywhere two runs share a baseline, they have to be
/// laid out from their MEASURED widths — the same single-source rule the tab strip and the lists
/// already follow.
#[allow(clippy::too_many_arguments)]
pub fn row_pair(
    c: &mut Canvas,
    f: &FontSet,
    left_x: f32,
    right_x: f32,
    baseline: f32,
    left: &str,
    left_st: &TextStyle,
    right: &str,
    right_st: &TextStyle,
    gap: f32,
) {
    let rw = if right.is_empty() { 0.0 } else { text::measure(f, right, right_st) };
    let avail = (right_x - rw - gap - left_x).max(0.0);
    text::draw(c, f, left_x, baseline, &fit(f, left, left_st, avail), left_st);
    if !right.is_empty() {
        self::right(c, f, right_x, baseline, right, right_st);
    }
}

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

/// Indeterminate spinner: eight dots on a ring, with a bright head that advances with `phase`
/// (seconds) and a fading tail behind it.
///
/// Motion is the whole point. The Devices screen already printed a static "CONNECTING…" while a
/// link attempt was in flight, and static text is exactly what a *stalled* attempt looks like — the
/// user cannot tell "working on it" from "wedged". Anything that can take seconds and can fail needs
/// to visibly tick.
///
/// Drawn with `blend` only, so the tail fade costs nothing extra and it works on every backend. The
/// caller owns `phase`; nav advances it from real elapsed time and repaints while it moves.
pub fn spinner(c: &mut Canvas, cx: i32, cy: i32, r: i32, dot: i32, phase: f32, col: Rgb888) {
    const N: i32 = 8;
    let head = ((phase * 8.0) as i32).rem_euclid(N);
    for i in 0..N {
        let a = i as f32 * core::f32::consts::PI * 2.0 / N as f32 - core::f32::consts::FRAC_PI_2;
        let px = cx + (a.cos() * r as f32).round() as i32;
        let py = cy + (a.sin() * r as f32).round() as i32;
        // How far this dot sits BEHIND the head, so the trail fades backwards around the ring.
        let back = (head - i).rem_euclid(N);
        let alpha: u8 = match back {
            0 => 255,
            1 => 200,
            2 => 150,
            3 => 100,
            4 => 60,
            _ => 32,
        };
        for dy in 0..dot {
            for dx in 0..dot {
                c.blend(px + dx - dot / 2, py + dy - dot / 2, col, alpha);
            }
        }
    }
}
