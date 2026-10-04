//! The saved-view editor (design handoff 5c): a smart playlist's name, rules and sort.
//!
//!   header    Smart playlist                                 SAVE
//!   strip     38 SONGS MATCH
//!   row       Name                                    Late favourites ›
//!   RATING       Any · 2+ · 3+ · 4+ · 5
//!   LAST PLAYED  Any · Recent · Not lately · Never
//!   FORMAT       Any · FLAC · MP3 · M4A · Hi-Res
//!   SORT         Title · Plays · Played · Rating · Added
//!   row       Delete smart playlist                  (only when editing one that exists)
//!
//! The mock also has "show as" chips and a "Pin to the Library bar" switch. Both are the Library
//! view bar's, which is R3 and not built; a switch that pins to a bar that does not exist would be
//! a row that does nothing, so they wait for it (`PLAN_redesign_2026-09.md` Part G). The mock's
//! "+ add" for further rules is the same: the three rules here are the ones the data exists for.
//!
//! One page, no scroll: `section_top` is the single table the renderer and the hit tests share.

use crate::chrome::HEADER_BOTTOM;
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::views::{FormatRule, Played, SavedView, ViewSort, RATINGS, RATING_LABELS};
use crate::Canvas;

/// The chip sections, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Rating,
    Played,
    Format,
    Sort,
}

pub const SECTIONS: [Section; 4] = [Section::Rating, Section::Played, Section::Format, Section::Sort];

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Rating => "RATING",
            Section::Played => "LAST PLAYED",
            Section::Format => "FORMAT",
            Section::Sort => "SORT",
        }
    }
    pub fn labels(self) -> &'static [&'static str] {
        match self {
            Section::Rating => &RATING_LABELS,
            Section::Played => &Played::LABELS,
            Section::Format => &FormatRule::LABELS,
            Section::Sort => &ViewSort::LABELS,
        }
    }
    /// Which chip `v` has selected in this section.
    pub fn selected(self, v: &SavedView) -> usize {
        match self {
            Section::Rating => RATINGS.iter().position(|r| *r == v.min_rating).unwrap_or(0),
            Section::Played => Played::ALL.iter().position(|p| *p == v.played).unwrap_or(0),
            Section::Format => FormatRule::ALL.iter().position(|p| *p == v.format).unwrap_or(0),
            Section::Sort => ViewSort::ALL.iter().position(|p| *p == v.sort).unwrap_or(0),
        }
    }
    /// Select chip `i` in `v`.
    pub fn select(self, v: &mut SavedView, i: usize) {
        match self {
            Section::Rating => v.min_rating = RATINGS.get(i).copied().unwrap_or(0),
            Section::Played => v.played = Played::ALL.get(i).copied().unwrap_or_default(),
            Section::Format => v.format = FormatRule::ALL.get(i).copied().unwrap_or_default(),
            Section::Sort => v.sort = ViewSort::ALL.get(i).copied().unwrap_or_default(),
        }
    }
}

/// The name row, under the strip.
pub const NAME_Y: i32 = HEADER_BOTTOM + kit::STRIP_H;
/// A chip section: its label, the chips, and a little air.
const SECTION_H: i32 = kit::SECTION_H + kit::CHIP_H + 10;

/// Top of section `i`'s label.
pub fn section_top(i: usize) -> i32 {
    NAME_Y + kit::ROW_H + i as i32 * SECTION_H
}

fn chips_top(i: usize) -> i32 {
    section_top(i) + kit::SECTION_H
}

/// The Delete row, below the last section.
pub const DELETE_Y: i32 = NAME_Y + kit::ROW_H + 4 * SECTION_H + 6;

pub fn hit_name(y: i32) -> bool {
    (NAME_Y..NAME_Y + kit::ROW_H).contains(&y)
}

pub fn hit_delete(y: i32) -> bool {
    (DELETE_Y..DELETE_Y + kit::ROW_H).contains(&y)
}

/// Which section's chip is under `(x, y)`.
pub fn chip_at(x: i32, y: i32) -> Option<(Section, usize)> {
    SECTIONS
        .iter()
        .enumerate()
        .find_map(|(i, s)| kit::chip_at(s.labels().len(), chips_top(i), x, y).map(|c| (*s, c)))
}

/// What the editor shows.
pub struct ViewEditView<'a> {
    pub draft: &'a SavedView,
    /// How many songs the draft matches right now.
    pub matches: usize,
    /// Editing a view that exists (so it can be deleted), rather than making a new one.
    pub existing: bool,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &ViewEditView) {
    c.fill(t.bg);
    crate::chrome::header_action(c, t, f, "Smart playlist", "SAVE");
    let line = match v.matches {
        1 => "1 SONG MATCHES".to_string(),
        n => format!("{n} SONGS MATCH"),
    };
    kit::strip(c, t, f, HEADER_BOTTOM, &line);
    let name = if v.draft.name.trim().is_empty() { "Choose a name" } else { v.draft.name.as_str() };
    kit::row(c, t, f, NAME_Y, kit::ROW_H, &Row::new("Name").trail(Trail::Open(name)));
    for (i, s) in SECTIONS.iter().enumerate() {
        kit::section_label(c, t, f, section_top(i), s.label(), None);
        kit::chips(c, t, f, chips_top(i), s.labels(), Some(s.selected(v.draft)));
    }
    if v.existing {
        kit::row(c, t, f, DELETE_Y, kit::ROW_H, &Row::new("Delete smart playlist").sub("The songs stay on the player"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page fits above the Shelf's swipe zone, and every chip is hit where it is drawn.
    #[test]
    fn the_page_fits_and_every_chip_hits_itself() {
        assert!(DELETE_Y + kit::ROW_H <= crate::canvas::H as i32 - 28, "Delete ends at {}", DELETE_Y + kit::ROW_H);
        for (i, s) in SECTIONS.iter().enumerate() {
            let n = s.labels().len();
            for c in 0..n {
                let (x, w) = kit::chip_span(c, n);
                assert_eq!(chip_at(x + w / 2, chips_top(i) + kit::CHIP_H / 2), Some((*s, c)));
            }
        }
        assert!(!hit_name(section_top(0)) && hit_name(NAME_Y + 1));
        assert_eq!(chip_at(240, section_top(0) + 2), None, "the label band is not a chip");
    }

    /// Selecting a chip and reading it back agree, in every section.
    #[test]
    fn select_and_selected_agree() {
        let mut v = SavedView::new("x");
        for s in SECTIONS {
            for i in 0..s.labels().len() {
                s.select(&mut v, i);
                assert_eq!(s.selected(&v), i, "{s:?} chip {i}");
            }
        }
    }
}
