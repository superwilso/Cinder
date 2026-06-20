//! Cinder design tokens (RUST-HANDOFF.md §1.1/§1.2). Two themes: Day + Night.

use embedded_graphics::pixelcolor::Rgb888;

#[inline]
fn rgb(hex: u32) -> Rgb888 {
    Rgb888::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub struct Theme {
    pub bg: Rgb888,
    pub panel: Rgb888,
    pub line: Rgb888,
    pub ink: Rgb888,
    pub dim: Rgb888,
    pub faint: Rgb888,
    pub acc: Rgb888,
    pub acc_ink: Rgb888,
    pub night: bool,
}

impl Theme {
    pub fn day() -> Self {
        Theme {
            bg: rgb(0x0d0c0b),
            panel: rgb(0x13110f),
            line: rgb(0x221f1b),
            ink: rgb(0xece7df),
            dim: rgb(0x95908a),
            faint: rgb(0x5f5a52),
            acc: rgb(0xf4651f),
            acc_ink: rgb(0x1a0a02),
            night: false,
        }
    }

    pub fn night() -> Self {
        Theme {
            bg: rgb(0x000000),
            panel: rgb(0x0a0908),
            line: rgb(0x161310),
            ink: rgb(0x8d8170),
            dim: rgb(0x5b5347),
            faint: rgb(0x3b362d),
            acc: rgb(0x863810),
            acc_ink: rgb(0x000000),
            night: true,
        }
    }
}
