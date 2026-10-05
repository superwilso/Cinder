//! Editing one of Cinder's own playlists (design handoff 5b): ≡ to move, × to remove, UNDO in the
//! section label, DONE in the header's right slot.
//!
//! An EDITOR, not the playlist page with extra buttons. Nothing here touches the file until DONE:
//! the navigator holds a working order, every change pushes the order it replaced onto an undo
//! stack, and DONE hands the finished order to the shell, which writes the `.m3u8` once (a
//! temporary file and a rename). Back leaves without saving — the same "leave without applying"
//! the keyboard has. So a slip of the thumb costs one UNDO, and a whole wrong session costs Back.
//!
//! Only playlists Cinder can write reach this screen: Sony's live in its database, and a smart
//! playlist's members are rules (its EDIT opens `view_edit` instead).
//!
//!   header    Edit playlist                                  DONE
//!   strip     NIGHT BUS
//!   label     12 TRACKS                                      UNDO
//!   rows      ≡  Title / artist                                 ×
//!
//! Every coordinate is a function here, read by the renderer AND the navigator's hit tests, so a
//! row can never be drawn in one place and tapped in another.

use crate::canvas::{H, W};
use crate::icons;
use crate::kit;
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::up_next::RowDrag;
use crate::widgets::{fill_rect, fit, hline, sty};
use crate::Canvas;

/// A track row: the Design Guide's 62 px track height.
pub const RH: i32 = crate::scale::TRACK_ROW_H;
/// The strip naming the playlist.
pub const STRIP_Y: i32 = crate::chrome::HEADER_BOTTOM;
/// The "N TRACKS · UNDO" section label.
pub const LABEL_Y: i32 = STRIP_Y + kit::STRIP_H;
/// Top of the scrolling rows.
pub const LIST_TOP: i32 = LABEL_Y + kit::SECTION_H;
/// Bottom of the scrolling rows: clear of the Shelf's swipe-up zone, as the pickers are.
pub const LIST_BOTTOM: i32 = H as i32 - 40;
/// The ≡ column. A vertical drag that STARTS here lifts the row — start-point ownership, the rule
/// Up Next's handle follows — and anywhere else it scrolls. A hold lifts a row from anywhere.
pub const GRIP_X1: i32 = 64;
/// The × column, stopping short of the scrollbar's grab strip on the right edge.
pub const REMOVE_X0: i32 = 400;
pub const REMOVE_X1: i32 = W as i32 - crate::library::SBAR_GRAB_W;

pub fn view_h() -> i32 {
    LIST_BOTTOM - LIST_TOP
}

pub fn content_h(n: usize) -> i32 {
    n as i32 * RH
}

pub fn max_scroll(n: usize) -> i32 {
    (content_h(n) - view_h()).max(0)
}

/// Screen-y of row `i`'s top.
pub fn row_top(i: usize, scroll_px: i32) -> i32 {
    LIST_TOP + i as i32 * RH - scroll_px.max(0)
}

/// The row under `y`, if any.
pub fn row_at(n: usize, scroll_px: i32, y: i32) -> Option<usize> {
    if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let i = ((y - LIST_TOP + scroll_px.max(0)) / RH) as usize;
    (i < n).then_some(i)
}

/// Is `x` in the ≡ column?
pub fn hit_grip(x: i32) -> bool {
    x < GRIP_X1
}

/// The row whose × is under `(x, y)`.
pub fn hit_remove(n: usize, scroll_px: i32, x: i32, y: i32) -> Option<usize> {
    if !(REMOVE_X0..REMOVE_X1).contains(&x) {
        return None;
    }
    row_at(n, scroll_px, y)
}

/// Is `(x, y)` on UNDO? Only meaningful while there is something to undo — the caller asks.
pub fn hit_undo(x: i32, y: i32) -> bool {
    kit::section_action_hit(LABEL_Y, x, y)
}

/// Where a row lifted from `from` would land with its top at screen-y `float_top`: the slot its
/// middle is over, clamped to the list.
pub fn slot_for(n: usize, float_top: i32, scroll_px: i32) -> usize {
    if n == 0 {
        return 0;
    }
    let mid = float_top + RH / 2 - LIST_TOP + scroll_px.max(0);
    (mid.div_euclid(RH)).clamp(0, n as i32 - 1) as usize
}

/// What the editor shows.
pub struct EditView<'a> {
    /// The playlist's name, for the strip.
    pub name: &'a str,
    /// The rows in the working order.
    pub rows: Vec<&'a SongRow>,
    pub scroll_px: i32,
    /// A row being dragged, in this list's index space.
    pub drag: Option<RowDrag>,
    pub can_undo: bool,
    pub sbar_active: bool,
}

fn row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, s: &SongRow, lifted: bool) {
    if lifted {
        fill_rect(c, 0, y, W as i32, RH, t.row_sel);
        hline(c, y, t.acc);
    }
    let cy = y + RH / 2;
    icons::grip(
        c,
        34.0,
        cy as f32,
        20.0,
        if lifted { t.acc } else { t.faint },
    );
    let x = (kit::LEFT + 26 + 14 + 8) as f32;
    let w = (REMOVE_X0 - 8) as f32 - x;
    let tst = sty(
        Family::Sans,
        Weight::SemiBold,
        crate::scale::ROW,
        if lifted { t.acc } else { t.ink },
        0.0,
    );
    text::draw(c, f, x, (cy - 3) as f32, &fit(f, &s.title, &tst, w), &tst);
    let sst = sty(
        Family::Sans,
        Weight::Regular,
        crate::scale::SECONDARY,
        t.dim,
        0.0,
    );
    text::draw(c, f, x, (cy + 18) as f32, &fit(f, &s.artist, &sst, w), &sst);
    icons::close(
        c,
        ((REMOVE_X0 + REMOVE_X1) / 2) as f32,
        cy as f32,
        12.0,
        t.faint,
    );
    hline(c, y + RH - 1, if lifted { t.acc } else { t.line });
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &EditView) {
    c.fill(t.bg);
    crate::chrome::header_action(c, t, f, "Edit playlist", "DONE");
    kit::strip(c, t, f, STRIP_Y, &v.name.to_uppercase());
    let n = v.rows.len();
    let count = if n == 1 {
        "1 TRACK".to_string()
    } else {
        format!("{n} TRACKS")
    };
    kit::section_label(c, t, f, LABEL_Y, &count, v.can_undo.then_some("UNDO"));

    let scroll = v.scroll_px.clamp(0, max_scroll(n));
    c.set_clip_y(LIST_TOP, LIST_BOTTOM);
    if n == 0 {
        let st = sty(
            Family::Sans,
            Weight::Regular,
            crate::scale::SECONDARY,
            t.dim,
            0.0,
        );
        let msg = fit(
            f,
            "Every track removed. DONE saves it empty.",
            &st,
            (kit::RIGHT - kit::LEFT) as f32,
        );
        text::draw(c, f, kit::LEFT as f32, (LIST_TOP + 40) as f32, &msg, &st);
    }
    // While a row is lifted the others part around the slot it would land in: the CONTENT flows
    // through fixed slots, the way Up Next's drag looks.
    let order: Vec<usize> = {
        let mut o: Vec<usize> = (0..n).collect();
        if let Some(d) = v.drag.filter(|d| d.from < n && d.to < n) {
            let it = o.remove(d.from);
            o.insert(d.to, it);
        }
        o
    };
    let lifted = v.drag.filter(|d| d.from < n).map(|d| d.from);
    for (slot, &i) in order.iter().enumerate() {
        let y = row_top(slot, scroll);
        if y + RH <= LIST_TOP || y >= LIST_BOTTOM || Some(i) == lifted {
            continue;
        }
        row(c, t, f, y, v.rows[i], false);
    }
    c.clear_clip();
    crate::library::scrollbar(
        c,
        t,
        LIST_TOP,
        LIST_BOTTOM,
        scroll,
        content_h(n),
        v.sbar_active,
    );
    // The lifted row floats under the finger, over everything, unclipped at the list's edges so it
    // never vanishes mid-gesture.
    if let (Some(d), Some(i)) = (v.drag, lifted) {
        let y = d.float_top().clamp(LIST_TOP - RH / 2, LIST_BOTTOM - RH / 2);
        row(c, t, f, y, v.rows[i], true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row is hit where it is drawn, at any scroll, and the columns do not overlap.
    #[test]
    fn rows_and_columns_hit_where_they_are_drawn() {
        let n = 30;
        for scroll in [0, 17, RH, 5 * RH + 3, max_scroll(n)] {
            for i in 0..n {
                let top = row_top(i, scroll);
                let mid = top + RH / 2;
                if !(LIST_TOP..LIST_BOTTOM).contains(&mid) {
                    continue;
                }
                assert_eq!(row_at(n, scroll, mid), Some(i), "scroll {scroll} row {i}");
                assert_eq!(hit_remove(n, scroll, REMOVE_X0 + 10, mid), Some(i));
                assert_eq!(
                    hit_remove(n, scroll, GRIP_X1 + 10, mid),
                    None,
                    "the title is not the ×"
                );
            }
        }
        assert!(
            REMOVE_X1 - REMOVE_X0 >= 44 && GRIP_X1 >= 44,
            "both columns are real targets"
        );
        assert_eq!(row_at(n, 0, LIST_TOP - 1), None);
        assert_eq!(row_at(2, 0, LIST_TOP + 2 * RH), None, "below the last row");
    }

    /// The landing slot follows the lifted row's middle and never leaves the list.
    #[test]
    fn a_lifted_row_lands_under_its_middle() {
        let n = 5;
        assert_eq!(slot_for(n, row_top(0, 0), 0), 0);
        assert_eq!(slot_for(n, row_top(3, 0), 0), 3);
        assert_eq!(slot_for(n, row_top(3, 0) + RH / 2 + 1, 0), 4);
        assert_eq!(slot_for(n, LIST_TOP - 500, 0), 0);
        assert_eq!(slot_for(n, LIST_BOTTOM + 500, 0), n - 1);
        assert_eq!(slot_for(0, LIST_TOP, 0), 0);
    }
}
