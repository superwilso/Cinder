//! Shared chrome — the status bar (`CStatus` in cinder-proto-screens1.jsx),
//! used by every screen. Left: clock + codec badge + NIGHT. Right: menu ·
//! bookmark · bt · battery.

use crate::canvas::Canvas;
use crate::icons;
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

/// Vertical centre of the strip; every glyph in it is centred here.
const STATUS_MID: f32 = STATUS_H as f32 / 2.0;

/// The bookmark/Shelf glyph's centre and its hit half-width. The glyph is 19px but the target is
/// 44 wide, because it is the one part of the strip that does something *other* than open the
/// Menu — a miss here is a wrong screen, not a no-op.
const SHELF_CX: i32 = 388;
const SHELF_HALF_W: i32 = 22;

/// What a tap on the status strip means. `None` = the tap was below the strip.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StatusTap {
    /// The bookmark glyph → the Shelf.
    Shelf,
    /// Anywhere else along the strip → the Menu. Deliberately forgiving.
    Menu,
}

/// Hit test for the status strip. SINGLE SOURCE with [`status_bar`]'s glyph placement: the Shelf
/// zone is built from the same `SHELF_CX` the bookmark is drawn at, so the target cannot drift out
/// from under the glyph.
pub fn status_hit(x: i32, y: i32) -> Option<StatusTap> {
    if y >= STATUS_H {
        return None;
    }
    Some(if (SHELF_CX - SHELF_HALF_W..=SHELF_CX + SHELF_HALF_W).contains(&x) {
        StatusTap::Shelf
    } else {
        StatusTap::Menu
    })
}

pub fn status_bar(c: &mut Canvas, t: &Theme, f: &FontSet, clock: &str, badge: &str, battery: u8) {
    // left: clock + codec badge + (NIGHT)
    let cx = text::draw(c, f, 18.0, 27.0, clock, &sty(Family::Mono, Weight::Regular, 15.0, t.dim, 0.06));
    // Skip the whole badge when there is no codec string. Drawing it unconditionally left a bare
    // 12px accent-stroked rectangle floating next to the clock whenever nothing was loaded —
    // caught on a live device screenshot; the host harness never renders that state.
    let mut nx = cx;
    if !badge.is_empty() {
        let bst = sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.12);
        let bw = text::measure(f, badge, &bst);
        let bx = cx + 12.0;
        Rectangle::new(Point::new((bx - 6.0) as i32, 11), Size::new((bw + 12.0) as u32, 21))
            .into_styled(PrimitiveStyle::with_stroke(t.acc, 1))
            .draw(c)
            .ok();
        nx = text::draw(c, f, bx, 26.0, badge, &bst);
    }
    if t.night {
        text::draw(c, f, nx + 12.0, 26.0, "NIGHT", &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.18));
    }

    // right: menu ≡, bookmark, bt, [battery].
    // The ≡ sits LEFT of the Shelf zone on purpose: the whole strip opens the Menu, so the glyph
    // only has to be outside SHELF_CX ± SHELF_HALF_W for "tap the ≡" to mean what it looks like.
    icons::menu(c, 344.0, STATUS_MID, 22.0, t.dim);
    icons::bookmark(c, SHELF_CX as f32, STATUS_MID, 19.0, t.dim);
    icons::bt(c, 424.0, STATUS_MID, 17.0, t.faint);
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
pub fn np_bar(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, artist: &str, playing: bool) {
    let (x, y, w, h) = np_bar_rect();
    fill_rect(c, x, y, w, h, t.panel);
    fill_rect(c, x, y, w, 1, t.line);

    // STATE glyph, left — deliberately not a transport button. The whole bar navigates to Now
    // Playing; drawing a pause icon here would promise a control that doesn't exist, so playback
    // state is carried by colour on one shape: accent = playing, faint = paused.
    let gy = y + h / 2;
    icons::play(c, 34.0, gy as f32, 20.0, if playing { t.acc } else { t.faint });

    let cap = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16);
    let cap_w = text::measure(f, "NOW PLAYING", &cap);
    text::draw(c, f, (crate::W as f32) - 22.0 - cap_w, (y + 26) as f32, "NOW PLAYING", &cap);

    // Title + artist, clamped so they never run under the caption.
    let avail = (crate::W as f32) - 62.0 - cap_w - 34.0;
    let ts = sty(Family::Sans, Weight::Bold, 17.0, t.ink, -0.01);
    let title = if title.is_empty() { "Nothing playing" } else { title };
    let title = crate::widgets::fit(f, title, &ts, avail);
    text::draw(c, f, 62.0, (y + 28) as f32, &title, &ts);
    if !artist.is_empty() {
        let asty = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
        let artist = crate::widgets::fit(f, artist, &asty, avail);
        text::draw(c, f, 62.0, (y + 48) as f32, &artist, &asty);
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
