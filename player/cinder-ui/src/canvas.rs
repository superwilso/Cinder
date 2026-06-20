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
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self { buf: vec![0; W * H] }
    }

    pub fn fill(&mut self, c: Rgb888) {
        let v = to_u32(c);
        self.buf.iter_mut().for_each(|p| *p = v);
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, v: u32) {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.buf[y as usize * W + x as usize] = v;
        }
    }

    /// Alpha-blend `c` over the existing pixel with coverage `a` (0..=255).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb888, a: u8) {
        if x < 0 || y < 0 || x as usize >= W || y as usize >= H {
            return;
        }
        let idx = y as usize * W + x as usize;
        let dst = self.buf[idx];
        let (a, ia) = (a as u32, 255 - a as u32);
        let dr = (dst >> 16) & 0xff;
        let dg = (dst >> 8) & 0xff;
        let db = dst & 0xff;
        let r = (dr * ia + c.r() as u32 * a + 127) / 255;
        let g = (dg * ia + c.g() as u32 * a + 127) / 255;
        let b = (db * ia + c.b() as u32 * a + 127) / 255;
        self.buf[idx] = (r << 16) | (g << 8) | b;
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
        let y0 = area.top_left.y.max(0);
        let x1 = (area.top_left.x + area.size.width as i32).min(W as i32);
        let y1 = (area.top_left.y + area.size.height as i32).min(H as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                self.buf[y as usize * W + x as usize] = v;
            }
        }
        Ok(())
    }
}
