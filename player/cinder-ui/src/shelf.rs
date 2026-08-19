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
    /// Pin the current place into slot `i` (the row was empty, or the header Pin button chose it).
    PinTo(usize),
    /// Jump to the place pinned in slot `i`.
    Go(usize),
    /// Forget slot `i`.
    Clear(usize),
}

// Sheet geometry (shared by render + hit).
//
// Rebalanced 2026-08-19, reported as the Shelf being fiddly. The PINS are the reason the sheet
// exists, and they had the worst of it: three 40 px rows with 6 px dead gaps between them, crammed
// into the last 130 px of the panel, while Back and This Place — used far less — got roomy ones and
// 400 px above sat as dimmed backdrop doing nothing. The sheet now starts higher and spends the
// space it gains on the pin rows (58 px, the same order as a library row), and `hit` bands them
// contiguously so a tap between two slots lands on one instead of being swallowed.
const TOP: i32 = 270; // sheet top y
const PIN_BTN: (i32, i32, i32, i32) = (372, 350, 88, 56); // x,y,w,h
const THIS_Y: i32 = 350;
const THIS_H: i32 = 56;
const SLOT0_Y: i32 = 440;
const SLOT_DY: i32 = 56;
const SLOT_H: i32 = 52;
/// x from which a filled slot's row is the "forget" (×) target; left of it is GO.
const CLEAR_X: i32 = 420;
/// Baseline for a section caption sitting above the block it names.
const fn caption_y(block_top: i32) -> i32 { block_top - 14 }

/// Number of pin slots.
///
/// Was 3, for no reason beyond the 130 px the old sheet left for them. Doubling it cost one
/// control: the sheet used to carry a "‹ Back — return to the previous screen" row, which
/// duplicated the back chevron that every screen already draws and that the user reaches far more
/// often. Trading a redundant button for three more bookmarks is a good trade.
///
/// Persistence needs nothing here: the config writes `pin0=`…`pin{SLOTS-1}` from this constant and
/// the reader parses whatever index it finds, so old files load and new slots simply start empty.
pub const SLOTS: usize = 6;

/// Centre of the header "Pin" button. Exposed so callers and tests aim at the layout instead of
/// repeating a pixel — the coordinates in the nav tests silently stopped hitting it when the sheet
/// was rebalanced, and the tests failed on the CONSEQUENCE (no pin was stored) rather than saying
/// the tap had missed.
pub fn pin_button_center() -> (i32, i32) {
    let (px, py, pw, ph) = PIN_BTN;
    (px + pw / 2, py + ph / 2)
}

/// Vertical centre of pin slot `i`.
pub fn slot_center_y(i: usize) -> i32 {
    SLOT0_Y + i as i32 * SLOT_DY + SLOT_H / 2
}

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

    // THIS PLACE
    text::draw(c, f, 22.0, caption_y(THIS_Y) as f32, "THIS PLACE", &cap);
    stroke_rect(c, 22, THIS_Y, 436, THIS_H, t.line, 1);
    text::draw(c, f, 36.0, (THIS_Y + 24) as f32, this_title, &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
    text::draw(c, f, 36.0, (THIS_Y + 42) as f32, this_sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    let (px, py, pw, ph) = PIN_BTN;
    fill_rect(c, px, py + 6, pw, ph - 12, t.acc);
    center(c, f, (px + pw / 2) as f32, (py + ph / 2 + 4) as f32, "Pin", &sty(Family::Sans, Weight::Bold, 14.0, t.acc_ink, 0.0));

    // PINNED · N/3
    let filled = pins.iter().filter(|p| p.is_some()).count();
    text::draw(c, f, 22.0, caption_y(SLOT0_Y) as f32, &format!("PINNED \u{00b7} {}/{}", filled, SLOTS), &cap);
    for (i, slot) in pins.iter().enumerate() {
        let y = SLOT0_Y + i as i32 * SLOT_DY;
        match slot {
            Some(p) => {
                stroke_rect(c, 22, y, 436, SLOT_H, t.line, 1);
                let mid = SLOT_H / 2;
                text::draw(c, f, 36.0, (y + mid + 5) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.acc, 0.0));
                text::draw(c, f, 58.0, (y + mid - 3) as f32, p.title, &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
                text::draw(c, f, 58.0, (y + mid + 15) as f32, p.sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
                // Separator makes the two tap zones legible: row body = GO, × column = forget.
                fill_rect(c, CLEAR_X, y + 6, 1, SLOT_H - 12, t.line);
                right(c, f, 404.0, (y + mid + 5) as f32, "GO \u{203a}", &sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.0));
                center(c, f, ((CLEAR_X + 458) / 2) as f32, (y + mid + 6) as f32, "\u{00d7}", &sty(Family::Mono, Weight::Regular, 17.0, t.faint, 0.0));
            }
            None => {
                // dashed border (drawn as dashes) + hint — the WHOLE row pins here
                let mut dx = 22;
                while dx < 458 {
                    fill_rect(c, dx, y, 7, 1, t.line);
                    fill_rect(c, dx, y + SLOT_H - 1, 7, 1, t.line);
                    dx += 14;
                }
                text::draw(c, f, 36.0, (y + SLOT_H / 2 + 5) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
                text::draw(c, f, 58.0, (y + SLOT_H / 2 + 5) as f32, "Empty slot \u{2014} tap to pin here", &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
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
    let (px, py, pw, ph) = PIN_BTN;
    if (py..py + ph).contains(&y) && (px..px + pw).contains(&x) {
        // Header Pin button: first empty slot, else replace slot 0 (announced by the navigator).
        return ShelfHit::PinTo(filled.iter().position(|f| !f).unwrap_or(0));
    }
    for (i, &is_filled) in filled.iter().enumerate() {
        // The BAND, not the drawn box: the gap between two rows used to swallow taps outright,
        // which on a three-row sheet reads as the Shelf ignoring you.
        let sy = SLOT0_Y + i as i32 * SLOT_DY;
        if !(sy..sy + SLOT_DY).contains(&y) {
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

    const EMPTY: [bool; SLOTS] = [false; SLOTS];
    const FULL: [bool; SLOTS] = [true; SLOTS];

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
        assert_eq!(hit(cx, cy, [true, false, false, false, false, false]), ShelfHit::PinTo(1));
        assert_eq!(hit(cx, cy, [true, true, false, false, false, false]), ShelfHit::PinTo(2));
        assert_eq!(hit(cx, cy, FULL), ShelfHit::PinTo(0)); // all full → replace the first
    }

    #[test]
    fn backdrop_and_close_dismiss_the_sheet() {
        assert_eq!(hit(240, 100, EMPTY), ShelfHit::Close);
        assert_eq!(hit(430, TOP + 24, EMPTY), ShelfHit::Close);
    }

    /// Every slot is reachable, and none of them collides with the header or the Pin button.
    #[test]
    fn all_six_slots_are_addressable_and_distinct() {
        let mut seen = Vec::new();
        for i in 0..SLOTS {
            let y = slot_center_y(i);
            assert!(y < 800, "slot {i} is off the bottom of the panel (y={y})");
            match hit(240, y, EMPTY) {
                ShelfHit::PinTo(n) => {
                    assert_eq!(n, i, "slot {i} resolved to {n}");
                    seen.push(n);
                }
                other => panic!("slot {i} at y={y} resolved to {other:?}"),
            }
        }
        assert_eq!(seen.len(), SLOTS);
        // A filled row goes rather than re-pins, and only its × column forgets.
        assert_eq!(hit(240, slot_center_y(2), FULL), ShelfHit::Go(2));
        assert_eq!(hit(440, slot_center_y(2), FULL), ShelfHit::Clear(2));
        // The Pin button is still its own target, not the first slot.
        let (pbx, pby) = pin_button_center();
        assert_eq!(hit(pbx, pby, EMPTY), ShelfHit::PinTo(0));
    }
}
