//! Menu (the hub) — ported from cinder-proto-screens3.jsx `CMenu`.
//! 10 rows: icon + label (17/600) + live value (mono) + chevron, each on a
//! 63px row with hairline separators.

use crate::canvas::{Canvas, W};
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub struct MenuItem<'a> {
    pub icon: &'a str,
    pub label: &'a str,
    pub value: &'a str,
    pub active: bool,
}

fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

fn draw_icon(c: &mut Canvas, name: &str, cx: f32, cy: f32, s: f32, col: Rgb888) {
    match name {
        "note" => icons::note(c, cx, cy, s, col),
        "library" => icons::library(c, cx, cy, s, col),
        "queue" => icons::queue(c, cx, cy, s, col),
        "radio" => icons::radio(c, cx, cy, s, col),
        "eq" => icons::eq(c, cx, cy, s, col),
        "sound" => icons::sound(c, cx, cy, s, col),
        "bt" => icons::bt(c, cx, cy, s, col),
        "usb" => icons::usb(c, cx, cy, s, col),
        "rx" => icons::rx(c, cx, cy, s, col),
        "settings" => icons::settings(c, cx, cy, s, col),
        "bookmark" => icons::bookmark(c, cx, cy, s, col),
        _ => {}
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, items: &[MenuItem]) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Menu", Some("NW-A55"));

    let rh = 63; // prototype metric (11 rows fit within the 800px panel)
    fill_rect(c, 0, y0, W as i32, 1, t.line); // top border
    for (i, m) in items.iter().enumerate() {
        let yt = y0 + i as i32 * rh;
        let cy = (yt + rh / 2) as f32;
        let icol = if m.active { t.acc } else { t.dim };
        draw_icon(c, m.icon, 33.0, cy, 19.0, icol);
        text::draw(c, f, 56.0, cy + 6.0, m.label, &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0));
        let vs = sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.04);
        let vw = text::measure(f, m.value, &vs);
        text::draw(c, f, 438.0 - vw, cy + 5.0, m.value, &vs);
        icons::chevron(c, 456.0, cy, 14.0, t.faint);
        fill_rect(c, 0, yt + rh, W as i32, 1, t.line); // bottom border
    }
}
