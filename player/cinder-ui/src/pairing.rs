//! Bluetooth devices — the **paired-device picker plus discovery**, reached from Bluetooth ▸ "Pair
//! new device".
//!
//! This screen used to render three hardcoded discoverable devices ("WH-1000XM5", "JBL Flip 6",
//! "Soundcore Q45") and was unreachable — there was no `Screen::Pairing`, so it existed only in the
//! host preview harness. Both halves of that are gone: it is a real route, and every row is a real
//! device.
//!
//! Two sections, two data sources, and each row maps 1:1 onto a call proven on this hardware:
//!   * **PAIRED** — `BtCommonServiceClient::GetPairedDeviceInfo` (slot 20)
//!       * row body, not connected → `BtTransmitterServiceClient::RequestConnection(const vector<uint8_t>&)`
//!       * row body, connected     → `RequestDisconnection()` (same call the Bluetooth screen uses)
//!       * FORGET                  → `BtCommonServiceClient::DeleteLinkkey(const vector<uint8_t>&)`
//!   * **FOUND** — `SetSearchMode(const bool&, const uint16_t&)` (slot 14), results arriving on
//!     `BtCommonServiceListener::OnNotifySearchedDevice` (listener slot 6)
//!       * row body → `BtCommonServiceClient::Pairing(const vector<uint8_t>&)` (slot 7)
//!
//! Pairing prompts are handled too, as a MODAL panel over the list: `OnNotifyNumericComparison`
//! (listener slot 3) and `OnNotifySspRequest` (slot 14) ask for a yes/no and are answered with
//! `SetNumericComparison` / `RequestSspReply`, while `OnNotifyPasskey` (slot 5) is display-only — that
//! code is for the OTHER device's user to type, so the panel offers nothing to accept.

use crate::icons;
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty};
use crate::Canvas;

/// One device row. Used for both sections; `connected` is only meaningful for a paired device, and
/// `kind` is a short descriptor derived from the class-of-device word ("" draws nothing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PairedDevice {
    pub name: String,
    pub kind: String,
    pub connected: bool,
}

// ---- layout (shared by render + hit; nothing here may be duplicated in one and not the other) ----
const CARD_Y: i32 = 100;
const CARD_H: i32 = 72;
const SCAN_BTN: (i32, i32, i32, i32) = (340, 118, 118, 36); // x,y,w,h
const LIST_Y0: i32 = 218;
const ROW_H: i32 = 62;
const FORGET_W: i32 = 104;
const FOOT_Y: i32 = 736;
/// Paired rows are capped so a long pairing history can never push the FOUND section off screen —
/// discovery is the reason you came to this screen, so it must stay reachable.
///
/// THE CAP USED TO TRUNCATE, FULL STOP. `devices.iter().take(MAX_PAIRED)` in the render and
/// `paired.min(MAX_PAIRED)` in the hit test, with the header cheerfully printing `PAIRED · 6`
/// beside four rows. Pair a fifth device and it was invisible and unreachable — and FORGET lives
/// only on this screen, so a link key you could see the count of could not be deleted by any path
/// in the app. (The Bluetooth screen's summary list showed five, a different number, and had no
/// FORGET at all.) The radio's table is the only place that state lives, and nothing else in Cinder
/// can clear it.
///
/// So the cap became a WINDOW rather than a limit: `MAX_PAIRED` rows at a time, and a MORE control
/// beside the header that advances to the next page and wraps. No scroll offset — this screen's
/// FOUND section is positioned from the paired row count, and the one screen in this app whose
/// render and hit test have drifted apart already is the one that grew a scroll offset (see
/// `Screen::Advanced`). A page index is a single integer both sides derive everything from.
pub const MAX_PAIRED: usize = 4;
const SECTION_GAP: i32 = 30;
/// The MORE control, beside the PAIRED header. Only drawn (and only hit) when there IS another
/// page — a control that does nothing is worse than no control.
const MORE_BTN: (i32, i32, i32, i32) = (352, LIST_Y0 - 30, 106, 24); // x,y,w,h

/// How many pages the paired list needs, and which one `page` really is once wrapped. Both sides
/// call these; neither computes them itself.
pub fn paired_pages(paired: usize) -> usize {
    if paired == 0 { 1 } else { (paired + MAX_PAIRED - 1) / MAX_PAIRED }
}
/// First absolute index shown on `page`. Wraps, so a page index left over from a longer list can
/// never point past the end.
pub fn paired_page_start(paired: usize, page: usize) -> usize {
    (page % paired_pages(paired)) * MAX_PAIRED
}
/// How many rows `page` actually draws.
pub fn paired_page_rows(paired: usize, page: usize) -> usize {
    paired.saturating_sub(paired_page_start(paired, page)).min(MAX_PAIRED)
}

/// Y of the FOUND section's header text, given how many paired rows are drawn.
fn found_header_y(paired: usize) -> i32 {
    LIST_Y0 + paired.min(MAX_PAIRED) as i32 * ROW_H + SECTION_GAP
}
/// Y of the first FOUND row.
fn found_y0(paired: usize) -> i32 {
    found_header_y(paired) + 14
}
/// How many FOUND rows fit above the footer rule.
fn found_capacity(paired: usize) -> usize {
    let room = FOOT_Y - found_y0(paired);
    if room <= 0 { 0 } else { (room / ROW_H) as usize }
}

/// A pairing prompt the radio is waiting on. `kind` matches the shell's enum: 1 = numeric comparison
/// (confirm the digits match), 2 = passkey (display only — the OTHER device's user types it),
/// 3 = secure-simple-pairing request.
#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub kind: u8,
    pub name: String,
    pub code: u32,
}

pub const PROMPT_NUMERIC: u8 = 1;
pub const PROMPT_PASSKEY: u8 = 2;

// ---- prompt panel geometry ----
const PP: (i32, i32, i32, i32) = (40, 250, 400, 260); // x,y,w,h
const PP_OK: (i32, i32, i32, i32) = (60, 440, 170, 50);
const PP_NO: (i32, i32, i32, i32) = (250, 440, 170, 50);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PairHit {
    None,
    /// Start/stop discovery.
    Scan,
    /// Paired row body — connect if idle, hang up if it is the connected one. Which of the two is
    /// the caller's decision, from its own `connected` flag, so geometry stays geometry.
    Row(usize),
    Forget(usize),
    /// A discovered, unpaired device — pair with it.
    Pair(usize),
    /// Advance the paired list to the next page (wrapping). Only reachable when there is more than
    /// one page — see `hit`.
    MorePaired,
    /// Prompt answers.
    PromptConfirm,
    PromptCancel,
}

/// Taps while a prompt is up. The prompt is MODAL: anything else on the screen is unreachable until
/// it is answered, because a half-finished pairing that the user has walked away from is worse than a
/// blocked screen — the radio is sitting there waiting for a yes or a no.
pub fn hit_prompt(x: i32, y: i32, kind: u8) -> PairHit {
    let (ox, oy, ow, oh) = PP_OK;
    let (nx, ny, nw, nh) = PP_NO;
    // A passkey panel has nothing to confirm: the code is for the other device's user to type, so the
    // single button dismisses (and tells the radio to give up).
    if kind != PROMPT_PASSKEY && (oy..oy + oh).contains(&y) && (ox..ox + ow).contains(&x) {
        return PairHit::PromptConfirm;
    }
    if (ny..ny + nh).contains(&y) && (nx..nx + nw).contains(&x) {
        return PairHit::PromptCancel;
    }
    if kind == PROMPT_PASSKEY && (oy..oy + oh).contains(&y) && (ox..ox + ow).contains(&x) {
        return PairHit::PromptCancel;
    }
    PairHit::None
}

/// The modal panel. Drawn last, over whatever the screen already had.
pub fn render_prompt(c: &mut Canvas, t: &Theme, f: &FontSet, p: &Prompt) {
    let (px, py, pw, ph) = PP;
    // Same scrim the Restart/Power-off modals use (confirm.rs): an alpha wash toward the background,
    // so the list behind stays legible as context but clearly isn't the thing to tap.
    for y in 0..crate::H as i32 {
        for x in 0..crate::W as i32 {
            c.blend(x, y, t.bg, 200);
        }
    }
    fill_rect(c, px, py, pw, ph, t.panel);
    stroke_rect(c, px, py, pw, ph, t.acc, 1);
    let title = if p.kind == PROMPT_PASSKEY { "PASSKEY" } else { "CONFIRM PAIRING" };
    center(c, f, 240.0, (py + 30) as f32, title, &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));
    center(c, f, 240.0, (py + 60) as f32, &p.name, &sty(Family::Sans, Weight::Bold, 20.0, t.ink, 0.0));
    center(c, f, 240.0, (py + 122) as f32, &format!("{:06}", p.code),
           &sty(Family::Mono, Weight::Regular, 38.0, t.ink, 0.22));
    let hint = if p.kind == PROMPT_PASSKEY {
        "ENTER THIS CODE ON THE OTHER DEVICE"
    } else {
        "DOES THE OTHER DEVICE SHOW THIS CODE?"
    };
    center(c, f, 240.0, (py + 158) as f32, hint, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.08));

    let (ox, oy, ow, oh) = PP_OK;
    let (nx, ny, nw, nh) = PP_NO;
    if p.kind == PROMPT_PASSKEY {
        // One button, spanning where the pair would be: there is nothing to accept.
        stroke_rect(c, ox, oy, nx + nw - ox, oh, t.line, 1);
        center(c, f, 240.0, (oy + oh / 2 + 5) as f32, "DISMISS",
               &sty(Family::Sans, Weight::SemiBold, 16.0, t.dim, 0.0));
    } else {
        fill_rect(c, ox, oy, ow, oh, t.acc);
        center(c, f, (ox + ow / 2) as f32, (oy + oh / 2 + 5) as f32, "YES, PAIR",
               &sty(Family::Sans, Weight::Bold, 16.0, t.acc_ink, 0.0));
        stroke_rect(c, nx, ny, nw, nh, t.line, 1);
        center(c, f, (nx + nw / 2) as f32, (ny + nh / 2 + 5) as f32, "CANCEL",
               &sty(Family::Sans, Weight::SemiBold, 16.0, t.dim, 0.0));
    }
}

/// Map a tap. `paired`/`found` are how many devices each list holds; both are clipped exactly the
/// way `render` clips them, so a tap can never resolve to a row that isn't on screen.
pub fn hit(x: i32, y: i32, paired: usize, found: usize, page: usize) -> PairHit {
    let (sx, sy, sw, sh) = SCAN_BTN;
    if (sy..sy + sh).contains(&y) && (sx..sx + sw).contains(&x) {
        return PairHit::Scan;
    }
    // MORE, before the rows: it sits above LIST_Y0, but only exists when there is another page, and
    // an inert control must not eat a tap. Same rule the SCAN button follows.
    if paired_pages(paired) > 1 {
        let (mx, my, mw, mh) = MORE_BTN;
        if (my..my + mh).contains(&y) && (mx..mx + mw).contains(&x) {
            return PairHit::MorePaired;
        }
    }
    let prows = paired_page_rows(paired, page);
    if (LIST_Y0..LIST_Y0 + prows as i32 * ROW_H).contains(&y) {
        // ABSOLUTE index out, page-relative row in. Every caller — connect, disconnect, forget —
        // indexes the shell's `g_bt_paired`, which is the whole list and never a page of it.
        let i = paired_page_start(paired, page) + ((y - LIST_Y0) / ROW_H) as usize;
        return if x >= 458 - FORGET_W { PairHit::Forget(i) } else { PairHit::Row(i) };
    }
    let frows = found.min(found_capacity(paired));
    let fy0 = found_y0(paired);
    if (fy0..fy0 + frows as i32 * ROW_H).contains(&y) {
        return PairHit::Pair(((y - fy0) / ROW_H) as usize);
    }
    PairHit::None
}

/// Draw one device row. Returns nothing; the caller owns the y cursor.
#[allow(clippy::too_many_arguments)]
fn row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, name: &str, sub: &str,
       accent: bool, right_label: Option<&str>, right_accent: bool) {
    let cy = y + ROW_H / 2;
    icons::bt(c, 34.0, cy as f32, 16.0, if accent { t.acc } else { t.dim });
    crate::widgets::draw_fit(c, f, 58.0, (cy - 2) as f32, name, &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0), 458.0);
    let scol = if accent { t.acc } else { t.faint };
    crate::widgets::draw_fit(c, f, 58.0, (cy + 15) as f32, sub, &sty(Family::Mono, Weight::Regular, 11.0, scol, 0.06), 458.0);
    if let Some(label) = right_label {
        // Fixed width, the same constant `hit(, 0)` splits the row on, so the touch target IS the drawn
        // button. Sizing the box to the label would leave a silent dead band beside it.
        let col = if right_accent { t.acc } else { t.dim };
        let s = sty(Family::Mono, Weight::Regular, 12.0, col, 0.1);
        stroke_rect(c, 458 - FORGET_W, cy - 13, FORGET_W, 26, col, 1);
        center(c, f, (458 - FORGET_W / 2) as f32, (cy + 4) as f32, label, &s);
    }
    hline(c, y + ROW_H, t.line);
}

/// `forget_armed` = the paired row whose FORGET is one tap from firing (two-tap confirm, same idiom
/// as Settings ▸ Boot to stock — dropping a link key can't be undone from here). `busy` = a paired
/// row with a connect in flight.
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    devices: &[PairedDevice],
    found: &[PairedDevice],
    forget_armed: Option<usize>,
    busy: Option<usize>,
    scanning: bool,
    busy_phase: f32,
    // Which page of the paired list to draw. Wrapped by `paired_page_start`, so a stale value
    // left over from a longer list is harmless.
    page: usize,
) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Devices", None);

    // Scan panel. This replaced a decorative NFC card: tap-to-pair isn't wired, so the most useful
    // thing this space can hold is the control that does work.
    fill_rect(c, 22, CARD_Y, 436, CARD_H, t.panel);
    stroke_rect(c, 22, CARD_Y, 436, CARD_H, t.line, 1);
    crate::widgets::draw_fit(c, f, 40.0, (CARD_Y + 30) as f32, "Scan for new devices",
               &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0), 458.0);
    let sub = if scanning {
        format!("SEARCHING… {} FOUND", found.len())
    } else if found.is_empty() {
        "PUT THE DEVICE IN PAIRING MODE FIRST".to_string()
    } else {
        format!("{} FOUND · SCAN AGAIN TO REFRESH", found.len())
    };
    crate::widgets::draw_fit(c, f, 40.0, (CARD_Y + 52) as f32, &sub,
               &sty(Family::Mono, Weight::Regular, 11.0, if scanning { t.acc } else { t.faint }, 0.08), 458.0);
    let (sx, sy, sw, sh) = SCAN_BTN;
    if scanning {
        fill_rect(c, sx, sy, sw, sh, t.acc);
    } else {
        stroke_rect(c, sx, sy, sw, sh, t.acc, 1);
    }
    center(c, f, (sx + sw / 2) as f32, (sy + sh / 2 + 4) as f32, if scanning { "STOP" } else { "SCAN" },
           &sty(Family::Sans, Weight::SemiBold, 15.0, if scanning { t.acc_ink } else { t.acc }, 0.04));

    // PAIRED — one page at a time, from the same three functions `hit` uses.
    let pages = paired_pages(devices.len());
    let start = paired_page_start(devices.len(), page);
    let prows = paired_page_rows(devices.len(), page);
    // Say WHICH ones these are once there is more than one page. "PAIRED · 6" over four rows was
    // technically true and actively misleading.
    let head = if pages > 1 {
        format!("PAIRED · {}-{} OF {}", start + 1, start + prows, devices.len())
    } else {
        format!("PAIRED · {}", devices.len())
    };
    crate::widgets::draw_fit(c, f, 22.0, (LIST_Y0 - 16) as f32, &head,
               &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18), 458.0);
    if pages > 1 {
        let (mx, my, mw, mh) = MORE_BTN;
        stroke_rect(c, mx, my, mw, mh, t.acc, 1);
        center(c, f, (mx + mw / 2) as f32, (my + mh / 2 + 4) as f32, "MORE \u{203a}",
               &sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.1));
    }
    if devices.is_empty() {
        crate::widgets::draw_fit(c, f, 58.0, (LIST_Y0 + 26) as f32, "Nothing paired yet",
                   &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0), 458.0);
    }
    for (i, d) in devices.iter().skip(start).take(prows).enumerate() {
        // `i` is the row on screen; `ai` is the device. Every flag the caller passes in
        // (forget_armed, busy) is an ABSOLUTE index, because that is what the taps returned.
        let ai = start + i;
        let sub = if busy == Some(ai) {
            "CONNECTING…".to_string()
        } else if d.connected {
            "CONNECTED · TAP TO DISCONNECT".to_string()
        } else if d.kind.is_empty() {
            "TAP TO CONNECT".to_string()
        } else {
            format!("{} · TAP TO CONNECT", d.kind.to_uppercase())
        };
        let armed = forget_armed == Some(ai);
        let ry = LIST_Y0 + i as i32 * ROW_H;
        row(c, t, f, ry, &d.name, &sub, d.connected,
            Some(if armed { "TAP AGAIN" } else { "FORGET" }), armed);
        // A moving indicator on the row that is actually attempting. "CONNECTING…" on its own is
        // indistinguishable from a wedged attempt, and BT connects here can take several seconds.
        // Placed in the empty gap between the subtitle and the FORGET button — x≈30 is where the
        // row's own Bluetooth glyph lives, so the obvious spot would have drawn straight over it.
        if busy == Some(ai) {
            crate::widgets::spinner(c, 330, ry + ROW_H / 2, 7, 3, busy_phase, t.acc);
        }
    }

    // FOUND — only shown once there is something to say, so the screen is quiet when idle.
    if scanning || !found.is_empty() {
        let hy = found_header_y(devices.len());
        crate::widgets::draw_fit(c, f, 22.0, hy as f32, &format!("FOUND · {}", found.len()),
                   &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18), 458.0);
        let cap = found_capacity(devices.len());
        let fy0 = found_y0(devices.len());
        if found.is_empty() {
            crate::widgets::draw_fit(c, f, 58.0, (fy0 + 26) as f32, "Searching…",
                       &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0), 458.0);
            if scanning {
                crate::widgets::spinner(c, 30, fy0 + 21, 7, 3, busy_phase, t.acc);
            }
        }
        for (i, d) in found.iter().take(cap).enumerate() {
            let sub = if d.kind.is_empty() {
                "TAP TO PAIR".to_string()
            } else {
                format!("{} · TAP TO PAIR", d.kind.to_uppercase())
            };
            row(c, t, f, fy0 + i as i32 * ROW_H, &d.name, &sub, false, Some("PAIR"), true);
        }
    }

    // Footer: what this screen can do that is not visible from it.
    //
    // THE SECOND LINE USED TO READ "NFC TAP-TO-PAIR IS NOT WIRED YET", AND IT WAS TRUE WHEN IT WAS
    // WRITTEN. NFC was wired on 2026-08-17 — the shell runs a listener, and a tap dispatches on the
    // link state (already linked → disconnect, bonded → connect, unknown → pair then connect). The
    // line was never revisited, so this screen went on telling the user a working feature was
    // broken, while the Bluetooth screen one push away advertised the same feature as available
    // ("NFC · TOUCH DEVICE TO REAR PANEL"). Two screens, one feature, opposite claims.
    //
    // A caption that says a feature does not work is not a harmless leftover: it is the reason
    // nobody tries it. Found by the 2026-09-06 UI audit.
    hline(c, FOOT_Y, t.line);
    crate::widgets::draw_fit(c, f, 22.0, 760.0, "PIN AND CONFIRM PROMPTS APPEAR HERE WHEN A DEVICE ASKS",
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1), 458.0);
    icons::rx(c, 30.0, 776.0, 14.0, t.faint);
    crate::widgets::draw_fit(c, f, 46.0, 780.0, "OR TAP THE DEVICE TO THE REAR PANEL (NFC)",
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.08), 458.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row bands must line up with what `render` draws, and the FORGET split must be the same
    /// constant on both sides. A row index here becomes a BD address on the shell side, so an
    /// off-by-one is not cosmetic — it connects to, forgets, or pairs with the wrong device.
    #[test]
    fn paired_rows_map_to_their_own_index_and_forget_is_the_drawn_button() {
        assert_eq!(hit(120, LIST_Y0 + 4, 2, 0, 0), PairHit::Row(0));
        assert_eq!(hit(120, LIST_Y0 + ROW_H - 1, 2, 0, 0), PairHit::Row(0));
        assert_eq!(hit(120, LIST_Y0 + ROW_H, 2, 0, 0), PairHit::Row(1));
        assert_eq!(hit(458 - FORGET_W, LIST_Y0 + 4, 2, 0, 0), PairHit::Forget(0));
        assert_eq!(hit(479, LIST_Y0 + ROW_H + 4, 2, 0, 0), PairHit::Forget(1));
        assert_eq!(hit(458 - FORGET_W - 1, LIST_Y0 + 4, 2, 0, 0), PairHit::Row(0));
    }

    /// EVERY paired device must be reachable, on some page. This is the defect the window replaced:
    /// with more devices than `MAX_PAIRED`, the extras were drawn by nothing and hit by nothing —
    /// and FORGET exists only on this screen, so a link key past the cap could not be deleted from
    /// anywhere in the app.
    #[test]
    fn every_paired_device_is_reachable_on_some_page() {
        for n in 0..=9usize {
            let mut seen = vec![false; n];
            for page in 0..paired_pages(n) {
                let rows = paired_page_rows(n, page);
                for r in 0..rows {
                    let y = LIST_Y0 + r as i32 * ROW_H + 4;
                    match hit(120, y, n, 0, page) {
                        PairHit::Row(i) => {
                            assert!(i < n, "n={n} page={page} row={r} gave out-of-range {i}");
                            assert!(!seen[i], "n={n}: device {i} reachable twice");
                            seen[i] = true;
                        }
                        other => panic!("n={n} page={page} row={r} gave {other:?}"),
                    }
                    // FORGET on the same row must name the same device, or the button deletes a
                    // link key belonging to someone else.
                    let f = hit(479, y, n, 0, page);
                    assert_eq!(f, PairHit::Forget(paired_page_start(n, page) + r));
                }
            }
            assert!(seen.iter().all(|s| *s), "n={n}: some devices were unreachable");
        }
    }

    /// The row count `hit` accepts taps for is the row count `render` draws — the two used to be
    /// written out separately (`paired.min(MAX_PAIRED)` against `.take(prows)`), which is exactly
    /// how a hit test drifts from what is on the glass.
    #[test]
    fn hit_stops_where_the_last_drawn_row_ends() {
        for n in 0..=9usize {
            for page in 0..paired_pages(n) {
                let rows = paired_page_rows(n, page);
                let past = LIST_Y0 + rows as i32 * ROW_H + 1;
                assert!(!matches!(hit(120, past, n, 0, page), PairHit::Row(_) | PairHit::Forget(_)),
                        "n={n} page={page}: a tap below the last drawn row hit a device");
            }
        }
    }

    /// MORE only exists when there is somewhere to go, and it wraps rather than running off the end.
    #[test]
    fn more_appears_only_with_a_second_page_and_wraps() {
        let (mx, my, mw, mh) = MORE_BTN;
        let (cx, cy) = (mx + mw / 2, my + mh / 2);
        for n in 0..=MAX_PAIRED {
            assert_eq!(hit(cx, cy, n, 0, 0), PairHit::None, "n={n}: MORE was live with one page");
        }
        assert_eq!(hit(cx, cy, MAX_PAIRED + 1, 0, 0), PairHit::MorePaired);
        // Five devices = two pages: page 1 shows exactly the fifth, and page 2 wraps to the first.
        assert_eq!(paired_pages(5), 2);
        assert_eq!(paired_page_start(5, 1), 4);
        assert_eq!(paired_page_rows(5, 1), 1);
        assert_eq!(paired_page_start(5, 2), 0);
        // A page index left over from a longer list must not point past the end.
        assert_eq!(paired_page_start(2, 7), 0);
        assert_eq!(paired_page_rows(2, 7), 2);
    }

    /// The prompt is modal, so its buttons are the only things reachable — and a PASSKEY panel must
    /// never report Confirm, because there is nothing to confirm and answering yes to a code the user
    /// hasn't typed anywhere would just wedge the pairing.
    #[test]
    fn a_passkey_prompt_can_only_be_dismissed() {
        let (ox, oy, ow, oh) = PP_OK;
        let (nx, ny, nh) = (PP_NO.0, PP_NO.1, PP_NO.3);
        assert_eq!(hit_prompt(ox + ow / 2, oy + oh / 2, PROMPT_NUMERIC), PairHit::PromptConfirm);
        assert_eq!(hit_prompt(nx + 10, ny + nh / 2, PROMPT_NUMERIC), PairHit::PromptCancel);
        // Passkey: BOTH button areas mean dismiss.
        assert_eq!(hit_prompt(ox + ow / 2, oy + oh / 2, PROMPT_PASSKEY), PairHit::PromptCancel);
        assert_eq!(hit_prompt(nx + 10, ny + nh / 2, PROMPT_PASSKEY), PairHit::PromptCancel);
        // Anywhere else while modal: nothing happens.
        assert_eq!(hit_prompt(240, 120, PROMPT_NUMERIC), PairHit::None);
        assert_eq!(hit_prompt(240, LIST_Y0 + 4, PROMPT_NUMERIC), PairHit::None);
    }

    #[test]
    fn the_scan_button_is_hittable_and_does_not_overlap_the_list() {
        let (sx, sy, sw, sh) = SCAN_BTN;
        assert_eq!(hit(sx + sw / 2, sy + sh / 2, 2, 0, 0), PairHit::Scan);
        assert_eq!(hit(sx - 1, sy + sh / 2, 2, 0, 0), PairHit::None);
        assert!(sy + sh < LIST_Y0, "the scan button must sit above the first row");
    }

    /// The FOUND section is offset by however many paired rows are drawn, and both sections clip the
    /// same way `render` does — a tap must never resolve to a row that is not on screen.
    #[test]
    fn found_rows_sit_below_the_paired_list_and_both_clip() {
        assert_eq!(hit(120, found_y0(2) + 4, 2, 3, 0), PairHit::Pair(0));
        assert_eq!(hit(120, found_y0(2) + ROW_H + 4, 2, 3, 0), PairHit::Pair(1));
        // With no paired devices the FOUND rows move up, and the same y is a different row.
        assert_eq!(hit(120, found_y0(0) + 4, 0, 3, 0), PairHit::Pair(0));
        assert_ne!(found_y0(0), found_y0(2));
        // Past the end of each list: nothing.
        assert_eq!(hit(120, found_y0(2) + 3 * ROW_H + 4, 2, 3, 0), PairHit::None);
        assert_eq!(hit(120, LIST_Y0 + 2 * ROW_H + 4, 2, 0, 0), PairHit::None);
        // A pairing history longer than MAX_PAIRED is clipped, and the FOUND section stays reachable.
        assert_eq!(hit(120, LIST_Y0 + MAX_PAIRED as i32 * ROW_H - 1, 99, 1, 0), PairHit::Row(MAX_PAIRED - 1));
        assert_eq!(hit(120, found_y0(99) + 4, 99, 1, 0), PairHit::Pair(0));
        assert!(found_capacity(99) >= 1, "there must always be room for at least one found device");
    }
}
