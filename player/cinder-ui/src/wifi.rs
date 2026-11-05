//! Settings ▸ Wi-Fi. Only offered when the Wi-Fi install option is on the player ([`installed`]):
//! the player's own firmware has no Wi-Fi at all, and Cinder's comes from `wifi/` (kernel modules
//! built from Sony's own GPL source) plus the setuid `cinder-wifi` helper.
//!
//! The shell runs the helper and hands this module its text output ([`parse_status`],
//! [`parse_scan`]); everything here is pure, so the parsing and the layout are host-tested.
//!
//!   ┌ Wi-Fi ───────────────────────────┐    ┌ Network ─────────────────────────┐
//!   │ Wi-Fi                     [■  ] │    │ NETWORKS                   SCAN  │
//!   │ CONNECTED · HOME · 192.168.1.20  │    │ Home              WPA2 · strong  │
//!   │ Network  Home                  › │ →  │ Cafe                OPEN · weak  │
//!   │ Internet radio                 › │    │ …  strongest first; tap to join, │
//!   │ Phone remote                   › │    │    the joined one to forget it   │
//!   │ AirPlay                   [   ] │    └──────────────────────────────────┘
//!   │ Options                        › │ →  Turn on at start · Set the clock ·
//!   └──────────────────────────────────┘    ListenBrainz · Check for updates · lyrics
//!                                          (every switch off unless switched on)

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

// ── Settings ▸ Wi-Fi: one function for layout, shared by render and hit ──────────────────────────

const FOOT_H: i32 = 44;

/// Settings ▸ Wi-Fi ▸ Options, in order. Each is a saved switch, off unless switched on.
pub const OPTIONS: [Hit; 8] = [
    Hit::AutoStart,
    Hit::Clock,
    Hit::ListenBrainz,
    Hit::Updates,
    Hit::Lyrics,
    Hit::Art,
    Hit::PlayTo,
    Hit::Drive,
];

struct Layout {
    switch_y: i32,
    strip_y: i32,
    /// "Network ›", "Internet radio ›", "Speakers ›" and "Phone remote ›", only while the radio
    /// is on.
    net_y: Option<i32>,
    radio_y: Option<i32>,
    speakers_y: Option<i32>,
    remote_y: Option<i32>,
    airplay_y: i32,
    options_y: i32,
}

fn layout(on: bool) -> Layout {
    let switch_y = crate::chrome::HEADER_BOTTOM;
    let strip_y = switch_y + kit::ROW_H();
    let mut y = strip_y + kit::STRIP_H;
    let mut next = || {
        y += kit::ROW_H();
        y - kit::ROW_H()
    };
    let net_y = on.then(&mut next);
    let radio_y = on.then(&mut next);
    let speakers_y = on.then(&mut next);
    let remote_y = on.then(&mut next);
    let airplay_y = next();
    let options_y = next();
    Layout {
        switch_y,
        strip_y,
        net_y,
        radio_y,
        speakers_y,
        remote_y,
        airplay_y,
        options_y,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Toggle,
    /// "Network ›": the list of networks in range.
    Networks,
    /// "Internet radio ›".
    Radio,
    /// "Speakers ›": play on a speaker on the network.
    Speakers,
    /// "Phone remote ›".
    Remote,
    /// The AirPlay receiver's switch.
    AirPlay,
    /// "Options ›".
    Options,
    // Settings ▸ Wi-Fi ▸ Options:
    AutoStart,
    Clock,
    ListenBrainz,
    Updates,
    Lyrics,
    Art,
    /// Play to this player: the DLNA renderer.
    PlayTo,
    /// Share the drive (WebDAV), on until the player restarts.
    Drive,
    None,
}

pub fn hit(_x: i32, y: i32, on: bool) -> Hit {
    let l = layout(on);
    let within = |top: i32| y >= top && y < top + kit::ROW_H();
    if within(l.switch_y) {
        return Hit::Toggle;
    }
    for (row, h) in [
        (l.net_y, Hit::Networks),
        (l.radio_y, Hit::Radio),
        (l.speakers_y, Hit::Speakers),
        (l.remote_y, Hit::Remote),
    ] {
        if row.is_some_and(within) {
            return h;
        }
    }
    if within(l.airplay_y) {
        return Hit::AirPlay;
    }
    if within(l.options_y) {
        return Hit::Options;
    }
    Hit::None
}

/// Settings ▸ Wi-Fi ▸ Options: which switch a tap is on.
pub fn hit_options(y: i32) -> Hit {
    let top = crate::chrome::HEADER_BOTTOM;
    if y < top {
        return Hit::None;
    }
    OPTIONS
        .get(((y - top) / kit::ROW_H()) as usize)
        .copied()
        .unwrap_or(Hit::None)
}

pub struct View<'a> {
    pub status: &'a Status,
    /// The Internet radio row's second line: what is playing, or how many stations.
    pub radio_note: &'a str,
    /// The Speakers row's second line: what it does, or where the music is playing.
    pub speakers_note: &'a str,
    /// The Phone remote row's second line: off, or the address to open.
    pub remote_note: &'a str,
    /// The AirPlay switch (saved), and what the receiver is doing.
    pub airplay: bool,
    pub airplay_note: &'a str,
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, v: &View) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Wi-Fi"), None);
    let on = !matches!(v.status.state, State::Off);
    let l = layout(on);
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
    let joined = matches!(v.status.state, State::Connected | State::Connecting);
    let name = String::from_utf8_lossy(&v.status.ssid).into_owned();
    let open_rows: [(Option<i32>, &str, &str); 4] = [
        (
            l.net_y,
            crate::i18n::text("Network"),
            if joined {
                name.as_str()
            } else {
                crate::i18n::text("Choose a network")
            },
        ),
        (l.radio_y, crate::i18n::text("Internet radio"), v.radio_note),
        (
            l.speakers_y,
            crate::i18n::text("Speakers"),
            if v.speakers_note.is_empty() {
                crate::i18n::text("Play on a speaker on this Wi-Fi")
            } else {
                v.speakers_note
            },
        ),
        (l.remote_y, crate::i18n::text("Phone remote"), v.remote_note),
    ];
    for (y, title, sub) in open_rows {
        if let Some(y) = y {
            kit::row(
                c,
                t,
                f,
                y,
                kit::ROW_H(),
                &Row::new(title).sub(sub).trail(Trail::Open("")),
            );
        }
    }
    kit::row(
        c,
        t,
        f,
        l.airplay_y,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("AirPlay"))
            .sub(if v.airplay_note.is_empty() {
                crate::i18n::text("Play from an iPhone or a Mac")
            } else {
                v.airplay_note
            })
            .trail(Trail::Switch(v.airplay)),
    );
    kit::row(
        c,
        t,
        f,
        l.options_y,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Options"))
            .sub(crate::i18n::text("Start · clock · ListenBrainz · updates"))
            .trail(Trail::Open("")),
    );
}

pub struct OptionsView<'a> {
    /// The saved switches, in [`OPTIONS`] order.
    pub options: [bool; 8],
    /// What the last ListenBrainz send and update check said ("Sent 12 songs", "0.3.23 is out").
    pub lb_note: &'a str,
    pub update_note: &'a str,
    /// What the renderer is doing ("Shows up as Walkman (Cinder)", "Playing So What").
    pub play_to_note: &'a str,
    /// The shared drive's address while it is on ("http://192.168.1.20:8081/k7m2qx9p").
    pub drive_note: &'a str,
}

pub fn render_options(c: &mut Canvas, t: &Theme, f: &FontSet, v: &OptionsView) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Wi-Fi options"), None);
    let rows: [(&str, &str); 8] = [
        (
            crate::i18n::text("Turn on at start"),
            crate::i18n::text("Joins your network when the player starts"),
        ),
        (
            crate::i18n::text("Set the clock"),
            crate::i18n::text("From the internet; keeps your time zone"),
        ),
        (
            crate::i18n::text("Send plays to ListenBrainz"),
            if v.lb_note.is_empty() {
                crate::i18n::text("Put your token in cinder_listenbrainz.txt on the drive")
            } else {
                v.lb_note
            },
        ),
        (
            crate::i18n::text("Check for updates"),
            if v.update_note.is_empty() {
                crate::i18n::text("Says when a new Cinder is out")
            } else {
                v.update_note
            },
        ),
        (
            crate::i18n::text("Fetch missing lyrics"),
            crate::i18n::text("From LRCLIB, saved beside the song"),
        ),
        (
            crate::i18n::text("Fetch missing album art"),
            crate::i18n::text("From MusicBrainz, saved as cover.jpg"),
        ),
        (
            crate::i18n::text("Play to this player"),
            if v.play_to_note.is_empty() {
                crate::i18n::text("Phones and computers can send music here")
            } else {
                v.play_to_note
            },
        ),
        (
            crate::i18n::text("Share the drive"),
            if v.drive_note.is_empty() {
                crate::i18n::text("Map it on a computer · off at each start")
            } else {
                v.drive_note
            },
        ),
    ];
    let mut y = crate::chrome::HEADER_BOTTOM;
    for ((title, sub), on) in rows.iter().zip(v.options) {
        y = kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(title).sub(sub).trail(Trail::Switch(on)),
        );
    }
}

// ── Settings ▸ Wi-Fi ▸ Network: the networks in range ───────────────────────────────────────────

fn list_layout(networks: usize) -> (i32, i32, usize) {
    let section_y = crate::chrome::HEADER_BOTTOM;
    let list_y = section_y + kit::SECTION_H;
    let room = (H as i32 - FOOT_H - list_y).max(0) / kit::ROW_H();
    (section_y, list_y, networks.min(room as usize))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListHit {
    Scan,
    /// Network `i` of the list as given to `render_list`.
    Network(usize),
    None,
}

pub fn hit_list(x: i32, y: i32, networks: usize) -> ListHit {
    let (section_y, list_y, rows) = list_layout(networks);
    if kit::section_action_hit(section_y, x, y) {
        return ListHit::Scan;
    }
    if y >= list_y && x >= 0 && x < W as i32 {
        let i = ((y - list_y) / kit::ROW_H()) as usize;
        if i < rows {
            return ListHit::Network(i);
        }
    }
    ListHit::None
}

pub struct ListView<'a> {
    pub status: &'a Status,
    pub networks: &'a [Network],
    pub scanning: bool,
    pub sel: Option<usize>,
}

pub fn render_list(c: &mut Canvas, t: &Theme, f: &FontSet, v: &ListView) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Network"), None);
    let (section_y, list_y, rows) = list_layout(v.networks.len());
    kit::section_label(
        c,
        t,
        f,
        section_y,
        "NETWORKS",
        Some(if v.scanning { "SCANNING…" } else { "SCAN" }),
    );
    let mut y = list_y;
    for (i, n) in v.networks.iter().take(rows).enumerate() {
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
    if v.networks.is_empty() {
        let (title, sub) = if matches!(v.status.state, State::Off) {
            ("Wi-Fi is off", "Switch it on first")
        } else if v.scanning {
            ("Looking for networks…", "This takes a few seconds")
        } else {
            ("No networks yet", "Tap SCAN")
        };
        kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text(title)).sub(crate::i18n::text(sub)),
        );
    }
}

// ── Settings ▸ Wi-Fi ▸ Speakers: speakers on the network (UPnP/DLNA renderers) ─────────────────────

/// Where the rows start: under the strip and the section label. While a cast is on, the first row
/// is "Play here".
fn speakers_layout(speakers: usize, casting: bool) -> (i32, i32, i32, usize) {
    let strip_y = crate::chrome::HEADER_BOTTOM;
    let section_y = strip_y + kit::STRIP_H;
    let list_y = section_y + kit::SECTION_H;
    let room = ((H as i32 - FOOT_H - list_y).max(0) / kit::ROW_H()) as usize;
    (
        strip_y,
        section_y,
        list_y,
        speakers.min(room.saturating_sub(casting as usize)),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeakerHit {
    Scan,
    /// "Play here": the music comes back to the player.
    Here,
    /// Speaker `i` of the list as given to `render_speakers`.
    Speaker(usize),
    None,
}

pub fn hit_speakers(x: i32, y: i32, speakers: usize, casting: bool) -> SpeakerHit {
    let (_, section_y, list_y, rows) = speakers_layout(speakers, casting);
    if kit::section_action_hit(section_y, x, y) {
        return SpeakerHit::Scan;
    }
    if y < list_y {
        return SpeakerHit::None;
    }
    let mut i = ((y - list_y) / kit::ROW_H()) as usize;
    if casting {
        if i == 0 {
            return SpeakerHit::Here;
        }
        i -= 1;
    }
    if i < rows {
        SpeakerHit::Speaker(i)
    } else {
        SpeakerHit::None
    }
}

pub struct SpeakersView<'a> {
    /// (name, model) of each speaker found.
    pub speakers: &'a [(String, String)],
    pub scanning: bool,
    pub connected: bool,
    /// The speaker playing, while a cast is on.
    pub casting: Option<&'a str>,
    /// The strip: what is playing where, or why the last cast ended.
    pub line: &'a str,
}

pub fn render_speakers(c: &mut Canvas, t: &Theme, f: &FontSet, v: &SpeakersView) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Speakers"), None);
    let casting = v.casting.is_some();
    let (strip_y, section_y, list_y, rows) = speakers_layout(v.speakers.len(), casting);
    let line = if !v.connected {
        crate::i18n::text("CONNECT TO WI-FI FIRST")
    } else if v.line.is_empty() {
        crate::i18n::text("ON THE PLAYER")
    } else {
        v.line
    };
    kit::strip(c, t, f, strip_y, line);
    kit::section_label(
        c,
        t,
        f,
        section_y,
        "ON THIS WI-FI",
        v.connected
            .then_some(if v.scanning { "SCANNING…" } else { "SCAN" }),
    );
    let mut y = list_y;
    if casting {
        y = kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text("Play here"))
                .sub(crate::i18n::text("Bring the music back to the player"))
                .trail(Trail::Open("")),
        );
    }
    for (name, model) in v.speakers.iter().take(rows) {
        let here = v.casting == Some(name.as_str());
        let row = Row::new(name).sub(model).trail(if here {
            Trail::Tag(crate::i18n::text("PLAYING"))
        } else {
            Trail::Open("")
        });
        y = kit::row(c, t, f, y, kit::ROW_H(), &row);
    }
    if v.speakers.is_empty() && v.connected {
        let (title, sub) = if v.scanning {
            ("Looking for speakers…", "This takes a few seconds")
        } else {
            (
                "No speakers found",
                "They need to be on this Wi-Fi and play UPnP or DLNA",
            )
        };
        kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text(title)).sub(crate::i18n::text(sub)),
        );
    }
}

// ── Settings ▸ Wi-Fi ▸ Phone remote ─────────────────────────────────────────────────────────────

/// The page's one control: the switch. Everything else on it is text to read.
pub fn hit_remote(y: i32) -> bool {
    let top = crate::chrome::HEADER_BOTTOM;
    y >= top && y < top + kit::ROW_H()
}

pub struct RemoteView<'a> {
    pub on: bool,
    pub wifi_connected: bool,
    /// `http://192.168.0.20:8080` while on.
    pub address: &'a str,
    pub code: &'a str,
}

pub fn render_remote(c: &mut Canvas, t: &Theme, f: &FontSet, v: &RemoteView) {
    use crate::text::{Family, Weight};
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Phone remote"), None);
    let mut y = kit::row(
        c,
        t,
        f,
        crate::chrome::HEADER_BOTTOM,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Phone remote"))
            .sub(crate::i18n::text(
                "Play, skip, volume and sending music, from a phone",
            ))
            .trail(Trail::Switch(v.on)),
    );
    let line = |c: &mut Canvas, y: i32, s: &str, size: f32, w: Weight, ink| {
        crate::widgets::center(
            c,
            f,
            (W / 2) as f32,
            y as f32,
            s,
            &crate::widgets::sty(Family::Sans, w, size, ink, 0.0),
        );
    };
    if !v.wifi_connected {
        kit::strip(c, t, f, y, crate::i18n::text("CONNECT TO WI-FI FIRST"));
        return;
    }
    if !v.on {
        kit::strip(c, t, f, y, crate::i18n::text("OFF"));
        return;
    }
    y += 70;
    line(
        c,
        y,
        crate::i18n::text("On a phone on this Wi-Fi, open"),
        18.0,
        Weight::Regular,
        t.dim,
    );
    y += 44;
    line(c, y, v.address, 24.0, Weight::SemiBold, t.ink);
    y += 70;
    line(
        c,
        y,
        crate::i18n::text("and enter the code"),
        18.0,
        Weight::Regular,
        t.dim,
    );
    y += 64;
    let spaced: String = v
        .code
        .chars()
        .enumerate()
        .flat_map(|(i, ch)| {
            (i == 3)
                .then_some(' ')
                .into_iter()
                .chain(std::iter::once(ch))
        })
        .collect();
    line(c, y, &spaced, 48.0, Weight::SemiBold, t.acc);
    y += 70;
    line(
        c,
        y,
        crate::i18n::text("A new code each time you switch it on."),
        15.0,
        Weight::Regular,
        t.faint,
    );
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
    fn the_rows_answer_where_they_are_drawn() {
        let off = layout(false);
        assert_eq!(hit(200, off.switch_y + 5, false), Hit::Toggle);
        assert!(off.net_y.is_none() && off.radio_y.is_none() && off.remote_y.is_none());
        assert!(
            off.speakers_y.is_none(),
            "no Speakers row while Wi-Fi is off"
        );
        assert_eq!(hit(200, off.airplay_y + 5, false), Hit::AirPlay);
        assert_eq!(hit(200, off.options_y + 5, false), Hit::Options);
        let on = layout(true);
        assert_eq!(hit(200, on.net_y.unwrap() + 5, true), Hit::Networks);
        assert_eq!(hit(200, on.radio_y.unwrap() + 5, true), Hit::Radio);
        assert_eq!(hit(200, on.speakers_y.unwrap() + 5, true), Hit::Speakers);
        assert_eq!(hit(200, on.remote_y.unwrap() + 5, true), Hit::Remote);
        assert_eq!(hit(200, on.airplay_y + 5, true), Hit::AirPlay);
        assert_eq!(hit(200, on.options_y + 5, true), Hit::Options);
        assert!(
            on.options_y + kit::ROW_H() <= H as i32,
            "every row fits at this size"
        );
        let top = crate::chrome::HEADER_BOTTOM;
        for (i, want) in OPTIONS.iter().enumerate() {
            assert_eq!(hit_options(top + i as i32 * kit::ROW_H() + 5), *want);
        }
        assert_eq!(
            hit_options(top + OPTIONS.len() as i32 * kit::ROW_H() + 5),
            Hit::None
        );
        assert!(
            top + OPTIONS.len() as i32 * kit::ROW_H() <= H as i32,
            "every option fits at this size"
        );
    }

    #[test]
    fn the_speakers_page_answers_play_here_then_its_speakers() {
        let (_, section_y, list_y, _) = speakers_layout(3, false);
        assert_eq!(
            hit_speakers(200, list_y + 5, 3, false),
            SpeakerHit::Speaker(0)
        );
        assert_eq!(
            hit_speakers(200, list_y + kit::ROW_H() + 5, 3, false),
            SpeakerHit::Speaker(1)
        );
        assert_eq!(
            hit_speakers(200, list_y + 3 * kit::ROW_H() + 5, 3, false),
            SpeakerHit::None
        );
        assert_eq!(hit_speakers(200, list_y + 5, 3, true), SpeakerHit::Here);
        assert_eq!(
            hit_speakers(200, list_y + kit::ROW_H() + 5, 3, true),
            SpeakerHit::Speaker(0)
        );
        assert_eq!(hit_speakers(200, list_y - 5, 3, true), SpeakerHit::None);
        assert_eq!(
            hit_speakers(W as i32 - 30, section_y + 10, 0, false),
            SpeakerHit::Scan
        );
    }

    #[test]
    fn the_network_list_answers_its_rows_only() {
        let (_, list_y, _) = list_layout(3);
        assert_eq!(hit_list(200, list_y + 5, 3), ListHit::Network(0));
        assert_eq!(
            hit_list(200, list_y + kit::ROW_H() + 5, 3),
            ListHit::Network(1)
        );
        assert_eq!(
            hit_list(200, list_y + 3 * kit::ROW_H() + 5, 3),
            ListHit::None
        );
        let (_, _, rows) = list_layout(40);
        assert!(rows >= 9, "the list page has room for {rows} networks");
    }

    #[test]
    fn strength_words() {
        assert_eq!(strength(-55), "strong");
        assert_eq!(strength(-70), "good");
        assert_eq!(strength(-80), "weak");
        assert_eq!(strength(-90), "very weak");
    }
}
