//! Settings ▸ Wi-Fi ▸ Speakers: the queue played on a speaker on the home network — any UPnP AV
//! (DLNA) media renderer: network speakers (Sonos among them), most network AV receivers and smart
//! TVs, Kodi, BubbleUPnP and the like.
//!
//! The player stays in charge. It serves each song's own file over HTTP, untouched, and tells the
//! speaker to fetch it (AVTransport `SetAVTransportURI` + `Play`); then it watches the speaker's
//! transport state and sends the next song when one ends. Play/pause, skip and the volume keys go
//! to the speaker while a cast is on. Nothing here runs until the Speakers page asks: discovery is
//! one SSDP search (UDP multicast to 239.255.255.250:1900), control is SOAP over plain HTTP to the
//! speaker, and the file server listens only during a cast, on the player's Wi-Fi address and a
//! random port, for paths that carry a random token.

use std::collections::HashSet;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    pub name: String,
    pub model: String,
    /// The AVTransport and RenderingControl control URLs.
    pub av: String,
    pub rc: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub object_id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub dur_ms: i64,
}

// ── XML, just enough for device descriptions and SOAP answers ────────────────────────────────────

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// The raw contents of every element whose local name is `name` (any namespace prefix), in order.
/// Elements of that name must not nest (true of everything read here: `service`, `controlURL`…).
pub fn elements<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = xml[at..].find('<') {
        let start = at + i + 1;
        at = start;
        let Some(end) = xml[start..].find('>') else {
            break;
        };
        let open = &xml[start..start + end];
        if open.starts_with(['/', '?', '!']) {
            continue;
        }
        let qname = open
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        if qname.rsplit(':').next() != Some(name) {
            continue;
        }
        let body = start + end + 1;
        if open.ends_with('/') {
            out.push("");
            continue;
        }
        let close = format!("</{qname}>");
        if let Some(j) = xml[body..].find(&close) {
            out.push(&xml[body..body + j]);
            at = body + j + close.len();
        }
    }
    out
}

/// The first `name` element's text, unescaped and trimmed.
pub fn tag(xml: &str, name: &str) -> Option<String> {
    elements(xml, name)
        .first()
        .map(|s| unescape(s.trim()))
        .filter(|s| !s.is_empty())
}

/// `rel` from the description at `base`: a full URL, a host-absolute path, or relative.
pub fn join(base: &str, rel: &str) -> String {
    if rel.starts_with("http://") || rel.starts_with("https://") {
        return rel.to_string();
    }
    let origin_end = base
        .find("://")
        .map(|s| base[s + 3..].find('/').map_or(base.len(), |p| s + 3 + p))
        .unwrap_or(base.len());
    if rel.starts_with('/') {
        return format!("{}{rel}", &base[..origin_end]);
    }
    let dir_end = base
        .rfind('/')
        .filter(|&i| i >= origin_end)
        .unwrap_or(origin_end);
    format!("{}/{rel}", &base[..dir_end])
}

/// A media renderer's description (fetched from its SSDP LOCATION). None when it has no
/// AVTransport. Sonos puts the renderer in an embedded device and names the room in `roomName`.
pub fn parse_description(xml: &str, location: &str) -> Option<Speaker> {
    let base = tag(xml, "URLBase").unwrap_or_else(|| location.to_string());
    let control = |kind: &str| {
        elements(xml, "service").into_iter().find_map(|s| {
            let ty = tag(s, "serviceType")?;
            ty.contains(&format!(":service:{kind}:"))
                .then(|| tag(s, "controlURL"))
                .flatten()
                .map(|u| join(&base, &u))
        })
    };
    let av = control("AVTransport")?;
    let name = tag(xml, "roomName")
        .or_else(|| tag(xml, "friendlyName"))
        .unwrap_or_else(|| "Speaker".into());
    Some(Speaker {
        name: name.chars().take(60).collect(),
        model: tag(xml, "modelName")
            .unwrap_or_default()
            .chars()
            .take(60)
            .collect(),
        av,
        rc: control("RenderingControl"),
    })
}

// ── HTTP client (plain HTTP on the home network) ─────────────────────────────────────────────────

fn dechunk(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let Some(nl) = b[at..].windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let size = std::str::from_utf8(&b[at..at + nl])
            .ok()
            .and_then(|s| usize::from_str_radix(s.split(';').next()?.trim(), 16).ok())
            .unwrap_or(0);
        at += nl + 2;
        if size == 0 || at + size > b.len() {
            break;
        }
        out.extend_from_slice(&b[at..at + size]);
        at += size + 2;
    }
    out
}

/// One request; the status and the body (at most 1 MB).
/// `host` or `host:port` (port 80 when none is given), resolved.
pub(crate) fn resolve(host: &str) -> Result<std::net::SocketAddr, String> {
    let target = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    target
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "no address".into())
}

pub fn http(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(u32, String), String> {
    let rest = url.strip_prefix("http://").ok_or("not an http address")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = resolve(host)?;
    let mut s =
        TcpStream::connect_timeout(&addr, Duration::from_secs(4)).map_err(|e| e.to_string())?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(8)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(8)));
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Cinder/{} UPnP/1.0\r\nConnection: close\r\nContent-Length: {}\r\n",
        cinder_ui::cinder_version!(),
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.take(1 << 20)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("no answer")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let code: u32 = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("not HTTP")?;
    let mut payload = raw[split + 4..].to_vec();
    if head.lines().any(|l| {
        l.to_ascii_lowercase().starts_with("transfer-encoding:")
            && l.to_ascii_lowercase().contains("chunked")
    }) {
        payload = dechunk(&payload);
    }
    Ok((code, String::from_utf8_lossy(&payload).into_owned()))
}

/// A UPnP action. Err carries the speaker's own error (`errorCode`/`errorDescription`).
pub fn soap(
    url: &str,
    service: &str,
    action: &str,
    args: &[(&str, &str)],
) -> Result<String, String> {
    let mut a = String::new();
    for (k, v) in args {
        a.push_str(&format!("<{k}>{}</{k}>", escape(v)));
    }
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} \
         xmlns:u=\"urn:schemas-upnp-org:service:{service}:1\">{a}</u:{action}></s:Body></s:Envelope>"
    );
    let sa = format!("\"urn:schemas-upnp-org:service:{service}:1#{action}\"");
    let (code, resp) = http(
        "POST",
        url,
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\""),
            ("SOAPACTION", &sa),
        ],
        &body,
    )?;
    if code == 200 {
        return Ok(resp);
    }
    Err(
        match (tag(&resp, "errorCode"), tag(&resp, "errorDescription")) {
            (Some(c), Some(d)) => format!("{action}: {c} {d}"),
            (Some(c), None) => format!("{action}: error {c}"),
            _ => format!("{action}: HTTP {code}"),
        },
    )
}

// ── Discovery ─────────────────────────────────────────────────────────────────────────────────────

const SEARCH: &str =
    "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\n\
                      ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";

/// The LOCATION of an SSDP answer (http only).
pub fn location(answer: &str) -> Option<String> {
    answer.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        let v = v.trim();
        (k.trim().eq_ignore_ascii_case("location") && v.starts_with("http://"))
            .then(|| v.to_string())
    })
}

/// Every speaker that answers within `wait`, by name: UPnP/DLNA renderers (SSDP) and Google Cast
/// devices (mDNS, `chromecast.rs`), searched for side by side.
pub fn discover(wait: Duration) -> Result<Vec<Speaker>, String> {
    let cast = std::thread::Builder::new()
        .stack_size(96 * 1024)
        .spawn(move || crate::chromecast::discover(wait))
        .ok();
    let mut v = discover_upnp(wait)?;
    if let Some(found) = cast.and_then(|t| t.join().ok()) {
        v.extend(found);
    }
    v.sort_by_key(|a| a.name.to_lowercase());
    Ok(v)
}

/// The UPnP media renderers that answer within `wait`. Their descriptions are fetched in
/// parallel; one that does not answer in time is left out.
fn discover_upnp(wait: Duration) -> Result<Vec<Speaker>, String> {
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let _ = sock.set_multicast_ttl_v4(2);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let to = ("239.255.255.250", 1900);
    sock.send_to(SEARCH.as_bytes(), to)
        .map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let mut resent = false;
    let mut seen = HashSet::new();
    let mut buf = [0u8; 2048];
    while t0.elapsed() < wait {
        if !resent && t0.elapsed() > Duration::from_millis(800) {
            let _ = sock.send_to(SEARCH.as_bytes(), to); // UDP: once more, in case the first was lost
            resent = true;
        }
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if let Some(l) = location(&String::from_utf8_lossy(&buf[..n])) {
                seen.insert(l);
            }
        }
    }
    let found = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = seen
        .into_iter()
        .take(32)
        .map(|loc| {
            let found = found.clone();
            std::thread::Builder::new()
                .stack_size(128 * 1024)
                .spawn(move || {
                    if let Ok((200, xml)) = http("GET", &loc, &[], "") {
                        if let Some(sp) = parse_description(&xml, &loc) {
                            found.lock().unwrap().push(sp);
                        }
                    }
                })
        })
        .collect();
    for t in threads.into_iter().flatten() {
        let _ = t.join();
    }
    let mut v = std::mem::take(&mut *found.lock().unwrap());
    v.sort_by_key(|a| a.name.to_lowercase());
    v.dedup_by(|a, b| a.av == b.av);
    Ok(v)
}

// ── Serving the songs ─────────────────────────────────────────────────────────────────────────────

/// The type a speaker is told, by extension. None: not sent (DSD, which renderers do not play, and
/// anything unknown).
pub fn mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" | "alac" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wma" => "audio/x-ms-wma",
        _ => return None,
    })
}

/// A Range header against a file of `len` bytes: the first and last byte to send. None = the whole
/// file; Err = not satisfiable.
pub fn range(h: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = h.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return Ok(None);
    };
    let spec = spec.split(',').next().unwrap_or("").trim();
    let (a, b) = spec.split_once('-').ok_or(())?;
    let r = if a.is_empty() {
        let n: u64 = b.parse().map_err(|_| ())?;
        if n == 0 {
            return Err(());
        }
        (len.saturating_sub(n), len.saturating_sub(1))
    } else {
        let a: u64 = a.parse().map_err(|_| ())?;
        let b = if b.is_empty() {
            len.saturating_sub(1)
        } else {
            b.parse::<u64>().map_err(|_| ())?.min(len.saturating_sub(1))
        };
        (a, b)
    };
    if len == 0 || r.0 > r.1 || r.0 >= len {
        return Err(());
    }
    Ok(Some(r))
}

fn serve_file(mut c: TcpStream, token: &str, files: &[(String, &'static str)]) {
    let Ok(read_half) = c.try_clone() else {
        return;
    };
    let mut r = BufReader::new(read_half);
    let Some(req) = crate::remote::read_head(&mut r) else {
        return;
    };
    let head_only = req.method == "HEAD";
    let file = req
        .path
        .strip_prefix("/cast/")
        .and_then(|p| p.strip_prefix(token))
        .and_then(|p| p.strip_prefix('/'))
        .and_then(|p| p.split('.').next())
        .and_then(|i| i.parse::<usize>().ok())
        .and_then(|i| files.get(i));
    let bad = |c: &mut TcpStream, status: &str| {
        let _ = c.write_all(
            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
    };
    let (Some((path, ctype)), true) = (file, req.method == "GET" || head_only) else {
        return bad(&mut c, "404 Not Found");
    };
    let Ok(mut f) = std::fs::File::open(path) else {
        return bad(&mut c, "404 Not Found");
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let (status, from, to) = match range(req.header("range"), len) {
        Ok(None) => ("200 OK", 0, len.saturating_sub(1)),
        Ok(Some((a, b))) => ("206 Partial Content", a, b),
        Err(()) => {
            let _ = c.write_all(
                format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
            return;
        }
    };
    let count = if len == 0 { 0 } else { to - from + 1 };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {count}\r\nAccept-Ranges: bytes\r\n\
         transferMode.dlna.org: Streaming\r\ncontentFeatures.dlna.org: DLNA.ORG_OP=01;DLNA.ORG_FLAGS=01700000000000000000000000000000\r\n\
         Connection: close\r\n"
    );
    if status.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {from}-{to}/{len}\r\n"));
    }
    head.push_str("\r\n");
    if c.write_all(head.as_bytes()).is_err() || head_only || f.seek(SeekFrom::Start(from)).is_err()
    {
        return;
    }
    let mut left = count;
    let mut buf = vec![0u8; 64 * 1024];
    while left > 0 {
        let want = (left as usize).min(buf.len());
        let Ok(n) = f.read(&mut buf[..want]) else {
            return;
        };
        if n == 0 || c.write_all(&buf[..n]).is_err() {
            return;
        }
        left -= n as u64;
    }
}

fn random_token() -> String {
    let mut b = [0u8; 12];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    if b == [0; 12] {
        // No /dev/urandom (a host test): the clock is unguessable enough off the player.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(1);
        b[..12].copy_from_slice(&n.to_le_bytes()[..12]);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The player's own address on the way to `url`'s host (no packet is sent: a UDP connect only
/// picks the route).
pub(crate) fn own_ip_towards(url: &str) -> Result<IpAddr, String> {
    let host = url
        .split_once("://")
        .and_then(|(_, r)| r.split('/').next())
        .ok_or("bad address")?;
    let addr = resolve(host)?;
    let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    s.connect(addr).map_err(|e| e.to_string())?;
    Ok(s.local_addr().map_err(|e| e.to_string())?.ip())
}

// ── The cast ──────────────────────────────────────────────────────────────────────────────────────

pub enum Cmd {
    Toggle,
    Next,
    Prev,
    Vol(i32),
    Stop,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// A cast is running.
    pub on: bool,
    pub speaker: String,
    /// The song playing, as an index into the songs `start` was given.
    pub index: usize,
    pub playing: bool,
    pub pos_ms: i64,
    pub dur_ms: i64,
    pub volume: Option<u32>,
    /// Why the last cast ended or what went wrong, for the page.
    pub note: String,
    /// Which cast this is (each `start` is a new one), and its speaker's control URL: a session
    /// that has been replaced must not write over its successor or stop its music.
    pub gen: u64,
    pub av: String,
}

static CMD: Mutex<Option<mpsc::Sender<Cmd>>> = Mutex::new(None);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static GEN: AtomicU64 = AtomicU64::new(0);

pub fn status() -> Status {
    STATUS.lock().unwrap().clone().unwrap_or_default()
}

/// Change the status, if cast `gen` is still the current one.
pub(crate) fn set_status(gen: u64, f: impl FnOnce(&mut Status)) {
    if let Some(s) = STATUS.lock().unwrap().as_mut().filter(|s| s.gen == gen) {
        f(s);
    }
}

/// Cast `gen` has been replaced by one on the same speaker (which needs no Stop from it).
fn replaced_on(gen: u64, av: &str) -> bool {
    STATUS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.gen != gen && s.on && s.av == av)
}

pub fn active() -> bool {
    status().on
}

/// Hand a command to the running cast. False when there is none.
pub fn send(cmd: Cmd) -> bool {
    CMD.lock()
        .unwrap()
        .as_ref()
        .is_some_and(|tx| tx.send(cmd).is_ok())
}

/// "0:03:21", "00:03:21.500" -> ms. None for "NOT_IMPLEMENTED" and the like.
pub fn hms_ms(s: &str) -> Option<i64> {
    let mut parts = s.trim().split(':');
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    parts
        .next()
        .is_none()
        .then_some(h * 3_600_000 + m * 60_000 + (sec * 1000.0) as i64)
}

pub fn ms_hms(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The DIDL-Lite a speaker shows for the song (title, artist, album) and plays from (`res`).
pub fn didl(s: &Song, url: &str, mime: &str) -> String {
    let dur = if s.dur_ms > 0 {
        format!(" duration=\"{}.000\"", ms_hms(s.dur_ms))
    } else {
        String::new()
    };
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"cinder{}\" parentID=\"0\" restricted=\"1\">\
         <dc:title>{}</dc:title><dc:creator>{}</dc:creator><upnp:artist>{}</upnp:artist><upnp:album>{}</upnp:album>\
         <upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo=\"http-get:*:{mime}:*\"{dur}>{}</res>\
         </item></DIDL-Lite>",
        s.object_id,
        escape(&s.title),
        escape(&s.artist),
        escape(&s.artist),
        escape(&s.album),
        escape(url)
    )
}

/// Start playing `songs` from `start` (at `start_ms` into it) on `sp`, replacing any cast running.
pub fn start(sp: Speaker, songs: Vec<Song>, start: usize, start_ms: i64) -> Result<(), String> {
    if songs.is_empty() {
        return Err("Nothing to play".into());
    }
    stop();
    let ip = own_ip_towards(&sp.av)?;
    let listener = TcpListener::bind((ip, 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = random_token();
    let files: Arc<Vec<(String, &'static str)>> = Arc::new(
        songs
            .iter()
            .map(|s| {
                (
                    s.path.clone(),
                    mime(&s.path).unwrap_or("application/octet-stream"),
                )
            })
            .collect(),
    );
    let run = Arc::new(AtomicBool::new(true));
    {
        let (run, token, files) = (run.clone(), token.clone(), files.clone());
        let _ = listener.set_nonblocking(true);
        let busy = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new()
            .name("cinder-cast-http".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                while run.load(Ordering::SeqCst) {
                    let Ok((c, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    };
                    // A speaker opens a few connections at once (probe, range reads); more than
                    // eight at a time is not a speaker.
                    if busy.load(Ordering::SeqCst) >= 8 {
                        continue;
                    }
                    let _ = c.set_nonblocking(false);
                    let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
                    let _ = c.set_write_timeout(Some(Duration::from_secs(60)));
                    let (token, files, done) = (token.clone(), files.clone(), busy.clone());
                    busy.fetch_add(1, Ordering::SeqCst);
                    let ok = std::thread::Builder::new()
                        .stack_size(160 * 1024)
                        .spawn(move || {
                            serve_file(c, &token, &files);
                            done.fetch_sub(1, Ordering::SeqCst);
                        });
                    if ok.is_err() {
                        busy.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    let base = format!("http://{ip}:{port}/cast/{token}");
    let (tx, rx) = mpsc::channel();
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *CMD.lock().unwrap() = Some(tx);
    *STATUS.lock().unwrap() = Some(Status {
        on: true,
        speaker: sp.name.clone(),
        index: start.min(songs.len() - 1),
        gen,
        av: sp.av.clone(),
        ..Status::default()
    });
    let spawned = std::thread::Builder::new()
        .name("cinder-cast".into())
        .stack_size(192 * 1024)
        .spawn(move || {
            let first = start.min(songs.len() - 1);
            let note = if sp.av.starts_with(crate::chromecast::SCHEME) {
                crate::chromecast::session(&sp, &songs, first, start_ms, &base, &rx, gen)
            } else {
                session(&sp, &songs, first, start_ms, &base, &rx, gen)
            };
            run.store(false, Ordering::SeqCst);
            eprintln!(
                "cinder-ffi: cast: ended on {}{}",
                sp.name,
                if note.is_empty() {
                    String::new()
                } else {
                    format!(" — {note}")
                }
            );
            set_status(gen, |s| {
                s.on = false;
                s.playing = false;
                s.note = note;
            });
        });
    if let Err(e) = spawned {
        set_status(gen, |s| s.on = false);
        return Err(e.to_string());
    }
    Ok(())
}

/// End the cast (the speaker is told to stop). Waits at most a second for it to go.
pub fn stop() {
    let tx = CMD.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(Cmd::Stop);
        for _ in 0..20 {
            if !active() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The cast itself; returns why it ended ("" = asked to stop).
fn session(
    sp: &Speaker,
    songs: &[Song],
    first: usize,
    start_ms: i64,
    base: &str,
    rx: &mpsc::Receiver<Cmd>,
    gen: u64,
) -> String {
    const AV: &str = "AVTransport";
    // `base` is http://<player>:<port>/cast/<token>; the token tells our songs from anyone else's.
    let token = base.rsplit('/').next().unwrap_or(base);
    let av = |action: &str, args: &[(&str, &str)]| {
        let mut a = vec![("InstanceID", "0")];
        a.extend_from_slice(args);
        soap(&sp.av, AV, action, &a)
    };
    let mut volume: Option<u32> = sp.rc.as_ref().and_then(|rc| {
        soap(
            rc,
            "RenderingControl",
            "GetVolume",
            &[("InstanceID", "0"), ("Channel", "Master")],
        )
        .ok()
        .and_then(|r| tag(&r, "CurrentVolume")?.parse().ok())
    });
    set_status(gen, |s| s.volume = volume);
    // Load song `i` (the next playable one from it, going `dir`), and play it.
    let load = |from: usize, dir: i64| -> Result<usize, String> {
        let mut i = from as i64;
        let mut last_err = String::from("Nothing here this speaker can play");
        let mut tries = 0;
        while i >= 0 && (i as usize) < songs.len() && tries < 4 {
            let s = &songs[i as usize];
            if let Some(m) = mime(&s.path) {
                let ext = s.path.rsplit_once('.').map_or("bin", |(_, e)| e);
                let url = format!("{base}/{i}.{}", ext.to_ascii_lowercase());
                let r = av(
                    "SetAVTransportURI",
                    &[
                        ("CurrentURI", &url),
                        ("CurrentURIMetaData", &didl(s, &url, m)),
                    ],
                )
                .and_then(|_| av("Play", &[("Speed", "1")]));
                match r {
                    Ok(_) => return Ok(i as usize),
                    Err(e) => {
                        eprintln!("cinder-ffi: cast: {} refused {}: {e}", sp.name, s.path);
                        last_err =
                            format!("{} would not play {}", sp.name, ext.to_ascii_uppercase());
                        tries += 1;
                    }
                }
            }
            i += dir;
        }
        Err(last_err)
    };
    let mut index = match load(first, 1) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut seek_to = (start_ms > 5000).then_some(start_ms);
    let mut playing_seen = false; // this song has been seen PLAYING
    let mut paused = false; // we paused it
    let mut last_pos = 0i64;
    let mut dur = songs[index].dur_ms;
    let mut fails = 0;
    let mut loaded_at = Instant::now();
    let mut last_poll = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    set_status(gen, |s| {
        s.index = index;
        s.playing = true;
        s.pos_ms = 0;
        s.dur_ms = dur;
    });
    eprintln!(
        "cinder-ffi: cast: playing {} of {} on {} ({})",
        index + 1,
        songs.len(),
        sp.name,
        sp.model
    );
    loop {
        let mut go_to: Option<(usize, i64)> = None;
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Cmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !replaced_on(gen, &sp.av) {
                    let _ = av("Stop", &[]);
                }
                return String::new();
            }
            Ok(Cmd::Toggle) => {
                let r = if paused {
                    av("Play", &[("Speed", "1")])
                } else {
                    av("Pause", &[])
                };
                if r.is_ok() {
                    paused = !paused;
                    set_status(gen, |s| s.playing = !paused);
                }
            }
            Ok(Cmd::Next) => go_to = Some((index + 1, 1)),
            Ok(Cmd::Prev) => {
                if last_pos > 3000 || index == 0 {
                    let _ = av("Seek", &[("Unit", "REL_TIME"), ("Target", "0:00:00")]);
                    last_pos = 0;
                } else {
                    go_to = Some((index - 1, -1));
                }
            }
            Ok(Cmd::Vol(d)) => {
                if let (Some(rc), Some(v)) = (sp.rc.as_ref(), volume) {
                    let nv = (v as i32 + d).clamp(0, 100) as u32;
                    let n = nv.to_string();
                    if soap(
                        rc,
                        "RenderingControl",
                        "SetVolume",
                        &[
                            ("InstanceID", "0"),
                            ("Channel", "Master"),
                            ("DesiredVolume", &n),
                        ],
                    )
                    .is_ok()
                    {
                        volume = Some(nv);
                        set_status(gen, |s| s.volume = volume);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if go_to.is_none() && last_poll.elapsed() >= Duration::from_secs(2) {
            last_poll = Instant::now();
            let state = av("GetTransportInfo", &[])
                .ok()
                .and_then(|r| tag(&r, "CurrentTransportState"));
            let pos = av("GetPositionInfo", &[]).ok();
            match (&state, &pos) {
                (Some(st), Some(p)) => {
                    fails = 0;
                    let uri = tag(p, "TrackURI").unwrap_or_default();
                    if let Some(d) = tag(p, "TrackDuration")
                        .and_then(|d| hms_ms(&d))
                        .filter(|&d| d > 0)
                    {
                        dur = d;
                    }
                    let rel = tag(p, "RelTime").and_then(|t| hms_ms(&t));
                    if st == "PLAYING" && !uri.is_empty() && !uri.contains(token) {
                        return format!("Another app is using {}", sp.name);
                    }
                    if st == "PLAYING" {
                        playing_seen = true;
                        paused = false;
                        if let Some(ms) = seek_to.take() {
                            let _ = av("Seek", &[("Unit", "REL_TIME"), ("Target", &ms_hms(ms))]);
                        }
                    } else if st == "PAUSED_PLAYBACK" {
                        paused = true;
                    }
                    if let Some(r) = rel {
                        last_pos = r;
                    }
                    if st == "STOPPED" || st == "NO_MEDIA_PRESENT" {
                        let near_end = dur > 0 && last_pos >= dur - 5000;
                        if playing_seen && (near_end || dur <= 0) {
                            go_to = Some((index + 1, 1));
                        } else if playing_seen {
                            return format!("{} stopped", sp.name);
                        } else if loaded_at.elapsed() > Duration::from_secs(20) {
                            // Loaded but never played: the speaker could not use the file.
                            go_to = Some((index + 1, 1));
                        }
                    }
                    let playing = st == "PLAYING" || st == "TRANSITIONING";
                    set_status(gen, |s| {
                        s.playing = playing && !paused;
                        s.pos_ms = last_pos;
                        s.dur_ms = dur;
                    });
                }
                _ => {
                    fails += 1;
                    if fails >= 5 {
                        return format!("Lost {}", sp.name);
                    }
                }
            }
        }
        if let Some((to, dir)) = go_to {
            if to >= songs.len() {
                let _ = av("Stop", &[]);
                return "Played to the end".into();
            }
            index = match load(to, dir) {
                Ok(i) => i,
                Err(e) => return e,
            };
            playing_seen = false;
            paused = false;
            last_pos = 0;
            dur = songs[index].dur_ms;
            loaded_at = Instant::now();
            last_poll = Instant::now();
            set_status(gen, |s| {
                s.index = index;
                s.playing = true;
                s.pos_ms = 0;
                s.dur_ms = dur;
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The casts share one status and one command channel, so the tests that start one take turns.
    pub(crate) static E2E: Mutex<()> = Mutex::new(());

    /// A cast's status without a speaker, for tests of what the screen shows. Take `E2E` first.
    pub(crate) fn set_status_for_test(st: Option<Status>) {
        *STATUS.lock().unwrap() = st;
    }

    /// The shape of a Sonos ZonePlayer description: the renderer is an embedded device, the room
    /// has its own element, and the control URLs are host-absolute.
    const SONOS: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<root xmlns="urn:schemas-upnp-org:device-1-0"><specVersion><major>1</major><minor>0</minor></specVersion>
<device><deviceType>urn:schemas-upnp-org:device:ZonePlayer:1</deviceType>
<friendlyName>192.168.1.20 - Sonos One - RINCON_000E58A1B2C301400</friendlyName>
<manufacturer>Sonos, Inc.</manufacturer><modelName>Sonos One</modelName><roomName>Living Room &amp; Kitchen</roomName>
<serviceList><service><serviceType>urn:schemas-upnp-org:service:AlarmClock:1</serviceType>
<controlURL>/AlarmClock/Control</controlURL></service></serviceList>
<deviceList>
<device><deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType><serviceList>
<service><serviceType>urn:schemas-upnp-org:service:ContentDirectory:1</serviceType><controlURL>/MediaServer/ContentDirectory/Control</controlURL></service>
</serviceList></device>
<device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><serviceList>
<service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
<controlURL>/MediaRenderer/RenderingControl/Control</controlURL><eventSubURL>/MediaRenderer/RenderingControl/Event</eventSubURL></service>
<service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
<controlURL>/MediaRenderer/AVTransport/Control</controlURL></service>
</serviceList></device></deviceList></device></root>"#;

    /// A plain renderer: services on the root device, relative control URLs, a URLBase, prefixed tags.
    const PLAIN: &str = r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0">
<URLBase>http://192.168.1.30:49152/</URLBase><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
<friendlyName>Kitchen Radio</friendlyName><modelName>gmediarender</modelName><serviceList>
<service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>upnp/control/cm</controlURL></service>
<service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>upnp/control/rendertransport1</controlURL></service>
</serviceList></device></root>"#;

    #[test]
    fn descriptions_name_the_speaker_and_its_controls() {
        let s = parse_description(SONOS, "http://192.168.1.20:1400/xml/device_description.xml")
            .unwrap();
        assert_eq!(s.name, "Living Room & Kitchen");
        assert_eq!(s.model, "Sonos One");
        assert_eq!(
            s.av,
            "http://192.168.1.20:1400/MediaRenderer/AVTransport/Control"
        );
        assert_eq!(
            s.rc.as_deref(),
            Some("http://192.168.1.20:1400/MediaRenderer/RenderingControl/Control")
        );
        let p = parse_description(PLAIN, "http://192.168.1.30:49152/description.xml").unwrap();
        assert_eq!(
            (p.name.as_str(), p.model.as_str()),
            ("Kitchen Radio", "gmediarender")
        );
        assert_eq!(
            p.av,
            "http://192.168.1.30:49152/upnp/control/rendertransport1"
        );
        assert_eq!(
            p.rc, None,
            "no RenderingControl: no volume, still a speaker"
        );
        let server_only = SONOS.replace("service:AVTransport:1", "service:Other:1");
        assert_eq!(
            parse_description(&server_only, "http://x/"),
            None,
            "no AVTransport, not a speaker"
        );
    }

    #[test]
    fn xml_and_addresses() {
        assert_eq!(tag("<u:GetVolumeResponse xmlns:u=\"x\"><CurrentVolume>23</CurrentVolume></u:GetVolumeResponse>", "CurrentVolume").as_deref(), Some("23"));
        assert_eq!(elements("<a/><a>x</a>", "a"), vec!["", "x"]);
        assert_eq!(
            unescape("&lt;b&gt; &amp;amp; &#233;&#x41; &bogus; &"),
            "<b> &amp; \u{e9}A &bogus; &"
        );
        assert_eq!(
            escape("<\"Tom & Jerry's\">\u{1}"),
            "&lt;&quot;Tom &amp; Jerry&apos;s&quot;&gt;"
        );
        assert_eq!(join("http://h:1/a/desc.xml", "ctl"), "http://h:1/a/ctl");
        assert_eq!(join("http://h:1/a/desc.xml", "/ctl"), "http://h:1/ctl");
        assert_eq!(join("http://h:1", "ctl"), "http://h:1/ctl");
        assert_eq!(join("http://h:1/", "http://o/x"), "http://o/x");
        let answer = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age = 1800\r\nLocation: http://192.168.1.20:1400/xml/device_description.xml\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
        assert_eq!(
            location(answer).as_deref(),
            Some("http://192.168.1.20:1400/xml/device_description.xml")
        );
        assert_eq!(
            location("HTTP/1.1 200 OK\r\nLOCATION: https://x/\r\n"),
            None,
            "http only"
        );
    }

    #[test]
    fn files_ranges_and_times() {
        assert_eq!(mime("/a/B.FLAC"), Some("audio/flac"));
        assert_eq!(mime("/a/b.m4a"), Some("audio/mp4"));
        assert_eq!(mime("/a/b.dsf"), None, "DSD is not sent");
        assert_eq!(range(None, 100), Ok(None));
        assert_eq!(range(Some("bytes=0-"), 100), Ok(Some((0, 99))));
        assert_eq!(range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(range(Some("bytes=100-"), 100), Err(()));
        assert_eq!(range(Some("bytes=5-2"), 100), Err(()));
        assert_eq!(hms_ms("0:03:21"), Some(201_000));
        assert_eq!(hms_ms("01:00:00.500"), Some(3_600_500));
        assert_eq!(hms_ms("NOT_IMPLEMENTED"), None);
        assert_eq!(ms_hms(3_725_000), "1:02:05");
        let d = didl(
            &Song {
                object_id: 7,
                title: "A & B".into(),
                artist: "<C>".into(),
                dur_ms: 61_000,
                ..Song::default()
            },
            "http://p/x?a=1&b=2",
            "audio/flac",
        );
        assert!(d.contains("<dc:title>A &amp; B</dc:title>") && d.contains("&lt;C&gt;"));
        assert!(d.contains("protocolInfo=\"http-get:*:audio/flac:*\" duration=\"0:01:01.000\">http://p/x?a=1&amp;b=2</res>"));
        assert_eq!(
            dechunk(b"4\r\nWiki\r\n6\r\npedia \r\n0\r\n\r\n"),
            b"Wikipedia "
        );
    }

    /// A pretend speaker on 127.0.0.1: answers SOAP, plays the first song to its end, then stops
    /// it, and fetches part of each file it is given the way a real one does (a Range request).
    #[test]
    fn a_cast_plays_on_through_the_queue_and_stops() {
        let _one = E2E.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cinder-cast-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.flac");
        let b = dir.join("b.mp3");
        std::fs::write(&a, b"0123456789abcdef").unwrap();
        std::fs::write(&b, b"mp3 bytes").unwrap();
        let songs = vec![
            Song {
                object_id: 1,
                path: a.to_string_lossy().into(),
                title: "One".into(),
                dur_ms: 60_000,
                ..Song::default()
            },
            Song {
                object_id: 2,
                path: b.to_string_lossy().into(),
                title: "Two".into(),
                dur_ms: 60_000,
                ..Song::default()
            },
        ];
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let got: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let (log, got) = (log.clone(), got.clone());
            std::thread::spawn(move || {
                let mut uri = String::new();
                let mut polls = 0;
                for c in l.incoming() {
                    let Ok(mut c) = c else { break };
                    let mut r = BufReader::new(c.try_clone().unwrap());
                    let Some(req) = crate::remote::read_head(&mut r) else {
                        continue;
                    };
                    let n: usize = req
                        .header("content-length")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let mut body = vec![0u8; n];
                    r.read_exact(&mut body).unwrap();
                    let body = String::from_utf8_lossy(&body).into_owned();
                    let action = req
                        .header("soapaction")
                        .unwrap_or("")
                        .rsplit('#')
                        .next()
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_string();
                    log.lock().unwrap().push(action.clone());
                    let reply = match action.as_str() {
                        "SetAVTransportURI" => {
                            uri = tag(&body, "CurrentURI").unwrap();
                            assert!(tag(&body, "CurrentURIMetaData").unwrap().contains("<dc:title>"), "metadata arrives as DIDL");
                            // Fetch the first ten bytes, as a speaker probing the file would.
                            let (code, text) = http("GET", &uri, &[("Range", "bytes=0-9")], "").unwrap();
                            got.lock().unwrap().push(format!("{code} {text}"));
                            polls = 0;
                            String::new()
                        }
                        "GetVolume" => "<CurrentVolume>20</CurrentVolume>".into(),
                        "GetTransportInfo" => {
                            polls += 1;
                            let first = uri.contains("/0.");
                            let st = if first && polls >= 2 { "STOPPED" } else { "PLAYING" };
                            format!("<CurrentTransportState>{st}</CurrentTransportState>")
                        }
                        "GetPositionInfo" => format!("<TrackURI>{}</TrackURI><RelTime>0:00:58</RelTime><TrackDuration>0:01:00</TrackDuration>", escape(&uri)),
                        _ => String::new(),
                    };
                    let xml =
                        format!("<s:Envelope><s:Body><u:R>{reply}</u:R></s:Body></s:Envelope>");
                    let _ = c.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{xml}",
                            xml.len()
                        )
                        .as_bytes(),
                    );
                }
            });
        }
        let sp = Speaker {
            name: "Test".into(),
            model: "fake".into(),
            av: format!("http://127.0.0.1:{port}/av"),
            rc: Some(format!("http://127.0.0.1:{port}/rc")),
        };
        start(sp, songs, 0, 0).unwrap();
        let t0 = Instant::now();
        while status().index != 1 && t0.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            status().index,
            1,
            "the second song followed the first by itself"
        );
        assert_eq!(status().volume, Some(20));
        assert!(send(Cmd::Vol(3)));
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(status().volume, Some(23));
        stop();
        assert!(!active());
        let log = log.lock().unwrap().clone();
        let count = |a: &str| log.iter().filter(|x| *x == a).count();
        assert_eq!(count("SetAVTransportURI"), 2, "{log:?}");
        assert_eq!(count("Play"), 2, "{log:?}");
        assert_eq!(log.last().map(String::as_str), Some("Stop"), "{log:?}");
        assert!(log.contains(&"SetVolume".to_string()));
        let got = got.lock().unwrap().clone();
        assert_eq!(
            got,
            vec!["206 0123456789".to_string(), "206 mp3 bytes".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
