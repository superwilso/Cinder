//! Folder browse — the file tree as it actually is on the volume.
//!
//! Sony has this view and Cinder did not. It is the only place that answers "where did this file
//! come from", which on a player you fill over USB mass storage is a constant question: anything
//! the tag scanner filed under the wrong artist is still exactly where you put it.
//!
//! One screen, two kinds of row: subdirectories first, then the tracks sitting directly in the
//! current directory. That order is not cosmetic — a directory is a container and a track is a
//! leaf, and mixing them alphabetically makes a deep tree unreadable.
//!
//! The layout is expressed ONCE, in [`row_at`]/[`row_top`], and both `render` and the hit test
//! read it. Every recurring bug in this file's neighbours has been a render and a hit test that
//! each computed the same geometry and then drifted.

use crate::library::{scrollbar, LIST_BOTTOM};
use crate::model::{FolderRow, Library};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, sty};
use crate::{icons, Canvas};
use crate::canvas::W;

/// Screen-y of the first row.
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
/// Row height.
///
/// This used to claim it was "the same 56 the Songs tab uses, so a track row looks the same
/// wherever it is". The Songs tab is **68**. A track row is 68 there, 62 on the album, artist,
/// playlist and Up Next lists, and 56 here — three heights for one thing, and this comment was the
/// only place that said otherwise. Left at 56 deliberately rather than "corrected" to one of the
/// others: re-pitching a list is a design decision with a blast radius across every screen and the
/// whole overflow matrix, and it is recorded as such in `docs/AUDIT_2026-09-06_ui.md` rather than
/// made quietly here. What is fixed is the sentence that was false.
pub const ROW_H: i32 = 56;

/// What a given visual row is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// Descend into `Library::folders[i]`.
    Dir(usize),
    /// Play the track at `i` in the current directory's own track list.
    Track(usize),
}

/// The current directory's rows, in display order: subdirectories, then tracks.
pub fn rows(lib: &Library, dir: Option<usize>) -> Vec<Row> {
    let (subs, tracks): (&[usize], usize) = match dir {
        // The top level is the storage roots. When there is exactly one it is skipped by the
        // navigator, so this only draws when a microSD is present too.
        None => (&lib.folder_roots, 0),
        Some(i) => match lib.folders.get(i) {
            Some(f) => (&f.subdirs, f.tracks.len()),
            None => (&[], 0),
        },
    };
    let mut out = Vec::with_capacity(subs.len() + tracks);
    out.extend(subs.iter().map(|i| Row::Dir(*i)));
    out.extend((0..tracks).map(Row::Track));
    out
}

pub fn content_h(lib: &Library, dir: Option<usize>) -> i32 {
    rows(lib, dir).len() as i32 * ROW_H
}

pub fn max_scroll_px(lib: &Library, dir: Option<usize>) -> i32 {
    (content_h(lib, dir) - (LIST_BOTTOM - TOP)).max(0)
}

/// Screen-y of visual row `r` at `scroll`.
pub fn row_top(r: usize, scroll: i32) -> i32 {
    TOP + r as i32 * ROW_H - scroll
}

/// Which row is under `y`? `None` above/below the list, or past the last row.
pub fn row_at(lib: &Library, dir: Option<usize>, y: i32, scroll: i32) -> Option<Row> {
    if !(TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let r = ((y - TOP + scroll.max(0)) / ROW_H) as usize;
    rows(lib, dir).get(r).copied()
}

/// The header title for a directory: its own name, or the browse root.
pub fn title(lib: &Library, dir: Option<usize>) -> &str {
    match dir.and_then(|i| lib.folders.get(i)) {
        Some(f) => &f.name,
        None => "Folders",
    }
}

/// The header's right-hand caption — the full path, so a folder called "Disc 1" or "Disc 2" is not
/// ambiguous. Empty at the top of a volume, where the title IS the path and printing it twice says
/// nothing; empty at the root list, where the path would just be the word Folders again.
pub fn subtitle(lib: &Library, dir: Option<usize>) -> &str {
    match dir.and_then(|i| lib.folders.get(i)) {
        Some(f) if f.parent.is_some() => &f.path,
        _ => "",
    }
}

fn count_label(f: &FolderRow) -> String {
    match f.total {
        1 => "1 TRACK".to_string(),
        n => format!("{n} TRACKS"),
    }
}

fn name_style(t: &Theme, dir: bool) -> TextStyle {
    sty(Family::Sans, if dir { Weight::SemiBold } else { Weight::Regular }, 18.0,
        if dir { t.ink } else { t.ink }, 0.0)
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    dir: Option<usize>,
    scroll_px: i32,
    sbar_active: bool,
) {
    let scroll = scroll_px.clamp(0, max_scroll_px(lib, dir));
    c.fill(t.bg);
    // The path is long, so it goes in the header's right slot where it is already ellipsised
    // against the title rather than over the rows.
    let sub = subtitle(lib, dir);
    let y0 = crate::chrome::header(
        c, t, f, title(lib, dir),
        (!sub.is_empty()).then_some(sub),
    );
    c.set_clip_y(y0, LIST_BOTTOM);

    let all = rows(lib, dir);
    if all.is_empty() {
        let st = sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
        let msg = if lib.folders.is_empty() {
            "No folders — the library has not been read yet."
        } else {
            "This folder holds no tracks."
        };
        text::draw(c, f, 22.0, (TOP + 40) as f32, msg, &st);
        c.clear_clip();
        return;
    }

    // Only the visible window: a music root can hold hundreds of directories.
    let first = ((scroll / ROW_H).max(0)) as usize;
    for (r, row) in all.iter().enumerate().skip(first) {
        let y = row_top(r, scroll);
        if y >= LIST_BOTTOM {
            break;
        }
        let cy = y + ROW_H / 2;
        match row {
            Row::Dir(i) => {
                let Some(fr) = lib.folders.get(*i) else { continue };
                // A folder glyph, so the two row kinds are distinguishable without reading — the
                // chevron alone reads as "drill in" on a track row too.
                icons::library(c, 34.0, cy as f32, 20.0, t.acc);
                let ns = name_style(t, true);
                text::draw(c, f, 62.0, (cy + 6) as f32,
                           &crate::widgets::fit(f, &fr.name, &ns, 300.0), &ns);
                let cs = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.12);
                let cl = count_label(fr);
                let w = text::measure(f, &cl, &cs);
                text::draw(c, f, 440.0 - w, (cy + 5) as f32, &cl, &cs);
                icons::chevron(c, 452.0, cy as f32, 9.0, t.faint);
            }
            Row::Track(i) => {
                let Some(tr) = dir.and_then(|d| lib.folders.get(d)).and_then(|d| d.tracks.get(*i))
                else {
                    continue;
                };
                let ns = name_style(t, false);
                text::draw(c, f, 62.0, (cy + 1) as f32,
                           &crate::widgets::fit(f, &tr.title, &ns, 300.0), &ns);
                let ss = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
                text::draw(c, f, 62.0, (cy + 19) as f32,
                           &crate::widgets::fit(f, &tr.artist, &ss, 300.0), &ss);
                let ds = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.06);
                let w = text::measure(f, &tr.dur, &ds);
                text::draw(c, f, 458.0 - w, (cy + 5) as f32, &tr.dur, &ds);
                // A small tick in the gutter where the folder glyph sits, so the eye can find the
                // boundary between the two groups at a glance.
                fill_rect(c, 32, cy - 2, 5, 5, t.line);
            }
        }
        hline(c, y + ROW_H - 1, t.line);
    }
    c.clear_clip();
    scrollbar(c, t, TOP, LIST_BOTTOM, scroll, content_h(lib, dir), sbar_active);
    let _ = W;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SongRow;

    /// Two roots, one with a nested album folder, so the ordering rule and the hit test have
    /// something with both kinds of row in it.
    fn lib() -> Library {
        let track = |n: &str| SongRow { title: n.into(), dur: "3:00".into(), ..Default::default() };
        let folders = vec![
            FolderRow {
                path: "/contents".into(), name: "/contents".into(), parent: None,
                subdirs: vec![1], tracks: vec![track("loose")], total: 3,
            },
            FolderRow {
                path: "/contents/Album".into(), name: "Album".into(), parent: Some(0),
                subdirs: vec![], tracks: vec![track("a"), track("b")], total: 2,
            },
        ];
        Library { folders, folder_roots: vec![0], ..Default::default() }
    }

    /// Subdirectories come first, then the tracks that live directly here. A tap has to land on
    /// exactly the row that was drawn under it, at any scroll.
    #[test]
    fn directories_sort_above_tracks_and_the_hit_test_agrees() {
        let l = lib();
        assert_eq!(rows(&l, Some(0)), vec![Row::Dir(1), Row::Track(0)]);
        assert_eq!(rows(&l, Some(1)), vec![Row::Track(0), Row::Track(1)]);

        for (r, want) in rows(&l, Some(0)).into_iter().enumerate() {
            for scroll in [0, 7, ROW_H, ROW_H * 2 - 1] {
                let y = row_top(r, scroll) + ROW_H / 2;
                if (TOP..LIST_BOTTOM).contains(&y) {
                    assert_eq!(row_at(&l, Some(0), y, scroll), Some(want),
                               "row {r} at scroll {scroll}");
                }
            }
        }
        // Past the last row is nothing, not the last row again.
        let past = row_top(2, 0) + ROW_H / 2;
        assert_eq!(row_at(&l, Some(0), past, 0), None);
    }

    /// The count on a folder row is the whole subtree. A directory of directories would otherwise
    /// read "0 tracks" while holding a discography.
    #[test]
    fn a_folder_row_counts_everything_below_it() {
        let l = lib();
        assert_eq!(count_label(&l.folders[0]), "3 TRACKS");
        assert_eq!(count_label(&l.folders[1]), "2 TRACKS");
        let one = FolderRow { total: 1, ..Default::default() };
        assert_eq!(count_label(&one), "1 TRACK");
    }

    /// An empty library and an empty folder are different situations and must not scroll.
    #[test]
    fn an_empty_directory_has_nothing_to_scroll() {
        let empty = Library::default();
        assert!(rows(&empty, None).is_empty());
        assert_eq!(max_scroll_px(&empty, None), 0);
        assert_eq!(content_h(&empty, None), 0);
        // A directory index that no longer exists resolves to nothing rather than panicking.
        assert!(rows(&empty, Some(99)).is_empty());
        assert_eq!(title(&empty, Some(99)), "Folders");
    }
}
