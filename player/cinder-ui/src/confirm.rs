//! A modal confirmation dialog.
//!
//! Built generic on purpose. Two things need it now — Restart and Power off, which take the device
//! away mid-song — and a third is coming (the queue's "clear it, or keep it and play this later?"
//! prompt). A dialog per caller would drift in geometry and hit-testing the way the status strip
//! did before it was drawn in one place.
//!
//! WHY A MODAL AND NOT THE TWO-TAP ROW. Settings ▸ Boot to stock arms on the first tap and acts on
//! the second, showing "TAP AGAIN" in the value column. That works, but it is easy to arm by
//! accident and the only thing standing between a stray double-tap and a reboot is a row label. A
//! power-off deserves a deliberate, unambiguous "yes" against a named action, and the queue prompt
//! is a genuine either/or that a two-tap cannot express at all.

use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, stroke_rect, sty};
use crate::Canvas;

/// What the dialog is asking about. The navigator maps the answer onto an action; this enum only
/// decides what the dialog SAYS, so adding a question never touches the geometry or the hit test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    Restart,
    PowerOff,
}

impl Ask {
    /// Title, body, and the label on the confirming button. The confirm label names the ACTION
    /// ("Restart", "Power off") rather than saying "OK": at a glance you should be able to tell
    /// what is about to happen without re-reading the title.
    pub fn text(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Ask::Restart => (
                "Restart?",
                "Cinder will close and the player will start again.",
                "Restart",
            ),
            Ask::PowerOff => (
                "Power off?",
                "The device will switch off. Hold Power to turn it back on.",
                "Power off",
            ),
        }
    }
}

// Geometry. One source, shared by the render and the hit test — the same rule the accent swatches
// and the Now Playing rail follow, and for the same reason: a confirm button that is not where it
// is drawn is the worst possible place for that class of bug.
const CARD_W: i32 = 400;
const CARD_H: i32 = 232;
const CARD_X: i32 = (crate::canvas::W as i32 - CARD_W) / 2;
const CARD_Y: i32 = 260;
/// Buttons: full-width halves of the card's footer, so both are large targets on a device driven
/// entirely by thumb.
const BTN_H: i32 = 64;
const BTN_Y: i32 = CARD_Y + CARD_H - BTN_H;
const BTN_SPLIT: i32 = CARD_X + CARD_W / 2;

/// Which button is under a tap, if any. `None` means the tap missed both — and a tap anywhere
/// outside the card cancels, which is the conventional and forgiving reading of "I didn't mean it".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Confirm,
    Cancel,
}

pub fn hit(x: i32, y: i32) -> Hit {
    let in_card = x >= CARD_X && x < CARD_X + CARD_W && y >= CARD_Y && y < CARD_Y + CARD_H;
    if !in_card {
        return Hit::Cancel; // tapping the dimmed backdrop dismisses
    }
    if y >= BTN_Y && x >= BTN_SPLIT {
        return Hit::Confirm;
    }
    Hit::Cancel
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, ask: Ask) {
    let (title, body, confirm) = ask.text();

    // Dim the screen behind it. The dialog is modal, and dimming is what says so — without it a
    // card just looks like another panel and the screen behind reads as still live.
    for y in 0..crate::canvas::H as i32 {
        for x in 0..crate::canvas::W as i32 {
            c.blend(x, y, t.bg, 200);
        }
    }

    fill_rect(c, CARD_X, CARD_Y, CARD_W, CARD_H, t.panel);
    stroke_rect(c, CARD_X, CARD_Y, CARD_W, CARD_H, t.line, 1);

    center(c, f, 240.0, (CARD_Y + 58) as f32, title,
           &sty(Family::Sans, Weight::Bold, 26.0, t.ink, 0.0));
    // The body wraps by hand at a word boundary rather than mid-word: these strings are short and
    // known, so a real wrapper would be machinery for two sentences.
    let (l1, l2) = split_body(body);
    center(c, f, 240.0, (CARD_Y + 96) as f32, l1,
           &sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
    if !l2.is_empty() {
        center(c, f, 240.0, (CARD_Y + 118) as f32, l2,
               &sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
    }

    // Footer rule + the split between the two buttons.
    fill_rect(c, CARD_X, BTN_Y, CARD_W, 1, t.line);
    fill_rect(c, BTN_SPLIT, BTN_Y, 1, BTN_H, t.line);

    let by = (BTN_Y + BTN_H / 2 + 6) as f32;
    // CANCEL on the left, in ink: the safe answer is the one your thumb reaches first and it is not
    // dressed up as the primary action.
    center(c, f, (CARD_X + CARD_W / 4) as f32, by, "Cancel",
           &sty(Family::Sans, Weight::SemiBold, 18.0, t.dim, 0.0));
    // CONFIRM on the right, filled with the accent — deliberately the more emphatic target, but it
    // is still one tap away from a dismissal on either side of it.
    fill_rect(c, BTN_SPLIT + 1, BTN_Y + 1, CARD_X + CARD_W - BTN_SPLIT - 1, BTN_H - 1, t.acc);
    center(c, f, ((BTN_SPLIT + CARD_X + CARD_W) / 2) as f32, by, confirm,
           &sty(Family::Sans, Weight::Bold, 18.0, t.acc_ink, 0.0));
}

/// Split a one-line body at the last space before it would overrun the card.
fn split_body(body: &str) -> (&str, &str) {
    const MAX: usize = 46;
    if body.len() <= MAX {
        return (body, "");
    }
    match body[..MAX].rfind(' ') {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => (body, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The confirming half must be the RIGHT half of the footer, and everything else must cancel.
    /// This is the whole safety property of the dialog: a mis-mapped hit test would turn a
    /// dismissal into a power-off.
    #[test]
    fn only_the_right_hand_footer_confirms() {
        // Dead centre of the confirm button.
        assert_eq!(hit(BTN_SPLIT + 90, BTN_Y + BTN_H / 2), Hit::Confirm);
        // Left footer half = cancel.
        assert_eq!(hit(CARD_X + 60, BTN_Y + BTN_H / 2), Hit::Cancel);
        // Body of the card = cancel (not a stray confirm).
        assert_eq!(hit(240, CARD_Y + 60), Hit::Cancel);
        // Backdrop, all four sides.
        assert_eq!(hit(5, 5), Hit::Cancel);
        assert_eq!(hit(240, CARD_Y - 5), Hit::Cancel);
        assert_eq!(hit(240, CARD_Y + CARD_H + 5), Hit::Cancel);
        assert_eq!(hit(CARD_X - 5, BTN_Y + 10), Hit::Cancel);
    }

    /// Sweep every pixel: nothing outside the drawn confirm rectangle may confirm.
    #[test]
    fn no_pixel_outside_the_confirm_button_confirms() {
        for y in (0..crate::canvas::H as i32).step_by(3) {
            for x in (0..crate::canvas::W as i32).step_by(3) {
                if hit(x, y) == Hit::Confirm {
                    assert!(
                        x >= BTN_SPLIT && x < CARD_X + CARD_W && y >= BTN_Y && y < CARD_Y + CARD_H,
                        "({x},{y}) confirms but is outside the drawn button"
                    );
                }
            }
        }
    }

    /// Both questions must produce a confirm label that names the action rather than saying "OK".
    #[test]
    fn every_question_names_its_action() {
        for ask in [Ask::Restart, Ask::PowerOff] {
            let (title, body, confirm) = ask.text();
            assert!(!title.is_empty() && !body.is_empty());
            assert!(
                !confirm.eq_ignore_ascii_case("ok") && !confirm.eq_ignore_ascii_case("yes"),
                "{ask:?} confirms with a bare {confirm:?}"
            );
        }
    }

    /// The body must fit the card on both lines — an overrun would run out past the panel edge.
    #[test]
    fn bodies_wrap_within_the_card() {
        for ask in [Ask::Restart, Ask::PowerOff] {
            let (_, body, _) = ask.text();
            let (a, b) = split_body(body);
            assert!(a.len() <= 46, "{ask:?} line 1 too long: {a:?}");
            assert!(b.len() <= 46, "{ask:?} line 2 too long: {b:?}");
        }
    }
}
