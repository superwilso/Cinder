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

/// The main window as the shell scales it: 275×116 across the 800 px of the panel turned on its
/// side (×2.909), which is how `winamp.rs` draws it.
pub const MAIN_W: usize = 800;
pub const MAIN_H: usize = 337;
/// The scale from skin pixels to `MAIN_*`, for placing sprites over the main window.
pub const MAIN_SCALE: f32 = MAIN_W as f32 / 275.0;

/// The decoded sheets. Every one but `main` is optional: a skin that lacks one falls back to
/// Cinder's own drawing for that part.
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
    /// shufrep.bmp: repeat (0,y, 28×15) and shuffle (28,y, 47×15); y 0 off, y 30 on. Also the
    /// EQ (0,61, 23×12) and PL (23,61) buttons.
    pub shufrep: Option<Image>,
    /// volume.bmp and balance.bmp: 28 backgrounds 15 px apart, the thumb at (15,422, 14×11).
    pub volume: Option<Image>,
    pub balance: Option<Image>,
    /// playpaus.bmp: the play (0,0) and pause (9,0) indicators, 9×9.
    pub playpaus: Option<Image>,
    /// monoster.bmp: stereo (0,y, 29×12) and mono (29,y, 27×12); y 0 lit, y 12 dark.
    pub monoster: Option<Image>,
    /// pledit.bmp: the playlist's title bar and side borders.
    pub pledit: Option<Image>,
    /// pledit.txt: the playlist's text, current text, background and current-row colours.
    pub pl_colors: [u32; 4],
    /// viscolor.txt: 24 colours; 2..=17 are the spectrum from top to bottom.
    pub viscolor: [u32; 24],
    /// region.txt's mask: the window's pixels outside every polygon, as `(y, x0, x1)` runs in skin
    /// pixels, which draw black. Empty: the whole window shows.
    pub region: Vec<(i32, i32, i32)>,
    /// The file name, for the line that says which skin is in use.
    pub name: String,
}

/// cbuttons.bmp sprites: (x, width), all 18 high.
pub const CB_PREV: (i32, i32) = (0, 23);
pub const CB_PLAY: (i32, i32) = (23, 23);
pub const CB_PAUSE: (i32, i32) = (46, 23);
pub const CB_NEXT: (i32, i32) = (92, 22);

/// Winamp's own colours, for a skin without pledit.txt or viscolor.txt.
pub const PL_DEFAULT: [u32; 4] = [0x00ff00, 0xffffff, 0x000000, 0x0000c6];
pub const VIS_DEFAULT: [u32; 24] = [
    0x000000, 0x181821, 0xef3110, 0xce2910, 0xd65a00, 0xd66600, 0xd67300, 0xc67b08, 0xdea518,
    0xd6b521, 0xbdde29, 0x94de21, 0x29ce10, 0x32be10, 0x39b510, 0x319c08, 0x298c00, 0x188408,
    0x969696, 0xd6d6de, 0xadadb5, 0xbdbdc6, 0xd6d6de, 0xc8c8c8,
];

impl Default for Skin {
    fn default() -> Self {
        Skin {
            main: None,
            titlebar: None,
            cbuttons: None,
            numbers: None,
            text: None,
            posbar: None,
            shufrep: None,
            volume: None,
            balance: None,
            playpaus: None,
            monoster: None,
            pledit: None,
            pl_colors: PL_DEFAULT,
            viscolor: VIS_DEFAULT,
            region: Vec::new(),
            name: String::new(),
        }
    }
}

/// `#RRGGBB` (or `RRGGBB`) as a colour.
pub fn hex_color(v: &str) -> Option<u32> {
    let h = v.trim().trim_start_matches('#');
    (h.len() >= 6).then(|| u32::from_str_radix(&h[..6], 16).ok())?
}

/// pledit.txt's `Normal`, `Current`, `NormalBG` and `SelectedBG`; Winamp's own for any it lacks.
pub fn parse_pledit(txt: &str) -> [u32; 4] {
    let mut out = PL_DEFAULT;
    for line in txt.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let i = match k.trim().to_ascii_lowercase().as_str() {
            "normal" => 0,
            "current" => 1,
            "normalbg" => 2,
            "selectedbg" => 3,
            _ => continue,
        };
        if let Some(c) = hex_color(v) {
            out[i] = c;
        }
    }
    out
}

/// viscolor.txt: one `r,g,b` per line (a comment may follow); Winamp's own for missing lines.
pub fn parse_viscolor(txt: &str) -> [u32; 24] {
    let mut out = VIS_DEFAULT;
    let rows = txt.lines().filter_map(|l| {
        let n: Vec<u32> = l
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .take(3)
            .filter_map(|p| p.parse().ok())
            .collect();
        (n.len() == 3).then(|| n[0].min(255) << 16 | n[1].min(255) << 8 | n[2].min(255))
    });
    for (o, c) in out.iter_mut().zip(rows) {
        *o = c;
    }
    out
}

/// region.txt: the first `[Normal]` section's polygons (`NumPoints` counts, `PointList` x,y pairs
/// on the pixel grid's corners), as the runs of the 275×116 window outside all of them. A pixel is
/// inside when its centre is (even-odd within a polygon, any polygon counts). Comments (`;`) are
/// skipped; a section that does not add up is no mask at all.
pub fn parse_region(txt: &str) -> Vec<(i32, i32, i32)> {
    let nums = |v: &str| -> Vec<i32> {
        v.split(|ch: char| !(ch.is_ascii_digit() || ch == '-'))
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let (mut section, mut counts, mut points) = (String::new(), None, None);
    for line in txt.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if counts.is_some() && points.is_some() {
                break;
            }
            section = name.trim().to_ascii_lowercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if section != "normal" {
            continue;
        }
        match k.trim().to_ascii_lowercase().as_str() {
            "numpoints" if counts.is_none() => counts = Some(nums(v)),
            "pointlist" if points.is_none() => points = Some(nums(v)),
            _ => {}
        }
    }
    let (Some(counts), Some(points)) = (counts, points) else {
        return Vec::new();
    };
    let total: i32 = counts.iter().sum();
    if counts.iter().any(|&n| n < 3) || total * 2 != points.len() as i32 {
        return Vec::new();
    }
    let mut polys: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut at = 0;
    for &n in &counts {
        let n = n as usize;
        polys.push(
            points[at..at + 2 * n]
                .chunks(2)
                .map(|p| (p[0] as f32, p[1] as f32))
                .collect(),
        );
        at += 2 * n;
    }
    let inside = |px: f32, py: f32| {
        polys.iter().any(|poly| {
            let mut odd = false;
            let mut j = poly.len() - 1;
            for i in 0..poly.len() {
                let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
                if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                    odd = !odd;
                }
                j = i;
            }
            odd
        })
    };
    let mut out = Vec::new();
    for y in 0..116 {
        let mut run: Option<i32> = None;
        for x in 0..=275 {
            let hole = x < 275 && !inside(x as f32 + 0.5, y as f32 + 0.5);
            match (hole, run) {
                (true, None) => run = Some(x),
                (false, Some(x0)) => {
                    out.push((y, x0, x));
                    run = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// Draw `src` (x, y, w, h) of `img` into `dst` (x, y, w, h), nearest neighbour, dimmed at night
/// like the cover. Pixels outside the sheet are skipped, so a short sheet cannot panic.
pub fn blit(
    c: &mut Canvas,
    dim_pct: u32,
    img: &Image,
    src: (i32, i32, i32, i32),
    dst: (i32, i32, i32, i32),
) {
    blit_to(c, dim_pct, img, src, dst, false, None);
}

/// `blit`, with `land`: `dst` is on the panel turned on its side (`winamp::to_portrait`), and
/// `key`: pixels of colour `key.0` are left out and the rest drawn in `key.1` (text in a colour).
pub fn blit_to(
    c: &mut Canvas,
    dim_pct: u32,
    img: &Image,
    (sx, sy, sw, sh): (i32, i32, i32, i32),
    (dx, dy, dw, dh): (i32, i32, i32, i32),
    land: bool,
    key: Option<(u32, u32)>,
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
            let mut px = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
            if let Some((k, fg)) = key {
                if px == k {
                    continue;
                }
                px = fg;
            }
            let ch = |s: u32| (((px >> s) & 0xff) * dim / 100) << s;
            let col = ch(16) | ch(8) | ch(0);
            if land {
                c.put(crate::canvas::W as i32 - 1 - (dy + y), dx + x, col);
            } else {
                c.put(dx + x, dy + y, col);
            }
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
