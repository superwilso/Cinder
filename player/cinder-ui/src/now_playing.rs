//! Now Playing — ported from cinder-proto-screens1.jsx `CNowPlaying`.
//! Day: full-bleed 480x480 gradient art, 36-bar visualiser, title/artist/codec,
//! progress + time, transport (shuffle·prev·play·next·repeat), bottom toolbar.
//! Night: compact 92px thumb + text header, art dimmed to 32%, viz centred in
//! the negative space; shared progress / transport / toolbar below.

use crate::art;
use crate::canvas::Canvas;
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{bars, fill_rect, hline, right, sty};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

pub struct NowPlaying<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub codec: &'a str, // "FLAC · 24bit / 96.0 kHz"
    pub badge: &'a str,  // status-bar badge "FLAC 24/96"
    pub clock: &'a str,
    pub battery: u8,
    pub elapsed: &'a str,
    pub remaining: &'a str,
    pub progress: f32, // 0..1
    pub art: &'a str,  // swatch name
    pub liked: bool,
    pub playing: bool,
}

fn s(fam: Family, weight: Weight, size: f32, color: embedded_graphics::pixelcolor::Rgb888, tracking: f32) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, np.clock, np.badge, np.battery);
    let seed = 2.0;

    if t.night {
        // compact header: 92px thumb @32% + title/artist/codec column
        art::block(c, t, 24, 80, 92, 92, np.art, 0.32);
        text::draw(c, f, 134.0, 110.0, np.title, &s(Family::Sans, Weight::Bold, 21.0, t.ink, 0.0));
        text::draw(c, f, 134.0, 133.0, np.artist, &s(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0));
        text::draw(c, f, 134.0, 153.0, np.codec, &s(Family::Mono, Weight::Regular, 10.0, t.acc, 0.08));
        // viz centred in the airy negative space
        bars(c, 24, 420, 432, 16, 36, 3, seed, t.acc, t.line);
    } else {
        // full-bleed album art (34..514)
        art::block(c, t, 0, 34, 480, 480, np.art, 1.0);
        // visualiser (524..546)
        bars(c, 24, 524, 432, 22, 36, 3, seed, t.acc, t.line);
        // title / artist / codec
        text::draw(c, f, 24.0, 580.0, np.title, &s(Family::Sans, Weight::Bold, 26.0, t.ink, 0.0));
        text::draw(c, f, 24.0, 605.0, np.artist, &s(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0));
        right(c, f, 456.0, 605.0, np.codec, &s(Family::Mono, Weight::Regular, 10.0, t.acc, 0.08));
    }

    // ---------- progress (shared) ----------
    let (py, px0, pw) = (636, 24, 432);
    fill_rect(c, px0, py, pw, 4, t.line);
    let fillw = (pw as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(c, px0, py, fillw, 4, t.acc);
    text::draw(c, f, 24.0, 660.0, np.elapsed, &s(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    right(c, f, 456.0, 660.0, np.remaining, &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));

    // ---------- transport (centre y 702) ----------
    let ty = 702.0;
    icons::shuffle(c, 50.0, ty, 18.0, t.faint);
    icons::prev(c, 140.0, ty, 28.0, t.ink);
    Circle::with_center(Point::new(240, ty as i32), 68)
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    if np.playing {
        icons::pause(c, 240.0, ty, 28.0, t.acc_ink);
    } else {
        icons::play(c, 240.0, ty, 28.0, t.acc_ink);
    }
    icons::next(c, 340.0, ty, 28.0, t.ink);
    icons::repeat(c, 430.0, ty, 18.0, t.acc);

    // ---------- bottom toolbar (738..800) ----------
    hline(c, 738, t.line);
    let tb = 769.0;
    icons::heart(c, 48.0, tb, 19.0, if np.liked { t.acc } else { t.dim });
    icons::queue(c, 144.0, tb, 19.0, t.dim);
    icons::eq(c, 240.0, tb, 19.0, t.dim);
    icons::bt(c, 336.0, tb, 18.0, t.dim);
    icons::library(c, 432.0, tb, 19.0, t.dim);
}
