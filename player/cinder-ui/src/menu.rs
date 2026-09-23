//! Menu (the hub) — ported from cinder-proto-screens3.jsx `CMenu`.
//! Twelve or thirteen rows: icon + label (17/600) + live value (mono) + chevron, each on a
//! `row_h` row with hairline separators. The list does NOT scroll, so the pitch is sized to fit
//! every row on the 800px panel — see [`row_h`].

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

/// Row pitch and list top — SINGLE SOURCE for both the render below and `nav`'s hit test.
/// 58, not the prototype's 63: the twelfth row (Folders) pushed 11x63 past the panel, and this
/// list does not scroll. Everything that positions or hit-tests a menu row derives from
/// [`row_h`], so the two move together.
pub const ROW_H: i32 = 58;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
/// The lowest y a row may reach: a few px clear of the panel's bottom edge.
const BOTTOM: i32 = crate::H as i32 - 7;

/// The pitch for `rows` rows: [`ROW_H`] when they fit, otherwise whatever does fit.
///
/// The list does not scroll, and `ROW_H` was sized for twelve rows (91 + 12 × 58 = 787). The
/// SensMe row made thirteen when that component is installed, and the thirteenth — Help &
/// Controls, the one row that explains the rest — was drawn below the glass and could never be
/// tapped (found on the owner's player, 2026-09-23). Thirteen rows get 54 px, still well above the
/// 44 px floor for a thumb target.
pub fn row_h(rows: usize) -> i32 {
    if rows == 0 {
        return ROW_H;
    }
    ROW_H.min((BOTTOM - TOP) / rows as i32)
}

/// Which menu row is under `y`, given how many rows there are.
pub fn row_at(y: i32, rows: usize) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / row_h(rows)) as usize;
    (r < rows).then_some(r)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, items: &[MenuItem]) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Menu", Some("NW-A55"));

    let rh = row_h(items.len());
    debug_assert_eq!(y0, TOP, "menu list top drifted from the hit test");
    fill_rect(c, 0, y0, W as i32, 1, t.line); // top border
    for (i, m) in items.iter().enumerate() {
        let yt = y0 + i as i32 * rh;
        let cy = (yt + rh / 2) as f32;
        let icol = if m.active { t.acc } else { t.dim };
        draw_icon(c, m.icon, 33.0, cy, 19.0, icol);
        let label_end = text::draw(c, f, 56.0, cy + 6.0, m.label,
                                   &sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, t.ink, 0.0));
        // The value is right-aligned by measuring it, so an over-long one puts its start x NEGATIVE
        // and it runs off the LEFT edge — under the icon, through the label, and off the panel.
        // Nothing here scrolls sideways, so those pixels are simply gone. The Now Playing row's
        // value is a live "Artist — Title", which is exactly the string with no length bound.
        // Truncate to the gap between the label and the chevron. Caught by tests/ui_overflow.rs.
        let vs = sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.04);
        let avail = (438.0 - (label_end + 12.0)).max(0.0);
        let value = crate::widgets::fit(f, m.value, &vs, avail);
        let vw = text::measure(f, &value, &vs);
        text::draw(c, f, 438.0 - vw, cy + 5.0, &value, &vs);
        icons::chevron(c, 456.0, cy, 14.0, t.faint);
        fill_rect(c, 0, yt + rh, W as i32, 1, t.line); // bottom border
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    /// Every row the Menu can show is ON the glass — the last row's bottom is above the panel edge
    /// and its middle is a row the hit test answers.
    #[test]
    fn every_menu_row_fits_on_the_panel() {
        for rows in 1..=14 {
            let rh = row_h(rows);
            let last_bottom = TOP + rows as i32 * rh;
            assert!(last_bottom <= crate::H as i32, "{rows} rows run to {last_bottom}");
            assert_eq!(row_at(TOP + (rows as i32 - 1) * rh + rh / 2, rows), Some(rows - 1));
            assert!(rh >= 44, "{rows} rows squeeze the pitch to {rh}");
        }
        assert_eq!(row_h(12), ROW_H, "twelve rows keep the designed pitch");
    }
}
