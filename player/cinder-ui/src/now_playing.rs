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
use crate::widgets::{fill_rect, hline, right, sty};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

#[derive(Clone, Copy)]
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
    pub art: &'a str,  // swatch name (gradient fallback when no decoded cover)
    /// Real decoded cover art, pre-scaled by the shell: full-bleed 480×480 (day) and the
    /// 92×92 thumb (night header). None = draw the gradient fallback.
    pub art_full: Option<&'a art::Image>,
    pub art_thumb: Option<&'a art::Image>,
    pub liked: bool,
    pub playing: bool,
    pub shuffle: bool,
    pub repeat: u8, // 0 off · 1 all · 2 one
    pub viz_seed: f32, // visualiser animation phase (the shell advances it while playing)
    pub viz_kind: u8,  // which visualiser type (index into viz::from_index)
    pub viz_on: bool,  // master enable — false hides the visualiser entirely (nav injects UI state)
    pub viz_levels: Option<&'a [f32]>, // real per-bar spectrum (0..1) from FFT; None = synthetic
}

fn s(fam: Family, weight: Weight, size: f32, color: embedded_graphics::pixelcolor::Rgb888, tracking: f32) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

/// Small accent "SLEEP {n}M" badge, top-right under the status bar, shown while a sleep timer runs.
/// Drawn by the navigator AFTER render() (it owns the live countdown) — kept here to share the
/// screen's draw imports. `min` = 0 hides it.
pub fn sleep_badge(c: &mut Canvas, t: &Theme, f: &FontSet, min: u32) {
    if min == 0 {
        return;
    }
    let label = format!("SLEEP {}M", min);
    let st = s(Family::Mono, Weight::Bold, 12.0, t.acc_ink, 0.08);
    let w = text::measure(f, &label, &st) as i32 + 22;
    let h = 24;
    let x = 458 - w;
    let y = 44;
    fill_rect(c, x, y, w, h, t.acc);
    text::draw(c, f, (x + 11) as f32, (y + h / 2 + 4) as f32, &label, &st);
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, np.clock, np.badge, np.battery);
    let seed = np.viz_seed; // animated by the shell while playing; constant when paused/host

    if t.night {
        // compact header: 92px thumb @32% + title/artist/codec column
        match np.art_thumb {
            Some(img) => art::draw_image(c, t, 24, 80, img, 0.32),
            None => art::block(c, t, 24, 80, 92, 92, np.art, 0.32),
        }
        text::draw(c, f, 134.0, 110.0, np.title, &s(Family::Sans, Weight::Bold, 23.0, t.ink, 0.0));
        text::draw(c, f, 134.0, 133.0, np.artist, &s(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
        text::draw(c, f, 134.0, 153.0, np.codec, &s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08));
        // viz centred in the airy negative space (skipped entirely when the user turns it off)
        if np.viz_on {
            crate::viz::draw(c, 24, 420, 432, 16, 36, 3, seed, crate::viz::from_index(np.viz_kind), t.acc, t.line, np.viz_levels);
        }
    } else {
        // full-bleed album art (34..514): the real decoded cover when available
        match np.art_full {
            Some(img) => art::draw_image(c, t, 0, 34, img, 1.0),
            None => art::block(c, t, 0, 34, 480, 480, np.art, 1.0),
        }
        // visualiser pushed up onto the lower album art — frees room for bigger controls
        // (skipped entirely when off → the album art shows through cleanly)
        if np.viz_on {
            crate::viz::draw(c, 24, 466, 432, 42, 36, 3, seed, crate::viz::from_index(np.viz_kind), t.acc, t.line, np.viz_levels);
        }
        // title / artist / codec
        text::draw(c, f, 24.0, 558.0, np.title, &s(Family::Sans, Weight::Bold, 29.0, t.ink, 0.0));
        text::draw(c, f, 24.0, 583.0, np.artist, &s(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0));
        right(c, f, 456.0, 583.0, np.codec, &s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08));
    }

    // ---------- progress (shared) ----------
    let (py, px0, pw) = (612, 24, 432);
    fill_rect(c, px0, py, pw, 4, t.line);
    let fillw = (pw as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(c, px0, py, fillw, 4, t.acc);
    text::draw(c, f, 24.0, 636.0, np.elapsed, &s(Family::Mono, Weight::Regular, 13.0, t.dim, 0.0));
    right(c, f, 456.0, 636.0, np.remaining, &s(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));

    // ---------- transport (centre y 692, larger controls) ----------
    let ty = 692.0;
    icons::shuffle(c, 44.0, ty, 24.0, if np.shuffle { t.acc } else { t.faint });
    icons::prev(c, 128.0, ty, 38.0, t.ink);
    Circle::with_center(Point::new(240, ty as i32), 92)
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    if np.playing {
        icons::pause(c, 240.0, ty, 38.0, t.acc_ink);
    } else {
        icons::play(c, 240.0, ty, 38.0, t.acc_ink);
    }
    icons::next(c, 352.0, ty, 38.0, t.ink);
    icons::repeat(c, 436.0, ty, 24.0, if np.repeat > 0 { t.acc } else { t.faint });
    if np.repeat == 2 {
        // repeat-one dot
        fill_rect(c, 435, ty as i32 - 1, 3, 3, t.acc);
    }

    // ---------- bottom toolbar (744..800): library · queue · eq · bt · settings ----------
    // (nav::tap's five 96px slots mirror this order — keep them in sync)
    hline(c, 744, t.line);
    let tb = 774.0;
    icons::library(c, 48.0, tb, 26.0, t.dim);
    icons::queue(c, 144.0, tb, 26.0, t.dim);
    icons::eq(c, 240.0, tb, 26.0, t.dim);
    icons::bt(c, 336.0, tb, 25.0, t.dim);
    icons::settings(c, 432.0, tb, 26.0, t.dim);
}
