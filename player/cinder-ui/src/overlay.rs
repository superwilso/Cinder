//! Transient overlays drawn on top of the current screen (volume HUD). The navigator holds a
//! frame countdown (`vol_overlay`) decremented each `App::tick`; while it's > 0 the volume HUD
//! is drawn over whatever screen is current — exactly the daily "press Vol± → see the level"
//! interaction. Volume is the Sony 0..30 step scale; we also show it as a percentage.

use crate::canvas::{Canvas, H, W};
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};

/// Sony hardware volume is 0..30 steps.
pub const VOL_MAX: u8 = 30;

/// How many pump frames the volume HUD stays up after the last change (~1.6 s at 30 fps; the
/// exact pump rate is the shell's, this is just "a moment").
pub const VOL_FRAMES: u8 = 48;

/// Draw the volume HUD: a centered slab with a speaker icon, a level bar, and the step value.
pub fn volume(c: &mut Canvas, t: &Theme, f: &FontSet, level: u8) {
    let level = level.min(VOL_MAX);
    let slab_w = 320;
    let slab_h = 96;
    let x0 = (W as i32 - slab_w) / 2;
    let y0 = (H as i32 - slab_h) / 2;

    // backing slab (panel tone) + 1px accent-tinted border
    fill_rect(c, x0, y0, slab_w, slab_h, t.panel);
    for (bx, by, bw, bh) in [
        (x0, y0, slab_w, 1),
        (x0, y0 + slab_h - 1, slab_w, 1),
        (x0, y0, 1, slab_h),
        (x0 + slab_w - 1, y0, 1, slab_h),
    ] {
        fill_rect(c, bx, by, bw, bh, t.line);
    }

    // speaker icon + "VOLUME" eyebrow
    let muted = level == 0;
    icons::sound(c, (x0 + 34) as f32, (y0 + 34) as f32, 22.0, if muted { t.faint } else { t.acc });
    text::draw(c, f, (x0 + 58) as f32, (y0 + 30) as f32, "VOLUME",
        &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.18));
    // numeric step value, right-aligned
    let val = format!("{level}");
    let vst = sty(Family::Mono, Weight::Bold, 22.0, t.ink, 0.0);
    let vw = text::measure(f, &val, &vst);
    text::draw(c, f, (x0 + slab_w - 28) as f32 - vw, (y0 + 34) as f32, &val, &vst);

    // level bar
    let bx = x0 + 34;
    let by = y0 + 56;
    let bw = slab_w - 68;
    let bh = 8;
    fill_rect(c, bx, by, bw, bh, t.line);
    let filled = (bw as f32 * (level as f32 / VOL_MAX as f32)).round() as i32;
    if filled > 0 {
        fill_rect(c, bx, by, filled, bh, if muted { t.faint } else { t.acc });
    }
    // percentage caption under the bar
    let pct = (level as u32 * 100 / VOL_MAX as u32).min(100);
    text::draw(c, f, bx as f32, (by + 24) as f32, &format!("{pct}%"),
        &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.1));
}
