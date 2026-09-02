//! Shared chrome — the status bar (`CStatus` in cinder-proto-screens1.jsx),
//! used by every screen. Left: clock + codec badge + NIGHT. Right: menu ·
//! bookmark · bt · battery.

use crate::canvas::Canvas;
use crate::icons;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

/// Height of the status strip. It is one big touch target (see [`status_hit`]), so this is a
/// TOUCH dimension first and a layout one second: 44px is the smallest comfortable target, and the
/// old 34 made the top-right glyphs genuinely hard to hit. Everything below it starts at
/// [`HEADER_BOTTOM`], which is unchanged — the strip grew into empty space above the back chevron.
pub const STATUS_H: i32 = 44;

/// A few pixels along the BOTTOM of the status strip that the strip does not claim.
///
/// The strip is hit-tested before any screen and its fallback is "anywhere else → the Menu", so it
/// used to own every pixel right down to its edge — and every header control below starts at that
/// same edge: the back chevron on every screen (`nav.rs`, x < 80) and the Bluetooth ON/OFF switch.
/// A tap a pixel or two high therefore did not MISS the control, it navigated: to the Menu on the
/// right, to Now Playing on the left. Reported 2026-08-19, first against the Bluetooth switch and
/// then — correctly — as affecting "lots of the back buttons".
///
/// Fixing it here rather than in each control fixes all of them at once, and costs nothing that
/// matters: the strip keeps 38 of its 44 px, its glyphs are centred far from this edge, and a tap
/// in the gap now does nothing instead of doing the wrong thing. Losing your place is worse than
/// having to tap again.
pub const STATUS_DEAD_H: i32 = 6;

/// Vertical centre of the strip; every glyph in it is centred here.
const STATUS_MID: f32 = STATUS_H as f32 / 2.0;

/// The bookmark/Shelf glyph's centre and its hit half-width. Widened from ±22 to ±30 (and the glyph
/// from 19 to 23px): it is the one part of the strip that does something *other* than open the
/// Menu, so a miss here is a wrong screen rather than a no-op, and 44px was tight for a thumb on a
/// device with no d-pad. Room came from dropping the old Bluetooth glyph that used to sit at x=424
/// — it was drawn `t.faint` unconditionally and never reflected any BT state, so it was decoration
/// occupying the scarcest space on the panel.
const SHELF_CX: i32 = 390;
const SHELF_HALF_W: i32 = 30;

/// Left zone of the strip — the clock and the codec badge. Tapping it goes straight to Now Playing.
/// That badge IS the now-playing indicator, so this is where a finger already points, and it makes
/// the return a ONE-tap gesture from every screen. Before this the only route back was the Now
/// Playing bar, which appears solely on Library and Album: from Settings, EQ, Sound, Bluetooth,
/// Up Next or the Menu there was no direct way back at all.
const NP_ZONE_W: i32 = 150;

/// What a tap on the status strip means. `None` = the tap was below the strip.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StatusTap {
    /// The bookmark glyph → the Shelf.
    Shelf,
    /// The clock/codec-badge zone → Now Playing. One tap back from anywhere.
    NowPlaying,
    /// Anywhere else along the strip → the Menu. Deliberately forgiving.
    Menu,
}

/// Hit test for the status strip. SINGLE SOURCE with [`status_bar`]'s glyph placement: the Shelf
/// zone is built from the same `SHELF_CX` the bookmark is drawn at, so the target cannot drift out
/// from under the glyph.
pub fn status_hit(x: i32, y: i32) -> Option<StatusTap> {
    if y >= STATUS_H - STATUS_DEAD_H {
        return None;
    }
    Some(if (SHELF_CX - SHELF_HALF_W..=SHELF_CX + SHELF_HALF_W).contains(&x) {
        StatusTap::Shelf
    } else if x < NP_ZONE_W {
        StatusTap::NowPlaying
    } else {
        StatusTap::Menu
    })
}

// ── "Sony IPC is dead for this boot" ────────────────────────────────────────────────────────────
//
// When `run_guarded` unwinds a Sony call it latches `g_ipc_dead`, and from that moment the device
// cannot play, pause, skip, drive Bluetooth volume or sleep its own panel until it is restarted.
// The ONLY trace of that was one line in `/contents/cinderhome.log`. From the user's side the
// player just stopped obeying — which on 2026-09-02 cost an evening and a reboot to guess at.
//
// So it gets said on the glass. Three properties this needs and a toast does not have:
//
//   * PERSISTENT. The condition lasts the whole boot, and the user is not necessarily looking when
//     it trips — a 1.8 s toast that fires while the device is in a pocket has said nothing.
//   * EVERYWHERE. `status_bar` is drawn by every screen, so one edit covers all of them.
//   * NO LAYOUT SHIFT. It takes over the codec-badge zone rather than adding a row. The badge
//     describes audio that is not going to play, so it is the right thing to lose, and nothing
//     below the strip moves.
//
// A process global, like the marquee clock and `text::set_scale_idx`, rather than a parameter: it
// is set from the guard's RECOVERY path, which has just siglongjmp'd and must not take a lock to
// report itself. An atomic store is the whole of it.
static IPC_DEAD: AtomicBool = AtomicBool::new(false);

/// Latch the "Sony IPC is dead for this boot" indicator. One-way: nothing clears it but a restart,
/// which is exactly what the flag means.
pub fn set_ipc_dead(dead: bool) {
    IPC_DEAD.store(dead, Ordering::Relaxed);
}
pub fn ipc_dead() -> bool {
    IPC_DEAD.load(Ordering::Relaxed)
}

pub fn status_bar(c: &mut Canvas, t: &Theme, f: &FontSet, clock: &str, badge: &str, battery: u8) {
    // left: clock + codec badge + (NIGHT)
    let cx = text::draw(c, f, 18.0, 27.0, clock, &sty(Family::Mono, Weight::Regular, 15.0, t.dim, 0.06));
    // Skip the whole badge when there is no codec string. Drawing it unconditionally left a bare
    // 12px accent-stroked rectangle floating next to the clock whenever nothing was loaded —
    // caught on a live device screenshot; the host harness never renders that state.
    // The degraded banner OWNS the badge zone while it is up — see IPC_DEAD above — and NOTHING
    // ELSE. It replaces the codec badge and the NIGHT label, then falls through to the shared
    // right-hand block below.
    //
    // FALL THROUGH, NOT `return`. The first version returned here after redrawing the menu and
    // bookmark glyphs, which silently dropped the battery readout — number, outline, nub and
    // charge fill — from every screen for as long as the device was degraded. That is precisely
    // backwards: a device that has just lost playback and its idle screen-off is a device whose
    // battery you most want to see, because it will now stay awake until it is flat. Duplicating
    // the right-hand glyphs into this branch would have "fixed" it by creating a second copy of
    // that layout to keep in step; one path through it cannot drift.
    let degraded = ipc_dead();
    if degraded {
        let wst = sty(Family::Mono, Weight::SemiBold, 12.0, t.acc_ink, 0.10);
        let msg = "AUDIO STOPPED \u{2014} RESTART";
        let ww = text::measure(f, msg, &wst);
        let wx = cx + 12.0;
        fill_rect(c, (wx - 6.0) as i32, 10, (ww + 12.0) as i32, 23, t.acc);
        text::draw(c, f, wx, 26.0, msg, &wst);
    }
    let mut nx = cx;
    if !degraded && !badge.is_empty() {
        let bst = sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.12);
        let bw = text::measure(f, badge, &bst);
        let bx = cx + 12.0;
        Rectangle::new(Point::new((bx - 6.0) as i32, 11), Size::new((bw + 12.0) as u32, 21))
            .into_styled(PrimitiveStyle::with_stroke(t.acc, 1))
            .draw(c)
            .ok();
        nx = text::draw(c, f, bx, 26.0, badge, &bst);
    }
    if t.night && !degraded {
        text::draw(c, f, nx + 12.0, 26.0, "NIGHT", &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.18));
    }

    // right: menu ≡, bookmark, bt, [battery].
    // The ≡ sits LEFT of the Shelf zone on purpose: the whole strip opens the Menu, so the glyph
    // only has to be outside SHELF_CX ± SHELF_HALF_W for "tap the ≡" to mean what it looks like.
    icons::menu(c, 338.0, STATUS_MID, 22.0, t.dim);
    icons::bookmark(c, SHELF_CX as f32, STATUS_MID, 23.0, t.dim);
    let batt = format!("{}", battery);
    let bs = sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.04);
    let bwid = text::measure(f, &batt, &bs);
    text::draw(c, f, 448.0 - bwid, 27.0, &batt, &bs);
    Rectangle::new(Point::new(452, 15), Size::new(18, 13))
        .into_styled(PrimitiveStyle::with_stroke(t.faint, 1))
        .draw(c)
        .ok();
    fill_rect(c, 470, 19, 2, 5, t.faint); // nub
    fill_rect(c, 454, 17, (14.0 * battery as f32 / 100.0) as i32, 9, t.faint); // charge
}

// ── Now Playing return bar ──────────────────────────────────────────────────────────────────
// A full-width slab pinned to the bottom of the browsing screens. Getting back to Now Playing used
// to mean Back-ing out of however deep the library drill-in went; this is one tap from anywhere in
// it. Full width × 64 makes it the largest target in the app, which is the point — it is meant to
// be hit without looking.

/// Height of the Now Playing return bar.
pub const NP_BAR_H: i32 = 64;

/// Rect `(x, y, w, h)` of the return bar. SINGLE SOURCE: [`np_bar`] fills exactly this,
/// [`hit_np_bar`] tests exactly this, and `library::LIST_BOTTOM` is derived from it so the list
/// never scrolls underneath.
/// Did a tap on the Now Playing bar land on its PLAY/PAUSE button (left zone) rather than the
/// navigate-to-Now-Playing rest of it? Built from the same constant the divider is drawn at.
pub fn hit_np_bar_play(x: i32, y: i32) -> bool {
    hit_np_bar(x, y) && x < NP_BAR_PLAY_W
}

pub fn np_bar_rect() -> (i32, i32, i32, i32) {
    (0, crate::H as i32 - NP_BAR_H, crate::W as i32, NP_BAR_H)
}

/// True if `(x, y)` is on the Now Playing return bar.
pub fn hit_np_bar(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = np_bar_rect();
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

/// Draw the return bar: play/pause state glyph, the current title/artist, and a "NOW PLAYING"
/// caption. Drawn on every browsing screen regardless of whether anything is loaded — a bar that
/// appears and disappears would move `LIST_BOTTOM` under the user mid-scroll.
/// Width of the bar's left zone: the play/pause button. The REST of the bar navigates to Now
/// Playing. Single source with [`hit_np_bar_play`].
pub const NP_BAR_PLAY_W: i32 = 74;

pub fn np_bar(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    title: &str,
    artist: &str,
    playing: bool,
    progress: f32,
) {
    let (x, y, w, h) = np_bar_rect();
    fill_rect(c, x, y, w, h, t.panel);
    fill_rect(c, x, y, w, 1, t.line);
    // Live progress along the bar's top edge. Two pixels, no labels: it costs nothing, and it makes
    // the strip read as the running track rather than a static label — you can see at a glance that
    // something is playing and roughly how far in, without leaving the list.
    let fillw = (w as f32 * progress.clamp(0.0, 1.0)) as i32;
    if fillw > 0 {
        fill_rect(c, x, y, fillw, 2, t.acc);
    }

    // Left zone is a REAL play/pause button now. It used to be a state-only glyph, on the reasoning
    // that drawing a pause icon would promise a control that didn't exist — the better answer is to
    // make the control exist: it is the one transport action worth having without leaving the list,
    // and every mini-player behaves this way, so a tap here was being attempted regardless.
    let gy = y + h / 2;
    if playing {
        icons::pause(c, 34.0, gy as f32, 22.0, t.acc);
    } else {
        icons::play(c, 34.0, gy as f32, 22.0, t.dim);
    }
    // Divider: makes the button read as a separate target from the rest of the bar.
    fill_rect(c, NP_BAR_PLAY_W, y + 14, 1, h - 28, t.line);

    let cap = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16);
    let cap_w = text::measure(f, "NOW PLAYING", &cap);
    text::draw(c, f, (crate::W as f32) - 34.0 - cap_w, (y + 26) as f32, "NOW PLAYING", &cap);
    // Chevron pointing UP, under the caption: says "this opens something" rather than leaving the
    // bar looking like a passive label. Without it the strip reads as status, not as a target.
    icons::chevron_up(c, (crate::W as f32) - 20.0, (y + 46) as f32, 12.0, t.faint);

    // Title + artist, clamped so they never run under the caption.
    let avail = (crate::W as f32) - 88.0 - cap_w - 34.0;
    let ts = sty(Family::Sans, Weight::Bold, 17.0, t.ink, -0.01);
    let title = if title.is_empty() { "Nothing playing" } else { title };
    let title = crate::widgets::fit(f, title, &ts, avail);
    text::draw(c, f, 90.0, (y + 28) as f32, &title, &ts);
    if !artist.is_empty() {
        let asty = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
        let artist = crate::widgets::fit(f, artist, &asty, avail);
        text::draw(c, f, 90.0, (y + 48) as f32, &artist, &asty);
    }
}

/// Screen header (`CHeader`): back chevron + title (27/700) + optional right caption.
/// Returns the y where content below the header should start.
/// Y where `header` ends and a screen's own content begins. Screens lay out from this and their
/// hit tests measure from it, so the two can't drift apart.
pub const HEADER_BOTTOM: i32 = 91;

pub fn header(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, right: Option<&str>) -> i32 {
    icons::back(c, 30.0, 62.0, 20.0, t.dim);
    let ts = sty(Family::Sans, Weight::Bold, 30.0, t.ink, -0.01);
    let title_end = text::draw(c, f, 50.0, 70.0, title, &ts);
    if let Some(r) = right {
        let rs = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1);
        // Clamp the caption to the space right of the title (never let it overlap the title).
        let avail = (458.0 - (title_end + 16.0)).max(0.0);
        let r = crate::widgets::fit(f, r, &rs, avail);
        let rw = text::measure(f, &r, &rs);
        text::draw(c, f, 458.0 - rw, 65.0, &r, &rs);
    }
    HEADER_BOTTOM
}

#[cfg(test)]
mod degraded_tests {
    use super::*;
    use crate::canvas::{Canvas, W as CW};
    use std::sync::{Mutex, MutexGuard};

    /// Count pixels differing from the background inside a band, so a test can say "something is
    /// drawn here" without hard-coding glyph shapes.
    fn ink(c: &Canvas, x0: i32, x1: i32, y0: i32, y1: i32, bg: u32) -> u32 {
        let mut n = 0;
        for y in y0.max(0)..y1 {
            for x in x0.max(0)..x1.min(CW as i32) {
                if c.buf[y as usize * CW + x as usize] != bg {
                    n += 1;
                }
            }
        }
        n
    }

    /// `IPC_DEAD` is PROCESS-WIDE state and cargo runs these in parallel threads of one process,
    /// so a latch set by one test is visible inside another's render. That is not hypothetical: it
    /// failed on the first run here, exactly as the UI-scale global does in `tests/ui_overflow.rs`,
    /// which solves it the same way. Every test that touches the latch takes this first.
    static LATCH: Mutex<()> = Mutex::new(());

    fn latch_lock() -> MutexGuard<'static, ()> {
        LATCH.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn shot(dead: bool, battery: u8) -> Canvas {
        set_ipc_dead(dead);
        let t = Theme::day();
        let f = FontSet::load();
        let mut c = Canvas::new();
        c.fill(t.bg);
        status_bar(&mut c, &t, &f, "14:32", "FLAC 24/96", battery);
        c
    }

    /// THE BUG THIS PINS: the degraded branch used to `return` before the right-hand block, which
    /// silently dropped the battery readout from every screen for as long as the device was
    /// degraded — the exact state in which the battery matters most, because the idle screen-off
    /// is among the things that have stopped working, so the device will now stay lit until flat.
    #[test]
    fn the_banner_keeps_the_battery_indicator() {
        let _g = latch_lock();
        let bg = crate::canvas::to_u32(Theme::day().bg);
        // The battery lives at the far right: number ~x 430..450, outline + nub 452..472.
        let normal = ink(&shot(false, 88), 425, CW as i32, 10, 34, bg);
        let warned = ink(&shot(true, 88), 425, CW as i32, 10, 34, bg);
        set_ipc_dead(false);
        assert!(normal > 0, "sanity: the battery is drawn in the normal state");
        assert_eq!(
            warned, normal,
            "the degraded banner must not disturb the battery indicator \
             (drew {warned} px where the normal strip draws {normal})"
        );
    }

    /// The menu and shelf glyphs survive too — the strip has to stay navigable while warning.
    #[test]
    fn the_banner_keeps_the_strip_navigable() {
        let _g = latch_lock();
        let bg = crate::canvas::to_u32(Theme::day().bg);
        let band = |dead| {
            let c = shot(dead, 50);
            (
                ink(&c, 320, 356, 6, 38, bg),                                  // menu glyph
                ink(&c, SHELF_CX - 16, SHELF_CX + 16, 6, 38, bg),              // bookmark glyph
            )
        };
        let normal = band(false);
        let warned = band(true);
        set_ipc_dead(false);
        assert!(normal.0 > 0 && normal.1 > 0, "sanity: both glyphs draw normally");
        assert_eq!(warned, normal, "menu and shelf glyphs must be untouched by the banner");
    }

    /// It actually says something, in the badge zone, and only when latched.
    #[test]
    fn the_banner_appears_only_when_latched() {
        let _g = latch_lock();
        let bg = crate::canvas::to_u32(Theme::day().bg);
        // The badge/banner zone sits between the clock and the menu glyph.
        let quiet = ink(&shot(false, 50), 60, 300, 8, 36, bg);
        let loud = ink(&shot(true, 50), 60, 300, 8, 36, bg);
        set_ipc_dead(false);
        assert!(
            loud > quiet * 2,
            "the warning should dominate the zone the codec badge occupies ({loud} vs {quiet})"
        );
    }

    /// The latch is one-way from the caller's side and survives being read repeatedly — the shell
    /// sets it once from the guard's recovery path and every later frame must still see it.
    #[test]
    fn the_latch_persists_across_frames() {
        let _g = latch_lock();
        set_ipc_dead(true);
        assert!(ipc_dead());
        assert!(ipc_dead(), "reading it must not clear it");
        set_ipc_dead(false);
        assert!(!ipc_dead());
    }
}
