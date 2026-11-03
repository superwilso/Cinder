//! USB mass-storage modal screen. Shown while the storage volume is handed to the PC (the shell
//! unmounted /contents and pointed the USB gadget's mass-storage LUN at it). Deliberately modal
//! and quiet: the library/player must not touch storage until the mode is left, so the ways out
//! are the on-screen TURN OFF button, the physical Back button, or unplugging the cable (the
//! shell watches for these and remounts before popping this screen).

use crate::canvas::{Canvas, H, W};
use crate::icons;
use crate::text::{Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};

const BTN_W: i32 = 220;
const BTN_H: i32 = 60;
/// Where the TURN OFF button sits when the text above it fits on one line a sentence.
const BTN_Y: i32 = H as i32 / 2 + 150;

thread_local! {
    /// Where render() last put the button. Big text wraps the sentences onto more lines, and the
    /// button moves down to stay clear of them, so the tap test reads the drawn position instead
    /// of a constant (the same arrangement as Now Playing's back arrow).
    static DRAWN_Y: std::cell::Cell<i32> = const { std::cell::Cell::new(BTN_Y) };
}

/// The TURN OFF button's rectangle (x, y, w, h) as last drawn.
pub fn off_btn() -> (i32, i32, i32, i32) {
    (
        (W as i32 - BTN_W) / 2,
        DRAWN_Y.with(|y| y.get()),
        BTN_W,
        BTN_H,
    )
}

/// Is a tap at (x, y) on the TURN OFF button?
pub fn hit_off(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = off_btn();
    x >= bx && x < bx + bw && y >= by && y < by + bh
}

fn body_style(t: &Theme) -> TextStyle {
    sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0)
}

/// The body text as drawn — each line with its baseline — and the button's top edge.
///
/// Each sentence wraps instead of being cut with "…": at Big view's text size two of them did not
/// fit the width (seen on the player 11-03). A wrapped line steps by the scaled line height; a
/// sentence still starts 28 px after the last, so at normal size nothing moves. The button goes
/// below the last line when the text needs the room, and never off the screen.
fn layout(f: &FontSet, st: &TextStyle) -> (Vec<(String, i32)>, i32) {
    let sentences = [
        crate::i18n::text("Storage is connected to your computer."),
        crate::i18n::text("Music and files are unavailable until you're done."),
        "",
        crate::i18n::text("Unplug the cable or turn off below to return."),
    ];
    let wrap_w = W as f32 - 2.0 * crate::widgets::EDGE;
    let step = crate::text::scaled(22.0) as i32;
    let mut out = Vec::new();
    let mut y = H as i32 / 2 + 8;
    for s in sentences {
        if !s.is_empty() {
            // Balanced: the narrowest width that still takes as many lines, so a wrapped sentence
            // does not leave one word on a line of its own.
            let n = crate::track_info::wrap(f, s, st, wrap_w).len();
            let (mut lo, mut hi) = (wrap_w / 3.0, wrap_w);
            while n > 1 && hi - lo > 2.0 {
                let mid = (lo + hi) / 2.0;
                if crate::track_info::wrap(f, s, st, mid).len() > n {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            for (i, part) in crate::track_info::wrap(f, s, st, hi).iter().enumerate() {
                if i > 0 {
                    y += step;
                }
                out.push((part.trim_end().to_string(), y));
            }
        }
        y += 28;
    }
    let below = out.last().map_or(0, |(_, y)| *y) + crate::text::scaled(28.0) as i32;
    (out, BTN_Y.max(below).min(H as i32 - BTN_H - 16))
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    let cx = (W / 2) as f32;
    let cy = (H / 2) as i32;

    icons::usb(c, cx, (cy - 120) as f32, 44.0, t.acc);

    let title = crate::i18n::text("USB Storage");
    let tst = sty(Family::Sans, Weight::ExtraBold, 29.0, t.ink, -0.01);
    crate::widgets::center(c, f, cx, (cy - 40) as f32, title, &tst);

    let st = body_style(t);
    let (lines, btn_y) = layout(f, &st);
    for (line, y) in &lines {
        crate::widgets::center(c, f, cx, *y as f32, line, &st);
    }

    // TURN OFF button (also reachable via the physical Back button)
    DRAWN_Y.with(|d| d.set(btn_y));
    let (bx, by, bw, bh) = off_btn();
    fill_rect(c, bx, by, bw, bh, t.acc);
    let bst = sty(Family::Sans, Weight::ExtraBold, 19.0, t.acc_ink, 0.06);
    crate::widgets::center(
        c,
        f,
        cx,
        (by + bh / 2 + 6) as f32,
        crate::i18n::text("TURN OFF"),
        &bst,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At every text size the slider offers, each line fits the width (so nothing is cut with
    /// "…"), the button sits below the last line and on the screen, and the tap test answers
    /// where the button was drawn. At normal size the layout is the original one.
    #[test]
    fn the_text_wraps_and_the_button_stays_clear_of_it() {
        let _g = crate::text::scale_guard();
        let f = FontSet::load();
        let t = Theme::day();
        let wrap_w = W as f32 - 2.0 * crate::widgets::EDGE;
        for pct in [80, 100, crate::nav::BIG_TEXT_PCT, 140] {
            crate::text::set_scale_pct(pct);
            let st = body_style(&t);
            let (lines, btn_y) = layout(&f, &st);
            for (line, _) in &lines {
                assert!(
                    crate::text::measure(&f, line, &st) <= wrap_w,
                    "{pct}%: {line:?} is wider than the screen"
                );
            }
            let last = lines.last().unwrap().1;
            assert!(
                last + 8 < btn_y,
                "{pct}%: the last line runs into the button"
            );
            assert!(
                btn_y + BTN_H <= H as i32,
                "{pct}%: the button is off the screen"
            );
            if pct == 100 {
                assert_eq!(lines.len(), 3, "normal size: one line a sentence");
                assert_eq!(btn_y, BTN_Y, "normal size: the button has not moved");
            }
            if pct >= crate::nav::BIG_TEXT_PCT {
                assert!(
                    lines.len() > 3,
                    "{pct}%: expected the long sentences to wrap"
                );
            }
            let mut c = Canvas::new();
            render(&mut c, &t, &f);
            let (bx, by, bw, bh) = off_btn();
            assert_eq!(by, btn_y);
            assert!(hit_off(bx + bw / 2, by + bh / 2));
            assert!(!hit_off(bx + bw / 2, by - 4));
        }
    }
}
