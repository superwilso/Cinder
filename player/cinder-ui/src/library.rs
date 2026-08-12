//! Library — ported from cinder-proto-screens2.jsx `CLibrary` + `CArtist`.
//! Tabs (Songs / Albums / Artists / Playlists), each with a scope-aware accent
//! shuffle row, then the list. Plus the drill-in Artist page.

use crate::art;
use crate::canvas::{H, W};
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
/// The ONE thumbnail edge a list row may ask [`thumb`] for, and the one the shell's art cache
/// stores (`cinder-ffi::art_cache::T48`, which static-asserts against this).
///
/// `thumb` falls back to the gradient when the cached image isn't exactly the requested size —
/// correct, but SILENT. The Artists tab asked for 44/36/40 px, so it never matched the cache and
/// drew procedural gradients forever: the covers were fetched, decoded, written to disk, loaded
/// into memory, looked up by the right id, and then thrown away one branch before being drawn.
/// It also cost 1315 us/frame against 224 for the Albums tab, because generating two gradients per
/// row is far more expensive than blitting two cached images.
pub const THUMB_PX: i32 = 48;

/// The album drill-in's cover edge — the cache's other stored size.
pub const COVER_PX: i32 = 96;

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

// ── Swipe-to-queue: the row travels with the finger ───────────────────────────────────────────
// The gesture already existed but acted only on release, so a flick either did nothing visible or
// popped a toast out of nowhere. Spotify moves the whole bar under the finger and reveals the
// action behind it, which makes the gesture self-teaching: you find it by half-doing it, and the
// revealed panel says what letting go will do BEFORE you let go.

/// A list row travelling under the finger. `y` is the SCREEN y the gesture started at — the
/// renderer finds the row by containment, so this works on every list without knowing how that
/// list is sorted, grouped or scrolled. `dx` is how far the row has moved: positive = rightward =
/// "add to queue", negative = leftward = "play next", matching what release does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SwipeRow {
    pub y: i32,
    pub dx: i32,
}

/// Travel at which release commits the queue action. Deliberately the same 60px the SHELL uses to
/// classify a horizontal swipe (`main.cpp`: `adx >= 60`) — the row lighting up and the gesture
/// firing have to be one event, or a row can look armed and then do nothing on release.
pub const SWIPE_COMMIT_PX: i32 = 60;

/// Hard limit on row travel. Past the commit point the row keeps moving so the gesture still feels
/// live, but at 40% of finger speed: it resists, which reads as "this is as far as it goes".
pub const SWIPE_MAX_PX: i32 = 150;

/// Raw finger travel → the row's rubber-banded offset.
pub fn swipe_offset(dx: i32) -> i32 {
    let a = dx.abs();
    let o = if a <= SWIPE_COMMIT_PX { a } else { SWIPE_COMMIT_PX + (a - SWIPE_COMMIT_PX) * 2 / 5 };
    o.min(SWIPE_MAX_PX) * if dx < 0 { -1 } else { 1 }
}

/// What releasing a swiped row will do — which decides the panel's icon and word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwipeIntent {
    /// A library/album/artist/playlist row: rightward queues it, leftward plays it next.
    Queue,
    /// A row of the queue itself: either direction removes it.
    Remove,
}

/// True once the row has travelled far enough that releasing will queue the track.
pub fn swipe_armed(dx: i32) -> bool {
    dx.abs() >= SWIPE_COMMIT_PX
}

/// Paint the action panel revealed behind a swiped row, then translate the canvas so the caller's
/// normal row drawing lands on top of it, offset. The caller MUST `clear_offset_x()` afterwards.
///
/// The panel spans the full row width so it shows on whichever side the row uncovered, and it goes
/// accent-coloured only once the swipe is armed. That colour change is the whole point: it is the
/// difference between "I am dragging something" and "letting go does this".
pub fn swipe_reveal(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, rh: i32, dx: i32,
                    intent: SwipeIntent) {
    let armed = swipe_armed(dx);
    let (bg, ink) = if armed { (t.acc, t.acc_ink) } else { (t.panel, t.dim) };
    fill_rect(c, 0, y, W as i32, rh, bg);
    let cy = y + rh / 2;
    let st = sty(Family::Mono, Weight::Bold, 13.0, ink, 0.12);
    // The panel's contents are CENTRED IN THE STRIP THE ROW HAS UNCOVERED, not pinned to the
    // screen edge: the row is drawn over the top of this, so anything placed past `|dx|` is simply
    // covered up — an edge-pinned label reads as "QU" for most of the gesture. The word only
    // appears once the strip is wide enough to hold it; below that the icon carries the meaning.
    let a = dx.abs();
    // On a list, the two directions are two different actions. On the QUEUE itself neither makes
    // sense — the track is already queued — so both directions mean remove, and the label says so
    // rather than leaving the direction to carry a meaning it no longer has.
    let label = match intent {
        SwipeIntent::Remove => "REMOVE",
        SwipeIntent::Queue if dx > 0 => "QUEUE",
        SwipeIntent::Queue => "PLAY NEXT",
    };
    let lw = text::measure(f, label, &st);
    let icon_w = 18.0;
    let gap = 8.0;
    let with_label = a as f32 >= icon_w + gap + lw + 28.0;
    let group = if with_label { icon_w + gap + lw } else { icon_w };
    let left = (a as f32 - group) / 2.0;
    let (ix, tx) = if dx > 0 {
        (left + icon_w / 2.0, left + icon_w + gap)
    } else {
        // Leftward: the strip is at the RIGHT edge, so mirror the group into it.
        let l = W as f32 - a as f32 + left;
        (l + icon_w / 2.0, l + icon_w + gap)
    };
    match intent {
        SwipeIntent::Remove => icons::close(c, ix, cy as f32, icon_w, ink),
        SwipeIntent::Queue if dx > 0 => icons::queue(c, ix, cy as f32, icon_w, ink),
        SwipeIntent::Queue => icons::next(c, ix, cy as f32, icon_w, ink),
    }
    if with_label {
        text::draw(c, f, tx, (cy + 5) as f32, label, &st);
    }
    // The row carries no background of its own (the list fills `t.bg` once, up front), so without
    // this the panel would read straight through the moving row instead of from beside it.
    c.set_offset_x(dx);
    fill_rect(c, 0, y, W as i32, rh, t.bg);
}

/// The swipe offset for the row occupying `y..y + rh`, if that is the row being swiped.
fn swipe_for(swipe: Option<SwipeRow>, y: i32, rh: i32) -> Option<i32> {
    swipe.filter(|s| (y..y + rh).contains(&s.y) && s.dx != 0).map(|s| s.dx)
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
    sort: usize,
    album_sort: usize,
    album_expanded: Option<usize>,
) -> Option<i32> {
    let key = az_key_for(tab, sort, album_sort)?;
    let max = max_scroll_px(tab, lib, album_sort, album_expanded);
    let top_px = match tab {
        Tab::Albums => {
            let flat = lib.albums_flat();
            let layout = albums_build(lib, album_sort, album_expanded);
            // Grouped sort indexes by ARTIST (that's the visible ordering); A-Z indexes by album
            // name. `az_key_for` already rejected the orderings that are neither.
            layout.rows.iter().find_map(|(vy, row)| match row {
                AlbumsRow::Group { flat: fi } if key == AzKey::Artist => {
                    (az_bucket(&flat[*fi].artist) == letter).then_some(*vy)
                }
                AlbumsRow::Album { flat: fi, .. } if key == AzKey::AlbumName => {
                    (az_bucket(&flat[*fi].name) == letter).then_some(*vy)
                }
                _ => None,
            })?
        }
        // VISUAL RANK, not the index in `lib.songs`. The list draws in `song_order(lib, sort)`, so
        // a position in DB order scrolls to an unrelated row on every sort but the one that
        // happens to match. This is the half of the bug you can see from the couch.
        Tab::Songs => {
            let order = song_order(lib, sort);
            let rank = order
                .iter()
                .position(|&i| az_bucket(song_az_field(&lib.songs[i], key)) == letter)?;
            rank as i32 * row_h(tab)
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
    sort: usize,
    album_sort: usize,
    album_expanded: Option<usize>,
) -> bool {
    az_scroll_for(tab, lib, letter, sort, album_sort, album_expanded).is_some()
}

/// Which field the A–Z rail indexes for a tab under its ACTIVE ordering — or `None` when that
/// ordering is not alphabetical at all, in which case the rail is hidden.
///
/// The rail used to bucket Songs by TITLE whatever the SORT chip said, so with any other sort
/// selected it was wrong twice over: it lit the wrong letters, and it jumped to a position in
/// `lib.songs` (DB order) instead of a visual rank. Under LENGTH / ADDED / ALBUM / YEAR there is no
/// letter ordering to index at all — "M" would land at whatever scroll offset the first M-titled
/// song happens to occupy — so showing the rail there is worse than not showing it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AzKey {
    Title,
    Artist,
    AlbumName,
}

pub fn az_key_for(tab: Tab, sort: usize, album_sort: usize) -> Option<AzKey> {
    match tab {
        // Indexes into SORTS. ARTIST Z-A still indexes by artist: the rail letters stay A→Z down
        // the screen, so on a descending list "A" jumps near the bottom. That reads oddly but it
        // lands on the right row, which is the part that matters.
        Tab::Songs => match sort {
            0 => Some(AzKey::Title),
            1 | 2 => Some(AzKey::Artist),
            _ => None,
        },
        // Indexes into ALBUM_SORTS: ARTIST groups by artist, A-Z is by album name, ADDED and YEAR
        // are not alphabetical.
        Tab::Albums => match album_sort {
            0 => Some(AzKey::Artist),
            1 => Some(AzKey::AlbumName),
            _ => None,
        },
        // These two have no sort chip; both lists are always built in name order.
        Tab::Artists => Some(AzKey::Artist),
        Tab::Playlists => Some(AzKey::Title),
    }
}

/// The field of a song row that `key` names. Shared by the rail's presence pass and its jump, so
/// the two cannot drift apart. `AlbumName` never reaches here (no Songs sort maps to it); it falls
/// back to the title rather than panicking if that ever changes.
fn song_az_field(r: &crate::model::SongRow, key: AzKey) -> &str {
    match key {
        AzKey::Artist => &r.artist,
        AzKey::Title | AzKey::AlbumName => &r.title,
    }
}

/// Index of a bucket letter in [`AZ_LETTERS`] ('#' first, then A–Z).
fn az_index(b: u8) -> Option<usize> {
    match b {
        b'#' => Some(0),
        b'A'..=b'Z' => Some((b - b'A') as usize + 1),
        _ => None,
    }
}

/// Which of the 27 rail letters have rows, in ONE pass over the library.
///
/// The rail used to answer this per letter, by asking `az_scroll_for` 27 times — so drawing it
/// scanned the whole library 27 times EVERY FRAME. On the real library (3349 songs) that measured
/// 1529 us/frame on the host, more than rendering the list it decorates, and on the Albums tab it
/// also rebuilt the accordion layout 27 times per frame. It is a per-frame cost on every Library
/// screen, which is most of what "the list feels heavy" was.
///
/// Must agree with `az_scroll_for` letter for letter — a bright letter that doesn't jump, or a
/// faint one that does, is worse than either. `az_rail_agrees_with_the_jump` pins that.
pub fn az_present(tab: Tab, lib: &Library, sort: usize, album_sort: usize) -> [bool; 27] {
    let mut out = [false; 27];
    // No alphabetical ordering under this sort chip -> no rail. Every letter reads as absent, so
    // `az_render` draws nothing and `az_hit_x` stops claiming taps (see `az_key_for`).
    let Some(key) = az_key_for(tab, sort, album_sort) else { return out };
    let mut mark = |s: &str| {
        if let Some(i) = az_index(az_bucket(s)) {
            out[i] = true;
        }
    };
    match tab {
        Tab::Songs => lib.songs.iter().for_each(|r| mark(song_az_field(r, key))),
        Tab::Artists => lib.artists.iter().for_each(|r| mark(&r.name)),
        Tab::Playlists => lib.playlists.iter().for_each(|r| mark(&r.name)),
        // ARTIST groups by artist — that's the visible ordering, and the only rows the jump can
        // land on are the group headers. A-Z is by album name.
        Tab::Albums if key == AzKey::Artist => {
            lib.album_groups.iter().for_each(|g| mark(&g.artist))
        }
        Tab::Albums => lib
            .album_groups
            .iter()
            .flat_map(|g| g.albums.iter())
            .for_each(|a| mark(&a.name)),
    }
    out
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
/// Bottom of every scrolling list on the library screens.
pub fn list_bottom() -> i32 {
    LIST_BOTTOM
}

/// Right-edge strip in which a vertical drag grabs the scrollbar, as the Sony UI does.
///
/// It is the SAME strip as the A–Z rail, and the two coexist by gesture rather than by geometry: a
/// TAP there is a letter jump, a DRAG is a scrollbar drag. That falls straight out of the shell's
/// existing tap-vs-drag classification and costs no screen width — and on the sorts where the rail
/// is hidden ([`az_key_for`] returns `None`) the strip does nothing else anyway. The drawn bar
/// stays 3 px; only the target is wide, because a 3 px target is not a target.
pub const SBAR_GRAB_W: i32 = AZ_W;

pub fn sbar_hit_x(x: i32) -> bool {
    x >= W as i32 - SBAR_GRAB_W
}

/// Thumb height for a `track_h`-tall bar showing `content_h` of content. Floored at 18 px so a
/// very long list still has something grabbable.
pub fn sbar_thumb_h(track_h: i32, content_h: i32) -> i32 {
    if content_h <= 0 {
        return track_h;
    }
    ((track_h as f32 / content_h as f32) * track_h as f32).max(18.0) as i32
}

/// How far the thumb can travel — the denominator that converts finger px into content px.
pub fn sbar_span(top: i32, content_h: i32) -> i32 {
    let track_h = LIST_BOTTOM - top;
    (track_h - sbar_thumb_h(track_h, content_h)).max(0)
}

pub(crate) fn scrollbar(c: &mut Canvas, t: &Theme, top: i32, scroll_px: i32, content_h: i32,
                        active: bool) {
    let track_h = LIST_BOTTOM - top;
    if track_h <= 0 || content_h <= track_h {
        return;
    }
    let thumb_h = sbar_thumb_h(track_h, content_h);
    let max_off = (content_h - track_h) as f32;
    let pos = if max_off > 0.0 { (scroll_px as f32 / max_off).clamp(0.0, 1.0) } else { 0.0 };
    let thumb_y = top + ((track_h - thumb_h) as f32 * pos) as i32;
    // faint full-height track + a brighter thumb so position is readable at a glance. While a
    // finger is on it the thumb goes wide and accent-coloured: the grab zone is much wider than
    // the drawn bar, so without that there is no way to tell the drag was picked up.
    let (w, col) = if active { (7, t.acc) } else { (3, t.faint) };
    fill_rect(c, W as i32 - 4, top, 3, track_h, t.line);
    fill_rect(c, W as i32 - 1 - w, thumb_y, w, thumb_h, col);
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
/// The Artists-tab cover stack: up to two album covers, back one offset behind the front.
///
/// Takes the LIBRARY so it can draw the real decoded covers. It used to take only the gradient
/// seeds, which is why Artists looked wrong beside Albums — the covers were sitting in
/// `lib.thumbs` the whole time with no id here to fetch them by. `thumb` falls back to the
/// gradient per-cover, so a half-built art cache degrades one square at a time rather than
/// dropping the whole stack.
/// How far the back cover of the stack peeks out from behind the front one.
const ART_STACK_OFFSET: i32 = 18;

/// Total width of the Artists-tab cover stack, so the row's text can be placed off it rather than
/// at a literal that stops clearing it the moment the squares change size.
pub const ART_STACK_W: i32 = THUMB_PX + ART_STACK_OFFSET;

/// "1 album" / "2 albums" — a count and a noun that disagree reads as a bug in everything near it.
fn plural(n: u32, noun: &str) -> String {
    if n == 1 { format!("{n} {noun}") } else { format!("{n} {noun}s") }
}

fn art_stack(c: &mut Canvas, t: &Theme, lib: &Library, x: i32, cy: i32, arts: &[&str], ids: &[i64]) {
    let op = artdim(t);
    let id_at = |i: usize| ids.get(i).copied().unwrap_or(i64::MIN);
    // Both squares at THUMB_PX — see the constant. The stack reads as two covers through the
    // OFFSET and the dimming of the one behind, not through drawing them at different sizes,
    // because any size but this one silently loses the real artwork.
    let s = THUMB_PX;
    let top = cy - s / 2;
    if arts.len() == 1 {
        thumb(c, t, lib, id_at(0), arts[0], x, top, s, op);
    } else {
        thumb(c, t, lib, id_at(1), arts[1], x + ART_STACK_OFFSET, top - 4, s, 0.55 * op);
        thumb(c, t, lib, id_at(0), arts[0], x, top + 4, s, op);
    }
}

/// Tab bar; returns the y where the shuffle row begins.
/// Y band of the tab strip (screen coords) — the header ends at 91 and the strip is 35px tall.
pub const TAB_TOP: i32 = 91;
pub const TAB_BOT: i32 = 126;

/// Where each tab label is DRAWN: (tab, x, width) in screen coords. Both `tabs()` (render) and
/// `tab_at()` (hit test) read this, so the two can't disagree.
///
/// They used to. `tabs()` laid the labels out from their MEASURED widths starting at x=22, while
/// the navigator hit-tested against hardcoded thresholds (x<120 → Songs, <220 → Albums, <330 →
/// Artists, else Playlists). At the default size "ALBUMS" is drawn at x≈94..154, so tapping its
/// left half selected SONGS; "ARTISTS" (≈176..247) and "PLAYLISTS" (≈269..360) were off in the
/// same direction. Deriving both from one function fixes that — and keeps it fixed at any UI
/// text scale, where fixed thresholds would drift much further.
pub fn tab_layout(f: &FontSet) -> Vec<(Tab, f32, f32)> {
    let mut out = Vec::with_capacity(TABS.len());
    let mut x = 22.0;
    for (tab, label) in TABS {
        let st = sty(Family::Mono, Weight::Regular, 14.0, Rgb888::new(0, 0, 0), 0.12);
        let w = text::measure(f, label, &st);
        out.push((tab, x, w));
        x += w + 22.0;
    }
    out
}

/// Which tab is at screen-x `x` in the tab strip? Splits the inter-label gaps down the middle so
/// every pixel of the strip belongs to its nearest label (no dead zones between tabs).
pub fn tab_at(f: &FontSet, x: i32) -> Option<Tab> {
    let zones = tab_layout(f);
    let x = x as f32;
    for (i, &(tab, tx, tw)) in zones.iter().enumerate() {
        let left = if i == 0 { 0.0 } else { let (_, px, pw) = zones[i - 1]; (px + pw + tx) / 2.0 };
        let right = match zones.get(i + 1) {
            Some(&(_, nx, _)) => (tx + tw + nx) / 2.0,
            None => W as f32,
        };
        if x >= left && x < right {
            return Some(tab);
        }
    }
    None
}

fn tabs(c: &mut Canvas, t: &Theme, f: &FontSet, y0: i32, active: Tab) -> i32 {
    for (tab, x, w) in tab_layout(f) {
        let on = tab == active;
        let st = sty(Family::Mono, Weight::Regular, 14.0, if on { t.acc } else { t.faint }, 0.12);
        text::draw(c, f, x, (y0 + 20) as f32, TABS.iter().find(|(tt, _)| *tt == tab).map(|(_, l)| *l).unwrap_or(""), &st);
        if on {
            fill_rect(c, x as i32, y0 + 32, w as i32, 2, t.acc);
        }
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
    // Fit inside the accent band (64 → the play glyph at 428), or the caption spills past it.
    let lst = sty(Family::Sans, Weight::Bold, 18.0, t.acc_ink, 0.0);
    let sst = sty(Family::Mono, Weight::Regular, 12.0, t.acc_ink, 0.06);
    text::draw(c, f, 64.0, (cy - 4) as f32, &crate::widgets::fit(f, label, &lst, 364.0), &lst);
    text::draw(c, f, 64.0, (cy + 14) as f32, &crate::widgets::fit(f, sub, &sst, 364.0), &sst);
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
    sort: usize,
    album_sort: usize,
) {
    // Nothing to index under this ordering (LENGTH / ADDED / ALBUM / YEAR) — draw no rail at all
    // rather than 27 letters that jump to arbitrary scroll offsets.
    if az_key_for(tab, sort, album_sort).is_none() {
        return;
    }
    let top = list_top(tab);
    let h = LIST_BOTTOM - top;
    let n = AZ_LETTERS.len() as i32;
    let x = W as i32 - AZ_W / 2;
    let present = az_present(tab, lib, sort, album_sort);
    for (i, &ch) in AZ_LETTERS.iter().enumerate() {
        let cy = top + (i as i32 * h) / n + h / (2 * n);
        let col = if present[i] { t.dim } else { t.faint };
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
    swipe: Option<SwipeRow>,
    sbar_active: bool,
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
                let sw = swipe_for(swipe, y, rh);
                if let Some(dx) = sw {
                    swipe_reveal(c, t, f, y, rh, dx, SwipeIntent::Queue);
                }
                if now {
                    fill_rect(c, 0, y, W as i32, rh, t.row_sel);
                }
                thumb(c, t, lib, sgn.album_id, &sgn.art, 22, y + (rh - THUMB_PX) / 2, THUMB_PX, artdim(t));
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
                if sw.is_some() {
                    c.clear_offset_x();
                }
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh, sbar_active);
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
                        let gst = sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.16);
                        text::draw(c, f, 22.0, (y + 20) as f32,
                            &crate::widgets::fit(f, &label, &gst, 436.0), &gst);
                    }
                    AlbumsRow::Album { flat: fi, expanded } => {
                        let al = flat[fi];
                        let now = rank == current;
                        rank += 1;
                        let cy = y + ALBUM_ROW_H / 2;
                        let sw = swipe_for(swipe, y, ALBUM_ROW_H);
                        if let Some(dx) = sw {
                            swipe_reveal(c, t, f, y, ALBUM_ROW_H, dx, SwipeIntent::Queue);
                        }
                        if now {
                            fill_rect(c, 0, y, W as i32, ALBUM_ROW_H, t.row_sel);
                        }
                        thumb(c, t, lib, al.album_id, &al.art, 22, y + (ALBUM_ROW_H - THUMB_PX) / 2, THUMB_PX, artdim(t));
                        let tcol = if now { t.acc } else { t.ink };
                        // Truncate against the space actually available (art at 80 → caret at 444).
                        // These two were drawn untruncated, so a long album name simply ran off the
                        // right edge of the panel.
                        let tst = body_label(Family::Sans, Weight::SemiBold, 20.0, tcol);
                        text::draw(c, f, 80.0, (cy - 2) as f32,
                            &crate::widgets::fit(f, &al.name, &tst, 356.0), &tst);
                        let sub = if al.year.is_empty() {
                            format!("{} tracks", al.tracks)
                        } else {
                            format!("{} · {} tracks", al.year, al.tracks)
                        };
                        let sst = body_label(Family::Sans, Weight::Regular, 15.0, t.dim);
                        text::draw(c, f, 80.0, (cy + 16) as f32,
                            &crate::widgets::fit(f, &sub, &sst, 356.0), &sst);
                        // Accordion caret (right): points down when open, right when closed.
                        let caret = if expanded { t.acc } else { t.faint };
                        icons::chevron(c, 452.0, cy as f32, 13.0, caret);
                        hline(c, y + ALBUM_ROW_H, t.line);
                        if sw.is_some() {
                            c.clear_offset_x();
                        }
                    }
                    AlbumsRow::Track { flat: fi, track } => {
                        let al = flat[fi];
                        if let Some(sgn) = al.track_list.get(track) {
                            let cy = y + ALBUM_CHILD_H / 2;
                            let sw = swipe_for(swipe, y, ALBUM_CHILD_H);
                            if let Some(dx) = sw {
                                swipe_reveal(c, t, f, y, ALBUM_CHILD_H, dx, SwipeIntent::Queue);
                            }
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
                            if sw.is_some() {
                                c.clear_offset_x();
                            }
                        }
                    }
                }
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, layout.content_h, sbar_active);
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
                art_stack(c, t, lib, 22, cy, &arts, &ar.album_ids);
                let tcol = if now { t.acc } else { t.ink };
                // Text clears the cover stack: it is 22 + STACK_OFFSET + THUMB_PX wide.
                let tx = (22 + ART_STACK_W + 10) as f32;
                let tst = body_label(Family::Sans, Weight::SemiBold, 20.0, tcol);
                text::draw(c, f, tx, (cy - 2) as f32,
                    &crate::widgets::fit(f, &ar.name, &tst, 402.0 - tx), &tst);
                let sub = format!("{} · {} tracks", plural(ar.albums, "album"), ar.tracks);
                text::draw(c, f, tx, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 15.0, t.dim));
                stroke_rect(c, 414, cy - 20, 40, 40, t.line, 1);
                icons::shuffle(c, 434.0, cy as f32, 15.0, t.dim);
                hline(c, y + rh, t.line);
                y += rh;
            }
            c.clear_clip();
            scrollbar(c, t, top, scroll_px, total as i32 * rh, sbar_active);
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
            scrollbar(c, t, top, scroll_px, total as i32 * rh, sbar_active);
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
    swipe: Option<SwipeRow>,
    sbar_active: bool,
) {
    let scroll_px = scroll_px.clamp(0, album_max_scroll_px(album));
    c.fill(t.bg);
    // back chevron + ALBUM eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ALBUM", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.2));
    // art block + title/artist/meta
    match cover {
        Some(img) if img.w == COVER_PX as usize && img.h == COVER_PX as usize =>
            art::draw_image(c, t, 22, 130, img, artdim(t)),
        _ => art::block(c, t, 22, 130, COVER_PX, COVER_PX, &album.art, artdim(t)),
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
        let sw = swipe_for(swipe, y, rh);
        if let Some(dx) = sw {
            swipe_reveal(c, t, f, y, rh, dx, SwipeIntent::Queue);
        }
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
        if sw.is_some() {
            c.clear_offset_x();
        }
        y += rh;
    }
    c.clear_clip();
    scrollbar(c, t, top, scroll_px, total as i32 * rh, sbar_active);
}

// ── Artist drill-in ───────────────────────────────────────────────────────────────────────────
// Every artist gets a real page: their albums (with the same decoded covers the Albums tab draws)
// then every one of their tracks, over one scroll. This used to be a static mock wired to
// `data::ARTIST_*` — three hard-coded albums and five hard-coded songs, the same ones whichever
// artist you were looking at — and nothing pushed it, so it was only ever reachable from the host
// preview.

/// The "Shuffle artist" band sits under the name/stats block.
pub const ARTIST_BAND_Y: i32 = 182;

/// Top of the artist page's scrolling content — derived from the band, like `list_top`.
pub fn artist_content_top() -> i32 {
    let (_, by, _, bh) = shuffle_band_rect(ARTIST_BAND_Y);
    by + bh + 8
}

/// True if `(x, y)` is inside the artist page's "Shuffle artist" band.
pub fn hit_artist_shuffle_band(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = shuffle_band_rect(ARTIST_BAND_Y);
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

pub const ARTIST_SEC_H: i32 = 36; // "ALBUMS · n" / "SONGS · n" section header
pub const ARTIST_ALBUM_RH: i32 = 68;
pub const ARTIST_TRACK_RH: i32 = 62;

/// One row of the artist page's scrolling content.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ArtistRowKind {
    AlbumsSection,
    /// Index into [`ArtistPage::albums`].
    Album(usize),
    SongsSection,
    /// Index into [`ArtistPage::tracks`].
    Song(usize),
}

impl ArtistRowKind {
    fn h(self) -> i32 {
        match self {
            ArtistRowKind::AlbumsSection | ArtistRowKind::SongsSection => ARTIST_SEC_H,
            ArtistRowKind::Album(_) => ARTIST_ALBUM_RH,
            ArtistRowKind::Song(_) => ARTIST_TRACK_RH,
        }
    }
}

/// Everything one artist's page shows, resolved from the library once when the page opens.
/// Borrows the library — this is a view, not a copy: an artist with 300 tracks would otherwise
/// clone 300 rows on every frame.
/// One track on an artist page, with the album it came from. The album name is carried separately
/// rather than read off `SongRow::art`: that field is an ART SEED, which equals the album name on
/// device but is a gradient key in the sample data — a subtitle that is right only on hardware is
/// a subtitle nobody can check.
pub struct ArtistTrack<'a> {
    pub song: &'a crate::model::SongRow,
    pub album: &'a str,
}

pub struct ArtistPage<'a> {
    pub name: &'a str,
    /// `(index into `Library::albums_flat()`, the album)` — the flat index is what the Album
    /// drill-in screen takes, so tapping an album here opens the same page the Albums tab does.
    pub albums: Vec<(usize, &'a crate::model::AlbumRow)>,
    pub tracks: Vec<ArtistTrack<'a>>,
    /// `(content-space y, row)` in draw order. ONE list, shared by the renderer and the hit test.
    pub rows: Vec<(i32, ArtistRowKind)>,
    pub content_h: i32,
}

/// Resolve an artist's page out of the library, by name.
///
/// Albums come from `album_groups`, which cinder-ffi groups by ALBUM ARTIST — the same key the
/// Artists tab is built from, so the two always agree. Tracks are the albums' track lists in album
/// order; if the artist has no albums at all (tracks with no album row behind them), it falls back
/// to matching the Songs list on artist name, so a page is never empty when the tab said it has
/// tracks.
pub fn artist_page<'a>(lib: &'a Library, name: &'a str) -> ArtistPage<'a> {
    let mut albums: Vec<(usize, &crate::model::AlbumRow)> = Vec::new();
    let mut flat = 0usize;
    for g in &lib.album_groups {
        for al in &g.albums {
            if g.artist == name {
                albums.push((flat, al));
            }
            flat += 1;
        }
    }
    let mut tracks: Vec<ArtistTrack> = albums
        .iter()
        .flat_map(|(_, al)| al.track_list.iter().map(|s| ArtistTrack { song: s, album: &al.name }))
        .collect();
    if tracks.is_empty() {
        tracks = lib
            .songs
            .iter()
            .filter(|s| s.artist == name)
            .map(|s| ArtistTrack { song: s, album: "" })
            .collect();
    }

    let mut rows: Vec<(i32, ArtistRowKind)> = Vec::new();
    let mut y = 0;
    if !albums.is_empty() {
        rows.push((y, ArtistRowKind::AlbumsSection));
        y += ARTIST_SEC_H;
        for i in 0..albums.len() {
            rows.push((y, ArtistRowKind::Album(i)));
            y += ARTIST_ALBUM_RH;
        }
    }
    if !tracks.is_empty() {
        rows.push((y, ArtistRowKind::SongsSection));
        y += ARTIST_SEC_H;
        for i in 0..tracks.len() {
            rows.push((y, ArtistRowKind::Song(i)));
            y += ARTIST_TRACK_RH;
        }
    }
    ArtistPage { name, albums, tracks, rows, content_h: y + 8 }
}

/// Visible height of the artist page's scrolling content.
pub fn artist_view_h() -> i32 {
    LIST_BOTTOM - artist_content_top()
}

/// Largest useful scroll offset for an artist page.
pub fn artist_max_scroll_px(page: &ArtistPage) -> i32 {
    (page.content_h - artist_view_h()).max(0)
}

/// What sits under a tap on the artist page's scrolling content.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ArtistHit {
    /// Open this album's drill-in (index into `Library::albums_flat()`).
    Album(usize),
    /// Play this track (index into [`ArtistPage::tracks`]).
    Track(usize),
}

/// Which artist-page row is under touch-`y`? Reads the SAME `page.rows` the renderer draws, so a
/// tap cannot land on a row other than the one under the finger.
pub fn artist_hit(page: &ArtistPage, scroll_px: i32, y: i32) -> Option<ArtistHit> {
    let top = artist_content_top();
    if y < top || y >= LIST_BOTTOM {
        return None;
    }
    let cy = y - top + scroll_px.max(0);
    page.rows.iter().find(|(vy, r)| (*vy..*vy + r.h()).contains(&cy)).and_then(|(_, r)| match *r {
        ArtistRowKind::Album(i) => page.albums.get(i).map(|(flat, _)| ArtistHit::Album(*flat)),
        ArtistRowKind::Song(i) => Some(ArtistHit::Track(i)),
        _ => None,
    })
}

/// Artist drill-in page: fixed header (name, stats, shuffle band), then a scrolling list of the
/// artist's albums and all of their tracks.
///
/// The albums are LIST ROWS with 48px covers rather than a poster grid on purpose: the on-device
/// art cache holds one pre-scaled size (48px, because decoding a 1425x1425 embedded JPEG costs
/// 365 ms), so a grid of big tiles would be a grid of gradient fallbacks — exactly the thing that
/// made the Artists tab look wrong in the first place.
#[allow(clippy::too_many_arguments)]
pub fn artist_view(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    page: &ArtistPage,
    scroll_px: i32,
    sel: usize,
    swipe: Option<SwipeRow>,
    sbar_active: bool,
) {
    let scroll_px = scroll_px.clamp(0, artist_max_scroll_px(page));
    c.fill(t.bg);
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ARTIST", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.2));

    let nst = sty(Family::Sans, Weight::ExtraBold, 28.0, t.ink, -0.01);
    let name = crate::widgets::fit(f, page.name, &nst, W as f32 - 44.0);
    text::draw(c, f, 22.0, 152.0, &name, &nst);
    let stats = format!("{} ALBUMS · {} TRACKS", page.albums.len(), page.tracks.len());
    text::draw(c, f, 22.0, 174.0, &stats, &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.1));
    shuffle_row(c, t, f, ARTIST_BAND_Y, "Shuffle artist",
        &format!("ALL {} TRACKS · RANDOM ORDER", page.tracks.len()));

    let top = artist_content_top();
    c.set_clip_y(top, LIST_BOTTOM);
    for (vy, row) in &page.rows {
        let y = top + *vy - scroll_px;
        let h = row.h();
        if y + h <= top {
            continue;
        }
        if y >= LIST_BOTTOM {
            break;
        }
        match *row {
            ArtistRowKind::AlbumsSection => {
                text::draw(c, f, 22.0, (y + 24) as f32, &format!("ALBUMS · {}", page.albums.len()),
                    &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
            }
            ArtistRowKind::SongsSection => {
                text::draw(c, f, 22.0, (y + 24) as f32, &format!("SONGS · {}", page.tracks.len()),
                    &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
            }
            ArtistRowKind::Album(i) => {
                let Some((_, al)) = page.albums.get(i) else { continue };
                let cy = y + ARTIST_ALBUM_RH / 2;
                let sw = swipe_for(swipe, y, ARTIST_ALBUM_RH);
                if let Some(dx) = sw {
                    swipe_reveal(c, t, f, y, ARTIST_ALBUM_RH, dx, SwipeIntent::Queue);
                }
                thumb(c, t, lib, al.album_id, &al.art, 22, y + (ARTIST_ALBUM_RH - THUMB_PX) / 2, THUMB_PX, artdim(t));
                let tst = body_label(Family::Sans, Weight::SemiBold, 19.0, t.ink);
                text::draw(c, f, 80.0, (cy - 2) as f32,
                    &crate::widgets::fit(f, &al.name, &tst, 340.0), &tst);
                let sub = if al.year.is_empty() {
                    format!("{} tracks", al.tracks)
                } else {
                    format!("{} · {} tracks", al.year, al.tracks)
                };
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub,
                    &body_label(Family::Sans, Weight::Regular, 15.0, t.dim));
                icons::chevron(c, 452.0, cy as f32, 13.0, t.faint);
                hline(c, y + ARTIST_ALBUM_RH, t.line);
                if sw.is_some() {
                    c.clear_offset_x();
                }
            }
            ArtistRowKind::Song(i) => {
                let Some(tr) = page.tracks.get(i) else { continue };
                let sgn = tr.song;
                let now = i == sel;
                let cy = y + ARTIST_TRACK_RH / 2;
                let sw = swipe_for(swipe, y, ARTIST_TRACK_RH);
                if let Some(dx) = sw {
                    swipe_reveal(c, t, f, y, ARTIST_TRACK_RH, dx, SwipeIntent::Queue);
                }
                if now {
                    fill_rect(c, 0, y, W as i32, ARTIST_TRACK_RH, t.row_sel);
                }
                text::draw(c, f, 26.0, (cy + 4) as f32, &format!("{}", i + 1),
                    &sty(Family::Mono, Weight::Regular, 12.0, if now { t.acc } else { t.faint }, 0.0));
                let tcol = if now { t.acc } else { t.ink };
                let tst = body_label(Family::Sans, Weight::SemiBold, 18.0, tcol);
                text::draw(c, f, 58.0, (cy - 2) as f32,
                    &crate::widgets::fit(f, &sgn.title, &tst, 300.0), &tst);
                let ast = body_label(Family::Sans, Weight::Regular, 14.0, t.dim);
                text::draw(c, f, 58.0, (cy + 16) as f32,
                    &crate::widgets::fit(f, tr.album, &ast, 300.0), &ast);
                right(c, f, 452.0, (cy + 4) as f32, &sgn.dur,
                    &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.0));
                hline(c, y + ARTIST_TRACK_RH, t.line);
                if sw.is_some() {
                    c.clear_offset_x();
                }
            }
        }
    }
    c.clear_clip();
    scrollbar(c, t, top, scroll_px, page.content_h, sbar_active);
}

// ── Playlist drill-in ────────────────────────────────────────────────────────────────────────
// The same shape as the artist page, minus the albums section: a playlist is one ordered list of
// tracks, so there is nothing to group. It reads `PlaylistRow::track_list`, resolved once at
// library build, so no frame here touches the DB.

/// The "Shuffle playlist" band. Sits higher than the artist page's because there is no
/// albums/tracks stat pair above it, just the one count.
pub const PLAYLIST_BAND_Y: i32 = 182;
pub const PLAYLIST_TRACK_RH: i32 = ARTIST_TRACK_RH;

pub fn playlist_content_top() -> i32 {
    let (_, by, _, bh) = shuffle_band_rect(PLAYLIST_BAND_Y);
    by + bh + 8
}

pub fn hit_playlist_shuffle_band(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = shuffle_band_rect(PLAYLIST_BAND_Y);
    (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
}

pub fn playlist_view_h() -> i32 {
    LIST_BOTTOM - playlist_content_top()
}

pub fn playlist_content_h(pl: &crate::model::PlaylistRow) -> i32 {
    pl.track_list.len() as i32 * PLAYLIST_TRACK_RH + 8
}

pub fn playlist_max_scroll_px(pl: &crate::model::PlaylistRow) -> i32 {
    (playlist_content_h(pl) - playlist_view_h()).max(0)
}

/// Which track is under touch-`y`. Mirrors the renderer's geometry exactly.
pub fn playlist_hit_track(pl: &crate::model::PlaylistRow, scroll_px: i32, y: i32) -> Option<usize> {
    let top = playlist_content_top();
    if y < top || y >= LIST_BOTTOM {
        return None;
    }
    let i = ((y - top + scroll_px.max(0)) / PLAYLIST_TRACK_RH) as usize;
    (i < pl.track_list.len()).then_some(i)
}

/// Playlist drill-in: fixed header (name, count, shuffle band) then the members in saved order.
#[allow(clippy::too_many_arguments)]
pub fn playlist_view(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    pl: &crate::model::PlaylistRow,
    scroll_px: i32,
    sel: usize,
    swipe: Option<SwipeRow>,
    sbar_active: bool,
) {
    let scroll_px = scroll_px.clamp(0, playlist_max_scroll_px(pl));
    c.fill(t.bg);
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "PLAYLIST", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.2));

    let nst = sty(Family::Sans, Weight::ExtraBold, 28.0, t.ink, -0.01);
    let name = crate::widgets::fit(f, &pl.name, &nst, W as f32 - 44.0);
    text::draw(c, f, 22.0, 152.0, &name, &nst);
    // The DB's own count, not the resolved length: a member whose file is gone still counts in
    // Sony's container, and silently showing a smaller number would hide that.
    let stats = if pl.track_list.len() as u32 == pl.tracks {
        plural(pl.tracks, "TRACK").to_uppercase()
    } else {
        format!("{} OF {} TRACKS AVAILABLE", pl.track_list.len(), pl.tracks)
    };
    text::draw(c, f, 22.0, 174.0, &stats, &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.1));
    shuffle_row(c, t, f, PLAYLIST_BAND_Y, "Shuffle playlist",
        &format!("ALL {} TRACKS · RANDOM ORDER", pl.track_list.len()));

    let top = playlist_content_top();
    c.set_clip_y(top, LIST_BOTTOM);
    if pl.track_list.is_empty() {
        let st = sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0);
        text::draw(c, f, 22.0, (top + 40) as f32, "Nothing in this playlist.",
            &sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0));
        text::draw(c, f, 22.0, (top + 66) as f32, "Its tracks are missing from the library.", &st);
        c.clear_clip();
        return;
    }
    let first = (scroll_px / PLAYLIST_TRACK_RH) as usize;
    let mut y = top - (scroll_px % PLAYLIST_TRACK_RH);
    for (i, sgn) in pl.track_list.iter().enumerate().skip(first) {
        if y >= LIST_BOTTOM {
            break;
        }
        let now = i == sel;
        let cy = y + PLAYLIST_TRACK_RH / 2;
        let sw = swipe_for(swipe, y, PLAYLIST_TRACK_RH);
        if let Some(dx) = sw {
            swipe_reveal(c, t, f, y, PLAYLIST_TRACK_RH, dx, SwipeIntent::Queue);
        }
        if now {
            fill_rect(c, 0, y, W as i32, PLAYLIST_TRACK_RH, t.row_sel);
        }
        text::draw(c, f, 26.0, (cy + 4) as f32, &format!("{}", i + 1),
            &sty(Family::Mono, Weight::Regular, 12.0, if now { t.acc } else { t.faint }, 0.0));
        thumb(c, t, lib, sgn.album_id, &sgn.art, 52, y + (PLAYLIST_TRACK_RH - THUMB_PX) / 2,
              THUMB_PX, artdim(t));
        let tcol = if now { t.acc } else { t.ink };
        let tst = body_label(Family::Sans, Weight::SemiBold, 18.0, tcol);
        text::draw(c, f, 110.0, (cy - 2) as f32,
            &crate::widgets::fit(f, &sgn.title, &tst, 268.0), &tst);
        let ast = body_label(Family::Sans, Weight::Regular, 14.0, t.dim);
        text::draw(c, f, 110.0, (cy + 16) as f32,
            &crate::widgets::fit(f, &sgn.artist, &ast, 268.0), &ast);
        right(c, f, 452.0, (cy + 4) as f32, &sgn.dur,
            &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.0));
        hline(c, y + PLAYLIST_TRACK_RH, t.line);
        if sw.is_some() {
            c.clear_offset_x();
        }
        y += PLAYLIST_TRACK_RH;
    }
    c.clear_clip();
    scrollbar(c, t, top, scroll_px, playlist_content_h(pl), sbar_active);
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

    /// The rail's brightness and the jump must never disagree: a bright letter that goes nowhere,
    /// or a faint one that does jump, is worse than either being wrong on its own. `az_present`
    /// (one pass, drawn) and `az_scroll_for` (per letter, tapped) are separate code, so pin them.
    #[test]
    fn az_rail_agrees_with_the_jump() {
        let l = lib();
        for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
            for sort in 0..SORTS.len() {
                for album_sort in 0..ALBUM_SORTS.len() {
                    let present = az_present(tab, &l, sort, album_sort);
                    for (i, &ch) in AZ_LETTERS.iter().enumerate() {
                        assert_eq!(
                            present[i],
                            az_scroll_for(tab, &l, ch, sort, album_sort, None).is_some(),
                            "{tab:?} sort={sort} album_sort={album_sort} letter {:?}", ch as char
                        );
                    }
                }
            }
        }
    }

    /// The rail jumps to a VISUAL RANK. It used to jump to a position in `lib.songs` (DB order),
    /// so with any sort but TITLE the tap scrolled to an unrelated row — and it bucketed by title
    /// even when the list was ordered by artist, so it lit the wrong letters too. Walk every
    /// alphabetical sort and check the row the jump actually lands on.
    #[test]
    fn az_jump_uses_the_active_sorts_key_and_visual_rank() {
        let l = lib();
        for sort in 0..SORTS.len() {
            let Some(key) = az_key_for(Tab::Songs, sort, 0) else { continue };
            let order = song_order(&l, sort);
            for &ch in AZ_LETTERS {
                let Some(px) = az_scroll_for(Tab::Songs, &l, ch, sort, 0, None) else { continue };
                // Un-clamped rank: a jump near the end of the list clamps to max_scroll, so only
                // check the row when the offset still maps back exactly.
                if px % row_h(Tab::Songs) != 0 {
                    continue;
                }
                let rank = (px / row_h(Tab::Songs)) as usize;
                let hit = &l.songs[order[rank]];
                let landed = az_bucket(song_az_field(hit, key)) == ch;
                let clamped = px == max_scroll_px(Tab::Songs, &l, 0, None);
                assert!(landed || clamped, "SORT={} letter {:?} landed on {:?}",
                        SORTS[sort], ch as char, hit.title);
            }
        }
    }

    /// LENGTH / ADDED / ALBUM / YEAR have no letter ordering, so the rail is hidden rather than
    /// shown pointing at arbitrary scroll offsets — and with it hidden, nothing may claim a jump.
    #[test]
    fn az_rail_is_absent_for_non_alphabetical_sorts() {
        let l = lib();
        for (sort, name) in SORTS.iter().enumerate() {
            let alphabetical = matches!(*name, "TITLE" | "ARTIST A-Z" | "ARTIST Z-A");
            assert_eq!(az_key_for(Tab::Songs, sort, 0).is_some(), alphabetical, "SORT {name}");
            if !alphabetical {
                assert_eq!(az_present(Tab::Songs, &l, sort, 0), [false; 27], "SORT {name}");
                assert!(AZ_LETTERS.iter()
                    .all(|&ch| az_scroll_for(Tab::Songs, &l, ch, sort, 0, None).is_none()));
            }
        }
        for (album_sort, name) in ALBUM_SORTS.iter().enumerate() {
            let alphabetical = matches!(*name, "ARTIST" | "A-Z");
            assert_eq!(
                az_key_for(Tab::Albums, 0, album_sort).is_some(), alphabetical, "ORDER {name}"
            );
        }
    }

    /// The row must ARM at exactly the travel the shell commits at. If the two ever drift, a row
    /// goes accent-coloured — promising "letting go queues this" — and then release does nothing,
    /// or the reverse: it queues silently with the row still looking un-armed.
    #[test]
    fn swipe_arms_at_the_same_travel_the_shell_commits_at() {
        // cinder-home/src/main.cpp classifies a horizontal swipe at `adx >= 60` on RAW travel, and
        // `swipe_offset` is the identity up to that point — so raw and offset agree at the edge.
        assert_eq!(swipe_offset(SWIPE_COMMIT_PX), SWIPE_COMMIT_PX);
        assert!(swipe_armed(swipe_offset(SWIPE_COMMIT_PX)));
        assert!(!swipe_armed(swipe_offset(SWIPE_COMMIT_PX - 1)));
        assert!(!swipe_armed(swipe_offset(-(SWIPE_COMMIT_PX - 1))));
        assert!(swipe_armed(swipe_offset(-SWIPE_COMMIT_PX)));
    }

    #[test]
    fn swipe_tracks_the_finger_then_resists_and_clamps() {
        assert_eq!(swipe_offset(0), 0);
        assert_eq!(swipe_offset(40), 40); // 1:1 below the commit point
        assert_eq!(swipe_offset(-40), -40); // symmetric
        // Past it, 40% of finger travel — still moving, visibly harder.
        assert_eq!(swipe_offset(160), SWIPE_COMMIT_PX + 40);
        // And never off the screen, however far the finger goes.
        assert_eq!(swipe_offset(100_000), SWIPE_MAX_PX);
        assert_eq!(swipe_offset(-100_000), -SWIPE_MAX_PX);
    }

    fn artist_lib() -> Library {
        let mut l = lib();
        l.songs.push(song("orphan", "Solo", "2:00", 900));
        l
    }

    #[test]
    fn artist_page_collects_that_artists_albums_and_tracks() {
        let l = artist_lib();
        let p = artist_page(&l, "One");
        // Both of One's albums, and NOT Two's — with the flat indices the Album screen takes.
        assert_eq!(p.albums.iter().map(|(f, a)| (*f, a.name.as_str())).collect::<Vec<_>>(),
            vec![(0, "A1"), (1, "A2")]);
        // Every track of both albums, in album order, each labelled with its own album.
        assert_eq!(p.tracks.len(), 5); // A1 has 3, A2 has 2
        assert_eq!(p.tracks[0].album, "A1");
        assert_eq!(p.tracks[4].album, "A2");
        // An artist with tracks but no album rows still gets a page (the Songs fallback) rather
        // than an empty one, which is what the Artists tab's track count promised.
        let solo = artist_page(&l, "Solo");
        assert!(solo.albums.is_empty());
        assert_eq!(solo.tracks.len(), 1);
        assert_eq!(solo.tracks[0].song.object_id, 900);
    }

    #[test]
    fn artist_hit_mirrors_the_rows_the_page_draws() {
        let l = artist_lib();
        let p = artist_page(&l, "One");
        let top = artist_content_top();
        // Section headers are labels, not targets.
        assert_eq!(artist_hit(&p, 0, top + ARTIST_SEC_H / 2), None);
        // First album row → its FLAT index, which is what opens the Album drill-in.
        let a0 = top + ARTIST_SEC_H + ARTIST_ALBUM_RH / 2;
        assert_eq!(artist_hit(&p, 0, a0), Some(ArtistHit::Album(0)));
        assert_eq!(artist_hit(&p, 0, a0 + ARTIST_ALBUM_RH), Some(ArtistHit::Album(1)));
        // First track row, past both album rows and the SONGS header.
        let s0 = top + ARTIST_SEC_H + 2 * ARTIST_ALBUM_RH + ARTIST_SEC_H + ARTIST_TRACK_RH / 2;
        assert_eq!(artist_hit(&p, 0, s0), Some(ArtistHit::Track(0)));
        assert_eq!(artist_hit(&p, 0, s0 + ARTIST_TRACK_RH), Some(ArtistHit::Track(1)));
        // Scrolling shifts what is under the same y by exactly the scroll.
        assert_eq!(artist_hit(&p, ARTIST_TRACK_RH, s0), Some(ArtistHit::Track(1)));
        // Above the content and below the list are both misses.
        assert_eq!(artist_hit(&p, 0, top - 1), None);
        assert_eq!(artist_hit(&p, 0, LIST_BOTTOM), None);
    }
}
