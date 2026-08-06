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
    /// The user's finger is on the progress rail right now (nav sets this while scrubbing):
    /// `progress` is the LIVE preview position, and the knob swells for feedback.
    pub scrubbing: bool,
}

fn s(fam: Family, weight: Weight, size: f32, color: embedded_graphics::pixelcolor::Rgb888, tracking: f32) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

// ── Progress rail = the SCRUB control (shared by render + hit) ───────────────────────────────
// The rail used to be a 4px decorative line with no tap target anywhere on the screen, and the
// only "go back" control was the ◁ key → PlayController::PrevTrack(). At the head of a queue
// PrevTrack has nothing to go to, so ◁ did *nothing* — "no rewind in some queue situations".
// The rail is now the primary rewind affordance: tap or drag it to seek. It gets a knob so it
// reads as draggable and a generous 36px-tall touch band (the drawn rail stays slim).
pub const RAIL_Y: i32 = 612;
pub const RAIL_X0: i32 = 24;
pub const RAIL_W: i32 = 432;
const RAIL_H: i32 = 6;
/// Touch band around the rail (44px-class target; the rail itself is 6px).
const RAIL_HIT_TOP: i32 = RAIL_Y - 18;
const RAIL_HIT_BOT: i32 = RAIL_Y + 24;

/// Does (x, y) land on the progress rail? Returns the scrub position in permille (0..1000).
/// x is clamped to the rail, so a slightly-off grab still starts at a sane place.
pub fn hit_progress(x: i32, y: i32) -> Option<u16> {
    if !(RAIL_HIT_TOP..RAIL_HIT_BOT).contains(&y) {
        return None;
    }
    // Horizontal slop matches the rail's own margins — the whole width of the screen at this
    // height is the control (there is nothing else on that row).
    Some(permille_at(x))
}

/// Map an x coordinate to a permille position along the rail (clamped at both ends).
pub fn permille_at(x: i32) -> u16 {
    let dx = (x - RAIL_X0).clamp(0, RAIL_W);
    ((dx as i64 * 1000) / RAIL_W as i64) as u16
}

/// Draw the rail at `progress` (0..1). `scrubbing` swells the knob so the finger has feedback.
fn rail(c: &mut Canvas, t: &Theme, progress: f32, scrubbing: bool) {
    let p = progress.clamp(0.0, 1.0);
    fill_rect(c, RAIL_X0, RAIL_Y, RAIL_W, RAIL_H, t.line);
    let fillw = (RAIL_W as f32 * p) as i32;
    fill_rect(c, RAIL_X0, RAIL_Y, fillw, RAIL_H, t.acc);
    // Knob: the "this is draggable" cue. Square (the design language is flat rectangles).
    let k = if scrubbing { 18 } else { 12 };
    let kx = (RAIL_X0 + fillw - k / 2).clamp(RAIL_X0 - k / 2, RAIL_X0 + RAIL_W - k / 2);
    fill_rect(c, kx, RAIL_Y + RAIL_H / 2 - k / 2, k, k, t.acc);
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
    crate::chrome::status_bar(c, t, f);
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

    // ---------- progress (shared) — tap/drag to seek ----------
    rail(c, t, np.progress, np.scrubbing);
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
    rail(c, t, 0.0, false);

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
