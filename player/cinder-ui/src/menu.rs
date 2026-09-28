//! Menu (the hub) — design handoff 5a.
//!
//! ```text
//!   Menu
//!   NOW › Nils Frahm · Says · 4:12 of 8:18        the strip: tap it for Now Playing
//!   Library               3,184 songs … HOME ›    one kit row per destination, title + state
//!   …
//!   START ON
//!   [Library] [Now Playing] [Menu] [Last screen]  what the player opens on
//! ```
//!
//! The Now Playing row became the strip, because "what is playing" is a line of state and not a
//! place to go, and the strip is the kit's place for a line of state. Up Next left with it: the
//! queue is one tap from Now Playing's toolbar, where the thing it is the queue of is on screen.
//! The home-screen picker lives here rather than three levels into Settings — the handoff's reason
//! is that the Menu is where you are when you think "I wish it opened on…".
//!
//! The list does NOT scroll, so the pitch is sized to fit every row between the strip and the chips
//! — see [`row_h`]. Everything that positions or hit-tests a Menu row derives from it.

use crate::canvas::Canvas;
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;

pub struct MenuItem<'a> {
    pub label: &'a str,
    /// The second line: live state for the row ("241 albums · 3,184 songs").
    pub sub: &'a str,
    /// The row is where the player opens (the HOME tag).
    pub home: bool,
    /// Cursor (button navigation).
    pub active: bool,
}

/// The home screens, in chip order. Index = `App::home_screen`.
pub const HOMES: [&str; 4] = ["Library", "Now Playing", "Menu", "Last screen"];

/// The strip's top edge (straight under the header) and the first row's.
pub const STRIP_TOP: i32 = crate::chrome::HEADER_BOTTOM;
pub const TOP: i32 = STRIP_TOP + kit::STRIP_H;
/// Designed pitch: the kit's row.
pub const ROW_H: i32 = kit::ROW_H;
/// Space kept under the chips.
const BOTTOM_PAD: i32 = 16;
/// Top of the START ON label, and of the chips under it.
pub const LABEL_TOP: i32 = crate::H as i32 - BOTTOM_PAD - kit::CHIP_H - kit::SECTION_H;
pub const CHIPS_TOP: i32 = LABEL_TOP + kit::SECTION_H;

/// The pitch for `rows` rows: [`ROW_H`] when they fit, otherwise whatever does fit between the
/// strip and the START ON label. Never below 52: two lines of text need it (checked by a test for
/// every count the Menu can have).
pub fn row_h(rows: usize) -> i32 {
    if rows == 0 {
        return ROW_H;
    }
    ROW_H.min((LABEL_TOP - TOP) / rows as i32)
}

/// Which menu row is under `y`, given how many rows there are.
pub fn row_at(y: i32, rows: usize) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / row_h(rows)) as usize;
    (r < rows).then_some(r)
}

/// Is `y` on the Now Playing strip?
pub fn strip_hit(y: i32) -> bool {
    (STRIP_TOP..TOP).contains(&y)
}

/// Which START ON chip is under `(x, y)`.
pub fn home_chip_at(x: i32, y: i32) -> Option<usize> {
    kit::chip_at(HOMES.len(), CHIPS_TOP, x, y)
}

/// `strip` is the Now Playing line; `home` the chosen [`HOMES`] index.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, strip: &str, items: &[MenuItem], home: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Menu", None);
    debug_assert_eq!(y0, STRIP_TOP, "menu strip drifted from the hit test");
    let y = kit::strip(c, t, f, y0, strip);
    debug_assert_eq!(y, TOP);
    let rh = row_h(items.len());
    for (i, m) in items.iter().enumerate() {
        let trail = if m.home { Trail::Tag("HOME") } else { Trail::Open("") };
        let yt = TOP + i as i32 * rh;
        kit::row(c, t, f, yt, rh, &Row::new(m.label).sub(m.sub).trail(trail).sel(m.active));
    }
    kit::section_label(c, t, f, LABEL_TOP, "START ON", None);
    kit::chips(c, t, f, CHIPS_TOP, &HOMES, Some(home.min(HOMES.len() - 1)));
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    /// Every row the Menu can show is ON the glass, above the START ON block, and two lines of text
    /// still fit the pitch.
    #[test]
    fn every_menu_row_fits_above_the_home_picker() {
        for rows in 1..=11 {
            let rh = row_h(rows);
            let last_bottom = TOP + rows as i32 * rh;
            assert!(last_bottom <= LABEL_TOP, "{rows} rows run to {last_bottom}, into START ON");
            assert_eq!(row_at(TOP + (rows as i32 - 1) * rh + rh / 2, rows), Some(rows - 1));
            assert!(rh >= 52, "{rows} rows squeeze the pitch to {rh}");
        }
        assert_eq!(row_h(8), ROW_H, "the designed eight rows keep the kit's pitch");
    }

    /// The strip, the rows and the chips do not overlap as targets.
    #[test]
    fn the_strip_rows_and_chips_are_separate_targets() {
        assert!(strip_hit(STRIP_TOP) && strip_hit(TOP - 1) && !strip_hit(TOP));
        assert_eq!(row_at(TOP - 1, 10), None);
        assert!(home_chip_at(240, CHIPS_TOP + 5).is_some());
        assert_eq!(home_chip_at(240, LABEL_TOP + 5), None, "the label is not a chip");
        assert!(CHIPS_TOP + kit::CHIP_H <= crate::H as i32);
    }
}
