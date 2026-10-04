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
    Wave,     // the spectrum drawn as a line about the centre (not the waveform: see Scope)
    Pulse,    // no per-column detail at all — one centred bar tracking overall level
    // ── from the decoded audio itself (the PCM tap), not from band levels ──────────────────────
    Scope,       // the real waveform, triggered on a rising zero crossing so it stands still
    Stereo,      // the stereo field: mid up, side across, with the L/R correlation under it
    Spectrogram, // the spectrum over the last few seconds, scrolling, brightness = level
    Meters,      // left and right: RMS bar, peak line and held peak on a dBFS scale
    Radial,      // the spectrum as spokes round a circle
}

pub const COUNT: u8 = 13;

/// Does this style draw from SAMPLES (the PCM tap) rather than band levels? Those need library
/// playback: FM, USB-DAC and the Bluetooth receiver reach Cinder only as Sony's twelve analyzer
/// bands, and a scope cannot be made from twelve numbers. The screen says so instead of drawing.
pub fn needs_samples(k: VizKind) -> bool {
    matches!(k, VizKind::Scope | VizKind::Stereo | VizKind::Meters)
}

/// What the sample-based styles draw from, built by cinder-ffi from the PCM tap each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Signal<'a> {
    /// A triggered stretch of the waveform, mono, -1..1 (Scope). Empty when the tap is not live.
    pub wave: &'a [f32],
    /// Left and right, decimated, -1..1, equal lengths (Stereo).
    pub left: &'a [f32],
    pub right: &'a [f32],
    /// 0..1 on a -60..0 dBFS scale: [peak L, peak R, RMS L, RMS R]. All zero when not live.
    pub meter: [f32; 4],
    /// The held peaks, L and R, on the same scale.
    pub hold: [f32; 2],
    /// The last `hist_rows` spectra, `hist_cols` levels each, a ring whose OLDEST row is `hist_head`
    /// (Spectrogram). Fed from whatever feeds the bars, so it works from Sony's analyzer too.
    pub hist: &'a [f32],
    pub hist_cols: usize,
    pub hist_rows: usize,
    pub hist_head: usize,
}

impl Signal<'_> {
    /// Is there anything for `k` to draw?
    pub fn has(&self, k: VizKind) -> bool {
        match k {
            VizKind::Scope => self.wave.len() >= 2,
            VizKind::Stereo => !self.left.is_empty() && self.left.len() == self.right.len(),
            VizKind::Meters => self.meter.iter().chain(self.hold.iter()).any(|v| *v > 0.0),
            VizKind::Spectrogram => self.hist_cols > 0 && self.hist_rows > 0 && self.hist.len() >= self.hist_cols * self.hist_rows,
            _ => true,
        }
    }
}

pub fn from_index(i: u8) -> VizKind {
    match i % COUNT {
        0 => VizKind::Bars,
        1 => VizKind::Ribbon,
        2 => VizKind::Line,
        3 => VizKind::Mirror,
        4 => VizKind::Segments,
        5 => VizKind::Dots,
        6 => VizKind::Wave,
        7 => VizKind::Pulse,
        8 => VizKind::Scope,
        9 => VizKind::Stereo,
        10 => VizKind::Spectrogram,
        11 => VizKind::Meters,
        _ => VizKind::Radial,
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
        VizKind::Scope => "SCOPE",
        VizKind::Stereo => "STEREO FIELD",
        VizKind::Spectrogram => "SPECTROGRAM",
        VizKind::Meters => "METERS",
        VizKind::Radial => "RADIAL",
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
        VizKind::Scope => "Scope",
        VizKind::Stereo => "Stereo field",
        VizKind::Spectrogram => "Spectrogram",
        VizKind::Meters => "Meters",
        VizKind::Radial => "Radial",
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

/// The gap between columns for a column count: 3 px up to 36 columns (what the visualiser has
/// always drawn), narrower above so 64 columns still leave each one a few pixels wide.
pub fn gap_for(columns: usize) -> i32 {
    match columns {
        0..=36 => 3,
        37..=48 => 2,
        _ => 1,
    }
}

/// How many columns to draw for `levels`: one per level when there are any, else 36.
pub fn columns_for(levels: Option<&[f32]>) -> usize {
    levels.map_or(36, |l| if l.is_empty() { 36 } else { l.len().min(96) })
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
/// Some (real spectrum, 0..1), the columns use it; otherwise the synthetic `seed` motion.
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
    draw_with_peaks(c, x, y, w, h, n, gap, seed, kind, acc, dim, levels, None, a_top, a_bot);
}

/// `draw`, plus peak-hold markers.
///
/// The markers are a separate argument rather than a field of the level slice because they are a
/// different KIND of value: a bar is where the band is now (after a 300 ms decay), a marker is
/// where it last peaked. Drawing them from the same array would mean either smoothing the peaks —
/// which defeats the point of a peak — or not smoothing the bars.
///
/// They are drawn only for the styles where "the top of this column" is a place: a Wave has no
/// column height to mark, and a Pulse has no columns at all.
#[allow(clippy::too_many_arguments)]
pub fn draw_with_peaks(
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
    peaks: Option<&[f32]>,
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
        // Without a signal (an old caller, or no tap), the sample styles draw nothing here — see
        // `draw_any`, which is what every screen calls. Radial and Spectrogram need more than a
        // strip of columns too, and are drawn there.
        VizKind::Scope | VizKind::Stereo | VizKind::Meters | VizKind::Spectrogram | VizKind::Radial => {}
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

    // ── peak-hold markers ────────────────────────────────────────────────────────────────────
    // A 2px cap floating at each column's held peak. Skipped for the two styles where a column
    // top is not a thing (Wave is a waveform about the centre line; Pulse has one bar for the
    // whole spectrum), and skipped entirely when the caller passed none.
    if let Some(pk) = peaks {
        if !pk.is_empty() && !matches!(kind, VizKind::Wave | VizKind::Pulse) && !is_signal_style(kind) {
            let mirror = kind == VizKind::Mirror;
            for i in 0..n {
                let p = pk[(i as usize * pk.len()) / n as usize % pk.len()].clamp(0.0, 1.0);
                let bx = x + i * (bw + gap);
                if mirror {
                    let cy = y + h / 2;
                    let half = ((p * (h as f32 / 2.0)).round() as i32).max(1);
                    vf(c, bx, (cy - half).max(y), bw, 2, acc);
                    vf(c, bx, (cy + half - 2).min(y + h - 2), bw, 2, acc);
                } else {
                    let ph = ((p * h as f32).round() as i32).clamp(2, h);
                    vf(c, bx, y + h - ph, bw, 2, acc);
                }
            }
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

/// The styles `draw_with_peaks` does not draw: the sample ones, and the two that are not columns.
pub fn is_signal_style(k: VizKind) -> bool {
    matches!(k, VizKind::Scope | VizKind::Stereo | VizKind::Meters | VizKind::Spectrogram | VizKind::Radial)
}

/// Is there anything to draw for `kind`? The screens use this to decide between the visualiser and
/// their "no signal" line, so the decision cannot drift from what `draw_any` would draw.
pub fn can_draw(kind: VizKind, levels: Option<&[f32]>, sig: Option<&Signal>) -> bool {
    match kind {
        VizKind::Scope | VizKind::Stereo | VizKind::Meters | VizKind::Spectrogram => sig.is_some_and(|s| s.has(kind)),
        _ => levels.is_some_and(|l| !l.is_empty()),
    }
}

/// THE entry point: every screen that draws a visualiser calls this. Columns and gap come from the
/// levels; the signal styles get the signal.
#[allow(clippy::too_many_arguments)]
pub fn draw_any(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    seed: f32,
    kind: VizKind,
    acc: Rgb888,
    dim: Rgb888,
    levels: Option<&[f32]>,
    peaks: Option<&[f32]>,
    sig: Option<&Signal>,
    a_top: u8,
    a_bot: u8,
) {
    let px = Px { x, y, h, a_top, a_bot };
    match kind {
        VizKind::Scope => {
            if let Some(s) = sig.filter(|s| s.has(kind)) {
                draw_scope(c, &px, w, s.wave, acc, dim);
            }
        }
        VizKind::Stereo => {
            if let Some(s) = sig.filter(|s| s.has(kind)) {
                draw_stereo(c, &px, w, s.left, s.right, acc, dim);
            }
        }
        VizKind::Meters => {
            if let Some(s) = sig.filter(|s| s.has(kind)) {
                draw_meters(c, &px, w, s.meter, s.hold, acc, dim);
            }
        }
        VizKind::Spectrogram => {
            if let Some(s) = sig.filter(|s| s.has(kind)) {
                draw_spectrogram(c, &px, w, s, acc, dim);
            }
        }
        VizKind::Radial => {
            let cols = columns_for(levels);
            let level = |i: usize| -> f32 {
                match levels {
                    Some(l) if !l.is_empty() => l[i * l.len() / cols % l.len()].clamp(0.0, 1.0),
                    _ => synth(i as i32, seed),
                }
            };
            draw_radial(c, &px, w, cols, &level, peaks, acc, dim);
        }
        _ => {
            let n = columns_for(levels);
            draw_with_peaks(c, x, y, w, h, n as i32, gap_for(n), seed, kind, acc, dim, levels, peaks, a_top, a_bot);
        }
    }
}

/// The box's vertical alpha ramp, shared by the signal styles.
struct Px {
    x: i32,
    y: i32,
    h: i32,
    a_top: u8,
    a_bot: u8,
}

impl Px {
    fn alpha(&self, yy: i32) -> u8 {
        if self.a_top == self.a_bot {
            return self.a_top;
        }
        let t = if self.h <= 1 { 1.0 } else { ((yy - self.y) as f32 / (self.h - 1) as f32).clamp(0.0, 1.0) };
        (self.a_top as f32 + (self.a_bot as f32 - self.a_top as f32) * t).round().clamp(0.0, 255.0) as u8
    }
    fn dot(&self, c: &mut Canvas, x: i32, y: i32, col: Rgb888, a: u8) {
        if y < self.y || y >= self.y + self.h {
            return;
        }
        let a = ((a as u32 * self.alpha(y) as u32) / 255) as u8;
        if a == 255 {
            c.put(x, y, crate::canvas::to_u32(col));
        } else if a > 0 {
            c.blend(x, y, col, a);
        }
    }
    fn rect(&self, c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
        if self.a_top == 255 && self.a_bot == 255 {
            let y0 = y.max(self.y);
            let y1 = (y + h).min(self.y + self.h);
            if y1 > y0 {
                fill_rect(c, x, y0, w, y1 - y0, col);
            }
            return;
        }
        for yy in y..y + h {
            for xx in x..x + w {
                self.dot(c, xx, yy, col, 255);
            }
        }
    }
    fn line(&self, c: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, col: Rgb888, thick: i32) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            for t in 0..thick {
                self.dot(c, x, y + t, col, 255);
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
}

/// `a` toward `b` by `t` (0..1).
fn mix(a: Rgb888, b: Rgb888, t: f32) -> Rgb888 {
    use embedded_graphics::pixelcolor::RgbColor;
    let t = t.clamp(0.0, 1.0);
    let m = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * t).round() as u8;
    Rgb888::new(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// The waveform: a line through the samples across the box, a hairline at zero. The trace is
/// already triggered upstream, so a steady tone stands still instead of crawling.
fn draw_scope(c: &mut Canvas, px: &Px, w: i32, wave: &[f32], acc: Rgb888, dim: Rgb888) {
    let cy = px.y + px.h / 2;
    let amp = (px.h / 2 - 2) as f32;
    px.rect(c, px.x, cy, w, 1, dim);
    let n = wave.len();
    let pt = |i: usize| -> (i32, i32) {
        let xx = px.x + (i as i64 * (w - 1) as i64 / (n - 1).max(1) as i64) as i32;
        (xx, cy - (wave[i].clamp(-1.0, 1.0) * amp).round() as i32)
    };
    let mut prev = pt(0);
    for i in 1..n {
        let p = pt(i);
        px.line(c, prev.0, prev.1, p.0, p.1, acc, 2);
        prev = p;
    }
}

/// The stereo field (a goniometer): every sample pair as a point, mid (L+R) up and side (R−L)
/// across. Mono is a vertical line, wide stereo a cloud, out of phase lies on its side. Gain rides
/// the loudest point so a quiet passage still has a shape; under it, the L/R correlation from −1
/// to +1 — the one number a stereo field is usually read for.
fn draw_stereo(c: &mut Canvas, px: &Px, w: i32, l: &[f32], r: &[f32], acc: Rgb888, dim: Rgb888) {
    let bar_h = 10;
    let size = (px.h - bar_h - 10).min(w).max(8);
    let cx = px.x + w / 2;
    let cy = px.y + size / 2;
    let half = size / 2 - 2;
    // guides: the M axis, the S axis, and the two channel diagonals
    px.rect(c, cx, cy - half, 1, half * 2, dim);
    px.rect(c, cx - half, cy, half * 2, 1, dim);
    for k in (-half..=half).step_by(6) {
        px.dot(c, cx + k, cy - k, dim, 255);
        px.dot(c, cx + k, cy + k, dim, 255);
    }
    let mut biggest = 0.0f32;
    let (mut lr, mut ll, mut rr) = (0.0f32, 0.0f32, 0.0f32);
    for (a, b) in l.iter().zip(r) {
        biggest = biggest.max(((a + b) * std::f32::consts::FRAC_1_SQRT_2).abs()).max(((b - a) * std::f32::consts::FRAC_1_SQRT_2).abs());
        lr += a * b;
        ll += a * a;
        rr += b * b;
    }
    let gain = if biggest > 1e-4 { (0.92 / biggest).clamp(1.0, 6.0) } else { 1.0 };
    for (a, b) in l.iter().zip(r) {
        let m = (a + b) * std::f32::consts::FRAC_1_SQRT_2 * gain;
        let sd = (b - a) * std::f32::consts::FRAC_1_SQRT_2 * gain;
        let x = cx + (sd * half as f32).round() as i32;
        let y = cy - (m * half as f32).round() as i32;
        px.dot(c, x, y, acc, 200);
        px.dot(c, x + 1, y, acc, 120);
    }
    // correlation: -1 (left end) .. +1 (right end), marker from the centre
    let corr = if ll > 0.0 && rr > 0.0 { (lr / (ll * rr).sqrt()).clamp(-1.0, 1.0) } else { 0.0 };
    let by = px.y + size + 6;
    let bx0 = cx - half;
    px.rect(c, bx0, by + bar_h / 2, half * 2, 1, dim);
    px.rect(c, cx, by, 1, bar_h, dim);
    let mx = cx + (corr * half as f32).round() as i32;
    let (a, b) = (mx.min(cx), mx.max(cx));
    px.rect(c, a, by + 2, (b - a).max(2), bar_h - 4, acc);
}

/// Two horizontal meters, L over R: the RMS as a bar, the peak as a line, the held peak as a tick,
/// on a −60..0 dBFS scale with ticks at −48, −24, −12, −6 and −3.
fn draw_meters(c: &mut Canvas, px: &Px, w: i32, m: [f32; 4], hold: [f32; 2], acc: Rgb888, dim: Rgb888) {
    let bar = (px.h / 4).clamp(6, 40);
    let gap = (px.h - 2 * bar) / 3;
    for ch in 0..2 {
        let by = px.y + gap + ch as i32 * (bar + gap);
        px.rect(c, px.x, by + bar - 1, w, 1, dim);
        for db in [-48.0f32, -24.0, -12.0, -6.0, -3.0, 0.0] {
            let tx = px.x + (((db + 60.0) / 60.0) * (w - 1) as f32).round() as i32;
            px.rect(c, tx, by + bar, 1, (bar / 4).max(2), dim);
        }
        let rms_w = (m[2 + ch].clamp(0.0, 1.0) * w as f32).round() as i32;
        px.rect(c, px.x, by, rms_w, bar - 2, mix(dim, acc, 0.55));
        let pk = px.x + (m[ch].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        px.rect(c, pk - 1, by, 2, bar - 2, acc);
        let hx = px.x + (hold[ch].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        px.rect(c, hx - 1, by - 3, 2, bar + 1, acc);
    }
}

/// The spectrogram: time runs left to right (newest at the right edge), frequency bottom to top,
/// and a cell's brightness is its level — background through the accent to near-white.
fn draw_spectrogram(c: &mut Canvas, px: &Px, w: i32, s: &Signal, acc: Rgb888, dim: Rgb888) {
    use embedded_graphics::pixelcolor::RgbColor;
    let rows = s.hist_rows;
    let cols = s.hist_cols;
    let cell_w = (w / rows as i32).max(1);
    let x0 = px.x + w - cell_w * rows as i32;
    let hot = mix(acc, Rgb888::WHITE, 0.6);
    for k in 0..rows {
        let row = (s.hist_head + k) % rows;
        let xx = x0 + k as i32 * cell_w;
        for band in 0..cols {
            let v = s.hist[row * cols + band].clamp(0.0, 1.0);
            if v < 0.04 {
                continue;
            }
            let y_top = px.y + px.h - ((band + 1) as i64 * px.h as i64 / cols as i64) as i32;
            let y_bot = px.y + px.h - (band as i64 * px.h as i64 / cols as i64) as i32;
            // A curve on the low half keeps the quiet cells near the background, so the loud
            // ones stand out the way they do on a real spectrogram rather than the whole block
            // glowing.
            let col = if v < 0.6 { mix(dim, acc, (v / 0.6).powf(1.8)) } else { mix(acc, hot, (v - 0.6) / 0.4) };
            px.rect(c, xx, y_top, cell_w, (y_bot - y_top).max(1), col);
        }
    }
}

/// The spectrum as spokes round a circle: lows at the top, running clockwise, a held-peak dot on
/// each spoke when peaks are on.
#[allow(clippy::too_many_arguments)]
fn draw_radial(c: &mut Canvas, px: &Px, w: i32, n: usize, level: &dyn Fn(usize) -> f32, peaks: Option<&[f32]>,
               acc: Rgb888, dim: Rgb888) {
    let cx = px.x + w / 2;
    let cy = px.y + px.h / 2;
    let rmax = (w.min(px.h) / 2 - 2) as f32;
    let r0 = rmax * 0.36;
    let spokes = n.clamp(12, 96);
    // the inner ring
    let ring = (r0 * 6.3) as usize;
    for k in 0..ring.max(12) {
        let a = k as f32 / ring.max(12) as f32 * std::f32::consts::TAU;
        px.dot(c, cx + (a.cos() * (r0 - 3.0)) as i32, cy + (a.sin() * (r0 - 3.0)) as i32, dim, 255);
    }
    for i in 0..spokes {
        let a = i as f32 / spokes as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (sa, ca) = a.sin_cos();
        let lv = level(i * n / spokes);
        let r1 = r0 + lv * (rmax - r0);
        let col = if i % 4 == 0 { acc } else { mix(dim, acc, 0.5 + 0.5 * lv) };
        px.line(c, cx + (ca * r0) as i32, cy + (sa * r0) as i32, cx + (ca * r1) as i32, cy + (sa * r1) as i32, col, 2);
        if let Some(pk) = peaks.filter(|p| !p.is_empty()) {
            let p = pk[(i * pk.len()) / spokes % pk.len()].clamp(0.0, 1.0);
            let rp = r0 + p * (rmax - r0);
            px.rect(c, cx + (ca * rp) as i32 - 1, cy + (sa * rp) as i32 - 1, 3, 3, acc);
        }
    }
}

#[cfg(test)]
mod signal_tests {
    use super::*;
    use crate::canvas::{Canvas, W};
    use crate::theme::Theme;

    fn painted(c: &Canvas, bg: u32, x: i32, y: i32, w: i32, h: i32) -> (usize, usize) {
        let (mut inside, mut outside) = (0, 0);
        for yy in 0..crate::canvas::H as i32 {
            for xx in 0..W as i32 {
                if c.buf[yy as usize * W + xx as usize] != bg {
                    if (x..x + w).contains(&xx) && (y..y + h).contains(&yy) { inside += 1 } else { outside += 1 }
                }
            }
        }
        (inside, outside)
    }

    fn sig_with<'a>(wave: &'a [f32], l: &'a [f32], r: &'a [f32], hist: &'a [f32]) -> Signal<'a> {
        Signal { wave, left: l, right: r, meter: [0.8, 0.7, 0.5, 0.45], hold: [0.85, 0.75],
                 hist, hist_cols: 32, hist_rows: 64, hist_head: 5 }
    }

    /// Every style draws something with a signal, and nothing outside its box.
    #[test]
    fn every_style_stays_in_its_box() {
        let t = Theme::day();
        let wave: Vec<f32> = (0..480).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        let l: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin() * 0.6).collect();
        let r: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1 + 0.6).sin() * 0.6).collect();
        let hist: Vec<f32> = (0..32 * 64).map(|i| (i % 32) as f32 / 32.0).collect();
        let levels: Vec<f32> = (0..48).map(|i| 0.2 + 0.6 * ((i as f32) * 0.3).sin().abs()).collect();
        let sig = sig_with(&wave, &l, &r, &hist);
        let bg = crate::canvas::to_u32(t.bg);
        for k in 0..COUNT {
            let kind = from_index(k);
            let mut c = Canvas::new();
            c.fill(t.bg);
            draw_any(&mut c, 24, 154, 432, 348, 2.0, kind, t.acc, t.line, Some(&levels), None, Some(&sig), 255, 255);
            let (inside, outside) = painted(&c, bg, 24, 154, 432, 348);
            assert!(inside > 50, "{} drew {inside} pixels", name(k));
            assert_eq!(outside, 0, "{} painted outside its box", name(k));
        }
    }

    /// Without a tap, the sample styles report nothing to draw and draw nothing; the others still do.
    #[test]
    fn the_sample_styles_need_the_tap() {
        let levels = [0.5f32; 12];
        for k in 0..COUNT {
            let kind = from_index(k);
            let can = can_draw(kind, Some(&levels), None);
            assert_eq!(can, !matches!(kind, VizKind::Scope | VizKind::Stereo | VizKind::Meters | VizKind::Spectrogram),
                       "{}", name(k));
        }
        let empty = Signal::default();
        assert!(!empty.has(VizKind::Scope) && !empty.has(VizKind::Stereo) && !empty.has(VizKind::Meters));
    }

    /// The names round-trip and the new ones are where the settings file expects them.
    #[test]
    fn indices_are_stable() {
        assert_eq!(from_index(7), VizKind::Pulse, "the eight older styles keep their numbers");
        assert_eq!(from_index(8), VizKind::Scope);
        assert_eq!(from_index(12), VizKind::Radial);
        assert_eq!(from_index(COUNT), VizKind::Bars, "wraps");
    }
}
