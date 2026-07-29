//! Sound Settings — ported from cinder-proto-screens3.jsx `CSound`. Sony DSP
//! suite: DSEE HX, Vinyl, VPT, DC Phase Linearizer, Dynamic Normalizer,
//! ClearAudio+. Footer renders the live signal path.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty, toggle};
use crate::Canvas;

/// Number of selectable rows on the Sound screen (DSEE, Vinyl, VPT, DC Phase, Normalizer, Clear+).
pub const ROWS: usize = 6;

/// Row pitch and list top — SINGLE SOURCE for the render below and `nav`'s hit test.
pub const ROW_H: i32 = 64;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// Which sound-effect row is under `y`.
pub fn row_at(y: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let r = ((y - TOP) / ROW_H) as usize;
    (r < ROWS).then_some(r)
}

pub struct Sound {
    pub dsee: bool,
    pub vinyl: bool,
    pub vpt: &'static str,     // Off / Studio / Club / Concert Hall
    pub dcphase: &'static str, // Off / Standard A.. / Low B
    pub normalizer: bool,
    pub clearaudio: bool,
    pub eq_preset: &'static str,
    pub bt_codec: Option<&'static str>,
}

/// Outlined value pill ending at `xr`; accent when value != "Off".
fn value_pill(c: &mut Canvas, f: &FontSet, t: &Theme, xr: i32, cy: i32, label: &str) {
    let active = !label.eq_ignore_ascii_case("off");
    let up = label.to_uppercase();
    let col = if active { t.acc } else { t.faint };
    let bord = if active { t.acc } else { t.line };
    let st = sty(Family::Mono, Weight::Regular, 12.0, col, 0.08);
    let w = text::measure(f, &up, &st) as i32 + 24;
    let h = 28;
    crate::widgets::stroke_rect(c, xr - w, cy - h / 2, w, h, bord, 1);
    text::draw(c, f, (xr - w + 12) as f32, (cy + 4) as f32, &up, &st);
}

fn row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool, label: &str, desc: &str) -> i32 {
    let rh = ROW_H;
    let cy = y + rh / 2;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, rh, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (cy - 3) as f32, label, &sty(Family::Sans, Weight::SemiBold, 18.0, lc, 0.0));
    text::draw(c, f, 22.0, (cy + 15) as f32, desc, &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
    hline(c, y + rh, t.line);
    cy
}

/// Simple word-wrap draw for the mono signal-path caption.
fn wrap(c: &mut Canvas, f: &FontSet, t: &Theme, x: f32, y0: f32, max_w: f32, text_s: &str) -> f32 {
    let st = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.12);
    let mut line = String::new();
    let mut y = y0;
    for word in text_s.split(' ') {
        let trial = if line.is_empty() { word.to_string() } else { format!("{} {}", line, word) };
        if text::measure(f, &trial, &st) > max_w && !line.is_empty() {
            text::draw(c, f, x, y, &line, &st);
            y += 15.0;
            line = word.to_string();
        } else {
            line = trial;
        }
    }
    if !line.is_empty() {
        text::draw(c, f, x, y, &line, &st);
        y += 15.0;
    }
    y
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, s: &Sound, sel: usize, ab_bypass: bool) {
    c.fill(t.bg);
    // No subtitle here — the A/B compare control occupies the header's right side.
    let y0 = crate::chrome::header(c, t, f, "Sound", None);

    // A/B compare control (top-right of the header): two segments, the active one in accent. B =
    // whole effect chain bypassed ("direct"), so you can instantly hear the DSP on vs off. Toggled
    // with the Option button (hinted below the segments).
    {
        let segs = [("A", !ab_bypass), ("B", ab_bypass)];
        let sw = 30;
        let sh = 26;
        let top = 44;
        let mut sx = 458 - (sw * 2 + 6);
        for (label, on) in segs.iter() {
            let st = sty(Family::Mono, Weight::Bold, 14.0, if *on { t.acc_ink } else { t.dim }, 0.1);
            if *on {
                fill_rect(c, sx, top, sw, sh, t.acc);
            }
            stroke_rect(c, sx, top, sw, sh, if *on { t.acc } else { t.line }, 1);
            let lw = text::measure(f, label, &st) as i32;
            text::draw(c, f, (sx + (sw - lw) / 2) as f32, (top + sh / 2 + 4) as f32, label, &st);
            sx += sw + 6;
        }
        // hint
        let hint = sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.14);
        right(c, f, 458.0, (top + sh + 11) as f32, "OPTION = A/B", &hint);
    }

    let rh = ROW_H;
    debug_assert_eq!(y0, TOP, "sound list top drifted from the hit test");
    hline(c, y0, t.line);
    let cy = row(c, t, f, y0, sel == 0, "DSEE HX", "Upscale compressed audio to near hi-res");
    toggle(c, t, 418, cy - 11, 40, 22, 14, s.dsee);
    let cy = row(c, t, f, y0 + rh, sel == 1, "Vinyl Processor", "Tonearm resonance + surface noise character");
    toggle(c, t, 418, cy - 11, 40, 22, 14, s.vinyl);
    let cy = row(c, t, f, y0 + rh * 2, sel == 2, "VPT Surround", "Studio / Club / Concert Hall acoustics");
    value_pill(c, f, t, 458, cy, s.vpt);
    let cy = row(c, t, f, y0 + rh * 3, sel == 3, "DC Phase Linearizer", "Analog-amp low-frequency phase response");
    value_pill(c, f, t, 458, cy, s.dcphase);
    let cy = row(c, t, f, y0 + rh * 4, sel == 4, "Dynamic Normalizer", "Even out volume between tracks");
    toggle(c, t, 418, cy - 11, 40, 22, 14, s.normalizer);
    let cy = row(c, t, f, y0 + rh * 5, sel == 5, "ClearAudio+", "Sony one-touch tuning — overrides EQ + DSP");
    toggle(c, t, 418, cy - 11, 40, 22, 14, s.clearaudio);

    // signal-path footer
    let fy = 700;
    hline(c, fy, t.line);
    let mut parts: Vec<String> = Vec::new();
    if s.dsee { parts.push("DSEE HX".into()); }
    if s.vinyl { parts.push("VINYL".into()); }
    if !s.vpt.eq_ignore_ascii_case("off") { parts.push(format!("VPT·{}", s.vpt.to_uppercase())); }
    if !s.dcphase.eq_ignore_ascii_case("off") { parts.push("DC PHASE".into()); }
    let mid = if parts.is_empty() { "DIRECT".to_string() } else { parts.join(" → ") };
    let out = match s.bt_codec {
        Some(codec) => format!("BT·{}", codec),
        None => "AMP → 3.5MM".to_string(),
    };
    // In B (bypass), the whole chain is out of the path regardless of the per-effect toggles.
    let path = if ab_bypass {
        format!("SIGNAL PATH (B/BYPASS): SOURCE → DIRECT → {}", out)
    } else {
        format!("SIGNAL PATH (A): SOURCE → EQ ({}) → {} → {}", s.eq_preset, mid, out)
    };
    let yend = wrap(c, f, t, 22.0, (fy + 22) as f32, 436.0, &path);
    if ab_bypass {
        text::draw(c, f, 22.0, yend + 8.0, "! A/B = B — EFFECT CHAIN BYPASSED (OPTION TO COMPARE)", &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.1));
    } else if s.clearaudio {
        text::draw(c, f, 22.0, yend + 8.0, "! CLEARAUDIO+ ACTIVE — EQ AND MANUAL DSP BYPASSED", &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.1));
    }
}
