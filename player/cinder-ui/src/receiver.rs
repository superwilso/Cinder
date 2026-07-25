//! BT Receiver — ported from cinder-proto-screens4.jsx `CReceiver`. Toggle in
//! the header; centred on/off state (phone → Walkman DAC + amp); footer note.

use crate::icons;
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{bars, center, hline, sty, toggle};
use crate::Canvas;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, on: bool) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    crate::chrome::header(c, t, f, "BT Receiver", None);
    toggle(c, t, 424, 56, 34, 18, 12, on);

    if on {
        icons::rx(c, 240.0, 300.0, 44.0, t.acc);
        center(c, f, 240.0, 362.0, "Discoverable as \"NW-A55\"", &sty(Family::Sans, Weight::Bold, 22.0, t.ink, 0.0));
        center(c, f, 240.0, 392.0, "Play from your phone — the Walkman becomes", &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
        center(c, f, 240.0, 412.0, "the DAC + amp for your wired headphones.", &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
        bars(c, 130, 452, 220, 26, 22, 3, 5.0, t.acc, t.line);
        center(c, f, 240.0, 508.0, "WAITING FOR SOURCE · LDAC / AAC / SBC", &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.12));
    } else {
        icons::rx(c, 240.0, 320.0, 44.0, t.faint);
        center(c, f, 240.0, 382.0, "Receiver mode is off", &sty(Family::Sans, Weight::Bold, 19.0, t.dim, 0.0));
        center(c, f, 240.0, 412.0, "Turn on to stream from a phone into the", &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
        center(c, f, 240.0, 432.0, "Walkman's DAC and amp. Local playback pauses.", &sty(Family::Sans, Weight::Regular, 13.0, t.faint, 0.0));
    }

    hline(c, 740, t.line);
    center(c, f, 240.0, 770.0, "NOTE: EQ + DSP APPLY TO RECEIVED AUDIO TOO.", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.1));
}
