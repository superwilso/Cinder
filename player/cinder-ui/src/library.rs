//! Library — ported from cinder-proto-screens2.jsx `CLibrary` + `CArtist`.
//! Tabs (Songs / Albums / Artists / Playlists), each with a scope-aware accent
//! shuffle row, then the list. Plus the drill-in Artist page.

use crate::art;
use crate::canvas::{H, W};
use crate::data;
use crate::icons;
use crate::model::Library;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;
use embedded_graphics::pixelcolor::Rgb888;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tab {
    Songs,
    Albums,
    Artists,
    Playlists,
}

/// Conservative rows-per-page used by `nav` to keep the cursor on screen (the actual render
/// clips at the pixel budget, so this only needs to be ≤ the smallest tab's visible count).
pub const PAGE: usize = 7;

/// Bottom y of the scrollable list area (leave a hair at the panel edge).
const LIST_BOTTOM: i32 = H as i32 - 12;

const TABS: [(Tab, &str); 4] = [
    (Tab::Songs, "SONGS"),
    (Tab::Albums, "ALBUMS"),
    (Tab::Artists, "ARTISTS"),
    (Tab::Playlists, "PLAYLISTS"),
];

fn count_caption(tab: Tab, lib: &Library) -> String {
    match tab {
        Tab::Songs => format!("{} TRACKS", group_thousands(lib.songs.len())),
        Tab::Albums => format!("{} ALBUMS", lib.album_count()),
        Tab::Artists => format!("{} ARTISTS", lib.artists.len()),
        Tab::Playlists => format!("{} PLAYLISTS", lib.playlists.len()),
    }
}

/// 1842 -> "1,842".
fn group_thousands(n: usize) -> String {
    let s = n.to_string();
    let b = s.as_bytes();
    let mut out = String::new();
    for (i, ch) in b.iter().enumerate() {
        if i > 0 && (b.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*ch as char);
    }
    out
}

/// Number of selectable rows in a tab (for nav cursor clamping).
pub fn row_count(tab: Tab, lib: &Library) -> usize {
    match tab {
        Tab::Songs => lib.songs.len(),
        Tab::Albums => lib.album_count(),
        Tab::Artists => lib.artists.len(),
        Tab::Playlists => lib.playlists.len(),
    }
}

/// Thin scrollbar on the right edge showing window position over `total` rows.
pub(crate) fn scrollbar(c: &mut Canvas, t: &Theme, top: i32, first: usize, shown: usize, total: usize) {
    if total <= shown || shown == 0 {
        return;
    }
    let track_h = LIST_BOTTOM - top;
    if track_h <= 0 {
        return;
    }
    let x = W as i32 - 4;
    let thumb_h = ((shown as f32 / total as f32) * track_h as f32).max(18.0) as i32;
    let max_off = (total - shown) as f32;
    let pos = if max_off > 0.0 { first as f32 / max_off } else { 0.0 };
    let thumb_y = top + ((track_h - thumb_h) as f32 * pos) as i32;
    // faint full-height track + a brighter thumb so position is readable at a glance
    fill_rect(c, x, top, 3, track_h, t.line);
    fill_rect(c, x, thumb_y, 3, thumb_h, t.faint);
}

fn artdim(t: &Theme) -> f32 {
    if t.night { 0.30 } else { 1.0 }
}

/// The 4 now-playing indicator bars (FBars n=4).
fn tiny_bars(c: &mut Canvas, x: i32, cy: i32, acc: Rgb888) {
    let hs = [10, 14, 7, 12];
    for (i, h) in hs.iter().enumerate() {
        fill_rect(c, x + i as i32 * 5, cy + 7 - h, 3, *h, acc);
    }
}

/// Overlapping album-art stack (artist identity): one or two swatches.
fn art_stack(c: &mut Canvas, t: &Theme, x: i32, cy: i32, arts: &[&str]) {
    let op = artdim(t);
    if arts.len() == 1 {
        art::block(c, t, x, cy - 22, 44, 44, arts[0], op);
    } else {
        art::block(c, t, x + 18, cy - 24, 36, 36, arts[1], 0.55 * op);
        art::block(c, t, x, cy - 4, 40, 40, arts[0], op);
    }
}

/// Tab bar; returns the y where the shuffle row begins.
fn tabs(c: &mut Canvas, t: &Theme, f: &FontSet, y0: i32, active: Tab) -> i32 {
    let mut x = 22.0;
    for (tab, label) in TABS {
        let on = tab == active;
        let st = sty(Family::Mono, Weight::Regular, 11.0, if on { t.acc } else { t.faint }, 0.12);
        let w = text::measure(f, label, &st);
        text::draw(c, f, x, (y0 + 20) as f32, label, &st);
        if on {
            fill_rect(c, x as i32, y0 + 32, w as i32, 2, t.acc);
        }
        x += w + 22.0;
    }
    hline(c, y0 + 34, t.line);
    y0 + 34
}

/// Accent shuffle row (scope-aware). Returns the y below it.
fn shuffle_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str, sub: &str) -> i32 {
    let top = y + 16;
    let h = 56;
    fill_rect(c, 22, top, (W as i32) - 44, h, t.acc);
    let cy = top + h / 2;
    icons::shuffle(c, 42.0, cy as f32, 20.0, t.acc_ink);
    text::draw(c, f, 64.0, (cy - 4) as f32, label, &sty(Family::Sans, Weight::Bold, 15.0, t.acc_ink, 0.0));
    text::draw(c, f, 64.0, (cy + 14) as f32, sub, &sty(Family::Mono, Weight::Regular, 9.0, t.acc_ink, 0.06));
    icons::play(c, 438.0, cy as f32, 18.0, t.acc_ink);
    top + h
}

fn body_label(fam: Family, w: Weight, size: f32, col: Rgb888) -> TextStyle {
    sty(fam, w, size, col, 0.0)
}

pub const SORTS: [&str; 3] = ["TITLE", "ARTIST", "LENGTH"];

fn dur_secs(d: &str) -> i32 {
    let mut it = d.split(':');
    let m: i32 = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let s: i32 = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    m * 60 + s
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    tab: Tab,
    current: usize,
    scroll: usize,
    sort: usize,
    lib: &Library,
) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    // Songs tab shows a tappable SORT chip in the header's right slot; others show the count.
    let rc = if matches!(tab, Tab::Songs) {
        format!("SORT \u{00b7} {}", SORTS[sort.min(2)])
    } else {
        count_caption(tab, lib)
    };
    let y0 = crate::chrome::header(c, t, f, "Library", Some(&rc));
    let yt = tabs(c, t, f, y0, tab);
    let total = row_count(tab, lib);

    match tab {
        Tab::Songs => {
            let mut y = shuffle_row(c, t, f, yt, "Shuffle all songs",
                &format!("{} TRACKS · RANDOM ORDER", group_thousands(lib.songs.len()))) + 8;
            let top = y;
            let rh = 62;
            // sort order (indices into lib.songs)
            let mut order: Vec<usize> = (0..lib.songs.len()).collect();
            match sort {
                0 => order.sort_by(|&a, &b| lib.songs[a].title.cmp(&lib.songs[b].title)),
                1 => order.sort_by(|&a, &b| lib.songs[a].artist.cmp(&lib.songs[b].artist)),
                2 => order.sort_by(|&a, &b| dur_secs(&lib.songs[a].dur).cmp(&dur_secs(&lib.songs[b].dur))),
                _ => {}
            }
            let mut shown = 0;
            for rank in scroll..order.len() {
                if y + rh > LIST_BOTTOM {
                    break;
                }
                let i = order[rank];
                let sgn = &lib.songs[i];
                let cy = y + rh / 2;
                let now = rank == current;
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                art::block(c, t, 22, y + (rh - 42) / 2, 42, 42, &sgn.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 78.0, (cy - 2) as f32, &sgn.title, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
                text::draw(c, f, 78.0, (cy + 16) as f32, &sgn.artist, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                if now {
                    tiny_bars(c, 408, cy, t.acc);
                }
                right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
                hline(c, y + rh, t.line);
                y += rh;
                shown += 1;
            }
            scrollbar(c, t, top, scroll, shown, total);
        }
        Tab::Albums => {
            let y = shuffle_row(c, t, f, yt, "Shuffle by album", "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE") + 4;
            let top = y;
            let mut y = y;
            let rh = 60;
            let flat = lib.albums_flat();
            let mut shown = 0;
            let mut prev_artist: Option<&str> = None;
            for idx in scroll..flat.len() {
                let al = flat[idx];
                let need_header = prev_artist != Some(al.artist.as_str());
                let block_h = rh + if need_header { 30 } else { 0 };
                if y + block_h > LIST_BOTTOM {
                    break;
                }
                if need_header {
                    let label = al.artist.to_uppercase();
                    text::draw(c, f, 22.0, (y + 20) as f32, &label, &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.16));
                    y += 30;
                    prev_artist = Some(al.artist.as_str());
                }
                let now = idx == current;
                let cy = y + rh / 2;
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                art::block(c, t, 22, y + (rh - 44) / 2, 44, 44, &al.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 80.0, (cy - 2) as f32, &al.name, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
                let sub = format!("{} · {} tracks", al.year, al.tracks);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                stroke_rect(c, 416, cy - 19, 38, 38, t.line, 1);
                icons::shuffle(c, 435.0, cy as f32, 14.0, t.dim);
                hline(c, y + rh, t.line);
                y += rh;
                shown += 1;
            }
            scrollbar(c, t, top, scroll, shown, total);
        }
        Tab::Artists => {
            let y = shuffle_row(c, t, f, yt, "Shuffle by artist", "RANDOM ARTIST · SHUFFLED WITHIN ARTIST") + 8;
            let top = y;
            let mut y = y;
            let rh = 64;
            let mut shown = 0;
            for idx in scroll..lib.artists.len() {
                if y + rh > LIST_BOTTOM {
                    break;
                }
                let ar = &lib.artists[idx];
                let now = idx == current;
                let cy = y + rh / 2;
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                let arts: Vec<&str> = ar.arts.iter().map(|s| s.as_str()).collect();
                art_stack(c, t, 22, cy, &arts);
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 90.0, (cy - 2) as f32, &ar.name, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
                let sub = format!("{} albums · {} tracks", ar.albums, ar.tracks);
                text::draw(c, f, 90.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                stroke_rect(c, 414, cy - 20, 40, 40, t.line, 1);
                icons::shuffle(c, 434.0, cy as f32, 15.0, t.dim);
                hline(c, y + rh, t.line);
                y += rh;
                shown += 1;
            }
            scrollbar(c, t, top, scroll, shown, total);
        }
        Tab::Playlists => {
            let y = shuffle_row(c, t, f, yt, "Shuffle a playlist", "RANDOM PLAYLIST · SHUFFLED") + 8;
            let top = y;
            let mut y = y;
            let rh = 64;
            let mut shown = 0;
            for idx in scroll..lib.playlists.len() {
                if y + rh > LIST_BOTTOM {
                    break;
                }
                let pl = &lib.playlists[idx];
                let now = idx == current;
                let cy = y + rh / 2;
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                art::block(c, t, 22, y + (rh - 44) / 2, 44, 44, &pl.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 80.0, (cy - 2) as f32, &pl.name, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
                let sub = format!("{} tracks", pl.tracks);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
                hline(c, y + rh, t.line);
                y += rh;
                shown += 1;
            }
            scrollbar(c, t, top, scroll, shown, total);
        }
    }
}

/// Album drill-in: art + title/artist header, a shuffle row, then the windowed track list.
/// `track_idx` is the highlighted row, `scroll` the first visible row.
pub fn album_view(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    album: &crate::model::AlbumRow,
    track_idx: usize,
    scroll: usize,
) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    // back chevron + ALBUM eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ALBUM", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.2));
    // art block + title/artist/meta
    art::block(c, t, 22, 130, 96, 96, &album.art, artdim(t));
    let title = crate::widgets::fit(
        f, &album.name, &sty(Family::Sans, Weight::ExtraBold, 22.0, t.ink, -0.01), (W as f32) - 150.0,
    );
    text::draw(c, f, 132.0, 158.0, &title, &sty(Family::Sans, Weight::ExtraBold, 22.0, t.ink, -0.01));
    text::draw(c, f, 132.0, 182.0, &album.artist, &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
    let meta = if album.year.is_empty() {
        format!("{} TRACKS", album.tracks)
    } else {
        format!("{} · {} TRACKS", album.year, album.tracks)
    };
    text::draw(c, f, 132.0, 204.0, &meta, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.1));

    let mut y = shuffle_row(c, t, f, 234, "Play album", "IN ORDER · THEN SHUFFLE") + 6;
    let top = y;
    let rh = 56;
    let total = album.track_list.len();
    let mut shown = 0;
    for idx in scroll..album.track_list.len() {
        if y + rh > LIST_BOTTOM {
            break;
        }
        let sgn = &album.track_list[idx];
        let now = idx == track_idx;
        let cy = y + rh / 2;
        if now {
            fill_rect(c, 0, y, W as i32, rh, t.row_sel);
        }
        // track number
        let num = format!("{}", idx + 1);
        text::draw(c, f, 28.0, (cy + 4) as f32, &num,
            &sty(Family::Mono, Weight::Regular, 11.0, if now { t.acc } else { t.faint }, 0.0));
        let tcol = if now { t.acc } else { t.ink };
        text::draw(c, f, 56.0, (cy - 2) as f32, &sgn.title, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
        if now {
            tiny_bars(c, 408, cy, t.acc);
        }
        right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        hline(c, y + rh, t.line);
        y += rh;
        shown += 1;
    }
    scrollbar(c, t, top, scroll, shown, total);
}

/// Artist drill-in page (`CArtist`).
pub fn artist(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    // back + ARTIST eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ARTIST", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.2));
    // name + stats
    text::draw(c, f, 22.0, 150.0, data::ARTIST_NAME, &sty(Family::Sans, Weight::ExtraBold, 28.0, t.ink, -0.01));
    text::draw(c, f, 22.0, 173.0, data::ARTIST_STATS, &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.1));
    let y = shuffle_row(c, t, f, 180, "Shuffle artist", "ALL 34 TRACKS · RANDOM ORDER");

    // ALBUMS · 3
    text::draw(c, f, 22.0, (y + 28) as f32, "ALBUMS · 3", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    let ay = y + 40;
    let cw = (W as i32 - 44 - 24) / 3; // 3 across with 12px gaps
    for (i, al) in data::ARTIST_ALBUMS.iter().enumerate() {
        let ax = 22 + i as i32 * (cw + 12);
        art::block(c, t, ax, ay, cw, cw, al.art, artdim(t));
        let nst = body_label(Family::Sans, Weight::SemiBold, 12.0, t.ink);
        let name = crate::widgets::fit(f, al.n, &nst, cw as f32);
        text::draw(c, f, ax as f32, (ay + cw + 16) as f32, &name, &nst);
        text::draw(c, f, ax as f32, (ay + cw + 32) as f32, al.y, &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.0));
    }

    // TOP SONGS
    let mut sy = ay + cw + 48;
    text::draw(c, f, 22.0, sy as f32, "TOP SONGS", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    sy += 12;
    let rh = 54;
    for (i, sgn) in data::ARTIST_TOP.iter().enumerate() {
        let cy = sy + rh / 2;
        let now = i == 0;
        let col = if now { t.acc } else { t.faint };
        text::draw(c, f, 22.0, (cy + 4) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 11.0, col, 0.0));
        let tcol = if now { t.acc } else { t.ink };
        text::draw(c, f, 48.0, (cy - 2) as f32, sgn.t, &body_label(Family::Sans, Weight::SemiBold, 14.0, tcol));
        text::draw(c, f, 48.0, (cy + 15) as f32, sgn.al, &body_label(Family::Sans, Weight::Regular, 10.0, t.dim));
        if now {
            tiny_bars(c, 410, cy, t.acc);
        }
        right(c, f, 458.0, (cy + 4) as f32, sgn.d, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        hline(c, sy + rh, t.line);
        sy += rh;
    }
}
