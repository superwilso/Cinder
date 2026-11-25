//! v0.5's Now Playing (`docs/PLAN_v0.5.md` P2), drawn from the 2026-11-24 boards
//! (`docs/design/v0.5/NowPlaying.dc.html` for Big, `ProposalHugeNP.dc.html` for Huge).
//!
//! What changed from v0.4's: the cover is inset, not full bleed; the title is bigger; the seek bar
//! always shows its thumb; the toolbar is labelled and its slots are the owner's choice (default
//! Library, Up Next, Bluetooth, Places); and a tap on the title block opens this song's actions
//! (`Hit::Info`). The status bar is the app's, as on every screen.
//!
//! Every number lives in [`Geo`], and [`layout`] and [`render`] both read it, so a target cannot
//! drift from what is drawn. The `Layout`, `Hit` and `Shape` types are v0.4's, so the tap code is
//! the same code.

use crate::canvas::{Canvas, W};
use crate::icons;
use crate::now_playing::{Hit, Layout, NowPlaying, Rail, Shape};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{disc, fill_rect, hline, right, stroke_rect, sty};

/// Big is the default; Huge is for anyone who wants more. (Compact is v0.4's screen.)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Size {
    #[default]
    Big,
    Huge,
}

/// One size's numbers, straight off its board.
struct Geo {
    top: i32,
    cover: (i32, i32, i32),
    title_top: i32,
    title_px: f32,
    title_lh: i32,
    artist_px: f32,
    heart: i32,
    heart_icon: f32,
    rail_band: (i32, i32),
    rail_h: i32,
    thumb: i32,
    times_px: f32,
    times_base: i32,
    row_top: i32,
    row_h: i32,
    /// shuffle, prev, play, next, repeat: (target, icon) in px.
    buttons: [(i32, f32); 5],
    bar_top: i32,
    bar_icon: f32,
}

const BIG: Geo = Geo {
    top: 44,
    cover: (24, 52, 432),
    title_top: 496,
    title_px: 34.0,
    title_lh: 40,
    artist_px: 18.0,
    heart: 56,
    heart_icon: 28.0,
    rail_band: (568, 612),
    rail_h: 6,
    thumb: 20,
    times_px: 15.0,
    times_base: 628,
    row_top: 644,
    row_h: 88,
    buttons: [(64, 28.0), (88, 36.0), (88, 34.0), (88, 36.0), (64, 28.0)],
    bar_top: 736,
    bar_icon: 24.0,
};

const HUGE: Geo = Geo {
    top: 56,
    cover: (70, 60, 340),
    title_top: 412,
    title_px: 42.0,
    title_lh: 48,
    artist_px: 24.0,
    heart: 64,
    heart_icon: 34.0,
    rail_band: (498, 546),
    rail_h: 8,
    thumb: 24,
    times_px: 19.0,
    times_base: 566,
    row_top: 584,
    row_h: 104,
    buttons: [(72, 34.0), (96, 44.0), (104, 40.0), (96, 44.0), (72, 34.0)],
    bar_top: 700,
    bar_icon: 30.0,
};

fn geo(s: Size) -> &'static Geo {
    match s {
        Size::Big => &BIG,
        Size::Huge => &HUGE,
    }
}

const SIDE: i32 = 24;

/// What a toolbar slot opens. The owner picks two to five.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Library,
    UpNext,
    Bluetooth,
    Places,
    Sound,
    Sleep,
    Lyrics,
    Settings,
    Shelf,
    Radio,
}

impl Tool {
    pub const ALL: [Tool; 10] = [
        Tool::Library,
        Tool::UpNext,
        Tool::Bluetooth,
        Tool::Places,
        Tool::Sound,
        Tool::Sleep,
        Tool::Lyrics,
        Tool::Settings,
        Tool::Shelf,
        Tool::Radio,
    ];
    /// The default bar (owner 11-24: library in the bar, Places for the rest).
    pub const DEFAULT: [Tool; 4] = [Tool::Library, Tool::UpNext, Tool::Bluetooth, Tool::Places];

    pub fn label(self) -> &'static str {
        crate::i18n::text(match self {
            Tool::Library => "Library",
            Tool::UpNext => "Up Next",
            Tool::Bluetooth => "Bluetooth",
            Tool::Places => "Places",
            Tool::Sound => "Sound",
            Tool::Sleep => "Sleep",
            Tool::Lyrics => "Lyrics",
            Tool::Settings => "Settings",
            Tool::Shelf => "Shelf",
            Tool::Radio => "Radio",
        })
    }

    /// As the settings file stores it.
    pub fn token(self) -> &'static str {
        match self {
            Tool::Library => "library",
            Tool::UpNext => "upnext",
            Tool::Bluetooth => "bluetooth",
            Tool::Places => "places",
            Tool::Sound => "sound",
            Tool::Sleep => "sleep",
            Tool::Lyrics => "lyrics",
            Tool::Settings => "settings",
            Tool::Shelf => "shelf",
            Tool::Radio => "radio",
        }
    }

    pub fn from_token(s: &str) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|t| t.token() == s.trim())
    }

    fn icon(
        self,
        c: &mut Canvas,
        cx: f32,
        cy: f32,
        s: f32,
        col: embedded_graphics::pixelcolor::Rgb888,
    ) {
        match self {
            Tool::Library => icons::books(c, cx, cy, s, col),
            Tool::UpNext => icons::queue(c, cx, cy, s, col),
            Tool::Bluetooth => icons::bt(c, cx, cy, s, col),
            Tool::Places => icons::library(c, cx, cy, s, col),
            Tool::Sound => icons::eq(c, cx, cy, s, col),
            Tool::Sleep => icons::moon(c, cx, cy, s, col),
            Tool::Lyrics => icons::list_view(c, cx, cy, s, col),
            Tool::Settings => icons::settings(c, cx, cy, s, col),
            Tool::Shelf => icons::bookmark(c, cx, cy, s, col),
            Tool::Radio => icons::radio(c, cx, cy, s, col),
        }
    }
}

/// The transport row's centres: the board's buttons, spread edge to edge 16 px in.
fn button_cx(g: &Geo) -> [i32; 5] {
    let total: i32 = g.buttons.iter().map(|b| b.0).sum();
    let gap = (W as i32 - 32 - total) / 4;
    let mut x = 16;
    let mut out = [0; 5];
    for (i, (w, _)) in g.buttons.iter().enumerate() {
        out[i] = x + w / 2;
        x += w + gap;
    }
    out
}

/// The cover's edge in px at `size`: what the app bakes or scales a cover to.
pub fn cover_edge(size: Size) -> i32 {
    geo(size).cover.2
}

fn heart_rect(g: &Geo) -> (i32, i32, i32, i32) {
    (W as i32 - SIDE - g.heart + 8, g.title_top, g.heart, g.heart)
}

/// What is where, for `size` with `tools` in the bar.
pub fn layout(size: Size, lyrics: bool, tools: usize) -> Layout {
    let g = geo(size);
    let (cx, cy, cs) = g.cover;
    let rail = Rail {
        x0: SIDE,
        y: (g.rail_band.0 + g.rail_band.1 - g.rail_h) / 2,
        w: W as i32 - 2 * SIDE,
        h: g.rail_h,
        grab_top: g.rail_band.0,
        grab_bot: g.rail_band.1,
    };
    let mut l = Layout::new(rail, cy, cy + cs);
    if lyrics {
        l.push(
            Shape::Rect {
                x: cx + 12,
                y: cy + 12,
                w: 96,
                h: 44,
            },
            Hit::Lyrics,
        );
    }
    let (hx, hy, hw, hh) = heart_rect(g);
    l.push(
        Shape::Rect {
            x: hx,
            y: hy,
            w: hw,
            h: hh,
        },
        Hit::Like,
    );
    // The title block, from the cover's foot to the rail: this song's actions.
    l.push(
        Shape::Rect {
            x: 0,
            y: cy + cs + 4,
            w: hx,
            h: g.rail_band.0 - (cy + cs + 4),
        },
        Hit::Info,
    );
    // The transport row as one band: each control owns the width to the midpoints with its
    // neighbours, from under the rail's times to the toolbar (v0.4's Big view rule).
    const ROW: [Hit; 5] = [
        Hit::Shuffle,
        Hit::Prev,
        Hit::PlayPause,
        Hit::Next,
        Hit::Repeat,
    ];
    let xs = button_cx(g);
    let top = g.times_base + 6;
    for (i, h) in ROW.iter().enumerate() {
        let x0 = if i == 0 { 0 } else { (xs[i - 1] + xs[i]) / 2 };
        let x1 = xs.get(i + 1).map_or(W as i32, |n| (xs[i] + n) / 2);
        l.push(
            Shape::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: g.bar_top - top,
            },
            *h,
        );
    }
    let n = tools.clamp(2, 5) as i32;
    for i in 0..n {
        let (x0, x1) = (i * W as i32 / n, (i + 1) * W as i32 / n);
        l.push(
            Shape::Rect {
                x: x0,
                y: g.bar_top + 1,
                w: x1 - x0,
                h: crate::canvas::H as i32 - g.bar_top - 1,
            },
            Hit::Toolbar(i as u8),
        );
    }
    l
}

/// Draw Now Playing below the status bar.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, size: Size, tools: &[Tool]) {
    let g = geo(size);
    fill_rect(c, 0, g.top, W as i32, crate::canvas::H as i32 - g.top, t.bg);
    let (cx, cy, cs) = g.cover;
    if np.title.is_empty() && np.artist.is_empty() {
        crate::widgets::center(
            c,
            f,
            240.0,
            (cy + cs / 2) as f32,
            crate::i18n::text("Nothing playing"),
            &sty(Family::Sans, Weight::Regular, g.artist_px + 4.0, t.dim, 0.0),
        );
    } else {
        match np.art_full {
            // ponytail: the shell pre-scales to 480; v0.5 asks for the board's size in P2's app work.
            Some(img) if img.w == cs as usize => crate::art::draw_image(c, t, cx, cy, img, 1.0),
            _ => crate::art::block_cached(c, t, cx, cy, cs, cs, np.art, 1.0),
        }
    }
    if np.lyrics {
        let st = sty(Family::Mono, Weight::Regular, 13.0, t.ink, 0.14);
        fill_rect(c, cx + 12, cy + 12, 96, 44, t.panel);
        stroke_rect(c, cx + 12, cy + 12, 96, 44, t.ctrl(), 1);
        crate::widgets::center(
            c,
            f,
            (cx + 60) as f32,
            (cy + 39) as f32,
            crate::i18n::text("LYRICS"),
            &st,
        );
    }

    // Title and artist, left of the heart. The title scrolls rather than losing its end.
    let (hx, hy, hw, hh) = heart_rect(g);
    let col_w = (hx - SIDE - 8) as f32;
    let tst = sty(Family::Sans, Weight::ExtraBold, g.title_px, t.ink, 0.0);
    let base = g.title_top + g.title_lh - (g.title_lh - g.title_px as i32) / 2 - 4;
    crate::widgets::marquee(c, f, SIDE as f32, base as f32, np.title, &tst, col_w);
    let ast = sty(Family::Sans, Weight::Regular, g.artist_px, t.dim, 0.0);
    let abase = g.title_top + g.title_lh + g.artist_px as i32 + 2;
    crate::widgets::marquee(c, f, SIDE as f32, abase as f32, np.artist, &ast, col_w);
    icons::heart(
        c,
        (hx + hw / 2) as f32,
        (hy + hh / 2) as f32,
        g.heart_icon,
        if np.liked { t.acc } else { t.faint },
    );

    // The rail, its thumb always shown: it is the thing to grab.
    let (ry, rw) = (
        (g.rail_band.0 + g.rail_band.1 - g.rail_h) / 2,
        W as i32 - 2 * SIDE,
    );
    fill_rect(c, SIDE, ry, rw, g.rail_h, t.line);
    let fw = (rw as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(c, SIDE, ry, fw, g.rail_h, t.acc);
    let thumb = if np.scrubbing { g.thumb + 8 } else { g.thumb };
    disc(c, SIDE + fw, ry + g.rail_h / 2, thumb as u32, t.acc);
    let ts = sty(Family::Mono, Weight::Regular, g.times_px, t.dim, 0.06);
    text::draw(c, f, SIDE as f32, g.times_base as f32, np.elapsed, &ts);
    right(
        c,
        f,
        (W as i32 - SIDE) as f32,
        g.times_base as f32,
        np.remaining,
        &ts,
    );

    // Transport.
    let xs = button_cx(g);
    let ty = (g.row_top + g.row_h / 2) as f32;
    let [(_, si), (_, pi), (pw, playi), (_, ni), (_, ri)] = g.buttons;
    icons::shuffle(
        c,
        xs[0] as f32,
        ty,
        si,
        if np.shuffle { t.acc } else { t.dim },
    );
    icons::prev(c, xs[1] as f32, ty, pi, t.ink);
    disc(c, xs[2], ty as i32, pw as u32, t.acc);
    if np.playing {
        icons::pause(c, xs[2] as f32, ty, playi, t.acc_ink);
    } else {
        icons::play(c, xs[2] as f32, ty, playi, t.acc_ink);
    }
    icons::next(c, xs[3] as f32, ty, ni, t.ink);
    icons::repeat(
        c,
        xs[4] as f32,
        ty,
        ri,
        if np.repeat > 0 { t.acc } else { t.dim },
    );
    if np.repeat == 1 {
        fill_rect(c, xs[4] - 1, ty as i32 - 1, 3, 3, t.acc);
    }

    // The labelled toolbar: equal slots, a hairline between them.
    let n = tools.len().clamp(2, 5);
    hline(c, g.bar_top, t.line);
    let h = crate::canvas::H as i32 - g.bar_top;
    let lst = sty(
        Family::Sans,
        Weight::SemiBold,
        if size == Size::Huge { 18.0 } else { 15.0 },
        t.ink,
        0.0,
    );
    let icon_cy = g.bar_top + h / 2 - (g.bar_icon as i32 / 2) + 2;
    for (i, tool) in tools.iter().take(n).enumerate() {
        let (x0, x1) = (
            i as i32 * W as i32 / n as i32,
            (i as i32 + 1) * W as i32 / n as i32,
        );
        if i > 0 {
            fill_rect(c, x0, g.bar_top + 1, 1, h - 1, t.line);
        }
        let mx = (x0 + x1) as f32 / 2.0;
        tool.icon(c, mx, icon_cy as f32, g.bar_icon, t.dim);
        let label = crate::widgets::fit(f, tool.label(), &lst, (x1 - x0 - 8) as f32);
        crate::widgets::center(
            c,
            f,
            mx,
            (icon_cy + g.bar_icon as i32 / 2 + 6 + lst.size as i32) as f32,
            &label,
            &lst,
        );
    }
}

/// The board's song, for previews and tests.
pub fn sample() -> NowPlaying<'static> {
    NowPlaying {
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        codec: "FLAC · 24bit / 96.0 kHz",
        badge: "FLAC 24/96",
        clock: "14:32",
        battery: 78,
        elapsed: "1:47",
        remaining: "-2:45",
        progress: 0.4,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: true,
        playing: true,
        shuffle: false,
        repeat: 2,
        viz_seed: 0.0,
        viz_kind: 0,
        viz_size: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        page: 0,
        scrubbing: false,
        lyrics: true,
        skin: None,
        tape: None,
    }
}

#[cfg(test)]
mod tests;
