//! The Winamp style with a skin on the drive: Now Playing on its side and across the whole panel,
//! laid out as Wampy lays it out. The skin's main window fills the top 337 of the 480 px (×2.9,
//! so its buttons are a finger's size), and three rows of the playlist (Up Next) sit under it. PL,
//! or the playlist's title bar, gives the playlist the whole panel, and the same takes it back.
//! Every control in the window works.
//!
//! The panel is turned a quarter anticlockwise, so the player's top is on the left, as in Wampy's
//! photographs. Everything here is in those landscape pixels (`LW` × `LH`). `to_land` turns a
//! touch into them, and `skin::blit_to(.., land)` draws out of them. Positions are the classic skin
//! format's public layout, in skin pixels, scaled by `K`.
//!
//! What each control does. As in Wampy, ⏏ opens the settings and EQ the equaliser. Wampy leaves
//! the title bar's buttons and the clutter bar inert; here they do what Winamp's would, as near as
//! a player with no windows can:
//!
//! | Control | Does |
//! |---|---|
//! | ◁ ▶ ‖ ▷, shuffle, repeat | what they say; ▶ only plays and ‖ only pauses |
//! | ■ | pauses and goes back to 0:00 |
//! | ⏏ | Settings |
//! | minimise, close | Library |
//! | the menu icon (top left) | Menu |
//! | PL, windowshade, the playlist's title bar | the playlist full screen, and back |
//! | EQ | the equaliser |
//! | O A I D V (the clutter bar) | Settings, Display, Track information, the next skin, the visualiser settings |
//! | the time | elapsed ⇄ remaining |
//! | the visualiser | Winamp's cycle: spectrum, oscilloscope, off |
//! | the song title | Track information |
//! | volume, balance, the seek bar | set by a tap or a drag |
//! | a playlist row | plays it |

use crate::art::Image;
use crate::canvas::{Canvas, W};
use crate::model::SongRow;
use crate::now_playing::NowPlaying;
use crate::skin::{blit_to, text_cell, Skin, MAIN_H, MAIN_W};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;

/// The panel on its side.
pub const LW: i32 = crate::canvas::H as i32;
pub const LH: i32 = W as i32;
/// Skin pixels to landscape pixels: the 275 px window across the 800.
const K: f32 = LW as f32 / 275.0;
/// The playlist's title bar (20 skin px) and rows, Wampy's sizes.
const PL_TITLE_H: i32 = 58;
const ROW_H: i32 = 30;
/// A near miss within this many skin px still lands on the control it was aimed at.
const SLOP: f32 = 3.0;

fn k(v: i32) -> i32 {
    (v as f32 * K).round() as i32
}

/// A skin-pixel rect in landscape pixels. Edges are scaled, not sizes, so neighbours still meet.
fn kr((x, y, w, h): (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    (k(x), k(y), k(x + w) - k(x), k(y + h) - k(y))
}

/// A touch at `(px, py)` on the panel, in landscape pixels.
pub fn to_land(px: i32, py: i32) -> (i32, i32) {
    (py, W as i32 - 1 - px)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ctl {
    Menu,
    Minimize,
    Shade,
    Close,
    Options,
    OnTop,
    FileInfo,
    Double,
    VisMenu,
    Time,
    Vis,
    Title,
    Volume,
    Balance,
    Eq,
    Pl,
    Seek,
    Prev,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Shuffle,
    Repeat,
    /// Row `n` of the playlist, counted from the playing track.
    PlRow(usize),
}

/// The window's controls in skin pixels.
const WINDOW: [(Ctl, (i32, i32, i32, i32)); 25] = [
    (Ctl::Menu, (6, 3, 9, 9)),
    (Ctl::Minimize, (244, 3, 9, 9)),
    (Ctl::Shade, (254, 3, 9, 9)),
    (Ctl::Close, (264, 3, 9, 9)),
    (Ctl::Options, (10, 25, 8, 8)),
    (Ctl::OnTop, (10, 33, 8, 8)),
    (Ctl::FileInfo, (10, 41, 8, 8)),
    (Ctl::Double, (10, 49, 8, 8)),
    (Ctl::VisMenu, (10, 57, 8, 8)),
    (Ctl::Time, (36, 26, 63, 13)),
    (Ctl::Vis, (24, 43, 76, 16)),
    (Ctl::Title, (111, 24, 154, 12)),
    (Ctl::Volume, (107, 57, 68, 13)),
    (Ctl::Balance, (177, 57, 38, 13)),
    (Ctl::Eq, (219, 58, 23, 12)),
    (Ctl::Pl, (242, 58, 23, 12)),
    (Ctl::Seek, (16, 72, 248, 10)),
    (Ctl::Prev, (16, 88, 23, 18)),
    (Ctl::Play, (39, 88, 23, 18)),
    (Ctl::Pause, (62, 88, 23, 18)),
    (Ctl::Stop, (85, 88, 23, 18)),
    (Ctl::Next, (108, 88, 22, 18)),
    (Ctl::Eject, (136, 89, 22, 16)),
    (Ctl::Shuffle, (164, 89, 47, 15)),
    (Ctl::Repeat, (210, 89, 28, 15)),
];

/// The window's height, and so where the playlist starts unless it has the panel.
pub const WINDOW_H: i32 = 337;

/// The playlist's top: under the window, or the top of the panel.
fn pl_top(full: bool) -> i32 {
    if full {
        0
    } else {
        WINDOW_H
    }
}

/// How many playlist rows show: three under the window, as in Wampy, or a panelful. A last row
/// the panel cuts short still counts; it is drawn clipped.
pub fn pl_rows(full: bool) -> usize {
    ((LH - pl_top(full) - PL_TITLE_H + ROW_H - 1) / ROW_H) as usize
}

/// The control at `(lx, ly)`: on the window, an exact hit first, else the nearest within `SLOP`;
/// on the playlist, its title bar or a row. `full`: the playlist has the panel.
pub fn ctl_at(lx: i32, ly: i32, full: bool) -> Option<Ctl> {
    let top = pl_top(full);
    if ly >= top {
        if ly < top + PL_TITLE_H {
            return Some(Ctl::Pl);
        }
        let i = ((ly - top - PL_TITLE_H) / ROW_H) as usize;
        return (i < pl_rows(full)).then_some(Ctl::PlRow(i));
    }
    let (sx, sy) = (lx as f32 / K, ly as f32 / K);
    let miss = |(x, y, w, h): (i32, i32, i32, i32)| {
        let dx = (x as f32 - sx).max(sx - (x + w) as f32).max(0.0);
        let dy = (y as f32 - sy).max(sy - (y + h) as f32).max(0.0);
        dx.max(dy)
    };
    WINDOW
        .iter()
        .map(|&(c, r)| (miss(r), c))
        .filter(|(d, _)| *d <= SLOP)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

/// Along a horizontal slider at skin `(x, w)`, 0..=1, for a touch at `lx`.
fn along(lx: i32, x: i32, w: i32) -> f32 {
    ((lx - k(x)) as f32 / (k(x + w) - k(x)).max(1) as f32).clamp(0.0, 1.0)
}

/// The seek bar's position under `lx`, in permille.
pub fn seek_permille(lx: i32) -> u16 {
    (along(lx, 16, 248) * 1000.0).round() as u16
}

/// The volume slider's level under `lx`, 0..=`max`.
pub fn volume_at(lx: i32, max: u8) -> u8 {
    (along(lx, 107, 68) * max as f32).round() as u8
}

/// The balance slider's value under `lx`, 0..=100 with 50 the centre, which it snaps to nearby.
pub fn balance_at(lx: i32) -> usize {
    let v = (along(lx, 177, 38) * 100.0).round() as usize;
    if v.abs_diff(50) <= 8 {
        50
    } else {
        v
    }
}

/// What the view shows that Now Playing's data does not carry.
pub struct View<'a> {
    /// Up Next, and the playing track's place in it.
    pub queue: &'a [SongRow],
    pub at: usize,
    /// The volume the rocker drives, and its top.
    pub volume: (u8, u8),
    /// 0..=100, 50 the centre.
    pub balance: usize,
    /// The time counts down.
    pub remaining: bool,
    /// The playlist has the whole panel.
    pub playlist: bool,
    /// The display's visualiser: 0 spectrum, 1 oscilloscope, 2 off (Winamp's cycle).
    pub vis: u8,
}

impl Default for View<'_> {
    fn default() -> Self {
        View {
            queue: &[],
            at: 0,
            volume: (60, 120),
            balance: 50,
            remaining: false,
            playlist: false,
            vis: 0,
        }
    }
}

fn dimmed(col: u32, dim: u32) -> u32 {
    let ch = |s: u32| (((col >> s) & 0xff) * dim.min(100) / 100) << s;
    ch(16) | ch(8) | ch(0)
}

/// A landscape rect filled.
fn fill(c: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), col: u32) {
    let col = Rgb888::new((col >> 16) as u8, (col >> 8) as u8, col as u8);
    crate::widgets::fill_rect(c, W as i32 - y - h, x, h, w, col);
}

/// `m:ss` or `h:mm:ss` (a leading `-` is ignored) as Winamp's minutes and seconds.
fn mmss(s: &str) -> (u32, u32) {
    let parts: Vec<u32> = s
        .trim_start_matches('-')
        .split(':')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [.., m, sec] => {
            let hours = parts
                .len()
                .checked_sub(3)
                .and_then(|i| parts.get(i))
                .copied();
            ((hours.unwrap_or(0) * 60 + m) % 100, *sec)
        }
        _ => (0, 0),
    }
}

/// The number in front of `unit` in `s` (`"FLAC · 24bit / 96.0 kHz"`, `"kHz"` → `"96"`).
fn number_before<'a>(s: &'a str, unit: &str) -> &'a str {
    let Some(i) = s.find(unit) else {
        return "";
    };
    let head = s[..i].trim_end();
    let start = head
        .rfind(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .map_or(0, |j| j + 1);
    head[start..].split('.').next().unwrap_or("")
}

/// A title too long for the display scrolls, one character a second. The clock is the playing
/// time, so it moves only while the song does and costs no frames of its own.
fn marquee(s: &str, elapsed: &str, room: usize) -> String {
    if s.chars().count() <= room {
        return s.to_string();
    }
    let (m, sec) = mmss(elapsed);
    let looped: Vec<char> = s.chars().chain("  ***  ".chars()).collect();
    let at = (m * 60 + sec) as usize % looped.len();
    looped.iter().cycle().skip(at).take(room).collect()
}

/// `s` in the skin's font, one cell per character from `cell(i)`, up to `max` characters. `key`
/// draws it in a colour: the font's background is left out and the rest drawn as `key.1`.
fn text(
    c: &mut Canvas,
    dim: u32,
    font: &Image,
    s: &str,
    max: usize,
    key: Option<(u32, u32)>,
    cell: impl Fn(i32) -> (i32, i32, i32, i32),
) {
    for (i, ch) in s.chars().take(max).enumerate() {
        let (col, row) = text_cell(ch);
        blit_to(
            c,
            dim,
            font,
            (col * 5, row * 6, 5, 6),
            cell(i as i32),
            true,
            key,
        );
    }
}

/// The font's background colour: the space cell's corner.
fn font_bg(font: &Image) -> u32 {
    let o = 150 * 3;
    font.rgb.get(o..o + 3).map_or(0, |p| {
        (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
    })
}

pub fn render(c: &mut Canvas, t: &Theme, np: &NowPlaying, skin: &Skin, v: &View) {
    let dim = t.dim_pct;
    let idle = np.title.is_empty() && np.artist.is_empty();
    let sprite = |c: &mut Canvas, img: &Option<Image>, src, dst| {
        if let Some(i) = img {
            blit_to(c, dim, i, src, kr(dst), true, None);
        }
    };
    c.fill(Rgb888::new(0, 0, 0));
    if !v.playlist {
        if let Some(m) = &skin.main {
            let src = (0, 0, MAIN_W as i32, MAIN_H as i32);
            blit_to(c, dim, m, src, (0, 0, LW, WINDOW_H), true, None);
        }
        sprite(c, &skin.titlebar, (27, 0, 275, 14), (0, 0, 275, 14));
        sprite(c, &skin.titlebar, (304, 0, 8, 43), (10, 22, 8, 43));
        window(c, dim, np, skin, v, idle);
    }
    playlist(c, dim, skin, v);
}

/// Everything inside the window that moves.
fn window(c: &mut Canvas, dim: u32, np: &NowPlaying, skin: &Skin, v: &View, idle: bool) {
    let sprite = |c: &mut Canvas, img: &Option<Image>, src, dst| {
        if let Some(i) = img {
            blit_to(c, dim, i, src, kr(dst), true, None);
        }
    };
    if !idle {
        let x = if np.playing { 0 } else { 9 };
        sprite(c, &skin.playpaus, (x, 0, 9, 9), (26, 28, 9, 9));
        // The time in the skin's digits, counting down when asked.
        if let Some(nums) = &skin.numbers {
            let (m, sec) = mmss(if v.remaining {
                np.remaining
            } else {
                np.elapsed
            });
            for (d, x) in [(m / 10, 48), (m % 10, 60), (sec / 10, 78), (sec % 10, 90)] {
                blit_to(
                    c,
                    dim,
                    nums,
                    ((d % 10) as i32 * 9, 0, 9, 13),
                    kr((x, 26, 9, 13)),
                    true,
                    None,
                );
            }
            if v.remaining {
                let (src, dst) = if nums.w >= 108 {
                    ((99, 0, 9, 13), (36, 26, 9, 13))
                } else {
                    ((20, 6, 5, 1), (38, 32, 5, 1))
                };
                blit_to(c, dim, nums, src, kr(dst), true, None);
            }
        }
        // The spectrum: 19 bars of 3 px, coloured by row from viscolor.txt.
        let levels = np.viz_levels.filter(|l| !l.is_empty() && v.vis == 0);
        if let Some(levels) = levels {
            for i in 0..19 {
                let lv = levels[i * levels.len() / 19].clamp(0.0, 1.0);
                let h = (lv * 16.0).round() as i32;
                for r in 16 - h..16 {
                    let col = dimmed(skin.viscolor[2 + r as usize], dim);
                    fill(c, kr((24 + i as i32 * 4, 43 + r, 3, 1)), col);
                }
            }
        }
        // The oscilloscope: the waveform across the 76 px, a dot a column, in viscolor's 18..=22
        // (brightest at the centre line), from the PCM tap when there is one.
        let wave = np
            .viz_sig
            .map(|sig| sig.wave)
            .filter(|w| !w.is_empty() && v.vis == 1);
        if let Some(wave) = wave {
            for x in 0..76 {
                let s = wave[x * wave.len() / 76].clamp(-1.0, 1.0);
                let y = (7.5 - s * 7.5).round() as i32;
                let col = skin.viscolor[18 + ((y - 8).unsigned_abs() as usize / 2).min(4)];
                fill(c, kr((24 + x as i32, 43 + y, 1, 1)), dimmed(col, dim));
            }
        }
        if let Some(font) = &skin.text {
            let line = marquee(&format!("{} - {}", np.artist, np.title), np.elapsed, 31);
            text(c, dim, font, &line, 31, None, |i| {
                kr((111 + 5 * i, 27, 5, 6))
            });
            let khz = number_before(np.codec, "kHz");
            text(c, dim, font, khz, 3, None, |i| kr((156 + 5 * i, 43, 5, 6)));
            let kbps = number_before(np.codec, "kbps");
            text(c, dim, font, kbps, 3, None, |i| kr((111 + 5 * i, 43, 5, 6)));
        }
    }
    let lit = if idle { 12 } else { 0 };
    sprite(c, &skin.monoster, (0, lit, 29, 12), (239, 41, 29, 12));
    sprite(c, &skin.monoster, (29, 12, 27, 12), (212, 41, 27, 12));

    let (level, max) = (v.volume.0 as i32, v.volume.1.max(1) as i32);
    sprite(
        c,
        &skin.volume,
        (0, level * 27 / max * 15, 68, 13),
        (107, 57, 68, 13),
    );
    sprite(
        c,
        &skin.volume,
        (15, 422, 14, 11),
        (107 + level * 54 / max, 58, 14, 11),
    );
    let bal = v.balance.min(100) as i32;
    let off = (bal - 50).abs() * 27 / 50;
    sprite(c, &skin.balance, (9, off * 15, 38, 13), (177, 57, 38, 13));
    sprite(
        c,
        &skin.balance,
        (15, 422, 14, 11),
        (177 + bal * 24 / 100, 58, 14, 11),
    );
    sprite(c, &skin.shufrep, (0, 61, 23, 12), (219, 58, 23, 12));
    sprite(c, &skin.shufrep, (23, 61, 23, 12), (242, 58, 23, 12));

    sprite(c, &skin.posbar, (0, 0, 248, 10), (16, 72, 248, 10));
    if !idle {
        let x = 16 + (np.progress.clamp(0.0, 1.0) * 219.0).round() as i32;
        sprite(c, &skin.posbar, (248, 0, 29, 10), (x, 72, 29, 10));
    }
    for (src, dst) in [
        ((0, 0, 23, 18), (16, 88, 23, 18)),
        ((23, 0, 23, 18), (39, 88, 23, 18)),
        ((46, 0, 23, 18), (62, 88, 23, 18)),
        ((69, 0, 23, 18), (85, 88, 23, 18)),
        ((92, 0, 22, 18), (108, 88, 22, 18)),
        ((114, 0, 22, 16), (136, 89, 22, 16)),
    ] {
        sprite(c, &skin.cbuttons, src, dst);
    }
    let on = |b: bool| if b && !idle { 30 } else { 0 };
    sprite(
        c,
        &skin.shufrep,
        (28, on(np.shuffle), 47, 15),
        (164, 89, 47, 15),
    );
    sprite(
        c,
        &skin.shufrep,
        (0, on(np.repeat > 0), 28, 15),
        (210, 89, 28, 15),
    );
}

/// The playlist, under the window or across the panel: pledit.bmp's title bar and sides, then Up
/// Next from the playing track, in the skin's font and pledit.txt's colours.
fn playlist(c: &mut Canvas, dim: u32, skin: &Skin, v: &View) {
    let top = pl_top(v.playlist);
    let [fg, cur, bg, sel] = skin.pl_colors.map(|col| dimmed(col, dim));
    fill(c, (0, top, LW, LH - top), bg);
    let (side_l, side_r) = (k(12), k(20));
    if let Some(pl) = &skin.pledit {
        let mut put = |src, dst| blit_to(c, dim, pl, src, dst, true, None);
        let cw = k(25);
        let mut x = cw;
        while x < LW - cw {
            put((127, 0, 25, 20), (x, top, cw, PL_TITLE_H));
            x += cw;
        }
        put((0, 0, 25, 20), (0, top, cw, PL_TITLE_H));
        let tw = k(100);
        put((26, 0, 100, 20), ((LW - tw) / 2, top, tw, PL_TITLE_H));
        put((153, 0, 25, 20), (LW - cw, top, cw, PL_TITLE_H));
        let mut y = top + PL_TITLE_H;
        while y < LH {
            put((0, 42, 12, 29), (0, y, side_l, k(29)));
            put((31, 42, 20, 29), (LW - side_r, y, side_r, k(29)));
            y += k(29);
        }
    }
    let Some(font) = &skin.text else {
        return;
    };
    let key = font_bg(font);
    let (x0, x1) = (side_l + 6, LW - side_r - 6);
    let cols = ((x1 - x0) / 15) as usize;
    for i in 0..pl_rows(v.playlist) {
        let qi = v.at + i;
        let Some(s) = v.queue.get(qi) else {
            break;
        };
        let y = top + PL_TITLE_H + i as i32 * ROW_H;
        if i == 0 {
            fill(c, (x0 - 3, y, x1 - x0 + 6, ROW_H), sel);
        }
        let colour = if i == 0 { cur } else { fg };
        let dur_w = s.dur.chars().count();
        let line = format!("{}. {} - {}", qi + 1, s.artist, s.title);
        let ty = y + (ROW_H - 18) / 2;
        let room = cols.saturating_sub(dur_w + 1);
        text(c, dim, font, &line, room, Some((key, colour)), |j| {
            (x0 + 15 * j, ty, 15, 18)
        });
        let dx = x1 - 15 * dur_w as i32;
        text(c, dim, font, &s.dur, dur_w, Some((key, colour)), |j| {
            (dx + 15 * j, ty, 15, 18)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_touch_turns_with_the_panel() {
        // The player's top is on the left: the panel's top-right corner is the landscape origin.
        assert_eq!(to_land(479, 0), (0, 0));
        assert_eq!(to_land(0, 799), (799, 479));
    }

    #[test]
    fn every_control_answers_where_it_is_drawn() {
        for (ctl, r) in WINDOW {
            let (x, y, w, h) = kr(r);
            assert_eq!(ctl_at(x + w / 2, y + h / 2, false), Some(ctl), "{ctl:?}");
        }
        // A near miss below Play is still Play; the gap between two buttons is the nearer one.
        let (x, y, w, h) = kr((39, 88, 23, 18));
        assert_eq!(ctl_at(x + w / 2, y + h + 4, false), Some(Ctl::Play));
        assert_eq!(ctl_at(5, k(110), false), None, "the window's plain border");
        assert_eq!(k(116), WINDOW_H);
        // The transport is a finger's size on its side.
        for (ctl, r) in &WINDOW[17..] {
            let (_, _, w, h) = kr(*r);
            assert!(w >= 60 && h >= 40, "{ctl:?} is {w}x{h}");
        }
        // Under the window: the playlist's title bar, then three rows, as in Wampy.
        assert_eq!(pl_rows(false), 3);
        assert_eq!(ctl_at(400, WINDOW_H + 10, false), Some(Ctl::Pl));
        assert_eq!(ctl_at(400, LH - 1, false), Some(Ctl::PlRow(2)));
        // With the panel: title bar at the top, a panelful of rows.
        assert_eq!(ctl_at(400, 10, true), Some(Ctl::Pl));
        assert_eq!(
            ctl_at(400, PL_TITLE_H + ROW_H + 2, true),
            Some(Ctl::PlRow(1))
        );
        assert!(pl_rows(true) >= 14);
    }

    #[test]
    fn sliders_read_their_whole_track() {
        assert_eq!(seek_permille(0), 0);
        assert_eq!(seek_permille(k(264)), 1000);
        assert_eq!(volume_at(k(107), 120), 0);
        assert_eq!(volume_at(k(175), 120), 120);
        assert_eq!(balance_at(k(177 + 19)), 50);
        assert_eq!(balance_at(k(177)), 0);
        assert_eq!(balance_at(k(215)), 100);
    }

    #[test]
    fn the_time_and_rates_come_out_of_the_strings() {
        assert_eq!(mmss("1:47"), (1, 47));
        assert_eq!(mmss("-2:45"), (2, 45));
        assert_eq!(mmss("1:02:03"), (62, 3));
        assert_eq!(mmss(""), (0, 0));
        assert_eq!(number_before("FLAC · 24bit / 96.0 kHz", "kHz"), "96");
        assert_eq!(number_before("MP3 · 320 kbps / 44.1 kHz", "kbps"), "320");
        assert_eq!(number_before("FLAC", "kbps"), "");
        assert_eq!(marquee("SHORT", "0:09", 31), "SHORT");
        assert_eq!(marquee("ABCDEF", "0:00", 4), "ABCD");
        assert_eq!(marquee("ABCDEF", "0:02", 4), "CDEF");
    }

    #[test]
    fn colours_come_from_the_skin_text_files() {
        let pl = crate::skin::parse_pledit("[Text]\nNormal=#00FF00\nCurrent=#FFFFFF\nNormalBG=#000000\nSelectedBG=#0000FF\nFont=Arial");
        assert_eq!(pl, [0x00ff00, 0xffffff, 0x000000, 0x0000ff]);
        let vis = crate::skin::parse_viscolor("0,0,0, // bg\n24,33,41\n239,49,16\n");
        assert_eq!(vis[2], 0xef3110);
        assert_eq!(
            vis[3],
            crate::skin::VIS_DEFAULT[3],
            "a short file keeps Winamp's own"
        );
    }
}
