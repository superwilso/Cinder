//! Sound Settings — ported from cinder-proto-screens3.jsx `CSound`. Sony DSP
//! suite: DSEE HX, Vinyl, VPT, DC Phase Linearizer, Dynamic Normalizer,
//! ClearAudio+. Footer renders the live signal path.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, right, stroke_rect, sty, toggle};
use crate::Canvas;

/// Number of selectable rows on the Sound screen (DSEE, Vinyl, VPT, DC Phase, Normalizer, Clear+,
/// Balance).
///
/// "High gain output" USED to sit between Clear+ and Balance. It was cut on 2026-08-17 after being
/// measured on the device: the control (`headphone smaster gain mode` / `headphone smaster se gain
/// mode`, numid 28/29) accepts `high`, reads back 1 and persists across a reboot — but the codec
/// does nothing with it and the output is unchanged by ear. The A50's output stage simply doesn't
/// have the high-gain hardware the ZX/WM1 series does; the mixer control is inherited from the
/// shared CXD3778GF driver. Kept here as a note so it isn't "discovered" and re-added: the write
/// landing is NOT evidence the feature works. See task #59.
pub const ROWS: usize = 7;
pub const ROW_BALANCE: usize = 6;

/// L/R balance, 0..=100 with 50 = centre — a continuous drag slider, not the 7 discrete stops it
/// started as. Left of centre attenuates the RIGHT channel and vice versa: panning left means the
/// left is louder, which is done by turning the other side down, since the mixer only offers
/// attenuation.
pub const BALANCE_CENTRE: usize = 50;
pub const BALANCE_MAX: usize = 100;

/// Label for a balance position: "Centre", "L 24", "R 12" — the number is distance from centre in
/// slider units (0..=50), so it reads the same on both sides.
pub fn balance_label(pos: usize) -> String {
    let i = pos.min(BALANCE_MAX);
    match i.cmp(&BALANCE_CENTRE) {
        std::cmp::Ordering::Equal => "Centre".to_string(),
        std::cmp::Ordering::Less => format!("L {}", BALANCE_CENTRE - i),
        std::cmp::Ordering::Greater => format!("R {}", i - BALANCE_CENTRE),
    }
}

/// Row pitch and list top — SINGLE SOURCE for the render below and `nav`'s hit test.
pub const ROW_H: i32 = 64;
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The Balance row is taller than the rest: it carries a full-width drag slider, a Centre reset
/// button and a readout, and a 64 px row would leave the track sharing an edge with ClearAudio+
/// above it — the same near-miss that made the library filter strip hard to hit.
pub const BALANCE_ROW_H: i32 = 132;

/// Top edge of the Balance row. Everything below it is slider.
pub fn balance_top() -> i32 {
    TOP + ROW_H * ROW_BALANCE as i32
}

/// Which sound-effect row is under `y`. The last row is taller, so this cannot be a plain divide.
pub fn row_at(y: i32) -> Option<usize> {
    if y < TOP {
        return None;
    }
    let bal = balance_top();
    if y >= bal {
        return (y < bal + BALANCE_ROW_H).then_some(ROW_BALANCE);
    }
    let r = ((y - TOP) / ROW_H) as usize;
    (r < ROW_BALANCE).then_some(r)
}

// ── Balance slider geometry — SINGLE SOURCE for the render and the drag hit test ────────────────
/// Track ends. Full width minus the standard 22 px gutter, so the grab target is the whole screen
/// width — this is a touch-only device and the knob is small.
pub const BAL_X0: i32 = 22;
pub const BAL_X1: i32 = 458;
/// Vertical centre of the track within the Balance row.
pub const BAL_TRACK_DY: i32 = 86;

/// "Centre" reset button, top-right of the Balance row. A slider needs a way back to its null that
/// isn't "drag carefully until the number reads 50" — on a 436 px track one slider step is 4.4 px,
/// so hitting exactly centre by hand is luck. Select does it too, but this is the touch path.
pub const BAL_RESET_W: i32 = 104;
pub const BAL_RESET_H: i32 = 38;
pub fn balance_reset_rect() -> (i32, i32, i32, i32) {
    (BAL_X1 - BAL_RESET_W, balance_top() + 12, BAL_RESET_W, BAL_RESET_H)
}
pub fn hit_balance_reset(x: i32, y: i32) -> bool {
    let (rx, ry, rw, rh) = balance_reset_rect();
    (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y)
}

/// Slider steps within which a drag snaps to dead centre. Without it, "centred" is a 1-in-101 shot
/// and the control has no null you can actually land on — the detent tick would be decoration.
pub const BAL_SNAP: usize = 3;
pub fn bal_track_y() -> i32 {
    balance_top() + BAL_TRACK_DY
}

/// Slider position (0..=100) for a finger at `x`. Clamped, so a drag that runs off either end
/// pins to the stop rather than stopping tracking.
pub fn balance_at(x: i32) -> usize {
    let span = (BAL_X1 - BAL_X0).max(1);
    let t = (x - BAL_X0).clamp(0, span);
    let pos = ((t as i64 * BALANCE_MAX as i64 + span as i64 / 2) / span as i64) as usize;
    // Snap through the detent, so centre is reachable with a normal finger.
    if pos.abs_diff(BALANCE_CENTRE) <= BAL_SNAP { BALANCE_CENTRE } else { pos }
}

/// Screen x of the knob for position `pos`.
pub fn balance_x(pos: usize) -> i32 {
    let span = BAL_X1 - BAL_X0;
    BAL_X0 + (pos.min(BALANCE_MAX) as i32 * span) / BALANCE_MAX as i32
}

/// Does `y` fall in the slider's grab band? Deliberately taller than the track: a 4 px line is not
/// a touch target, and the whole lower half of the row is dead space otherwise.
pub fn balance_grab(y: i32) -> bool {
    let ty = bal_track_y();
    (ty - 30..=ty + 34).contains(&y)
}

// ── A/B compare control ─────────────────────────────────────────────────────────────────────────
/// Two segments in the header. They were 30x26 with a 26 px hit band and reported hard to press on
/// 2026-08-17; a 26 px target is under every touch guideline and this one sits in a corner, where a
/// thumb is least accurate. Now 48x44 with a band that overhangs it, and A/B is the SINGLE SOURCE
/// for both the render and nav's hit test — they used to be written out separately, which is how
/// the hit band ended up 4 px shorter than the thing it was meant to cover.
pub const AB_W: i32 = 48;
pub const AB_H: i32 = 44;
pub const AB_TOP: i32 = 34;
pub const AB_GAP: i32 = 8;
pub fn ab_rect(seg: usize) -> (i32, i32, i32, i32) {
    let x0 = BAL_X1 - (AB_W * 2 + AB_GAP);
    (x0 + seg as i32 * (AB_W + AB_GAP), AB_TOP, AB_W, AB_H)
}
/// Which A/B segment a tap lands on: 0 = A (effects active), 1 = B (chain bypassed). The band is
/// slack around the drawn boxes, and the midpoint splits it, so a tap between them still resolves.
pub fn hit_ab(x: i32, y: i32) -> Option<usize> {
    let (x0, _, _, _) = ab_rect(0);
    // Asymmetric overhang on purpose: only 6px upward, because the status-bar icons sit at y~22
    // and a band that reached them would eat their taps; 10px downward, where there is nothing
    // until HEADER_BOTTOM.
    if !(AB_TOP - 6..AB_TOP + AB_H + 10).contains(&y) || x < x0 - 10 {
        return None;
    }
    let (x1, ..) = ab_rect(1);
    Some(if x < x1 - AB_GAP / 2 { 0 } else { 1 })
}

/// Every field is Copy, so a preview or a test can spin a variant off an existing one with
/// struct-update syntax instead of restating the whole chain.
#[derive(Clone, Copy)]
pub struct Sound {
    pub dsee: bool,
    pub vinyl: bool,
    pub vpt: &'static str,     // Off / Studio / Club / Concert Hall
    pub dcphase: &'static str, // Off / Standard A.. / Low B
    pub normalizer: bool,
    pub clearaudio: bool,
    pub eq_preset: &'static str,
    pub bt_codec: Option<&'static str>,
    /// L/R balance position, 0..=100 with 50 = centre. A codec mixer control rather than a DSP
    /// effect: `l balance volume` / `r balance volume`, INTEGER 0..88 of attenuation in HALF-dB.
    pub balance: usize,
    /// True while the finger is on the slider — the knob grows and the readout goes accent, so a
    /// touch-only device gives some sign it took the gesture (same idea as the scrollbar thumb).
    pub balance_drag: bool,
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

/// The Balance row: label, live readout, and a full-width drag slider with a centre detent.
///
/// Drawn from the same `BAL_*` constants the hit test uses, so the knob cannot drift away from the
/// place a finger has to land — the recurring defect in this codebase is a render that computes its
/// geometry independently of the tap handler.
fn balance_row(c: &mut Canvas, t: &Theme, f: &FontSet, s: &Sound, sel: bool) {
    let y = balance_top();
    let centred = s.balance == BALANCE_CENTRE;
    if sel {
        fill_rect(c, 0, y, crate::canvas::W as i32, BALANCE_ROW_H, t.row_sel);
    }
    let lc = if sel { t.acc } else { t.ink };
    text::draw(c, f, 22.0, (y + 36) as f32, "Balance",
               &sty(Family::Sans, Weight::SemiBold, 18.0, lc, 0.0));
    text::draw(c, f, 22.0, (y + 58) as f32, "Drag the slider to shift the stereo image",
               &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));

    // CENTRE reset. Greyed out when already centred — it is not a state, it is an action, and an
    // action with nothing to do should say so rather than looking armed.
    {
        let (rx, ry, rw, rh) = balance_reset_rect();
        let col = if centred { t.faint } else { t.acc };
        stroke_rect(c, rx, ry, rw, rh, if centred { t.line } else { t.acc }, 1);
        center(c, f, (rx + rw / 2) as f32, (ry + rh / 2 + 5) as f32, "CENTRE",
               &sty(Family::Mono, Weight::Bold, 12.0, col, 0.14));
    }

    let ty = bal_track_y();
    let cx = balance_x(BALANCE_CENTRE);
    let kx = balance_x(s.balance);

    // Track, then the deflection fill from the centre detent out to the knob, so the direction and
    // the amount are both readable at a glance without doing arithmetic on the label.
    fill_rect(c, BAL_X0, ty - 1, BAL_X1 - BAL_X0, 3, t.line);
    if !centred {
        let (fx, fw) = if kx < cx { (kx, cx - kx) } else { (cx, kx - cx) };
        fill_rect(c, fx, ty - 1, fw, 3, t.acc);
    }
    // Centre detent: a taller tick, so you can see where the null is while dragging.
    fill_rect(c, cx - 1, ty - 11, 2, 23, if centred { t.acc } else { t.dim });

    // Knob. Square, to match the toggle's knob — this UI has no circle primitive and a hand-rolled
    // one here would be the only round thing on the screen. It grows under a finger.
    let k = if s.balance_drag { 32 } else { 24 };
    fill_rect(c, kx - k / 2, ty - k / 2, k, k, if centred && !s.balance_drag { t.dim } else { t.acc });

    // End caps and the live readout share the line under the track. L and R sit at the extremes so
    // they never collide with the knob at full stop; the value sits between them, centred, where it
    // is closest to where you are looking while dragging.
    let cap = sty(Family::Mono, Weight::Bold, 12.0, t.faint, 0.12);
    let vc = if centred { t.faint } else { t.acc };
    text::draw(c, f, BAL_X0 as f32, (ty + 32) as f32, "L", &cap);
    right(c, f, BAL_X1 as f32, (ty + 32) as f32, "R", &cap);
    center(c, f, 240.0, (ty + 32) as f32, &balance_label(s.balance).to_uppercase(),
           &sty(Family::Mono, Weight::Regular, 13.0, vc, 0.1));
    hline(c, y + BALANCE_ROW_H, t.line);
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
        for (i, (label, on)) in segs.iter().enumerate() {
            let (sx, sy, sw, sh) = ab_rect(i);
            let st = sty(Family::Mono, Weight::Bold, 18.0, if *on { t.acc_ink } else { t.dim }, 0.1);
            if *on {
                fill_rect(c, sx, sy, sw, sh, t.acc);
            }
            stroke_rect(c, sx, sy, sw, sh, if *on { t.acc } else { t.line }, 1);
            center(c, f, (sx + sw / 2) as f32, (sy + sh / 2 + 6) as f32, label, &st);
        }
        // The hint moved LEFT of the segments rather than under them: the segments now fill the
        // header's vertical space, and a caption below would have run past HEADER_BOTTOM into the
        // first row.
        let hint = sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.14);
        let (x0, _, _, _) = ab_rect(0);
        right(c, f, (x0 - 12) as f32, (AB_TOP + AB_H / 2 + 4) as f32, "OPTION = A/B", &hint);
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
    balance_row(c, t, f, s, sel == ROW_BALANCE);

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
