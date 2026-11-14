//! Settings ▸ Wi-Fi ▸ Media servers: the music servers on the home network, then their folders and
//! songs, one level a screen deep (Back goes up a level, as in Folders). The shell searches,
//! browses and plays (`cinder-ffi/src/media.rs`); this draws what it hands over and says which row
//! was tapped.
//!
//! Layout: the header, then a strip (what is playing, tap to stop; or what the page is doing),
//! then the rows.

use crate::canvas::H;
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::Canvas;

/// One row: a server, a folder or a song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub title: String,
    /// The second line: a folder's size, a song's artist.
    pub sub: String,
    /// Opens a level (a server or a folder) rather than playing.
    pub folder: bool,
    /// A song in a format the player cannot decode: listed, not played.
    pub unplayable: bool,
}

/// One level: its title, rows and scroll.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Level {
    pub title: String,
    pub entries: Vec<Entry>,
    pub scroll: i32,
    /// The first row is "Search this server" (the app's own, not the server's).
    pub searchable: bool,
}

pub struct View<'a> {
    pub level: Option<&'a Level>,
    /// The search or a folder is loading.
    pub busy: bool,
    /// What went wrong last ("" = nothing).
    pub note: &'a str,
    /// The song playing from a server ("" = none).
    pub playing: &'a str,
}

const STRIP_Y: i32 = crate::chrome::HEADER_BOTTOM;
const TOP: i32 = STRIP_Y + kit::STRIP_H;

/// The strip under the header: tapping it stops what plays.
pub fn hit_strip(y: i32) -> bool {
    (STRIP_Y..TOP).contains(&y)
}

/// The row under `y` at `scroll`.
pub fn row_at(y: i32, scroll: i32, n: usize) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let i = ((y - TOP + scroll) / kit::ROW_H()) as usize;
    (i < n).then_some(i)
}

/// How far a level of `n` rows scrolls.
pub fn max_scroll(n: usize) -> i32 {
    (TOP + n as i32 * kit::ROW_H() + 8 - H as i32).max(0)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &View) {
    c.fill(t.bg);
    let title = v
        .level
        .map(|l| l.title.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(crate::i18n::text("Media servers"));
    crate::chrome::header(c, t, f, title, None);
    let line = if !v.playing.is_empty() {
        crate::ui_format!("PLAYING · {song} · TAP TO STOP", song = v.playing)
    } else if v.busy {
        crate::i18n::text("LOADING…").to_string()
    } else if !v.note.is_empty() {
        v.note.to_string()
    } else if v.level.is_none_or(|l| l.title.is_empty()) {
        // The server list: a tap on the strip searches again.
        crate::i18n::text("TAP TO SEARCH AGAIN").to_string()
    } else {
        crate::i18n::text("ON THIS WI-FI").to_string()
    };
    kit::strip(c, t, f, STRIP_Y, &line);
    let entries = v.level.map(|l| l.entries.as_slice()).unwrap_or(&[]);
    let searchable = v.level.is_some_and(|l| l.searchable);
    let scroll = v.level.map_or(0, |l| l.scroll);
    if entries.is_empty() {
        // A level with a title is a server's folder; the untitled first level is the server list.
        let in_folder = v.level.is_some_and(|l| !l.title.is_empty());
        let (title, sub) = if v.busy && !in_folder {
            ("Looking for media servers…", "This takes a few seconds")
        } else if v.busy {
            ("Loading…", "")
        } else if in_folder {
            ("This folder is empty", "")
        } else {
            (
                "No media servers found",
                "They need to be on this Wi-Fi and share with UPnP or DLNA",
            )
        };
        let row = Row::new(crate::i18n::text(title)).sub(if sub.is_empty() {
            ""
        } else {
            crate::i18n::text(sub)
        });
        kit::row(c, t, f, TOP, kit::ROW_H(), &row);
        return;
    }
    c.set_clip_y(TOP, H as i32);
    for (i, e) in entries.iter().enumerate() {
        let y = TOP + i as i32 * kit::ROW_H() - scroll;
        if y + kit::ROW_H() < TOP || y >= H as i32 {
            continue;
        }
        // The search row is the app's words, so it is translated; a server's names are not.
        let mine = searchable && i == 0;
        let sub = if e.unplayable {
            crate::i18n::text("Not a format the player plays")
        } else if mine {
            crate::i18n::text(&e.sub)
        } else {
            e.sub.as_str()
        };
        let title = if mine {
            crate::i18n::text(&e.title)
        } else {
            e.title.as_str()
        };
        let row = Row::new(title).sub(sub).trail(if e.folder {
            Trail::Open("")
        } else {
            Trail::None
        });
        kit::row(c, t, f, y, kit::ROW_H(), &row);
    }
    c.clear_clip();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_start_under_the_strip_and_scroll() {
        assert!(hit_strip(STRIP_Y) && !hit_strip(TOP));
        assert_eq!(row_at(TOP - 1, 0, 5), None);
        assert_eq!(row_at(TOP, 0, 5), Some(0));
        assert_eq!(row_at(TOP + kit::ROW_H(), 0, 5), Some(1));
        assert_eq!(row_at(TOP, kit::ROW_H(), 5), Some(1));
        assert_eq!(row_at(TOP + 5 * kit::ROW_H(), 0, 5), None);
        assert_eq!(max_scroll(2), 0);
        assert!(max_scroll(200) > 0);
    }
}
