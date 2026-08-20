//! On-screen keyboard — the device's only text input.
//!
//! There is no d-pad and no hardware keyboard on an NW-A55: the transport buttons and the
//! touchscreen are the whole input surface (`feedback_nwa55_input_model`). Naming a playlist is
//! the first thing in Cinder that needs free text, so this is a touch keyboard and nothing else —
//! no cursor movement, no selection, no clipboard. Text is appended and backspaced, which is what
//! naming something needs and is the most a 480 px panel can offer honestly.
//!
//! GEOMETRY IS SHARED. `key_rect` is the single source for both the render and the hit test — the
//! one class of bug this UI has repeatedly shipped is a control drawn in one place and tested in
//! another (see the EQ preset pills and the Up Next rows in `AUDIT_2026-07-26.md` §F6b).
//!
//! Keys are 44x62 with 4 px gaps, which is a bigger target than any list row in the app. That is
//! deliberate: a mistyped letter costs a backspace and a retry, and the audit that resized the
//! Shelf found that anything under ~50 px on this panel is fiddly under a thumb.

use crate::canvas::W;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, stroke_rect, sty};
use crate::Canvas;

/// What a tap on the keyboard means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    /// Caps for the next character (sticky until used, like every phone keyboard).
    Shift,
    /// Letters ⇄ numbers/symbols.
    Page,
    Space,
    Backspace,
    /// Commit. The header's Back arrow cancels — the same "leave without applying" this UI uses
    /// everywhere else, so there is no second cancel button competing with Done.
    Done,
}

pub const ROWS: usize = 4;
// The grid is sized to fit INSIDE the 22 px gutter every other screen keeps clear: 10 keys of 40
// with 4 px gaps is exactly 436, and the shared text helpers clamp anything drawn nearer the glass
// than that — which is why the first render came out with "…" where Q and P should be.
pub const KEY_W: i32 = 40;
pub const KEY_H: i32 = 62;
pub const GAP: i32 = 4;
/// Bottom row keys are wider; these are their widths in the same grid.
const MODE_W: i32 = 76;
const SPACE_W: i32 = 220;
const DONE_W: i32 = 130;
const WIDE_W: i32 = 60; // shift / backspace

/// The keyboard block is bottom-anchored: the field and its hint sit above it, and the thumb
/// reaches the bottom of a 800 px panel far more comfortably than the middle.
pub const KEYS_TOP: i32 = 468;
pub const TEXT_Y: i32 = 116;
pub const TEXT_H: i32 = 72;
pub const TEXT_X: i32 = 22;
pub const TEXT_W: i32 = W as i32 - 44;

/// Letter rows, and the symbol rows for page 1. Row 2's first and last slots are always
/// shift/mode and backspace, so only the middle characters are listed.
const LETTERS: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
const SYMBOLS: [&str; 3] = ["1234567890", "-_'&()!?,", ".:;/@#\""];

/// How many keys row `r` has on `page` (row 2 counts its two wide keys, row 3 its three).
fn row_len(page: u8, row: usize) -> usize {
    match row {
        0 | 1 => chars_of(page, row).chars().count(),
        2 => chars_of(page, 2).chars().count() + 2,
        _ => 3,
    }
}

fn chars_of(page: u8, row: usize) -> &'static str {
    if page == 0 { LETTERS[row.min(2)] } else { SYMBOLS[row.min(2)] }
}

fn row_y(row: usize) -> i32 {
    KEYS_TOP + row as i32 * (KEY_H + GAP)
}

/// Total width of a row, so it can be centred rather than left-aligned — an off-centre row makes
/// the outer keys unreachable on one side.
fn row_width(page: u8, row: usize) -> i32 {
    let n = row_len(page, row) as i32;
    match row {
        3 => MODE_W + SPACE_W + DONE_W + 2 * GAP,
        2 => 2 * WIDE_W + (n - 2) * KEY_W + (n - 1) * GAP,
        _ => n * KEY_W + (n - 1) * GAP,
    }
}

/// The rect of one key. THE single source of geometry for render and hit alike.
pub fn key_rect(page: u8, row: usize, col: usize) -> Option<(i32, i32, i32, i32)> {
    if row >= ROWS || col >= row_len(page, row) {
        return None;
    }
    let y = row_y(row);
    let mut x = (W as i32 - row_width(page, row)) / 2;
    for index in 0..col {
        x += key_w(page, row, index) + GAP;
    }
    Some((x, y, key_w(page, row, col), KEY_H))
}

fn key_w(page: u8, row: usize, col: usize) -> i32 {
    let last = row_len(page, row) - 1;
    match row {
        2 if col == 0 || col == last => WIDE_W,
        3 => match col {
            0 => MODE_W,
            1 => SPACE_W,
            _ => DONE_W,
        },
        _ => KEY_W,
    }
}

/// What key sits at (row, col) on `page`.
pub fn key_at(page: u8, row: usize, col: usize) -> Option<Key> {
    let last = row_len(page, row).checked_sub(1)?;
    if col > last {
        return None;
    }
    Some(match row {
        0 | 1 => Key::Char(chars_of(page, row).chars().nth(col)?),
        2 => {
            if col == 0 {
                if page == 0 { Key::Shift } else { Key::Char('+') }
            } else if col == last {
                Key::Backspace
            } else {
                Key::Char(chars_of(page, 2).chars().nth(col - 1)?)
            }
        }
        _ => match col {
            0 => Key::Page,
            1 => Key::Space,
            _ => Key::Done,
        },
    })
}

/// Which key a tap lands on. None for the gaps and everything above the block, so a stray tap on
/// the text field does nothing rather than typing whatever key is nearest.
pub fn hit(page: u8, x: i32, y: i32) -> Option<Key> {
    for row in 0..ROWS {
        for col in 0..row_len(page, row) {
            let (kx, ky, kw, kh) = key_rect(page, row, col)?;
            if (kx..kx + kw).contains(&x) && (ky..ky + kh).contains(&y) {
                return key_at(page, row, col);
            }
        }
    }
    None
}

/// Apply a key to the text being edited. Returns true when the caller should commit (Done).
///
/// Pure, and separate from the hit test, so the editing rules are testable without geometry.
pub fn apply(key: Key, text: &mut String, shift: &mut bool, page: &mut u8) -> bool {
    match key {
        Key::Char(ch) => {
            if text.chars().count() < MAX_LEN {
                if *shift {
                    text.extend(ch.to_uppercase());
                    *shift = false;
                } else {
                    text.push(ch);
                }
            }
        }
        Key::Space => {
            // No leading space, and never two in a row: the name is trimmed and collapsed when it
            // is saved anyway, so typing them only makes the field lie about what will be stored.
            if !text.is_empty() && !text.ends_with(' ') && text.chars().count() < MAX_LEN {
                text.push(' ');
            }
        }
        Key::Backspace => {
            text.pop();
        }
        Key::Shift => *shift = !*shift,
        Key::Page => {
            *page = if *page == 0 { 1 } else { 0 };
            *shift = false;
        }
        Key::Done => return !text.trim().is_empty(),
    }
    false
}

/// Longest name the editor accepts. The store trims to the same length; a row can show about 24
/// characters, so this is already generous.
pub const MAX_LEN: usize = 64;

/// Draw the keyboard screen.
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, value: &str,
              placeholder: &str, page: u8, shift: bool) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, title, None);
    debug_assert_eq!(y0, crate::chrome::HEADER_BOTTOM);

    // ── the field ────────────────────────────────────────────────────────────────
    fill_rect(c, TEXT_X, TEXT_Y, TEXT_W, TEXT_H, t.panel);
    stroke_rect(c, TEXT_X, TEXT_Y, TEXT_W, TEXT_H, t.acc, 2);
    let vs = sty(Family::Sans, Weight::SemiBold, 24.0, t.ink, 0.0);
    let baseline = (TEXT_Y + TEXT_H / 2 + 9) as f32;
    let inner = (TEXT_W - 40) as f32;
    if value.is_empty() {
        text::draw(c, f, (TEXT_X + 16) as f32, baseline, placeholder,
                   &sty(Family::Sans, Weight::Regular, 22.0, t.faint, 0.0));
        fill_rect(c, TEXT_X + 16, TEXT_Y + 18, 2, TEXT_H - 36, t.acc);
    } else {
        // Show the TAIL when the name outgrows the field — what you just typed is the part you
        // need to see, and the caret must stay visible or the field looks frozen.
        let shown = tail_that_fits(f, value, &vs, inner);
        let end = text::draw(c, f, (TEXT_X + 16) as f32, baseline, &shown, &vs);
        fill_rect(c, end as i32 + 3, TEXT_Y + 18, 2, TEXT_H - 36, t.acc);
    }
    center(c, f, 240.0, (TEXT_Y + TEXT_H + 26) as f32,
           if shift { "CAPS — NEXT LETTER IS A CAPITAL" } else { "TAP DONE TO SAVE · BACK TO CANCEL" },
           &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16));

    // ── the keys ─────────────────────────────────────────────────────────────────
    for row in 0..ROWS {
        for col in 0..row_len(page, row) {
            let Some((x, y, w, h)) = key_rect(page, row, col) else { continue };
            let Some(key) = key_at(page, row, col) else { continue };
            let (label, wide) = label_of(key, page, shift);
            let accent = matches!(key, Key::Done) || (matches!(key, Key::Shift) && shift);
            if accent {
                fill_rect(c, x, y, w, h, t.acc);
            } else if wide {
                fill_rect(c, x, y, w, h, t.panel);
            }
            stroke_rect(c, x, y, w, h, t.line, 1);
            let ink = if accent { t.acc_ink } else { t.ink };
            let size = if wide { 15.0 } else { 24.0 };
            let weight = if wide { Weight::SemiBold } else { Weight::Regular };
            center(c, f, (x + w / 2) as f32, (y + h / 2 + size as i32 / 3) as f32, &label,
                   &sty(Family::Sans, weight, size, ink, 0.0));
        }
    }
}

/// Label + whether it is a "wide" (word) key, which draws smaller and on a panel fill.
fn label_of(key: Key, page: u8, shift: bool) -> (String, bool) {
    match key {
        Key::Char(ch) => {
            let ch = if shift && page == 0 {
                ch.to_uppercase().next().unwrap_or(ch)
            } else {
                ch
            };
            (ch.to_string(), false)
        }
        Key::Shift => ("CAPS".to_string(), true),
        Key::Page => (if page == 0 { "123" } else { "ABC" }.to_string(), true),
        Key::Space => ("SPACE".to_string(), true),
        Key::Backspace => ("DEL".to_string(), true),
        Key::Done => ("DONE".to_string(), true),
    }
}

/// The longest suffix of `s` that fits in `avail` px — the field scrolls with the caret.
fn tail_that_fits(f: &FontSet, s: &str, style: &text::TextStyle, avail: f32) -> String {
    if text::measure(f, s, style) <= avail {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    for start in 1..chars.len() {
        let candidate: String = chars[start..].iter().collect();
        if text::measure(f, &candidate, style) <= avail {
            return candidate;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_reachable_by_a_tap_at_its_centre() {
        for page in [0u8, 1] {
            for row in 0..ROWS {
                for col in 0..row_len(page, row) {
                    let (x, y, w, h) = key_rect(page, row, col).unwrap();
                    let hit = hit(page, x + w / 2, y + h / 2);
                    assert_eq!(hit, key_at(page, row, col),
                               "page {page} row {row} col {col} is not where it is drawn");
                }
            }
        }
    }

    #[test]
    fn keys_stay_on_the_panel_and_do_not_overlap() {
        for page in [0u8, 1] {
            let mut boxes: Vec<(i32, i32, i32, i32)> = Vec::new();
            for row in 0..ROWS {
                for col in 0..row_len(page, row) {
                    let (x, y, w, h) = key_rect(page, row, col).unwrap();
                    assert!(x >= 0 && x + w <= W as i32, "page {page} row {row} col {col} off-panel");
                    assert!(y + h <= crate::canvas::H as i32, "row {row} runs off the bottom");
                    for other in &boxes {
                        let overlap = x < other.0 + other.2 && other.0 < x + w
                            && y < other.1 + other.3 && other.1 < y + h;
                        assert!(!overlap, "keys overlap on page {page}");
                    }
                    boxes.push((x, y, w, h));
                }
            }
        }
    }

    #[test]
    fn gaps_between_keys_are_not_typing() {
        // 2 px into the gap between two keys on the top row must hit nothing.
        let (x, y, w, h) = key_rect(0, 0, 0).unwrap();
        assert_eq!(hit(0, x + w + GAP / 2, y + h / 2), None);
        // and neither does the text field
        assert_eq!(hit(0, 240, TEXT_Y + 10), None);
    }

    #[test]
    fn shift_capitalises_exactly_one_letter() {
        let (mut text, mut shift, mut page) = (String::new(), false, 0u8);
        apply(Key::Shift, &mut text, &mut shift, &mut page);
        apply(Key::Char('a'), &mut text, &mut shift, &mut page);
        apply(Key::Char('b'), &mut text, &mut shift, &mut page);
        assert_eq!(text, "Ab");
        assert!(!shift);
    }

    #[test]
    fn space_never_leads_or_doubles() {
        let (mut text, mut shift, mut page) = (String::new(), false, 0u8);
        apply(Key::Space, &mut text, &mut shift, &mut page);
        assert_eq!(text, "");
        apply(Key::Char('a'), &mut text, &mut shift, &mut page);
        apply(Key::Space, &mut text, &mut shift, &mut page);
        apply(Key::Space, &mut text, &mut shift, &mut page);
        assert_eq!(text, "a ");
    }

    #[test]
    fn done_only_commits_a_name_with_something_in_it() {
        let (mut text, mut shift, mut page) = (String::from("   "), false, 0u8);
        assert!(!apply(Key::Done, &mut text, &mut shift, &mut page));
        text.push('x');
        assert!(apply(Key::Done, &mut text, &mut shift, &mut page));
    }

    #[test]
    fn the_symbol_page_types_symbols_and_comes_back() {
        let (mut text, mut shift, mut page) = (String::new(), false, 0u8);
        apply(Key::Page, &mut text, &mut shift, &mut page);
        assert_eq!(page, 1);
        let key = key_at(1, 0, 0).unwrap();
        apply(key, &mut text, &mut shift, &mut page);
        assert_eq!(text, "1");
        apply(Key::Page, &mut text, &mut shift, &mut page);
        assert_eq!(page, 0);
    }

    #[test]
    fn backspace_removes_the_last_character_and_stops_at_empty() {
        let (mut text, mut shift, mut page) = (String::from("ab"), false, 0u8);
        for _ in 0..5 {
            apply(Key::Backspace, &mut text, &mut shift, &mut page);
        }
        assert_eq!(text, "");
    }

    #[test]
    fn the_name_cannot_grow_past_the_limit() {
        let (mut text, mut shift, mut page) = (String::new(), false, 0u8);
        for _ in 0..(MAX_LEN + 20) {
            apply(Key::Char('x'), &mut text, &mut shift, &mut page);
        }
        assert_eq!(text.chars().count(), MAX_LEN);
    }
}
