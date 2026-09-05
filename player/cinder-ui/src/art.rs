//! Album-art swatches — 3-stop linear gradients per `data-art` (screens.css),
//! with a soft top-left highlight. Used full-bleed on Now Playing (opacity 1)
//! and as thumbnails in Library / Up Next / Artist (opacity = theme artDim:
//! 1.0 day, 0.30 night → blends toward `t.bg`).

use crate::canvas::Canvas;
use crate::theme::Theme;
use embedded_graphics::prelude::*;

fn stops(name: &str) -> [(f32, (u8, u8, u8)); 3] {
    match name {
        "harvest" => [(0.0, (0xe8, 0xc3, 0x4a)), (0.7, (0x8a, 0x6b, 0x1d)), (1.0, (0x2b, 0x20, 0x08))],
        "midnight" => [(0.0, (0x4a, 0x6d, 0xb8)), (0.6, (0x1a, 0x25, 0x47)), (1.0, (0x05, 0x08, 0x10))],
        "ferns" => [(0.0, (0x6b, 0x9e, 0x6b)), (0.6, (0x2f, 0x4d, 0x34)), (1.0, (0x0d, 0x1a, 0x10))],
        "halcyon" => [(0.0, (0xc4, 0xa3, 0xd4)), (0.6, (0x6b, 0x4a, 0x83)), (1.0, (0x1a, 0x0d, 0x2b))],
        "atlas" => [(0.0, (0x2a, 0x25, 0x22)), (0.5, (0x1a, 0x16, 0x14)), (1.0, (0x05, 0x04, 0x03))],
        "bloom" => [(0.0, (0xf0, 0xa3, 0xa0)), (0.6, (0xb0, 0x48, 0x55)), (1.0, (0x3a, 0x0d, 0x18))],
        "prism" => [(0.0, (0x4a, 0x8a, 0xcb)), (0.5, (0xb0, 0x4e, 0x9a)), (1.0, (0xd4, 0xa9, 0x55))],
        "static" => [(0.0, (0x2a, 0x2a, 0x2a)), (0.5, (0x1f, 0x1f, 0x1f)), (1.0, (0x1a, 0x1a, 0x1a))],
        "cassette" => [(0.0, (0xc8, 0xa4, 0x5b)), (0.5, (0x5a, 0x45, 0x20)), (1.0, (0x1d, 0x16, 0x10))],
        "kind" | "" => [(0.0, (0xd9, 0x77, 0x57)), (0.6, (0x8b, 0x3a, 0x1e)), (1.0, (0x2a, 0x11, 0x08))],
        // Real library items (album/track titles) hash to a distinct, stable gradient so
        // each looks different even before real album-art thumbnails are decoded.
        other => hashed_stops(other),
    }
}

/// HSV→RGB (h in degrees, s/v in 0..1).
fn hsv(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (
        (((r + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((g + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((b + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
    )
}

/// FNV-1a over the name — the same hash `hashed_stops` derives its hue from, exposed so a caller
/// can key a cache on "which gradient is this" without recomputing the gradient to find out.
/// Two names that collide here would share a swatch, which is already true of the hue itself.
pub fn name_key(name: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Derive a bright→mid→dark 3-stop gradient from a string hash (FNV-1a → hue).
fn hashed_stops(name: &str) -> [(f32, (u8, u8, u8)); 3] {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32;
    [
        (0.0, hsv(hue, 0.52, 0.80)),
        (0.6, hsv(hue, 0.62, 0.40)),
        (1.0, hsv((hue + 14.0) % 360.0, 0.66, 0.12)),
    ]
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// A decoded album-cover bitmap (packed RGB, 3 bytes/px). Produced by the shell's decoder
/// (cinder-ffi) already PRE-SCALED to its draw size, so `draw_image` is a plain blit —
/// no per-frame resampling while the visualiser animates over it.
#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

impl Image {
    /// Resample into a new dw×dh image (called once per track change / cache build, not per frame).
    ///
    /// SHRINKING USES AN AREA AVERAGE, not bilinear. Bilinear reads exactly four source pixels
    /// whatever the ratio, and album art on this device is ~1425×1425 going to 96 or 48 — a 15×
    /// or 30× reduction, where those four pixels are ~0.5% of the source and the other 99.5% is
    /// discarded. The result is not a soft thumbnail, it is an aliased one: fine detail folds down
    /// into false speckle, which reads as "pixelated" even though every pixel is in the right
    /// place. Averaging the whole source rectangle that maps to each output pixel is the correct
    /// filter, and it costs one pass over the source — trivial beside the JPEG decode that just
    /// produced it.
    ///
    /// Upscaling and equal sizes keep the bilinear path, where there is no aliasing to fix.
    pub fn scaled_to(&self, dw: usize, dh: usize) -> Image {
        let mut out = vec![0u8; dw * dh * 3];
        if self.w == 0 || self.h == 0 || dw == 0 || dh == 0 {
            return Image { w: dw, h: dh, rgb: out };
        }
        if dw <= self.w && dh <= self.h && (dw < self.w || dh < self.h) {
            return self.area_scaled(dw, dh);
        }
        let sx = self.w as f32 / dw as f32;
        let sy = self.h as f32 / dh as f32;
        for y in 0..dh {
            let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
            let y0 = (fy as usize).min(self.h - 1);
            let y1 = (y0 + 1).min(self.h - 1);
            let ty = fy - y0 as f32;
            for x in 0..dw {
                let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                let x0 = (fx as usize).min(self.w - 1);
                let x1 = (x0 + 1).min(self.w - 1);
                let tx = fx - x0 as f32;
                let o = (y * dw + x) * 3;
                for ch in 0..3 {
                    let p00 = self.rgb[(y0 * self.w + x0) * 3 + ch] as f32;
                    let p01 = self.rgb[(y0 * self.w + x1) * 3 + ch] as f32;
                    let p10 = self.rgb[(y1 * self.w + x0) * 3 + ch] as f32;
                    let p11 = self.rgb[(y1 * self.w + x1) * 3 + ch] as f32;
                    let v = p00 * (1.0 - tx) * (1.0 - ty) + p01 * tx * (1.0 - ty)
                        + p10 * (1.0 - tx) * ty + p11 * tx * ty;
                    out[o + ch] = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        Image { w: dw, h: dh, rgb: out }
    }

    /// Box filter: each output pixel is the mean of every source pixel that maps onto it. Only
    /// used when both axes shrink, so each output rectangle covers at least one source pixel.
    fn area_scaled(&self, dw: usize, dh: usize) -> Image {
        let mut out = vec![0u8; dw * dh * 3];
        for y in 0..dh {
            let sy0 = y * self.h / dh;
            let sy1 = (((y + 1) * self.h / dh).max(sy0 + 1)).min(self.h);
            for x in 0..dw {
                let sx0 = x * self.w / dw;
                let sx1 = (((x + 1) * self.w / dw).max(sx0 + 1)).min(self.w);
                let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                for yy in sy0..sy1 {
                    let row = yy * self.w;
                    for xx in sx0..sx1 {
                        let i = (row + xx) * 3;
                        r += self.rgb[i] as u32;
                        g += self.rgb[i + 1] as u32;
                        b += self.rgb[i + 2] as u32;
                        n += 1;
                    }
                }
                let o = (y * dw + x) * 3;
                out[o] = (r / n) as u8;
                out[o + 1] = (g / n) as u8;
                out[o + 2] = (b / n) as u8;
            }
        }
        Image { w: dw, h: dh, rgb: out }
    }
}

/// Blit a pre-scaled cover at (x0,y0). `opacity` blends toward `t.bg` exactly like `block`
/// (night thumbs sit back at 0.32). The image is drawn 1:1 — scale at decode time.
pub fn draw_image(c: &mut Canvas, t: &Theme, x0: i32, y0: i32, img: &Image, opacity: f32) {
    let (br, bg, bb) = (t.bg.r(), t.bg.g(), t.bg.b());
    let op = opacity.clamp(0.0, 1.0);
    // NIGHT DIM APPLIES TO PIXELS, NOT JUST TO THE PALETTE. Album art is decoded image data and
    // owes nothing to the theme's colours, so scaling the palette left covers at full brightness —
    // at the dimmest night step the UI went black and the artwork stayed blazing, which is exactly
    // backwards. Reported directly.
    //
    // A 256-entry LOOKUP TABLE rather than a multiply per channel: a full cover is 230,400 pixels
    // and three multiplies plus three divides each is real time on this CPU, where a table costs
    // one load. Built only when the dim is actually on, so the day path is byte-for-byte the code
    // it was before this existed and pays nothing.
    let lut: Option<[u8; 256]> = if t.dim_pct >= 100 {
        None
    } else {
        let mut l = [0u8; 256];
        for (i, e) in l.iter_mut().enumerate() {
            *e = ((i as u32) * t.dim_pct / 100) as u8;
        }
        Some(l)
    };
    if let Some(l) = lut {
        for yy in 0..img.h {
            let Some((skip, dst)) = c.row_run(y0 + yy as i32, x0, img.w) else { continue };
            let base = (yy * img.w + skip) * 3;
            let Some(src) = img.rgb.get(base..) else { continue };
            if op >= 1.0 {
                for (d, s) in dst.iter_mut().zip(src.chunks_exact(3)) {
                    *d = ((l[s[0] as usize] as u32) << 16)
                        | ((l[s[1] as usize] as u32) << 8)
                        | l[s[2] as usize] as u32;
                }
            } else {
                for (d, s) in dst.iter_mut().zip(src.chunks_exact(3)) {
                    let r = l[lerp(br, s[0], op) as usize];
                    let g = l[lerp(bg, s[1], op) as usize];
                    let b = l[lerp(bb, s[2], op) as usize];
                    *d = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                }
            }
        }
        return;
    }
    // Row at a time. The per-pixel `put` this replaced re-checked four bounds and recomputed an
    // index for each of a full cover's 230,400 pixels; `row_run` does the clip once per row and
    // hands back a slice, so the inner loop is just a pack-and-store the optimiser can unroll.
    for yy in 0..img.h {
        let Some((skip, dst)) = c.row_run(y0 + yy as i32, x0, img.w) else { continue };
        let base = (yy * img.w + skip) * 3;
        let Some(src) = img.rgb.get(base..) else { continue };
        if op >= 1.0 {
            for (d, s) in dst.iter_mut().zip(src.chunks_exact(3)) {
                *d = ((s[0] as u32) << 16) | ((s[1] as u32) << 8) | s[2] as u32;
            }
        } else {
            for (d, s) in dst.iter_mut().zip(src.chunks_exact(3)) {
                let r = lerp(br, s[0], op);
                let g = lerp(bg, s[1], op);
                let b = lerp(bb, s[2], op);
                *d = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            }
        }
    }
}

/// The gradient's base colour ramp, precomputed into a table indexed by position along the
/// diagonal. The two-segment interpolation it replaces did a float DIVIDE per pixel, and the ramp
/// is a smooth function of one variable — so 512 samples reproduce it below the eye's resolution
/// (each entry spans well under one output colour step) for the cost of 512 evaluations instead of
/// one per pixel.
const RAMP: usize = 512;

fn ramp_lut(name: &str) -> [(u8, u8, u8); RAMP] {
    let s = stops(name);
    // Hoisted reciprocals: these two divides used to be inside the pixel loop.
    let inv0 = 1.0 / (s[1].0 - s[0].0).max(1e-3);
    let inv1 = 1.0 / (s[2].0 - s[1].0).max(1e-3);
    let mut lut = [(0u8, 0u8, 0u8); RAMP];
    for (i, e) in lut.iter_mut().enumerate() {
        let pos = i as f32 / (RAMP - 1) as f32;
        *e = if pos <= s[1].0 {
            let u = (pos - s[0].0) * inv0;
            (lerp(s[0].1 .0, s[1].1 .0, u), lerp(s[0].1 .1, s[1].1 .1, u), lerp(s[0].1 .2, s[1].1 .2, u))
        } else {
            let u = (pos - s[1].0) * inv1;
            (lerp(s[1].1 .0, s[2].1 .0, u), lerp(s[1].1 .1, s[2].1 .1, u), lerp(s[1].1 .2, s[2].1 .2, u))
        };
    }
    lut
}

/// Radius of the soft top-left highlight. Outside it the highlight contributes exactly nothing,
/// which is most of any block — so the `sqrt` only runs inside the disc instead of on every pixel.
const HL_R: f32 = 0.65;
const HL_R2: f32 = HL_R * HL_R;

/// One row of the gradient, written as packed 0x00RRGGBB into `dst`. `x_off` is the first source
/// column (for a row clipped at the left edge).
///
/// THE single copy of the gradient maths: `block` and `bake` both call it, so a cached gradient can
/// never drift from a directly-drawn one.
#[allow(clippy::too_many_arguments)]
fn grad_row(
    dst: &mut [u32],
    lut: &[(u8, u8, u8); RAMP],
    x_off: usize,
    v: f32,
    inv_w: f32,
    op: f32,
    bg: (u8, u8, u8),
) {
    let dy = v - 0.1;
    let dyy = dy * dy;
    for (i, d) in dst.iter_mut().enumerate() {
        let u_ = (x_off + i) as f32 * inv_w;
        let pos = ((u_ + v) * 0.5).clamp(0.0, 1.0);
        let (mut r, mut g, mut b) = lut[(pos * (RAMP - 1) as f32) as usize];
        // Soft radial highlight near top-left (the .art ::after overlay). Skipped entirely outside
        // the disc, which is where the `sqrt` used to be paid for every pixel of every block.
        let dx = u_ - 0.2;
        let d2 = dx * dx + dyy;
        if d2 < HL_R2 {
            let hl = (1.0 - d2.sqrt() * (1.0 / HL_R)).clamp(0.0, 1.0) * 0.16;
            r = lerp(r, 255, hl);
            g = lerp(g, 255, hl);
            b = lerp(b, 255, hl);
        }
        if op < 1.0 {
            r = lerp(bg.0, r, op);
            g = lerp(bg.1, g, op);
            b = lerp(bg.2, b, op);
        }
        *d = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
    }
}

/// Draw a gradient art block at (x0,y0) sized w×h. `opacity` (0..1) blends the
/// gradient toward `t.bg` so night-dimmed art and small thumbnails sit back.
pub fn block(c: &mut Canvas, t: &Theme, x0: i32, y0: i32, w: i32, h: i32, name: &str, opacity: f32) {
    let lut = ramp_lut(name);
    let bg = (t.bg.r(), t.bg.g(), t.bg.b());
    let op = opacity.clamp(0.0, 1.0);
    let inv_w = 1.0 / (w.max(1) as f32);
    let inv_h = 1.0 / (h.max(1) as f32);
    for yy in 0..h {
        let Some((skip, dst)) = c.row_run(y0 + yy, x0, w.max(0) as usize) else { continue };
        grad_row(dst, &lut, skip, yy as f32 * inv_h, inv_w, op, bg);
    }
}

/// Cached form of [`block`]: bake once, blit thereafter.
///
/// Keyed on the name's hash, the edge, the opacity and the background it was blended toward — the
/// four things the pixels depend on. Baked at the row's ACTUAL opacity rather than at 1.0 and
/// blended on the way out, because blending twice quantises twice; `the_baked_gradient_matches_the
/// _drawn_one_exactly` pins the two paths together.
///
/// USE THIS FROM ANY PER-FRAME PATH. The library rows have had this since the Now Playing art was
/// fixed; the album drill-in's 96×96 cover, the Playlists rows and Now Playing's night header did
/// not, and each was recomputing a gradient every single frame — 9,216 pixels for the album cover
/// alone, at a table lookup, a squared-distance test and a `sqrt` per pixel.
///
/// Above `CACHE_MAX_EDGE` it declines and draws directly: one 480×480 entry is 691 KB, and this
/// device has aborted an allocator over render-path churn once already (ROADMAP 2026-07-28). The
/// full-screen fallback is supplied pre-baked by the shell in normal operation anyway.
pub fn block_cached(c: &mut Canvas, t: &Theme, x0: i32, y0: i32, w: i32, h: i32,
                    name: &str, opacity: f32) {
    let op = opacity.clamp(0.0, 1.0);
    if w != h || w > CACHE_MAX_EDGE || w <= 0 {
        block(c, t, x0, y0, w, h, name, op);
        return;
    }
    use embedded_graphics::prelude::RgbColor;
    let bg = ((t.bg.r() as u32) << 16) | ((t.bg.g() as u32) << 8) | t.bg.b() as u32;
    let key: GradKey = (name_key(name), w, (op * 1000.0) as u16, bg);
    GRAD_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // EVICT THE OLDEST HALF, DON'T EMPTY THE CACHE. This used to `clear()` on overflow, which
        // threw away the entries for the rows CURRENTLY ON SCREEN along with everything else —
        // roughly 14 of them, all of which the very next frame had to bake again. Scrolling steadily
        // past 64 distinct album names triggered that, so it recurred every 64 rows.
        //
        // Each entry carries the tick it was last drawn at, so the rows on screen are by definition
        // the most recently used and survive; what gets dropped is what has scrolled away. The cap
        // is unchanged and is still the contract — this only decides WHICH entries live.
        //
        // The sort is over at most GRAD_CACHE_MAX (64) keys and runs once per 32 inserts, which is
        // nothing against a bake (~85 us for a 48x48).
        if cache.len() >= GRAD_CACHE_MAX && !cache.contains_key(&key) {
            let mut by_age: Vec<(u64, GradKey)> = cache.iter().map(|(k, v)| (v.0, *k)).collect();
            by_age.sort_unstable();
            for (_, k) in by_age.iter().take(GRAD_CACHE_MAX / 2) {
                cache.remove(k);
            }
        }
        let tick = GRAD_TICK.with(|t| {
            let n = t.get() + 1;
            t.set(n);
            n
        });
        let e = cache.entry(key).or_insert_with(|| (tick, gradient_image(t, w, h, name, op)));
        e.0 = tick; // touch: this key is in the visible set as of this frame
        // The opacity is already baked in, so blit at 1.0 — blending again would darken it twice.
        draw_image(c, t, x0, y0, &e.1, 1.0);
    });
}

/// (name hash, edge, opacity ×1000, background) — everything the baked pixels depend on.
type GradKey = (u32, i32, u16, u32);

/// Largest square this will cache. 96×96×3 B is 27 KB; 128 is the ceiling for anything that is
/// drawn per frame on this device. Bigger art is a real cover's job.
const CACHE_MAX_EDGE: i32 = 128;

/// Cap on cached swatches. A 48×48 entry is ~6.9 KB and a 96×96 one ~27 KB, so 64 entries is
/// bounded at a few hundred KB — deliberately modest, for the allocator reason above. ~13 rows are
/// visible at a time; the headroom covers a scroll without thrash.
const GRAD_CACHE_MAX: usize = 64;

thread_local! {
    /// Value is `(last-used tick, baked pixels)` — the tick is what makes eviction keep the
    /// rows that are actually on screen. See `block_cached`.
    static GRAD_CACHE: std::cell::RefCell<std::collections::HashMap<GradKey, (u64, Image)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    /// Monotonic per-thread counter for the above. Wrapping is not a concern: at one tick per
    /// cached swatch per frame it would take longer than the device's battery lasts by a wide
    /// margin, and the only consequence would be one early eviction.
    static GRAD_TICK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many entries the cache is holding. Tests only — the cap is the contract, not the contents.
#[doc(hidden)]
pub fn grad_cache_len() -> usize {
    GRAD_CACHE.with(|c| c.borrow().len())
}

#[doc(hidden)]
pub fn grad_cache_max() -> usize {
    GRAD_CACHE_MAX
}

/// Render the gradient fallback ONCE into an `Image`, so it can be blitted like a real cover
/// instead of recomputed every frame.
///
/// `block` costs real work per pixel (a ramp lookup, a squared-distance test, and a `sqrt` inside
/// the highlight disc). At 480×480 that measured ~8 ms a frame on the host — 98% of what a cover
/// frame cost — and it was paid 20 times a second whenever the visualiser animated, on tracks that
/// simply have no embedded artwork. The pixels are identical; only the timing changes, from every
/// frame to once per track.
///
/// Writes straight into the output buffer. An earlier version rendered into a scratch `Canvas`,
/// which is a 1.5 MB allocation — the exact size whose churn already caused one on-device
/// allocation abort (and therefore a reboot).
pub fn gradient_image(t: &Theme, w: i32, h: i32, name: &str, opacity: f32) -> Image {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let lut = ramp_lut(name);
    let bg = (t.bg.r(), t.bg.g(), t.bg.b());
    let op = opacity.clamp(0.0, 1.0);
    let inv_w = 1.0 / w as f32;
    let inv_h = 1.0 / h as f32;
    let mut row = vec![0u32; w];
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        grad_row(&mut row, &lut, 0, y as f32 * inv_h, inv_w, op, bg);
        for p in &row {
            rgb.push((p >> 16) as u8);
            rgb.push((p >> 8) as u8);
            rgb.push(*p as u8);
        }
    }
    Image { w, h, rgb }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The baked gradient must be pixel-identical to the drawn one. They share `grad_row`, and this
    /// test is what keeps them sharing it: the moment a "fast path" is added to one of them, a
    /// track with no embedded artwork would render differently depending on whether it came from
    /// the cache or the renderer, which is exactly the kind of bug nobody reports precisely.
    #[test]
    fn the_baked_gradient_matches_the_drawn_one_exactly() {
        let t = Theme::day();
        for name in ["kind", "harvest", "Atlas Hands", "", "prism"] {
            for (w, h, op) in [(480, 480, 1.0f32), (92, 92, 0.32), (48, 48, 1.0)] {
                let mut c = Canvas::new();
                block(&mut c, &t, 0, 0, w, h, name, op);
                let img = gradient_image(&t, w, h, name, op);
                assert_eq!(img.w, w as usize);
                assert_eq!(img.h, h as usize);
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let drawn = c.buf[y * crate::canvas::W + x];
                        let o = (y * w as usize + x) * 3;
                        let baked = ((img.rgb[o] as u32) << 16)
                            | ((img.rgb[o + 1] as u32) << 8)
                            | img.rgb[o + 2] as u32;
                        assert_eq!(drawn, baked, "{name} {w}x{h} op{op} differs at ({x},{y})");
                    }
                }
            }
        }
    }

    /// The gradient is a smooth ramp; the 512-entry table must not introduce visible banding.
    /// Adjacent pixels along the diagonal may differ by a colour step, never by a jump.
    #[test]
    fn the_ramp_table_does_not_band() {
        let t = Theme::day();
        let mut c = Canvas::new();
        block(&mut c, &t, 0, 0, 480, 480, "kind", 1.0);
        for y in 0..480usize {
            for x in 1..480usize {
                let a = c.buf[y * crate::canvas::W + x - 1];
                let b = c.buf[y * crate::canvas::W + x];
                for sh in [16, 8, 0] {
                    let d = (((a >> sh) & 0xff) as i32 - ((b >> sh) & 0xff) as i32).abs();
                    assert!(d <= 3, "banding at ({x},{y}): {a:06x} -> {b:06x}");
                }
            }
        }
    }

    /// `row_run` is the new clipping boundary for both blitters — a block drawn partly off the left
    /// or right edge must clip, not wrap onto the next row or panic.
    #[test]
    fn blocks_clip_at_the_canvas_edges() {
        let t = Theme::day();
        let mut c = Canvas::new();
        block(&mut c, &t, -40, 10, 100, 20, "kind", 1.0); // hangs off the left
        block(&mut c, &t, 440, 40, 100, 20, "kind", 1.0); // hangs off the right
        block(&mut c, &t, -500, 70, 100, 20, "kind", 1.0); // entirely off-screen
        block(&mut c, &t, 0, -30, 100, 20, "kind", 1.0); // entirely above
        // Column 479 belongs to the right-hand block's rows only; column 0 to the left one's.
        assert_ne!(c.buf[10 * crate::canvas::W], 0, "left-clipped block drew nothing");
        assert_ne!(c.buf[40 * crate::canvas::W + 479], 0, "right-clipped block drew nothing");
        // Nothing may have leaked onto the row used by the fully off-screen draws.
        for x in 0..crate::canvas::W {
            assert_eq!(c.buf[70 * crate::canvas::W + x], 0, "off-screen block painted row 70");
        }
    }

    /// THE NIGHT DIM HAS TO REACH THE PIXELS. Scaling the palette alone left album art at full
    /// brightness — reported directly: the UI went dark at the dimmest night step and the cover
    /// stayed blazing. Art is decoded image data and owes the theme nothing, so it only dims if
    /// `draw_image` applies the factor itself.
    #[test]
    fn album_art_honours_the_night_dim() {
        let img = Image { w: 2, h: 1, rgb: vec![0xff, 0x80, 0x40, 0xff, 0x80, 0x40] };

        // Full brightness: untouched, byte for byte.
        let mut c = Canvas::new();
        draw_image(&mut c, &Theme::day(), 0, 0, &img, 1.0);
        assert_eq!(c.buf[0], 0xff8040, "the day path must not alter a single pixel");

        // Dimmed: every channel scaled by the same factor, so the image keeps its own balance.
        let dim = Theme::night().scaled(50);
        let mut c2 = Canvas::new();
        draw_image(&mut c2, &dim, 0, 0, &img, 1.0);
        let px = c2.buf[0];
        let (r, g, b) = ((px >> 16) & 0xff, (px >> 8) & 0xff, px & 0xff);
        assert_eq!((r, g, b), (0x7f, 0x40, 0x20), "art was not scaled by the night dim");
        assert!(r > g && g > b, "scaling changed the image's colour balance");

        // And the dimmest rung really is dimmer than the brightest one.
        let mut c3 = Canvas::new();
        draw_image(&mut c3, &Theme::night().scaled(Theme::NIGHT_LEVEL_PCT[0]), 0, 0, &img, 1.0);
        assert!((c3.buf[0] >> 16) & 0xff < r, "the lowest night step is not the dimmest");
    }

    /// The accent swatches are raw palette entries, not theme colours — the same bypass as art.
    #[test]
    fn scale_color_dims_raw_palette_colours() {
        use embedded_graphics::pixelcolor::RgbColor;
        let full = Theme::night();
        let dim = Theme::night().scaled(25);
        let c = embedded_graphics::pixelcolor::Rgb888::new(200, 100, 40);
        assert_eq!(full.scale_color(c), c, "no dim at 100% must be a no-op");
        let d = dim.scale_color(c);
        assert_eq!((d.r(), d.g(), d.b()), (50, 25, 10));
    }

    /// An image blitted through the rewritten row-wise path must land exactly where the old
    /// per-pixel one did, including when it hangs off an edge.    /// An image blitted through the rewritten row-wise path must land exactly where the old
    /// per-pixel one did, including when it hangs off an edge.
    #[test]
    fn draw_image_places_and_clips_correctly() {
        let t = Theme::day();
        let mut c = Canvas::new();
        let img = Image { w: 4, h: 2, rgb: vec![0x11, 0x22, 0x33].repeat(8) };
        draw_image(&mut c, &t, 2, 5, &img, 1.0);
        assert_eq!(c.buf[5 * crate::canvas::W + 2], 0x112233);
        assert_eq!(c.buf[5 * crate::canvas::W + 5], 0x112233);
        assert_eq!(c.buf[5 * crate::canvas::W + 1], 0, "painted left of x0");
        assert_eq!(c.buf[5 * crate::canvas::W + 6], 0, "painted right of the image");
        // Hanging off the left: only the visible columns land, and on the right row.
        draw_image(&mut c, &t, -2, 20, &img, 1.0);
        assert_eq!(c.buf[20 * crate::canvas::W], 0x112233);
        assert_eq!(c.buf[20 * crate::canvas::W + 2], 0, "clipped image drew too wide");
        assert_eq!(c.buf[19 * crate::canvas::W + 479], 0, "wrapped onto the previous row");
    }
}

#[cfg(test)]
mod scale_tests {
    use super::Image;

    /// A checkerboard reduced to one pixel must be the MEAN of the board (mid grey). Bilinear
    /// fails this — it samples four neighbouring pixels near one corner and returns whatever
    /// happens to be there, which is the aliasing that made thumbnails look pixelated.
    #[test]
    fn shrinking_averages_the_whole_source() {
        let n = 64;
        let mut rgb = Vec::with_capacity(n * n * 3);
        for y in 0..n {
            for x in 0..n {
                let v = if (x + y) % 2 == 0 { 0u8 } else { 255u8 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        let one = Image { w: n, h: n, rgb }.scaled_to(1, 1);
        assert_eq!((one.w, one.h), (1, 1));
        for ch in 0..3 {
            assert!(
                (one.rgb[ch] as i32 - 127).abs() <= 1,
                "checkerboard averaged to {} — the scaler is sampling, not averaging",
                one.rgb[ch]
            );
        }
    }

    /// Every source pixel must contribute: a single bright pixel in a dark field survives a big
    /// reduction as a small but non-zero lift. Bilinear drops it entirely unless it lands on a
    /// sample point.
    #[test]
    fn no_source_pixel_is_skipped() {
        let n = 60;
        let mut rgb = vec![0u8; n * n * 3];
        let (px, py) = (37, 11); // deliberately not on a 1/4 sample point
        let i = (py * n + px) * 3;
        rgb[i] = 255;
        rgb[i + 1] = 255;
        rgb[i + 2] = 255;
        let small = Image { w: n, h: n, rgb }.scaled_to(4, 4);
        let total: u32 = small.rgb.iter().map(|&v| v as u32).sum();
        assert!(total > 0, "the bright pixel vanished — output is all black");
    }

    /// Upscaling still goes through bilinear, and an even magnification of a flat image is flat.
    #[test]
    fn upscaling_still_works() {
        let img = Image { w: 2, h: 2, rgb: vec![10, 20, 30].repeat(4) };
        let big = img.scaled_to(8, 8);
        assert_eq!((big.w, big.h), (8, 8));
        assert_eq!(&big.rgb[0..3], &[10, 20, 30]);
    }
}
