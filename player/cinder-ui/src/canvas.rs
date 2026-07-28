//! A 480x800 XRGB8888 software canvas. Stores `u32` pixels as `0x00RRGGBB`,
//! matching the device framebuffer exactly so the device backend is a memcpy.
//! Implements `embedded-graphics` `DrawTarget` so primitives draw onto it, and
//! exposes `blend()` for the fontdue text path.

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

pub const W: usize = 480;
pub const H: usize = 800;

pub struct Canvas {
    pub buf: Vec<u32>, // W*H, 0x00RRGGBB
    /// Vertical clip band [clip_top, clip_bot) enforced by every pixel write. Lists set this
    /// around their scroll area so pixel-offset (partially visible) rows can't paint over the
    /// chrome above or below; everything else draws with the full-screen default.
    clip_top: i32,
    clip_bot: i32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self { buf: vec![0; W * H], clip_top: 0, clip_bot: H as i32 }
    }

    /// Restrict drawing to rows `top..bottom` (screen coords). Pair with `clear_clip`.
    pub fn set_clip_y(&mut self, top: i32, bottom: i32) {
        self.clip_top = top.clamp(0, H as i32);
        self.clip_bot = bottom.clamp(self.clip_top, H as i32);
    }

    pub fn clear_clip(&mut self) {
        self.clip_top = 0;
        self.clip_bot = H as i32;
    }

    pub fn fill(&mut self, c: Rgb888) {
        self.buf.fill(to_u32(c));
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, v: u32) {
        if x >= 0 && y >= self.clip_top && y < self.clip_bot && (x as usize) < W {
            self.buf[y as usize * W + x as usize] = v;
        }
    }

    /// Alpha-blend `c` over the existing pixel with coverage `a` (0..=255).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb888, a: u8) {
        if x < 0 || y < self.clip_top || y >= self.clip_bot || x as usize >= W {
            return;
        }
        let idx = y as usize * W + x as usize;
        let dst = self.buf[idx];
        let (a, ia) = (a as u32, 255 - a as u32);
        let dr = (dst >> 16) & 0xff;
        let dg = (dst >> 8) & 0xff;
        let db = dst & 0xff;
        let r = div255(dr * ia + c.r() as u32 * a);
        let g = div255(dg * ia + c.g() as u32 * a);
        let b = div255(db * ia + c.b() as u32 * a);
        self.buf[idx] = (r << 16) | (g << 8) | b;
    }

    /// A writable, clipped run of ONE row: the destination slice, plus how many leading source
    /// pixels fell off the left edge so the caller can advance its own pointer.
    ///
    /// This exists so blitters can pay the clip test once per row instead of once per pixel.
    /// `put` is correct but it re-checks four bounds and recomputes an index for every pixel, and
    /// a full-bleed 480x480 cover is 230,400 of them — measured at ~1 ms/frame on the host, which
    /// is most of what a Now Playing frame costs, redrawn 20x a second while the visualiser runs.
    pub fn row_run(&mut self, y: i32, x: i32, len: usize) -> Option<(usize, &mut [u32])> {
        if y < self.clip_top || y >= self.clip_bot || len == 0 {
            return None;
        }
        let x1 = x + len as i32;
        if x1 <= 0 || x >= W as i32 {
            return None;
        }
        let skip = if x < 0 { (-x) as usize } else { 0 };
        let dx0 = x.max(0) as usize;
        let dx1 = x1.min(W as i32) as usize;
        if dx1 <= dx0 {
            return None;
        }
        let row = y as usize * W;
        Some((skip, &mut self.buf[row + dx0..row + dx1]))
    }

    /// RGB byte triples for PNG export (host backend).
    pub fn to_rgb_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(W * H * 3);
        for &p in &self.buf {
            v.push((p >> 16) as u8);
            v.push((p >> 8) as u8);
            v.push(p as u8);
        }
        v
    }
}

/// Rounded `x / 255` without a division, exact for the 0..=65535 range alpha blending produces.
///
/// This matters far more here than it looks: the device is an ARMv7-A core with no hardware
/// integer divide, so `/ 255` compiles to a `__aeabi_uidiv` CALL. `blend` runs once per glyph
/// pixel — order 10^5 times a frame while a text list scrolls — and was paying three of them.
#[inline]
fn div255(x: u32) -> u32 {
    let t = x + 128;
    (t + (t >> 8)) >> 8
}

#[inline]
pub fn to_u32(c: Rgb888) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

impl DrawTarget for Canvas {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            self.put(p.x, p.y, to_u32(c));
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let v = to_u32(color);
        let x0 = area.top_left.x.max(0);
        let y0 = area.top_left.y.max(self.clip_top);
        let x1 = (area.top_left.x + area.size.width as i32).min(W as i32);
        let y1 = (area.top_left.y + area.size.height as i32).min(self.clip_bot);
        // Row-at-a-time slice fill, not pixel-at-a-time: this is the single hottest primitive in
        // the UI (every row background, separator, band and panel goes through it), and the
        // per-pixel form paid an index calculation and a bounds check for each of ~400k pixels a
        // frame. `[T]::fill` on a slice compiles to a memset the pixel loop can't become.
        if x1 > x0 {
            for y in y0..y1 {
                let row = y as usize * W;
                self.buf[row + x0 as usize..row + x1 as usize].fill(v);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::div255;

    /// The shift form must agree with real division across everything blending can produce,
    /// otherwise text picks up a colour cast that no test would otherwise catch.
    #[test]
    fn div255_matches_division() {
        for x in 0..=65535u32 {
            assert_eq!(div255(x), (x + 127) / 255, "x={x}");
        }
    }
}
