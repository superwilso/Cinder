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

// ── Progress-rail geometry — THE single source ────────────────────────────────────────────────
// Both renders below (loaded + idle) and the drag-to-seek hit test read these. The rail used to be
// literal numbers in three places; a scrub target that disagrees with the drawn bar by even a few
// pixels feels broken, so they live here and nowhere else.
pub const RAIL_Y: i32 = 612;
pub const RAIL_X0: i32 = 24;
pub const RAIL_W: i32 = 432;
/// Rail thickness. Single source for both the track and the filled portion.
pub const RAIL_H: i32 = 6;
/// Vertical grab band for drag-to-seek. The rail itself is 4 px tall — unhittable with a thumb —
/// so the band spans from just above the rail down through the elapsed/remaining labels. It stops
/// short of the transport row (centre y 692, radius 44 ⇒ from 648) so it can never steal a
/// play/pause tap.
pub const RAIL_GRAB_TOP: i32 = 594;
pub const RAIL_GRAB_BOT: i32 = 646;

/// Like (heart) glyph centre, and the half-size of its touch target. Single source for the draw
/// above and the hit test in nav.
pub const HEART_CX: i32 = 432;
pub const HEART_CY: i32 = 548;
pub const HEART_HALF: i32 = 30;

/// Did a tap land on the like heart?
pub fn hit_heart(x: i32, y: i32) -> bool {
    (x - HEART_CX).abs() <= HEART_HALF && (y - HEART_CY).abs() <= HEART_HALF
}

/// Map a UI x coordinate to a 0..1 position along the rail (clamped). Used by the scrub.
pub fn rail_fraction(x: i32) -> f32 {
    ((x - RAIL_X0) as f32 / RAIL_W as f32).clamp(0.0, 1.0)
}

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
    /// How much room the visualiser gets, as a `viz::VizSize` index (0 = OFF). Replaced the old
    /// on/off flag: on the day theme the visualiser is drawn OVER the album art, so "how much"
    /// is the question that actually matters, and off is just the smallest answer.
    pub viz_size: u8,
    pub viz_levels: Option<&'a [f32]>, // real per-bar spectrum (0..1) from FFT; None = synthetic
    /// Which Now Playing PAGE is showing (index into `NpPage`). Only the block above the title
    /// changes — the title, progress, transport and toolbar are identical on every page, so the
    /// controls never move under your thumb.
    pub page: u8,
    /// A drag-to-seek is in progress: `progress`/`elapsed`/`remaining` show the pending TARGET
    /// rather than the live position, and the rail grows a handle under the finger.
    pub scrubbing: bool,
}

/// The pages you swipe between on Now Playing. The visualiser used to be painted ON the cover,
/// which meant every choice was a compromise between seeing the artwork and seeing the audio. As
/// pages they stop competing: the cover page is the cover, and the visualiser gets a whole block
/// to itself instead of a strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NpPage {
    /// The album cover, full bleed. Optionally with a small visualiser (see `viz_size`).
    Cover,
    /// The spectrum, given the entire art block. Style follows the Visualiser type setting.
    Spectrum,
    /// Output level: one large meter with a peak marker. No per-band detail — the calm page.
    Level,
}

pub const PAGES: u8 = 3;

pub fn page_from_index(i: u8) -> NpPage {
    match i % PAGES {
        0 => NpPage::Cover,
        1 => NpPage::Spectrum,
        _ => NpPage::Level,
    }
}

/// The block that pages: the full-bleed art area on the day theme.
pub const PAGE_TOP: i32 = 34;
pub const PAGE_BOT: i32 = 514;
/// A horizontal swipe ABOVE this y flips the page; below it, it still skips tracks. Splitting the
/// gesture by zone keeps both: you swipe the artwork to turn it, and the controls area to change
/// what is playing. (The physical FF/REW keys skip from anywhere regardless, and they are the
/// primary skip affordance on a device with no d-pad.)
pub const PAGE_SWIPE_BOT: i32 = PAGE_BOT;

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

/// Page indicator: one dot per page, in the strip between the paging block and the title. Small
/// and faint — it is a "there is more this way" hint, not a control. Without it the pages would be
/// undiscoverable, which is the usual way a swipe-only feature ends up never being found.
fn page_dots(c: &mut Canvas, t: &Theme, page: u8) {
    const D: i32 = 6;
    const GAP: i32 = 10;
    let total = PAGES as i32 * D + (PAGES as i32 - 1) * GAP;
    let x0 = 240 - total / 2;
    for i in 0..PAGES {
        let x = x0 + i as i32 * (D + GAP);
        let col = if i == page % PAGES { t.acc } else { t.faint };
        fill_rect(c, x, 524, D, D, col);
    }
}

/// Mean and peak of the current spectrum, 0..1. `None` levels (no analyzer running) give zeros,
/// so both audio pages render flat and empty rather than inventing motion.
fn level_stats(np: &NowPlaying) -> (f32, f32) {
    match np.viz_levels {
        Some(l) if !l.is_empty() => {
            let sum: f32 = l.iter().sum();
            let peak = l.iter().cloned().fold(0.0f32, f32::max);
            ((sum / l.len() as f32).clamp(0.0, 1.0), peak.clamp(0.0, 1.0))
        }
        _ => (0.0, 0.0),
    }
}

/// PAGE 2 — the spectrum, given the whole block instead of a strip. Same styles as the cover
/// overlay, just with room: this is where a visualiser is worth looking at.
fn spectrum_page(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, seed: f32) {
    let (x, w) = (24, 432);
    let (y, h) = (154, 348); // stands at 502, clear of the page dots at 524
    if np.viz_levels.is_some() {
        crate::viz::draw(c, x, y, w, h, 36, 3, seed, crate::viz::from_index(np.viz_kind),
                         t.acc, t.line, np.viz_levels, 255, 255);
    } else {
        // No analyzer feeding us. Say so rather than drawing a still, empty graph that reads as a
        // broken screen — the same rule the rest of the app follows about showing what isn't there.
        crate::widgets::center(c, f, 240.0, 330.0, "No audio signal",
            &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0));
        crate::widgets::center(c, f, 240.0, 356.0, "PLAY SOMETHING TO SEE THE SPECTRUM",
            &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    }
    crate::widgets::center(c, f, 240.0, 130.0, crate::viz::name_upper(np.viz_kind),
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
}

/// PAGE 2, night layout. Same page, different block: the compact header occupies the top ~160px,
/// so the spectrum takes the open space beneath it rather than a cover's footprint.
fn spectrum_page_night(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, seed: f32) {
    let (x, w) = (24, 432);
    let (y, h) = (220, 260); // stands at 480, clear of the page dots
    if np.viz_levels.is_some() {
        crate::viz::draw(c, x, y, w, h, 36, 3, seed, crate::viz::from_index(np.viz_kind),
                         t.acc, t.line, np.viz_levels, 255, 255);
    } else {
        crate::widgets::center(c, f, 240.0, 340.0, "No audio signal",
            &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0));
    }
    crate::widgets::center(c, f, 240.0, 196.0, crate::viz::name_upper(np.viz_kind),
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
}

/// Decimal for 0..=999 into a caller-supplied buffer. Allocation-free, and clamped rather than
/// fallible so a nonsense level can never panic a render (a panic here aborts, and an abort on this
/// device is a reboot into stock).
fn dec(v: i32, buf: &mut [u8; 3]) -> &str {
    let v = v.clamp(0, 999) as u32;
    let mut n = 0;
    if v >= 100 {
        buf[n] = b'0' + (v / 100) as u8;
        n += 1;
    }
    if v >= 10 {
        buf[n] = b'0' + (v / 10 % 10) as u8;
        n += 1;
    }
    buf[n] = b'0' + (v % 10) as u8;
    n += 1;
    core::str::from_utf8(&buf[..n]).unwrap_or("0")
}

/// "PEAK nnn", allocation-free.
fn peak_label(v: i32, buf: &mut [u8; 8]) -> &str {
    buf[..5].copy_from_slice(b"PEAK ");
    let mut d = [0u8; 3];
    let s = dec(v, &mut d);
    let n = 5 + s.len();
    buf[5..n].copy_from_slice(s.as_bytes());
    core::str::from_utf8(&buf[..n]).unwrap_or("PEAK")
}

/// PAGE 3 — output level. One big meter, a peak marker, and a scale. No per-band detail: this is
/// the page for when you want to see that it is playing without anything asking for attention.
fn level_page(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    let (mean, peak) = level_stats(np);
    let (x, w) = (36, 408);
    let (y, h) = (270, 64);

    // The night layout puts the track header at the top of the screen, where the day layout has
    // artwork — so this caption has to move out from under it rather than sit at a fixed y.
    let label_y = if t.night { 200.0 } else { 130.0 };
    crate::widgets::center(c, f, 240.0, label_y, "OUTPUT LEVEL",
        &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));

    // Track, then fill. A visible empty track is what makes the fill mean something — a bare bar
    // on a black screen has no scale to be read against.
    fill_rect(c, x, y, w, h, t.panel);
    crate::widgets::stroke_rect(c, x, y, w, h, t.line, 1);
    let fw = (w as f32 * mean).round() as i32;
    if fw > 0 {
        fill_rect(c, x, y, fw, h, t.acc);
    }
    // Peak marker: a 3px rule at the loudest band. Peak sits at or right of the mean by definition,
    // so it never hides inside the fill.
    let px = x + ((w - 3) as f32 * peak).round() as i32;
    fill_rect(c, px, y - 8, 3, h + 16, if peak > 0.0 { t.ink } else { t.line });

    // Scale ticks under the meter, at tenths. Every fifth is full height.
    for i in 0..=10 {
        let tx = x + (w - 1) * i / 10;
        let th = if i % 5 == 0 { 10 } else { 5 };
        fill_rect(c, tx, y + h + 8, 1, th, t.faint);
    }

    // The numbers, big, in the space below. Mono so they do not jitter as the digits change —
    // proportional figures would make the whole line dance at 20 fps.
    // Stack-formatted, not `format!`. This page redraws at ~20 fps while playing, and two heap
    // allocations a frame is churn this device has already been bitten by once — the per-frame
    // Canvas allocation that ended in an allocator abort, and an abort here means a reboot.
    let mut mb = [0u8; 3];
    crate::widgets::center(c, f, 240.0, 430.0, dec((mean * 100.0).round() as i32, &mut mb),
        &s(Family::Mono, Weight::Bold, 56.0, t.ink, 0.0));
    let mut pb = [0u8; 8];
    crate::widgets::center(c, f, 240.0, 460.0, peak_label((peak * 100.0).round() as i32, &mut pb),
        &s(Family::Mono, Weight::Regular, 12.0, t.faint, 0.14));
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    c.fill(t.bg);
    let seed = np.viz_seed; // animated by the shell while playing; constant when paused/host

    // Nothing loaded — the state the device sits in from boot until the first track is picked.
    // Falling through would draw an art block seeded from an empty string (an orphan coloured
    // square) above three empty text runs, which is what the device actually showed.
    if np.title.is_empty() && np.artist.is_empty() {
        let cy = if t.night { 120.0 } else { 274.0 };
        crate::widgets::center(c, f, 240.0, cy, "Nothing playing",
            &s(Family::Sans, Weight::Regular, 22.0, t.dim, 0.0));
        crate::widgets::center(c, f, 240.0, cy + 26.0, "CHOOSE A TRACK FROM YOUR LIBRARY",
            &s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
        idle_chrome(c, t, f);
        return;
    }

    if t.night {
        // compact header: 92px thumb @32% + title/artist/codec column
        match np.art_thumb {
            Some(img) => art::draw_image(c, t, 24, 80, img, 0.32),
            None => art::block(c, t, 24, 80, 92, 92, np.art, 0.32),
        }
        text::draw(c, f, 134.0, 110.0, np.title, &s(Family::Sans, Weight::Bold, 23.0, t.ink, 0.0));
        text::draw(c, f, 134.0, 133.0, np.artist, &s(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
        text::draw(c, f, 134.0, 153.0, np.codec, &s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08));
        // Night pages the same way the day theme does, but the block is different: there is no
        // full-bleed cover to page, only the airy negative space under the compact header. So the
        // header stays put and the SPACE changes. Swiping still works, the dots still say where you
        // are, and nothing about the gesture has to be relearned when the theme flips.
        //
        // Everything here inherits the night palette, whose accent is already taken down to ~55%
        // luminance — so the spectrum page at night is a dim spectrum, not a bright one. That is
        // the point of the theme and the visualiser does not get an exemption from it.
        match page_from_index(np.page) {
            NpPage::Cover => {
                // The small visualiser, standing on y=436 in the open space — over nothing, so at
                // night this size axis is about restraint rather than about hiding artwork.
                if let Some((vy, vh, at, ab)) =
                    crate::viz::size_box(crate::viz::size_from_index(np.viz_size), 436, true)
                {
                    crate::viz::draw(c, 24, vy, 432, vh, 36, 3, seed,
                                     crate::viz::from_index(np.viz_kind), t.acc, t.line,
                                     np.viz_levels, at, ab);
                }
            }
            NpPage::Spectrum => spectrum_page_night(c, t, f, np, seed),
            NpPage::Level => level_page(c, t, f, np),
        }
        page_dots(c, t, np.page);
    } else {
        // The PAGING BLOCK (34..514). Only this changes between pages; everything below it is
        // identical on every page, so the transport never moves under your thumb when you turn one.
        match page_from_index(np.page) {
            NpPage::Cover => {
                // full-bleed album art: the real decoded cover when available
                match np.art_full {
                    Some(img) => art::draw_image(c, t, 0, 34, img, 1.0),
                    None => art::block(c, t, 0, 34, 480, 480, np.art, 1.0),
                }
                // The visualiser stands on the BOTTOM EDGE of the cover (y=508, six px clear of
                // the art's real edge at 514) and grows upward into it, so changing size moves
                // only its top and the cover's composition below never shifts.
                let vsize = crate::viz::size_from_index(np.viz_size);
                if let Some((vy, vh, at, ab)) = crate::viz::size_box(vsize, 508, false) {
                    crate::viz::draw(c, 24, vy, 432, vh, 36, 3, seed,
                                     crate::viz::from_index(np.viz_kind), t.acc, t.line,
                                     np.viz_levels, at, ab);
                }
            }
            NpPage::Spectrum => spectrum_page(c, t, f, np, seed),
            NpPage::Level => level_page(c, t, f, np),
        }
        page_dots(c, t, np.page);
        // title / artist / codec
        text::draw(c, f, 24.0, 558.0, &crate::widgets::fit(f, np.title, &s(Family::Sans, Weight::Bold, 29.0, t.ink, 0.0), 372.0), &s(Family::Sans, Weight::Bold, 29.0, t.ink, 0.0));
        text::draw(c, f, 24.0, 583.0, np.artist, &s(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0));
        right(c, f, 456.0, 583.0, np.codec, &s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.08));
    }

    // ---------- like (heart) ----------
    // Wired at last: `liked` and icons::heart existed but nothing ever drew the glyph, so the
    // field was carried through four crates for an invisible feature. Sits on the title row, which
    // is where the eye already is and clear of every transport target.
    icons::heart(c, HEART_CX as f32, HEART_CY as f32, 26.0,
                 if np.liked { t.acc } else { t.faint });

    // ---------- progress (shared) ----------
    let (py, px0, pw) = (RAIL_Y, RAIL_X0, RAIL_W);
    fill_rect(c, px0, py, pw, RAIL_H, t.line);
    let fillw = (pw as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(c, px0, py, fillw, RAIL_H, t.acc);
    // Scrub handle: only while a drag-to-seek is in progress. It gives the finger something to
    // aim at and makes it obvious the bar is showing a pending target, not the live position.
    if np.scrubbing {
        let cx = px0 + fillw;
        Circle::with_center(Point::new(cx, py + RAIL_H / 2), 22)
            .into_styled(PrimitiveStyle::with_fill(t.acc))
            .draw(c)
            .ok();
    }
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

/// Progress rail + transport + toolbar for the idle screen. Geometry is duplicated from `render`
/// deliberately: every one of these is a TAP TARGET that `nav::tap` resolves by coordinate, so the
/// idle screen has to put them in exactly the same places or the controls stop working when
/// nothing is loaded. Only the *state* differs — empty rail, no times, transport shows play.
fn idle_chrome(c: &mut Canvas, t: &Theme, f: &FontSet) {
    fill_rect(c, RAIL_X0, RAIL_Y, RAIL_W, RAIL_H, t.line);

    let ty = 692.0;
    icons::shuffle(c, 44.0, ty, 24.0, t.faint);
    icons::prev(c, 128.0, ty, 38.0, t.faint);
    Circle::with_center(Point::new(240, ty as i32), 92)
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    icons::play(c, 240.0, ty, 38.0, t.acc_ink);
    icons::next(c, 352.0, ty, 38.0, t.faint);
    icons::repeat(c, 436.0, ty, 24.0, t.faint);

    hline(c, 744, t.line);
    let tb = 774.0;
    icons::library(c, 48.0, tb, 26.0, t.acc); // the one thing worth tapping from here
    icons::queue(c, 144.0, tb, 26.0, t.dim);
    icons::eq(c, 240.0, tb, 26.0, t.dim);
    icons::bt(c, 336.0, tb, 25.0, t.dim);
    icons::settings(c, 432.0, tb, 26.0, t.dim);
    let _ = f;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn np_with(page: u8, viz_size: u8, levels: &[f32]) -> NowPlaying<'_> {
        NowPlaying {
            title: "Atlas Hands",
            artist: "Benjamin Francis Leftwich",
            codec: "FLAC · 24bit / 96.0 kHz",
            badge: "FLAC 24/96",
            clock: "14:32",
            battery: 78,
            elapsed: "1:47",
            remaining: "-2:45",
            progress: 0.39,
            art: "atlas hands",
            art_full: None,
            art_thumb: None,
            liked: false,
            playing: true,
            shuffle: false,
            repeat: 0,
            viz_seed: 2.0,
            viz_kind: 0,
            viz_size,
            viz_levels: Some(levels),
            page,
            scrubbing: false,
        }
    }

    /// How many pixels of the paging block differ between two frames. Counting *accent* pixels
    /// would not work: the translucent sizes blend with the artwork, so none of their pixels is
    /// ever exactly `t.acc`. Differencing against a known-clean frame measures the thing that
    /// actually matters — did anything land on the cover.
    fn art_pixels_differing(a: &Canvas, b: &Canvas) -> usize {
        let mut n = 0;
        for y in PAGE_TOP..PAGE_BOT {
            for x in 0..crate::canvas::W as i32 {
                let i = (y as usize) * crate::canvas::W + x as usize;
                if a.buf[i] != b.buf[i] {
                    n += 1;
                }
            }
        }
        n
    }

    /// A frame of just the cover, with nothing drawn over it — the baseline everything else is
    /// compared against.
    fn bare_cover(t: &Theme, f: &FontSet, levels: &[f32]) -> Canvas {
        let mut c = Canvas::new();
        render(&mut c, t, f, &np_with(0, 0, levels));
        c
    }

    /// "Cover visualiser · OFF" must mean a genuinely untouched cover — nothing drawn over the
    /// artwork at all, not a smaller or relocated visualiser. An earlier design had a BELOW ART
    /// option that satisfied "off the album art" without satisfying "none", and the label has to
    /// keep meaning the stronger thing.
    #[test]
    fn cover_visualiser_off_leaves_the_artwork_completely_clean() {
        let t = Theme::day();
        let f = FontSet::load();
        let levels: Vec<f32> = (0..36).map(|i| 0.2 + 0.7 * (i as f32 / 36.0)).collect();

        // OFF must be pixel-identical to a cover with no visualiser concept at all: render it
        // twice with wildly different spectrum data and require the art block to be the same.
        let clean = bare_cover(&t, &f, &levels);
        let loud: Vec<f32> = vec![1.0; 36];
        let mut clean_loud = Canvas::new();
        render(&mut clean_loud, &t, &f, &np_with(0, 0, &loud));
        assert_eq!(
            art_pixels_differing(&clean, &clean_loud), 0,
            "OFF let the audio change the cover — something is still being drawn"
        );

        // …and the other sizes really do draw, or the assertion above proves nothing.
        for size in 1..crate::viz::SIZE_COUNT {
            let mut c = Canvas::new();
            render(&mut c, &t, &f, &np_with(0, size, &levels));
            assert!(art_pixels_differing(&clean, &c) > 100, "size {size} drew nothing at all");
        }
    }

    /// Turning the cover visualiser off must not touch the pages — the spectrum page still draws.
    /// That is the whole reason the row is named "Cover visualiser" rather than "Visualiser".
    #[test]
    fn the_spectrum_page_still_draws_with_the_cover_visualiser_off() {
        let t = Theme::day();
        let f = FontSet::load();
        let levels: Vec<f32> = (0..36).map(|i| 0.2 + 0.7 * (i as f32 / 36.0)).collect();
        let clean = bare_cover(&t, &f, &levels);
        let mut c = Canvas::new();
        render(&mut c, &t, &f, &np_with(1, 0, &levels));
        assert!(
            art_pixels_differing(&clean, &c) > 100,
            "the spectrum page went blank because the cover visualiser was off"
        );
    }
}
