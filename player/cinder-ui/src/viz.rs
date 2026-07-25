//! Visualiser types — several ways to draw the Now Playing audio visualiser, all in the Cinder
//! aesthetic (warm amber accent + dim neutral) and all driven by the SAME animated per-column
//! "spectrum" so switching type is purely a render choice. The shell advances `seed` while
//! playing; `kind` selects the style (user-cyclable). Until we tap real PCM (analyzer service),
//! the spectrum is a smooth synthetic function — the motion is decorative, not real audio.

use crate::canvas::Canvas;
use crate::widgets::fill_rect;
use embedded_graphics::pixelcolor::Rgb888;

/// The available visualiser types, in cycle order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VizKind {
    Bars,     // classic spectrum bars rising from the baseline
    Mirror,   // bars mirrored above + below a centre line
    Segments, // LED VU-meter: each bar is stacked lit segments
    Dots,     // a peak dot per column (sparse, low-ink)
    Wave,     // an oscilloscope-style waveform line
}

pub const COUNT: u8 = 5;

pub fn from_index(i: u8) -> VizKind {
    match i % COUNT {
        0 => VizKind::Bars,
        1 => VizKind::Mirror,
        2 => VizKind::Segments,
        3 => VizKind::Dots,
        _ => VizKind::Wave,
    }
}

/// Short display name (for a settings row).
pub fn name(i: u8) -> &'static str {
    match from_index(i) {
        VizKind::Bars => "Bars",
        VizKind::Mirror => "Mirror",
        VizKind::Segments => "Segments",
        VizKind::Dots => "Dots",
        VizKind::Wave => "Wave",
    }
}

/// Synthetic per-column level 0..1 (used when no real spectrum is supplied). Two sine components
/// give livelier, less obviously-periodic motion than a single sine.
#[inline]
fn synth(i: i32, seed: f32) -> f32 {
    let a = (i as f32 * 1.93 + seed * 2.7).sin().abs();
    let b = (i as f32 * 0.71 - seed * 1.6).sin().abs();
    (0.14 + 0.7 * a + 0.16 * b).clamp(0.0, 1.0)
}

/// Draw the visualiser of `kind` into the box (x, y, w, h) with `n` columns. If `levels` is
/// Some (real FFT spectrum, 0..1), the columns use it; otherwise the synthetic `seed` motion.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    n: i32,
    gap: i32,
    seed: f32,
    kind: VizKind,
    acc: Rgb888,
    dim: Rgb888,
    levels: Option<&[f32]>,
) {
    let n = n.max(1);
    let bw = ((w - gap * (n - 1)) / n).max(1);
    // per-column level: real spectrum (mapped to n columns) if present, else synthetic
    let level = |i: i32| -> f32 {
        match levels {
            Some(l) if !l.is_empty() => l[(i as usize * l.len()) / n as usize % l.len()].clamp(0.0, 1.0),
            _ => synth(i, seed),
        }
    };
    match kind {
        VizKind::Bars => {
            for i in 0..n {
                let bh = ((level(i) * h as f32).round() as i32).max(2);
                let bx = x + i * (bw + gap);
                let col = if i % 4 == 0 { acc } else { dim };
                fill_rect(c, bx, y + h - bh, bw, bh, col);
            }
        }
        VizKind::Mirror => {
            let cy = y + h / 2;
            for i in 0..n {
                let half = ((level(i) * (h as f32 / 2.0)).round() as i32).max(1);
                let bx = x + i * (bw + gap);
                let col = if i % 4 == 0 { acc } else { dim };
                fill_rect(c, bx, cy - half, bw, half, col); // up
                fill_rect(c, bx, cy, bw, half, col); // down
            }
        }
        VizKind::Segments => {
            // each column is a stack of fixed segments; light the bottom `lit` of them
            let seg_h = 4;
            let seg_gap = 2;
            let segs = (h / (seg_h + seg_gap)).max(1);
            for i in 0..n {
                let lit = (level(i) * segs as f32).round() as i32;
                let bx = x + i * (bw + gap);
                for s in 0..segs {
                    let sy = y + h - (s + 1) * (seg_h + seg_gap);
                    // top-most lit segments accent, the rest dim; unlit = faint baseline
                    let col = if s < lit {
                        if s >= lit - 2 { acc } else { dim }
                    } else {
                        continue; // leave unlit cells empty for a cleaner look
                    };
                    fill_rect(c, bx, sy, bw, seg_h, col);
                }
            }
        }
        VizKind::Dots => {
            let d = bw.min(4).max(2);
            for i in 0..n {
                let lv = level(i);
                let bx = x + i * (bw + gap) + (bw - d) / 2;
                let dy = y + h - (lv * h as f32) as i32 - d;
                fill_rect(c, bx, dy.max(y), d, d, acc); // peak dot
                                                        // a faint baseline tick under each column
                fill_rect(c, bx, y + h - 1, d, 1, dim);
            }
        }
        VizKind::Wave => {
            // oscilloscope: a 2px line through per-column points around the centre
            let cy = y + h / 2;
            let amp = h as f32 / 2.0 - 2.0;
            let pt = |i: i32| -> (i32, i32) {
                let lv = level(i) * 2.0 - 1.0; // -1..1
                (x + i * (bw + gap) + bw / 2, cy + (lv * amp) as i32)
            };
            for i in 0..n - 1 {
                let (x0, y0) = pt(i);
                let (x1, y1) = pt(i + 1);
                line2(c, x0, y0, x1, y1, acc);
            }
            // centre baseline
            fill_rect(c, x, cy, w, 1, dim);
        }
    }
}

/// Thin 2px line via integer Bresenham (no embedded-graphics needed; bounds-checked put).
fn line2(c: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, col: Rgb888) {
    let v = crate::canvas::to_u32(col);
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        c.put(x, y, v);
        c.put(x, y + 1, v); // 2px thick
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
