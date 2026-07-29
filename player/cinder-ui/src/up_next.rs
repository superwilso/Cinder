//! Up Next — the play queue. Now data-driven: it shows the **current album** (the album the
//! now-playing track belongs to, resolved from the library), highlighting the playing row and the
//! tracks that follow. Windowed like the library lists so long albums scroll. When nothing is
//! playing / the track isn't in the library, a clean empty state is shown (no fake data).

use crate::canvas::W;
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use crate::Canvas;

pub const RH: i32 = 62;
const LIST_BOTTOM: i32 = 736; // leave room for the footer rule
const LIST_TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The reorder grab handle's hit strip on a user-queue row. Wide, because this device has no d-pad
/// and reordering is a thumb-only gesture, but it STOPS short of the right edge: the last
/// `library::SBAR_GRAB_W` px belong to the scrollbar drag, and one strip cannot serve both. A
/// vertical drag starting here reorders; anywhere else it scrolls, which is the same start-point
/// ownership rule the scrub rail uses.
pub const GRIP_X0: i32 = 424;
pub const GRIP_X1: i32 = W as i32 - crate::library::SBAR_GRAB_W;

/// The "clear the queue" chip in the header: `(x, y, w, h)`. An explicit, labelled control rather
/// than a gesture, because emptying the queue is the one action here that cannot be undone.
pub const CLEAR_CHIP: (i32, i32, i32, i32) = (388, 48, 70, 28);

pub fn hit_clear_chip(x: i32, y: i32) -> bool {
    let (cx, cy, cw, ch) = CLEAR_CHIP;
    (cx..cx + cw).contains(&x) && (cy..cy + ch).contains(&y)
}

/// A queue row being dragged to a new position.
///
/// `y`/`grab_off` are in SCREEN space, not content space, so the floating row keeps sitting under
/// the finger while the list auto-scrolls beneath it — deriving the float from `from * RH` instead
/// would make it slide away from the thumb the moment the edge-scroll kicked in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct QueueDrag {
    /// Index in the queue the finger picked up.
    pub from: usize,
    /// Index it would land on if released now. The other rows part to show this slot.
    pub to: usize,
    /// Finger y when the row was picked up. Kept so the shell can report TOTAL travel from the
    /// gesture's start — the same thing it measures for a swipe — instead of per-event deltas,
    /// which drift whenever the driver coalesces events.
    pub start_y: i32,
    /// Current finger y, UI screen coords.
    pub y: i32,
    /// Where inside the row the finger grabbed it, so the row doesn't jump on pick-up.
    pub grab_off: i32,
}

impl QueueDrag {
    /// Top of the floating row, in screen coords.
    pub fn float_top(&self) -> i32 {
        self.y - self.grab_off
    }
}

/// Height of the user queue's scrolling window.
pub fn queue_view_h() -> i32 {
    LIST_BOTTOM - LIST_TOP
}

pub fn queue_max_scroll_px(len: usize) -> i32 {
    (len as i32 * RH - queue_view_h()).max(0)
}

/// Which queue index sits under screen-`y` at this scroll offset. Unlike [`visible_row_at`] this
/// returns the index into the queue itself, so the caller never has to know where the window is.
pub fn queue_row_at(y: i32, scroll_px: i32, len: usize) -> Option<usize> {
    if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let i = ((y - LIST_TOP + scroll_px) / RH) as usize;
    (i < len).then_some(i)
}

/// Is this x on the grab handle?
pub fn queue_grip_hit(x: i32) -> bool {
    (GRIP_X0..GRIP_X1).contains(&x)
}

/// Which slot a floating row is hovering over, from its top edge in screen coords. Uses the row's
/// CENTRE, so the swap happens when the dragged row is more than half way over its neighbour —
/// swapping on the leading edge makes the list twitch a full row before the finger has committed.
pub fn queue_slot_for(float_top: i32, scroll_px: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let centre = float_top - LIST_TOP + scroll_px + RH / 2;
    (centre.div_euclid(RH)).clamp(0, len as i32 - 1) as usize
}

/// The queue in the order it is currently DRAWN: `from` lifted out and re-inserted at `to`.
fn drag_order(len: usize, drag: Option<QueueDrag>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if let Some(d) = drag {
        if d.from < len && d.to < len {
            let it = order.remove(d.from);
            order.insert(d.to, it);
        }
    }
    order
}

/// Which visible row index `y` falls on (0 = the topmost DRAWN row, which is not necessarily
/// track 0 — this list auto-scrolls to follow playback). `nav` pairs this with the ids the
/// renderer publishes, so a tap resolves to the row actually under the finger.
pub fn visible_row_at(y: i32) -> Option<usize> {
    let top = crate::chrome::HEADER_BOTTOM;
    if !(top..LIST_BOTTOM).contains(&y) {
        return None;
    }
    Some(((y - top) / RH) as usize)
}

/// How many rows fit in the window, and where the auto-scrolled window starts for `current`.
/// Kept next to the renderer that uses it so the two can't disagree.
pub fn window(len: usize, current: usize) -> (usize, usize) {
    let visible = ((LIST_BOTTOM - crate::chrome::HEADER_BOTTOM) / RH).max(1) as usize;
    let max_scroll = len.saturating_sub(visible);
    (visible, current.saturating_sub(4).min(max_scroll))
}

/// Render the queue: `tracks` = the current album's tracks (play order), `current` = the playing
/// index within it. `album` is shown in the header. The window auto-scrolls to keep the playing
/// track visible (no cursor state needed — the queue follows playback).
#[allow(clippy::too_many_arguments)]
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, album: &str, tracks: &[SongRow],
              current: usize, lib: &crate::model::Library) {
    c.fill(t.bg);

    if tracks.is_empty() {
        let _ = crate::chrome::header(c, t, f, "Up Next", None);
        let st = sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0);
        text::draw(c, f, 22.0, 360.0, "Nothing queued.", &sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0));
        text::draw(c, f, 22.0, 386.0, "Play a track and its album appears here.", &st);
        return;
    }

    let sub = format!("{} · {} TRACKS", album.to_uppercase(), tracks.len());
    let y0 = crate::chrome::header(c, t, f, "Up Next", Some(&sub));

    // Window that keeps the playing row visible: ~4 rows of lead-in, clamped to the list end.
    let (_visible, scroll) = window(tracks.len(), current);
    // This view scrolls by whole rows to follow playback; there is nothing for a finger to drag.
    let sbar_active = false;

    let mut y = y0;
    let mut shown = 0;
    for (i, song) in tracks.iter().enumerate().skip(scroll) {
        if y + RH > LIST_BOTTOM {
            break;
        }
        let cy = (y + RH / 2) as f32;
        let now = i == current;
        if now {
            fill_rect(c, 0, y, W as i32, RH, t.panel);
        }
        // index / ▶
        let idx_col = if now { t.acc } else { t.faint };
        let idx = if now { "▶".to_string() } else { format!("{:02}", i + 1) };
        text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 12.0, idx_col, 0.0));
        // Thumb: the REAL decoded cover when the art cache has one, exactly like the library rows.
        // This screen drew the gradient fallback unconditionally, so a queue of tracks whose covers
        // were already decoded and sitting on disk still showed twelve coloured squares. Drawn at
        // 48px because that is the size the cache stores (T48) — at any other size `thumb` cannot
        // match and silently falls back to the gradient, which is how it would look "fixed" while
        // changing nothing.
        crate::library::thumb(c, t, lib, song.album_id, &song.art,
                              46, y + (RH - 48) / 2, 48, if t.night { 0.30 } else { 1.0 });
        // title / artist
        let title_col = if now { t.acc } else { t.ink };
        let tst = sty(Family::Sans, Weight::SemiBold, 20.0, title_col, 0.0);
        text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, 306.0), &tst);
        let ast = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
        text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, 320.0), &ast);
        // duration
        right(c, f, 458.0, cy + 4.0, &song.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
        hline(c, y + RH, t.line);
        y += RH;
        shown += 1;
    }

    // scrollbar (only if the album overflows the window) — px-space equivalents of the
    // row-window this screen still scrolls by
    if tracks.len() > shown {
        crate::library::scrollbar(c, t, y0, scroll as i32 * RH, tracks.len() as i32 * RH, sbar_active);
    }
}

/// One user-queue row's content at screen-`y`. `n` is the position label (1-based).
fn queue_row(c: &mut Canvas, t: &Theme, f: &FontSet, song: &SongRow, lib: &crate::model::Library,
             y: i32, n: usize) {
    let cy = (y + RH / 2) as f32;
    text::draw(c, f, 22.0, cy + 4.0, &format!("{n:02}"),
        &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
    // Real cover here too — the user queue had the same gradient-only problem as the album
    // window above it.
    crate::library::thumb(c, t, lib, song.album_id, &song.art,
                          46, y + (RH - 48) / 2, 48, if t.night { 0.30 } else { 1.0 });
    let tst = sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0);
    text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, 262.0), &tst);
    let ast = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
    text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, 276.0), &ast);
    // Duration moved in from 458 to clear the grab handle.
    right(c, f, 410.0, cy + 4.0, &song.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
    grip(c, t, y, false);
}

/// The reorder grab handle: three stacked bars, the universal "drag me" mark. Accent while the row
/// is lifted so the gesture reads as engaged even though the finger covers the icon.
fn grip(c: &mut Canvas, t: &Theme, y: i32, lifted: bool) {
    let col = if lifted { t.acc } else { t.faint };
    let cy = y + RH / 2;
    let w = GRIP_X1 - GRIP_X0 - 8;
    for k in -1..=1 {
        fill_rect(c, GRIP_X0 + 4, cy + k * 7 - 1, w, 2, col);
    }
}

/// Render the USER queue (songs added by the Spotify-style right-swipe), in add order. No
/// "now playing" highlight — these are upcoming picks, not the live album window.
///
/// `drag` is the row being reordered, if any: the list is drawn in its would-be order with that
/// row's slot left empty, and the row itself floats under the finger on top.
#[allow(clippy::too_many_arguments)]
pub fn render_queue(c: &mut Canvas, t: &Theme, f: &FontSet, queue: &[SongRow],
                    lib: &crate::model::Library, scroll_px: i32, drag: Option<QueueDrag>,
                    swipe: Option<crate::library::SwipeRow>, sbar_active: bool) {
    c.fill(t.bg);
    let scroll_px = scroll_px.clamp(0, queue_max_scroll_px(queue.len()));
    let sub = if drag.is_some() {
        String::from("DRAG TO REORDER")
    } else {
        format!("{} TRACKS", queue.len())
    };
    let y0 = crate::chrome::header(c, t, f, "Up Next", None);
    // Count on the left of the chip, right-aligned into the gap it leaves.
    let (chx, chy, chw, chh) = CLEAR_CHIP;
    right(c, f, (chx - 12) as f32, 65.0, &sub,
          &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));
    crate::widgets::stroke_rect(c, chx, chy, chw, chh, t.line, 1);
    let cst = sty(Family::Mono, Weight::Bold, 11.0, t.dim, 0.14);
    crate::widgets::center(c, f, (chx + chw / 2) as f32, (chy + chh / 2 + 4) as f32, "CLEAR", &cst);

    let order = drag_order(queue.len(), drag);
    let first = (scroll_px / RH) as usize;
    let mut y = y0 - (scroll_px % RH);
    c.set_clip_y(y0, LIST_BOTTOM);
    for slot in first..order.len() {
        if y >= LIST_BOTTOM {
            break;
        }
        let i = order[slot];
        // The lifted row is drawn floating below, not in the list — leave its slot as a well, so
        // there is somewhere for the eye (and the row) to land.
        if drag.map(|d| d.from) == Some(i) {
            fill_rect(c, 0, y, W as i32, RH, t.panel);
        } else {
            // Swipe-to-remove. Both directions mean the same thing here — see `SwipeIntent`.
            let sw = swipe
                .filter(|s| (y..y + RH).contains(&s.y) && s.dx != 0)
                .map(|s| s.dx);
            if let Some(dx) = sw {
                crate::library::swipe_reveal(c, t, f, y, RH, dx, crate::library::SwipeIntent::Remove);
            }
            queue_row(c, t, f, &queue[i], lib, y, slot + 1);
            if sw.is_some() {
                c.clear_offset_x();
            }
        }
        hline(c, y + RH, t.line);
        y += RH;
    }
    c.clear_clip();

    // The floating row, last so it sits over everything, and clipped to the list so it cannot
    // smear across the header on an over-drag.
    if let Some(d) = drag {
        if let Some(song) = queue.get(d.from) {
            let ft = d.float_top().clamp(y0 - RH / 2, LIST_BOTTOM - RH / 2);
            c.set_clip_y(y0, LIST_BOTTOM);
            fill_rect(c, 0, ft, W as i32, RH, t.row_sel);
            fill_rect(c, 0, ft, 4, RH, t.acc);       // lifted marker down the leading edge
            hline(c, ft, t.line);
            hline(c, ft + RH, t.line);
            queue_row(c, t, f, song, lib, ft, d.to + 1);
            grip(c, t, ft, true);
            c.clear_clip();
        }
    }

    if queue_max_scroll_px(queue.len()) > 0 {
        crate::library::scrollbar(c, t, y0, scroll_px, queue.len() as i32 * RH, sbar_active);
    }
}
