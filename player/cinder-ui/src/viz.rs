//! Visualiser — the Now Playing audio display. Two independent axes:
//!
//!   * `VizKind` — the STYLE (bars, mirror, segments, dots, wave). Purely how the columns are drawn.
//!   * `VizSize` — how much of the screen it is allowed to take, including OFF.
//!
//! `VizSize` exists because on the day theme the visualiser is drawn **over the album art**, and
//! the art is the emotional content of that screen — the visualiser is ambient. At the original
//! 42px opaque full-width it won that contest: a hard-edged graph parked across the lower third of
//! every cover. The smaller sizes trade height for transparency so the artwork reads through, and
//! `Veil` in particular has no hard top edge at all — its alpha ramps to nothing, so it reads as
//! part of the cover's own shadow rather than a panel sitting on top of it.
//!
//! Both axes are driven by the SAME per-column level, which comes from Sony's analyzer when it is
//! streaming and is otherwise absent (there is no synthetic fallback on device — see
//! cinder-ffi's viz_decay). `seed` only animates the host/sim previews.

use crate::canvas::Canvas;
use crate::widgets::fill_rect;
use embedded_graphics::pixelcolor::Rgb888;

/// The available visualiser types, in cycle order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VizKind {
    Bars,     // classic spectrum bars rising from the baseline
    Ribbon,   // one filled shape under a smooth contour — soft, reads as a single object
    Line,     // that contour alone, unfilled: the lowest-ink style there is
    Mirror,   // bars mirrored above + below a centre line
    Segments, // LED VU-meter: each bar is stacked lit segments
    Dots,     // a peak dot per column (sparse, low-ink)
    Wave,     // an oscilloscope-style waveform line
    Pulse,    // no per-column detail at all — one centred bar tracking overall level
}

pub const COUNT: u8 = 8;

pub fn from_index(i: u8) -> VizKind {
    match i % COUNT {
        0 => VizKind::Bars,
        1 => VizKind::Ribbon,
        2 => VizKind::Line,
        3 => VizKind::Mirror,
        4 => VizKind::Segments,
        5 => VizKind::Dots,
        6 => VizKind::Wave,
        _ => VizKind::Pulse,
    }
}

/// Short display name, already uppercased — the Now Playing spectrum page draws it as a caption at
/// ~20 fps, and `name(i).to_uppercase()` there allocated a `String` on every single frame.
pub fn name_upper(i: u8) -> &'static str {
    match from_index(i) {
        VizKind::Bars => "BARS",
        VizKind::Ribbon => "RIBBON",
        VizKind::Line => "LINE",
        VizKind::Mirror => "MIRROR",
        VizKind::Segments => "SEGMENTS",
        VizKind::Dots => "DOTS",
        VizKind::Wave => "WAVE",
        VizKind::Pulse => "PULSE",
    }
}

/// Short display name (for a settings row).
pub fn name(i: u8) -> &'static str {
    match from_index(i) {
        VizKind::Bars => "Bars",
        VizKind::Ribbon => "Ribbon",
        VizKind::Line => "Line",
        VizKind::Mirror => "Mirror",
        VizKind::Segments => "Segments",
        VizKind::Dots => "Dots",
        VizKind::Wave => "Wave",
        VizKind::Pulse => "Pulse",
    }
}

/// How much of the COVER PAGE the visualiser occupies, separate from the style: every size can
/// draw every `VizKind`.
///
/// Three options, deliberately. An earlier pass had six, including two short strips and a band
/// below the artwork — but once Now Playing became a pager the whole "don't cover the art" problem
/// is solved by simply not putting a visualiser on the cover page, and the below-art band was left
/// fighting the progress rail for 16px. A picker whose options look the same, or exist to work
/// around a problem that no longer exists, is not offering a choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VizSize {
    /// Nothing drawn on the cover page at all — a completely clean cover, not a smaller or
    /// relocated visualiser. The spectrum still lives on its own page, one swipe away, and costs
    /// nothing while you are not looking at it (the shell does not start Sony's analyzer).
    Off,
    /// Tall, with alpha ramping to nothing at the top — no hard edge anywhere, so it reads as part
    /// of the cover's own shadow rather than a panel sitting on it.
    Veil,
    /// The original: 42px, opaque, full width. Most legible, most intrusive.
    Full,
}

/// Number of settings the Visualiser row cycles through (Off is one of them). Ordered by how much
/// of the cover they claim.
pub const SIZE_COUNT: u8 = 3;

pub fn size_from_index(i: u8) -> VizSize {
    match i % SIZE_COUNT {
        0 => VizSize::Off,
        1 => VizSize::Veil,
        _ => VizSize::Full,
    }
}

/// Label for the Settings row.
pub fn size_name(i: u8) -> &'static str {
    match size_from_index(i) {
        VizSize::Off => "OFF",
        VizSize::Veil => "VEIL",
        VizSize::Full => "FULL",
    }
}

/// Geometry + opacity for a size, as `(y, h, alpha_top, alpha_bottom)`.
///
/// `bottom` is the y the visualiser stands on. Alpha is interpolated down the box, so `Veil` fades
/// out upward; a flat pair means uniform opacity, and 255/255 keeps the original opaque fast path
/// with no per-pixel blending at all.
pub fn size_box(size: VizSize, bottom: i32, night: bool) -> Option<(i32, i32, u8, u8)> {
    // Night puts the visualiser in empty space rather than over a cover, so intrusiveness is not
    // the same problem there — the sizes still apply, scaled to that layout's smaller band.
    let (h, a_top, a_bot) = match (size, night) {
        (VizSize::Off, _) => return None,
        (VizSize::Veil, false) => (64, 0, 180),
        (VizSize::Veil, true) => (28, 0, 180),
        (VizSize::Full, false) => (42, 255, 255),
        (VizSize::Full, true) => (16, 255, 255),
    };
    Some((bottom - h, h, a_top, a_bot))
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
    a_top: u8,
    a_bot: u8,
) {
    let n = n.max(1);
    let bw = ((w - gap * (n - 1)) / n).max(1);
    // Alpha for a pixel row, interpolated down the WHOLE box (not the individual bar), so a ramp
    // fades the visualiser out as one object instead of fading each bar over its own height.
    let alpha_at = |yy: i32| -> u8 {
        if a_top == a_bot {
            return a_top;
        }
        let t = if h <= 1 { 1.0 } else { ((yy - y) as f32 / (h - 1) as f32).clamp(0.0, 1.0) };
        (a_top as f32 + (a_bot as f32 - a_top as f32) * t).round().clamp(0.0, 255.0) as u8
    };
    // Opaque is the original path: a straight store, no per-pixel blend, so `Full` costs exactly
    // what it always did and only the translucent sizes pay for compositing over the artwork.
    let opaque = a_top == 255 && a_bot == 255;
    let vf = |c: &mut Canvas, rx: i32, ry: i32, rw: i32, rh: i32, col: Rgb888| {
        if opaque {
            fill_rect(c, rx, ry, rw, rh, col);
            return;
        }
        for row in 0..rh {
            let yy = ry + row;
            let a = alpha_at(yy);
            if a == 0 {
                continue;
            }
            for cx in 0..rw {
                c.blend(rx + cx, yy, col, a);
            }
        }
    };
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
                vf(c, bx, y + h - bh, bw, bh, col);
            }
        }
        VizKind::Mirror => {
            let cy = y + h / 2;
            for i in 0..n {
                let half = ((level(i) * (h as f32 / 2.0)).round() as i32).max(1);
                let bx = x + i * (bw + gap);
                let col = if i % 4 == 0 { acc } else { dim };
                vf(c, bx, cy - half, bw, half, col); // up
                vf(c, bx, cy, bw, half, col); // down
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
                    vf(c, bx, sy, bw, seg_h, col);
                }
            }
        }
        VizKind::Dots => {
            let d = bw.min(4).max(2);
            for i in 0..n {
                let lv = level(i);
                let bx = x + i * (bw + gap) + (bw - d) / 2;
                let dy = y + h - (lv * h as f32) as i32 - d;
                vf(c, bx, dy.max(y), d, d, acc); // peak dot
                                                 // a faint baseline tick under each column
                vf(c, bx, y + h - 1, d, 1, dim);
            }
        }
        VizKind::Ribbon | VizKind::Line => {
            // One smooth contour across the whole box instead of 36 separate rectangles. The level
            // is interpolated between column centres per PIXEL column, so the shape reads as a
            // single object — which is the point: bars are 36 things competing with the artwork,
            // a ribbon is one.
            let top_at = |px: i32| -> i32 {
                let step = (bw + gap).max(1);
                // Position along the column axis, in units of columns, sampled at column CENTRES.
                let f = ((px - x) as f32 - bw as f32 / 2.0) / step as f32;
                let i0 = (f.floor() as i32).clamp(0, n - 1);
                let i1 = (i0 + 1).min(n - 1);
                let frac = (f - i0 as f32).clamp(0.0, 1.0);
                let lv = level(i0) * (1.0 - frac) + level(i1) * frac;
                y + h - ((lv * h as f32).round() as i32).clamp(1, h)
            };
            // The crest has to be CONNECTED, not one 2px stub per pixel column: between adjacent
            // columns the contour can jump tens of pixels, and stamping a stub at each one draws a
            // dotted line up a cliff instead of a curve. Each column fills the span between its own
            // top and the previous column's, which is the integer equivalent of joining the points.
            let mut prev: Option<i32> = None;
            for px in 0..w {
                let cx = x + px;
                let ty = top_at(cx);
                if kind == VizKind::Ribbon {
                    vf(c, cx, ty, 1, y + h - ty, dim);
                }
                let (c0, c1) = match prev {
                    Some(p) => (p.min(ty), p.max(ty)),
                    None => (ty, ty),
                };
                vf(c, cx, c0, 1, (c1 - c0 + 2).min(y + h - c0).max(1), acc);
                prev = Some(ty);
            }
        }
        VizKind::Pulse => {
            // No per-column detail whatsoever: one centred bar whose WIDTH is the overall level.
            // The least busy thing that is still honestly derived from the audio — it says "this is
            // playing, and this is roughly how loud" and nothing else.
            let mut sum = 0.0f32;
            for i in 0..n {
                sum += level(i);
            }
            let overall = (sum / n as f32).clamp(0.0, 1.0);
            // Scale with the box and CENTRE it. Pinning a 6px bar to the bottom looked right in a
            // 42px strip and absurd in the 348px spectrum page — a lone sliver at the foot of an
            // empty block. One style has to work at both sizes, so the bar is a fraction of the
            // height it is given.
            let bh = (h / 5).clamp(4, 56).min(h);
            let by = y + (h - bh) / 2;
            let pw = ((w as f32 * overall).round() as i32).max(2);
            let px = x + (w - pw) / 2;
            vf(c, x, by + bh / 2, w, 1, dim); // full-width rule: a scale to read the bar against
            vf(c, px, by, pw, bh, acc);
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
                line2(c, x0, y0, x1, y1, acc, &alpha_at, opaque);
            }
            // centre baseline
            vf(c, x, cy, w, 1, dim);
        }
    }
}

/// Thin 2px line via integer Bresenham (no embedded-graphics needed; bounds-checked put).
fn line2(
    c: &mut Canvas,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    col: Rgb888,
    alpha_at: &dyn Fn(i32) -> u8,
    opaque: bool,
) {
    let v = crate::canvas::to_u32(col);
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        if opaque {
            c.put(x, y, v);
            c.put(x, y + 1, v); // 2px thick
        } else {
            c.blend(x, y, col, alpha_at(y));
            c.blend(x, y + 1, col, alpha_at(y + 1));
        }
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
