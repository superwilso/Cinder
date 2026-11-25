//! v0.5's two sheets over Now Playing (`docs/PLAN_v0.5.md` P2), from the boards
//! `NowPlayingMore.dc.html` (tap the title: this song's actions) and `ProposalPlaces.dc.html`
//! (Places: six tiles and a list of the rest). Both are a panel rising to a fixed top, a grab
//! handle, a header, their content, and a Close bar along the bottom.
//!
//! Each `*_hit` reads the same numbers as its draw, as everywhere else.

use crate::canvas::{Canvas, H, W};
use crate::kit::{self, Row};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, fit, hline, stroke_rect, sty};

/// The Close bar's top: the last 64 px of the screen.
pub const CLOSE_TOP: i32 = H as i32 - 64;
const SIDE: i32 = 24;

/// What a tap on a sheet means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetHit {
    /// The panel's own blank space or header: nothing.
    Panel,
    /// Above the panel, or the Close bar: close it.
    Close,
    Item(usize),
}

/// Dim what is behind, then the panel from `top` down, its handle, and the Close bar. Returns the
/// y under the handle.
fn panel(c: &mut Canvas, t: &Theme, f: &FontSet, top: i32) -> i32 {
    // The veil: the screen behind at a fifth of its light, so the sheet is plainly in front.
    for y in crate::chrome::STATUS_H..top {
        for x in 0..W as i32 {
            c.blend(x, y, t.bg, 200);
        }
    }
    fill_rect(c, 0, top, W as i32, H as i32 - top, t.panel);
    hline(c, top, t.ctrl());
    fill_rect(c, W as i32 / 2 - 24, top + 8, 48, 4, t.ctrl());
    hline(c, CLOSE_TOP, t.line);
    center(
        c,
        f,
        W as f32 / 2.0,
        (CLOSE_TOP + 39) as f32,
        crate::i18n::text("Close"),
        &sty(Family::Sans, Weight::Bold, crate::scale::ROW, t.ink, 0.0),
    );
    top + 20
}

fn common_hit(top: i32, y: i32) -> Option<SheetHit> {
    if y < top || y >= CLOSE_TOP {
        Some(SheetHit::Close)
    } else {
        None
    }
}

// ── This song's actions ─────────────────────────────────────────────────────────────────────

const ACT_TOP: i32 = 176;
const ACT_HEAD: i32 = 68;
const ACT_ROW: i32 = 72;

/// The title tap's sheet: "THIS SONG", the title, then `rows` (Lyrics, Track info, Go to album,
/// Add to playlist, Sleep timer, Output: the app decides which apply).
pub fn song_actions(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, rows: &[Row]) {
    let y = panel(c, t, f, ACT_TOP);
    let lst = sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.16);
    text::draw(
        c,
        f,
        SIDE as f32,
        (y + 26) as f32,
        crate::i18n::text("THIS SONG"),
        &lst,
    );
    let tst = sty(Family::Sans, Weight::Bold, crate::scale::TITLE, t.ink, 0.0);
    let s = fit(f, title, &tst, (W as i32 - 2 * SIDE) as f32);
    text::draw(c, f, SIDE as f32, (y + 54) as f32, &s, &tst);
    hline(c, y + ACT_HEAD - 1, t.line);
    let mut ry = y + ACT_HEAD;
    for r in rows.iter().take(song_actions_fit()) {
        ry = kit::row(c, t, f, ry, ACT_ROW, r);
    }
}

/// How many action rows fit above the Close bar.
pub fn song_actions_fit() -> usize {
    ((CLOSE_TOP - ACT_TOP - 20 - ACT_HEAD) / ACT_ROW) as usize
}

pub fn song_actions_hit(n: usize, y: i32) -> SheetHit {
    if let Some(h) = common_hit(ACT_TOP, y) {
        return h;
    }
    let first = ACT_TOP + 20 + ACT_HEAD;
    let i = (y - first).div_euclid(ACT_ROW);
    if y >= first && (i as usize) < n.min(song_actions_fit()) {
        SheetHit::Item(i as usize)
    } else {
        SheetHit::Panel
    }
}

// ── Places ──────────────────────────────────────────────────────────────────────────────────

const PL_TOP: i32 = 52;
const TILE_TOP: i32 = PL_TOP + 20 + 56 + 4;
const TILE_H: i32 = 96;
const GAP: i32 = 8;
const TILE_W: i32 = (W as i32 - 2 * SIDE - GAP) / 2;
const LIST_TOP: i32 = TILE_TOP + 3 * TILE_H + 2 * GAP + 16;
const LI_H: i32 = 56;

/// A tile: a name and what is there now ("412 albums").
pub struct Tile<'a> {
    pub name: &'a str,
    pub sub: &'a str,
}

/// Places: up to six tiles, then the rest as a two-column list. Opt-in places (SensMe, Preview,
/// Audiobooks) are in `list` only while they are switched on.
pub fn places(c: &mut Canvas, t: &Theme, f: &FontSet, tiles: &[Tile], list: &[&str]) {
    let y = panel(c, t, f, PL_TOP);
    let hst = sty(Family::Sans, Weight::ExtraBold, 26.0, t.ink, 0.0);
    text::draw(
        c,
        f,
        SIDE as f32,
        (y + 38) as f32,
        crate::i18n::text("Places"),
        &hst,
    );
    let nst = sty(Family::Sans, Weight::Bold, 21.0, t.ink, 0.0);
    let sst = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
    for (i, tile) in tiles.iter().take(6).enumerate() {
        let (x, ty) = tile_at(i);
        fill_rect(c, x, ty, TILE_W, TILE_H, t.bg);
        stroke_rect(c, x, ty, TILE_W, TILE_H, t.line, 1);
        let w = (TILE_W - 32) as f32;
        let name = fit(f, crate::i18n::text(tile.name), &nst, w);
        text::draw(
            c,
            f,
            (x + 16) as f32,
            (ty + TILE_H - 38) as f32,
            &name,
            &nst,
        );
        text::draw(
            c,
            f,
            (x + 16) as f32,
            (ty + TILE_H - 16) as f32,
            &fit(f, tile.sub, &sst, w),
            &sst,
        );
    }
    hline(c, LIST_TOP, t.line);
    let lst = sty(Family::Sans, Weight::SemiBold, 18.0, t.ink, 0.0);
    for (i, name) in list.iter().take(list_fit()).enumerate() {
        let (x, ly) = li_at(i);
        let s = fit(f, crate::i18n::text(name), &lst, (TILE_W - 24) as f32);
        text::draw(c, f, (x + 12) as f32, (ly + LI_H / 2 + 7) as f32, &s, &lst);
        fill_rect(c, x, ly + LI_H - 1, TILE_W, 1, t.line);
    }
}

fn tile_at(i: usize) -> (i32, i32) {
    let (col, row) = ((i % 2) as i32, (i / 2) as i32);
    (SIDE + col * (TILE_W + GAP), TILE_TOP + row * (TILE_H + GAP))
}

fn li_at(i: usize) -> (i32, i32) {
    let (col, row) = ((i % 2) as i32, (i / 2) as i32);
    (SIDE + col * (TILE_W + GAP), LIST_TOP + 1 + row * LI_H)
}

/// How many list items fit above the Close bar.
pub fn list_fit() -> usize {
    (2 * ((CLOSE_TOP - LIST_TOP - 1) / LI_H)) as usize
}

/// A tap on Places: tiles are items `0..tiles`, list items follow them.
pub fn places_hit(tiles: usize, list: usize, x: i32, y: i32) -> SheetHit {
    if let Some(h) = common_hit(PL_TOP, y) {
        return h;
    }
    let inside = |(bx, by): (i32, i32), h: i32| {
        // Half the gap belongs to each neighbour, so there is no dead strip.
        x >= bx - GAP / 2 && x < bx + TILE_W + GAP / 2 && y >= by - GAP / 2 && y < by + h + GAP / 2
    };
    if let Some(i) = (0..tiles.min(6)).find(|&i| inside(tile_at(i), TILE_H)) {
        return SheetHit::Item(i);
    }
    match (0..list.min(list_fit())).find(|&i| inside(li_at(i), LI_H - GAP)) {
        Some(i) => SheetHit::Item(tiles.min(6) + i),
        None => SheetHit::Panel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_row_answers_where_it_is_drawn() {
        let n = 6;
        assert_eq!(song_actions_fit(), 6, "the board's six rows fit");
        for i in 0..n {
            let y = ACT_TOP + 20 + ACT_HEAD + i as i32 * ACT_ROW + ACT_ROW / 2;
            assert_eq!(song_actions_hit(n, y), SheetHit::Item(i));
        }
        assert_eq!(song_actions_hit(n, ACT_TOP - 1), SheetHit::Close);
        assert_eq!(song_actions_hit(n, CLOSE_TOP + 10), SheetHit::Close);
        assert_eq!(song_actions_hit(n, ACT_TOP + 40), SheetHit::Panel);
    }

    #[test]
    fn every_place_answers_where_it_is_drawn() {
        let (tiles, list) = (6, 8);
        assert!(
            list_fit() >= 8,
            "the board's eight list items fit: {}",
            list_fit()
        );
        for i in 0..tiles {
            let (x, y) = tile_at(i);
            assert_eq!(
                places_hit(tiles, list, x + TILE_W / 2, y + TILE_H / 2),
                SheetHit::Item(i)
            );
        }
        for i in 0..list {
            let (x, y) = li_at(i);
            assert_eq!(
                places_hit(tiles, list, x + 20, y + LI_H / 2),
                SheetHit::Item(tiles + i)
            );
        }
        assert_eq!(places_hit(tiles, list, 240, 780), SheetHit::Close);
        assert_eq!(places_hit(tiles, list, 240, 20), SheetHit::Close);
    }
}
