//! Winamp classic skins (`.wsz`) for the Winamp Now Playing style.
//!
//! A skin is a zip of BMP sheets; the shell finds one at the top of the drive, decodes it
//! (`cinder-ffi/src/wsz.rs`) and hands the sheets here. Nothing of any skin ships with Cinder: with
//! no `.wsz` on the drive the style says so and draws as Terminal.
//!
//! The positions below are the classic skin format's public layout (the sheets' sizes and where
//! each sprite sits), as every skin editor documents it. Sprites are drawn with nearest-neighbour
//! scaling, so the pixel art stays sharp at the sizes a 480 px panel needs; the main window is
//! scaled once, when the skin is loaded, because it is the one large sheet.

use crate::art::Image;
use crate::Canvas;

/// The main window as the shell scales it: 275×116 at 408 px wide (×1.4836).
pub const MAIN_W: usize = 408;
pub const MAIN_H: usize = 172;
/// The scale from skin pixels to `MAIN_*`, for placing sprites over the main window.
pub const MAIN_SCALE: f32 = MAIN_W as f32 / 275.0;

/// The decoded sheets. Every one but `main` is optional: a skin that lacks one falls back to
/// Cinder's own drawing for that part.
#[derive(Default)]
pub struct Skin {
    /// main.bmp, already scaled to MAIN_W × MAIN_H.
    pub main: Option<Image>,
    /// titlebar.bmp (the active bar is at 27,0, 275×14).
    pub titlebar: Option<Image>,
    /// cbuttons.bmp: prev, play, pause, stop, next across the top row (y 0), pressed below (y 18).
    pub cbuttons: Option<Image>,
    /// numbers.bmp: digits 0–9, 9×13 each, left to right.
    pub numbers: Option<Image>,
    /// text.bmp: a 5×6 font in three rows.
    pub text: Option<Image>,
    /// posbar.bmp: the seek bar (0,0, 248×10) and its thumb (248,0, 29×10).
    pub posbar: Option<Image>,
    /// shufrep.bmp: repeat (0,y, 28×15) and shuffle (28,y, 47×15); y 0 off, y 30 on.
    pub shufrep: Option<Image>,
    /// The file name, for the line that says which skin is in use.
    pub name: String,
}

/// cbuttons.bmp sprites: (x, width), all 18 high.
pub const CB_PREV: (i32, i32) = (0, 23);
pub const CB_PLAY: (i32, i32) = (23, 23);
pub const CB_PAUSE: (i32, i32) = (46, 23);
pub const CB_NEXT: (i32, i32) = (92, 22);

/// Draw `src` (x, y, w, h) of `img` into `dst` (x, y, w, h), nearest neighbour, dimmed at night
/// like the cover. Pixels outside the sheet are skipped, so a short sheet cannot panic.
pub fn blit(
    c: &mut Canvas,
    dim_pct: u32,
    img: &Image,
    (sx, sy, sw, sh): (i32, i32, i32, i32),
    (dx, dy, dw, dh): (i32, i32, i32, i32),
) {
    if sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0 {
        return;
    }
    let dim = dim_pct.min(100);
    for y in 0..dh {
        let iy = sy + y * sh / dh;
        if iy < 0 || iy as usize >= img.h {
            continue;
        }
        for x in 0..dw {
            let ix = sx + x * sw / dw;
            if ix < 0 || ix as usize >= img.w {
                continue;
            }
            let o = (iy as usize * img.w + ix as usize) * 3;
            let Some(p) = img.rgb.get(o..o + 3) else {
                continue;
            };
            let ch = |v: u8| (v as u32 * dim / 100) & 0xff;
            c.put(dx + x, dy + y, ch(p[0]) << 16 | ch(p[1]) << 8 | ch(p[2]));
        }
    }
}

/// Where `ch` sits in text.bmp, in 5×6 cells: (column, row). Unknown characters are a space.
pub fn text_cell(ch: char) -> (i32, i32) {
    const ROW1: &str = "0123456789\u{2026}.:()-'!_+\\/[]^&%,=$#";
    let up = ch.to_ascii_uppercase();
    if up.is_ascii_uppercase() {
        return (up as i32 - 'A' as i32, 0);
    }
    match ch {
        '"' => (26, 0),
        '@' => (27, 0),
        '\u{c5}' | '\u{e5}' => (0, 2),
        '\u{d6}' | '\u{f6}' => (1, 2),
        '\u{c4}' | '\u{e4}' => (2, 2),
        '?' => (3, 2),
        '*' => (4, 2),
        _ => ROW1
            .chars()
            .position(|r| r == ch)
            .map_or((30, 0), |i| (i as i32, 1)),
    }
}

/// `s` in the skin's font at `scale`× from (x, y), clipped to `max_w` px. Returns the width drawn.
pub fn draw_text(
    c: &mut Canvas,
    dim_pct: u32,
    font: &Image,
    s: &str,
    (x, y): (i32, i32),
    scale: i32,
    max_w: i32,
) -> i32 {
    let mut cx = x;
    for ch in s.chars() {
        if cx + 5 * scale > x + max_w {
            break;
        }
        let (col, row) = text_cell(ch);
        blit(
            c,
            dim_pct,
            font,
            (col * 5, row * 6, 5, 6),
            (cx, y, 5 * scale, 6 * scale),
        );
        cx += 5 * scale;
    }
    cx - x
}

/// One digit of numbers.bmp (9×13) at (x, y), `w`×`h` on the panel.
pub fn draw_digit(
    c: &mut Canvas,
    dim_pct: u32,
    nums: &Image,
    d: u32,
    (x, y, w, h): (i32, i32, i32, i32),
) {
    blit(
        c,
        dim_pct,
        nums,
        (d.min(9) as i32 * 9, 0, 9, 13),
        (x, y, w, h),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_map_covers_letters_digits_and_falls_back_to_space() {
        assert_eq!(text_cell('a'), (0, 0));
        assert_eq!(text_cell('Z'), (25, 0));
        assert_eq!(text_cell('0'), (0, 1));
        assert_eq!(text_cell(':'), (12, 1));
        assert_eq!(text_cell('-'), (15, 1));
        assert_eq!(text_cell('?'), (3, 2));
        assert_eq!(text_cell('\u{4e2d}'), (30, 0), "anything else is a space");
    }

    #[test]
    fn blit_scales_and_never_reads_past_the_sheet() {
        let img = Image {
            w: 2,
            h: 1,
            rgb: vec![255, 0, 0, 0, 0, 255],
        };
        let mut c = Canvas::new();
        blit(&mut c, 100, &img, (0, 0, 2, 1), (10, 10, 4, 2));
        let px = |c: &Canvas, x: usize, y: usize| c.buf[y * crate::canvas::W + x];
        assert_eq!(px(&c, 10, 10), 0xff0000);
        assert_eq!(px(&c, 13, 11), 0x0000ff);
        // A source rect past the sheet draws what exists and nothing else.
        blit(&mut c, 100, &img, (1, 0, 4, 4), (100, 100, 8, 8));
        assert_eq!(px(&c, 100, 100), 0x0000ff);
    }
}
