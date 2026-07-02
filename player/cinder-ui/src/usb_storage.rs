//! USB mass-storage modal screen. Shown while the storage volume is handed to the PC (the shell
//! unmounted /contents and pointed the USB gadget's mass-storage LUN at it). Deliberately modal
//! and quiet: the library/player must not touch storage until the mode is left, so the only ways
//! out are the physical Back button or unplugging the cable (the shell watches for both and
//! remounts before popping this screen).

use crate::canvas::{Canvas, H, W};
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::sty;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    let cx = (W / 2) as f32;
    let cy = (H / 2) as i32;

    icons::usb(c, cx, (cy - 120) as f32, 44.0, t.acc);

    let title = "USB Storage";
    let tst = sty(Family::Sans, Weight::ExtraBold, 26.0, t.ink, -0.01);
    let tw = text::measure(f, title, &tst);
    text::draw(c, f, cx - tw / 2.0, (cy - 40) as f32, title, &tst);

    let lines = [
        "Storage is connected to your computer.",
        "Music and files are unavailable until you're done.",
        "",
        "Unplug the cable (or press Back) to return.",
    ];
    let mut y = cy + 8;
    for l in lines {
        if !l.is_empty() {
            let st = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
            let w = text::measure(f, l, &st);
            text::draw(c, f, cx - w / 2.0, y as f32, l, &st);
        }
        y += 28;
    }
}
