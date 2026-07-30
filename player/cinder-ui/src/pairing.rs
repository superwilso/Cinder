//! Bluetooth devices — the **paired-device picker**, reached from Bluetooth ▸ "Pair new device".
//!
//! This screen used to render three hardcoded discoverable devices ("WH-1000XM5", "JBL Flip 6",
//! "Soundcore Q45") and was unreachable — there was no `Screen::Pairing`, so it existed only in the
//! host preview harness. Both halves of that are now gone: it is a real route, and every row is a
//! device the radio actually has a link key for, read from
//! `BtCommonServiceClient::GetPairedDeviceInfo(vector<BtPairedDeviceInformation>&)` (slot 20).
//!
//! What each row can do maps 1:1 onto a call that is known-good on this hardware:
//!   * row body, not connected  → `BtTransmitterServiceClient::RequestConnection(const vector<uint8_t>&)`
//!   * row body, connected      → `RequestDisconnection()` (same call the Bluetooth screen uses)
//!   * FORGET                   → `BtCommonServiceClient::DeleteLinkkey(const vector<uint8_t>&)`
//!
//! Discovering a device that is **not** already paired is deliberately absent rather than faked:
//! results of `SetSearchMode` arrive on `BtCommonServiceListener::OnNotifySearchedDevice`, and
//! Cinder implements no Sony listener vtable yet (the player path passes `NULL` and polls). The
//! footer says so on screen instead of drawing a scanner that can never find anything.

use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, hline, stroke_rect, sty};
use crate::Canvas;

/// One paired device, as the shell last read it off the radio. `kind` is a short descriptor built
/// from the class-of-device word (e.g. "Headphones"); empty is fine and simply draws nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PairedDevice {
    pub name: String,
    pub kind: String,
    pub connected: bool,
}

// ---- layout (shared by render + hit; the row height must never be duplicated) ----
const CARD_Y: i32 = 100;
const CARD_H: i32 = 72;
const LIST_Y0: i32 = 218;
const ROW_H: i32 = 62;
const FORGET_W: i32 = 104;
const FOOT_Y: i32 = 736;
/// Rows that fit between `LIST_Y0` and the footer rule. The radio holds far fewer pairings than
/// this in practice, but the list is clipped rather than trusted.
pub const MAX_ROWS: usize = ((FOOT_Y - LIST_Y0) / ROW_H) as usize;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PairHit {
    None,
    /// Row body — connect if the device is idle, hang up if it is the connected one. Which of the
    /// two is decided by the caller from its own `connected` flag, so geometry stays geometry.
    Row(usize),
    Forget(usize),
}

/// Map a tap to a paired-device action. `count` is how many rows are actually drawn.
pub fn hit(x: i32, y: i32, count: usize) -> PairHit {
    let rows = count.min(MAX_ROWS);
    if !(LIST_Y0..LIST_Y0 + rows as i32 * ROW_H).contains(&y) {
        return PairHit::None;
    }
    let i = ((y - LIST_Y0) / ROW_H) as usize;
    if x >= 458 - FORGET_W {
        PairHit::Forget(i)
    } else {
        PairHit::Row(i)
    }
}

/// `forget_armed` = the row whose FORGET is one tap from firing (the same two-tap confirm the
/// Settings ▸ Boot to stock row uses — dropping a link key is not undoable from this screen).
/// `busy` = a row with a connect in flight.
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    devices: &[PairedDevice],
    forget_armed: Option<usize>,
    busy: Option<usize>,
) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "Devices", None);

    // NFC card — honest: the radio's OOB pairing path is not wired, so this describes the
    // hardware, not a Cinder feature.
    fill_rect(c, 22, CARD_Y, 436, CARD_H, t.panel);
    stroke_rect(c, 22, CARD_Y, 436, CARD_H, t.line, 1);
    icons::rx(c, 46.0, (CARD_Y + CARD_H / 2) as f32, 22.0, t.dim);
    text::draw(c, f, 76.0, (CARD_Y + 30) as f32, "One-touch NFC", &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
    text::draw(c, f, 76.0, (CARD_Y + 50) as f32, "Tap-to-pair is not wired yet — see below",
               &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));

    let rows = devices.len().min(MAX_ROWS);
    text::draw(c, f, 22.0, (LIST_Y0 - 16) as f32, &format!("PAIRED · {}", devices.len()),
               &sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18));

    if rows == 0 {
        center(c, f, 240.0, (LIST_Y0 + 46) as f32, "No paired devices",
               &sty(Family::Sans, Weight::Regular, 16.0, t.faint, 0.0));
        center(c, f, 240.0, (LIST_Y0 + 72) as f32, "PAIR FROM THE SONY PLAYER, THEN COME BACK",
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
    }

    for (i, d) in devices.iter().take(rows).enumerate() {
        let y = LIST_Y0 + i as i32 * ROW_H;
        let cy = y + ROW_H / 2;
        let icol = if d.connected { t.acc } else { t.dim };
        icons::bt(c, 34.0, cy as f32, 16.0, icol);
        text::draw(c, f, 58.0, (cy - 2) as f32, &d.name,
                   &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0));

        // Second line carries the live state first and the device class second — the state is what
        // the user is on this screen to read.
        let sub = if busy == Some(i) {
            "CONNECTING…".to_string()
        } else if d.connected {
            "CONNECTED · TAP TO DISCONNECT".to_string()
        } else if d.kind.is_empty() {
            "TAP TO CONNECT".to_string()
        } else {
            format!("{} · TAP TO CONNECT", d.kind.to_uppercase())
        };
        let scol = if d.connected { t.acc } else { t.faint };
        text::draw(c, f, 58.0, (cy + 15) as f32, &sub, &sty(Family::Mono, Weight::Regular, 11.0, scol, 0.06));

        // FORGET, two-tap. The armed state replaces the label in place so it cannot be missed.
        // The box is a FIXED `FORGET_W` wide — the same constant `hit()` splits the row on — so the
        // touch target is exactly the drawn button. Sizing it to the label instead would leave a
        // silent dead band beside it that armed FORGET when the user meant to tap the device.
        let armed = forget_armed == Some(i);
        let flabel = if armed { "TAP AGAIN" } else { "FORGET" };
        let fcol = if armed { t.acc } else { t.dim };
        let fs = sty(Family::Mono, Weight::Regular, 12.0, fcol, 0.1);
        stroke_rect(c, 458 - FORGET_W, cy - 13, FORGET_W, 26, fcol, 1);
        center(c, f, (458 - FORGET_W / 2) as f32, (cy + 4) as f32, flabel, &fs);
        hline(c, y + ROW_H, t.line);
    }

    // Footer: why there is no scanner here. `SetSearchMode` starts a scan happily; the results come
    // back on a listener vtable Cinder does not implement yet, so a scan would look broken instead.
    hline(c, FOOT_Y, t.line);
    text::draw(c, f, 22.0, 760.0, "CONNECT · DISCONNECT · FORGET DRIVE THE REAL RADIO.",
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
    text::draw(c, f, 22.0, 776.0, "PAIRING A NEW DEVICE NEEDS THE SONY PLAYER FOR NOW.",
               &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row bands must line up with what `render` draws, and the FORGET split must be the same
    /// constant on both sides. A row index here becomes a BD address on the shell side, so an
    /// off-by-one is not a cosmetic bug — it connects to, or forgets, the wrong headphones.
    #[test]
    fn rows_map_to_their_own_index_and_the_forget_band_is_the_drawn_button() {
        // First row, body.
        assert_eq!(hit(120, LIST_Y0 + 4, 2), PairHit::Row(0));
        assert_eq!(hit(120, LIST_Y0 + ROW_H - 1, 2), PairHit::Row(0));
        // Second row, body — the boundary belongs to the row below, not above.
        assert_eq!(hit(120, LIST_Y0 + ROW_H, 2), PairHit::Row(1));
        // FORGET starts exactly where the box is stroked (458 - FORGET_W) and runs to the edge.
        assert_eq!(hit(458 - FORGET_W, LIST_Y0 + 4, 2), PairHit::Forget(0));
        assert_eq!(hit(479, LIST_Y0 + ROW_H + 4, 2), PairHit::Forget(1));
        // One pixel left of the box is still the row body.
        assert_eq!(hit(458 - FORGET_W - 1, LIST_Y0 + 4, 2), PairHit::Row(0));
    }

    /// Taps below the last DRAWN row are inert. The list length is the shell's, not the screen's, so
    /// an empty or short list must not hand back a row that is not on screen — the shell would then
    /// index its address vector with a row the user never saw.
    #[test]
    fn taps_past_the_last_drawn_row_hit_nothing() {
        assert_eq!(hit(120, LIST_Y0 + 4, 0), PairHit::None);
        assert_eq!(hit(120, LIST_Y0 + 2 * ROW_H, 2), PairHit::None);
        assert_eq!(hit(120, LIST_Y0 - 1, 2), PairHit::None);
        // A list longer than the screen is clipped to MAX_ROWS, matching `render`'s take().
        assert_eq!(hit(120, LIST_Y0 + MAX_ROWS as i32 * ROW_H, 99), PairHit::None);
        assert_eq!(hit(120, LIST_Y0 + (MAX_ROWS as i32 - 1) * ROW_H, 99), PairHit::Row(MAX_ROWS - 1));
    }
}
