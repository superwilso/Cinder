//! Settings ▸ Language — the languages the installer put on this player, each in its own name and
//! script, the one in use marked the way the palette picker marks its palette. A list rather than
//! a value the Settings row cycles through, so forty languages are a scroll, not forty taps.
//!
//! The whole screen draws inside `text::with_fixed_ui_glyphs`: every name is in its own script,
//! whatever the UI language is, and must come from the bundled fonts.

use crate::canvas::H;
use crate::i18n::Language;
use crate::kit::{self, Row};
use crate::text::FontSet;
use crate::theme::Theme;
use crate::Canvas;

/// Where the first row starts: right under the header.
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The language under `y` at `scroll`, as an index into the list shown.
pub fn row_at(y: i32, scroll: i32, n: usize) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let i = ((y - TOP + scroll) / kit::ROW_H) as usize;
    (i < n).then_some(i)
}

/// How far a list of `n` scrolls; 0 while it fits.
pub fn max_scroll(n: usize) -> i32 {
    (TOP + n as i32 * kit::ROW_H + 8 - H as i32).max(0)
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    languages: &[Language],
    current: Language,
    scroll: i32,
) {
    c.fill(t.bg);
    crate::text::with_fixed_ui_glyphs(|| {
        let title = crate::settings::language_title(languages);
        crate::chrome::header(c, t, f, &title, None);
        c.set_clip_y(TOP, H as i32);
        for (i, l) in languages.iter().enumerate() {
            let y = TOP + i as i32 * kit::ROW_H - scroll;
            if y + kit::ROW_H < TOP || y >= H as i32 {
                continue;
            }
            // The English name under it, for whoever installed the player for someone else.
            let sub = if l.english() == l.name() {
                ""
            } else {
                l.english()
            };
            kit::row(
                c,
                t,
                f,
                y,
                kit::ROW_H,
                &Row::new(l.name()).sub(sub).sel(*l == current),
            );
        }
        c.clear_clip();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_hit_where_they_are_drawn_and_long_lists_scroll() {
        assert_eq!(row_at(TOP - 1, 0, 2), None);
        assert_eq!(row_at(TOP + kit::ROW_H / 2, 0, 2), Some(0));
        assert_eq!(row_at(TOP + kit::ROW_H * 3 / 2, 0, 2), Some(1));
        assert_eq!(row_at(TOP + kit::ROW_H * 5 / 2, 0, 2), None);
        assert_eq!(max_scroll(2), 0, "two languages fit");
        let forty = max_scroll(40);
        assert!(forty > 0);
        let last = H as i32 - 8 - kit::ROW_H / 2;
        assert_eq!(
            row_at(last, forty, 40),
            Some(39),
            "the last of forty is reachable"
        );
    }
}
