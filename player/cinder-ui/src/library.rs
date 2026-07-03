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

// ── Pixel-scroll geometry ────────────────────────────────────────────────────────────────
// Lists scroll in PIXELS (live drag + fling), not rows: `scroll_px` is the content offset in
// px, rows render at `top - (scroll_px % rh)` under a clip band so partial rows are fine.
// These helpers give nav the shared geometry: list top / row height / total content height.

/// Top y of each tab's row area (below header+tabs+shuffle band).
pub fn list_top(tab: Tab) -> i32 {
    match tab {
        Tab::Albums => 201,
        _ => 205,
    }
}

/// Fixed row height per tab (Albums rows are 60 but carry extra 30px artist headers —
/// see `albums_layout`).
pub fn row_h(tab: Tab) -> i32 {
    match tab {
        Tab::Songs => 62,
        Tab::Albums => 60,
        Tab::Artists | Tab::Playlists => 64,
    }
}

/// Album drill-in track rows: top y and row height.
pub const ALBUM_TRACKS_TOP: i32 = 312;
pub const ALBUM_TRACK_RH: i32 = 56;

/// Virtual y (content px, 0 = list top) of each album row in the grouped Albums layout, plus
/// the total content height. Headers (30px) appear before the first album of each artist.
fn albums_layout(lib: &Library) -> (Vec<i32>, i32) {
    let flat = lib.albums_flat();
    let mut tops = Vec::with_capacity(flat.len());
    let mut vy = 0;
    let mut prev_artist: Option<&str> = None;
    for al in &flat {
        if prev_artist != Some(al.artist.as_str()) {
            vy += 30;
            prev_artist = Some(al.artist.as_str());
        }
        tops.push(vy);
        vy += 60;
    }
    (tops, vy)
}

/// Total scrollable content height (px) of a tab's list.
pub fn content_h(tab: Tab, lib: &Library) -> i32 {
    match tab {
        Tab::Albums => albums_layout(lib).1,
        _ => row_count(tab, lib) as i32 * row_h(tab),
    }
}

/// Largest useful `scroll_px` for a tab (0 when everything fits).
pub fn max_scroll_px(tab: Tab, lib: &Library) -> i32 {
    (content_h(tab, lib) - (LIST_BOTTOM - list_top(tab))).max(0)
}

/// Largest useful `scroll_px` for the album drill-in track list.
pub fn album_max_scroll_px(album: &crate::model::AlbumRow) -> i32 {
    (album.track_list.len() as i32 * ALBUM_TRACK_RH - (LIST_BOTTOM - ALBUM_TRACKS_TOP)).max(0)
}

/// Virtual y (content px) of row `idx` — for the cursor-follow used by button navigation.
pub fn row_top_px(tab: Tab, lib: &Library, idx: usize) -> i32 {
    match tab {
        Tab::Albums => albums_layout(lib).0.get(idx).copied().unwrap_or(0),
        _ => idx as i32 * row_h(tab),
    }
}

/// Visible list height for a tab (px).
pub fn view_h(tab: Tab) -> i32 {
    LIST_BOTTOM - list_top(tab)
}

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

/// The Songs tab's DRAW order (indices into `lib.songs`) for the given sort chip. Shared by
/// `render()` and every selection path (tap + button Select), so the row you act on is always
/// the row that was drawn — indexing `lib.songs` directly with a visual rank selects the wrong
/// song whenever the sort differs from DB order.
pub fn song_order(lib: &Library, sort: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..lib.songs.len()).collect();
    match sort {
        0 => order.sort_by(|&a, &b| lib.songs[a].title.cmp(&lib.songs[b].title)),
        1 => order.sort_by(|&a, &b| lib.songs[a].artist.cmp(&lib.songs[b].artist)),
        2 => order.sort_by(|&a, &b| dur_secs(&lib.songs[a].dur).cmp(&dur_secs(&lib.songs[b].dur))),
        _ => {}
    }
    order
}

/// The song actually SHOWN at `rank` in the sorted Songs list (rank = the `scroll`/`current`
/// index space the render uses).
pub fn song_at(lib: &Library, sort: usize, rank: usize) -> Option<&crate::model::SongRow> {
    song_order(lib, sort).get(rank).and_then(|&i| lib.songs.get(i))
}

/// Which list row sits under touch-`y`, given the current pixel scroll? Mirrors `render()`'s
/// per-tab layout EXACTLY: the tap's screen y is mapped into content space
/// (`y - top + scroll_px`) and resolved against the same geometry the renderer used —
/// including partially visible edge rows. None = chrome/gap/header/off-list.
pub fn hit_row(tab: Tab, lib: &Library, scroll_px: i32, y: i32) -> Option<usize> {
    let top = list_top(tab);
    if y < top || y >= LIST_BOTTOM {
        return None;
    }
    let cy = y - top + scroll_px.max(0); // content-space y
    match tab {
        Tab::Songs | Tab::Artists | Tab::Playlists => {
            let r = (cy / row_h(tab)) as usize;
            (r < row_count(tab, lib)).then_some(r)
        }
        Tab::Albums => {
            let (tops, _) = albums_layout(lib);
            for (idx, &vy) in tops.iter().enumerate() {
                if cy >= vy && cy < vy + 60 {
                    return Some(idx);
                }
            }
            None // an artist header band or past the end
        }
    }
}

/// Which track row of the album drill-in sits under touch-`y`? Rows from ALBUM_TRACKS_TOP
/// @56px in content space — mirrors `album_view()` at the given pixel scroll.
pub fn album_hit_track(album: &crate::model::AlbumRow, scroll_px: i32, y: i32) -> Option<usize> {
    if y < ALBUM_TRACKS_TOP || y >= LIST_BOTTOM {
        return None;
    }
    let cy = y - ALBUM_TRACKS_TOP + scroll_px.max(0);
    let r = (cy / ALBUM_TRACK_RH) as usize;
    (r < album.track_list.len()).then_some(r)
}

/// Thin scrollbar on the right edge: window position in PIXEL space (scroll_px over content_h).
pub(crate) fn scrollbar(c: &mut Canvas, t: &Theme, top: i32, scroll_px: i32, content_h: i32) {
    let track_h = LIST_BOTTOM - top;
    if track_h <= 0 || content_h <= track_h {
        return;
    }
    let x = W as i32 - 4;
    let thumb_h = ((track_h as f32 / content_h as f32) * track_h as f32).max(18.0) as i32;
    let max_off = (content_h - track_h) as f32;
    let pos = if max_off > 0.0 { (scroll_px as f32 / max_off).clamp(0.0, 1.0) } else { 0.0 };
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
        let st = sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc } else { t.faint }, 0.12);
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
    text::draw(c, f, 64.0, (cy - 4) as f32, label, &sty(Family::Sans, Weight::Bold, 16.0, t.acc_ink, 0.0));
    text::draw(c, f, 64.0, (cy + 14) as f32, sub, &sty(Family::Mono, Weight::Regular, 10.0, t.acc_ink, 0.06));
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
    scroll_px: i32,
    sort: usize,
    lib: &Library,
) {
    let scroll_px = scroll_px.clamp(0, max_scroll_px(tab, lib));
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
            let top = shuffle_row(c, t, f, yt, "Shuffle all songs",
                &format!("{} TRACKS · RANDOM ORDER", group_thousands(lib.songs.len()))) + 8;
            let rh = 62;
            let order = song_order(lib, sort); // shared with hit_row/selection — keep in sync
            let first = (scroll_px / rh) as usize;
            let mut y = top - (scroll_px % rh);
            c.set_clip_y(top, LIST_BOTTOM);
            for rank in first..order.len() {
                if y >= LIST_BOTTOM {
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
                text::draw(c, f, 78.0, (cy - 2) as f32, &sgn.title, &body_label(Family::Sans, Weight::SemiBold, 18.0, tcol));
                text::draw(c, f, 78.0, (cy + 16) as f32, &sgn.artist, &body_label(Family::Sans, Weight::Regular, 13.0, t.dim));
                if now {
                    tiny_bars(c, 408, cy, t.acc);
                }
                right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));
                hline(c, y + rh, t.line);
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh);
        }
        Tab::Albums => {
            let top = shuffle_row(c, t, f, yt, "Shuffle by album", "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE") + 4;
            let rh = 60;
            let flat = lib.albums_flat();
            let (tops, content) = albums_layout(lib);
            c.set_clip_y(top, LIST_BOTTOM);
            let mut prev_artist: Option<&str> = None;
            for (idx, al) in flat.iter().enumerate() {
                let need_header = prev_artist != Some(al.artist.as_str());
                prev_artist = Some(al.artist.as_str());
                let y = top + tops[idx] - scroll_px; // screen y of this album row
                if y + rh <= top {
                    continue; // fully above the window (its header is even higher)
                }
                if y >= LIST_BOTTOM {
                    break;
                }
                if need_header {
                    let label = al.artist.to_uppercase();
                    text::draw(c, f, 22.0, (y - 10) as f32, &label, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.16));
                }
                let now = idx == current;
                let cy = y + rh / 2;
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                art::block(c, t, 22, y + (rh - 44) / 2, 44, 44, &al.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 80.0, (cy - 2) as f32, &al.name, &body_label(Family::Sans, Weight::SemiBold, 18.0, tcol));
                let sub = format!("{} · {} tracks", al.year, al.tracks);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 13.0, t.dim));
                stroke_rect(c, 416, cy - 19, 38, 38, t.line, 1);
                icons::shuffle(c, 435.0, cy as f32, 14.0, t.dim);
                hline(c, y + rh, t.line);
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, content);
        }
        Tab::Artists => {
            let top = shuffle_row(c, t, f, yt, "Shuffle by artist", "RANDOM ARTIST · SHUFFLED WITHIN ARTIST") + 8;
            let rh = 64;
            let first = (scroll_px / rh) as usize;
            let mut y = top - (scroll_px % rh);
            c.set_clip_y(top, LIST_BOTTOM);
            for idx in first..lib.artists.len() {
                if y >= LIST_BOTTOM {
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
                text::draw(c, f, 90.0, (cy - 2) as f32, &ar.name, &body_label(Family::Sans, Weight::SemiBold, 18.0, tcol));
                let sub = format!("{} albums · {} tracks", ar.albums, ar.tracks);
                text::draw(c, f, 90.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 13.0, t.dim));
                stroke_rect(c, 414, cy - 20, 40, 40, t.line, 1);
                icons::shuffle(c, 434.0, cy as f32, 15.0, t.dim);
                hline(c, y + rh, t.line);
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh);
        }
        Tab::Playlists => {
            let top = shuffle_row(c, t, f, yt, "Shuffle a playlist", "RANDOM PLAYLIST · SHUFFLED") + 8;
            let rh = 64;
            let first = (scroll_px / rh) as usize;
            let mut y = top - (scroll_px % rh);
            c.set_clip_y(top, LIST_BOTTOM);
            for idx in first..lib.playlists.len() {
                if y >= LIST_BOTTOM {
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
                text::draw(c, f, 80.0, (cy - 2) as f32, &pl.name, &body_label(Family::Sans, Weight::SemiBold, 18.0, tcol));
                let sub = format!("{} tracks", pl.tracks);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 13.0, t.dim));
                icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
                hline(c, y + rh, t.line);
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh);
        }
    }
}

/// Album drill-in: art + title/artist header, a shuffle row, then the pixel-scrolled track
/// list. `track_idx` is the highlighted row, `scroll_px` the content offset in px.
pub fn album_view(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    album: &crate::model::AlbumRow,
    track_idx: usize,
    scroll_px: i32,
) {
    let scroll_px = scroll_px.clamp(0, album_max_scroll_px(album));
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

    let top = shuffle_row(c, t, f, 234, "Play album", "IN ORDER · THEN SHUFFLE") + 6;
    let rh = ALBUM_TRACK_RH;
    let total = album.track_list.len();
    let first = (scroll_px / rh) as usize;
    let mut y = top - (scroll_px % rh);
    c.set_clip_y(top, LIST_BOTTOM);
    for idx in first..album.track_list.len() {
        if y >= LIST_BOTTOM {
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
        text::draw(c, f, 56.0, (cy - 2) as f32, &sgn.title, &body_label(Family::Sans, Weight::SemiBold, 18.0, tcol));
        if now {
            tiny_bars(c, 408, cy, t.acc);
        }
        right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));
        hline(c, y + rh, t.line);
        y += rh;
    }
    c.clear_clip();
    scrollbar(c, t, top, scroll_px, total as i32 * rh);
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
        right(c, f, 458.0, (cy + 4) as f32, sgn.d, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));
        hline(c, sy + rh, t.line);
        sy += rh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AlbumRow, ArtistGroup, Library, SongRow};

    fn song(title: &str, artist: &str, dur: &str, id: i64) -> SongRow {
        SongRow {
            title: title.into(),
            artist: artist.into(),
            dur: dur.into(),
            art: String::new(),
            object_id: id,
        }
    }
    fn album(name: &str, artist: &str, n_tracks: usize) -> AlbumRow {
        AlbumRow {
            name: name.into(),
            artist: artist.into(),
            year: "2020".into(),
            tracks: n_tracks as u32,
            art: String::new(),
            album_id: 0,
            track_list: (0..n_tracks)
                .map(|i| song(&format!("t{i}"), artist, "3:00", 100 + i as i64))
                .collect(),
        }
    }
    fn lib() -> Library {
        Library {
            songs: vec![song("Charlie", "Zed", "9:00", 1), song("Alpha", "Mid", "1:00", 2), song("Bravo", "Aa", "5:00", 3)],
            album_groups: vec![
                ArtistGroup { artist: "One".into(), albums: vec![album("A1", "One", 3), album("A2", "One", 2)] },
                ArtistGroup { artist: "Two".into(), albums: vec![album("B1", "Two", 4)] },
            ],
            artists: Vec::new(),
            playlists: Vec::new(),
        }
    }

    #[test]
    fn song_at_follows_the_drawn_sort_order() {
        let l = lib();
        // sort 0 = TITLE: Alpha, Bravo, Charlie — rank 0 must be Alpha (id 2), NOT DB row 0.
        assert_eq!(song_at(&l, 0, 0).unwrap().object_id, 2);
        assert_eq!(song_at(&l, 0, 2).unwrap().object_id, 1);
        // sort 1 = ARTIST: Aa, Mid, Zed → Bravo first.
        assert_eq!(song_at(&l, 1, 0).unwrap().object_id, 3);
        // sort 2 = LENGTH: 1:00, 5:00, 9:00 → Alpha first.
        assert_eq!(song_at(&l, 2, 0).unwrap().object_id, 2);
        assert!(song_at(&l, 0, 3).is_none());
    }

    #[test]
    fn albums_hit_row_accounts_for_artist_headers() {
        let l = lib();
        // Content layout (px, 0 = list top 201): header(One) 0..30, A1 30..90, A2 90..150
        // (same artist, no header), header(Two) 150..180, B1 180..240.
        assert_eq!(hit_row(Tab::Albums, &l, 0, 210), None); // artist header — not a row
        assert_eq!(hit_row(Tab::Albums, &l, 0, 232), Some(0)); // A1
        assert_eq!(hit_row(Tab::Albums, &l, 0, 300), Some(1)); // A2 — no second header
        assert_eq!(hit_row(Tab::Albums, &l, 0, 360), None); // header(Two)
        assert_eq!(hit_row(Tab::Albums, &l, 0, 400), Some(2)); // B1
        assert_eq!(hit_row(Tab::Albums, &l, 0, 500), None); // below the content
        // Scrolled 60px (A1 off screen): the same screen y now lands on A2.
        assert_eq!(hit_row(Tab::Albums, &l, 60, 232), Some(1));
    }

    #[test]
    fn fixed_tabs_only_hit_drawn_rows() {
        let l = lib();
        // Songs rows: 205 @62. Row 0 = 205..267.
        assert_eq!(hit_row(Tab::Songs, &l, 0, 204), None); // shuffle band
        assert_eq!(hit_row(Tab::Songs, &l, 0, 206), Some(0));
        assert_eq!(hit_row(Tab::Songs, &l, 0, 266), Some(0));
        assert_eq!(hit_row(Tab::Songs, &l, 0, 268), Some(1));
        assert_eq!(hit_row(Tab::Songs, &l, 0, 700), None); // past the 3-song list
        // Pixel scroll: partially visible bottom rows ARE live (they're drawn under the clip),
        // but nothing below LIST_BOTTOM hits.
        assert_eq!(hit_row(Tab::Songs, &lib_many(), 0, 770), Some(9));
        assert_eq!(hit_row(Tab::Songs, &lib_many(), 0, 760), Some(8));
        assert_eq!(hit_row(Tab::Songs, &lib_many(), 0, 792), None); // >= LIST_BOTTOM
        // Half-row offset: screen y 206 maps into row 0's second half → still row 0 at 31px in.
        assert_eq!(hit_row(Tab::Songs, &lib_many(), 31, 206), Some(0));
        assert_eq!(hit_row(Tab::Songs, &lib_many(), 62, 206), Some(1));
    }

    fn lib_many() -> Library {
        Library {
            songs: (0..40).map(|i| song(&format!("s{i:02}"), "x", "3:00", i)).collect(),
            album_groups: Vec::new(),
            artists: Vec::new(),
            playlists: Vec::new(),
        }
    }

    #[test]
    fn album_track_hit_matches_album_view_geometry() {
        let l = lib();
        let flat = l.albums_flat();
        let a = flat[2]; // B1, 4 tracks; rows 312 @56: 312..368, 368..424, ...
        assert_eq!(album_hit_track(a, 0, 311), None); // Play-album band / gap
        assert_eq!(album_hit_track(a, 0, 313), Some(0));
        assert_eq!(album_hit_track(a, 0, 420), Some(1));
        assert_eq!(album_hit_track(a, 0, 500), Some(3)); // row 3 = 480..536
        assert_eq!(album_hit_track(a, 0, 640), None); // past the 4-track list
        assert_eq!(album_hit_track(a, 112, 313), Some(2)); // scrolled 2 rows (112 px)
    }

    #[test]
    fn pixel_scroll_geometry_helpers() {
        let l = lib();
        // Albums content: 2 headers (30) + 3 rows (60) = 240 px; fits in the view → no scroll.
        assert_eq!(content_h(Tab::Albums, &l), 240);
        assert_eq!(max_scroll_px(Tab::Albums, &l), 0);
        // Songs: 3 * 62 = 186 px, fits.
        assert_eq!(content_h(Tab::Songs, &l), 186);
        // 40 songs don't fit: max scroll = content - view, positive.
        let many = lib_many();
        let max = max_scroll_px(Tab::Songs, &many);
        assert_eq!(max, 40 * 62 - (LIST_BOTTOM - 205));
        assert!(max > 0);
        // Row-top helper replays the grouped Albums layout.
        assert_eq!(row_top_px(Tab::Albums, &l, 0), 30);
        assert_eq!(row_top_px(Tab::Albums, &l, 1), 90);
        assert_eq!(row_top_px(Tab::Albums, &l, 2), 180);
    }
}
