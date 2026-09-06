//! BT Receiver — the Walkman as a Bluetooth SINK: play from a phone, and the device becomes the
//! DAC and amp for the wired headphones plugged into it.
//!
//! NOT WIRED, AND THIS SCREEN SAYS SO. It used to draw a working-looking toggle in the header,
//! which `nav` renders with `on: false` unconditionally and which no `tap()` branch answers — so
//! it was a switch that could not be moved, on a screen whose whole "on" layout was unreachable.
//! The 2026-09-06 UI audit found it; the rule it broke is this project's own, stated two entries
//! away in STATUS.md about the Devices screen: a feature that cannot work "says so on screen
//! instead of drawing a scanner that cannot work".
//!
//! So the switch is gone and the screen explains what the feature would be and what is missing.
//! What it describes is real and researched — HFP in Hands-Free-unit role is present on this
//! firmware (`analysis/G_bt_nfc/RE_findings.md`) — it simply has no route through Cinder yet. When
//! it is wired, the "on" state comes back with a switch that moves.

use crate::icons;
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, hline, sty};
use crate::Canvas;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, "BT Receiver", Some("NOT AVAILABLE YET"));

    icons::rx(c, 240.0, 320.0, 44.0, t.faint);
    center(c, f, 240.0, 382.0, "Receiver mode is not available yet",
           &sty(Family::Sans, Weight::Bold, 21.0, t.dim, 0.0));
    center(c, f, 240.0, 412.0, "It would let a phone stream into the Walkman's",
           &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    center(c, f, 240.0, 432.0, "DAC and amp, for the headphones plugged into it.",
           &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));
    center(c, f, 240.0, 462.0, "Nothing on this screen is switched on yet.",
           &sty(Family::Sans, Weight::Regular, 15.0, t.faint, 0.0));

    hline(c, 740, t.line);
    center(c, f, 240.0, 770.0, "EQ + DSP WOULD APPLY TO RECEIVED AUDIO TOO.",
           &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.1));
}
