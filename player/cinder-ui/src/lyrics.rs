//! Lyrics — the words of the playing song, reached from the Lyrics row on Track information.
//!
//! Synced lyrics (a `.lrc` with timestamps) follow the song: the line being sung is drawn in the
//! accent colour and the view keeps it a third of the way down the page, until you scroll it
//! yourself. Plain lyrics are simply a page that scrolls.
//!
//! The current line differs from the others only in COLOUR. A bold current line would be wider, a
//! wider line can wrap differently, and a line that rewraps as it becomes current makes every line
//! below it jump.
//!
//! The words are user text, so they go through the script-gated font fallback like any title does.
//! Everything else on this screen is ASCII and stays inside the bundled faces.

use crate::library::{scrollbar, LIST_BOTTOM};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::track_info::wrap;
use crate::widgets::sty;
use crate::Canvas;

/// One line of lyrics. `at_ms` is when it starts, for synced lyrics; `None` for plain text.
/// An empty `text` in synced lyrics is a deliberate gap — an instrumental break that clears the
/// highlight — and is drawn as a short spacer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub at_ms: Option<u32>,
    pub text: String,
}

/// A song's lyrics, in display order. Synced lyrics are sorted by `at_ms`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
}

impl Lyrics {
    pub fn is_synced(&self) -> bool {
        self.lines.iter().any(|l| l.at_ms.is_some())
    }

    /// The line being sung at `pos_ms`: the last one that has started. `None` before the first
    /// line starts, and always for plain lyrics, which have no clock to follow.
    pub fn current(&self, pos_ms: u32) -> Option<usize> {
        if !self.is_synced() {
            return None;
        }
        let started = self.lines.partition_point(|l| l.at_ms.map_or(true, |t| t <= pos_ms));
        started.checked_sub(1)
    }

    /// The value shown in the Lyrics row on Track information.
    pub fn summary(&self) -> String {
        let n = self.lines.iter().filter(|l| !l.text.trim().is_empty()).count();
        let unit = if n == 1 { "line" } else { "lines" };
        if self.is_synced() {
            format!("Synced, {n} {unit}")
        } else {
            format!("{n} {unit}")
        }
    }
}

/// Screen-y of the scrollable area: under the header, down to the list bottom.
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
pub const BOTTOM: i32 = LIST_BOTTOM;
const X: f32 = 22.0;
const RIGHT: f32 = 458.0;
/// One wrapped line, the space after a lyric line, and the height of a deliberate gap.
const LINE_H: i32 = 26;
const GAP: i32 = 12;
const BLANK_H: i32 = 18;
/// Breathing room above the first line and below the last, so neither sits against an edge.
const PAD_TOP: i32 = 16;
const PAD_BOTTOM: i32 = 24;
/// Baseline offset of the first wrapped line inside its row.
const BASELINE: i32 = 19;

fn base_style(t: &Theme) -> TextStyle {
    sty(Family::Sans, Weight::Regular, 18.0, t.ink, 0.0)
}

/// Each line's height, in display order. Render and scroll both come from this, so the scrollbar
/// and the follow position agree with what is drawn.
pub fn heights(f: &FontSet, t: &Theme, lyr: &Lyrics) -> Vec<i32> {
    let st = base_style(t);
    lyr.lines
        .iter()
        .map(|l| {
            if l.text.trim().is_empty() {
                BLANK_H
            } else {
                wrap(f, &l.text, &st, RIGHT - X).len() as i32 * LINE_H + GAP
            }
        })
        .collect()
}

pub fn content_h(heights: &[i32]) -> i32 {
    PAD_TOP + heights.iter().sum::<i32>() + PAD_BOTTOM
}

pub fn max_scroll_px(heights: &[i32]) -> i32 {
    (content_h(heights) - (BOTTOM - TOP)).max(0)
}

/// The scroll that puts line `cur` a third of the way down the page — high enough that the next
/// few lines are visible under it, which is the point of reading along.
pub fn follow_scroll(heights: &[i32], cur: usize) -> i32 {
    let top: i32 = heights.iter().take(cur).sum();
    (PAD_TOP + top - (BOTTOM - TOP) / 3).clamp(0, max_scroll_px(heights))
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lyr: Option<&Lyrics>,
    cur: Option<usize>,
    scroll_px: i32,
    sbar_active: bool,
) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Lyrics", None);
    let Some(lyr) = lyr.filter(|l| !l.lines.is_empty()) else {
        // Reached only if the file went away between the row being built and the tap. Say what
        // would fix it rather than drawing an empty page that reads as a failed load.
        let st = sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
        text::draw(c, f, X, (TOP + 40) as f32, "No lyrics for this song.", &st);
        let hint = sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0);
        text::draw(c, f, X, (TOP + 70) as f32, "Put a .lrc file with the same name", &hint);
        text::draw(c, f, X, (TOP + 92) as f32, "next to the song.", &hint);
        return;
    };

    let hs = heights(f, t, lyr);
    let scroll = scroll_px.clamp(0, max_scroll_px(&hs));
    let synced = lyr.is_synced();
    let (plain, ahead, now) = (
        base_style(t),
        TextStyle { color: t.dim, ..base_style(t) },
        TextStyle { color: t.acc, ..base_style(t) },
    );
    c.set_clip_y(y0, BOTTOM);
    let mut y = TOP + PAD_TOP - scroll;
    for (i, (line, h)) in lyr.lines.iter().zip(&hs).enumerate() {
        if y >= BOTTOM {
            break;
        }
        if y + h > y0 && !line.text.trim().is_empty() {
            let st = if !synced {
                &plain
            } else if Some(i) == cur {
                &now
            } else {
                &ahead
            };
            for (k, part) in wrap(f, &line.text, &plain, RIGHT - X).iter().enumerate() {
                text::draw(c, f, X, (y + BASELINE + k as i32 * LINE_H) as f32, part, st);
            }
        }
        y += h;
    }
    c.clear_clip();
    scrollbar(c, t, TOP, BOTTOM, scroll, content_h(&hs), sbar_active);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synced(stamps: &[u32]) -> Lyrics {
        Lyrics {
            lines: stamps
                .iter()
                .map(|&ms| Line { at_ms: Some(ms), text: format!("at {ms}") })
                .collect(),
        }
    }

    #[test]
    fn the_current_line_is_the_last_one_that_has_started() {
        let l = synced(&[500, 1000, 5000]);
        assert_eq!(l.current(0), None, "nothing has started yet");
        assert_eq!(l.current(500), Some(0), "a line is current from its own timestamp");
        assert_eq!(l.current(999), Some(0));
        assert_eq!(l.current(1000), Some(1));
        assert_eq!(l.current(u32::MAX), Some(2), "the last line stays current to the end");
    }

    #[test]
    fn plain_lyrics_have_no_current_line() {
        let l = Lyrics { lines: vec![Line { at_ms: None, text: "words".into() }] };
        assert_eq!(l.current(10_000), None);
        assert_eq!(l.summary(), "1 line");
    }

    #[test]
    fn the_summary_counts_words_not_gaps() {
        let mut l = synced(&[0, 1000]);
        l.lines.insert(1, Line { at_ms: Some(500), text: String::new() });
        assert_eq!(l.summary(), "Synced, 2 lines");
    }

    /// Following the song can never scroll past either end, whatever the line and however short
    /// the page — a follow position outside the range would fight the clamp in `render` and jitter.
    #[test]
    fn following_stays_inside_the_scroll_range() {
        for hs in [vec![], vec![38], vec![38; 3], vec![38, 64, 18, 90, 38].repeat(20)] {
            let max = max_scroll_px(&hs);
            for cur in 0..=hs.len() {
                let s = follow_scroll(&hs, cur);
                assert!((0..=max).contains(&s), "cur {cur} of {} -> {s}, max {max}", hs.len());
            }
        }
    }

    /// Heights come from the same wrap the render draws with, so a long line is taller and the
    /// scrollbar's total includes it.
    #[test]
    fn a_line_that_wraps_is_taller() {
        let _scale = crate::text::scale_guard();
        let f = FontSet::load();
        let t = Theme::day();
        let lyr = Lyrics {
            lines: vec![
                Line { at_ms: Some(0), text: "Short".into() },
                Line { at_ms: Some(1), text: "A much longer line of lyrics that cannot possibly fit on one row of this screen".into() },
                Line { at_ms: Some(2), text: String::new() },
            ],
        };
        let hs = heights(&f, &t, &lyr);
        assert_eq!(hs[0], LINE_H + GAP);
        assert!(hs[1] >= 2 * LINE_H + GAP, "the long line wrapped: {hs:?}");
        assert_eq!(hs[2], BLANK_H);
        assert_eq!(content_h(&hs), PAD_TOP + hs.iter().sum::<i32>() + PAD_BOTTOM);
    }
}
