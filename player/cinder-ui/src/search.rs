//! Library search — find a song by any part of its title, artist or album name, and play it.
//!
//! An opt-in component (`search` in `components.conf`): the Library header draws the button only
//! when the shell says it is installed. The layout is the "Add tracks" picker's — a fixed search band
//! over a plain list — because that screen already taught the idiom, and its geometry and hit tests
//! are shared rather than copied, so the two cannot drift.

use crate::model::SongRow;
use crate::playlist_pick::{content_h, BOTTOM, LIST_TOP, ROW_H, SEARCH_H, TOP};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, fit, hline, sty};
use crate::Canvas;

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    songs: &[&SongRow],
    query: &str,
    total: usize,
    scroll_px: i32,
    sbar_active: bool,
) {
    c.fill(t.bg);
    let has = !query.trim().is_empty();
    let caption = if has { format!("{} FOUND", songs.len()) } else { format!("{total} SONGS") };
    crate::chrome::header(c, t, f, "Search", Some(&caption));

    // ── the search band: shows the query, and a tap on it edits it ────────────────────────────
    fill_rect(c, 0, TOP, crate::canvas::W as i32, SEARCH_H, t.panel);
    hline(c, TOP + SEARCH_H - 1, t.line);
    let qs = sty(Family::Sans, Weight::Regular, 17.0, if has { t.ink } else { t.faint }, 0.0);
    let shown = if has { query } else { "Title, artist or album" };
    text::draw(c, f, 26.0, (TOP + SEARCH_H / 2 + 5) as f32, &fit(f, shown, &qs, 420.0), &qs);

    if songs.is_empty() {
        // Say which of the two empty states this is: nothing asked, or nothing found.
        let st = sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
        let note = if has { "Nothing matches." } else { "Tap above and start typing." };
        text::draw(c, f, 26.0, (LIST_TOP + 44) as f32, note, &st);
        return;
    }

    c.set_clip_y(LIST_TOP, BOTTOM);
    let first = (scroll_px.max(0) / ROW_H) as usize;
    let mut y = LIST_TOP - (scroll_px.max(0) % ROW_H);
    let title_style = sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, t.ink, 0.0);
    let artist_style = sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0);
    for song in songs.iter().skip(first) {
        if y >= BOTTOM {
            break;
        }
        let cy = y + ROW_H / 2;
        text::draw(c, f, 26.0, (cy - 2) as f32, &fit(f, &song.title, &title_style, 380.0), &title_style);
        text::draw(c, f, 26.0, (cy + 17) as f32, &fit(f, &song.artist, &artist_style, 380.0), &artist_style);
        crate::icons::play(c, 440.0, cy as f32, 14.0, t.faint);
        hline(c, y + ROW_H, t.line);
        y += ROW_H;
    }
    c.clear_clip();
    // The same track bounds the "Add tracks" list uses, so `nav::sbar_metrics` can share them.
    crate::library::scrollbar(c, t, TOP, BOTTOM, scroll_px, content_h(songs.len()), sbar_active);
}
