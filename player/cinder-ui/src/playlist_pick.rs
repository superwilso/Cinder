//! The two pickers that make playlists usable: "which playlist?" and "which tracks?".
//!
//! Both are plain scrolling lists, deliberately: the Library already has four of them and every
//! one of its idioms — row height, the accent for the active row, the chevron, the scrollbar — is
//! learned by the time anyone gets here.
//!
//! ONLY CINDER'S OWN PLAYLISTS can be added to. Sony's live in the MediaStore database, which is
//! rebuilt by a rescan and is not ours to write (see `cinder-ffi/src/playlists.rs`), so offering
//! them here would be offering something that silently could not work. The footer says so when the
//! library has some, rather than leaving their absence looking like a bug.

use crate::canvas::{H, W};
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, fit, hline, sty};
use crate::Canvas;

pub const ROW_H: i32 = 64;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM + 8;
/// Above the bottom edge, with room for the footer note.
pub const BOTTOM: i32 = H as i32 - 40;

/// One row of the "which playlist?" list.
pub struct Target<'a> {
    pub name: &'a str,
    pub tracks: u32,
}

/// Rows fit in the window; used by the navigator to clamp scrolling.
pub fn content_h(rows: usize) -> i32 {
    rows as i32 * ROW_H
}

/// Which row is under `y` (content-space, honouring the pixel scroll). None = chrome or past the
/// end, so a tap below a short list does nothing instead of hitting the last row.
pub fn hit_row(rows: usize, scroll_px: i32, y: i32) -> Option<usize> {
    if y < TOP || y >= BOTTOM {
        return None;
    }
    let index = ((y - TOP + scroll_px.max(0)) / ROW_H) as usize;
    (index < rows).then_some(index)
}

/// "Add to playlist": row 0 is always NEW PLAYLIST, then the user's own lists.
pub fn render_targets(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, track: &str,
                      targets: &[Target], sony_count: usize, scroll_px: i32) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, title, Some(&fit_caption(f, track, t)));
    let rows = targets.len() + 1;
    c.set_clip_y(TOP, BOTTOM);
    let first = (scroll_px.max(0) / ROW_H) as usize;
    let mut y = TOP - (scroll_px.max(0) % ROW_H);
    for index in first..rows {
        if y >= BOTTOM {
            break;
        }
        let cy = y + ROW_H / 2;
        if index == 0 {
            fill_rect(c, 0, y, W as i32, ROW_H, t.acc);
            text::draw(c, f, 26.0, (cy + 1) as f32, "+  NEW PLAYLIST",
                       &sty(Family::Sans, Weight::ExtraBold, 19.0, t.acc_ink, 0.04));
            text::draw(c, f, 26.0, (cy + 19) as f32, "NAME IT, THEN ADD THIS TRACK",
                       &sty(Family::Mono, Weight::Regular, 10.0, t.acc_ink, 0.14));
        } else {
            let target = &targets[index - 1];
            text::draw(c, f, 26.0, (cy - 2) as f32, &fit(f, target.name,
                       &sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0), 400.0),
                       &sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0));
            text::draw(c, f, 26.0, (cy + 17) as f32, &format!("{} tracks", target.tracks),
                       &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0));
            crate::icons::chevron(c, 452.0, cy as f32, 13.0, t.faint);
        }
        hline(c, y + ROW_H, t.line);
        y += ROW_H;
    }
    c.clear_clip();
    footer(c, t, f, sony_count);
}

/// "Add tracks": every song in the library, with the ones already in the playlist ticked.
pub fn render_tracks(c: &mut Canvas, t: &Theme, f: &FontSet, playlist: &str, songs: &[&SongRow],
                     is_in: &dyn Fn(usize) -> bool, scroll_px: i32, added: usize) {
    c.fill(t.bg);
    let caption = if added > 0 { format!("{added} ADDED") } else { "TAP TO ADD".to_string() };
    crate::chrome::header(c, t, f, playlist, Some(&caption));
    c.set_clip_y(TOP, BOTTOM);
    let first = (scroll_px.max(0) / ROW_H) as usize;
    let mut y = TOP - (scroll_px.max(0) % ROW_H);
    for index in first..songs.len() {
        if y >= BOTTOM {
            break;
        }
        let song = songs[index];
        let cy = y + ROW_H / 2;
        let inside = is_in(index);
        let title_style = sty(Family::Sans, Weight::SemiBold, 19.0,
                              if inside { t.dim } else { t.ink }, 0.0);
        text::draw(c, f, 26.0, (cy - 2) as f32, &fit(f, &song.title, &title_style, 372.0), &title_style);
        text::draw(c, f, 26.0, (cy + 17) as f32,
                   &fit(f, &song.artist, &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0), 372.0),
                   &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0));
        // ✓ for a track already in the list, + for one that is not. The tick is drawn dim, so the
        // list reads as "what is left to add" at a glance.
        if inside {
            tick(c, t, 442.0, cy as f32, 13.0);
        } else {
            plus(c, t, 442.0, cy as f32, 13.0);
        }
        hline(c, y + ROW_H, t.line);
        y += ROW_H;
    }
    c.clear_clip();
    crate::widgets::center(c, f, 240.0, (BOTTOM + 26) as f32,
                           "BACK WHEN YOU ARE DONE",
                           &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.16));
}

fn footer(c: &mut Canvas, t: &Theme, f: &FontSet, sony_count: usize) {
    // Short enough to fit the panel at every UI scale — the first version ran off the right edge.
    let note = if sony_count > 0 {
        format!("{sony_count} SONY PLAYLIST(S) CANNOT BE EDITED HERE")
    } else {
        "PLAYLISTS YOU MAKE ARE SAVED ON THE DEVICE".to_string()
    };
    crate::widgets::center(c, f, 240.0, (BOTTOM + 26) as f32, &note,
                           &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.16));
}

/// A tick. `icons` has neither a tick nor a plus, and this is the only screen that wants them.
fn tick(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, r: f32) {
    let (x, y, r) = (cx as i32, cy as i32, r as i32);
    // Two strokes, drawn as short stepped runs — the canvas has no line primitive and a tick is
    // two diagonals, so each is a staircase of 2x2 dots.
    for step in 0..r / 2 {
        fill_rect(c, x - r / 2 + step, y + step, 2, 2, t.dim);
    }
    for step in 0..r {
        fill_rect(c, x - r / 2 + r / 2 + step, y + r / 2 - step, 2, 2, t.dim);
    }
}

/// A `+` glyph. `icons` has no plus and this is the only place that wants one.
fn plus(c: &mut Canvas, t: &Theme, cx: f32, cy: f32, r: f32) {
    let x = cx as i32;
    let y = cy as i32;
    let r = r as i32;
    fill_rect(c, x - r, y - 1, r * 2, 2, t.acc);
    fill_rect(c, x - 1, y - r, 2, r * 2, t.acc);
}

fn fit_caption(f: &FontSet, s: &str, t: &Theme) -> String {
    fit(f, &s.to_uppercase(), &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1), 180.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_row_matches_the_drawn_rows() {
        // Row 0 starts at TOP; the centre of row 2 with a 40 px scroll is where row 2 is drawn.
        assert_eq!(hit_row(5, 0, TOP + ROW_H / 2), Some(0));
        assert_eq!(hit_row(5, 0, TOP + ROW_H * 2 + ROW_H / 2), Some(2));
        assert_eq!(hit_row(5, 40, TOP + ROW_H * 2 + ROW_H / 2 - 40), Some(2));
    }

    #[test]
    fn taps_off_the_list_hit_nothing() {
        assert_eq!(hit_row(3, 0, TOP - 1), None);
        assert_eq!(hit_row(3, 0, BOTTOM), None);
        assert_eq!(hit_row(3, 0, TOP + ROW_H * 3 + 2), None, "past the last row");
        assert_eq!(hit_row(0, 0, TOP + 2), None, "an empty list has no rows to hit");
    }
}
