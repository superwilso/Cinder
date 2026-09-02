//! Small shared draw helpers used across screens — flat fills, 1px hairlines,
//! stroked borders, text alignment, and the Cinder toggle switch (`CToggle` /
//! the header on/off switches in cinder-proto-screens3/4.jsx).

use crate::canvas::{Canvas, W};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

/// Full-width 1px hairline at row `y`.
pub fn hline(c: &mut Canvas, y: i32, col: Rgb888) {
    fill_rect(c, 0, y, W as i32, 1, col);
}

/// Stroked (outline) rectangle.
pub fn stroke_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888, weight: u32) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_stroke(col, weight))
        .draw(c)
        .ok();
}

pub fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

/// Draw text right-aligned so it ends at `xr`.
/// The gutter both helpers keep clear of the panel edges — the same 22px every screen's left
/// margin uses, so clamped text lines up with the rest of the layout instead of touching the glass.
const EDGE: f32 = 22.0;

/// Draw text ending at `xr`.
///
/// The string is fitted to the space between the left gutter and `xr` first. Right-aligning by
/// measuring puts the START x at `xr - w`, so an over-long run silently walks off the LEFT edge —
/// nothing on this device scrolls sideways, so those pixels are gone. It is size-dependent, which
/// is why it only ever showed up at 130-140% UI scale.
pub fn right(c: &mut Canvas, f: &FontSet, xr: f32, baseline: f32, s: &str, st: &TextStyle) {
    let s = fit(f, s, st, (xr - EDGE).max(0.0));
    let w = text::measure(f, &s, st);
    text::draw(c, f, xr - w, baseline, &s, st);
}

/// Draw LEFT-aligned text that stops at `right`, ellipsising if it would not fit.
///
/// For static copy at a literal x. Such a line is only safe at ONE text size — the x and the
/// string are fixed but the glyphs grow with the UI-scale slider — so at 140% a caption laid out
/// to look comfortable at 100% runs off the panel and the tail is silently discarded. Returns the
/// end x, like `text::draw`.
pub fn draw_fit(c: &mut Canvas, f: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle,
                right: f32) -> f32 {
    let s = fit(f, s, st, (right - x).max(0.0));
    text::draw(c, f, x, baseline, &s, st)
}

/// Draw text horizontally centred on `cx`.
///
/// Fitted to the panel first: centred text wider than the screen overflows BOTH edges at once, and
/// the ellipsis is far better than losing the ends of the sentence. Same size-dependence as
/// `right` — these are the two helpers where the x is computed from the measured width rather than
/// being a layout constant, so they are exactly where a text-scale change turns into overflow.
pub fn center(c: &mut Canvas, f: &FontSet, cx: f32, baseline: f32, s: &str, st: &TextStyle) {
    // Symmetric about cx, so the fitted run stays centred on the anchor rather than drifting.
    let half = (cx - EDGE).min(crate::canvas::W as f32 - EDGE - cx).max(0.0);
    let s = fit(f, s, st, half * 2.0);
    let w = text::measure(f, &s, st);
    text::draw(c, f, cx - w / 2.0, baseline, &s, st);
}

/// Truncate `s` (with a trailing ellipsis) so it fits within `max_w` px.
/// Draw a LEFT label and a RIGHT value on the same baseline without ever letting them collide.
///
/// The right item is measured first and keeps its full width (it is the value — truncating
/// "FLAC 24-bit / 96.0 kHz" to "FLAC 24-bit / 96.0…" would be worse than shortening the artist);
/// the left item is then `fit()` into whatever is left, minus `gap`.
///
/// This exists because a fixed left x plus a fixed right edge is only safe at ONE text size. With
/// the UI-scale slider both runs grow, and at 140% the Now Playing artist ran straight through the
/// codec string in the middle of the line. Anywhere two runs share a baseline, they have to be
/// laid out from their MEASURED widths — the same single-source rule the tab strip and the lists
/// already follow.
#[allow(clippy::too_many_arguments)]
pub fn row_pair(
    c: &mut Canvas,
    f: &FontSet,
    left_x: f32,
    right_x: f32,
    baseline: f32,
    left: &str,
    left_st: &TextStyle,
    right: &str,
    right_st: &TextStyle,
    gap: f32,
) {
    let rw = if right.is_empty() { 0.0 } else { text::measure(f, right, right_st) };
    let avail = (right_x - rw - gap - left_x).max(0.0);
    text::draw(c, f, left_x, baseline, &fit(f, left, left_st, avail), left_st);
    if !right.is_empty() {
        self::right(c, f, right_x, baseline, right, right_st);
    }
}

// ── Marquee ─────────────────────────────────────────────────────────────────────────────────────
//
// `fit()` is the right answer for a list row: forty of them on screen, all animating, would be
// noise, and a truncated row is still enough to pick the one you want. It is the WRONG answer for
// the one line you are actually reading. Now Playing's title is fitted to 372px at 29pt, so
// "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364" reads
// "Sinfonia concertante for Violi…" and the movement — the part that says which track this is —
// is never visible at all. Reported 2026-09-02: long song names "not being readable on the screen
// because they overflow".
//
// So: the title scrolls, the way every hardware player including Sony's own does it. Ping-pong
// rather than wrap-around — dwell, slide to the end, dwell, slide back — because a wrap needs a
// gap and a second copy of the string to look right, and a snap back to the start reads as a
// glitch at this size.
//
// TIME COMES FROM THE SHELL, not from a field on `NowPlaying`. That struct is built in 28 places
// (the sim, the host harness, the device shell, the render bench, the overflow audit) and a
// required new field would be 28 edits for a value only this function reads — the same reasoning
// that already makes `text::set_scale_idx` a process global. The shell advances the phase once a
// frame; anything that never calls `set_marquee_ms` simply gets a static, fitted line, which is
// what every test wants anyway.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static MARQUEE_MS: AtomicU32 = AtomicU32::new(0);
static MARQUEE_LIVE: AtomicBool = AtomicBool::new(false);

/// Advance the marquee clock. Monotonic milliseconds; the shell calls this once a frame.
pub fn set_marquee_ms(ms: u32) {
    MARQUEE_MS.store(ms, Ordering::Relaxed);
}

/// Did anything actually SCROLL since this was last called? Read-and-clear.
///
/// The renderer is dirty-flag gated, so a scrolling line has to ask for the next frame or it
/// paints once and stops. Asking only when something is really moving is the point: a title that
/// fits sets nothing, and Now Playing stays as cheap as it was. Text that fits must never keep the
/// panel repainting — that is the cost the screen-off timer exists to avoid.
pub fn marquee_scrolled() -> bool {
    MARQUEE_LIVE.swap(false, Ordering::Relaxed)
}

/// Pause at each end before reversing, ms. Long enough to read the end of a title.
const MARQUEE_DWELL_MS: u32 = 1500;
/// Scroll speed. Slow enough to read at 29pt, fast enough that a 90-character title is not a
/// twenty-second round trip.
const MARQUEE_PX_PER_S: f32 = 32.0;

/// Draw left-aligned text in a `max_w` box, scrolling it horizontally when it does not fit.
///
/// Returns the width actually occupied. A string that fits is drawn exactly as `text::draw` would
/// draw it, with no clip band and no repaint request — the fitting case has to stay free.
pub fn marquee(c: &mut Canvas, f: &FontSet, x: f32, baseline: f32, s: &str, st: &TextStyle,
               max_w: f32) -> f32 {
    // No room at all: draw NOTHING. The early version folded this into the "it fits" branch and
    // drew the whole string unclipped, which at 140% (where the codec eats the artist's entire
    // share of the baseline) put 3,187px of text across the panel and off the right edge.
    if max_w <= 0.0 {
        return 0.0;
    }
    let tw = text::measure(f, s, st);
    if tw <= max_w {
        text::draw(c, f, x, baseline, s, st);
        return tw;
    }
    let travel = tw - max_w;
    let scroll_ms = (travel / MARQUEE_PX_PER_S * 1000.0).max(1.0) as u32;
    // dwell, out, dwell, back
    let half = MARQUEE_DWELL_MS + scroll_ms;
    let t = MARQUEE_MS.load(Ordering::Relaxed) % (half * 2);
    let off = if t < MARQUEE_DWELL_MS {
        0.0
    } else if t < half {
        (t - MARQUEE_DWELL_MS) as f32 / scroll_ms as f32 * travel
    } else if t < half + MARQUEE_DWELL_MS {
        travel
    } else {
        (1.0 - (t - half - MARQUEE_DWELL_MS) as f32 / scroll_ms as f32) * travel
    };
    // Claim the next frame for as long as the line OVERFLOWS, dwell included. Skipping the dwells
    // to save two repaints a cycle looks like an easy win and is a deadlock: the renderer is
    // dirty-gated, so the frame that ends a dwell is itself a frame nobody asked for, and the
    // marquee stops at the end of its first slide until something unrelated happens to repaint.
    // An animation has to keep asking while it is running, not only while it is moving.
    MARQUEE_LIVE.store(true, Ordering::Relaxed);
    c.set_clip_x(x as i32, (x + max_w).ceil() as i32);
    text::draw(c, f, x - off, baseline, s, st);
    c.clear_clip_x();
    max_w
}

pub fn fit(f: &FontSet, s: &str, st: &TextStyle, max_w: f32) -> String {
    if text::measure(f, s, st) <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let trial = format!("{}{}…", out, ch);
        if text::measure(f, &trial, st) > max_w {
            break;
        }
        out.push(ch);
    }
    format!("{}…", out)
}

/// Cinder toggle switch: a 1px box with a square knob, accent when on.
pub fn toggle(c: &mut Canvas, t: &Theme, x: i32, y: i32, w: i32, h: i32, knob: i32, on: bool) {
    stroke_rect(c, x, y, w, h, if on { t.acc } else { t.line }, 1);
    let inset = (h - knob) / 2;
    let kx = if on { x + w - inset - knob } else { x + inset };
    let ky = y + inset;
    fill_rect(c, kx, ky, knob, knob, if on { t.acc } else { t.faint });
}

/// Static visualiser bar strip (`FBars`): deterministic heights, every 4th bar
/// accent. Bars are bottom-aligned within the (x,y,w,h) box.
pub fn bars(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, n: i32, gap: i32, seed: f32, acc: Rgb888, dim: Rgb888) {
    let bw = ((w - gap * (n - 1)) / n).max(1);
    for i in 0..n {
        let v = 0.18 + 0.82 * (i as f32 * 1.93 + seed * 2.7).sin().abs();
        let bh = ((v * h as f32).round() as i32).max(2);
        let bx = x + i * (bw + gap);
        let col = if i % 4 == 0 { acc } else { dim };
        fill_rect(c, bx, y + h - bh, bw, bh, col);
    }
}

/// A mono-caption "pill": bordered box, accent fill when `on`. Returns its width.
pub fn pill(c: &mut Canvas, f: &FontSet, t: &Theme, x: i32, y: i32, h: i32, label: &str, on: bool) -> i32 {
    let st = sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc_ink } else { t.dim }, 0.08);
    let tw = text::measure(f, label, &st);
    let w = tw as i32 + 24;
    if on {
        fill_rect(c, x, y, w, h, t.acc);
    }
    stroke_rect(c, x, y, w, h, if on { t.acc } else { t.line }, 1);
    text::draw(c, f, (x + 12) as f32, (y + h / 2 + 4) as f32, label, &st);
    w
}

/// Indeterminate spinner: eight dots on a ring, with a bright head that advances with `phase`
/// (seconds) and a fading tail behind it.
///
/// Motion is the whole point. The Devices screen already printed a static "CONNECTING…" while a
/// link attempt was in flight, and static text is exactly what a *stalled* attempt looks like — the
/// user cannot tell "working on it" from "wedged". Anything that can take seconds and can fail needs
/// to visibly tick.
///
/// Drawn with `blend` only, so the tail fade costs nothing extra and it works on every backend. The
/// caller owns `phase`; nav advances it from real elapsed time and repaints while it moves.
pub fn spinner(c: &mut Canvas, cx: i32, cy: i32, r: i32, dot: i32, phase: f32, col: Rgb888) {
    const N: i32 = 8;
    let head = ((phase * 8.0) as i32).rem_euclid(N);
    for i in 0..N {
        let a = i as f32 * core::f32::consts::PI * 2.0 / N as f32 - core::f32::consts::FRAC_PI_2;
        let px = cx + (a.cos() * r as f32).round() as i32;
        let py = cy + (a.sin() * r as f32).round() as i32;
        // How far this dot sits BEHIND the head, so the trail fades backwards around the ring.
        let back = (head - i).rem_euclid(N);
        let alpha: u8 = match back {
            0 => 255,
            1 => 200,
            2 => 150,
            3 => 100,
            4 => 60,
            _ => 32,
        };
        for dy in 0..dot {
            for dx in 0..dot {
                c.blend(px + dx - dot / 2, py + dy - dot / 2, col, alpha);
            }
        }
    }
}

#[cfg(test)]
mod marquee_tests {
    use super::*;
    use crate::canvas::Canvas;

    fn fonts() -> FontSet {
        FontSet::load()
    }
    fn style() -> TextStyle {
        sty(Family::Sans, Weight::Bold, 29.0, Rgb888::new(255, 255, 255), 0.0)
    }

    /// A short line is drawn exactly as before and asks for NOTHING: no clip band, no repaint.
    /// This is the case that must stay free — most titles fit, and a marquee that kept the panel
    /// repainting for them would be a battery regression disguised as a feature.
    #[test]
    fn text_that_fits_does_not_animate() {
        let f = fonts();
        let mut c = Canvas::new();
        marquee_scrolled(); // clear anything a neighbouring test left
        set_marquee_ms(0);
        let w = marquee(&mut c, &f, 24.0, 100.0, "Atlas Hands", &style(), 372.0);
        assert!(w > 0.0 && w <= 372.0, "a fitting line reports its real width");
        assert!(!marquee_scrolled(), "a line that fits must not request repaints");
    }

    /// A long line scrolls, and asks for the next frame while it does.
    #[test]
    fn long_text_scrolls_and_requests_repaints() {
        let f = fonts();
        let mut c = Canvas::new();
        let long = "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto";
        marquee_scrolled();
        set_marquee_ms(0);
        marquee(&mut c, &f, 24.0, 100.0, long, &style(), 372.0);
        assert!(marquee_scrolled(), "an overflowing line drives its own animation");
    }

    /// The scrolled run is CUT OFF at the box, not drawn across the panel.
    ///
    /// This is the whole reason `Canvas` grew a horizontal clip band. Without it the tail of a
    /// long title slides over the artist, the codec and the right margin — and because every
    /// write is clipped to the framebuffer anyway, it would have looked merely "a bit odd" rather
    /// than wrong. Measured as pixels written outside the box.
    #[test]
    fn scrolled_text_stays_inside_its_box() {
        let f = fonts();
        let long = "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto";
        let (x, boxw) = (24.0f32, 372.0f32);
        // Mid-slide, where the run is at its widest inside the box.
        set_marquee_ms(MARQUEE_DWELL_MS + 400);
        let mut c = Canvas::new();
        c.fill(Rgb888::new(0, 0, 0));
        marquee(&mut c, &f, x, 100.0, long, &style(), boxw);
        let mut outside = 0u32;
        let mut inside = 0u32;
        for y in 0..crate::canvas::H {
            for px in 0..W {
                if c.buf[y * W + px] != 0 {
                    if (px as f32) < x || (px as f32) >= x + boxw {
                        outside += 1;
                    } else {
                        inside += 1;
                    }
                }
            }
        }
        assert!(inside > 0, "the title should actually be drawn");
        assert_eq!(outside, 0, "{outside} px of title escaped its box");
    }

    /// The slide actually moves: the same string at two phases is not the same picture.
    #[test]
    fn the_phase_moves_the_text() {
        let f = fonts();
        let long = "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto";
        let shot = |ms: u32| {
            set_marquee_ms(ms);
            let mut c = Canvas::new();
            c.fill(Rgb888::new(0, 0, 0));
            marquee(&mut c, &f, 24.0, 100.0, long, &style(), 372.0);
            c.buf.clone()
        };
        let a = shot(MARQUEE_DWELL_MS / 2); // inside the opening dwell
        let b = shot(MARQUEE_DWELL_MS / 3); // also inside it — must be identical
        let d = shot(MARQUEE_DWELL_MS + 900); // mid-slide — must differ
        assert_eq!(a, b, "the text must be still during the dwell");
        assert_ne!(a, d, "the text must have moved once the dwell ends");
    }

    /// A box with no room left draws nothing rather than spilling the whole string.
    /// At 140% UI scale the codec claims the entire artist baseline, and the first version of this
    /// helper treated "no room" as "it fits" — 3,187 px of artist across the panel.
    #[test]
    fn no_room_draws_nothing() {
        let f = fonts();
        let mut c = Canvas::new();
        c.fill(Rgb888::new(0, 0, 0));
        let w = marquee(&mut c, &f, 24.0, 100.0, "A Very Long Guest Artist Name", &style(), 0.0);
        assert_eq!(w, 0.0);
        assert!(c.buf.iter().all(|&p| p == 0), "nothing may be drawn into a zero-width box");
    }
}
