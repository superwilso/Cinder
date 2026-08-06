//! Up Next — the play queue. Now data-driven: it shows the **current album** (the album the
//! now-playing track belongs to, resolved from the library), highlighting the playing row and the
//! tracks that follow. Windowed like the library lists so long albums scroll. When nothing is
//! playing / the track isn't in the library, a clean empty state is shown (no fake data).

use crate::art;
use crate::canvas::W;
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use crate::Canvas;

pub const RH: i32 = 62;
const LIST_BOTTOM: i32 = 736; // leave room for the footer rule
/// Row area top — `chrome::header` always returns 91, and both render paths start there.
pub const LIST_TOP: i32 = 91;

/// How many rows fit in the window (shared by render + the window calculation).
pub fn visible_rows() -> usize {
    (((LIST_BOTTOM - LIST_TOP) / RH).max(1)) as usize
}

/// First drawn index for a `len`-row list following the playing row `current`. Shared by
/// `render` and the tap hit-test so they can't drift.
pub fn window_scroll(len: usize, current: usize) -> usize {
    let max_scroll = len.saturating_sub(visible_rows());
    current.saturating_sub(4).min(max_scroll)
}

/// Which DRAWN row (0-based, from the top of the list area) is at screen-y `y`?
/// `None` for the header/chrome. Add `window_scroll(..)` to get the list index.
pub fn drawn_row_at(y: i32) -> Option<usize> {
    if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    Some(((y - LIST_TOP) / RH) as usize)
}

/// Render the queue: `tracks` = the current album's tracks (play order), `current` = the playing
/// index within it. `album` is shown in the header. The window auto-scrolls to keep the playing
/// track visible (no cursor state needed — the queue follows playback).
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, album: &str, tracks: &[SongRow], current: usize) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f);

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
    let scroll = window_scroll(tracks.len(), current);

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
        // thumb
        art::block(c, t, 46, y + (RH - 40) / 2, 40, 40, &song.art, if t.night { 0.30 } else { 1.0 });
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
        crate::library::scrollbar(c, t, y0, scroll as i32 * RH, tracks.len() as i32 * RH);
    }
}

/// Render the USER queue (songs added by the Spotify-style right-swipe), in add order. No
/// "now playing" highlight — these are upcoming picks, not the live album window.
pub fn render_queue(c: &mut Canvas, t: &Theme, f: &FontSet, queue: &[SongRow]) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f);
    let sub = format!("QUEUE · {} TRACKS", queue.len());
    let y0 = crate::chrome::header(c, t, f, "Up Next", Some(&sub));

    let mut y = y0;
    let mut shown = 0;
    for (i, song) in queue.iter().enumerate() {
        if y + RH > LIST_BOTTOM {
            break;
        }
        let cy = (y + RH / 2) as f32;
        text::draw(c, f, 22.0, cy + 4.0, &format!("{:02}", i + 1),
            &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
        art::block(c, t, 46, y + (RH - 40) / 2, 40, 40, &song.art, if t.night { 0.30 } else { 1.0 });
        let tst = sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0);
        text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, 306.0), &tst);
        let ast = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
        text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, 320.0), &ast);
        right(c, f, 458.0, cy + 4.0, &song.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
        hline(c, y + RH, t.line);
        y += RH;
        shown += 1;
    }
    if queue.len() > shown {
        crate::library::scrollbar(c, t, y0, 0, queue.len() as i32 * RH);
    }
}
