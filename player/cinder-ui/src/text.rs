//! Text engine: fontdue rasteriser + bundled OFL fonts (Hanken Grotesk sans,
//! JetBrains Mono mono). embedded-graphics' built-in fonts are bitmap-only, so
//! we rasterise glyph coverage with fontdue and alpha-blend onto the Canvas.
//! `tracking` is letter-spacing in em (multiplied by px size), per the design.

use crate::canvas::Canvas;
use embedded_graphics::pixelcolor::Rgb888;
use fontdue::{Font, FontSettings};

#[derive(Clone, Copy, PartialEq)]
pub enum Family {
    Sans,
    Mono,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Weight {
    Light,
    Regular,
    SemiBold,
    Bold,
    ExtraBold,
}

pub struct FontSet {
    sans_regular: Font,
    sans_semibold: Font,
    sans_bold: Font,
    sans_extrabold: Font,
    mono_light: Font,
    mono_regular: Font,
    mono_bold: Font,
}

impl FontSet {
    pub fn load() -> Self {
        let f = |b: &[u8]| Font::from_bytes(b, FontSettings::default()).expect("font parse");
        FontSet {
            sans_regular: f(include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf")),
            sans_semibold: f(include_bytes!("../assets/fonts/HankenGrotesk-SemiBold.ttf")),
            sans_bold: f(include_bytes!("../assets/fonts/HankenGrotesk-Bold.ttf")),
            sans_extrabold: f(include_bytes!("../assets/fonts/HankenGrotesk-ExtraBold.ttf")),
            mono_light: f(include_bytes!("../assets/fonts/JetBrainsMono-Light.ttf")),
            mono_regular: f(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
            mono_bold: f(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
        }
    }

    fn pick(&self, fam: Family, w: Weight) -> &Font {
        match fam {
            Family::Sans => match w {
                Weight::Light | Weight::Regular => &self.sans_regular,
                Weight::SemiBold => &self.sans_semibold,
                Weight::Bold => &self.sans_bold,
                Weight::ExtraBold => &self.sans_extrabold,
            },
            Family::Mono => match w {
                Weight::Light => &self.mono_light,
                Weight::Regular | Weight::SemiBold => &self.mono_regular,
                Weight::Bold | Weight::ExtraBold => &self.mono_bold,
            },
        }
    }
}

pub struct TextStyle {
    pub fam: Family,
    pub weight: Weight,
    pub size: f32,
    pub color: Rgb888,
    pub tracking: f32, // em
}

/// Pixel width of `s` rendered with `st` (advances + tracking).
pub fn measure(fonts: &FontSet, s: &str, st: &TextStyle) -> f32 {
    let font = fonts.pick(st.fam, st.weight);
    let track = st.tracking * st.size;
    s.chars()
        .map(|ch| font.metrics(ch, st.size).advance_width + track)
        .sum()
}

/// Draw `s` with its baseline at (`x`, `baseline`). Returns the pen x after.
pub fn draw(canvas: &mut Canvas, fonts: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle) -> f32 {
    let font = fonts.pick(st.fam, st.weight);
    let track = st.tracking * st.size;
    let mut pen = x;
    for ch in s.chars() {
        let (m, bitmap) = font.rasterize(ch, st.size);
        let gx0 = (pen + m.xmin as f32).round() as i32;
        // fontdue: bitmap top is `ymin + height` above the baseline.
        let gy0 = (baseline - (m.height as f32 + m.ymin as f32)).round() as i32;
        for gy in 0..m.height {
            for gx in 0..m.width {
                let a = bitmap[gy * m.width + gx];
                if a > 0 {
                    canvas.blend(gx0 + gx as i32, gy0 + gy as i32, st.color, a);
                }
            }
        }
        pen += m.advance_width + track;
    }
    pen
}
