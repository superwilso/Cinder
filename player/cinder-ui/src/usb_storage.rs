//! USB mass-storage modal screen. Shown while the storage volume is handed to the PC (the shell
//! unmounted /contents and pointed the USB gadget's mass-storage LUN at it). Deliberately modal
//! and quiet: the library/player must not touch storage until the mode is left, so the ways out
//! are the on-screen TURN OFF button, the physical Back button, or unplugging the cable (the
//! shell watches for these and remounts before popping this screen).

use crate::canvas::{Canvas, H, W};
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};

/// The TURN OFF button's hit rectangle (x, y, w, h) — shared by render() and the tap test so
/// they can never drift apart.
pub const OFF_BTN: (i32, i32, i32, i32) = ((W as i32 - 220) / 2, H as i32 / 2 + 150, 220, 60);

/// Is a tap at (x, y) on the TURN OFF button?
pub fn hit_off(x: i32, y: i32) -> bool {
    let (bx, by, bw, bh) = OFF_BTN;
    x >= bx && x < bx + bw && y >= by && y < by + bh
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    let cx = (W / 2) as f32;
    let cy = (H / 2) as i32;

    icons::usb(c, cx, (cy - 120) as f32, 44.0, t.acc);

    let title = "USB Storage";
    let tst = sty(Family::Sans, Weight::ExtraBold, 29.0, t.ink, -0.01);
    let tw = text::measure(f, title, &tst);
    text::draw(c, f, cx - tw / 2.0, (cy - 40) as f32, title, &tst);

    let lines = [
        "Storage is connected to your computer.",
        "Music and files are unavailable until you're done.",
        "",
        "Unplug the cable or turn off below to return.",
    ];
    let mut y = cy + 8;
    for l in lines {
        if !l.is_empty() {
            let st = sty(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0);
            let w = text::measure(f, l, &st);
            text::draw(c, f, cx - w / 2.0, y as f32, l, &st);
        }
        y += 28;
    }

    // TURN OFF button (also reachable via the physical Back button)
    let (bx, by, bw, bh) = OFF_BTN;
    fill_rect(c, bx, by, bw, bh, t.acc);
    let bst = sty(Family::Sans, Weight::ExtraBold, 19.0, t.acc_ink, 0.06);
    let lw = text::measure(f, "TURN OFF", &bst);
    text::draw(c, f, cx - lw / 2.0, (by + bh / 2 + 6) as f32, "TURN OFF", &bst);
}
