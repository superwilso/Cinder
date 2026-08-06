//! Shelf sheet (`CShelfSheet`) — a bottom-sheet overlay drawn over the current screen: pin the
//! current place to one of three slots, jump straight back to a pin, and step back one screen.
//!
//! `render` dims whatever is already in the canvas (the screen behind) and draws the opaque sheet
//! over the lower portion. `hit` maps a click to a `ShelfHit` so the geometry lives in one place.
//!
//! ## Audit fixes (2026-08-05)
//! The sheet rendered fine but behaved wrongly under the finger:
//! * **A filled slot's body did nothing useful.** Only a ~60px "GO ›" column jumped to the pin;
//!   tapping the row itself fell through to `Pin`, so tapping a bookmark *overwrote a different
//!   slot* instead of going there. Now the whole row body is GO, and only the `×` column clears.
//! * **An empty slot's "GO" column silently closed the sheet.** `Go(i)` on an empty slot did
//!   nothing but still dismissed the overlay, so the row that says "Empty slot — pin here" would
//!   sometimes just make the Shelf vanish. Empty rows are now `PinTo(i)` across their whole width.
//! * **`Pin` never said which slot it used.** It filled the first empty slot, else silently
//!   clobbered slot 0. `PinTo(i)` makes the target explicit, and the navigator toasts the result.
//! * **A dead "Redo ›" control** sat next to Undo, permanently greyed and wired to nothing. It's
//!   gone; "Undo" is now labelled for what it does (go back one screen).

use crate::text::{Family, Weight};
use crate::widgets::{center, fill_rect, right, stroke_rect, sty};
use crate::{icons, text, Canvas, FontSet, Theme};

pub struct Pin<'a> {
    pub title: &'a str,
    pub sub: &'a str,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ShelfHit {
    None,
    Close,
    /// Step back one screen (and close the sheet).
    Back,
    /// Pin the current place into slot `i` (the row was empty, or the header Pin button chose it).
    PinTo(usize),
    /// Jump to the place pinned in slot `i`.
    Go(usize),
    /// Forget slot `i`.
    Clear(usize),
}

// Sheet geometry (shared by render + hit).
const TOP: i32 = 406; // sheet top y
const BACK_Y: i32 = 480;
const BACK_H: i32 = 46;
const PIN_BTN: (i32, i32, i32, i32) = (382, 558, 76, 48); // x,y,w,h
const THIS_Y: i32 = 558;
const THIS_H: i32 = 48;
const SLOT0_Y: i32 = 640;
const SLOT_DY: i32 = 46;
const SLOT_H: i32 = 40;
/// x from which a filled slot's row is the "forget" (×) target; left of it is GO.
const CLEAR_X: i32 = 412;

/// Number of pin slots.
pub const SLOTS: usize = 3;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, this_title: &str, this_sub: &str, pins: &[Option<Pin>; SLOTS]) {
    // 1. dim the screen behind (≈55% black backdrop)
    for px in c.buf.iter_mut() {
        let r = ((*px >> 16) & 0xff) * 45 / 100;
        let g = ((*px >> 8) & 0xff) * 45 / 100;
        let b = (*px & 0xff) * 45 / 100;
        *px = (r << 16) | (g << 8) | b;
    }
    // 2. sheet panel + accent top border
    fill_rect(c, 0, TOP, 480, 800 - TOP, t.bg);
    fill_rect(c, 0, TOP, 480, 1, t.acc);

    // header
    icons::bookmark(c, 24.0, (TOP + 18) as f32, 16.0, t.ink);
    text::draw(c, f, 48.0, (TOP + 30) as f32, "Shelf", &sty(Family::Sans, Weight::Bold, 22.0, t.ink, 0.0));
    right(c, f, 458.0, (TOP + 28) as f32, "CLOSE ×", &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.08));

    let cap = sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18);

    // NAVIGATION — one honest control (the old "Redo ›" was permanently inert)
    text::draw(c, f, 22.0, 466.0, "NAVIGATION", &cap);
    stroke_rect(c, 22, BACK_Y, 436, BACK_H, t.line, 1);
    text::draw(c, f, 36.0, (BACK_Y + 19) as f32, "\u{2039} Back", &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
    text::draw(c, f, 36.0, (BACK_Y + 36) as f32, "Return to the previous screen", &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));

    // THIS PLACE
    text::draw(c, f, 22.0, 544.0, "THIS PLACE", &cap);
    stroke_rect(c, 22, THIS_Y, 436, THIS_H, t.line, 1);
    text::draw(c, f, 36.0, 580.0, this_title, &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
    text::draw(c, f, 36.0, 596.0, this_sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    let (px, py, pw, ph) = PIN_BTN;
    fill_rect(c, px, py + 6, pw, ph - 12, t.acc);
    center(c, f, (px + pw / 2) as f32, (py + ph / 2 + 4) as f32, "Pin", &sty(Family::Sans, Weight::Bold, 14.0, t.acc_ink, 0.0));

    // PINNED · N/3
    let filled = pins.iter().filter(|p| p.is_some()).count();
    text::draw(c, f, 22.0, 626.0, &format!("PINNED \u{00b7} {}/{}", filled, SLOTS), &cap);
    for (i, slot) in pins.iter().enumerate() {
        let y = SLOT0_Y + i as i32 * SLOT_DY;
        match slot {
            Some(p) => {
                stroke_rect(c, 22, y, 436, SLOT_H, t.line, 1);
                text::draw(c, f, 36.0, (y + 24) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.acc, 0.0));
                text::draw(c, f, 58.0, (y + 17) as f32, p.title, &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
                text::draw(c, f, 58.0, (y + 32) as f32, p.sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
                // Separator makes the two tap zones legible: row body = GO, × column = forget.
                fill_rect(c, CLEAR_X, y + 6, 1, SLOT_H - 12, t.line);
                right(c, f, 406.0, (y + 24) as f32, "GO \u{203a}", &sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.0));
                center(c, f, ((CLEAR_X + 458) / 2) as f32, (y + 24) as f32, "\u{00d7}", &sty(Family::Mono, Weight::Regular, 15.0, t.faint, 0.0));
            }
            None => {
                // dashed border (drawn as dashes) + hint — the WHOLE row pins here
                let mut dx = 22;
                while dx < 458 {
                    fill_rect(c, dx, y, 7, 1, t.line);
                    fill_rect(c, dx, y + SLOT_H - 1, 7, 1, t.line);
                    dx += 14;
                }
                text::draw(c, f, 36.0, (y + 24) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
                text::draw(c, f, 58.0, (y + 24) as f32, "Empty slot \u{2014} tap to pin here", &sty(Family::Sans, Weight::Regular, 14.0, t.faint, 0.0));
            }
        }
    }
}

/// Map a click to a shelf action (geometry mirrors `render`). `filled[i]` says whether slot `i`
/// holds a pin — an empty row pins, a filled row goes (except its × column, which forgets).
pub fn hit(x: i32, y: i32, filled: [bool; SLOTS]) -> ShelfHit {
    if y < TOP {
        return ShelfHit::Close; // tap the dimmed backdrop
    }
    // CLOSE × in the header (top-right)
    if (TOP + 8..TOP + 44).contains(&y) && x > 380 {
        return ShelfHit::Close;
    }
    if (BACK_Y..BACK_Y + BACK_H).contains(&y) {
        return ShelfHit::Back;
    }
    let (px, py, pw, ph) = PIN_BTN;
    if (py..py + ph).contains(&y) && (px..px + pw).contains(&x) {
        // Header Pin button: first empty slot, else replace slot 0 (announced by the navigator).
        return ShelfHit::PinTo(filled.iter().position(|f| !f).unwrap_or(0));
    }
    for (i, &is_filled) in filled.iter().enumerate() {
        let sy = SLOT0_Y + i as i32 * SLOT_DY;
        if !(sy..sy + SLOT_H).contains(&y) {
            continue;
        }
        if !is_filled {
            return ShelfHit::PinTo(i); // whole row, exactly as the hint says
        }
        return if x >= CLEAR_X { ShelfHit::Clear(i) } else { ShelfHit::Go(i) };
    }
    ShelfHit::None
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: [bool; SLOTS] = [false, false, false];
    const FULL: [bool; SLOTS] = [true, true, true];

    fn slot_mid(i: usize) -> i32 {
        SLOT0_Y + i as i32 * SLOT_DY + SLOT_H / 2
    }

    #[test]
    fn empty_slot_pins_across_its_whole_width() {
        // Regression: the "GO" column on an EMPTY row used to return Go(i), which did nothing but
        // still dismissed the sheet — the row's own hint says it should pin.
        for x in [30, 200, 380, 400, 450] {
            assert_eq!(hit(x, slot_mid(1), EMPTY), ShelfHit::PinTo(1), "x={x}");
        }
    }

    #[test]
    fn filled_slot_body_goes_and_only_the_cross_clears() {
        // Regression: tapping a bookmark's row body used to PIN the current place into a
        // different slot instead of jumping to the bookmark.
        for x in [30, 200, 380, CLEAR_X - 1] {
            assert_eq!(hit(x, slot_mid(2), FULL), ShelfHit::Go(2), "x={x}");
        }
        assert_eq!(hit(CLEAR_X + 10, slot_mid(2), FULL), ShelfHit::Clear(2));
        assert_eq!(hit(455, slot_mid(0), FULL), ShelfHit::Clear(0));
    }

    #[test]
    fn header_pin_targets_the_first_empty_slot() {
        let (px, py, pw, ph) = PIN_BTN;
        let (cx, cy) = (px + pw / 2, py + ph / 2);
        assert_eq!(hit(cx, cy, EMPTY), ShelfHit::PinTo(0));
        assert_eq!(hit(cx, cy, [true, false, false]), ShelfHit::PinTo(1));
        assert_eq!(hit(cx, cy, [true, true, false]), ShelfHit::PinTo(2));
        assert_eq!(hit(cx, cy, FULL), ShelfHit::PinTo(0)); // all full → replace the first
    }

    #[test]
    fn backdrop_closes_and_back_row_navigates() {
        assert_eq!(hit(240, 100, EMPTY), ShelfHit::Close);
        assert_eq!(hit(240, BACK_Y + 10, EMPTY), ShelfHit::Back);
        assert_eq!(hit(430, TOP + 24, EMPTY), ShelfHit::Close);
    }
}
