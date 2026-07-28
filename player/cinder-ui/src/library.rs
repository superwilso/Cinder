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

/// Bottom y of the scrollable list area — the top of the Now Playing return bar, which is pinned
/// to the bottom of every library screen. Derived, not a literal, so the list and the bar can't
/// overlap if the bar's height changes.
const LIST_BOTTOM: i32 = H as i32 - crate::chrome::NP_BAR_H;

// ── Pixel-scroll geometry ────────────────────────────────────────────────────────────────
// Lists scroll in PIXELS (live drag + fling), not rows: `scroll_px` is the content offset in
// px, rows render at `top - (scroll_px % rh)` under a clip band so partial rows are fine.
// These helpers give nav the shared geometry: list top / row height / total content height.

/// Draw an album thumbnail at `size`: the real decoded cover when the shell has one for this
/// `album_id`, else the generated gradient keyed by `name`.
///
/// One place, so every list row behaves identically while the background art cache fills in —
/// rows switch from gradient to cover the moment a thumbnail lands, with no layout change (the
/// cover occupies exactly the rect the gradient did). A thumbnail of the wrong size falls back
/// rather than scaling: resampling here would run per row per frame, which is what the cache
/// exists to avoid.
pub(crate) fn thumb(
    c: &mut Canvas, t: &Theme, lib: &Library, album_id: i64, name: &str,
    x: i32, y: i32, size: i32, op: f32,
) {
    match lib.thumbs.get(&album_id) {
        Some(img) if img.w == size as usize && img.h == size as usize => {
            art::draw_image(c, t, x, y, img, op)
        }
        _ => art::block(c, t, x, y, size, size, name, op),
    }
}

/// Y where the tab bar ends on the Library screen — `chrome::header` returns a fixed 91 and
/// `tabs` adds 34, so this is constant and both the renderer and the hit test can rely on it.
pub const TABS_BOTTOM: i32 = 125;

/// Rect `(x, y, w, h)` of the accent "Shuffle …" band that `shuffle_row` draws when called with
/// `y_below`. SINGLE SOURCE, like [`row_h`]: `shuffle_row` fills exactly this and
/// [`hit_shuffle_band`] tests exactly this, so the biggest touch target on the screen cannot
/// drift out from under the finger.
pub fn shuffle_band_rect(y_below: i32) -> (i32, i32, i32, i32) {
    (22, y_below + 16, W as i32 - 44, 56)
}

/// The Library-tab shuffle band (all four tabs draw it at the same place).
pub fn library_shuffle_band() -> (i32, i32, i32, i32) {
    shuffle_band_rect(TABS_BOTTOM)
}

/// The album drill-in's "Play album" band sits under the cover/title block.
pub const ALBUM_BAND_Y: i32 = 234;

/// The "Play album" band on the album drill-in.
pub fn album_play_band() -> (i32, i32, i32, i32) {
    shuffle_band_rect(ALBUM_BAND_Y)
}

/// True if `(x, y)` is inside the album drill-in's "Play album" band.
pub fn hit_album_play_band(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = album_play_band();
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

/// True if `(x, y)` is inside the Library shuffle band.
pub fn hit_shuffle_band(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = library_shuffle_band();
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

/// Top y of each tab's row area — derived from the band it sits under, so moving the band moves
/// the list (and the hit test) with it instead of desyncing.
pub fn list_top(tab: Tab) -> i32 {
    let (_, by, _, bh) = library_shuffle_band();
    let below = by + bh;
    match tab {
        Tab::Albums => below + 4,
        _ => below + 8,
    }
}

/// Fixed row height per tab (Albums rows are 60 but carry extra 30px artist headers —
/// see `albums_layout`).
/// Single source of truth for fixed-tab row heights — BOTH `render()` and the hit-test
/// (`hit_row`/`content_h`/`row_top_px`) read this, so a tap always resolves to the drawn row.
pub fn row_h(tab: Tab) -> i32 {
    match tab {
        Tab::Songs => 68,
        Tab::Albums => ALBUM_ROW_H,
        Tab::Artists | Tab::Playlists => 70,
    }
}

// ── A–Z jump strip ────────────────────────────────────────────────────────────────────────────
// A 300-album library is ~20 screens of flicking. The stock player has no way to jump, and Wampy
// structurally can't add one (it drives Sony's app and never owns the list). Cinder owns the list,
// so it gets the iPod-style alphabet rail: tap or drag the letters down the right edge to jump.
// Touch-native, which matters on a device with no d-pad.
pub const AZ_W: i32 = 26;            // rail width — narrow enough not to eat row taps
pub const AZ_LETTERS: &[u8] = b"#ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The sort key a row is indexed by, per tab. Matches what the list is ordered by, so a jump always
/// lands where the eye expects: Songs by title, Albums by artist (grouped) or album name, Artists
/// by artist name, Playlists by playlist name.
fn az_key(c: char) -> u8 {
    let u = c.to_ascii_uppercase();
    if u.is_ascii_alphabetic() { u as u8 } else { b'#' }
}

/// First letter of `s`, normalised: leading "The " is skipped (it is a sort-order artefact, not how
/// anyone looks a band up), and anything non-alphabetic buckets under '#'.
pub fn az_bucket(s: &str) -> u8 {
    let t = s.trim();
    let t = t.strip_prefix("The ").or_else(|| t.strip_prefix("the ")).unwrap_or(t);
    t.chars().next().map(az_key).unwrap_or(b'#')
}

/// Which letter is at screen y on the rail? None when y is outside it.
pub fn az_letter_at(y: i32, tab: Tab) -> Option<u8> {
    let top = list_top(tab);
    let h = LIST_BOTTOM - top;
    if y < top || y >= LIST_BOTTOM {
        return None;
    }
    let n = AZ_LETTERS.len() as i32;
    let i = ((y - top) * n / h).clamp(0, n - 1);
    Some(AZ_LETTERS[i as usize])
}

/// Is screen x inside the rail?
pub fn az_hit_x(x: i32) -> bool {
    x >= W as i32 - AZ_W
}

/// Album drill-in track rows: top y and row height.
pub const ALBUM_TRACKS_TOP: i32 = 312;
pub const ALBUM_TRACK_RH: i32 = 62;

// ── Albums tab: sortable + expandable (accordion) display list ────────────────────────────
// The Albums tab is a single flat list of variable-height rows: an artist header (grouped sort
// only), an album row, or the track rows of the one expanded album. `albums_build` produces that
// list (with content-space y tops) once per render/hit; layout/hit/scroll all read from it so the
// three can never drift.
pub const ALBUM_HDR_H: i32 = 34; // artist section header (grouped sort)
pub const ALBUM_ROW_H: i32 = 68; // an album row
pub const ALBUM_CHILD_H: i32 = 50; // an expanded track row (indented under its album)
/// A tap on an album row left of this x opens the drill-in page (cover art); right of it toggles
/// the inline accordion. Keeps both affordances on one row.
pub const ALBUM_ART_HIT_X: i32 = 72;

/// One row in the Albums tab's flat display list.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlbumsRow {
    /// Artist section header (grouped sort only). `flat` = the first album under it (for its name).
    Group { flat: usize },
    /// An album row. `flat` = index into `lib.albums_flat()`; `expanded` = its accordion is open.
    Album { flat: usize, expanded: bool },
    /// A track inside the expanded album. `flat` = album index, `track` = track index in it.
    Track { flat: usize, track: usize },
}

/// The built Albums display list: each row with its content-space top y, plus total height.
pub struct AlbumsLayout {
    pub rows: Vec<(i32, AlbumsRow)>,
    pub content_h: i32,
}

/// Display order (indices into `lib.albums_flat()`) for the Albums ORDER chip. Sort 0 keeps the
/// artist-then-name order `albums_flat()` already has; 1-3 are flat re-orders. Ties break on name.
pub fn album_display_order(lib: &Library, sort: usize) -> Vec<usize> {
    let flat = lib.albums_flat();
    let mut order: Vec<usize> = (0..flat.len()).collect();
    let year = |i: usize| flat[i].year.trim().parse::<i32>().unwrap_or(0);
    match sort {
        1 => order.sort_by(|&a, &b| flat[a].name.cmp(&flat[b].name)),
        2 => order.sort_by(|&a, &b| flat[b].added.cmp(&flat[a].added).then_with(|| flat[a].name.cmp(&flat[b].name))),
        3 => order.sort_by(|&a, &b| year(b).cmp(&year(a)).then_with(|| flat[a].name.cmp(&flat[b].name))),
        _ => {} // 0 = ARTIST: albums_flat() is already artist-then-name
    }
    order
}

/// Build the Albums display list for the given ORDER + the expanded album (a `albums_flat()`
/// index; None = all collapsed). Group headers appear only in grouped sort (0).
pub fn albums_build(lib: &Library, sort: usize, expanded: Option<usize>) -> AlbumsLayout {
    let flat = lib.albums_flat();
    let order = album_display_order(lib, sort);
    let grouped = sort == 0;
    let mut rows: Vec<(i32, AlbumsRow)> = Vec::with_capacity(order.len());
    let mut vy = 0;
    let mut prev_artist: Option<&str> = None;
    for &fi in &order {
        let al = flat[fi];
        if grouped && prev_artist != Some(al.artist.as_str()) {
            rows.push((vy, AlbumsRow::Group { flat: fi }));
            vy += ALBUM_HDR_H;
            prev_artist = Some(al.artist.as_str());
        }
        let is_exp = expanded == Some(fi) && !al.track_list.is_empty();
        rows.push((vy, AlbumsRow::Album { flat: fi, expanded: is_exp }));
        vy += ALBUM_ROW_H;
        if is_exp {
            for track in 0..al.track_list.len() {
                rows.push((vy, AlbumsRow::Track { flat: fi, track }));
                vy += ALBUM_CHILD_H;
            }
        }
    }
    AlbumsLayout { rows, content_h: vy }
}

/// What a tap on the Albums tab hit (content mapped from screen y at the current scroll).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlbumsHit {
    /// Album row body (right of the art) — toggle its accordion. `flat` index.
    AlbumToggle(usize),
    /// Album row art (left) — open the drill-in album page. `flat` index.
    AlbumOpen(usize),
    /// An expanded track row — play it. (`flat` album index, `track` index.)
    Track(usize, usize),
}

/// Which Albums display element sits under touch `(x, y)` at the given pixel scroll? None for the
/// shuffle band / group headers / gaps. Mirrors `render`'s layout exactly (built from the same
/// `albums_build`).
pub fn albums_hit(
    lib: &Library,
    sort: usize,
    expanded: Option<usize>,
    scroll_px: i32,
    x: i32,
    y: i32,
) -> Option<AlbumsHit> {
    let top = list_top(Tab::Albums);
    if y < top || y >= LIST_BOTTOM {
        return None;
    }
    let cy = y - top + scroll_px.max(0);
    let layout = albums_build(lib, sort, expanded);
    for (vy, row) in &layout.rows {
        let h = match row {
            AlbumsRow::Group { .. } => ALBUM_HDR_H,
            AlbumsRow::Album { .. } => ALBUM_ROW_H,
            AlbumsRow::Track { .. } => ALBUM_CHILD_H,
        };
        if cy >= *vy && cy < *vy + h {
            return match *row {
                AlbumsRow::Group { .. } => None,
                AlbumsRow::Album { flat, .. } => Some(if x < ALBUM_ART_HIT_X {
                    AlbumsHit::AlbumOpen(flat)
                } else {
                    AlbumsHit::AlbumToggle(flat)
                }),
                AlbumsRow::Track { flat, track } => Some(AlbumsHit::Track(flat, track)),
            };
        }
    }
    None
}

/// Total scrollable content height (px) of a tab's list. Albums depends on its ORDER + which
/// album is expanded (variable-height accordion); the fixed tabs are `rows * row_h`.
pub fn content_h(tab: Tab, lib: &Library, album_sort: usize, album_expanded: Option<usize>) -> i32 {
    match tab {
        Tab::Albums => albums_build(lib, album_sort, album_expanded).content_h,
        _ => row_count(tab, lib) as i32 * row_h(tab),
    }
}

/// Scroll offset that puts the first row of bucket `letter` at the top of the list, clamped to the
/// tab's scroll range. None when the tab has no row in that bucket (the rail greys those out, so a
/// tap on one is a no-op rather than a confusing jump to the nearest neighbour).
///
/// Reads the SAME layout the render and hit test use (`albums_build` for Albums, `row_h` elsewhere),
/// so a jump can't land somewhere the drawn list disagrees with.
pub fn az_scroll_for(
    tab: Tab,
    lib: &Library,
    letter: u8,
    album_sort: usize,
    album_expanded: Option<usize>,
) -> Option<i32> {
    let max = max_scroll_px(tab, lib, album_sort, album_expanded);
    let top_px = match tab {
        Tab::Albums => {
            let flat = lib.albums_flat();
            let layout = albums_build(lib, album_sort, album_expanded);
            // Grouped sort indexes by ARTIST (that's the visible ordering); every other album sort
            // is by album name.
            layout.rows.iter().find_map(|(vy, row)| match row {
                AlbumsRow::Group { flat: fi } if album_sort == 0 => {
                    (az_bucket(&flat[*fi].artist) == letter).then_some(*vy)
                }
                AlbumsRow::Album { flat: fi, .. } if album_sort != 0 => {
                    (az_bucket(&flat[*fi].name) == letter).then_some(*vy)
                }
                _ => None,
            })?
        }
        Tab::Songs => {
            let i = lib.songs.iter().position(|r| az_bucket(&r.title) == letter)?;
            i as i32 * row_h(tab)
        }
        Tab::Artists => {
            let i = lib.artists.iter().position(|r| az_bucket(&r.name) == letter)?;
            i as i32 * row_h(tab)
        }
        Tab::Playlists => {
            let i = lib.playlists.iter().position(|r| az_bucket(&r.name) == letter)?;
            i as i32 * row_h(tab)
        }
    };
    Some(top_px.clamp(0, max))
}

/// Does this tab have any row in `letter`'s bucket? Drives the rail's greying.
pub fn az_has(
    tab: Tab,
    lib: &Library,
    letter: u8,
    album_sort: usize,
    album_expanded: Option<usize>,
) -> bool {
    az_scroll_for(tab, lib, letter, album_sort, album_expanded).is_some()
}

/// Largest useful `scroll_px` for a tab (0 when everything fits).
pub fn max_scroll_px(tab: Tab, lib: &Library, album_sort: usize, album_expanded: Option<usize>) -> i32 {
    (content_h(tab, lib, album_sort, album_expanded) - (LIST_BOTTOM - list_top(tab))).max(0)
}

/// Largest useful `scroll_px` for the album drill-in track list.
pub fn album_max_scroll_px(album: &crate::model::AlbumRow) -> i32 {
    (album.track_list.len() as i32 * ALBUM_TRACK_RH - (LIST_BOTTOM - ALBUM_TRACKS_TOP)).max(0)
}

/// Virtual y (content px) of selectable row `idx` — for the cursor-follow used by button nav. For
/// Albums, `idx` is the ALBUM display rank (0-based over albums, ignoring headers/tracks).
pub fn row_top_px(tab: Tab, lib: &Library, idx: usize, album_sort: usize, album_expanded: Option<usize>) -> i32 {
    match tab {
        Tab::Albums => {
            let layout = albums_build(lib, album_sort, album_expanded);
            let mut rank = 0;
            for (vy, row) in &layout.rows {
                if let AlbumsRow::Album { .. } = row {
                    if rank == idx {
                        return *vy;
                    }
                    rank += 1;
                }
            }
            0
        }
        _ => idx as i32 * row_h(tab),
    }
}

/// Visible list height for a tab (px).
pub fn view_h(tab: Tab) -> i32 {
    LIST_BOTTOM - list_top(tab)
}

/// The `albums_flat()` index of the album at ALBUM display rank `idx` under the given ORDER
/// (button nav / drill-in resolution). None if out of range.
pub fn album_flat_at_rank(lib: &Library, sort: usize, idx: usize) -> Option<usize> {
    album_display_order(lib, sort).get(idx).copied()
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
/// song whenever the sort differs from DB order. Secondary key = title, so ties are stable.
pub fn song_order(lib: &Library, sort: usize) -> Vec<usize> {
    let s = &lib.songs;
    let mut order: Vec<usize> = (0..s.len()).collect();
    let by_title = |a: usize, b: usize| s[a].title.cmp(&s[b].title);
    match sort {
        0 => order.sort_by(|&a, &b| by_title(a, b)),
        1 => order.sort_by(|&a, &b| s[a].artist.cmp(&s[b].artist).then_with(|| by_title(a, b))),
        2 => order.sort_by(|&a, &b| s[b].artist.cmp(&s[a].artist).then_with(|| by_title(a, b))),
        3 => order.sort_by(|&a, &b| dur_secs(&s[a].dur).cmp(&dur_secs(&s[b].dur)).then_with(|| by_title(a, b))),
        // Recently added: newest addedtime first.
        4 => order.sort_by(|&a, &b| s[b].added.cmp(&s[a].added).then_with(|| by_title(a, b))),
        // Album order: album, then disc/track within it.
        5 => order.sort_by(|&a, &b| {
            (s[a].album_id, s[a].disc, s[a].track).cmp(&(s[b].album_id, s[b].disc, s[b].track))
        }),
        // Release year: newest first (0 = unresolved years sink to the bottom).
        6 => order.sort_by(|&a, &b| s[b].year.cmp(&s[a].year).then_with(|| by_title(a, b))),
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
        // Albums is a variable-height accordion — its taps go through `albums_hit`, not here.
        Tab::Albums => None,
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
        let st = sty(Family::Mono, Weight::Regular, 14.0, if on { t.acc } else { t.faint }, 0.12);
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
    let (bx, top, bw, h) = shuffle_band_rect(y);
    fill_rect(c, bx, top, bw, h, t.acc);
    let cy = top + h / 2;
    icons::shuffle(c, 42.0, cy as f32, 20.0, t.acc_ink);
    text::draw(c, f, 64.0, (cy - 4) as f32, label, &sty(Family::Sans, Weight::Bold, 18.0, t.acc_ink, 0.0));
    text::draw(c, f, 64.0, (cy + 14) as f32, sub, &sty(Family::Mono, Weight::Regular, 12.0, t.acc_ink, 0.06));
    icons::play(c, 438.0, cy as f32, 18.0, t.acc_ink);
    top + h
}

fn body_label(fam: Family, w: Weight, size: f32, col: Rgb888) -> TextStyle {
    sty(fam, w, size, col, 0.0)
}

/// Songs-tab SORT chip labels (ASCII only — the mono font has no arrow glyphs). The chip cycles
/// through these; `song_order` implements each index. Kept in lock-step with `song_order`.
pub const SORTS: [&str; 7] =
    ["TITLE", "ARTIST A-Z", "ARTIST Z-A", "LENGTH", "ADDED", "ALBUM", "YEAR"];

/// Albums-tab ORDER chip labels. Index 0 = the classic artist-grouped view (with section
/// headers); 1-3 are flat lists in the named order. `album_display_order` implements each.
pub const ALBUM_SORTS: [&str; 4] = ["ARTIST", "A-Z", "ADDED", "YEAR"];

fn dur_secs(d: &str) -> i32 {
    let mut it = d.split(':');
    let m: i32 = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let s: i32 = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    m * 60 + s
}

#[allow(clippy::too_many_arguments)]
/// Draw the A–Z rail down the right edge of the list. Letters with no rows are drawn faint, so the
/// rail also reads as a map of what the library actually contains.
pub fn az_render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    tab: Tab,
    lib: &Library,
    album_sort: usize,
    album_expanded: Option<usize>,
) {
    let top = list_top(tab);
    let h = LIST_BOTTOM - top;
    let n = AZ_LETTERS.len() as i32;
    let x = W as i32 - AZ_W / 2;
    for (i, &ch) in AZ_LETTERS.iter().enumerate() {
        let cy = top + (i as i32 * h) / n + h / (2 * n);
        let has = az_has(tab, lib, ch, album_sort, album_expanded);
        let col = if has { t.dim } else { t.faint };
        let label = (ch as char).to_string();
        let st = sty(Family::Mono, Weight::Regular, 11.0, col, 0.0);
        let w = text::measure(f, &label, &st) as i32;
        text::draw(c, f, (x - w / 2) as f32, (cy + 4) as f32, &label, &st);
    }
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    tab: Tab,
    current: usize,
    scroll_px: i32,
    sort: usize,
    album_sort: usize,
    album_expanded: Option<usize>,
    lib: &Library,
) {
    let scroll_px = scroll_px.clamp(0, max_scroll_px(tab, lib, album_sort, album_expanded));
    c.fill(t.bg);
    // Songs shows a tappable SORT chip; Albums an ORDER chip; the others show their count.
    let rc = match tab {
        Tab::Songs => format!("SORT \u{00b7} {}", SORTS[sort.min(SORTS.len() - 1)]),
        Tab::Albums => format!("ORDER \u{00b7} {}", ALBUM_SORTS[album_sort.min(ALBUM_SORTS.len() - 1)]),
        _ => count_caption(tab, lib),
    };
    let y0 = crate::chrome::header(c, t, f, "Library", Some(&rc));
    let yt = tabs(c, t, f, y0, tab);
    let total = row_count(tab, lib);

    match tab {
        Tab::Songs => {
            let top = shuffle_row(c, t, f, yt, "Shuffle all songs",
                &format!("{} TRACKS · RANDOM ORDER", group_thousands(lib.songs.len()))) + 8;
            let rh = row_h(Tab::Songs);
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
                thumb(c, t, lib, sgn.album_id, &sgn.art, 22, y + (rh - 48) / 2, 48, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                let tst = body_label(Family::Sans, Weight::SemiBold, 20.0, tcol);
                text::draw(c, f, 78.0, (cy - 2) as f32, &crate::widgets::fit(f, &sgn.title, &tst, 300.0), &tst);
                let ast = body_label(Family::Sans, Weight::Regular, 15.0, t.dim);
                text::draw(c, f, 78.0, (cy + 16) as f32, &crate::widgets::fit(f, &sgn.artist, &ast, 320.0), &ast);
                if now {
                    tiny_bars(c, 386, cy, t.acc);
                }
                right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
                hline(c, y + rh, t.line);
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh);
        }
        Tab::Albums => {
            let top = shuffle_row(c, t, f, yt, "Shuffle by album", "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE") + 4;
            let flat = lib.albums_flat();
            let layout = albums_build(lib, album_sort, album_expanded);
            c.set_clip_y(top, LIST_BOTTOM);
            let mut rank = 0; // album display rank (skips headers/tracks) — matches the button cursor
            for (vy, row) in &layout.rows {
                let h = match row {
                    AlbumsRow::Group { .. } => ALBUM_HDR_H,
                    AlbumsRow::Album { .. } => ALBUM_ROW_H,
                    AlbumsRow::Track { .. } => ALBUM_CHILD_H,
                };
                let y = top + *vy - scroll_px;
                if y + h <= top {
                    if let AlbumsRow::Album { .. } = row {
                        rank += 1;
                    }
                    continue; // fully above the window
                }
                if y >= LIST_BOTTOM {
                    break;
                }
                match *row {
                    AlbumsRow::Group { flat: fi } => {
                        let label = flat[fi].artist.to_uppercase();
                        text::draw(c, f, 22.0, (y + 20) as f32, &label,
                            &sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.16));
                    }
                    AlbumsRow::Album { flat: fi, expanded } => {
                        let al = flat[fi];
                        let now = rank == current;
                        rank += 1;
                        let cy = y + ALBUM_ROW_H / 2;
                        if now {
                            fill_rect(c, 0, y, W as i32, ALBUM_ROW_H, t.row_sel);
                        }
                        thumb(c, t, lib, al.album_id, &al.art, 22, y + (ALBUM_ROW_H - 48) / 2, 48, artdim(t));
                        let tcol = if now { t.acc } else { t.ink };
                        text::draw(c, f, 80.0, (cy - 2) as f32, &al.name,
                            &body_label(Family::Sans, Weight::SemiBold, 20.0, tcol));
                        let sub = if al.year.is_empty() {
                            format!("{} tracks", al.tracks)
                        } else {
                            format!("{} · {} tracks", al.year, al.tracks)
                        };
                        text::draw(c, f, 80.0, (cy + 16) as f32, &sub,
                            &body_label(Family::Sans, Weight::Regular, 15.0, t.dim));
                        // Accordion caret (right): points down when open, right when closed.
                        let caret = if expanded { t.acc } else { t.faint };
                        icons::chevron(c, 452.0, cy as f32, 13.0, caret);
                        hline(c, y + ALBUM_ROW_H, t.line);
                    }
                    AlbumsRow::Track { flat: fi, track } => {
                        let al = flat[fi];
                        if let Some(sgn) = al.track_list.get(track) {
                            let cy = y + ALBUM_CHILD_H / 2;
                            // subtle inset band so tracks read as children of the album above
                            fill_rect(c, 0, y, W as i32, ALBUM_CHILD_H, t.panel);
                            let num = format!("{}", track + 1);
                            text::draw(c, f, 84.0, (cy + 4) as f32, &num,
                                &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.0));
                            let cst = body_label(Family::Sans, Weight::Regular, 17.0, t.ink);
                            text::draw(c, f, 112.0, (cy + 4) as f32,
                                &crate::widgets::fit(f, &sgn.title, &cst, 296.0), &cst);
                            right(c, f, 452.0, (cy + 4) as f32, &sgn.dur,
                                &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.0));
                            hline(c, y + ALBUM_CHILD_H, t.line);
                        }
                    }
                }
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, layout.content_h);
        }
        Tab::Artists => {
            let top = shuffle_row(c, t, f, yt, "Shuffle by artist", "RANDOM ARTIST · SHUFFLED WITHIN ARTIST") + 8;
            let rh = row_h(Tab::Artists);
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
                text::draw(c, f, 90.0, (cy - 2) as f32, &ar.name, &body_label(Family::Sans, Weight::SemiBold, 20.0, tcol));
                let sub = format!("{} albums · {} tracks", ar.albums, ar.tracks);
                text::draw(c, f, 90.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 15.0, t.dim));
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
            let rh = row_h(Tab::Playlists);
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
                art::block(c, t, 22, y + (rh - 48) / 2, 48, 48, &pl.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 80.0, (cy - 2) as f32, &pl.name, &body_label(Family::Sans, Weight::SemiBold, 20.0, tcol));
                let sub = format!("{} tracks", pl.tracks);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 15.0, t.dim));
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
/// `cover` is the album's decoded art at exactly 96x96, or None to draw the gradient. The shell
/// loads it from the art cache when the drill-in opens — one image, not a map, because only one
/// album is ever open.
pub fn album_view(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    album: &crate::model::AlbumRow,
    track_idx: usize,
    scroll_px: i32,
    cover: Option<&crate::art::Image>,
) {
    let scroll_px = scroll_px.clamp(0, album_max_scroll_px(album));
    c.fill(t.bg);
    // back chevron + ALBUM eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ALBUM", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.2));
    // art block + title/artist/meta
    match cover {
        Some(img) if img.w == 96 && img.h == 96 => art::draw_image(c, t, 22, 130, img, artdim(t)),
        _ => art::block(c, t, 22, 130, 96, 96, &album.art, artdim(t)),
    }
    let title = crate::widgets::fit(
        f, &album.name, &sty(Family::Sans, Weight::ExtraBold, 24.0, t.ink, -0.01), (W as f32) - 150.0,
    );
    text::draw(c, f, 132.0, 158.0, &title, &sty(Family::Sans, Weight::ExtraBold, 24.0, t.ink, -0.01));
    text::draw(c, f, 132.0, 182.0, &album.artist, &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0));
    let meta = if album.year.is_empty() {
        format!("{} TRACKS", album.tracks)
    } else {
        format!("{} · {} TRACKS", album.year, album.tracks)
    };
    text::draw(c, f, 132.0, 204.0, &meta, &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));

    let top = shuffle_row(c, t, f, ALBUM_BAND_Y, "Play album", "IN ORDER · THEN SHUFFLE") + 6;
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
            &sty(Family::Mono, Weight::Regular, 13.0, if now { t.acc } else { t.faint }, 0.0));
        let tcol = if now { t.acc } else { t.ink };
        let tst = body_label(Family::Sans, Weight::SemiBold, 20.0, tcol);
        text::draw(c, f, 56.0, (cy - 2) as f32, &crate::widgets::fit(f, &sgn.title, &tst, 320.0), &tst);
        if now {
            tiny_bars(c, 386, cy, t.acc);
        }
        right(c, f, 452.0, (cy + 4) as f32, &sgn.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
        hline(c, y + rh, t.line);
        y += rh;
    }
    c.clear_clip();
    scrollbar(c, t, top, scroll_px, total as i32 * rh);
}

/// Artist drill-in page (`CArtist`).
pub fn artist(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    // back + ARTIST eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ARTIST", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.2));
    // name + stats
    text::draw(c, f, 22.0, 150.0, data::ARTIST_NAME, &sty(Family::Sans, Weight::ExtraBold, 31.0, t.ink, -0.01));
    text::draw(c, f, 22.0, 173.0, data::ARTIST_STATS, &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.1));
    let y = shuffle_row(c, t, f, 180, "Shuffle artist", "ALL 34 TRACKS · RANDOM ORDER");

    // ALBUMS · 3
    text::draw(c, f, 22.0, (y + 28) as f32, "ALBUMS · 3", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    let ay = y + 40;
    let cw = (W as i32 - 44 - 24) / 3; // 3 across with 12px gaps
    for (i, al) in data::ARTIST_ALBUMS.iter().enumerate() {
        let ax = 22 + i as i32 * (cw + 12);
        art::block(c, t, ax, ay, cw, cw, al.art, artdim(t));
        let nst = body_label(Family::Sans, Weight::SemiBold, 14.0, t.ink);
        let name = crate::widgets::fit(f, al.n, &nst, cw as f32);
        text::draw(c, f, ax as f32, (ay + cw + 16) as f32, &name, &nst);
        text::draw(c, f, ax as f32, (ay + cw + 32) as f32, al.y, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));
    }

    // TOP SONGS
    let mut sy = ay + cw + 48;
    text::draw(c, f, 22.0, sy as f32, "TOP SONGS", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    sy += 12;
    let rh = 54;
    for (i, sgn) in data::ARTIST_TOP.iter().enumerate() {
        let cy = sy + rh / 2;
        let now = i == 0;
        let col = if now { t.acc } else { t.faint };
        text::draw(c, f, 22.0, (cy + 4) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, col, 0.0));
        let tcol = if now { t.acc } else { t.ink };
        text::draw(c, f, 48.0, (cy - 2) as f32, sgn.t, &body_label(Family::Sans, Weight::SemiBold, 16.0, tcol));
        text::draw(c, f, 48.0, (cy + 15) as f32, sgn.al, &body_label(Family::Sans, Weight::Regular, 12.0, t.dim));
        if now {
            tiny_bars(c, 388, cy, t.acc);
        }
        right(c, f, 458.0, (cy + 4) as f32, sgn.d, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
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
            object_id: id,
            ..Default::default()
        }
    }
    fn album(name: &str, artist: &str, n_tracks: usize) -> AlbumRow {
        AlbumRow {
            name: name.into(),
            artist: artist.into(),
            year: "2020".into(),
            tracks: n_tracks as u32,
            track_list: (0..n_tracks)
                .map(|i| song(&format!("t{i}"), artist, "3:00", 100 + i as i64))
                .collect(),
            ..Default::default()
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
            thumbs: Default::default(),
            playlists: Vec::new(),
        }
    }

    #[test]
    fn song_at_follows_the_drawn_sort_order() {
        let l = lib();
        // sort 0 = TITLE: Alpha, Bravo, Charlie — rank 0 must be Alpha (id 2), NOT DB row 0.
        assert_eq!(song_at(&l, 0, 0).unwrap().object_id, 2);
        assert_eq!(song_at(&l, 0, 2).unwrap().object_id, 1);
        // sort 1 = ARTIST A-Z: Aa, Mid, Zed → Bravo (id 3) first.
        assert_eq!(song_at(&l, 1, 0).unwrap().object_id, 3);
        // sort 2 = ARTIST Z-A: Zed, Mid, Aa → Charlie (id 1) first.
        assert_eq!(song_at(&l, 2, 0).unwrap().object_id, 1);
        // sort 3 = LENGTH: 1:00, 5:00, 9:00 → Alpha (id 2) first.
        assert_eq!(song_at(&l, 3, 0).unwrap().object_id, 2);
        assert!(song_at(&l, 0, 3).is_none());
    }

    #[test]
    fn albums_grouped_hit_accounts_for_headers_and_art_split() {
        let l = lib();
        // Grouped (sort 0), nothing expanded. Content (px, 0 = list top 201):
        // header(One) 0..30, A1 30..90, A2 90..150 (same artist, no header),
        // header(Two) 150..180, B1 180..240.
        // Content-y (0 = list top): header(One) 0..HDR, A1 HDR.., A2 (same artist, no header)..,
        // header(Two).., B1.. — all derived from the shared row-height constants.
        let top = list_top(Tab::Albums);
        let (hdr, row) = (ALBUM_HDR_H, ALBUM_ROW_H);
        let a1 = hdr + row / 2; // A1 body centre
        let a2 = hdr + row + row / 2; // A2 (no second header)
        let hdr_two = hdr + 2 * row + hdr / 2; // header(Two)
        let b1 = 2 * hdr + 2 * row + row / 2; // B1
        let hit = |x, cy| albums_hit(&l, 0, None, 0, x, top + cy);
        assert_eq!(hit(200, hdr / 2), None); // artist header — not a row
        assert_eq!(hit(200, a1), Some(AlbumsHit::AlbumToggle(0))); // A1 body → toggle
        assert_eq!(hit(10, a1), Some(AlbumsHit::AlbumOpen(0))); // A1 art → drill-in
        assert_eq!(hit(200, a2), Some(AlbumsHit::AlbumToggle(1))); // A2 — no second header
        assert_eq!(hit(200, hdr_two), None); // header(Two)
        assert_eq!(hit(200, b1), Some(AlbumsHit::AlbumToggle(2))); // B1
        assert_eq!(hit(200, 3 * hdr + 3 * row), None); // below the content
        // Scrolled one album row (A1 off screen): the same screen y now lands on A2.
        assert_eq!(albums_hit(&l, 0, None, row, 200, top + a1), Some(AlbumsHit::AlbumToggle(1)));
    }

    #[test]
    fn albums_accordion_inserts_track_rows() {
        let l = lib();
        // Expand A1 (flat 0, 3 tracks). Content-y layout (from the shared constants):
        //   header(One) 0, A1 HDR, t0 HDR+ROW, t1 +CH, t2 +2CH, A2 HDR+ROW+3CH, ...
        let exp = Some(0);
        let top = list_top(Tab::Albums);
        let (hdr, row, ch) = (ALBUM_HDR_H, ALBUM_ROW_H, ALBUM_CHILD_H);
        let hit = |cy| albums_hit(&l, 0, exp, 0, 200, top + cy);
        assert_eq!(hit(hdr + row / 2), Some(AlbumsHit::AlbumToggle(0))); // A1 row
        assert_eq!(hit(hdr + row + ch / 2), Some(AlbumsHit::Track(0, 0))); // t0
        assert_eq!(hit(hdr + row + ch + ch / 2), Some(AlbumsHit::Track(0, 1))); // t1
        assert_eq!(hit(hdr + row + 2 * ch + ch / 2), Some(AlbumsHit::Track(0, 2))); // t2
        assert_eq!(hit(hdr + row + 3 * ch + row / 2), Some(AlbumsHit::AlbumToggle(1))); // A2, pushed down
        // Content height grew by 3 track rows vs collapsed.
        let collapsed = albums_build(&l, 0, None).content_h;
        let opened = albums_build(&l, 0, exp).content_h;
        assert_eq!(opened - collapsed, 3 * ALBUM_CHILD_H);
    }

    #[test]
    fn album_order_flat_sorts_drop_headers() {
        let l = lib();
        // Sort 1 (A-Z): no group headers, albums by name: A1, A2, B1 → flat order [0,1,2].
        assert_eq!(album_display_order(&l, 1), vec![0, 1, 2]);
        let layout = albums_build(&l, 1, None);
        assert!(layout.rows.iter().all(|(_, r)| !matches!(r, AlbumsRow::Group { .. })));
        // First row sits at content y 0 (no leading header) and is an album.
        assert!(matches!(layout.rows[0], (0, AlbumsRow::Album { flat: 0, .. })));
    }

    #[test]
    fn song_sorts_added_album_year() {
        // Three songs with distinct sort keys.
        let mut l = Library {
            songs: vec![
                song("Cee", "x", "3:00", 1),
                song("Aay", "x", "3:00", 2),
                song("Bee", "x", "3:00", 3),
            ],
            album_groups: Vec::new(),
            artists: Vec::new(),
            thumbs: Default::default(),
            playlists: Vec::new(),
        };
        // added: song 1 newest, 3 oldest. album order: song 2 first. year: song 3 newest.
        l.songs[0] = SongRow { added: 300, album_id: 5, disc: 1, track: 9, year: 2000, ..l.songs[0].clone() };
        l.songs[1] = SongRow { added: 200, album_id: 1, disc: 1, track: 1, year: 2010, ..l.songs[1].clone() };
        l.songs[2] = SongRow { added: 100, album_id: 5, disc: 1, track: 1, year: 2020, ..l.songs[2].clone() };
        // ARTIST Z-A (sort 2): all same artist → tie-break by title: Aay, Bee, Cee.
        assert_eq!(song_at(&l, 2, 0).unwrap().object_id, 2);
        // ADDED (sort 4): newest first → 1, 2, 3.
        assert_eq!(song_at(&l, 4, 0).unwrap().object_id, 1);
        assert_eq!(song_at(&l, 4, 2).unwrap().object_id, 3);
        // ALBUM (sort 5): (album_id, disc, track) → song2(1,1,1), song3(5,1,1), song1(5,1,9).
        assert_eq!(song_at(&l, 5, 0).unwrap().object_id, 2);
        assert_eq!(song_at(&l, 5, 1).unwrap().object_id, 3);
        assert_eq!(song_at(&l, 5, 2).unwrap().object_id, 1);
        // YEAR (sort 6): newest first → 2020(3), 2010(2), 2000(1).
        assert_eq!(song_at(&l, 6, 0).unwrap().object_id, 3);
        assert_eq!(song_at(&l, 6, 2).unwrap().object_id, 1);
    }

    #[test]
    fn fixed_tabs_only_hit_drawn_rows() {
        let l = lib();
        let top = list_top(Tab::Songs);
        let rh = row_h(Tab::Songs); // same height render draws — row 0 = top..top+rh
        assert_eq!(hit_row(Tab::Songs, &l, 0, top - 1), None); // shuffle band
        assert_eq!(hit_row(Tab::Songs, &l, 0, top + 1), Some(0));
        assert_eq!(hit_row(Tab::Songs, &l, 0, top + rh - 1), Some(0));
        assert_eq!(hit_row(Tab::Songs, &l, 0, top + rh + 1), Some(1));
        assert_eq!(hit_row(Tab::Songs, &l, 0, 700), None); // past the 3-song list
        // Pixel scroll: partially visible bottom rows ARE live (they're drawn under the clip),
        // but nothing >= LIST_BOTTOM hits.
        let m = lib_many();
        assert_eq!(hit_row(Tab::Songs, &m, 0, LIST_BOTTOM - 1), Some(((LIST_BOTTOM - 1 - top) / rh) as usize));
        assert_eq!(hit_row(Tab::Songs, &m, 0, LIST_BOTTOM), None); // >= LIST_BOTTOM
        // Scrolling by exactly one row height advances the hit by one row at the same screen y.
        assert_eq!(hit_row(Tab::Songs, &m, rh / 2, top + 1), Some(0));
        assert_eq!(hit_row(Tab::Songs, &m, rh, top + 1), Some(1));
    }

    fn lib_many() -> Library {
        Library {
            songs: (0..40).map(|i| song(&format!("s{i:02}"), "x", "3:00", i)).collect(),
            album_groups: Vec::new(),
            artists: Vec::new(),
            playlists: Vec::new(),
            thumbs: Default::default(),
        }
    }

    #[test]
    fn album_track_hit_matches_album_view_geometry() {
        let l = lib();
        let flat = l.albums_flat();
        let a = flat[2]; // B1, 4 tracks
        let t = ALBUM_TRACKS_TOP;
        let rh = ALBUM_TRACK_RH; // rows from t @rh
        assert_eq!(album_hit_track(a, 0, t - 1), None); // Play-album band / gap
        assert_eq!(album_hit_track(a, 0, t + 1), Some(0));
        assert_eq!(album_hit_track(a, 0, t + rh + 1), Some(1));
        assert_eq!(album_hit_track(a, 0, t + 3 * rh + 1), Some(3)); // row 3
        assert_eq!(album_hit_track(a, 0, t + 4 * rh + 1), None); // past the 4-track list
        assert_eq!(album_hit_track(a, 2 * rh, t + 1), Some(2)); // scrolled 2 rows
    }

    #[test]
    fn pixel_scroll_geometry_helpers() {
        let l = lib();
        let (hdr, row) = (ALBUM_HDR_H, ALBUM_ROW_H);
        // Albums content (grouped, collapsed): 2 headers + 3 rows; fits → no scroll.
        assert_eq!(content_h(Tab::Albums, &l, 0, None), 2 * hdr + 3 * row);
        assert_eq!(max_scroll_px(Tab::Albums, &l, 0, None), 0);
        // Songs: 3 rows, fits.
        assert_eq!(content_h(Tab::Songs, &l, 0, None), 3 * row_h(Tab::Songs));
        // 40 songs don't fit: max scroll = content - view, positive.
        let many = lib_many();
        let max = max_scroll_px(Tab::Songs, &many, 0, None);
        assert_eq!(max, 40 * row_h(Tab::Songs) - (LIST_BOTTOM - list_top(Tab::Songs)));
        assert!(max > 0);
        // Row-top helper (grouped albums, by album display rank): A1, A2, B1.
        assert_eq!(row_top_px(Tab::Albums, &l, 0, 0, None), hdr);
        assert_eq!(row_top_px(Tab::Albums, &l, 1, 0, None), hdr + row);
        assert_eq!(row_top_px(Tab::Albums, &l, 2, 0, None), 2 * hdr + 2 * row);
    }
}
