//! Settings — interactive, and now SCROLLABLE. Up/Down move the cursor; Select acts on the
//! focused row. Rows: DISPLAY (Theme, UI scale, Visualiser type, Visualiser animation, Sleep
//! timer, Screen-off, Brightness), SYSTEM (Storage, Database, Battery care, USB mode),
//! ABOUT (Firmware, Model).
//!
//! **Why it scrolls now.** The layout is 13 rows × 56px plus three section eyebrows = 919px of
//! content on an 800px panel. Even at the previous 12 rows it was 863px: "Firmware" was drawn
//! half off the bottom edge and "Model" was drawn *entirely* off-screen, and because `row_at`
//! mirrored the same arithmetic those rows were untappable while the button cursor could still
//! move onto them (the screen then just looked frozen). The list is now pixel-scrolled like the
//! library lists, and `layout()` is the single source of truth both `render` and `row_at` read,
//! so a row can never be drawn somewhere the hit test doesn't look.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;

/// Number of selectable rows (for nav cursor clamping).
pub const ROWS: usize = 13;

// Actionable / addressable rows. These are symbolic everywhere (nav + tests), so inserting a row
// only means renumbering here.
pub const ROW_THEME: usize = 0;
pub const ROW_UI_SCALE: usize = 1;
pub const ROW_VIZ: usize = 2;
pub const ROW_VIZ_ANIM: usize = 3;
pub const ROW_SLEEP: usize = 4;
pub const ROW_SCREEN_OFF: usize = 5;
pub const ROW_BRIGHTNESS: usize = 6;
pub const ROW_STORAGE: usize = 7;
pub const ROW_DATABASE: usize = 8;
pub const ROW_BATTERY: usize = 9;
pub const ROW_USB_MODE: usize = 10; // tapping enters USB mass-storage (file transfer to a PC)
pub const ROW_FIRMWARE: usize = 11;
pub const ROW_MODEL: usize = 12;

const RH: i32 = 56;
const EYEBROW_H: i32 = 24;
const SECTION_GAP: i32 = 14;
/// Screen y where the scrollable list area starts (directly under the header).
pub const LIST_TOP: i32 = 91;
/// Screen y where it ends. Matches the library lists' bottom so the shared `scrollbar()`
/// (which measures its track against that same constant) lines up exactly.
pub const LIST_BOTTOM: i32 = crate::canvas::H as i32 - 12;

/// Firmware/build label shown on the Settings "Firmware" row. The `dev` feature (development
/// channel, built from the same tree) makes the two builds visually distinguishable on-device.
#[cfg(feature = "dev")]
pub const FIRMWARE_LABEL: &str = "CINDER DEV · RUST";
#[cfg(not(feature = "dev"))]
pub const FIRMWARE_LABEL: &str = "CINDER 1.0 · RUST";

/// Current settings values to display.
pub struct SettingsView<'a> {
    pub night: bool,
    pub viz_name: &'a str,
    pub viz_on: bool,
    pub usb_dac: bool,
    pub battery_care: bool,
    pub storage: &'a str,
    pub sleep: &'a str,
}

/// One entry in the settings display list.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Item {
    Eyebrow(&'static str),
    Row(usize),
}

/// The display list: (content-space top y, item). One source of truth for `render` and `row_at`.
pub fn layout() -> Vec<(i32, Item)> {
    let sections: [(&'static str, std::ops::Range<usize>); 3] = [
        ("DISPLAY", ROW_THEME..ROW_STORAGE),
        ("SYSTEM", ROW_STORAGE..ROW_FIRMWARE),
        ("ABOUT", ROW_FIRMWARE..ROWS),
    ];
    let mut out = Vec::with_capacity(ROWS + 3);
    let mut y = 0;
    for (i, (label, rows)) in sections.into_iter().enumerate() {
        if i > 0 {
            y += SECTION_GAP;
        }
        out.push((y, Item::Eyebrow(label)));
        y += EYEBROW_H;
        for r in rows {
            out.push((y, Item::Row(r)));
            y += RH;
        }
    }
    out
}

/// Total content height in px.
pub fn content_h() -> i32 {
    layout()
        .last()
        .map(|&(y, it)| y + if matches!(it, Item::Eyebrow(_)) { EYEBROW_H } else { RH })
        .unwrap_or(0)
}

/// Largest useful scroll offset (0 when everything fits).
pub fn max_scroll_px() -> i32 {
    (content_h() - (LIST_BOTTOM - LIST_TOP)).max(0)
}

/// Content-space top y of row `r` — used to keep the button cursor on screen.
pub fn row_top_px(r: usize) -> i32 {
    layout().iter().find(|(_, it)| *it == Item::Row(r)).map(|&(y, _)| y).unwrap_or(0)
}

/// Height of a settings row (nav needs it to scroll the cursor into view).
pub const fn row_h() -> i32 {
    RH
}

/// Which selectable row is at screen-y `y` for a list scrolled by `scroll_px`?
/// Returns None for a gap, an eyebrow, or outside the list area.
pub fn row_at(y: i32, scroll_px: i32) -> Option<usize> {
    if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let cy = y - LIST_TOP + scroll_px; // content space
    layout().into_iter().find_map(|(top, it)| match it {
        Item::Row(r) if (top..top + RH).contains(&cy) => Some(r),
        _ => None,
    })
}

// ── UI scale slider ─────────────────────────────────────────────────────────────────────────
// A real slider (track + detents + knob), not a value that cycles on tap: tapping anywhere on
// the track jumps to that stop, and dragging it scrubs live (nav routes the gesture through
// `App::scrub_*`). Left/Right on the buttons step one stop.
// The track stops short of the right edge to reserve a gutter for the "NNN%" readout — which is
// itself drawn at the current scale, so at 140% it is ~55px wide and would otherwise sit on top
// of the knob.
const SLIDER_X0: i32 = 176;
const SLIDER_W: i32 = 196;

/// Map a tap/drag x on the UI-scale row to a `text::SCALE_STEPS` index. x is clamped, so
/// grabbing past either end pins to the min/max stop rather than doing nothing.
pub fn ui_scale_idx_at(x: i32) -> usize {
    let n = text::SCALE_STEPS.len() as i32;
    let dx = (x - SLIDER_X0).clamp(0, SLIDER_W);
    // Round to the nearest stop so the knob lands under the finger.
    ((dx * (n - 1) * 2 + SLIDER_W) / (SLIDER_W * 2)).clamp(0, n - 1) as usize
}

fn slider_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str) {
    let cy = y + RH / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));

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
    // The readout is drawn at a CONSTANT pixel size: it is the control that sets the scale, so
    // letting it grow with the scale both crowded the knob at 140% and made this one row the
    // widest thing on the screen. Compensating here keeps the slider's geometry identical at
    // every stop (and the hit test, `ui_scale_idx_at`, is scale-independent to match).
    let unscaled = 14.0 * 100.0 / text::scale_pct() as f32;
    right(c, f, 458.0, (cy + 4) as f32, &format!("{}%", text::scale_pct()),
          &sty(Family::Mono, Weight::Regular, unscaled, t.faint, 0.04));
    hline(c, y + RH, t.line);
}

fn eyebrow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) {
    text::draw(c, f, 22.0, (y + 14) as f32, label, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
}

/// A label/value row; highlights when selected.
fn srow(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str, value: &str, chevron: bool) {
    let cy = y + RH / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, label, &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    let vx = if chevron { 438.0 } else { 458.0 };
    right(c, f, vx, (cy + 4) as f32, value, &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.04));
    if chevron {
        icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
    }
    hline(c, y + RH, t.line);
}

/// The Theme row's day/night segmented control (its own draw — it isn't a label/value row).
fn theme_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, night: bool) {
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, RH, t.row_sel);
    }
    hline(c, y, t.line);
    let cy = y + RH / 2;
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy + 5) as f32, "Theme", &sty(Family::Sans, Weight::SemiBold, 20.0, lc, 0.0));
    let segs = [("DAY", !night), ("NIGHT", night)];
    let sh = 26;
    let mut widths = [0i32; 2];
    for (i, (label, _)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.1);
        widths[i] = text::measure(f, label, &st) as i32 + 26;
    }
    let total = widths[0] + widths[1] + 8;
    let mut sx = 458 - total;
    for (i, (label, on)) in segs.iter().enumerate() {
        let st = sty(Family::Mono, Weight::Regular, 12.0, if *on { t.acc_ink } else { t.dim }, 0.1);
        if *on {
            fill_rect(c, sx, cy - sh / 2, widths[i], sh, t.acc);
        }
        stroke_rect(c, sx, cy - sh / 2, widths[i], sh, if *on { t.acc } else { t.line }, 1);
        text::draw(c, f, (sx + 13) as f32, (cy + 4) as f32, label, &st);
        sx += widths[i] + 8;
    }
    hline(c, y + RH, t.line);
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, scroll_px: i32, v: &SettingsView) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f);
    crate::chrome::header(c, t, f, "Settings", None);

    let scroll = scroll_px.clamp(0, max_scroll_px());
    // Clip to the list area so a partially-scrolled row can't paint over the header.
    c.set_clip_y(LIST_TOP, LIST_BOTTOM);
    for (top, item) in layout() {
        let y = LIST_TOP + top - scroll;
        if y > LIST_BOTTOM || y + RH < LIST_TOP {
            continue; // fully outside the window
        }
        match item {
            Item::Eyebrow(label) => eyebrow(c, t, f, y, label),
            Item::Row(ROW_THEME) => theme_row(c, t, f, y, sel == ROW_THEME, v.night),
            Item::Row(ROW_UI_SCALE) => slider_row(c, t, f, y, sel == ROW_UI_SCALE, "UI scale"),
            Item::Row(r) => {
                let (label, value, chev): (&str, &str, bool) = match r {
                    ROW_VIZ => ("Visualiser type", v.viz_name, false),
                    ROW_VIZ_ANIM => ("Visualiser", if v.viz_on { "ON" } else { "OFF" }, false),
                    ROW_SLEEP => ("Sleep timer", v.sleep, false),
                    ROW_SCREEN_OFF => ("Screen-off timer", "30 SEC", false),
                    ROW_BRIGHTNESS => ("Brightness", "3 / 5", false),
                    ROW_STORAGE => ("Storage", v.storage, false),
                    ROW_DATABASE => ("Database", "REBUILD", true),
                    ROW_BATTERY => ("Battery care", if v.battery_care { "ON · 90%" } else { "OFF" }, false),
                    ROW_USB_MODE => ("USB mode", if v.usb_dac { "DAC" } else { "MASS STORAGE" }, true),
                    ROW_FIRMWARE => ("Firmware", FIRMWARE_LABEL, false),
                    _ => ("Model", "SONY NW-A55", false),
                };
                srow(c, t, f, y, sel == r, label, value, chev);
            }
        }
    }
    c.clear_clip();
    if max_scroll_px() > 0 {
        crate::library::scrollbar(c, t, LIST_TOP, scroll, content_h());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_reachable_by_scrolling() {
        // The regression this guards: the ABOUT rows used to be laid out past y=800 with no
        // scroll, so they were drawn off-panel AND `row_at` could never return them.
        let max = max_scroll_px();
        let view_h = LIST_BOTTOM - LIST_TOP;
        for r in 0..ROWS {
            let top = row_top_px(r);
            // The same "scroll the cursor into view" arithmetic nav uses.
            let scroll = (top + RH - view_h).max(0).min(top).min(max);
            let mid = LIST_TOP + top - scroll + RH / 2;
            assert!((LIST_TOP..LIST_BOTTOM).contains(&mid), "row {r} off-panel at scroll {scroll}");
            assert_eq!(row_at(mid, scroll), Some(r), "row {r} unreachable at scroll {scroll}");
        }
    }

    #[test]
    fn ui_scale_slider_spans_every_stop() {
        let n = text::SCALE_STEPS.len();
        assert_eq!(ui_scale_idx_at(SLIDER_X0 - 40), 0); // clamps left
        assert_eq!(ui_scale_idx_at(SLIDER_X0 + SLIDER_W + 40), n - 1); // clamps right
        for i in 0..n {
            let x = SLIDER_X0 + (i as i32) * SLIDER_W / (n as i32 - 1);
            assert_eq!(ui_scale_idx_at(x), i, "stop {i} not selectable at its detent");
        }
    }

    #[test]
    fn layout_has_one_entry_per_row() {
        let rows: Vec<usize> = layout()
            .into_iter()
            .filter_map(|(_, it)| match it {
                Item::Row(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(rows, (0..ROWS).collect::<Vec<_>>());
    }
}
