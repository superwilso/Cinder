//! Settings ▸ Wi-Fi. Only offered when the Wi-Fi install option is on the player ([`installed`]):
//! the player's own firmware has no Wi-Fi at all, and Cinder's comes from `wifi/` (kernel modules
//! built from Sony's own GPL source) plus the setuid `cinder-wifi` helper.
//!
//! The shell runs the helper and hands this module its text output ([`parse_status`],
//! [`parse_scan`]); everything here is pure, so the parsing and the layout are host-tested.
//!
//!   ┌ Wi-Fi ───────────────────────────┐
//!   │ Wi-Fi                     [■  ] │  the switch: radio on/off
//!   │ CONNECTED · HOME · 192.168.1.20  │  the strip: what it is doing
//!   │ NETWORKS                   SCAN  │
//!   │ Home              WPA2 · strong  │  one row a network, strongest first; tap to join
//!   │ …                                │
//!   └ Uses battery while on.  ─────────┘

use crate::canvas::{Canvas, H, W};
use crate::kit::{self, Row, Trail};
use crate::text::FontSet;
use crate::theme::Theme;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The shell found the Wi-Fi install option on this player. Without it the Settings row is hidden.
pub fn set_installed(on: bool) {
    INSTALLED.store(on, Ordering::Relaxed);
}
pub fn installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// The last state the navigator heard, for the Settings row's value (which is drawn from a
/// `SettingsView` that does not carry the Wi-Fi page's state).
static LAST: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub fn note_state(s: State) {
    LAST.store(s as u8, Ordering::Relaxed);
}
/// The Settings row's value, from [`note_state`].
pub fn row_value() -> &'static str {
    let s = match LAST.load(Ordering::Relaxed) {
        1 => State::Busy,
        2 => State::On,
        3 => State::Connecting,
        4 => State::Connected,
        _ => State::Off,
    };
    settings_value(&Status {
        state: s,
        ..Status::default()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum State {
    #[default]
    Off = 0,
    /// The shell has asked for on/off and the helper has not answered yet.
    Busy,
    /// Radio on, not associated (no saved network, or it is out of range).
    On,
    Connecting,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    Open,
    Wep,
    Wpa2,
    Wpa3,
}

impl Security {
    pub fn label(self) -> &'static str {
        match self {
            Security::Open => "OPEN",
            Security::Wep => "WEP",
            Security::Wpa2 => "WPA2",
            Security::Wpa3 => "WPA3",
        }
    }
    /// A password is asked for. WEP is listed but cannot be joined: the build has no WEP.
    pub fn needs_password(self) -> bool {
        !matches!(self, Security::Open)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    /// Received signal, dBm.
    pub signal: i32,
    pub security: Security,
    /// The SSID's bytes. Usually UTF-8; anything else is shown with replacement marks.
    pub ssid: Vec<u8>,
}

impl Network {
    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.ssid).into_owned()
    }
}

/// What the page shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub ssid: Vec<u8>,
    pub ip: String,
    pub signal: Option<i32>,
    /// The helper's last complaint ("no saved network", "did not join a network in 30 s"), shown in
    /// the strip until the next status.
    pub note: String,
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || s.len() > 64 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `cinder-wifi status`: `state=` `ssid=`(hex) `ip=` `signal=` lines.
pub fn parse_status(text: &str) -> Status {
    let mut st = Status::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "state" => {
                st.state = match v.trim() {
                    "connected" => State::Connected,
                    "connecting" => State::Connecting,
                    "on" => State::On,
                    _ => State::Off,
                }
            }
            "ssid" => st.ssid = unhex(v.trim()).unwrap_or_default(),
            "ip" => {
                st.ip = v
                    .trim()
                    .chars()
                    .filter(|c| c.is_ascii_graphic())
                    .take(45)
                    .collect()
            }
            "signal" => st.signal = v.trim().parse().ok(),
            _ => {}
        }
    }
    st
}

/// `cinder-wifi scan`: `<dBm>\t<open|wep|wpa2|wpa3>\t<ssid hex>` lines. One row per name (the
/// strongest of each — a house with three access points is one network to the person choosing),
/// strongest first.
pub fn parse_scan(text: &str) -> Vec<Network> {
    let mut out: Vec<Network> = Vec::new();
    for line in text.lines() {
        let mut f = line.split('\t');
        let (Some(sig), Some(sec), Some(ssid)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let (Ok(signal), Some(ssid)) = (sig.trim().parse::<i32>(), unhex(ssid.trim())) else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let security = match sec.trim() {
            "open" => Security::Open,
            "wep" => Security::Wep,
            "wpa3" => Security::Wpa3,
            _ => Security::Wpa2,
        };
        match out.iter_mut().find(|n| n.ssid == ssid) {
            Some(n) if n.signal >= signal => {}
            Some(n) => {
                n.signal = signal;
                n.security = security;
            }
            None => out.push(Network {
                signal,
                security,
                ssid,
            }),
        }
    }
    out.sort_by(|a, b| b.signal.cmp(&a.signal).then_with(|| a.ssid.cmp(&b.ssid)));
    out
}

/// Signal as a word: what a person can act on, not a dBm figure.
pub fn strength(dbm: i32) -> &'static str {
    match dbm {
        d if d >= -60 => "strong",
        d if d >= -72 => "good",
        d if d >= -82 => "weak",
        _ => "very weak",
    }
}

/// The strip's one line.
pub fn strip_line(s: &Status) -> String {
    let name = String::from_utf8_lossy(&s.ssid);
    match s.state {
        State::Off if !s.note.is_empty() => s.note.clone(),
        State::Off => crate::i18n::text("OFF").to_string(),
        State::Busy => crate::i18n::text("WORKING…").to_string(),
        State::On if !s.note.is_empty() => s.note.clone(),
        State::On => crate::i18n::text("ON · NOT CONNECTED").to_string(),
        State::Connecting => format!("{} · {name}", crate::i18n::text("CONNECTING")),
        State::Connected => {
            let mut l = format!("{} · {name}", crate::i18n::text("CONNECTED"));
            if !s.ip.is_empty() {
                l.push_str(" · ");
                l.push_str(&s.ip);
            }
            l
        }
    }
}

/// The Settings row's value.
pub fn settings_value(s: &Status) -> &'static str {
    match s.state {
        State::Off => crate::i18n::text("OFF"),
        State::Busy => "…",
        State::On | State::Connecting => crate::i18n::text("ON"),
        State::Connected => crate::i18n::text("CONNECTED"),
    }
}

// ── layout: one function, shared by render and hit ──────────────────────────────────────────────

const FOOT_H: i32 = 44;

struct Layout {
    switch_y: i32,
    strip_y: i32,
    section_y: i32,
    list_y: i32,
    rows: usize,
}

fn layout(networks: usize) -> Layout {
    let switch_y = crate::chrome::HEADER_BOTTOM;
    let strip_y = switch_y + kit::ROW_H();
    let section_y = strip_y + kit::STRIP_H;
    let list_y = section_y + kit::SECTION_H;
    let room = (H as i32 - FOOT_H - list_y).max(0) / kit::ROW_H();
    Layout {
        switch_y,
        strip_y,
        section_y,
        list_y,
        rows: networks.min(room as usize),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Toggle,
    Scan,
    /// Network `i` of the list as given to `render`.
    Network(usize),
    None,
}

pub fn hit(x: i32, y: i32, networks: usize, on: bool) -> Hit {
    let l = layout(networks);
    if y >= l.switch_y && y < l.switch_y + kit::ROW_H() {
        return Hit::Toggle;
    }
    if !on {
        return Hit::None;
    }
    if kit::section_action_hit(l.section_y, x, y) {
        return Hit::Scan;
    }
    if y >= l.list_y && x >= 0 && x < W as i32 {
        let i = ((y - l.list_y) / kit::ROW_H()) as usize;
        if i < l.rows {
            return Hit::Network(i);
        }
    }
    Hit::None
}

pub struct View<'a> {
    pub status: &'a Status,
    pub networks: &'a [Network],
    pub scanning: bool,
    pub sel: Option<usize>,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &View) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Wi-Fi"), None);
    let on = !matches!(v.status.state, State::Off);
    let l = layout(v.networks.len());
    kit::row(
        c,
        t,
        f,
        l.switch_y,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Wi-Fi"))
            .sub(crate::i18n::text("Uses battery while on"))
            .trail(Trail::Switch(on || matches!(v.status.state, State::Busy))),
    );
    kit::strip(c, t, f, l.strip_y, &strip_line(v.status));
    if !on {
        return;
    }
    kit::section_label(
        c,
        t,
        f,
        l.section_y,
        "NETWORKS",
        Some(if v.scanning { "SCANNING…" } else { "SCAN" }),
    );
    let mut y = l.list_y;
    for (i, n) in v.networks.iter().take(l.rows).enumerate() {
        let joined = matches!(v.status.state, State::Connected | State::Connecting)
            && n.ssid == v.status.ssid;
        let name = n.name();
        let sub = format!(
            "{} · {}",
            n.security.label(),
            crate::i18n::text(strength(n.signal))
        );
        let row = Row::new(&name)
            .sub(&sub)
            .trail(if joined {
                Trail::Tag(crate::i18n::text("JOINED"))
            } else {
                Trail::Open("")
            })
            .sel(v.sel == Some(i));
        y = kit::row(c, t, f, y, kit::ROW_H(), &row);
    }
    if v.networks.is_empty() && !v.scanning {
        kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text("No networks yet")).sub(crate::i18n::text("Tap SCAN")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines copied from the helper on the owner's player, 2026-11-04 (addresses scrubbed).
    #[test]
    fn status_from_the_helper() {
        let s = parse_status(
            "state=connected\nssid=534b59545a4d5941\nip=192.0.2.20\nfreq=2437\nsignal=-73\nmac=02:01:2e:83:8c:87\n",
        );
        assert_eq!(s.state, State::Connected);
        assert_eq!(s.ssid, b"SKYTZMYA");
        assert_eq!(s.ip, "192.0.2.20");
        assert_eq!(s.signal, Some(-73));
        assert_eq!(parse_status("state=off\n").state, State::Off);
        assert_eq!(parse_status("").state, State::Off);
    }

    #[test]
    fn scan_keeps_the_strongest_of_each_name_strongest_first() {
        let n = parse_scan(
            "-74\twpa2\t534b59545a4d5941\n-71\twpa2\t766f6461666f6e65\n-70\twpa2\t534b59545a4d5941\n\
             -82\topen\t436166c3a9\n-90\twpa2\t\nnot a line\n-60\twpa2\tzz\n",
        );
        let names: Vec<String> = n.iter().map(|x| x.name()).collect();
        assert_eq!(names, ["SKYTZMYA", "vodafone", "Café"]);
        assert_eq!(
            n[0].signal, -70,
            "the stronger of the two SKYTZMYA access points"
        );
        assert_eq!(n[2].security, Security::Open);
        assert!(!n[2].security.needs_password());
    }

    #[test]
    fn the_switch_answers_when_off_and_nothing_else_does() {
        let l = layout(3);
        assert_eq!(hit(200, l.switch_y + 5, 3, false), Hit::Toggle);
        assert_eq!(hit(200, l.list_y + 5, 3, false), Hit::None);
        assert_eq!(hit(200, l.list_y + 5, 3, true), Hit::Network(0));
        assert_eq!(
            hit(200, l.list_y + kit::ROW_H() + 5, 3, true),
            Hit::Network(1)
        );
        assert_eq!(
            hit(200, l.list_y + 3 * kit::ROW_H() + 5, 3, true),
            Hit::None
        );
    }

    #[test]
    fn strength_words() {
        assert_eq!(strength(-55), "strong");
        assert_eq!(strength(-70), "good");
        assert_eq!(strength(-80), "weak");
        assert_eq!(strength(-90), "very weak");
    }
}
