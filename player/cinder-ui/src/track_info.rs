//! Track information — Sony's "Detailed Information", reached by tapping the title/artist/codec
//! block on Now Playing.
//!
//! The rows are a plain `(label, value)` list handed over by the shell rather than a typed struct.
//! Everything on this screen comes from the library DB, cinder-ui has no DB, and the fields worth
//! showing differ per file (a FLAC has a bit depth, a lossless-less MP3 does not). A typed view
//! would mean a wide struct of `Option<&str>` here, a matching builder there, and a new field in
//! three crates every time the DB learns one more thing. A list says exactly as much as is known.
//!
//! Values that do not fit — a long path, most of all — WRAP rather than being truncated. A path is
//! the one value on this screen you might actually need to read character by character, and
//! `widgets::fit` would end it in an ellipsis precisely where the useful part lives.

use crate::library::{scrollbar, LIST_BOTTOM};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{hline, sty};
use crate::Canvas;

/// Screen-y of the first row (under the header).
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
/// Bottom of the scrollable area. The SAME bound `scrollbar` uses for its track, so the thumb
/// describes the window the rows are actually clipped to.
pub const BOTTOM: i32 = LIST_BOTTOM;
/// Label column x, value column x, and the right edge values wrap against.
const LABEL_X: f32 = 22.0;
const VALUE_X: f32 = 176.0;
const RIGHT: f32 = 458.0;
/// Vertical rhythm: a one-line row, and how much each extra wrapped line adds.
const ROW_H: i32 = 44;
const WRAP_H: i32 = 20;

fn label_style(t: &Theme) -> TextStyle {
    sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18)
}
fn value_style(t: &Theme) -> TextStyle {
    sty(Family::Sans, Weight::Regular, 17.0, t.ink, 0.0)
}

/// Split `v` into lines that each fit the value column. Greedy by word, then by character for a
/// single "word" longer than the column — which is the normal case for a file path.
fn wrap(f: &FontSet, v: &str, st: &TextStyle, w: f32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    let fits = |s: &str| text::measure(f, s, st) <= w;
    for word in v.split_inclusive(['/', ' ']) {
        let mut word = word;
        // A single fragment wider than the column: break it at the last character that fits,
        // repeatedly. Without this the loop would emit one over-wide line and draw past the edge.
        while !fits(word) && word.chars().count() > 1 {
            let mut cut = 0;
            let mut acc = String::new();
            for (i, ch) in word.char_indices() {
                acc.push(ch);
                if !fits(&acc) {
                    break;
                }
                cut = i + ch.len_utf8();
            }
            if cut == 0 {
                break; // one character does not fit — draw it anyway rather than loop forever
            }
            let (head, tail) = word.split_at(cut);
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            out.push(head.to_string());
            word = tail;
        }
        let candidate = format!("{line}{word}");
        if line.is_empty() || fits(&candidate) {
            line = candidate;
        } else {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Height of one row, given how many lines its value wrapped to.
fn row_h(lines: usize) -> i32 {
    ROW_H + (lines.saturating_sub(1) as i32) * WRAP_H
}

/// Total scrollable height. Shares `wrap` with the render, so the scrollbar and the last row agree
/// about where the content ends — the two used to be independent guesses on the album screen and
/// that is exactly how a list ends up scrolling past its own bottom.
pub fn content_h(f: &FontSet, t: &Theme, rows: &[(String, String)]) -> i32 {
    let st = value_style(t);
    rows.iter().map(|(_, v)| row_h(wrap(f, v, &st, RIGHT - VALUE_X).len())).sum()
}

pub fn max_scroll_px(f: &FontSet, t: &Theme, rows: &[(String, String)]) -> i32 {
    (content_h(f, t, rows) - (BOTTOM - TOP)).max(0)
}

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    rows: &[(String, String)],
    scroll_px: i32,
    sbar_active: bool,
) {
    let scroll = scroll_px.clamp(0, max_scroll_px(f, t, rows));
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Track information", None);
    c.set_clip_y(y0, BOTTOM);

    if rows.is_empty() {
        // Nothing playing, or a URI the library does not know. Say so rather than drawing an
        // empty frame that reads as a failed load.
        let st = sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
        text::draw(c, f, LABEL_X, (TOP + 40) as f32, "Nothing is playing.", &st);
        c.clear_clip();
        return;
    }

    let (ls, vs) = (label_style(t), value_style(t));
    let mut y = TOP - scroll;
    for (label, value) in rows {
        let lines = wrap(f, value, &vs, RIGHT - VALUE_X);
        let h = row_h(lines.len());
        // Skip rows entirely above the window; stop once past the bottom. Cheap, and it keeps a
        // long path list from measuring text it will never draw.
        if y + h > y0 {
            if y >= BOTTOM {
                break;
            }
            text::draw(c, f, LABEL_X, (y + 27) as f32, &label.to_uppercase(), &ls);
            for (i, line) in lines.iter().enumerate() {
                text::draw(c, f, VALUE_X, (y + 27 + i as i32 * WRAP_H) as f32, line, &vs);
            }
            hline(c, y + h - 1, t.line);
        }
        y += h;
    }
    c.clear_clip();
    scrollbar(c, t, TOP, BOTTOM, scroll, content_h(f, t, rows), sbar_active);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> FontSet {
        FontSet::load()
    }

    /// A path is the one value here you might have to read character by character, so it wraps
    /// instead of being cut — and every produced line has to actually fit the column, including
    /// the ones split mid-word.
    #[test]
    fn long_values_wrap_inside_the_value_column() {
        // Measures text, so it shares the crate-wide UI-scale lock — see text::scale_guard. Without
        // it this passes alone and fails in a full run, because another test's 140% is still live.
        let _scale = crate::text::scale_guard();
        let f = fonts();
        let t = Theme::day();
        let vs = value_style(&t);
        let w = RIGHT - VALUE_X;
        let path = "/contents/Music/Some Very Long Artist Name/An Album With A Long Title/\
                    01 - A Track Whose Filename Nobody Would Choose.flac";
        let lines = wrap(&f, path, &vs, w);
        assert!(lines.len() > 1, "a full path does not fit one line");
        for l in &lines {
            assert!(text::measure(&f, l, &vs) <= w, "line ran past the column: {l:?}");
        }
        // Nothing is lost: the pieces still spell the path.
        assert_eq!(lines.concat(), path);
    }

    /// A single unbreakable run longer than the column still has to terminate, and still has to
    /// stay inside it. This is the case the greedy word loop cannot handle on its own.
    #[test]
    fn an_unbreakable_run_is_split_by_character() {
        let _scale = crate::text::scale_guard();
        let f = fonts();
        let t = Theme::day();
        let vs = value_style(&t);
        let w = RIGHT - VALUE_X;
        let run = "A".repeat(200);
        let lines = wrap(&f, &run, &vs, w);
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(text::measure(&f, l, &vs) <= w);
        }
        assert_eq!(lines.concat(), run);
    }

    /// The scrollbar's total and the last row's bottom come from the same measurement, so the
    /// screen cannot scroll past its own content.
    #[test]
    fn content_height_matches_the_rows_actually_drawn() {
        let _scale = crate::text::scale_guard();
        let f = fonts();
        let t = Theme::day();
        let rows: Vec<(String, String)> = vec![
            ("Title".into(), "Short".into()),
            ("Path".into(), "/contents/".to_string() + &"deep/".repeat(20) + "file.flac"),
        ];
        let vs = value_style(&t);
        let by_hand: i32 = rows
            .iter()
            .map(|(_, v)| row_h(wrap(&f, v, &vs, RIGHT - VALUE_X).len()))
            .sum();
        assert_eq!(content_h(&f, &t, &rows), by_hand);
        assert!(max_scroll_px(&f, &t, &rows) >= 0);
    }
}
