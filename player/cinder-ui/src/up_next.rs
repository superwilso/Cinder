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

const RH: i32 = 62;
const LIST_BOTTOM: i32 = 736; // leave room for the footer rule

/// Render the queue: `tracks` = the current album's tracks (play order), `current` = the playing
/// index within it. `album` is shown in the header. The window auto-scrolls to keep the playing
/// track visible (no cursor state needed — the queue follows playback).
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, album: &str, tracks: &[SongRow], current: usize) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);

    if tracks.is_empty() {
        let _ = crate::chrome::header(c, t, f, "Up Next", None);
        let st = sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0);
        text::draw(c, f, 22.0, 360.0, "Nothing queued.", &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
        text::draw(c, f, 22.0, 386.0, "Play a track and its album appears here.", &st);
        return;
    }

    let sub = format!("{} · {} TRACKS", album.to_uppercase(), tracks.len());
    let y0 = crate::chrome::header(c, t, f, "Up Next", Some(&sub));

    // Window that keeps the playing row visible: ~4 rows of lead-in, clamped to the list end.
    let visible = ((LIST_BOTTOM - y0) / RH).max(1) as usize;
    let max_scroll = tracks.len().saturating_sub(visible);
    let scroll = current.saturating_sub(4).min(max_scroll);

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
        text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 10.0, idx_col, 0.0));
        // thumb
        art::block(c, t, 46, y + (RH - 40) / 2, 40, 40, &song.art, if t.night { 0.30 } else { 1.0 });
        // title / artist
        let title_col = if now { t.acc } else { t.ink };
        text::draw(c, f, 100.0, cy - 2.0, &song.title, &sty(Family::Sans, Weight::SemiBold, 15.0, title_col, 0.0));
        text::draw(c, f, 100.0, cy + 16.0, &song.artist, &sty(Family::Sans, Weight::Regular, 11.0, t.dim, 0.0));
        // duration
        right(c, f, 458.0, cy + 4.0, &song.dur, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        hline(c, y + RH, t.line);
        y += RH;
        shown += 1;
    }

    // scrollbar (only if the album overflows the window)
    if tracks.len() > shown {
        crate::library::scrollbar(c, t, y0, scroll, shown, tracks.len());
    }
}
