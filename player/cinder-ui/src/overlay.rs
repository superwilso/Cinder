//! Transient overlays drawn on top of the current screen (volume HUD). The navigator holds a
//! frame countdown (`vol_overlay`) decremented each `App::tick`; while it's > 0 the volume HUD
//! is drawn over whatever screen is current — exactly the daily "press Vol± → see the level"
//! interaction. Volume is the stock 0..120 step scale (CXD3778GF 'master volume'), shown as
//! "N / 120" like the stock player.

use crate::canvas::{Canvas, H, W};
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};

/// Stock hardware volume is 0..120 steps (HAGOROMO_DEFAULT_VOLUME_MAX; ALSA card0
/// 'master volume' range) — the UI level maps 1:1 onto the mixer.
pub const VOL_MAX: u8 = 120;

/// How many pump frames the volume HUD stays up after the last change (~1.6 s at the 60 fps
/// pump; the exact pump rate is the shell's, this is just "a moment").
pub const VOL_FRAMES: u8 = 96;

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
        &sty(Family::Mono, Weight::Regular, 12.0, t.dim, 0.18));
    // numeric step value "N / 120", right-aligned (stock shows the raw 0..120 level)
    let val = format!("{level}");
    let vst = sty(Family::Mono, Weight::Bold, 24.0, t.ink, 0.0);
    let mst = sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0);
    let max_lbl = format!(" / {VOL_MAX}");
    let vw = text::measure(f, &val, &vst);
    let mw = text::measure(f, &max_lbl, &mst);
    text::draw(c, f, (x0 + slab_w - 28) as f32 - mw - vw, (y0 + 34) as f32, &val, &vst);
    text::draw(c, f, (x0 + slab_w - 28) as f32 - mw, (y0 + 34) as f32, &max_lbl, &mst);

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
        &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}

/// Confirmation toast: a bottom-centered pill with a one-line message (e.g. after swipe-to-queue).
/// Sized to the text; long titles are clipped by the pill edge rather than wrapped.
pub fn toast(c: &mut Canvas, t: &Theme, f: &FontSet, msg: &str) {
    let st = sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0);
    let tw = text::measure(f, msg, &st).min((W - 64) as f32);
    let pw = (tw as i32 + 44).min(W as i32 - 20);
    let ph = 44;
    let x0 = (W as i32 - pw) / 2;
    let y0 = H as i32 - 96;
    fill_rect(c, x0, y0, pw, ph, t.panel);
    for (bx, by, bw, bh) in [
        (x0, y0, pw, 1),
        (x0, y0 + ph - 1, pw, 1),
        (x0, y0, 1, ph),
        (x0 + pw - 1, y0, 1, ph),
    ] {
        fill_rect(c, bx, by, bw, bh, t.acc);
    }
    text::draw(c, f, (x0 + 22) as f32, (y0 + 28) as f32, msg, &st);
}

/// Swipe-to-queue chip: a "+ QUEUED" pill anchored on the swiped row that slides off the right
/// edge with an ease-in, echoing the rightward flick that queued the song. `progress` runs
/// 1.0 → 0.0 (frames left / total); the pill is fully off-screen by 0. No alpha compositing
/// needed — the exit off the edge *is* the fade.
pub fn queue_chip(c: &mut Canvas, t: &Theme, f: &FontSet, row_y: i32, progress: f32) {
    let p = progress.clamp(0.0, 1.0);
    let pw = 128;
    let ph = 34;
    let x0 = 330 + (((1.0 - p) * (1.0 - p)) * (W as f32 - 330.0)) as i32;
    let y0 = (row_y - ph / 2).clamp(0, H as i32 - ph);
    fill_rect(c, x0, y0, pw, ph, t.acc);
    text::draw(c, f, (x0 + 16) as f32, (y0 + 23) as f32, "+ QUEUED",
        &sty(Family::Mono, Weight::Bold, 14.0, t.acc_ink, 0.12));
}
