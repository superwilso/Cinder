//! The Audiobooks screen — one row per book (a folder under `LEARNING`), opened from the book
//! button in the Library's header.
//!
//! A row says which book it is and where the listener got to; tapping it plays from that place
//! (`Action::PlayBook`). That is the whole screen: no chapter list, no speed control. A book keeps
//! its own place, so two books by one author never disturb each other.
//!
//! Rendering is pure, like every screen here: the saved place arrives on the row
//! (`BookRow::{chapter, pos_ms, finished}`), filled in by the shell from its own file.

use crate::library::{scrollbar, LIST_BOTTOM};
use crate::model::Library;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{hline, sty};
use crate::{icons, Canvas};

pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
#[allow(non_snake_case)]
pub fn ROW_H() -> i32 {
    crate::scale::TRACK_ROW_H()
}

/// An author's heading is slimmer than a book row.
pub const HEAD_H: i32 = 34;
/// A series' heading, under its author's, is slimmer still.
pub const SERIES_H: i32 = 30;
/// How far a book sits in under its author's heading; a series heading sits in by this much, and
/// its books by twice it.
const INDENT: f32 = 18.0;

/// One line of the list: an author's heading, a series' heading, or a book (an index into
/// `Library::books`; a heading carries the index of the first book it covers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Author(usize),
    Series(usize),
    Book(usize),
}

impl Line {
    fn height(self) -> i32 {
        match self {
            Line::Author(_) => HEAD_H,
            Line::Series(_) => SERIES_H,
            Line::Book(_) => ROW_H(),
        }
    }
}

/// The list top to bottom. `Library::books` arrives sorted by author and, under an author, by
/// series (`books::order`), so a heading goes in wherever the author changes and a series heading
/// wherever the series does; a book with no author folder gets no author heading and sits at the
/// top, and a book in no series gets no series heading.
pub fn lines(lib: &Library) -> Vec<Line> {
    let mut out = Vec::with_capacity(lib.books.len() * 2);
    let mut prev: Option<&crate::model::BookRow> = None;
    for (i, b) in lib.books.iter().enumerate() {
        let new_author = prev.map_or(true, |p| !p.author.eq_ignore_ascii_case(&b.author));
        if new_author && !b.author.is_empty() {
            out.push(Line::Author(i));
        }
        let new_series =
            new_author || prev.map_or(true, |p| !p.series.eq_ignore_ascii_case(&b.series));
        if new_series && !b.series.is_empty() {
            out.push(Line::Series(i));
        }
        out.push(Line::Book(i));
        prev = Some(b);
    }
    out
}

/// How far in a book's row sits: under its author, under its series, or flush.
fn indent_of(b: &crate::model::BookRow) -> f32 {
    let mut n = 0.0;
    if !b.author.is_empty() {
        n += INDENT;
    }
    if !b.series.is_empty() {
        n += INDENT;
    }
    n
}

pub fn content_h(lib: &Library) -> i32 {
    lines(lib).iter().map(|l| l.height()).sum()
}

pub fn max_scroll_px(lib: &Library) -> i32 {
    (content_h(lib) - (LIST_BOTTOM - TOP)).max(0)
}

/// Where book `r`'s row starts on screen.
pub fn row_top(lib: &Library, r: usize, scroll: i32) -> i32 {
    let mut y = TOP - scroll;
    for l in lines(lib) {
        if l == Line::Book(r) {
            break;
        }
        y += l.height();
    }
    y
}

/// The book under a tap at `y`, if any. A heading is not a target.
pub fn row_at(lib: &Library, y: i32, scroll: i32) -> Option<usize> {
    if !(TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let mut top = TOP - scroll.max(0);
    for l in lines(lib) {
        if y < top + l.height() {
            return match l {
                Line::Book(i) => Some(i),
                Line::Author(_) | Line::Series(_) => None,
            };
        }
        top += l.height();
    }
    None
}

/// The header's second line.
pub fn subtitle(lib: &Library) -> String {
    match lib.books.len() {
        1 => crate::i18n::text("1 BOOK").to_string(),
        n => crate::ui_format!("{n} BOOKS", n = n),
    }
}

fn title_style(t: &Theme) -> TextStyle {
    sty(Family::Sans, Weight::SemiBold, 18.0, t.ink, 0.0)
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    scroll_px: i32,
    sbar_active: bool,
) {
    let scroll = scroll_px.clamp(0, max_scroll_px(lib));
    c.fill(t.bg);
    let sub = subtitle(lib);
    let y0 = crate::chrome::header(
        c,
        t,
        f,
        crate::i18n::text("Audiobooks"),
        (!lib.books.is_empty()).then_some(&*sub),
    );
    c.set_clip_y(y0, LIST_BOTTOM);

    if lib.books.is_empty() {
        let st = sty(Family::Sans, Weight::Regular, crate::scale::ROW, t.dim, 0.0);
        let hint = sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0);
        text::draw(
            c,
            f,
            22.0,
            (TOP + 40) as f32,
            crate::i18n::text("No audiobooks yet."),
            &st,
        );
        text::draw(
            c,
            f,
            22.0,
            (TOP + 66) as f32,
            &crate::widgets::fit(
                f,
                crate::i18n::text("Put each book in its own folder inside LEARNING."),
                &hint,
                436.0,
            ),
            &hint,
        );
        c.clear_clip();
        return;
    }

    let mut y = TOP - scroll;
    for l in lines(lib) {
        let h = l.height();
        if y >= LIST_BOTTOM {
            break;
        }
        if y + h <= TOP {
            y += h;
            continue;
        }
        match l {
            Line::Author(i) => {
                let hs = sty(Family::Sans, Weight::SemiBold, 14.0, t.dim, 0.0);
                let name = crate::widgets::fit(f, &lib.books[i].author, &hs, 420.0);
                text::draw(c, f, 22.0, (y + h / 2 + 5) as f32, &name, &hs);
                hline(c, y + h - 1, t.line);
            }
            Line::Series(i) => {
                let b = &lib.books[i];
                let hs = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
                let x = if b.author.is_empty() { 0.0 } else { INDENT };
                let name = crate::widgets::fit(f, &b.series, &hs, 400.0 - x);
                text::draw(c, f, 22.0 + x, (y + h / 2 + 5) as f32, &name, &hs);
            }
            Line::Book(i) => {
                let b = &lib.books[i];
                let x = indent_of(b);
                let cy = y + ROW_H() / 2;
                // A book part-way through is lit; one finished or not started is quiet.
                let lit = b.in_progress();
                icons::book(
                    c,
                    34.0 + x,
                    cy as f32,
                    22.0,
                    if lit { t.acc } else { t.dim },
                );
                let ts = title_style(t);
                text::draw(
                    c,
                    f,
                    62.0 + x,
                    (cy + 1) as f32,
                    &crate::widgets::fit(f, &b.list_title(), &ts, 330.0 - x),
                    &ts,
                );
                let ps = sty(
                    Family::Mono,
                    Weight::Regular,
                    11.0,
                    if lit { t.acc } else { t.faint },
                    0.12,
                );
                text::draw(
                    c,
                    f,
                    62.0 + x,
                    (cy + 19) as f32,
                    &crate::widgets::fit(f, &b.progress_label(), &ps, 380.0 - x),
                    &ps,
                );
                hline(c, y + ROW_H() - 1, t.line);
            }
        }
        y += h;
    }
    c.clear_clip();
    scrollbar(c, t, TOP, LIST_BOTTOM, scroll, content_h(lib), sbar_active);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BookRow;

    fn lib(n: usize) -> Library {
        Library {
            books: (0..n)
                .map(|i| BookRow {
                    title: format!("Book {i}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_row_a_tap_lands_on_is_the_row_that_was_drawn_there() {
        let l = lib(20);
        for scroll in [0, 7, ROW_H(), ROW_H() * 3 + 5] {
            for r in 0..l.books.len() {
                let y = row_top(&l, r, scroll) + ROW_H() / 2;
                if (TOP..LIST_BOTTOM).contains(&y) {
                    assert_eq!(row_at(&l, y, scroll), Some(r), "row {r} at scroll {scroll}");
                }
            }
        }
        assert_eq!(row_at(&l, TOP - 1, 0), None, "above the list");
        assert_eq!(
            row_at(&l, LIST_BOTTOM, 0),
            None,
            "under the list, where the bar is"
        );
        let short = lib(2);
        assert_eq!(
            row_at(&short, row_top(&short, 1, 0) + ROW_H() + 5, 0),
            None,
            "past the last book"
        );
    }

    #[test]
    fn every_book_can_be_scrolled_to() {
        let l = lib(20);
        let max = max_scroll_px(&l);
        assert!(max > 0);
        let last = l.books.len() - 1;
        assert!(row_top(&l, last, max) + ROW_H() <= LIST_BOTTOM);
        assert_eq!(max_scroll_px(&lib(2)), 0, "a short list does not scroll");
    }

    #[test]
    fn the_subtitle_counts_books() {
        assert_eq!(subtitle(&lib(1)), "1 BOOK");
        assert_eq!(subtitle(&lib(12)), "12 BOOKS");
    }

    fn by(author: &str, title: &str) -> crate::model::BookRow {
        BookRow {
            author: author.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    fn grouped() -> Library {
        Library {
            books: vec![
                by("Author 1", "Book 1"),
                by("Author 1", "Book 2"),
                by("Author 2", "Book 3"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn each_author_is_a_heading_over_their_books() {
        assert_eq!(
            lines(&grouped()),
            vec![
                Line::Author(0),
                Line::Book(0),
                Line::Book(1),
                Line::Author(2),
                Line::Book(2),
            ]
        );
    }

    #[test]
    fn a_book_with_no_author_folder_has_no_heading() {
        let l = Library {
            books: vec![by("", "Loose"), by("Zed", "Z1")],
            ..Default::default()
        };
        assert_eq!(
            lines(&l),
            vec![Line::Book(0), Line::Author(1), Line::Book(1)]
        );
    }

    #[test]
    fn a_tap_on_a_heading_plays_nothing_and_on_a_book_plays_that_book() {
        let l = grouped();
        assert_eq!(row_at(&l, TOP + HEAD_H / 2, 0), None);
        for r in 0..3 {
            let y = row_top(&l, r, 0) + ROW_H() / 2;
            assert_eq!(row_at(&l, y, 0), Some(r));
        }
        assert_eq!(row_top(&l, 0, 0), TOP + HEAD_H);
        assert_eq!(row_top(&l, 2, 0), TOP + HEAD_H + 2 * ROW_H() + HEAD_H);
        assert_eq!(content_h(&l), 2 * HEAD_H + 3 * ROW_H());
    }

    fn in_series(author: &str, series: &str, part: &str, title: &str) -> crate::model::BookRow {
        BookRow {
            series: series.into(),
            series_part: part.into(),
            ..by(author, title)
        }
    }

    fn serial() -> Library {
        Library {
            books: vec![
                in_series("Au", "TKKG", "1", "Erste"),
                in_series("Au", "TKKG", "2", "Zweite"),
                in_series("Au", "Other", "1", "Solo"),
                by("Au", "Standalone"),
                in_series("Zed", "TKKG", "3", "Dritte"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn a_series_is_a_heading_between_the_author_and_its_books() {
        assert_eq!(
            lines(&serial()),
            vec![
                Line::Author(0),
                Line::Series(0),
                Line::Book(0),
                Line::Book(1),
                Line::Series(2),
                Line::Book(2),
                Line::Book(3),
                Line::Author(4),
                Line::Series(4),
                Line::Book(4),
            ],
            "the same series under another author is a new heading"
        );
    }

    #[test]
    fn a_series_without_an_author_folder_still_has_its_heading() {
        let l = Library {
            books: vec![
                in_series("", "S", "1", "One"),
                in_series("", "S", "2", "Two"),
            ],
            ..Default::default()
        };
        assert_eq!(
            lines(&l),
            vec![Line::Series(0), Line::Book(0), Line::Book(1)]
        );
    }

    #[test]
    fn a_numbered_book_shows_its_number_and_an_unnumbered_one_does_not() {
        assert_eq!(
            in_series("Au", "TKKG", "12", "Title").list_title(),
            "12. Title"
        );
        assert_eq!(in_series("Au", "TKKG", "", "Title").list_title(), "Title");
        assert_eq!(by("Au", "Title").list_title(), "Title");
    }

    #[test]
    fn taps_still_land_on_books_when_series_headings_shift_the_rows() {
        let l = serial();
        assert_eq!(
            row_at(&l, TOP + HEAD_H + SERIES_H / 2, 0),
            None,
            "series heading"
        );
        for r in 0..l.books.len() {
            let y = row_top(&l, r, 0) + ROW_H() / 2;
            assert_eq!(row_at(&l, y, 0), Some(r), "book {r}");
        }
        assert_eq!(
            content_h(&l),
            2 * HEAD_H + 3 * SERIES_H + 5 * ROW_H(),
            "two authors, three series headings, five books"
        );
    }
}
