//! Settings — ported from cinder-proto-screens3.jsx `CSettings`. Sections:
//! DISPLAY (Theme seg toggle, screen-off timer, brightness), SYSTEM (storage,
//! database, battery care, USB mode), ABOUT (firmware, model).

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;

fn eyebrow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    text::draw(c, f, 22.0, (y + 14) as f32, label, &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    y + 24
}

fn srow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str, value: &str, chevron: bool) -> i32 {
    let rh = 58;
    let cy = y + rh / 2;
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
    let vx = if chevron { 438.0 } else { 458.0 };
    right(c, f, vx, (cy + 4) as f32, value, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.04));
    if chevron {
        icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
    }
    hline(c, y + rh, t.line);
    y + rh
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, theme_is_night: bool, usb_dac: bool) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Settings", None);

    let mut y = eyebrow(c, t, f, y0, "DISPLAY");

    // Theme row with Day/Night segmented control
    let rh = 58;
    hline(c, y, t.line);
    let cy = y + rh / 2;
    text::draw(c, f, 22.0, (cy + 5) as f32, "Theme", &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
    // segmented: Day | Night, ending at 458
    let segs = [("DAY", !theme_is_night), ("NIGHT", theme_is_night)];
    // measure total width to right-align
    let sh = 28;
    let mut widths = [0i32; 2];
    for (i, (label, _)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.1);
        widths[i] = text::measure(f, label, &st) as i32 + 26;
    }
    let total = widths[0] + widths[1] + 8;
    let mut sx = 458 - total;
    for (i, (label, on)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 10.0, if *on { t.acc_ink } else { t.dim }, 0.1);
        if *on {
            fill_rect(c, sx, cy - sh / 2, widths[i], sh, t.acc);
        }
        stroke_rect(c, sx, cy - sh / 2, widths[i], sh, if *on { t.acc } else { t.line }, 1);
        text::draw(c, f, (sx + 13) as f32, (cy + 4) as f32, label, &st);
        sx += widths[i] + 8;
    }
    hline(c, y + rh, t.line);
    y += rh;

    y = srow(c, t, f, y, "Screen-off timer", "30 SEC", false);
    y = srow(c, t, f, y, "Brightness", "3 / 5", false);

    y = eyebrow(c, t, f, y + 20, "SYSTEM");
    y = srow(c, t, f, y, "Storage", "12.4 / 16 GB · SD 64 GB", true);
    y = srow(c, t, f, y, "Database", "REBUILD · LAST: TODAY", true);
    y = srow(c, t, f, y, "Battery care", "CHARGE LIMIT 90%", true);
    y = srow(c, t, f, y, "USB mode", if usb_dac { "DAC" } else { "MASS STORAGE" }, true);

    y = eyebrow(c, t, f, y + 20, "ABOUT");
    y = srow(c, t, f, y, "Firmware", "CINDER 1.0 · RUST", false);
    let _ = srow(c, t, f, y, "Model", "SONY NW-A55", false);
}
