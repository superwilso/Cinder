//! Settings ▸ Display — how the screen looks (design handoff 5k).
//!
//! Everything about colour, the volume readout and text size, on one page that fits the glass:
//!
//!   COLOUR   Palette ›   Accent (six swatches)   Night (switch)
//!   VOLUME   Full | Minimal
//!   TEXT     Size (slider)   Visualiser ›
//!
//! These five rows were the top of the Settings list, where they shared one scrolling column with
//! Restart and Reset. The handoff's point 4 is that settings you change weekly and settings you
//! change once should not share a list; Settings now carries one "Display" row that says what is
//! behind it.
//!
//! Two deliberate departures from the mock, both keeping a control that is better than the row it
//! was drawn as: Accent keeps its six swatches (the mock draws a value and a chevron, but no picker
//! screen was designed, and six direct targets beat a list you open to choose from six), and Size
//! keeps its slider (the same reasoning — the mock's chevron leads nowhere yet).
//!
//! The page does not scroll, so the layout is a fixed table: `row_top` is the single source both
//! `render` and the hit tests read.

use crate::canvas::{Canvas, W};
use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::{Accent, Theme};
use crate::widgets::{fill_rect, right, stroke_rect, sty};

pub const ROW_PALETTE: usize = 0;
pub const ROW_ACCENT: usize = 1;
pub const ROW_NIGHT: usize = 2;
/// The volume HUD chips (Full | Minimal). A row for the cursor's sake; the chips are the targets.
pub const ROW_VOLUME: usize = 3;
pub const ROW_SIZE: usize = 4;
pub const ROW_VIZ: usize = 5;
pub const ROWS: usize = 6;

/// The volume HUD styles, in chip order. Index = `App::volume_hud`.
pub const VOLUME_HUDS: [&str; 2] = ["Full", "Minimal"];

/// Section labels and the rows under each. The ONE statement of the layout.
const SECTIONS: [(&str, usize); 3] = [("COLOUR", 3), ("VOLUME", 1), ("TEXT", 2)];

/// Screen-y of the top of row `r`.
pub fn row_top(r: usize) -> i32 {
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut i = 0;
    for (_, n) in SECTIONS {
        y += kit::SECTION_H;
        for _ in 0..n {
            if i == r {
                return y;
            }
            y += kit::ROW_H;
            i += 1;
        }
    }
    y
}

/// The row under `y`, if any.
pub fn row_at(y: i32) -> Option<usize> {
    (0..ROWS).find(|&r| (row_top(r)..row_top(r) + kit::ROW_H).contains(&y))
}

/// Top of the volume chips, centred in their row.
fn chips_top() -> i32 {
    row_top(ROW_VOLUME) + (kit::ROW_H - kit::CHIP_H) / 2
}

/// Which volume HUD chip is under `(x, y)`.
pub fn volume_chip_at(x: i32, y: i32) -> Option<usize> {
    kit::chip_at(VOLUME_HUDS.len(), chips_top(), x, y)
}

// ── Accent swatches ────────────────────────────────────────────────────────────────────────────
const SW: i32 = 30;
const SW_GAP: i32 = 6;
fn swatch_x(i: usize) -> i32 {
    let total = Accent::COUNT as i32 * SW + (Accent::COUNT as i32 - 1) * SW_GAP;
    kit::RIGHT - total + i as i32 * (SW + SW_GAP)
}

/// Which accent swatch is under `(x, y)`. The whole row height counts vertically, and half the gap
/// on each side counts as the swatch, so a near miss still lands on the colour it was aimed at.
pub fn accent_hit(x: i32, y: i32) -> Option<usize> {
    let top = row_top(ROW_ACCENT);
    if !(top..top + kit::ROW_H).contains(&y) {
        return None;
    }
    (0..Accent::COUNT).find(|&i| {
        let sx = swatch_x(i);
        x >= sx - SW_GAP / 2 && x < sx + SW + SW_GAP / 2
    })
}

// ── Size slider ────────────────────────────────────────────────────────────────────────────────
// Tap a stop or drag; the readout is drawn at a CONSTANT pixel size because this is the control
// that sets the scale (see the note in `render`).
const SLIDER_X0: i32 = 176;
const SLIDER_W: i32 = 196;

/// Map an x on the Size row to a `text::SCALE_STEPS` index, clamped at both ends.
pub fn size_idx_at(x: i32) -> usize {
    let n = text::SCALE_STEPS.len() as i32;
    let dx = (x - SLIDER_X0).clamp(0, SLIDER_W);
    ((dx * (n - 1) * 2 + SLIDER_W) / (SLIDER_W * 2)).clamp(0, n - 1) as usize
}

/// What the page shows. Built by `nav`.
pub struct DisplayView<'a> {
    /// The palette being drawn, by name.
    pub palette: &'a str,
    /// The palette brings its own accent, so the swatches would pick nothing.
    pub accent_locked: bool,
    pub accent: Accent,
    pub night: bool,
    /// Index into [`VOLUME_HUDS`].
    pub volume_hud: u8,
    /// "BARS · VEIL".
    pub viz: &'a str,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, v: &DisplayView) {
    c.fill(t.bg);
    let mut y = crate::chrome::header(c, t, f, "Display", None);
    let mut r = 0usize;
    for (label, n) in SECTIONS {
        y = kit::section_label(c, t, f, y, label, None);
        for _ in 0..n {
            debug_assert_eq!(y, row_top(r), "display row {r} drifted from its hit test");
            y = draw_row(c, t, f, y, r, sel == r, v);
            r += 1;
        }
    }
}

fn draw_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, r: usize, sel: bool, v: &DisplayView) -> i32 {
    match r {
        ROW_PALETTE => {
            let name = v.palette.to_uppercase();
            let next = kit::row(c, t, f, y, kit::ROW_H, &Row::new("Palette").trail(Trail::Open(&name)).sel(sel));
            // The palette's own colours beside its name: background, line, dim, ink, accent — the
            // five a palette file actually changes, as 10 x 18 cells.
            let vst = sty(Family::Mono, Weight::Regular, crate::scale::CAPTION, t.faint, 0.1);
            let vx = kit::RIGHT - 20 - text::measure(f, &name, &vst) as i32 - 14;
            let cells = [t.bg, t.line, t.dim, t.ink, t.acc];
            let x0 = vx - cells.len() as i32 * 10;
            let cy = y + kit::ROW_H / 2;
            for (i, col) in cells.iter().enumerate() {
                fill_rect(c, x0 + i as i32 * 10, cy - 9, 10, 18, *col);
            }
            stroke_rect(c, x0, cy - 9, cells.len() as i32 * 10, 18, t.ctrl(), 1);
            next
        }
        ROW_ACCENT => {
            if v.accent_locked {
                return kit::row(c, t, f, y, kit::ROW_H,
                    &Row::new("Accent").sub("Set by the palette").trail(Trail::Value("—")).sel(sel));
            }
            // "Amber", not the chip-case "AMBER": it is a second line of prose, not a value.
            let upper = v.accent.name();
            let mut name = upper[..1].to_string();
            name.push_str(&upper[1..].to_lowercase());
            let next = kit::row(c, t, f, y, kit::ROW_H, &Row::new("Accent").sub(&name).sel(sel));
            let cy = y + kit::ROW_H / 2;
            for (i, a) in Accent::ALL.iter().enumerate() {
                let sx = swatch_x(i);
                let sy = cy - SW / 2;
                // Through the theme's dim: a swatch is a raw palette entry, not a theme colour.
                fill_rect(c, sx, sy, SW, SW, t.scale_color(a.swatch(t.night)));
                if *a == v.accent {
                    // Ink, not accent: on BONE the swatch already is near-ink.
                    stroke_rect(c, sx - 3, sy - 3, SW + 6, SW + 6, t.ink, 2);
                }
            }
            next
        }
        ROW_NIGHT => kit::row(c, t, f, y, kit::ROW_H,
            &Row::new("Night").sub("Dims everything after dark").trail(Trail::Switch(v.night)).sel(sel)),
        ROW_VOLUME => {
            if sel {
                fill_rect(c, 0, y, W as i32, kit::ROW_H, t.row_sel);
            }
            kit::chips(c, t, f, chips_top(), &VOLUME_HUDS, Some(v.volume_hud as usize));
            crate::widgets::hline(c, y + kit::ROW_H - 1, t.line);
            y + kit::ROW_H
        }
        ROW_SIZE => size_row(c, t, f, y, sel),
        _ => kit::row(c, t, f, y, kit::ROW_H, &Row::new("Visualiser").trail(Trail::Open(v.viz)).sel(sel)),
    }
}

fn size_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool) -> i32 {
    let next = kit::row(c, t, f, y, kit::ROW_H, &Row::new("Size").sel(sel));
    let cy = y + kit::ROW_H / 2;
    let n = text::SCALE_STEPS.len() as i32;
    let idx = text::scale_idx() as i32;
    fill_rect(c, SLIDER_X0, cy - 1, SLIDER_W, 2, t.line);
    for i in 0..n {
        let x = SLIDER_X0 + i * SLIDER_W / (n - 1);
        fill_rect(c, x - 1, cy - 4, 2, 8, if i <= idx { t.acc } else { t.line });
    }
    let kx = SLIDER_X0 + idx * SLIDER_W / (n - 1);
    fill_rect(c, SLIDER_X0, cy - 1, kx - SLIDER_X0, 2, t.acc);
    fill_rect(c, kx - 7, cy - 9, 14, 18, t.acc);
    // A CONSTANT pixel size: this is the control that sets the scale, so letting the readout grow
    // with it would crowd the knob at 140% and move the slider's own geometry.
    let unscaled = 14.0 * 100.0 / text::scale_pct() as f32;
    right(c, f, kit::RIGHT as f32, (cy + 5) as f32, &format!("{}%", text::scale_pct()),
          &sty(Family::Mono, Weight::Regular, unscaled, t.faint, 0.04));
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page fits above the Now Playing bar without scrolling.
    #[test]
    fn the_page_fits_above_the_now_playing_bar() {
        let bottom = row_top(ROWS - 1) + kit::ROW_H;
        assert!(bottom <= crate::H as i32 - crate::chrome::NP_BAR_H, "Display runs to {bottom}");
    }

    /// Every row's middle is that row, and the gaps (section labels) are nobody's.
    #[test]
    fn every_row_is_hittable_and_labels_are_not() {
        for r in 0..ROWS {
            assert_eq!(row_at(row_top(r) + kit::ROW_H / 2), Some(r));
        }
        assert_eq!(row_at(crate::chrome::HEADER_BOTTOM + 5), None, "the first section label");
    }

    /// Every swatch is hittable at its centre and picks itself.
    #[test]
    fn swatches_pick_themselves() {
        let cy = row_top(ROW_ACCENT) + kit::ROW_H / 2;
        for i in 0..Accent::COUNT {
            assert_eq!(accent_hit(swatch_x(i) + SW / 2, cy), Some(i));
        }
        assert_eq!(accent_hit(40, cy), None, "the title is not a swatch");
    }

    /// The chips sit inside the Volume row.
    #[test]
    fn the_volume_chips_are_inside_their_row() {
        let top = row_top(ROW_VOLUME);
        assert!(chips_top() >= top && chips_top() + kit::CHIP_H <= top + kit::ROW_H);
        assert_eq!(volume_chip_at(60, chips_top() + 10), Some(0));
        assert_eq!(volume_chip_at(420, chips_top() + 10), Some(1));
    }

    /// The slider pins to its ends and lands on every stop.
    #[test]
    fn the_size_slider_covers_every_stop() {
        let n = text::SCALE_STEPS.len();
        assert_eq!(size_idx_at(0), 0);
        assert_eq!(size_idx_at(W as i32), n - 1);
        for i in 0..n {
            let x = SLIDER_X0 + i as i32 * SLIDER_W / (n as i32 - 1);
            assert_eq!(size_idx_at(x), i);
        }
    }
}
