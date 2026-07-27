//! Equalizer — ported from cinder-proto-screens3.jsx `CEq`. Preset pills
//! (FLAT/ROCK/JAZZ/A1/A2), a 10-band slider field with a zero line, accent
//! deviation fills + knobs, dB + Hz labels, and a Reset / Save footer.

use crate::canvas::W;
use crate::data::{EQ_BANDS, EQ_PRESETS};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

// ── Layout, shared by render and the hit test ────────────────────────────────────────────────
// Everything tappable on this screen is positioned by the helpers below, and `nav` hit-tests
// through the same helpers. They used to be independent magic numbers, and they disagreed: the
// preset pills were drawn at their text width but hit-tested as five uniform 86px slots, so
// tapping "A2" selected "JAZZ" and taps on blank space past the last pill still changed the EQ.

/// Preset pill row: a uniform 5-across grid. Uniform (rather than text-width) keeps the hit test
/// exact without needing font metrics, and gives the short names ("A1"/"A2") the same big target
/// as the long ones.
pub const PRESET_TOP: i32 = crate::chrome::HEADER_BOTTOM + 6;
pub const PRESET_H: i32 = 30;
const PRESET_X0: i32 = 22;
const PRESET_GAP: i32 = 8;
const PRESET_W: i32 = (W as i32 - 2 * PRESET_X0 - 4 * PRESET_GAP) / 5;

/// `(x, y, w, h)` of preset pill `i`.
pub fn preset_rect(i: usize) -> (i32, i32, i32, i32) {
    (PRESET_X0 + i as i32 * (PRESET_W + PRESET_GAP), PRESET_TOP, PRESET_W, PRESET_H)
}

/// Which preset pill is under `(x, y)`, if any. Returns None for the gaps between pills, so a
/// miss does nothing instead of silently changing the sound.
pub fn preset_at(x: i32, y: i32) -> Option<usize> {
    if !(PRESET_TOP..PRESET_TOP + PRESET_H).contains(&y) {
        return None;
    }
    (0..EQ_PRESETS.len()).find(|&i| {
        let (px, _, pw, _) = preset_rect(i);
        (px..px + pw).contains(&x)
    })
}

/// The 10-band slider field.
pub const FIELD_TOP: i32 = 170;
pub const FIELD_BOTTOM: i32 = 470;
/// The zero line — and therefore the boundary between "tap to raise" and "tap to lower".
pub const FIELD_MID: i32 = (FIELD_TOP + FIELD_BOTTOM) / 2;
const BAND_X0: i32 = 30;
const BAND_SLOT: i32 = 420 / 10;

/// Centre x of band `i` (where its guide, knob and labels are drawn).
pub fn band_center_x(i: usize) -> i32 {
    BAND_X0 + i as i32 * BAND_SLOT + BAND_SLOT / 2
}

/// Which band column is under `x` within the field, if any.
pub fn band_at(x: i32) -> Option<usize> {
    let i = (x - BAND_X0).div_euclid(BAND_SLOT);
    (0..10).contains(&i).then_some(i as usize)
}

/// Footer controls.
pub const FOOTER_TOP: i32 = 740;
const FOOTER_H: i32 = 60;

/// A tap on the footer, if it landed on one of its two controls.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Footer {
    Reset,
    Save,
}

pub fn footer_at(x: i32, y: i32) -> Option<Footer> {
    if !(FOOTER_TOP..FOOTER_TOP + FOOTER_H).contains(&y) {
        return None;
    }
    // Split down the middle: "Reset" is left-aligned, "Save Sound Preset" right-aligned.
    Some(if x < W as i32 / 2 { Footer::Reset } else { Footer::Save })
}

fn disc(c: &mut Canvas, cx: i32, cy: i32, d: u32, col: embedded_graphics::pixelcolor::Rgb888) {
    Circle::with_center(Point::new(cx, cy), d)
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, bands: &[i8; 10], preset: &str, sel: usize) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Equalizer", None);
    // header-right pill: CUSTOM <preset>
    let plabel = format!("CUSTOM {}", preset);
    let ps = sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.12);
    let pw = text::measure(f, &plabel, &ps) as i32 + 18;
    stroke_rect(c, 458 - pw, 52, pw, 26, t.acc, 1);
    text::draw(c, f, (458 - pw + 9) as f32, 69.0, &plabel, &ps);

    // preset pills row — laid out by `preset_rect`, the same helper the hit test uses.
    for (i, (name, _)) in EQ_PRESETS.iter().enumerate() {
        let on = *name == preset;
        let st = sty(Family::Mono, Weight::Regular, 12.0, if on { t.acc_ink } else { t.dim }, 0.08);
        let (px, py, pw, ph) = preset_rect(i);
        if on {
            fill_rect(c, px, py, pw, ph, t.acc);
        }
        stroke_rect(c, px, py, pw, ph, if on { t.acc } else { t.line }, 1);
        crate::widgets::center(c, f, (px + pw / 2) as f32, (py + ph / 2 + 4) as f32, name, &st);
    }

    // slider field
    let (sy, by) = (FIELD_TOP, FIELD_BOTTOM);
    let mid = FIELD_MID;
    let span = (by - sy) / 2 - 10;
    // zero line (dashed)
    let mut dx = 30;
    while dx < 450 {
        fill_rect(c, dx, mid, 5, 1, t.line);
        dx += 11;
    }
    for i in 0..10 {
        let bx = band_center_x(i);
        let db = bands[i] as i32;
        let knob_y = mid - db * span / 10;
        // vertical guide
        fill_rect(c, bx - 1, sy, 2, by - sy, t.line);
        // deviation fill (mid → knob)
        let (fy, fh) = if knob_y < mid { (knob_y, mid - knob_y) } else { (mid, knob_y - mid) };
        fill_rect(c, bx - 1, fy, 2, fh, t.acc);
        let on = i == sel;
        // knob: bg ring + accent core (selected band gets a brighter, larger highlight ring)
        if on {
            disc(c, bx, knob_y, 22, t.ink);
        }
        disc(c, bx, knob_y, 16, t.bg);
        disc(c, bx, knob_y, if on { 12 } else { 10 }, t.acc);
        // dB label above (brighter on the selected band)
        let dbl = if db > 0 { format!("+{}", db) } else { format!("{}", db) };
        let dbcol = if on { t.ink } else if db != 0 { t.acc } else { t.faint };
        crate::widgets::center(c, f, bx as f32, (sy - 6) as f32, &dbl, &sty(Family::Mono, Weight::Regular, if on { 10.0 } else { 9.0 }, dbcol, 0.0));
        // Hz label below
        crate::widgets::center(c, f, bx as f32, (by + 22) as f32, EQ_BANDS[i], &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    }

    // footer
    let fy = FOOTER_TOP;
    hline(c, fy, t.line);
    let fcy = (fy + FOOTER_H / 2) as f32;
    text::draw(c, f, 22.0, fcy + 4.0, "Reset", &sty(Family::Sans, Weight::SemiBold, 16.0, t.dim, 0.0));
    right(c, f, 458.0, fcy + 4.0, "Save Sound Preset", &sty(Family::Sans, Weight::Bold, 16.0, t.acc, 0.0));
    let _ = W;
}
