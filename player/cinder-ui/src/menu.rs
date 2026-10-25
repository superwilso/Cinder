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
//!
//! Except in Big view, where squeezing defeated the point (its rows came out 63 px, not 80): the
//! rows keep the kit's pitch, START ON follows them with its chips two to a line ("Now Playing"
//! was cut four across), and the page scrolls under the strip. The geometry below is in page
//! coordinates; `nav` adds the scroll to a tap's y before asking.

use crate::canvas::Canvas;
use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets;

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
#[allow(non_snake_case)]
pub fn ROW_H() -> i32 {
    kit::ROW_H()
}

/// Space kept under the chips.
const BOTTOM_PAD: i32 = 16;
/// Top of the START ON label, and of the chips under it.
pub const LABEL_TOP: i32 = crate::H as i32 - BOTTOM_PAD - kit::CHIP_H - kit::SECTION_H;
pub const CHIPS_TOP: i32 = LABEL_TOP + kit::SECTION_H;

/// The pitch for `rows` rows: [`ROW_H()`] when they fit, otherwise whatever does fit between the
/// strip and the START ON label. Never below 52: two lines of text need it (checked by a test for
/// every count the Menu can have).
pub fn row_h(rows: usize) -> i32 {
    if rows == 0 || big() {
        return ROW_H();
    }
    ROW_H().min((LABEL_TOP - TOP) / rows as i32)
}

fn big() -> bool {
    crate::scale::large()
}

/// START ON's chips per line.
fn chip_cols() -> usize {
    if big() {
        2
    } else {
        HOMES.len()
    }
}

/// Top of the START ON label: pinned to the bottom of the glass, or in Big view straight under the
/// last row.
pub fn label_top(rows: usize) -> i32 {
    if big() {
        TOP + rows as i32 * ROW_H()
    } else {
        LABEL_TOP
    }
}

/// Top of START ON's chip line `line`.
fn chip_line_top(rows: usize, line: usize) -> i32 {
    label_top(rows) + kit::SECTION_H + line as i32 * (kit::CHIP_H + kit::CHIP_GAP)
}

/// How far the page scrolls: 0 except in Big view.
pub fn max_scroll_px(rows: usize) -> i32 {
    if !big() {
        return 0;
    }
    let lines = HOMES.len().div_ceil(chip_cols());
    let bottom = chip_line_top(rows, lines - 1) + kit::CHIP_H + BOTTOM_PAD;
    (bottom - crate::H as i32).max(0)
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

/// Which START ON chip is under `(x, y)`, with `rows` rows above them.
pub fn home_chip_at(x: i32, y: i32, rows: usize) -> Option<usize> {
    let cols = chip_cols();
    (0..HOMES.len().div_ceil(cols)).find_map(|line| {
        kit::chip_at(cols, chip_line_top(rows, line), x, y)
            .map(|i| line * cols + i)
            .filter(|&i| i < HOMES.len())
    })
}

/// `strip` is the Now Playing line; `home` the chosen [`HOMES`] index.
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    strip: &str,
    items: &[MenuItem],
    home: usize,
    scroll: i32,
) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, crate::i18n::text("Menu"), None);
    debug_assert_eq!(y0, STRIP_TOP, "menu strip drifted from the hit test");
    let y = kit::strip(c, t, f, y0, strip);
    debug_assert_eq!(y, TOP);
    let rh = row_h(items.len());
    // Big view's rows are the kit's full pitch, where the kit's own subtitle fits.
    let compact_subtitles = text::scale_pct() > 100 && !big();
    c.set_clip_y(TOP, crate::H as i32);
    for (i, m) in items.iter().enumerate() {
        let trail = if m.home {
            Trail::Tag(crate::i18n::text("HOME"))
        } else {
            Trail::Open("")
        };
        let yt = TOP + i as i32 * rh - scroll;
        if compact_subtitles && !m.sub.is_empty() {
            // The Menu has ten fixed destinations plus START ON. At enlarged type, its computed
            // row pitch is only 55 px; the shared row subtitle baseline then meets the separator.
            // Keep the Menu's two-line layout but use a smaller, locally positioned caption so the
            // caption clears both the title and the separator without changing other screens.
            kit::row(
                c,
                t,
                f,
                yt,
                rh,
                &Row::new(crate::i18n::text(m.label))
                    .sub(" ")
                    .trail(trail)
                    .sel(m.active),
            );
            let style = widgets::sty(Family::Sans, Weight::Regular, 12.0, t.dim, 0.0);
            let reserve = if m.home { 96.0 } else { 0.0 };
            let caption = widgets::fit(
                f,
                crate::i18n::text(m.sub),
                &style,
                (kit::RIGHT - kit::LEFT) as f32 - reserve,
            );
            text::draw(
                c,
                f,
                kit::LEFT as f32,
                (yt + rh - 8) as f32,
                &caption,
                &style,
            );
        } else {
            kit::row(
                c,
                t,
                f,
                yt,
                rh,
                &Row::new(crate::i18n::text(m.label))
                    .sub(m.sub)
                    .trail(trail)
                    .sel(m.active),
            );
        }
    }
    let rows = items.len();
    let label = crate::i18n::text("START ON");
    kit::section_label(c, t, f, label_top(rows) - scroll, label, None);
    let cols = chip_cols();
    let on = home.min(HOMES.len() - 1);
    for (line, chunk) in HOMES.chunks(cols).enumerate() {
        let top = chip_line_top(rows, line) - scroll;
        let sel = on.checked_sub(line * cols).filter(|&i| i < chunk.len());
        kit::chips_in(c, t, f, top, chunk, sel, cols);
    }
    c.clear_clip();
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
            assert!(
                last_bottom <= LABEL_TOP,
                "{rows} rows run to {last_bottom}, into START ON"
            );
            assert_eq!(
                row_at(TOP + (rows as i32 - 1) * rh + rh / 2, rows),
                Some(rows - 1)
            );
            assert!(rh >= 52, "{rows} rows squeeze the pitch to {rh}");
        }
        assert_eq!(
            row_h(8),
            ROW_H(),
            "the designed eight rows keep the kit's pitch"
        );
    }

    /// The strip, the rows and the chips do not overlap as targets.
    #[test]
    fn the_strip_rows_and_chips_are_separate_targets() {
        assert!(strip_hit(STRIP_TOP) && strip_hit(TOP - 1) && !strip_hit(TOP));
        assert_eq!(row_at(TOP - 1, 10), None);
        assert!(home_chip_at(240, CHIPS_TOP + 5, 10).is_some());
        assert_eq!(
            home_chip_at(240, LABEL_TOP + 5, 10),
            None,
            "the label is not a chip"
        );
        assert!(CHIPS_TOP + kit::CHIP_H <= crate::H as i32);
        assert_eq!(max_scroll_px(10), 0);
    }

    /// Big view: full-pitch rows, START ON under them two chips to a line, each chip picking
    /// itself, and a scroll that reaches the last line.
    #[test]
    fn big_view_keeps_full_rows_and_scrolls_to_start_on() {
        crate::scale::set_large(true);
        let rows = 9;
        assert_eq!(row_h(rows), ROW_H());
        assert_eq!(label_top(rows), TOP + 9 * ROW_H());
        let picks: Vec<_> = (0..HOMES.len())
            .map(|i| {
                let (x, w) = kit::chip_span(i % 2, 2);
                home_chip_at(x + w / 2, chip_line_top(rows, i / 2) + 5, rows)
            })
            .collect();
        let last = chip_line_top(rows, 1) + kit::CHIP_H;
        let max = max_scroll_px(rows);
        crate::scale::set_large(false);
        assert_eq!(picks, [Some(0), Some(1), Some(2), Some(3)]);
        assert!(last + BOTTOM_PAD - max <= crate::H as i32);
    }
}
