//! Choosing a playlist's cover: automatic, an album already in the list, or a picture from the drive.
//!
//!   header    Cover                                       LATE NIGHT
//!   rows      [art] Automatic / The first song's cover
//!             [art] Album / artist                     (one per album in the list)
//!             [art] beach / Picture                    (one per picture in cinder_playlists)
//!   footer    Copy pictures into the cinder_playlists folder to use them
//!
//! Pictures come from the shell (`Library::cover_pictures`); the albums are the playlist's own.

use crate::canvas::{H, W};
use crate::model::{Library, PlaylistRow};
use crate::text::{self, FontSet};
use crate::theme::Theme;
use crate::widgets::{fill_rect, fit, hline, sty};
use crate::Canvas;

pub const ROW_H: i32 = 64;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM + 8;
/// Above the bottom edge, with room for the footer hint.
pub const BOTTOM: i32 = H as i32 - 40;
const ART: i32 = 48;

/// One thing the cover can be.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Auto,
    /// An album in the list, named by a member track (`object_id`) the shell resolves to a path.
    Album {
        album_id: i64,
        object_id: i64,
        name: String,
        artist: String,
    },
    /// `Library::cover_pictures[i]`.
    Picture(usize),
}

/// Automatic, then each album in list order, then every picture on the drive.
pub fn choices(lib: &Library, pl: &PlaylistRow) -> Vec<Choice> {
    let mut out = vec![Choice::Auto];
    for t in &pl.track_list {
        let seen = out
            .iter()
            .any(|c| matches!(c, Choice::Album { album_id, .. } if *album_id == t.album_id));
        if t.album_id != 0 && !seen {
            out.push(Choice::Album {
                album_id: t.album_id,
                object_id: t.object_id,
                name: t.art.clone(),
                artist: t.artist.clone(),
            });
        }
    }
    out.extend((0..lib.cover_pictures.len()).map(Choice::Picture));
    out
}

/// Is `c` the cover the playlist has now?
pub fn is_current(lib: &Library, pl: &PlaylistRow, c: &Choice) -> bool {
    let pic = lib.playlist_pictures.get(&pl.id);
    match c {
        Choice::Auto => !pl.cover_custom,
        Choice::Album { album_id, .. } => {
            pl.cover_custom && pic.is_none() && pl.cover_album_id == *album_id
        }
        Choice::Picture(i) => pic == Some(i),
    }
}

pub fn content_h(rows: usize) -> i32 {
    rows as i32 * ROW_H
}

pub fn max_scroll(rows: usize) -> i32 {
    (content_h(rows) - (BOTTOM - TOP)).max(0)
}

pub fn hit_row(rows: usize, scroll_px: i32, y: i32) -> Option<usize> {
    if !(TOP..BOTTOM).contains(&y) {
        return None;
    }
    let i = ((y - TOP + scroll_px.max(0)) / ROW_H) as usize;
    (i < rows).then_some(i)
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    pl: &PlaylistRow,
    choices: &[Choice],
    scroll_px: i32,
    sbar_active: bool,
) {
    c.fill(t.bg);
    crate::chrome::header(
        c,
        t,
        f,
        crate::i18n::text("Cover"),
        Some(&pl.name.to_uppercase()),
    );
    let scroll = scroll_px.clamp(0, max_scroll(choices.len()));
    c.set_clip_y(TOP, BOTTOM);
    let tx = (crate::kit::LEFT + ART + 14) as f32;
    let tw = crate::kit::RIGHT as f32 - tx;
    for (i, ch) in choices.iter().enumerate() {
        let y = TOP + i as i32 * ROW_H - scroll;
        if y + ROW_H <= TOP || y >= BOTTOM {
            continue;
        }
        let sel = is_current(lib, pl, ch);
        if sel {
            fill_rect(c, 0, y, W as i32, ROW_H, t.row_sel);
        }
        let ay = y + (ROW_H - ART) / 2;
        let ax = crate::kit::LEFT;
        let (title, sub): (String, String) = match ch {
            Choice::Auto => {
                match pl.track_list.first() {
                    Some(s) => {
                        crate::library::thumb(c, t, lib, s.album_id, &s.art, ax, ay, ART, 1.0)
                    }
                    None => crate::art::block_cached(c, t, ax, ay, ART, ART, &pl.art, 1.0),
                }
                (
                    crate::i18n::text("Automatic").into(),
                    crate::i18n::text("The first song's cover").into(),
                )
            }
            Choice::Album {
                album_id,
                name,
                artist,
                ..
            } => {
                crate::library::thumb(c, t, lib, *album_id, name, ax, ay, ART, 1.0);
                (name.clone(), artist.clone())
            }
            Choice::Picture(i) => {
                let p = &lib.cover_pictures[*i];
                if p.thumb.w == ART as usize && p.thumb.h == ART as usize {
                    crate::art::draw_image(c, t, ax, ay, &p.thumb, 1.0);
                } else {
                    crate::art::block_cached(c, t, ax, ay, ART, ART, &p.name, 1.0);
                }
                (p.name.clone(), crate::i18n::text("Picture").into())
            }
        };
        let cy = y + ROW_H / 2;
        let tst = sty(
            text::Family::Sans,
            text::Weight::SemiBold,
            crate::scale::ROW,
            if sel { t.acc } else { t.ink },
            0.0,
        );
        text::draw(c, f, tx, (cy - 3) as f32, &fit(f, &title, &tst, tw), &tst);
        let sst = sty(
            text::Family::Sans,
            text::Weight::Regular,
            crate::scale::SECONDARY,
            t.dim,
            0.0,
        );
        text::draw(
            c,
            f,
            tx,
            (cy + 18) as f32 + (text::scaled(20.0) - 20.0).max(0.0).ceil(),
            &fit(f, &sub, &sst, tw),
            &sst,
        );
        hline(c, y + ROW_H - 1, t.line);
    }
    c.clear_clip();
    crate::library::scrollbar(
        c,
        t,
        TOP,
        BOTTOM,
        scroll,
        content_h(choices.len()),
        sbar_active,
    );
    if lib.cover_pictures.is_empty() {
        let st = crate::kit::hint_style(t);
        let msg = fit(
            f,
            crate::i18n::text("Copy pictures into the cinder_playlists folder to use them"),
            &st,
            (crate::kit::RIGHT - crate::kit::LEFT) as f32,
        );
        text::draw(
            c,
            f,
            crate::kit::LEFT as f32,
            (BOTTOM + 24) as f32,
            &msg,
            &st,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SongRow;

    fn song(object_id: i64, album_id: i64, art: &str) -> SongRow {
        SongRow {
            object_id,
            album_id,
            art: art.into(),
            ..Default::default()
        }
    }

    /// Automatic first, each album once in list order, then the drive's pictures; the current one
    /// is the only one marked.
    #[test]
    fn choices_are_auto_then_albums_once_then_pictures() {
        let mut lib = Library::default();
        lib.cover_pictures.push(crate::model::CoverPicture {
            path: "/contents/cinder_playlists/beach.jpg".into(),
            name: "beach".into(),
            thumb: crate::art::Image {
                w: 0,
                h: 0,
                rgb: Vec::new(),
            },
        });
        let pl = PlaylistRow {
            id: -3,
            user: true,
            track_list: vec![song(1, 10, "A"), song(2, 20, "B"), song(3, 10, "A")],
            ..Default::default()
        };
        let got = choices(&lib, &pl);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], Choice::Auto);
        assert!(matches!(
            got[1],
            Choice::Album {
                album_id: 10,
                object_id: 1,
                ..
            }
        ));
        assert!(matches!(
            got[2],
            Choice::Album {
                album_id: 20,
                object_id: 2,
                ..
            }
        ));
        assert_eq!(got[3], Choice::Picture(0));
        assert!(is_current(&lib, &pl, &got[0]));

        let album = PlaylistRow {
            cover_custom: true,
            cover_album_id: 20,
            ..pl.clone()
        };
        let marked: Vec<bool> = got.iter().map(|c| is_current(&lib, &album, c)).collect();
        assert_eq!(marked, [false, false, true, false]);

        lib.playlist_pictures.insert(-3, 0);
        let marked: Vec<bool> = got.iter().map(|c| is_current(&lib, &album, c)).collect();
        assert_eq!(marked, [false, false, false, true]);
    }

    #[test]
    fn rows_hit_where_they_are_drawn() {
        assert_eq!(hit_row(3, 0, TOP), Some(0));
        assert_eq!(hit_row(3, 0, TOP + ROW_H), Some(1));
        assert_eq!(hit_row(3, 0, TOP + 3 * ROW_H), None);
        assert_eq!(hit_row(3, 0, TOP - 1), None);
        assert_eq!(hit_row(30, ROW_H, TOP), Some(1));
    }
}
