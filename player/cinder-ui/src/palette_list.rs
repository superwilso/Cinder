//! Settings ▸ Display ▸ Palette — the palette picker (design handoff 5j).
//!
//! ```text
//!   Palette
//!   SORT
//!   [   Name   ][  Added  ]
//!   PALETTES
//!   Cinder            Built in              ▮▮▮▮▮   the one in use: row_sel, title in the accent
//!   Slate             slate.palette         ▮▮▮▮▮
//!   SKIPPED
//!   neon.palette      line 3: `day.ink` is not a colour
//!   ADD
//!   Add a palette     Copy .palette files to cinder_palettes/
//! ```
//!
//! Until this screen the Palette row CYCLED: each tap moved to the next file, so choosing the fifth
//! palette meant four taps through colour schemes you did not want, and a file the player refused
//! was only ever reported in a log nobody reads. This lists what the folder holds, with each
//! palette's own colours beside its name, and says in plain words why a file was skipped — the
//! first of `Palette::parse`'s problems, which is the one to fix first.
//!
//! The built-in palette is always first, whatever the sort: it is the way back from any file.
//!
//! The list scrolls (32 files are allowed), so the layout is ONE function, [`items`], that both
//! `render` and the hit test read.

use crate::canvas::{Canvas, H, W};
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::widgets::{fill_rect, stroke_rect};
use embedded_graphics::pixelcolor::Rgb888;

/// The sort chips, in order. Index = `App::palette_sort`, persisted as `palette_sort=name|added`.
pub const SORTS: [&str; 2] = ["Name", "Added"];
pub const SORT_WORDS: [&str; 2] = ["name", "added"];

/// The swatch: five cells of 18 x 28 — background, line, dim, ink, accent, the five colours a
/// palette file actually changes.
const CELL_W: i32 = 18;
const CELL_H: i32 = 28;
pub const SWATCH_W: i32 = CELL_W * 5;

/// One palette row.
pub struct Entry {
    pub name: String,
    /// "Built in", or the file it came from.
    pub sub: String,
    /// The palette's own colours, in the mode on screen: bg, line, dim, ink, accent.
    pub cells: [Rgb888; 5],
    /// The palette in use.
    pub active: bool,
}

/// One refused file: its name and the first reason.
pub struct Skipped {
    pub file: String,
    pub why: String,
}

/// Split one of `palette::load_files`'s skip messages ("file: problem; problem") into the file and
/// its FIRST problem. The first is the one to fix first, and a row has room for one sentence.
pub fn skipped_from(msg: &str) -> Skipped {
    let (file, rest) = msg.split_once(": ").unwrap_or((msg, ""));
    let why = rest.split("; ").next().unwrap_or("").to_string();
    Skipped {
        file: file.to_string(),
        why,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Sort(usize),
    Entry(usize),
    Skipped(usize),
    Add,
}

/// A label or a row, with its content-space top (0 = straight under the header) and height.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Label(&'static str),
    Chips,
    Row(Item),
}

/// The whole layout, in order. `render`, `item_at` and `content_height` all read this.
fn parts(entries: usize, skipped: usize) -> Vec<(Part, i32, i32)> {
    let mut out = Vec::new();
    let mut y = 0;
    let mut push = |p: Part, h: i32| {
        out.push((p, y, h));
        y += h;
    };
    push(Part::Label("SORT"), kit::SECTION_H);
    push(Part::Chips, kit::CHIP_H + 8);
    push(Part::Label("PALETTES"), kit::SECTION_H);
    for i in 0..entries {
        push(Part::Row(Item::Entry(i)), kit::ROW_H);
    }
    if skipped > 0 {
        push(Part::Label("SKIPPED"), kit::SECTION_H);
        for i in 0..skipped {
            push(Part::Row(Item::Skipped(i)), kit::ROW_H);
        }
    }
    push(Part::Label("ADD"), kit::SECTION_H);
    push(Part::Row(Item::Add), kit::ROW_H);
    out
}

const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// Content height below the header.
pub fn content_height(entries: usize, skipped: usize) -> i32 {
    parts(entries, skipped).last().map_or(0, |&(_, y, h)| y + h)
}

pub fn max_scroll(entries: usize, skipped: usize) -> i32 {
    (content_height(entries, skipped) + 8 - (H as i32 - TOP)).max(0)
}

/// What is under screen point `(x, y)` at scroll `scroll`.
pub fn item_at(x: i32, y: i32, scroll: i32, entries: usize, skipped: usize) -> Option<Item> {
    if y < TOP {
        return None;
    }
    let cy = y - TOP + scroll;
    for (p, top, h) in parts(entries, skipped) {
        if !(top..top + h).contains(&cy) {
            continue;
        }
        return match p {
            Part::Label(_) => None,
            Part::Chips => kit::chip_at(SORTS.len(), top, x, cy).map(Item::Sort),
            Part::Row(item) => Some(item),
        };
    }
    None
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    sort: usize,
    entries: &[Entry],
    skipped: &[Skipped],
    scroll: i32,
) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Palette", None);
    c.set_clip_y(TOP, H as i32);
    for (p, top, h) in parts(entries.len(), skipped.len()) {
        let y = TOP + top - scroll;
        if y + h < TOP || y >= H as i32 {
            continue;
        }
        match p {
            Part::Label(l) => {
                kit::section_label(c, t, f, y, l, None);
            }
            Part::Chips => {
                kit::chips(c, t, f, y, &SORTS, Some(sort.min(SORTS.len() - 1)));
            }
            Part::Row(Item::Entry(i)) => {
                let e = &entries[i];
                kit::row(
                    c,
                    t,
                    f,
                    y,
                    kit::ROW_H,
                    &Row::new(&e.name)
                        .sub(&e.sub)
                        .trail(Trail::Reserve(SWATCH_W))
                        .sel(e.active),
                );
                let x0 = kit::RIGHT - SWATCH_W;
                let sy = y + (kit::ROW_H - CELL_H) / 2;
                for (k, col) in e.cells.iter().enumerate() {
                    fill_rect(
                        c,
                        x0 + k as i32 * CELL_W,
                        sy,
                        CELL_W,
                        CELL_H,
                        t.scale_color(*col),
                    );
                }
                stroke_rect(c, x0, sy, SWATCH_W, CELL_H, t.ctrl(), 1);
            }
            Part::Row(Item::Skipped(i)) => {
                let s = &skipped[i];
                // No value: the section label already says SKIPPED, and the reason needs the width.
                kit::row(c, t, f, y, kit::ROW_H, &Row::new(&s.file).sub(&s.why));
            }
            Part::Row(Item::Add) => {
                kit::row(
                    c,
                    t,
                    f,
                    y,
                    kit::ROW_H,
                    &Row::new("Add a palette").sub("Copy .palette files to cinder_palettes/"),
                );
            }
            Part::Row(Item::Sort(_)) => {}
        }
    }
    c.clear_clip();
    let _ = W;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skip_message_splits_into_the_file_and_its_first_problem() {
        let s = skipped_from(
            "neon.palette: line 3: `day.ink` is not a colour; line 9: unknown key `glow`",
        );
        assert_eq!(s.file, "neon.palette");
        assert_eq!(s.why, "line 3: `day.ink` is not a colour");
        let s = skipped_from("odd");
        assert_eq!((s.file.as_str(), s.why.as_str()), ("odd", ""));
    }

    /// The hit test reads the same layout the render draws: every row answers at its centre.
    #[test]
    fn every_row_answers_where_it_is_drawn() {
        let (n, k) = (3, 2);
        let mut seen = Vec::new();
        for (p, top, h) in parts(n, k) {
            if let Part::Row(item) = p {
                assert_eq!(item_at(240, TOP + top + h / 2, 0, n, k), Some(item));
                seen.push(item);
            }
        }
        assert_eq!(seen.len(), n + k + 1);
        let (x, w) = kit::chip_span(1, 2);
        assert_eq!(
            item_at(x + w / 2, TOP + kit::SECTION_H + kit::CHIP_H / 2, 0, n, k),
            Some(Item::Sort(1))
        );
        assert_eq!(
            item_at(240, TOP + 5, 0, n, k),
            None,
            "a label is not a target"
        );
    }

    /// No SKIPPED section when nothing was skipped, and the list scrolls only when it must.
    #[test]
    fn the_skipped_section_and_the_scroll_appear_only_when_needed() {
        assert!(!parts(1, 0)
            .iter()
            .any(|(p, _, _)| *p == Part::Label("SKIPPED")));
        assert!(parts(1, 1)
            .iter()
            .any(|(p, _, _)| *p == Part::Label("SKIPPED")));
        assert_eq!(max_scroll(1, 0), 0);
        assert!(max_scroll(crate::palette::MAX_FILES + 1, 3) > 0);
        let deep = max_scroll(33, 3);
        assert_eq!(
            item_at(240, H as i32 - 40, deep, 33, 3),
            Some(Item::Add),
            "the ADD row is reachable"
        );
    }
}
