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
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

impl Image {
    /// Bilinear-resample into a new dw×dh image (called once per track change, not per frame).
    pub fn scaled_to(&self, dw: usize, dh: usize) -> Image {
        let mut out = vec![0u8; dw * dh * 3];
        if self.w == 0 || self.h == 0 || dw == 0 || dh == 0 {
            return Image { w: dw, h: dh, rgb: out };
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
}

/// Blit a pre-scaled cover at (x0,y0). `opacity` blends toward `t.bg` exactly like `block`
/// (night thumbs sit back at 0.32). The image is drawn 1:1 — scale at decode time.
pub fn draw_image(c: &mut Canvas, t: &Theme, x0: i32, y0: i32, img: &Image, opacity: f32) {
    let (br, bg, bb) = (t.bg.r(), t.bg.g(), t.bg.b());
    let op = opacity.clamp(0.0, 1.0);
    for yy in 0..img.h {
        let row = yy * img.w * 3;
        for xx in 0..img.w {
            let o = row + xx * 3;
            let (mut r, mut g, mut b) = (img.rgb[o], img.rgb[o + 1], img.rgb[o + 2]);
            if op < 1.0 {
                r = lerp(br, r, op);
                g = lerp(bg, g, op);
                b = lerp(bb, b, op);
            }
            c.put(x0 + xx as i32, y0 + yy as i32, ((r as u32) << 16) | ((g as u32) << 8) | b as u32);
        }
    }
}

/// Draw a gradient art block at (x0,y0) sized w×h. `opacity` (0..1) blends the
/// gradient toward `t.bg` so night-dimmed art and small thumbnails sit back.
pub fn block(c: &mut Canvas, t: &Theme, x0: i32, y0: i32, w: i32, h: i32, name: &str, opacity: f32) {
    let s = stops(name);
    let (br, bg, bb) = (t.bg.r(), t.bg.g(), t.bg.b());
    let op = opacity.clamp(0.0, 1.0);
    for yy in 0..h {
        for xx in 0..w {
            // 135deg ≈ top-left → bottom-right
            let pos = ((xx as f32 / w as f32) + (yy as f32 / h as f32)) * 0.5;
            let (mut r, mut g, mut b) = if pos <= s[1].0 {
                let u = (pos - s[0].0) / (s[1].0 - s[0].0).max(1e-3);
                (lerp(s[0].1 .0, s[1].1 .0, u), lerp(s[0].1 .1, s[1].1 .1, u), lerp(s[0].1 .2, s[1].1 .2, u))
            } else {
                let u = (pos - s[1].0) / (s[2].0 - s[1].0).max(1e-3);
                (lerp(s[1].1 .0, s[2].1 .0, u), lerp(s[1].1 .1, s[2].1 .1, u), lerp(s[1].1 .2, s[2].1 .2, u))
            };
            // soft radial highlight near top-left (the .art ::after overlay)
            let dx = xx as f32 / w as f32 - 0.2;
            let dy = yy as f32 / h as f32 - 0.1;
            let hl = (1.0 - (dx * dx + dy * dy).sqrt() / 0.65).clamp(0.0, 1.0) * 0.16;
            r = lerp(r, 255, hl);
            g = lerp(g, 255, hl);
            b = lerp(b, 255, hl);
            // blend toward bg by (1 - opacity)
            if op < 1.0 {
                r = lerp(br, r, op);
                g = lerp(bg, g, op);
                b = lerp(bb, b, op);
            }
            c.put(x0 + xx, y0 + yy, ((r as u32) << 16) | ((g as u32) << 8) | b as u32);
        }
    }
}
