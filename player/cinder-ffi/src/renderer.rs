//! Settings ▸ Wi-Fi ▸ Options ▸ Play to this player: a UPnP/DLNA media renderer, so a phone app
//! (BubbleUPnP, Hi-Fi Cast), Windows' "Cast to device" or a music server's own app can play on the
//! player. Opt-in, off by default, and only while Wi-Fi is connected.
//!
//! The player answers SSDP (239.255.255.250:1900) as a MediaRenderer, serves its description and
//! the three services' SCPDs, and takes SOAP for AVTransport, RenderingControl and
//! ConnectionManager. A controller sends a URL; the shell plays it through the internet radio's
//! player (curl, then the MP3 or AAC decoder, to the jack or Bluetooth) as station -2, and the
//! radio's status comes back here ([`radio_status`]) so the controller hears PLAYING and STOPPED,
//! which is what moves it on to the next song. Events (GENA) go to subscribers as LastChange.
//!
//! NO LOCK. UPnP has none: anyone on the same network can play here while the switch is on, as on
//! any network speaker. Only http and https links are taken, so a controller cannot have the
//! player read its own files. Pause stops (a stream cannot be held); Play after a pause starts the
//! song again. Seek, Next and Previous are refused (the controller does those by sending a link).
//! The volume is reported and remembered but not applied: the player's own buttons set it.
// ponytail: volume not applied, pause restarts the song; byte-range resume when someone asks.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::cast::{escape, tag};
use crate::remote::{read_head, Request};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
/// The formats the radio's decoders play, as a sink's protocolInfo list.
const SINK: &str = "http-get:*:audio/mpeg:*,http-get:*:audio/mp3:*,http-get:*:audio/x-mpeg:*,\
http-get:*:audio/aac:*,http-get:*:audio/aacp:*,http-get:*:audio/x-aac:*,http-get:*:audio/vnd.dlna.adts:*,\
http-get:*:application/vnd.apple.mpegurl:*,http-get:*:audio/x-mpegurl:*,http-get:*:audio/mpegurl:*";

/// What the shell is asked to do ([`take`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Play(String),
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Svc {
    Avt,
    Rc,
    Cm,
}

struct Sub {
    sid: String,
    url: String,
    svc: Svc,
    seq: u32,
    until: Instant,
}

#[derive(Default)]
struct State {
    name: String,
    uuid: String,
    base: String,
    /// NO_MEDIA_PRESENT, STOPPED, TRANSITIONING, PLAYING, PAUSED_PLAYBACK.
    transport: &'static str,
    uri: String,
    meta: String,
    title: String,
    dur_ms: i64,
    since: Option<Instant>,
    /// A Play was asked for and the radio has not yet said it is connecting or playing.
    awaiting: Option<Instant>,
    ask: Option<Ask>,
    volume: u32,
    mute: bool,
    note: String,
    subs: Vec<Sub>,
}

static RUN: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

pub fn running() -> bool {
    RUN.load(Ordering::SeqCst)
}

/// For the shell, once a second: what a controller asked for since the last call.
pub fn take() -> Option<Ask> {
    state().lock().unwrap().ask.take()
}

/// What the renderer is doing, for the switch's second line ("" = the default text).
pub fn note() -> String {
    state().lock().unwrap().note.clone()
}

/// A stable id from the Wi-Fi chip's MAC, so a controller keeps knowing the player.
pub fn uuid_from_mac(mac: &str) -> String {
    let hex: String = mac
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_lowercase();
    let hex = format!("{hex:0>12}");
    format!("43696e64-6572-4000-8000-{}", &hex[hex.len() - 12..])
}

/// Start answering on `ip` as `name`. Returns the HTTP port.
pub fn start(ip: Ipv4Addr, name: &str, uuid: &str) -> Result<u16, String> {
    if running() {
        stop();
        std::thread::sleep(Duration::from_millis(1200));
    }
    let ssdp = UdpSocket::bind(("0.0.0.0", 1900)).map_err(|e| format!("SSDP port: {e}"))?;
    ssdp.join_multicast_v4(&GROUP, &ip)
        .map_err(|e| format!("SSDP group: {e}"))?;
    ssdp.set_read_timeout(Some(Duration::from_millis(500))).ok();
    let mut bound = None;
    for port in 49494..49504u16 {
        if let Ok(l) = TcpListener::bind((ip, port)) {
            bound = Some((l, port));
            break;
        }
    }
    let (l, port) = bound.ok_or("no free port")?;
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    {
        let mut s = state().lock().unwrap();
        *s = State {
            name: name.to_string(),
            uuid: uuid.to_string(),
            base: format!("http://{ip}:{port}"),
            transport: "NO_MEDIA_PRESENT",
            volume: 50,
            ..State::default()
        };
        s.note = cinder_ui::ui_format!("Shows up as {}", name);
    }
    RUN.store(true, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("cinder-dlna-http".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            while running() {
                match l.accept() {
                    Ok((c, _)) => {
                        let _ = c.set_nonblocking(false);
                        let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
                        let _ = c.set_write_timeout(Some(Duration::from_secs(10)));
                        serve(c);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("cinder-dlna-ssdp".into())
        .stack_size(128 * 1024)
        .spawn(move || ssdp_loop(ssdp))
        .map_err(|e| e.to_string())?;
    Ok(port)
}

/// Say goodbye on the network and stop. Whatever is playing from a controller stops too.
pub fn stop() {
    if !RUN.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut s = state().lock().unwrap();
    if matches!(s.transport, "PLAYING" | "TRANSITIONING") {
        s.ask = Some(Ask::Stop);
    }
    s.subs.clear();
    s.note.clear();
}

// ── The radio's status, from the shell ────────────────────────────────────────────────────────────

/// The internet radio's status text (`state=`, `station=`, `note=`): station -2 is ours.
pub fn radio_status(text: &str) {
    if !running() {
        return;
    }
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let (st, station, why) = (field("state"), field("station"), field("note"));
    let mut s = state().lock().unwrap();
    let before = s.transport;
    let ours = station == "-2";
    match (st.as_str(), ours) {
        ("connecting", true) => {
            s.awaiting = None;
            s.transport = "TRANSITIONING";
        }
        ("playing", true) => {
            s.awaiting = None;
            if s.transport != "PLAYING" {
                s.since = Some(Instant::now());
            }
            s.transport = "PLAYING";
            s.note = cinder_ui::ui_format!(
                "Playing {}",
                if s.title.is_empty() {
                    "from the network"
                } else {
                    s.title.as_str()
                }
            );
        }
        // Another station took the output while ours was on.
        ("connecting" | "playing", false) if matches!(s.transport, "PLAYING" | "TRANSITIONING") => {
            s.transport = "STOPPED";
            s.since = None;
        }
        ("off", _) if matches!(s.transport, "PLAYING" | "TRANSITIONING") => {
            // While a Play is on its way, the off is the last song's (or another station's) ending,
            // unless it says why ours failed.
            let waiting = s
                .awaiting
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30));
            if !waiting || (!why.is_empty() && ours) {
                s.awaiting = None;
                s.transport = "STOPPED";
                s.since = None;
                s.note = if why.is_empty() {
                    cinder_ui::ui_format!("Shows up as {}", s.name)
                } else {
                    why
                };
            }
        }
        _ => {}
    }
    if s.transport != before {
        notify(&mut s, Svc::Avt);
    }
}

// ── SSDP ──────────────────────────────────────────────────────────────────────────────────────────

fn targets(uuid: &str) -> Vec<(String, String)> {
    let mut v = vec![
        (
            "upnp:rootdevice".to_string(),
            format!("uuid:{uuid}::upnp:rootdevice"),
        ),
        (format!("uuid:{uuid}"), format!("uuid:{uuid}")),
    ];
    for t in [
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:service:AVTransport:1",
        "urn:schemas-upnp-org:service:RenderingControl:1",
        "urn:schemas-upnp-org:service:ConnectionManager:1",
    ] {
        v.push((t.to_string(), format!("uuid:{uuid}::{t}")));
    }
    v
}

/// The answers to an M-SEARCH (`ssdp:all` gets one per target; nothing for a target we are not).
pub fn search_answers(msg: &str, uuid: &str, base: &str) -> Vec<String> {
    let mut lines = msg.lines();
    if !lines.next().is_some_and(|l| l.starts_with("M-SEARCH")) {
        return Vec::new();
    }
    let st = msg
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("ST")
                .then(|| v.trim().to_string())
        })
        .unwrap_or_default();
    targets(uuid)
        .into_iter()
        .filter(|(nt, _)| st == "ssdp:all" || *nt == st)
        .map(|(nt, usn)| {
            format!(
                "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nEXT:\r\nLOCATION: {base}/desc.xml\r\n\
                 SERVER: Linux/3.10 UPnP/1.0 Cinder/1\r\nST: {nt}\r\nUSN: {usn}\r\n\r\n"
            )
        })
        .collect()
}

fn notify_all(sock: &UdpSocket, uuid: &str, base: &str, nts: &str) {
    for (nt, usn) in targets(uuid) {
        let m = format!(
            "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\n\
             LOCATION: {base}/desc.xml\r\nNT: {nt}\r\nNTS: {nts}\r\nSERVER: Linux/3.10 UPnP/1.0 Cinder/1\r\nUSN: {usn}\r\n\r\n"
        );
        let _ = sock.send_to(m.as_bytes(), (GROUP, 1900));
    }
}

fn ssdp_loop(sock: UdpSocket) {
    let (uuid, base) = {
        let s = state().lock().unwrap();
        (s.uuid.clone(), s.base.clone())
    };
    let mut buf = [0u8; 2048];
    let mut last = Instant::now() - Duration::from_secs(3600);
    let mut said = 0;
    while running() {
        // Twice at the start (UDP is lossy), then every 15 minutes, well inside max-age.
        let every = if said < 2 { 3 } else { 900 };
        if last.elapsed() >= Duration::from_secs(every) {
            notify_all(&sock, &uuid, &base, "ssdp:alive");
            last = Instant::now();
            said += 1;
        }
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            let msg = String::from_utf8_lossy(&buf[..n]);
            for a in search_answers(&msg, &uuid, &base) {
                let _ = sock.send_to(a.as_bytes(), from);
            }
        }
    }
    notify_all(&sock, &uuid, &base, "ssdp:byebye");
}

// ── Description and SCPDs ─────────────────────────────────────────────────────────────────────────

pub fn description(name: &str, uuid: &str) -> String {
    let svc = |t: &str, id: &str, p: &str| {
        format!(
            "<service><serviceType>urn:schemas-upnp-org:service:{t}:1</serviceType>\
             <serviceId>urn:upnp-org:serviceId:{id}</serviceId><SCPDURL>/{p}.xml</SCPDURL>\
             <controlURL>/ctl/{p}</controlURL><eventSubURL>/evt/{p}</eventSubURL></service>"
        )
    };
    format!(
        "<?xml version=\"1.0\"?>\n<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major>\
         <minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>\
         <friendlyName>{}</friendlyName><manufacturer>Cinder</manufacturer><modelName>NW-A50 series</modelName>\
         <UDN>uuid:{uuid}</UDN><dlna:X_DLNADOC xmlns:dlna=\"urn:schemas-dlna-org:device-1-0\">DMR-1.50</dlna:X_DLNADOC>\
         <serviceList>{}{}{}</serviceList></device></root>",
        escape(name),
        svc("AVTransport", "AVTransport", "avt"),
        svc("RenderingControl", "RenderingControl", "rc"),
        svc("ConnectionManager", "ConnectionManager", "cm"),
    )
}

/// (action, [(argument, in?, state variable)]).
type Action = (&'static str, &'static [(&'static str, bool, &'static str)]);
const ID: (&str, bool, &str) = ("InstanceID", true, "A_ARG_TYPE_InstanceID");

const AVT: &[Action] = &[
    (
        "SetAVTransportURI",
        &[
            ID,
            ("CurrentURI", true, "AVTransportURI"),
            ("CurrentURIMetaData", true, "AVTransportURIMetaData"),
        ],
    ),
    (
        "GetMediaInfo",
        &[
            ID,
            ("NrTracks", false, "NumberOfTracks"),
            ("MediaDuration", false, "CurrentMediaDuration"),
            ("CurrentURI", false, "AVTransportURI"),
            ("CurrentURIMetaData", false, "AVTransportURIMetaData"),
            ("NextURI", false, "NextAVTransportURI"),
            ("NextURIMetaData", false, "NextAVTransportURIMetaData"),
            ("PlayMedium", false, "PlaybackStorageMedium"),
            ("RecordMedium", false, "RecordStorageMedium"),
            ("WriteStatus", false, "RecordMediumWriteStatus"),
        ],
    ),
    (
        "GetTransportInfo",
        &[
            ID,
            ("CurrentTransportState", false, "TransportState"),
            ("CurrentTransportStatus", false, "TransportStatus"),
            ("CurrentSpeed", false, "TransportPlaySpeed"),
        ],
    ),
    (
        "GetPositionInfo",
        &[
            ID,
            ("Track", false, "CurrentTrack"),
            ("TrackDuration", false, "CurrentTrackDuration"),
            ("TrackMetaData", false, "CurrentTrackMetaData"),
            ("TrackURI", false, "CurrentTrackURI"),
            ("RelTime", false, "RelativeTimePosition"),
            ("AbsTime", false, "AbsoluteTimePosition"),
            ("RelCount", false, "RelativeCounterPosition"),
            ("AbsCount", false, "AbsoluteCounterPosition"),
        ],
    ),
    (
        "GetDeviceCapabilities",
        &[
            ID,
            ("PlayMedia", false, "PossiblePlaybackStorageMedia"),
            ("RecMedia", false, "PossibleRecordStorageMedia"),
            ("RecQualityModes", false, "PossibleRecordQualityModes"),
        ],
    ),
    (
        "GetTransportSettings",
        &[
            ID,
            ("PlayMode", false, "CurrentPlayMode"),
            ("RecQualityMode", false, "CurrentRecordQualityMode"),
        ],
    ),
    (
        "GetCurrentTransportActions",
        &[ID, ("Actions", false, "CurrentTransportActions")],
    ),
    ("Stop", &[ID]),
    ("Play", &[ID, ("Speed", true, "TransportPlaySpeed")]),
    ("Pause", &[ID]),
    (
        "Seek",
        &[
            ID,
            ("Unit", true, "A_ARG_TYPE_SeekMode"),
            ("Target", true, "A_ARG_TYPE_SeekTarget"),
        ],
    ),
    ("Next", &[ID]),
    ("Previous", &[ID]),
];
const RC: &[Action] = &[
    (
        "ListPresets",
        &[ID, ("CurrentPresetNameList", false, "PresetNameList")],
    ),
    (
        "SelectPreset",
        &[ID, ("PresetName", true, "A_ARG_TYPE_PresetName")],
    ),
    (
        "GetVolume",
        &[
            ID,
            ("Channel", true, "A_ARG_TYPE_Channel"),
            ("CurrentVolume", false, "Volume"),
        ],
    ),
    (
        "SetVolume",
        &[
            ID,
            ("Channel", true, "A_ARG_TYPE_Channel"),
            ("DesiredVolume", true, "Volume"),
        ],
    ),
    (
        "GetMute",
        &[
            ID,
            ("Channel", true, "A_ARG_TYPE_Channel"),
            ("CurrentMute", false, "Mute"),
        ],
    ),
    (
        "SetMute",
        &[
            ID,
            ("Channel", true, "A_ARG_TYPE_Channel"),
            ("DesiredMute", true, "Mute"),
        ],
    ),
];
const CM: &[Action] = &[
    (
        "GetProtocolInfo",
        &[
            ("Source", false, "SourceProtocolInfo"),
            ("Sink", false, "SinkProtocolInfo"),
        ],
    ),
    (
        "GetCurrentConnectionIDs",
        &[("ConnectionIDs", false, "CurrentConnectionIDs")],
    ),
    (
        "GetCurrentConnectionInfo",
        &[
            ("ConnectionID", true, "A_ARG_TYPE_ConnectionID"),
            ("RcsID", false, "A_ARG_TYPE_RcsID"),
            ("AVTransportID", false, "A_ARG_TYPE_AVTransportID"),
            ("ProtocolInfo", false, "A_ARG_TYPE_ProtocolInfo"),
            (
                "PeerConnectionManager",
                false,
                "A_ARG_TYPE_ConnectionManager",
            ),
            ("PeerConnectionID", false, "A_ARG_TYPE_ConnectionID"),
            ("Direction", false, "A_ARG_TYPE_Direction"),
            ("Status", false, "A_ARG_TYPE_ConnectionStatus"),
        ],
    ),
];

fn actions(svc: Svc) -> &'static [Action] {
    match svc {
        Svc::Avt => AVT,
        Svc::Rc => RC,
        Svc::Cm => CM,
    }
}

fn scpd(svc: Svc) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\"?>\n<scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major>\
         <minor>0</minor></specVersion><actionList>",
    );
    let mut vars: Vec<&str> = Vec::new();
    for (name, args) in actions(svc) {
        out.push_str(&format!("<action><name>{name}</name><argumentList>"));
        for (a, inn, v) in *args {
            out.push_str(&format!(
                "<argument><name>{a}</name><direction>{}</direction><relatedStateVariable>{v}</relatedStateVariable></argument>",
                if *inn { "in" } else { "out" }
            ));
            if !vars.contains(v) {
                vars.push(v);
            }
        }
        out.push_str("</argumentList></action>");
    }
    out.push_str("</actionList><serviceStateTable>");
    if svc != Svc::Cm {
        vars.push("LastChange");
    }
    for v in vars {
        let ty = match v {
            "A_ARG_TYPE_InstanceID" | "NumberOfTracks" | "CurrentTrack" => "ui4",
            "RelativeCounterPosition"
            | "AbsoluteCounterPosition"
            | "A_ARG_TYPE_ConnectionID"
            | "A_ARG_TYPE_RcsID"
            | "A_ARG_TYPE_AVTransportID" => "i4",
            "Volume" => "ui2",
            "Mute" => "boolean",
            _ => "string",
        };
        let events = matches!(
            v,
            "LastChange" | "SourceProtocolInfo" | "SinkProtocolInfo" | "CurrentConnectionIDs"
        );
        out.push_str(&format!(
            "<stateVariable sendEvents=\"{}\"><name>{v}</name><dataType>{ty}</dataType></stateVariable>",
            if events { "yes" } else { "no" }
        ));
    }
    out.push_str("</serviceStateTable></scpd>");
    out
}

// ── SOAP ──────────────────────────────────────────────────────────────────────────────────────────

fn svc_name(svc: Svc) -> &'static str {
    match svc {
        Svc::Avt => "AVTransport",
        Svc::Rc => "RenderingControl",
        Svc::Cm => "ConnectionManager",
    }
}

fn envelope(inner: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>{inner}</s:Body></s:Envelope>"
    )
}

fn fault(code: u32, what: &str) -> String {
    envelope(&format!(
        "<s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail>\
         <UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{code}</errorCode>\
         <errorDescription>{what}</errorDescription></UPnPError></detail></s:Fault>"
    ))
}

fn hms(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The duration a DIDL-Lite `<res duration="H:MM:SS.mmm">` gives, in ms.
fn didl_duration(didl: &str) -> i64 {
    let Some(at) = didl.find("duration=\"") else {
        return 0;
    };
    let v = &didl[at + 10..];
    let v = &v[..v.find('"').unwrap_or(0)];
    let mut parts = v.split(':').rev();
    let sec: f64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let min: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let hr: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (hr * 3600 + min * 60) * 1000 + (sec * 1000.0) as i64
}

/// Answer one SOAP action: (HTTP status, body).
fn control(svc: Svc, action: &str, body: &str) -> (&'static str, String) {
    let mut s = state().lock().unwrap();
    let urn = format!("urn:schemas-upnp-org:service:{}:1", svc_name(svc));
    let ok = |args: &[(&str, String)]| {
        let mut inner = format!("<u:{action}Response xmlns:u=\"{urn}\">");
        for (k, v) in args {
            inner.push_str(&format!("<{k}>{}</{k}>", escape(v)));
        }
        inner.push_str(&format!("</u:{action}Response>"));
        ("200 OK", envelope(&inner))
    };
    let arg = |name: &str| tag(body, name).unwrap_or_default();
    let elapsed = s.since.map(|t| t.elapsed().as_millis() as i64).unwrap_or(0);
    match (svc, action) {
        (Svc::Avt, "SetAVTransportURI") => {
            let uri = arg("CurrentURI");
            if !(uri.starts_with("http://") || uri.starts_with("https://")) {
                return (
                    "500 Internal Server Error",
                    fault(716, "Only http and https links can be played"),
                );
            }
            let meta = arg("CurrentURIMetaData");
            s.title = tag(&meta, "title").unwrap_or_default();
            s.dur_ms = didl_duration(&meta);
            s.uri = uri;
            s.meta = meta;
            // A new link while one plays replaces it at once, as a speaker would.
            if matches!(s.transport, "PLAYING" | "TRANSITIONING") {
                s.ask = Some(Ask::Play(s.uri.clone()));
                s.awaiting = Some(Instant::now());
                s.since = None;
                s.transport = "TRANSITIONING";
            } else {
                s.transport = "STOPPED";
            }
            notify(&mut s, Svc::Avt);
            ok(&[])
        }
        (Svc::Avt, "Play") => {
            if s.uri.is_empty() {
                return ("500 Internal Server Error", fault(701, "No link to play"));
            }
            if s.transport != "PLAYING" {
                s.ask = Some(Ask::Play(s.uri.clone()));
                s.awaiting = Some(Instant::now());
                s.since = None;
                s.transport = "TRANSITIONING";
                notify(&mut s, Svc::Avt);
            }
            ok(&[])
        }
        (Svc::Avt, "Stop" | "Pause") => {
            if matches!(s.transport, "PLAYING" | "TRANSITIONING") {
                s.ask = Some(Ask::Stop);
            }
            s.awaiting = None;
            s.since = None;
            s.transport = if action == "Pause" && !s.uri.is_empty() {
                "PAUSED_PLAYBACK"
            } else if s.uri.is_empty() {
                "NO_MEDIA_PRESENT"
            } else {
                "STOPPED"
            };
            s.note = cinder_ui::ui_format!("Shows up as {}", s.name);
            notify(&mut s, Svc::Avt);
            ok(&[])
        }
        (Svc::Avt, "Seek" | "Next" | "Previous") => {
            ("500 Internal Server Error", fault(701, "Not offered"))
        }
        (Svc::Avt, "GetTransportInfo") => ok(&[
            ("CurrentTransportState", s.transport.to_string()),
            ("CurrentTransportStatus", "OK".into()),
            ("CurrentSpeed", "1".into()),
        ]),
        (Svc::Avt, "GetPositionInfo") => ok(&[
            ("Track", if s.uri.is_empty() { "0" } else { "1" }.into()),
            ("TrackDuration", hms(s.dur_ms)),
            ("TrackMetaData", s.meta.clone()),
            ("TrackURI", s.uri.clone()),
            ("RelTime", hms(elapsed)),
            ("AbsTime", hms(elapsed)),
            ("RelCount", "2147483647".into()),
            ("AbsCount", "2147483647".into()),
        ]),
        (Svc::Avt, "GetMediaInfo") => ok(&[
            ("NrTracks", if s.uri.is_empty() { "0" } else { "1" }.into()),
            ("MediaDuration", hms(s.dur_ms)),
            ("CurrentURI", s.uri.clone()),
            ("CurrentURIMetaData", s.meta.clone()),
            ("NextURI", String::new()),
            ("NextURIMetaData", String::new()),
            ("PlayMedium", "NETWORK".into()),
            ("RecordMedium", "NOT_IMPLEMENTED".into()),
            ("WriteStatus", "NOT_IMPLEMENTED".into()),
        ]),
        (Svc::Avt, "GetDeviceCapabilities") => ok(&[
            ("PlayMedia", "NETWORK".into()),
            ("RecMedia", "NOT_IMPLEMENTED".into()),
            ("RecQualityModes", "NOT_IMPLEMENTED".into()),
        ]),
        (Svc::Avt, "GetTransportSettings") => ok(&[
            ("PlayMode", "NORMAL".into()),
            ("RecQualityMode", "NOT_IMPLEMENTED".into()),
        ]),
        (Svc::Avt, "GetCurrentTransportActions") => {
            let a = if s.transport == "PLAYING" {
                "Stop,Pause"
            } else if s.uri.is_empty() {
                ""
            } else {
                "Play"
            };
            ok(&[("Actions", a.into())])
        }
        (Svc::Rc, "GetVolume") => ok(&[("CurrentVolume", s.volume.to_string())]),
        (Svc::Rc, "SetVolume") => {
            s.volume = arg("DesiredVolume")
                .parse::<u32>()
                .unwrap_or(s.volume)
                .min(100);
            notify(&mut s, Svc::Rc);
            ok(&[])
        }
        (Svc::Rc, "GetMute") => ok(&[("CurrentMute", (s.mute as u8).to_string())]),
        (Svc::Rc, "SetMute") => {
            s.mute = matches!(arg("DesiredMute").as_str(), "1" | "true");
            notify(&mut s, Svc::Rc);
            ok(&[])
        }
        (Svc::Rc, "ListPresets") => ok(&[("CurrentPresetNameList", "FactoryDefaults".into())]),
        (Svc::Rc, "SelectPreset") => ok(&[]),
        (Svc::Cm, "GetProtocolInfo") => ok(&[("Source", String::new()), ("Sink", SINK.into())]),
        (Svc::Cm, "GetCurrentConnectionIDs") => ok(&[("ConnectionIDs", "0".into())]),
        (Svc::Cm, "GetCurrentConnectionInfo") => ok(&[
            ("RcsID", "0".into()),
            ("AVTransportID", "0".into()),
            ("ProtocolInfo", String::new()),
            ("PeerConnectionManager", String::new()),
            ("PeerConnectionID", "-1".into()),
            ("Direction", "Input".into()),
            ("Status", "OK".into()),
        ]),
        _ => ("500 Internal Server Error", fault(401, "Invalid Action")),
    }
}

// ── Events (GENA) ─────────────────────────────────────────────────────────────────────────────────

/// The event body a subscriber of `svc` gets now.
fn event_body(s: &State, svc: Svc) -> String {
    let prop = |inner: String| {
        format!(
            "<?xml version=\"1.0\"?>\n<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\">{inner}</e:propertyset>"
        )
    };
    match svc {
        Svc::Avt => {
            let ev = format!(
                "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/AVT/\"><InstanceID val=\"0\">\
                 <TransportState val=\"{}\"/><TransportStatus val=\"OK\"/><AVTransportURI val=\"{}\"/>\
                 <CurrentTrackURI val=\"{}\"/><CurrentTrackDuration val=\"{}\"/><CurrentMediaDuration val=\"{}\"/>\
                 <NumberOfTracks val=\"{}\"/><CurrentTrack val=\"{}\"/><CurrentPlayMode val=\"NORMAL\"/>\
                 <TransportPlaySpeed val=\"1\"/><PlaybackStorageMedium val=\"NETWORK\"/></InstanceID></Event>",
                s.transport,
                escape(&s.uri),
                escape(&s.uri),
                hms(s.dur_ms),
                hms(s.dur_ms),
                !s.uri.is_empty() as u8,
                !s.uri.is_empty() as u8,
            );
            prop(format!(
                "<e:property><LastChange>{}</LastChange></e:property>",
                escape(&ev)
            ))
        }
        Svc::Rc => {
            let ev = format!(
                "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/RCS/\"><InstanceID val=\"0\">\
                 <Volume channel=\"Master\" val=\"{}\"/><Mute channel=\"Master\" val=\"{}\"/></InstanceID></Event>",
                s.volume, s.mute as u8
            );
            prop(format!(
                "<e:property><LastChange>{}</LastChange></e:property>",
                escape(&ev)
            ))
        }
        Svc::Cm => prop(format!(
            "<e:property><SourceProtocolInfo></SourceProtocolInfo></e:property>\
             <e:property><SinkProtocolInfo>{SINK}</SinkProtocolInfo></e:property>\
             <e:property><CurrentConnectionIDs>0</CurrentConnectionIDs></e:property>"
        )),
    }
}

/// Send `svc`'s state to its subscribers, from a thread of its own (a slow one must not hold the
/// server). Lapsed subscriptions are dropped.
fn notify(s: &mut State, svc: Svc) {
    s.subs.retain(|x| x.until > Instant::now());
    let body = event_body(s, svc);
    let mut jobs = Vec::new();
    for x in s.subs.iter_mut().filter(|x| x.svc == svc) {
        jobs.push((x.url.clone(), x.sid.clone(), x.seq));
        x.seq = x.seq.wrapping_add(1);
    }
    if jobs.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("cinder-dlna-event".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            for (url, sid, seq) in jobs {
                send_event(&url, &sid, seq, &body);
            }
        });
}

fn send_event(url: &str, sid: &str, seq: u32, body: &str) {
    let Some(rest) = url.strip_prefix("http://") else {
        return;
    };
    let (host, path) = rest
        .split_once('/')
        .map(|(h, p)| (h, format!("/{p}")))
        .unwrap_or((rest, "/".into()));
    let Ok(addr) = host
        .parse::<SocketAddr>()
        .or_else(|_| format!("{host}:80").parse())
    else {
        return;
    };
    let Ok(mut c) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
        return;
    };
    let _ = c.set_write_timeout(Some(Duration::from_secs(3)));
    let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
    let req = format!(
        "NOTIFY {path} HTTP/1.1\r\nHOST: {host}\r\nCONTENT-TYPE: text/xml; charset=\"utf-8\"\r\nNT: upnp:event\r\n\
         NTS: upnp:propchange\r\nSID: {sid}\r\nSEQ: {seq}\r\nCONTENT-LENGTH: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = c.write_all(req.as_bytes());
    let _ = c.read(&mut [0u8; 256]);
}

/// The first URL of a CALLBACK header (`<http://…><http://…>`).
fn callback(h: &str) -> Option<String> {
    let a = h.find('<')? + 1;
    let b = a + h[a..].find('>')?;
    Some(h[a..b].to_string()).filter(|u| u.starts_with("http://"))
}

fn timeout_secs(h: Option<&str>) -> u64 {
    h.and_then(|v| v.trim().strip_prefix("Second-"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800)
        .clamp(60, 1800)
}

// ── HTTP ──────────────────────────────────────────────────────────────────────────────────────────

fn reply(c: &mut TcpStream, status: &str, ctype: &str, extra: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nServer: Linux/3.10 UPnP/1.0 Cinder/1\r\n\
         Connection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = c.write_all(head.as_bytes());
    let _ = c.write_all(body.as_bytes());
}

fn which(path: &str) -> Option<Svc> {
    let p = path.split('?').next().unwrap_or("");
    let last = p.rsplit('/').next().unwrap_or("").trim_end_matches(".xml");
    match last {
        "avt" => Some(Svc::Avt),
        "rc" => Some(Svc::Rc),
        "cm" => Some(Svc::Cm),
        _ => None,
    }
}

fn serve(c: TcpStream) {
    let mut r = BufReader::new(match c.try_clone() {
        Ok(x) => x,
        Err(_) => return,
    });
    let Some(req) = read_head(&mut r) else {
        return;
    };
    let mut c = c;
    let (status, ctype, extra, body) = respond(&req, &mut r);
    reply(&mut c, status, ctype, &extra, &body);
    // The first event goes after the SUBSCRIBE's answer, as GENA asks.
    if req.method == "SUBSCRIBE" && status.starts_with("200") && req.header("SID").is_none() {
        if let Some(svc) = which(&req.path) {
            let mut s = state().lock().unwrap();
            let sid = extra
                .lines()
                .find_map(|l| l.strip_prefix("SID: "))
                .unwrap_or("")
                .to_string();
            let body = event_body(&s, svc);
            if let Some(x) = s.subs.iter_mut().find(|x| x.sid == sid) {
                let (url, seq) = (x.url.clone(), x.seq);
                x.seq += 1;
                drop(s);
                send_event(&url, &sid, seq, &body);
            }
        }
    }
}

/// (status, content type, extra headers, body) for one request; `r` has the body.
fn respond(req: &Request, r: &mut impl BufRead) -> (&'static str, &'static str, String, String) {
    const XML: &str = "text/xml; charset=\"utf-8\"";
    let path = req.path.split('?').next().unwrap_or("");
    match (req.method.as_str(), path) {
        ("GET" | "HEAD", "/desc.xml") => {
            let s = state().lock().unwrap();
            ("200 OK", XML, String::new(), description(&s.name, &s.uuid))
        }
        ("GET" | "HEAD", p) if p.ends_with(".xml") && which(p).is_some() => {
            ("200 OK", XML, String::new(), scpd(which(p).unwrap()))
        }
        ("POST", p) if p.starts_with("/ctl/") && which(p).is_some() => {
            let len = req
                .header("content-length")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0)
                .min(256 * 1024);
            let mut body = String::new();
            let _ = r.take(len).read_to_string(&mut body);
            let action = req
                .header("soapaction")
                .and_then(|a| a.trim().trim_matches('"').rsplit('#').next())
                .unwrap_or("")
                .to_string();
            let (st, b) = control(which(p).unwrap(), &action, &body);
            (st, XML, "EXT:\r\n".into(), b)
        }
        ("SUBSCRIBE", p) if p.starts_with("/evt/") && which(p).is_some() => {
            let secs = timeout_secs(req.header("timeout"));
            let mut s = state().lock().unwrap();
            let until = Instant::now() + Duration::from_secs(secs);
            if let Some(sid) = req.header("sid") {
                // A renewal.
                match s.subs.iter_mut().find(|x| x.sid == sid) {
                    Some(x) => x.until = until,
                    None => return ("412 Precondition Failed", XML, String::new(), String::new()),
                }
                return (
                    "200 OK",
                    XML,
                    format!("SID: {sid}\r\nTIMEOUT: Second-{secs}\r\n"),
                    String::new(),
                );
            }
            let Some(url) = req.header("callback").and_then(callback) else {
                return ("412 Precondition Failed", XML, String::new(), String::new());
            };
            let sid = format!(
                "uuid:{}",
                crate::remote::new_code() + &crate::remote::new_code()
            );
            s.subs.retain(|x| x.until > Instant::now());
            if s.subs.len() >= 16 {
                s.subs.remove(0);
            }
            s.subs.push(Sub {
                sid: sid.clone(),
                url,
                svc: which(p).unwrap(),
                seq: 0,
                until,
            });
            (
                "200 OK",
                XML,
                format!("SID: {sid}\r\nTIMEOUT: Second-{secs}\r\n"),
                String::new(),
            )
        }
        ("UNSUBSCRIBE", _) => {
            let sid = req.header("sid").unwrap_or("").to_string();
            state().lock().unwrap().subs.retain(|x| x.sid != sid);
            ("200 OK", XML, String::new(), String::new())
        }
        _ => ("404 Not Found", XML, String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::unescape;

    /// One request to the running server: (status line, headers and body).
    fn ask(port: u16, req: &str) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(req.as_bytes()).unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        out
    }

    fn soap_req(port: u16, svc: &str, action: &str, args: &str) -> String {
        let urn = match svc {
            "avt" => "AVTransport",
            "rc" => "RenderingControl",
            _ => "ConnectionManager",
        };
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
             <u:{action} xmlns:u=\"urn:schemas-upnp-org:service:{urn}:1\"><InstanceID>0</InstanceID>{args}</u:{action}></s:Body></s:Envelope>"
        );
        ask(
            port,
            &format!(
                "POST /ctl/{svc} HTTP/1.1\r\nHost: x\r\nContent-Type: text/xml\r\n\
                 SOAPACTION: \"urn:schemas-upnp-org:service:{urn}:1#{action}\"\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    #[test]
    fn the_id_comes_from_the_mac_and_searches_are_answered() {
        assert_eq!(
            uuid_from_mac("00:0A:F5:12:34:56\n"),
            "43696e64-6572-4000-8000-000af5123456"
        );
        let base = "http://192.168.1.20:49494";
        let all = search_answers(
            "M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\nMX: 2\r\n\r\n",
            "u",
            base,
        );
        assert_eq!(all.len(), 6);
        let mr = search_answers(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nst: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n",
            "u",
            base,
        );
        assert_eq!(mr.len(), 1);
        assert!(mr[0].contains("LOCATION: http://192.168.1.20:49494/desc.xml\r\n"));
        assert!(mr[0].contains("USN: uuid:u::urn:schemas-upnp-org:device:MediaRenderer:1\r\n"));
        assert!(search_answers(
            "M-SEARCH * HTTP/1.1\r\nST: urn:schemas-upnp-org:device:MediaServer:1\r\n\r\n",
            "u",
            base
        )
        .is_empty());
        assert!(search_answers(
            "NOTIFY * HTTP/1.1\r\nNT: upnp:rootdevice\r\n\r\n",
            "u",
            base
        )
        .is_empty());
    }

    #[test]
    fn the_description_and_scpds_read_back_as_a_controller_reads_them() {
        // cast.rs is Cinder's own control point: it must find this renderer's services.
        let d = description("Walkman (Cinder)", "u");
        let sp = crate::cast::parse_description(&d, "http://10.0.0.5:49494/desc.xml").unwrap();
        assert_eq!(sp.name, "Walkman (Cinder)");
        assert_eq!(sp.av, "http://10.0.0.5:49494/ctl/avt");
        assert_eq!(sp.rc.as_deref(), Some("http://10.0.0.5:49494/ctl/rc"));
        let avt = scpd(Svc::Avt);
        for a in [
            "SetAVTransportURI",
            "Play",
            "Stop",
            "GetPositionInfo",
            "GetTransportInfo",
        ] {
            assert!(avt.contains(&format!("<name>{a}</name>")), "{a}");
        }
        assert!(avt.contains("<stateVariable sendEvents=\"yes\"><name>LastChange</name>"));
        assert_eq!(
            avt.matches("<name>AVTransportURI</name>").count(),
            1,
            "each variable once"
        );
        assert!(scpd(Svc::Rc).contains("<name>Volume</name><dataType>ui2</dataType>"));
        assert!(scpd(Svc::Cm).contains("<name>SinkProtocolInfo</name>"));
    }

    #[test]
    fn a_controller_plays_the_radio_answers_and_events_follow() {
        let _g = crate::cast::tests::E2E
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // A subscriber, to see the events.
        let ev = TcpListener::bind("127.0.0.1:0").unwrap();
        let ev_port = ev.local_addr().unwrap().port();
        let port = start(Ipv4Addr::LOCALHOST, "Walkman (Cinder)", "u-test").unwrap();
        let sub = ask(
            port,
            &format!("SUBSCRIBE /evt/avt HTTP/1.1\r\nHost: x\r\nCALLBACK: <http://127.0.0.1:{ev_port}/e>\r\nNT: upnp:event\r\nTIMEOUT: Second-300\r\n\r\n"),
        );
        assert!(sub.starts_with("HTTP/1.1 200"), "{sub}");
        assert!(sub.contains("TIMEOUT: Second-300"));
        let event = |l: &TcpListener| {
            let (mut c, _) = l.accept().unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut b = vec![0u8; 8192];
            let mut got = String::new();
            while !got.contains("</e:propertyset>") {
                let n = c.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                got.push_str(&String::from_utf8_lossy(&b[..n]));
            }
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            unescape(&got)
        };
        let first = event(&ev);
        assert!(
            first.starts_with("NOTIFY /e HTTP/1.1") && first.contains("SEQ: 0"),
            "{first}"
        );
        assert!(first.contains("<TransportState val=\"NO_MEDIA_PRESENT\"/>"));

        let bad = soap_req(
            port,
            "avt",
            "SetAVTransportURI",
            "<CurrentURI>file:///data/x</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>",
        );
        assert!(
            bad.contains("<errorCode>716</errorCode>"),
            "a file link is refused"
        );
        let meta = escape(
            "<DIDL-Lite><item><dc:title>So What</dc:title><res protocolInfo=\"http-get:*:audio/mpeg:*\" duration=\"0:09:22.000\">x</res></item></DIDL-Lite>",
        );
        let set = soap_req(port, "avt", "SetAVTransportURI", &format!("<CurrentURI>http://10.0.0.9/a.mp3</CurrentURI><CurrentURIMetaData>{meta}</CurrentURIMetaData>"));
        assert!(set.contains("<u:SetAVTransportURIResponse"), "{set}");
        assert!(event(&ev).contains("<TransportState val=\"STOPPED\"/>"));
        assert_eq!(take(), None, "a link alone plays nothing");
        soap_req(port, "avt", "Play", "<Speed>1</Speed>");
        assert_eq!(take(), Some(Ask::Play("http://10.0.0.9/a.mp3".into())));
        assert!(event(&ev).contains("TRANSITIONING"));

        // The last station's ending, while ours is on its way: not ours to report.
        radio_status("state=off\nstation=3\ntitle=\nnote=\n");
        radio_status("state=connecting\nstation=-2\ntitle=\nnote=\n");
        radio_status("state=playing\nstation=-2\ntitle=\nnote=\n");
        assert!(event(&ev).contains("<TransportState val=\"PLAYING\"/>"));
        assert_eq!(note(), "Playing So What");
        let info = soap_req(port, "avt", "GetPositionInfo", "");
        assert!(
            info.contains("<TrackDuration>0:09:22</TrackDuration>"),
            "{info}"
        );
        assert!(soap_req(port, "avt", "GetTransportInfo", "")
            .contains("<CurrentTransportState>PLAYING</CurrentTransportState>"));
        // The song ends: STOPPED, which moves the controller on.
        radio_status("state=off\nstation=-2\ntitle=\nnote=\n");
        assert!(event(&ev).contains("<TransportState val=\"STOPPED\"/>"));

        // A format the radio cannot play says why.
        soap_req(port, "avt", "Play", "");
        assert!(take().is_some());
        let _ = event(&ev);
        radio_status(
            "state=off\nstation=-2\ntitle=\nnote=This station's format is not supported\n",
        );
        assert!(event(&ev).contains("STOPPED"));
        assert_eq!(note(), "This station's format is not supported");

        assert!(soap_req(
            port,
            "rc",
            "SetVolume",
            "<Channel>Master</Channel><DesiredVolume>140</DesiredVolume>"
        )
        .contains("SetVolumeResponse"));
        assert!(
            soap_req(port, "rc", "GetVolume", "<Channel>Master</Channel>")
                .contains("<CurrentVolume>100</CurrentVolume>")
        );
        assert!(soap_req(port, "cm", "GetProtocolInfo", "").contains("audio/mpeg"));
        assert!(soap_req(
            port,
            "avt",
            "Seek",
            "<Unit>REL_TIME</Unit><Target>0:01:00</Target>"
        )
        .contains("<errorCode>701</errorCode>"));
        assert!(soap_req(port, "avt", "Bogus", "").contains("<errorCode>401</errorCode>"));
        assert!(ask(port, "GET /desc.xml HTTP/1.1\r\nHost: x\r\n\r\n")
            .contains("<friendlyName>Walkman (Cinder)</friendlyName>"));
        stop();
        assert!(!running());
    }
}
