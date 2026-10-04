//! Now Playing in the styles other than Cinder (see `style.rs`).
//!
//! Each style is two functions over one table of numbers: `*_layout`, which says where every
//! target is, and `*_render`, which draws the screen and places every control FROM that layout.
//! Neither has a coordinate the other lacks, so a style cannot draw Play in one place and answer
//! for it in another. The contract they both pass is `now_playing::tests::every_style_*`.
//!
//! What every style keeps, whatever it looks like (`docs/DESIGN_GUIDE.md`, "Now Playing"):
//!
//! - the status bar, and the band under it (y 44–91) for the Menu, the Lyrics chip on the left and
//!   the sleep badge on the right. Nothing of a style's own is drawn there;
//! - the palette's colours only, with night drawing the cover at a third of its brightness;
//! - the three pages (cover, spectrum, level), turned by a swipe across the cover's block;
//! - every control Cinder has: like, title block (Track information), seek, times, shuffle, prev,
//!   play/pause, next, repeat, and a way to the Library, Up Next, Bluetooth and Settings.

use crate::art;
use crate::canvas::Canvas;
use crate::icons;
use crate::now_playing::{self as np, Hit, Layout, NowPlaying, NpPage, Rail, Shape};
use crate::style::Style;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{self, fill_rect, hline, stroke_rect, sty};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

const W: i32 = crate::canvas::W as i32;
const H: i32 = crate::canvas::H as i32;

/// The layout of a style other than Cinder.
pub fn layout(style: Style, lyrics: bool) -> Layout {
    match style {
        Style::Terminal => terminal_layout(lyrics),
        _ => nocturne_layout(lyrics),
    }
}

/// Now Playing, drawn in `t.style`.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    let l = layout(t.style, np.lyrics);
    match t.style {
        Style::Terminal => terminal_render(c, t, f, np, &l),
        _ => nocturne_render(c, t, f, np, &l),
    }
    if np.lyrics {
        np::lyrics_chip(c, t, f);
    }
}

/// The Lyrics chip and the Menu band: the same in every style, and first and last in its order.
fn push_lyrics(l: &mut Layout, lyrics: bool) {
    if lyrics {
        l.push(Shape::Rect { x: 0, y: crate::chrome::STATUS_H, w: np::LYRICS_HIT_W, h: 44 }, Hit::Lyrics);
    }
}

fn push_menu(l: &mut Layout) {
    l.push(Shape::Rect { x: 0, y: 0, w: W, h: crate::chrome::HEADER_BOTTOM }, Hit::Menu);
}

fn centre(l: &Layout, h: Hit) -> (f32, f32) {
    let (x, y) = l.at(h).map_or((0, 0), Shape::centre);
    (x as f32, y as f32)
}

fn s(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

fn idle(np: &NowPlaying) -> bool {
    np.title.is_empty() && np.artist.is_empty()
}

/// The page block's contents for pages 2 and 3, inside `(x, y, w, h)`; page 1 is the style's own.
fn audio_page(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, (x, y, w, h): (i32, i32, i32, i32)) {
    let mid = (x + w / 2) as f32;
    let caption = s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18);
    match np::page_from_index(np.page) {
        NpPage::Cover => {}
        NpPage::Spectrum => {
            widgets::center(c, f, mid, (y + 24) as f32, crate::viz::name_upper(np.viz_kind), &caption);
            if np::viz_live(np) {
                crate::viz::draw_any(c, x + 12, y + 44, w - 24, h - 60, np.viz_seed, crate::viz::from_index(np.viz_kind), t.acc, t.line, np.viz_levels, np.viz_peaks, np.viz_sig, 255, 255);
            } else {
                widgets::center(c, f, mid, (y + h / 2) as f32, np::viz_absent_text(np).0,
                                &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0));
            }
        }
        NpPage::Level => {
            widgets::center(c, f, mid, (y + 24) as f32, "OUTPUT LEVEL", &caption);
            let (mean, peak) = np::level_stats(np);
            let (mx, mw, my, mh) = (x + 16, w - 32, y + h / 2 - 40, 48);
            fill_rect(c, mx, my, mw, mh, t.panel);
            stroke_rect(c, mx, my, mw, mh, t.line, 1);
            let fw = (mw as f32 * mean).round() as i32;
            if fw > 0 {
                fill_rect(c, mx, my, fw, mh, t.acc);
            }
            let px = mx + ((mw - 3) as f32 * peak).round() as i32;
            fill_rect(c, px, my - 6, 3, mh + 12, if peak > 0.0 { t.ink } else { t.line });
            let mut b = [0u8; 3];
            widgets::center(c, f, mid, (my + mh + 72) as f32, np::dec((mean * 100.0).round() as i32, &mut b),
                            &s(Family::Mono, Weight::Bold, 48.0, t.ink, 0.0));
        }
    }
}

/// The cover, or what stands in for it, in a `size` square at `(x, y)`. Night draws it at a third
/// of its brightness, as Cinder's night thumbnail does.
fn cover(c: &mut Canvas, t: &Theme, np: &NowPlaying, x: i32, y: i32, size: i32) {
    let op = if t.night { 0.32 } else { 1.0 };
    match np.art_full {
        Some(img) => art::draw_fitted(c, t, x, y, img, size as usize, op),
        None => art::block_cached(c, t, x, y, size, size, np.art, op),
    }
}

/// The small visualiser over the bottom of the cover, standing on `bottom`, when it is on.
fn cover_viz(c: &mut Canvas, t: &Theme, np: &NowPlaying, x: i32, w: i32, bottom: i32) {
    let size = crate::viz::size_from_index(np.viz_size);
    if let Some((vy, vh, at, ab)) = crate::viz::size_box(size, bottom, false) {
        crate::viz::draw_any(c, x, vy, w, vh, np.viz_seed, crate::viz::from_index(np.viz_kind), t.acc, t.line, np.viz_levels, np.viz_peaks, np.viz_sig, at, ab);
    }
}

fn nothing_playing(c: &mut Canvas, t: &Theme, f: &FontSet, cx: f32, cy: f32, upper: bool) {
    let head = if upper { "NOTHING PLAYING" } else { "Nothing playing" };
    let fam = if upper { Family::Mono } else { Family::Sans };
    widgets::center(c, f, cx, cy, head, &s(fam, Weight::Regular, 22.0, t.dim, 0.0));
    widgets::center(c, f, cx, cy + 26.0, "CHOOSE A TRACK FROM YOUR LIBRARY",
                    &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
}

// ── Nocturne ──────────────────────────────────────────────────────────────────────────────────
//
// Dark editorial: the cover inset with room around it, the title large and light, one thin rule
// for the rail, and an outlined ring for Play. Quiet on purpose — the accent is the ring, the
// played part of the rail and the codec line, and nothing else.

mod noc {
    pub const ART_X: i32 = 40;
    pub const ART_Y: i32 = 96;
    pub const ART: i32 = 400;
    pub const DOTS_Y: i32 = 508;
    pub const KICKER_Y: f32 = 538.0;
    pub const TITLE_Y: f32 = 576.0;
    pub const ARTIST_Y: f32 = 603.0;
    pub const TEXT_X: f32 = 40.0;
    pub const TEXT_W: f32 = 344.0;
    pub const HEART: (i32, i32, i32) = (424, 566, 26);
    pub const RAIL_Y: i32 = 636;
    pub const TIMES_Y: f32 = 661.0;
    pub const TRANSPORT_Y: i32 = 714;
    pub const TOOLBAR_TOP: i32 = 754;
}

fn nocturne_layout(lyrics: bool) -> Layout {
    use noc::*;
    let rail = Rail { x0: ART_X, y: RAIL_Y, w: ART, h: 2, grab_top: 614, grab_bot: 668 };
    let mut l = Layout::new(rail, ART_Y, ART_Y + ART);
    push_lyrics(&mut l, lyrics);
    let (hx, hy, hh) = HEART;
    l.push(Shape::Rect { x: hx - hh, y: hy - hh, w: 2 * hh + 1, h: 2 * hh + 1 }, Hit::Like);
    l.push(Shape::Rect { x: 0, y: 516, w: hx - hh - 4, h: 94 }, Hit::Info);
    let ty = TRANSPORT_Y;
    l.push(Shape::Circle { cx: 240, cy: ty, r: 38 }, Hit::PlayPause);
    l.push(Shape::Circle { cx: 150, cy: ty, r: 30 }, Hit::Prev);
    l.push(Shape::Circle { cx: 330, cy: ty, r: 30 }, Hit::Next);
    l.push(Shape::Circle { cx: 64, cy: ty, r: 24 }, Hit::Shuffle);
    l.push(Shape::Circle { cx: 416, cy: ty, r: 24 }, Hit::Repeat);
    for i in 0..4 {
        l.push(Shape::Rect { x: i * 120, y: TOOLBAR_TOP + 1, w: 120, h: H - TOOLBAR_TOP - 1 }, Hit::Toolbar(i as u8));
    }
    push_menu(&mut l);
    l
}

/// Tracked capitals under each toolbar slot. Words, not icons: this style's one concession to
/// density is to say what each place is.
const TOOLBAR_WORDS: [&str; 4] = ["LIBRARY", "UP NEXT", "BLUETOOTH", "SETTINGS"];

fn nocturne_render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout) {
    use noc::*;
    c.fill(t.bg);
    let quiet = idle(np);
    let mid = (ART_X + ART / 2) as f32;

    // The page block: the cover, or the audio pages in its place.
    if quiet {
        nothing_playing(c, t, f, mid, (ART_Y + ART / 2) as f32, false);
    } else if np::page_from_index(np.page) == NpPage::Cover {
        cover(c, t, np, ART_X, ART_Y, ART);
        cover_viz(c, t, np, ART_X + 12, ART - 24, ART_Y + ART - 8);
    } else {
        audio_page(c, t, f, np, (ART_X, ART_Y, ART, ART));
    }
    if !quiet {
        for i in 0..np::PAGES as i32 {
            let on = i == (np.page % np::PAGES) as i32;
            fill_rect(c, 240 - 22 + i * 16, DOTS_Y, if on { 12 } else { 4 }, 2, if on { t.acc } else { t.faint });
        }
    }

    // The words: codec as a kicker, the title large and light, the artist under it.
    if !quiet {
        let kick = s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.18);
        text::draw(c, f, TEXT_X, KICKER_Y, &widgets::fit(f, np.codec, &kick, TEXT_W), &kick);
        widgets::marquee(c, f, TEXT_X, TITLE_Y, np.title, &s(Family::Sans, Weight::Light, 34.0, t.ink, 0.0), TEXT_W);
        widgets::marquee(c, f, TEXT_X, ARTIST_Y, np.artist, &s(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0), TEXT_W);
        let (hx, hy) = centre(l, Hit::Like);
        icons::heart(c, hx, hy, 24.0, if np.liked { t.acc } else { t.faint });
    }

    // The rail: one thin rule, the played part in the accent, a short upright at the position.
    let r = l.rail;
    fill_rect(c, r.x0, r.y, r.w, r.h, t.line);
    let fw = (r.w as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    if !quiet {
        fill_rect(c, r.x0, r.y, fw, r.h, t.acc);
        let knob = if np.scrubbing { 18 } else { 10 };
        fill_rect(c, r.x0 + fw - 1, r.y + r.h / 2 - knob / 2, 2, knob, t.acc);
        let ts = s(Family::Mono, Weight::Regular, 12.0, t.dim, 0.06);
        text::draw(c, f, r.x0 as f32, TIMES_Y, np.elapsed, &ts);
        widgets::right(c, f, (r.x0 + r.w) as f32, TIMES_Y, np.remaining, &TextStyle { color: t.faint, ..ts });
    }

    // The transport: plain glyphs either side of an outlined ring.
    let side = if quiet { t.faint } else { t.ink };
    let (px, py) = centre(l, Hit::PlayPause);
    for (rad, col) in [(33, t.acc), (31, t.bg)] {
        Circle::with_center(Point::new(px as i32, py as i32), (rad * 2) as u32)
            .into_styled(PrimitiveStyle::with_fill(col))
            .draw(c)
            .ok();
    }
    if np.playing && !quiet {
        icons::pause(c, px, py, 26.0, t.acc);
    } else {
        icons::play(c, px + 2.0, py, 26.0, t.acc);
    }
    let (x, y) = centre(l, Hit::Prev);
    icons::prev(c, x, y, 28.0, side);
    let (x, y) = centre(l, Hit::Next);
    icons::next(c, x, y, 28.0, side);
    let (x, y) = centre(l, Hit::Shuffle);
    icons::shuffle(c, x, y, 20.0, if np.shuffle && !quiet { t.acc } else { t.faint });
    let (x, y) = centre(l, Hit::Repeat);
    icons::repeat(c, x, y, 20.0, if np.repeat > 0 && !quiet { t.acc } else { t.faint });
    if !quiet && np.repeat == 1 {
        fill_rect(c, x as i32 - 1, y as i32 - 1, 3, 3, t.acc);
    } else if !quiet && np.repeat == np::REPEAT_ALBUM {
        widgets::center(c, f, x, y + 24.0, "ALBUM", &s(Family::Mono, Weight::Bold, 9.0, t.acc, 0.1));
    }

    // The toolbar: four words under a hairline.
    hline(c, TOOLBAR_TOP, t.line);
    for (i, word) in TOOLBAR_WORDS.iter().enumerate() {
        let (x, _) = centre(l, Hit::Toolbar(i as u8));
        let col = if quiet && i == 0 { t.acc } else { t.dim };
        let st = s(Family::Mono, Weight::Regular, 11.0, col, 0.16);
        widgets::center(c, f, x, 782.0, &widgets::fit(f, word, &st, 112.0), &st);
    }
}

// ── Terminal ──────────────────────────────────────────────────────────────────────────────────
//
// Retro and blunt: a framed cover, monospace capitals, controls as bracketed words, and a rail of
// cells. ASCII only — a glyph the bundled fonts lack costs the font chain tens of megabytes, and one
// nothing covers kills the app (`reference_font_chain_oom`), so no ► or ░ here.

mod term {
    pub const FRAME_X: i32 = 36;
    pub const FRAME_Y: i32 = 96;
    pub const FRAME: i32 = 408;
    pub const ART_INSET: i32 = 6;
    pub const READOUT_Y: f32 = 528.0;
    pub const TITLE_Y: f32 = 562.0;
    pub const ARTIST_Y: f32 = 588.0;
    pub const TEXT_X: f32 = 24.0;
    pub const TEXT_W: f32 = 376.0;
    pub const HEART: (i32, i32, i32) = (436, 554, 26);
    pub const RAIL_X0: i32 = 24;
    pub const RAIL_Y: i32 = 620;
    pub const RAIL_W: i32 = 432;
    pub const CELLS: i32 = 27;
    /// Under the cells, not beside them: beside, "-2:45:09" at 140% ran off the panel.
    pub const TIMES_Y: f32 = 654.0;
    pub const TRANSPORT_Y: i32 = 700;
    pub const TOOLBAR_TOP: i32 = 744;
}

fn terminal_layout(lyrics: bool) -> Layout {
    use term::*;
    let rail = Rail { x0: RAIL_X0, y: RAIL_Y, w: RAIL_W, h: 14, grab_top: 608, grab_bot: 660 };
    let mut l = Layout::new(rail, FRAME_Y, FRAME_Y + FRAME);
    push_lyrics(&mut l, lyrics);
    let (hx, hy, hh) = HEART;
    l.push(Shape::Rect { x: hx - hh, y: hy - hh, w: 2 * hh + 1, h: 2 * hh + 1 }, Hit::Like);
    l.push(Shape::Rect { x: 0, y: 508, w: hx - hh - 6, h: 96 }, Hit::Info);
    let ty = TRANSPORT_Y;
    l.push(Shape::Rect { x: 172, y: ty - 26, w: 136, h: 52 }, Hit::PlayPause);
    l.push(Shape::Rect { x: 104, y: ty - 22, w: 64, h: 44 }, Hit::Prev);
    l.push(Shape::Rect { x: 312, y: ty - 22, w: 64, h: 44 }, Hit::Next);
    l.push(Shape::Rect { x: 20, y: ty - 22, w: 76, h: 44 }, Hit::Shuffle);
    l.push(Shape::Rect { x: 384, y: ty - 22, w: 76, h: 44 }, Hit::Repeat);
    for i in 0..4 {
        l.push(Shape::Rect { x: i * 120, y: TOOLBAR_TOP + 1, w: 120, h: H - TOOLBAR_TOP - 1 }, Hit::Toolbar(i as u8));
    }
    push_menu(&mut l);
    l
}

const TERMINAL_TOOLBAR: [&str; 4] = ["[LIB]", "[QUEUE]", "[BT]", "[SET]"];

fn terminal_render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout) {
    use term::*;
    c.fill(t.bg);
    let quiet = idle(np);
    let mono = |size: f32, w: Weight, col: Rgb888| s(Family::Mono, w, size, col, 0.04);

    // The framed page block.
    stroke_rect(c, FRAME_X, FRAME_Y, FRAME, FRAME, if quiet { t.line } else { t.dim }, 1);
    let (ax, ay, asz) = (FRAME_X + ART_INSET, FRAME_Y + ART_INSET, FRAME - ART_INSET * 2);
    if quiet {
        nothing_playing(c, t, f, 240.0, (FRAME_Y + FRAME / 2) as f32, true);
    } else if np::page_from_index(np.page) == NpPage::Cover {
        cover(c, t, np, ax, ay, asz);
        cover_viz(c, t, np, ax + 8, asz - 16, ay + asz - 6);
    } else {
        audio_page(c, t, f, np, (ax, ay, asz, asz));
    }

    // The readout: where you are, which page, what is playing.
    let cap = mono(11.0, Weight::Regular, t.faint);
    text::draw(c, f, TEXT_X, READOUT_Y, "// NOW PLAYING", &cap);
    if !quiet {
        let mut pg = *b"[1/3]";
        pg[1] = b'1' + (np.page % np::PAGES);
        widgets::center(c, f, 240.0, READOUT_Y, core::str::from_utf8(&pg).unwrap_or(""), &cap);
        if !np.badge.is_empty() {
            let st = mono(11.0, Weight::Regular, t.acc);
            let badge = widgets::fit(f, np.badge, &st, 140.0);
            widgets::right(c, f, 456.0, READOUT_Y, &format!("[{badge}]"), &st);
        }
        widgets::marquee(c, f, TEXT_X, TITLE_Y, &np.title.to_uppercase(), &mono(24.0, Weight::Bold, t.ink), TEXT_W);
        widgets::marquee(c, f, TEXT_X, ARTIST_Y, &format!("BY {}", np.artist.to_uppercase()),
                         &mono(13.0, Weight::Regular, t.dim), TEXT_W);
        let (hx, hy) = centre(l, Hit::Like);
        icons::heart(c, hx, hy, 24.0, if np.liked { t.acc } else { t.faint });
    }

    // The rail: a row of cells, filled to the position, with the times under its two ends.
    let r = l.rail;
    let cell = r.w / CELLS;
    let filled = if quiet { 0 } else { (np.progress.clamp(0.0, 1.0) * CELLS as f32).round() as i32 };
    for i in 0..CELLS {
        let col = if i < filled { t.acc } else { t.line };
        fill_rect(c, r.x0 + i * cell, r.y, cell - 2, r.h, col);
    }
    if np.scrubbing {
        stroke_rect(c, r.x0 - 3, r.y - 3, r.w + 4, r.h + 6, t.acc, 1);
    }
    if !quiet {
        let ts = mono(13.0, Weight::Regular, t.dim);
        text::draw(c, f, r.x0 as f32, TIMES_Y, np.elapsed, &ts);
        widgets::right(c, f, (r.x0 + r.w) as f32, TIMES_Y, np.remaining, &TextStyle { color: t.faint, ..ts });
    }

    // The transport: bracketed words, Play boxed in the accent.
    let word = |c: &mut Canvas, h: Hit, label: &str, col: Rgb888| {
        let (x, y) = centre(l, h);
        widgets::center(c, f, x, y + 5.0, label, &mono(14.0, Weight::Bold, col));
    };
    let side = if quiet { t.faint } else { t.ink };
    if let Some(Shape::Rect { x, y, w, h }) = l.at(Hit::PlayPause) {
        stroke_rect(c, x + 4, y + 4, w - 8, h - 8, t.acc, 2);
    }
    word(c, Hit::PlayPause, if np.playing && !quiet { "|| PAUSE" } else { "> PLAY" }, t.acc);
    word(c, Hit::Prev, "[<<]", side);
    word(c, Hit::Next, "[>>]", side);
    word(c, Hit::Shuffle, "[SHUF]", if np.shuffle && !quiet { t.acc } else { t.faint });
    let rpt = match np.repeat {
        _ if quiet => "[RPT]",
        1 => "[RPT1]",
        np::REPEAT_ALBUM => "[ALBM]",
        _ => "[RPT]",
    };
    word(c, Hit::Repeat, rpt, if np.repeat > 0 && !quiet { t.acc } else { t.faint });

    // The toolbar.
    hline(c, TOOLBAR_TOP, t.line);
    for (i, label) in TERMINAL_TOOLBAR.iter().enumerate() {
        let col = if quiet && i == 0 { t.acc } else { t.dim };
        word(c, Hit::Toolbar(i as u8), label, col);
    }
}
