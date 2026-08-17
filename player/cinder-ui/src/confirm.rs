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
    /// The Power-button hold menu — Sony's own gesture. Not a yes/no question but a CHOICE, so it
    /// draws three stacked rows instead of the two-button footer. It is its own `Ask` rather than
    /// a separate widget so that the modal stays one thing: one place that dims the screen, one
    /// place the navigator consumes taps, one hit test to keep honest.
    PowerMenu,
    /// Play was pressed on a song while the user queue still has tracks in it. Apple asks the same
    /// question, and it is a genuine either/or rather than a confirmation: the queue you built by
    /// hand is not something to silently discard, nor something to silently keep.
    QueueOnPlay,
    /// "Reset settings" from the Settings screen. A yes/no card rather than the two-tap arm used
    /// by Boot to stock: this one cannot be undone by rebooting, and the body has to say what the
    /// scope is before the finger commits.
    ResetSettings,
}

/// Every question the modal can ask. Exists so the overflow audit can render all of them without
/// a list that silently stops being complete when a new one is added.
pub const ALL: &[Ask] = &[
    Ask::Restart, Ask::PowerOff, Ask::PowerMenu, Ask::QueueOnPlay, Ask::ResetSettings,
];

impl Ask {
    /// Title, body, and the label on the confirming button. The confirm label names the ACTION
    /// ("Restart", "Power off") rather than saying "OK": at a glance you should be able to tell
    /// what is about to happen without re-reading the title.
    ///
    /// `PowerMenu` has no single confirming action, so its third field is empty and unused — the
    /// row labels live in `MENU_ROWS`.
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
            Ask::PowerMenu => ("Power", "", ""),
            Ask::QueueOnPlay => ("Play this now?", "", ""),
            Ask::ResetSettings => (
                "Reset settings?",
                "Every preference goes back to its default. Your music, playlists and pins are untouched.",
                "Reset",
            ),
        }
    }

    /// Does this question draw the stacked-row menu rather than the two-button footer?
    pub fn is_menu(self) -> bool {
        !self.rows().is_empty()
    }

    /// The menu's rows, top to bottom, each with the `Hit` it produces. Empty = this is a yes/no
    /// card, not a menu. Keeping the rows on the Ask is what lets one hit test and one renderer
    /// serve every menu — a second stacked-choice dialog would otherwise mean a second geometry to
    /// keep in step with its own hit test.
    pub fn rows(self) -> &'static [(&'static str, Hit)] {
        match self {
            // Destructive first, the way the two-button card puts Cancel under the thumb that is
            // already there — but the escape is also the LAST row, furthest from where the finger
            // lands after a Power hold.
            Ask::PowerMenu => &[
                ("Power off", Hit::PowerOff),
                ("Restart", Hit::Restart),
                ("Cancel", Hit::Cancel),
            ],
            // "Play now" first because it is what the tap already asked for; keeping the queue is
            // the considered choice and sits below it.
            Ask::QueueOnPlay => &[
                ("Clear queue and play", Hit::ClearQueue),
                ("Play now, keep queue", Hit::KeepQueue),
                ("Cancel", Hit::Cancel),
            ],
            _ => &[],
        }
    }

    /// Card height. The menu needs room for three stacked rows; the yes/no card does not.
    fn card_h(self) -> i32 {
        if self.is_menu() { MENU_HEAD_H + MENU_ROW_H * self.rows().len() as i32 } else { CARD_H }
    }

    /// Card top, derived from the shared optical centre so both cards sit in the same place.
    fn card_y(self) -> i32 {
        CARD_MID - self.card_h() / 2
    }
}

// Geometry. One source, shared by the render and the hit test — the same rule the accent swatches
// and the Now Playing rail follow, and for the same reason: a confirm button that is not where it
// is drawn is the worst possible place for that class of bug.
const CARD_W: i32 = 400;
const CARD_H: i32 = 232;
const CARD_X: i32 = (crate::canvas::W as i32 - CARD_W) / 2;
const CARD_Y: i32 = 260;
/// Both cards share this optical centre, so the modal does not appear to jump when a Power hold
/// opens the menu on top of where a Settings confirm would have been.
const CARD_MID: i32 = CARD_Y + CARD_H / 2;
/// Buttons: full-width halves of the card's footer, so both are large targets on a device driven
/// entirely by thumb.
const BTN_H: i32 = 64;
const BTN_Y: i32 = CARD_Y + CARD_H - BTN_H;
const BTN_SPLIT: i32 = CARD_X + CARD_W / 2;

/// Menu card: a title band plus three stacked full-width rows. Full-width rather than a 3-way
/// horizontal split because 400/3 = 133 px is too narrow to read a label in, and because a
/// vertical list is what a thumb scans on a portrait screen.
const MENU_ROW_H: i32 = 68;
const MENU_HEAD_H: i32 = 76;

/// Which button is under a tap. A tap anywhere outside the card cancels, which is the conventional
/// and forgiving reading of "I didn't mean it".
///
/// `Confirm` is what the two-button card produces (the caller already knows which `Ask` it asked).
/// `PowerOff` / `Restart` are named because the MENU has more than one affirmative answer, so the
/// hit test — not the caller — is what decides which one was chosen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Confirm,
    Cancel,
    PowerOff,
    Restart,
    /// Discard the hand-built user queue, then play the tapped song.
    ClearQueue,
    /// Play the tapped song and leave the queue alone — it plays after.
    KeepQueue,
}

pub fn hit(ask: Ask, x: i32, y: i32) -> Hit {
    let (cy, ch) = (ask.card_y(), ask.card_h());
    let in_card = x >= CARD_X && x < CARD_X + CARD_W && y >= cy && y < cy + ch;
    if !in_card {
        return Hit::Cancel; // tapping the dimmed backdrop dismisses
    }
    if ask.is_menu() {
        let top = cy + MENU_HEAD_H;
        if y < top {
            return Hit::Cancel; // the title band is not a button
        }
        let row = ((y - top) / MENU_ROW_H) as usize;
        return ask.rows().get(row).map(|r| r.1).unwrap_or(Hit::Cancel);
    }
    if y >= BTN_Y && x >= BTN_SPLIT {
        return Hit::Confirm;
    }
    Hit::Cancel
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, ask: Ask) {
    if ask.is_menu() {
        render_menu(c, t, f, ask);
        return;
    }
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

/// The Power-hold menu: a title band over three stacked rows. Deliberately plain — no accent fill
/// on any row. The two-button card can afford to emphasise its single affirmative answer; a menu
/// where the emphatic-looking row is the one that switches the device off is a trap, and the
/// finger arrives here already moving after a one-second hold.
fn render_menu(c: &mut Canvas, t: &Theme, f: &FontSet, ask: Ask) {
    let (cy, ch) = (ask.card_y(), ask.card_h());
    let (title, _, _) = ask.text();

    for y in 0..crate::canvas::H as i32 {
        for x in 0..crate::canvas::W as i32 {
            c.blend(x, y, t.bg, 200);
        }
    }
    fill_rect(c, CARD_X, cy, CARD_W, ch, t.panel);
    stroke_rect(c, CARD_X, cy, CARD_W, ch, t.line, 1);

    center(c, f, 240.0, (cy + 48) as f32, title,
           &sty(Family::Sans, Weight::Bold, 26.0, t.ink, 0.0));

    let top = cy + MENU_HEAD_H;
    for (i, (label, what)) in ask.rows().iter().enumerate() {
        let ry = top + MENU_ROW_H * i as i32;
        fill_rect(c, CARD_X, ry, CARD_W, 1, t.line);   // separator above every row, incl. the first
        // Cancel is dimmed, the two actions are in ink: the row that does nothing should not read
        // as equal in weight to the two that take the device away.
        let col = if *what == Hit::Cancel { t.dim } else { t.ink };
        center(c, f, 240.0, (ry + MENU_ROW_H / 2 + 7) as f32, label,
               &sty(Family::Sans, Weight::SemiBold, 20.0, col, 0.0));
    }
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
        let a = Ask::Restart;
        // Dead centre of the confirm button.
        assert_eq!(hit(a, BTN_SPLIT + 90, BTN_Y + BTN_H / 2), Hit::Confirm);
        // Left footer half = cancel.
        assert_eq!(hit(a, CARD_X + 60, BTN_Y + BTN_H / 2), Hit::Cancel);
        // Body of the card = cancel (not a stray confirm).
        assert_eq!(hit(a, 240, CARD_Y + 60), Hit::Cancel);
        // Backdrop, all four sides.
        assert_eq!(hit(a, 5, 5), Hit::Cancel);
        assert_eq!(hit(a, 240, CARD_Y - 5), Hit::Cancel);
        assert_eq!(hit(a, 240, CARD_Y + CARD_H + 5), Hit::Cancel);
        assert_eq!(hit(a, CARD_X - 5, BTN_Y + 10), Hit::Cancel);
    }

    /// Sweep every pixel: nothing outside the drawn confirm rectangle may confirm.
    #[test]
    fn no_pixel_outside_the_confirm_button_confirms() {
        for ask in [Ask::Restart, Ask::PowerOff] {
            for y in (0..crate::canvas::H as i32).step_by(3) {
                for x in (0..crate::canvas::W as i32).step_by(3) {
                    if hit(ask, x, y) == Hit::Confirm {
                        assert!(
                            x >= BTN_SPLIT
                                && x < CARD_X + CARD_W
                                && y >= BTN_Y
                                && y < CARD_Y + CARD_H,
                            "({x},{y}) confirms but is outside the drawn button"
                        );
                    }
                }
            }
        }
    }

    /// The yes/no questions must produce a confirm label that names the action rather than "OK".
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

    /// Each menu row must hit-test to its OWN action, over its whole drawn band. A row that is
    /// one pixel out here means a Power hold offers "Restart" and delivers a power-off.
    #[test]
    fn every_menu_row_hits_its_own_action() {
        let a = Ask::PowerMenu;
        let top = a.card_y() + MENU_HEAD_H;
        for (i, (label, want)) in a.rows().iter().enumerate() {
            let ry = top + MENU_ROW_H * i as i32;
            for dy in [1, MENU_ROW_H / 2, MENU_ROW_H - 1] {
                for x in [CARD_X + 2, 240, CARD_X + CARD_W - 2] {
                    assert_eq!(hit(a, x, ry + dy), *want, "{label} row missed at ({x},{})", ry + dy);
                }
            }
        }
    }

    /// The title band is not a button, and the backdrop still cancels on every side.
    #[test]
    fn menu_title_and_backdrop_cancel() {
        let a = Ask::PowerMenu;
        let (cy, ch) = (a.card_y(), a.card_h());
        assert_eq!(hit(a, 240, cy + 10), Hit::Cancel);
        assert_eq!(hit(a, 240, cy + MENU_HEAD_H - 1), Hit::Cancel);
        assert_eq!(hit(a, 5, 5), Hit::Cancel);
        assert_eq!(hit(a, 240, cy - 5), Hit::Cancel);
        assert_eq!(hit(a, 240, cy + ch + 5), Hit::Cancel);
        assert_eq!(hit(a, CARD_X - 5, cy + MENU_HEAD_H + 10), Hit::Cancel);
    }

    /// Sweep every pixel of the menu: nothing outside the three drawn rows may return an action,
    /// and the rows must exactly tile the space below the title band with no gap and no overlap.
    #[test]
    fn no_pixel_outside_a_menu_row_acts() {
        let a = Ask::PowerMenu;
        let (cy, ch) = (a.card_y(), a.card_h());
        let top = cy + MENU_HEAD_H;
        assert_eq!(top + MENU_ROW_H * a.rows().len() as i32, cy + ch, "rows must fill the card exactly");
        for y in 0..crate::canvas::H as i32 {
            for x in (0..crate::canvas::W as i32).step_by(3) {
                if hit(a, x, y) != Hit::Cancel {
                    assert!(
                        x >= CARD_X && x < CARD_X + CARD_W && y >= top && y < cy + ch,
                        "({x},{y}) acts but is outside the drawn rows"
                    );
                }
            }
        }
    }

    /// The two cards must share an optical centre — the menu opening over a Settings confirm
    /// should not make the modal appear to jump.
    #[test]
    fn both_cards_share_a_centre() {
        for ask in [Ask::Restart, Ask::PowerOff, Ask::PowerMenu] {
            assert_eq!(ask.card_y() + ask.card_h() / 2, CARD_MID, "{ask:?} is off-centre");
        }
    }

    /// The menu must offer exactly one way out and it must be distinct from both actions.
    #[test]
    fn menu_offers_one_escape() {
        let cancels = Ask::PowerMenu.rows().iter().filter(|r| r.1 == Hit::Cancel).count();
        assert_eq!(cancels, 1, "the Power menu needs exactly one Cancel row");
        assert!(Ask::PowerMenu.rows().iter().any(|r| r.1 == Hit::PowerOff));
        assert!(Ask::PowerMenu.rows().iter().any(|r| r.1 == Hit::Restart));
    }
}
