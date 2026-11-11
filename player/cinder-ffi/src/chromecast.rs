//! Google Cast speakers for Settings ▸ Wi-Fi ▸ Speakers (with `cast.rs`, which owns the file
//! server, the commands and the status): Chromecast Audio, Google/Nest speakers, and speakers,
//! soundbars and TVs with Chromecast built in.
//!
//! Found by mDNS (`_googlecast._tcp.local`, one query to 224.0.0.251:5353 from an ordinary port,
//! answered by unicast). Driven over the Cast v2 protocol: TLS to the device's port (8009), then
//! length-prefixed protobuf `CastMessage`s carrying JSON. The device's own Default Media Receiver
//! app (`CC1AD845`) is launched and told to `LOAD` each song's URL from the player's file server;
//! its MEDIA_STATUS says when a song has FINISHED. The TLS is `openssl s_client` from the Wi-Fi
//! option (the device's certificate is its own self-signed one, as for every Cast sender).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::cast::{set_status, Cmd, Song, Speaker};
use crate::net::Json;

/// `Speaker::av` for a Cast device: `cast://<ip>:<port>`.
pub const SCHEME: &str = "cast://";
const APP: &str = "CC1AD845"; // the Default Media Receiver
const NS_CONN: &str = "urn:x-cast:com.google.cast.tp.connection";
const NS_BEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const NS_RECV: &str = "urn:x-cast:com.google.cast.receiver";
const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

// ── mDNS ──────────────────────────────────────────────────────────────────────────────────────────

/// The PTR question for `_googlecast._tcp.local`, asking for a unicast answer.
pub fn mdns_query() -> Vec<u8> {
    let mut q = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in ["_googlecast", "_tcp", "local"] {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 12, 0x80, 1]); // root, PTR, unicast-response + IN
    q
}

/// A (possibly compressed) DNS name at `at`; the name and where the record continues.
fn dns_name(p: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *p.get(at)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(at + 1)));
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *p.get(at + 1)? as usize;
            end.get_or_insert(at + 2);
            at = ptr;
            continue;
        }
        labels.push(String::from_utf8_lossy(p.get(at + 1..at + 1 + len)?).into_owned());
        at += 1 + len;
    }
    None
}

#[derive(Default)]
struct Found {
    port: Option<u16>,
    host: Option<String>,
    txt: HashMap<String, String>,
}

/// The Cast devices in one mDNS answer, `from` = the address it came from (used when the answer
/// carries no A record).
pub fn parse_mdns(p: &[u8], from: Ipv4Addr) -> Vec<Speaker> {
    let count = |i: usize| {
        p.get(i..i + 2)
            .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as usize)
    };
    if p.len() < 12 {
        return Vec::new();
    }
    let (qd, records) = (count(4), count(6) + count(8) + count(10));
    let mut at = 12;
    for _ in 0..qd {
        let Some((_, next)) = dns_name(p, at) else {
            return Vec::new();
        };
        at = next + 4;
    }
    let mut instances: Vec<String> = Vec::new();
    let mut found: HashMap<String, Found> = HashMap::new();
    let mut addrs: HashMap<String, Ipv4Addr> = HashMap::new();
    for _ in 0..records {
        let Some((name, next)) = dns_name(p, at) else {
            break;
        };
        let Some(h) = p.get(next..next + 10) else {
            break;
        };
        let (ty, len) = (
            u16::from_be_bytes([h[0], h[1]]),
            u16::from_be_bytes([h[8], h[9]]) as usize,
        );
        let rd = next + 10;
        let Some(data) = p.get(rd..rd + len) else {
            break;
        };
        match ty {
            12 => {
                if let Some((target, _)) = dns_name(p, rd) {
                    if name.eq_ignore_ascii_case("_googlecast._tcp.local")
                        && !instances.contains(&target)
                    {
                        instances.push(target);
                    }
                }
            }
            33 if len >= 6 => {
                let f = found.entry(name).or_default();
                f.port = Some(u16::from_be_bytes([data[4], data[5]]));
                f.host = dns_name(p, rd + 6).map(|(h, _)| h);
            }
            16 => {
                let f = found.entry(name).or_default();
                let mut i = 0;
                while i < data.len() {
                    let n = data[i] as usize;
                    if let Some(kv) = data.get(i + 1..i + 1 + n) {
                        let kv = String::from_utf8_lossy(kv);
                        if let Some((k, v)) = kv.split_once('=') {
                            f.txt.insert(k.to_ascii_lowercase(), v.to_string());
                        }
                    }
                    i += 1 + n;
                }
            }
            1 if len == 4 => {
                addrs.insert(
                    name.to_ascii_lowercase(),
                    Ipv4Addr::new(data[0], data[1], data[2], data[3]),
                );
            }
            _ => {}
        }
        at = rd + len;
    }
    instances
        .iter()
        .filter_map(|inst| {
            let f = found.get(inst)?;
            let ip = f
                .host
                .as_ref()
                .and_then(|h| addrs.get(&h.to_ascii_lowercase()).copied())
                .unwrap_or(from);
            let name = f
                .txt
                .get("fn")
                .cloned()
                .unwrap_or_else(|| inst.split('.').next().unwrap_or("Cast").to_string());
            Some(Speaker {
                name: name.chars().take(60).collect(),
                model: f
                    .txt
                    .get("md")
                    .map_or_else(
                        || "Google Cast".to_string(),
                        |m| format!("{m} · Google Cast"),
                    )
                    .chars()
                    .take(60)
                    .collect(),
                av: format!("{SCHEME}{ip}:{}", f.port.unwrap_or(8009)),
                rc: None,
            })
        })
        .collect()
}

/// The Cast devices that answer within `wait`.
pub fn discover(wait: Duration) -> Vec<Speaker> {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    let _ = sock.set_multicast_ttl_v4(255);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let q = mdns_query();
    let _ = sock.send_to(&q, ("224.0.0.251", 5353));
    let t0 = Instant::now();
    let mut resent = false;
    let mut out: Vec<Speaker> = Vec::new();
    let mut buf = [0u8; 4096];
    while t0.elapsed() < wait {
        if !resent && t0.elapsed() > Duration::from_millis(800) {
            let _ = sock.send_to(&q, ("224.0.0.251", 5353));
            resent = true;
        }
        if let Ok((n, std::net::SocketAddr::V4(from))) = sock.recv_from(&mut buf) {
            for sp in parse_mdns(&buf[..n], *from.ip()) {
                if !out.iter().any(|o| o.av == sp.av) {
                    out.push(sp);
                }
            }
        }
    }
    out
}

// ── Cast v2 messages ──────────────────────────────────────────────────────────────────────────────

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// One CastMessage, with its 4-byte big-endian length in front.
pub fn frame(src: &str, dst: &str, ns: &str, json: &str) -> Vec<u8> {
    let mut m = vec![0x08, 0x00]; // 1: protocol_version CASTV2_1_0
    for (field, s) in [(2u8, src), (3, dst), (4, ns)] {
        m.push(field << 3 | 2);
        varint(&mut m, s.len() as u64);
        m.extend_from_slice(s.as_bytes());
    }
    m.extend_from_slice(&[0x28, 0x00]); // 5: payload_type STRING
    m.push(6 << 3 | 2);
    varint(&mut m, json.len() as u64);
    m.extend_from_slice(json.as_bytes());
    let mut out = (m.len() as u32).to_be_bytes().to_vec();
    out.extend(m);
    out
}

/// A CastMessage's source, namespace and JSON payload.
pub fn unframe(m: &[u8]) -> Option<(String, String, String)> {
    let (mut src, mut ns, mut json) = (String::new(), String::new(), String::new());
    let mut at = 0;
    let read_varint = |at: &mut usize| -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = *m.get(*at)?;
            *at += 1;
            v |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    };
    while at < m.len() {
        let key = read_varint(&mut at)?;
        match key & 7 {
            0 => {
                read_varint(&mut at)?;
            }
            2 => {
                let len = read_varint(&mut at)? as usize;
                let bytes = m.get(at..at + len)?;
                at += len;
                let s = String::from_utf8_lossy(bytes).into_owned();
                match key >> 3 {
                    2 => src = s,
                    4 => ns = s,
                    6 => json = s,
                    _ => {}
                }
            }
            _ => return None,
        }
    }
    Some((src, ns, json))
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The LOAD for one song.
pub fn load_json(s: &Song, url: &str, mime: &str, start_s: f64, req: u32) -> String {
    format!(
        "{{\"type\":\"LOAD\",\"requestId\":{req},\"autoplay\":true,\"currentTime\":{start_s:.1},\"media\":{{\"contentId\":{},\
         \"contentType\":{},\"streamType\":\"BUFFERED\",\"metadata\":{{\"metadataType\":3,\"title\":{},\"artist\":{},\"albumName\":{}}}}}}}",
        jstr(url),
        jstr(mime),
        jstr(&s.title),
        jstr(&s.artist),
        jstr(&s.album)
    )
}

/// What a MEDIA_STATUS says: the player state, why it is idle, the position and the length (s),
/// and the media session id.
#[derive(Debug, Default, PartialEq)]
pub struct Media {
    pub state: String,
    pub idle: String,
    pub pos_s: f64,
    pub dur_s: f64,
    pub session: Option<i64>,
}

pub fn media_status(json: &str) -> Option<Media> {
    let v = Json::parse(json)?;
    if v.get("type")?.str()? != "MEDIA_STATUS" {
        return None;
    }
    let Some(st) = v.get("status")?.items().first() else {
        return Some(Media {
            state: "IDLE".into(),
            ..Media::default()
        });
    };
    Some(Media {
        state: st
            .get("playerState")
            .and_then(Json::str)
            .unwrap_or("")
            .to_string(),
        idle: st
            .get("idleReason")
            .and_then(Json::str)
            .unwrap_or("")
            .to_string(),
        pos_s: st.get("currentTime").and_then(Json::num).unwrap_or(0.0),
        dur_s: st
            .get("media")
            .and_then(|m| m.get("duration"))
            .and_then(Json::num)
            .unwrap_or(0.0),
        session: st
            .get("mediaSessionId")
            .and_then(Json::num)
            .map(|n| n as i64),
    })
}

/// From a RECEIVER_STATUS: the Default Media Receiver's transport id once it runs, and the volume.
pub fn receiver_status(json: &str) -> Option<(Option<String>, Option<f64>)> {
    let v = Json::parse(json)?;
    if v.get("type")?.str()? != "RECEIVER_STATUS" {
        return None;
    }
    let st = v.get("status")?;
    let transport = st
        .get("applications")
        .map(|a| a.items())
        .unwrap_or(&[])
        .iter()
        .find(|a| a.get("appId").and_then(Json::str) == Some(APP))
        .and_then(|a| a.get("transportId").and_then(Json::str))
        .map(str::to_string);
    let level = st
        .get("volume")
        .and_then(|v| v.get("level"))
        .and_then(Json::num);
    Some((transport, level))
}

// ── The session ───────────────────────────────────────────────────────────────────────────────────

const ME: &str = "sender-cinder";

fn send(wr: &mut dyn Write, dst: &str, ns: &str, json: &str) -> bool {
    wr.write_all(&frame(ME, dst, ns, json))
        .and_then(|_| wr.flush())
        .is_ok()
}

type Link = (
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
    Option<std::process::Child>,
);

/// TLS to the device: `openssl s_client` from the Wi-Fi option, stdin/stdout as the stream.
#[cfg(not(test))]
fn connect(addr: &str) -> Result<Link, String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(format!("{}/openssl", crate::net::WIFI_DIR))
        .args([
            "s_client",
            "-connect",
            addr,
            "-quiet",
            "-nocommands",
            "-verify_quiet",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("openssl: {e}"))?;
    let r = child.stdout.take().ok_or("no stdout")?;
    let w = child.stdin.take().ok_or("no stdin")?;
    Ok((Box::new(r), Box::new(w), Some(child)))
}

/// Tests talk plain TCP to a pretend device (the TLS layer is openssl's own).
#[cfg(test)]
fn connect(addr: &str) -> Result<Link, String> {
    let s = std::net::TcpStream::connect(addr).map_err(|e| e.to_string())?;
    let r = s.try_clone().map_err(|e| e.to_string())?;
    Ok((Box::new(r), Box::new(s), None))
}

/// The cast on a Google Cast device; returns why it ended ("" = asked to stop). The same contract
/// as `cast::session`.
pub fn session(
    sp: &Speaker,
    songs: &[Song],
    first: usize,
    start_ms: i64,
    base: &str,
    rx: &mpsc::Receiver<Cmd>,
    gen: u64,
) -> String {
    let addr = sp.av.trim_start_matches(SCHEME);
    let (mut rd, mut wr, child) = match connect(addr) {
        Ok(l) => l,
        Err(e) => return format!("Could not reach {}: {e}", sp.name),
    };
    // Incoming messages on their own thread, into a channel.
    let (in_tx, in_rx) = mpsc::channel::<(String, String, String)>();
    std::thread::Builder::new()
        .name("cinder-cast-rx".into())
        .stack_size(96 * 1024)
        .spawn(move || {
            let mut len = [0u8; 4];
            while rd.read_exact(&mut len).is_ok() {
                let n = u32::from_be_bytes(len) as usize;
                if n > 1 << 20 {
                    break;
                }
                let mut m = vec![0u8; n];
                if rd.read_exact(&mut m).is_err() {
                    break;
                }
                if let Some(msg) = unframe(&m) {
                    if in_tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();
    let note = run(sp, songs, first, start_ms, base, rx, gen, &mut *wr, &in_rx);
    drop(wr);
    if let Some(mut c) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
    note
}

#[allow(clippy::too_many_arguments)]
fn run(
    sp: &Speaker,
    songs: &[Song],
    first: usize,
    start_ms: i64,
    base: &str,
    rx: &mpsc::Receiver<Cmd>,
    gen: u64,
    wr: &mut dyn Write,
    in_rx: &mpsc::Receiver<(String, String, String)>,
) -> String {
    let mut req = 1u32;
    if !send(wr, "receiver-0", NS_CONN, "{\"type\":\"CONNECT\"}")
        || !send(
            wr,
            "receiver-0",
            NS_RECV,
            &format!("{{\"type\":\"LAUNCH\",\"appId\":\"{APP}\",\"requestId\":{req}}}"),
        )
    {
        return format!("Lost {}", sp.name);
    }
    req += 1;
    // Wait for the receiver app (it takes a few seconds the first time).
    let mut transport = None;
    let mut volume: Option<f64> = None;
    let t0 = Instant::now();
    while transport.is_none() && t0.elapsed() < Duration::from_secs(20) {
        match in_rx.recv_timeout(Duration::from_millis(500)) {
            Ok((src, ns, json)) => {
                if ns == NS_BEAT && json.contains("PING") {
                    send(wr, &src, NS_BEAT, "{\"type\":\"PONG\"}");
                } else if let Some((t, v)) = receiver_status(&json) {
                    transport = t;
                    volume = v.or(volume);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return format!("Lost {}", sp.name),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Ok(Cmd::Stop) = rx.try_recv() {
            return String::new();
        }
    }
    let Some(tid) = transport else {
        return format!("{} did not start its player", sp.name);
    };
    send(wr, &tid, NS_CONN, "{\"type\":\"CONNECT\"}");
    let pct = |v: Option<f64>| v.map(|l| (l * 100.0).round() as u32);
    set_status(gen, |s| s.volume = pct(volume));
    let load = |wr: &mut dyn Write, i: usize, at_s: f64, req: u32| {
        let s = &songs[i];
        let mime = crate::cast::mime(&s.path).unwrap_or("audio/mpeg");
        let ext = s
            .path
            .rsplit_once('.')
            .map_or("bin", |(_, e)| e)
            .to_ascii_lowercase();
        let url = format!("{base}/{i}.{ext}");
        send(wr, &tid, NS_MEDIA, &load_json(s, &url, mime, at_s, req))
    };
    let next_playable =
        |from: usize| (from..songs.len()).find(|&i| crate::cast::mime(&songs[i].path).is_some());
    let Some(mut index) = next_playable(first) else {
        return "Nothing here this speaker can play".into();
    };
    load(
        wr,
        index,
        if index == first {
            start_ms as f64 / 1000.0
        } else {
            0.0
        },
        req,
    );
    req += 1;
    let mut session: Option<i64> = None;
    let mut paused = false;
    let mut seen_playing = false;
    let mut last_pos = 0.0f64;
    let mut dur = songs[index].dur_ms as f64 / 1000.0;
    let mut beat = Instant::now();
    let mut poll = Instant::now();
    let mut loaded = Instant::now();
    let publish = |index: usize, playing: bool, pos: f64, dur: f64| {
        set_status(gen, |s| {
            s.index = index;
            s.playing = playing;
            s.pos_ms = (pos * 1000.0) as i64;
            s.dur_ms = (dur * 1000.0) as i64;
        })
    };
    publish(index, true, 0.0, dur);
    eprintln!(
        "cinder-ffi: cast: playing {} of {} on {} (Google Cast, {})",
        index + 1,
        songs.len(),
        sp.name,
        sp.model
    );
    loop {
        let mut go: Option<usize> = None;
        // The device's messages first.
        loop {
            match in_rx.try_recv() {
                Ok((src, ns, json)) => {
                    if ns == NS_BEAT && json.contains("\"PING\"") {
                        send(wr, &src, NS_BEAT, "{\"type\":\"PONG\"}");
                    } else if ns == NS_CONN && json.contains("\"CLOSE\"") {
                        return format!("{} closed the connection", sp.name);
                    } else if let Some((t, v)) = receiver_status(&json) {
                        if v.is_some() {
                            volume = v;
                            set_status(gen, |s| s.volume = pct(volume));
                        }
                        if t.as_deref() != Some(tid.as_str()) {
                            return format!("Another app is using {}", sp.name);
                        }
                    } else if let Some(m) = media_status(&json) {
                        if m.session.is_some() {
                            session = m.session;
                        }
                        if m.dur_s > 0.0 {
                            dur = m.dur_s;
                        }
                        last_pos = m.pos_s.max(0.0);
                        match m.state.as_str() {
                            "PLAYING" | "BUFFERING" => {
                                seen_playing = true;
                                paused = false;
                            }
                            "PAUSED" => paused = true,
                            "IDLE" if m.idle == "FINISHED" => go = Some(index + 1),
                            "IDLE" if m.idle == "ERROR" => go = Some(index + 1), // this file would not play
                            "IDLE"
                                if seen_playing
                                    && (m.idle == "CANCELLED" || m.idle == "INTERRUPTED") =>
                            {
                                return format!("{} stopped", sp.name);
                            }
                            _ => {}
                        }
                        publish(index, !paused && m.state != "IDLE", last_pos, dur);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return format!("Lost {}", sp.name),
            }
        }
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Cmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(id) = session {
                    send(
                        wr,
                        &tid,
                        NS_MEDIA,
                        &format!(
                            "{{\"type\":\"STOP\",\"mediaSessionId\":{id},\"requestId\":{req}}}"
                        ),
                    );
                }
                send(wr, &tid, NS_CONN, "{\"type\":\"CLOSE\"}");
                return String::new();
            }
            Ok(Cmd::Toggle) => {
                if let Some(id) = session {
                    let ty = if paused { "PLAY" } else { "PAUSE" };
                    send(
                        wr,
                        &tid,
                        NS_MEDIA,
                        &format!(
                            "{{\"type\":\"{ty}\",\"mediaSessionId\":{id},\"requestId\":{req}}}"
                        ),
                    );
                    req += 1;
                    paused = !paused;
                    set_status(gen, |s| s.playing = !paused);
                }
            }
            Ok(Cmd::Next) => go = Some(index + 1),
            Ok(Cmd::Prev) => {
                if last_pos > 3.0 || index == 0 {
                    if let Some(id) = session {
                        send(wr, &tid, NS_MEDIA, &format!("{{\"type\":\"SEEK\",\"mediaSessionId\":{id},\"currentTime\":0,\"requestId\":{req}}}"));
                        req += 1;
                    }
                } else {
                    go = Some(index - 1);
                }
            }
            Ok(Cmd::Seek(ms)) => {
                if let Some(id) = session {
                    let secs = ms.max(0) as f64 / 1000.0;
                    send(wr, &tid, NS_MEDIA, &format!("{{\"type\":\"SEEK\",\"mediaSessionId\":{id},\"currentTime\":{secs:.1},\"requestId\":{req}}}"));
                    req += 1;
                    last_pos = secs;
                    set_status(gen, |s| s.pos_ms = ms);
                }
            }
            Ok(Cmd::Vol(d)) => {
                if let Some(l) = volume {
                    let nv = (l + d as f64 / 100.0).clamp(0.0, 1.0);
                    send(wr, "receiver-0", NS_RECV, &format!("{{\"type\":\"SET_VOLUME\",\"volume\":{{\"level\":{nv:.2}}},\"requestId\":{req}}}"));
                    req += 1;
                    volume = Some(nv);
                    set_status(gen, |s| s.volume = pct(volume));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if beat.elapsed() >= Duration::from_secs(5) {
            beat = Instant::now();
            if !send(wr, "receiver-0", NS_BEAT, "{\"type\":\"PING\"}") {
                return format!("Lost {}", sp.name);
            }
        }
        if go.is_none() && poll.elapsed() >= Duration::from_secs(2) {
            poll = Instant::now();
            let ask = match session {
                Some(id) => format!(
                    "{{\"type\":\"GET_STATUS\",\"mediaSessionId\":{id},\"requestId\":{req}}}"
                ),
                None => format!("{{\"type\":\"GET_STATUS\",\"requestId\":{req}}}"),
            };
            send(wr, &tid, NS_MEDIA, &ask);
            req += 1;
            if !seen_playing && loaded.elapsed() > Duration::from_secs(25) {
                go = Some(index + 1); // loaded but never played: the device could not use the file
            }
        }
        if let Some(to) = go {
            let Some(i) = next_playable(to) else {
                if let Some(id) = session {
                    send(
                        wr,
                        &tid,
                        NS_MEDIA,
                        &format!(
                            "{{\"type\":\"STOP\",\"mediaSessionId\":{id},\"requestId\":{req}}}"
                        ),
                    );
                }
                return "Played to the end".into();
            };
            index = i;
            load(wr, index, 0.0, req);
            req += 1;
            session = None;
            paused = false;
            seen_playing = false;
            last_pos = 0.0;
            dur = songs[index].dur_ms as f64 / 1000.0;
            loaded = Instant::now();
            publish(index, true, 0.0, dur);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::{self, Cmd, Song, Speaker};
    use std::io::{BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn name(out: &mut Vec<u8>, labels: &[&str]) {
        for l in labels {
            out.push(l.len() as u8);
            out.extend_from_slice(l.as_bytes());
        }
        out.push(0);
    }
    fn rr(out: &mut Vec<u8>, owner: &[u8], ty: u16, data: &[u8]) {
        out.extend_from_slice(owner);
        out.extend_from_slice(&ty.to_be_bytes());
        out.extend_from_slice(&[0x80, 1, 0, 0, 0, 120]);
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    }

    /// An answer laid out as RFC 6762/6763 describe a Cast device's: PTR in the answers; SRV, TXT
    /// and A among the additionals; names compressed against the first one.
    fn answer() -> Vec<u8> {
        let mut p = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 3];
        let service_at = p.len() as u8;
        name(&mut p, &["_googlecast", "_tcp", "local"]);
        p.extend_from_slice(&[0, 12, 0, 1, 0, 0, 0, 120]);
        let mut inst = vec![26u8];
        inst.extend_from_slice(b"Google-Nest-Mini-0a1b2c3d");
        inst.extend_from_slice(&[0xC0, service_at]);
        p.extend_from_slice(&(inst.len() as u16).to_be_bytes());
        let inst_at = p.len() as u8;
        p.extend_from_slice(&inst);
        let owner = [0xC0, inst_at];
        let mut srv = vec![0, 0, 0, 0, 0x1F, 0x49]; // priority, weight, port 8009
        name(&mut srv, &["0a1b2c3d-host", "local"]);
        rr(&mut p, &owner, 33, &srv);
        let mut txt = Vec::new();
        for kv in [
            "id=0a1b2c3d",
            "md=Google Nest Mini",
            "fn=Kitchen speaker",
            "ca=199172",
        ] {
            txt.push(kv.len() as u8);
            txt.extend_from_slice(kv.as_bytes());
        }
        rr(&mut p, &owner, 16, &txt);
        let mut host = Vec::new();
        name(&mut host, &["0a1b2c3d-host", "local"]);
        rr(&mut p, &host, 1, &[192, 168, 1, 50]);
        p
    }

    #[test]
    fn mdns_names_the_device_and_its_address() {
        let q = mdns_query();
        assert_eq!(&q[12..13], &[11]);
        assert!(q.ends_with(&[0, 0, 12, 0x80, 1]));
        let v = parse_mdns(&answer(), Ipv4Addr::new(10, 0, 0, 9));
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].name, "Kitchen speaker");
        assert_eq!(v[0].model, "Google Nest Mini · Google Cast");
        assert_eq!(v[0].av, "cast://192.168.1.50:8009");
        assert!(
            parse_mdns(&answer()[..40], Ipv4Addr::LOCALHOST).is_empty(),
            "a cut-off answer is nothing"
        );
        let mut no_a = answer();
        no_a.truncate(no_a.len() - (21 + 10 + 4)); // the A record: its name, header and address
        no_a[11] = 2;
        let v = parse_mdns(&no_a, Ipv4Addr::new(10, 0, 0, 9));
        assert_eq!(
            v[0].av, "cast://10.0.0.9:8009",
            "no A record: the address it came from"
        );
        assert_eq!(
            v[0].name, "Kitchen speaker",
            "the TXT record is still whole"
        );
    }

    #[test]
    fn messages_go_out_and_come_back() {
        let json = format!("{{\"type\":\"LOAD\",\"pad\":\"{}\"}}", "x".repeat(300));
        let f = frame("sender-cinder", "web-5", NS_MEDIA, &json);
        assert_eq!(
            u32::from_be_bytes([f[0], f[1], f[2], f[3]]) as usize,
            f.len() - 4
        );
        assert_eq!(
            unframe(&f[4..]),
            Some(("sender-cinder".into(), NS_MEDIA.into(), json))
        );
        assert_eq!(
            unframe(&[0x12, 0x05, b'a']),
            None,
            "a field longer than the message"
        );
        let m = media_status(r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":3,"playerState":"IDLE","idleReason":"FINISHED","currentTime":181.2,"media":{"duration":181.5}}],"requestId":0}"#).unwrap();
        assert_eq!(
            (m.state.as_str(), m.idle.as_str(), m.session),
            ("IDLE", "FINISHED", Some(3))
        );
        assert_eq!((m.pos_s, m.dur_s), (181.2, 181.5));
        let r = receiver_status(
            r#"{"type":"RECEIVER_STATUS","requestId":1,"status":{"applications":[{"appId":"CC1AD845","displayName":"Default Media Receiver","sessionId":"7E2F","transportId":"web-5"}],"volume":{"level":0.3,"muted":false}}}"#,
        );
        assert_eq!(r, Some((Some("web-5".into()), Some(0.3))));
        let l = load_json(
            &Song {
                title: "Say \"hi\"".into(),
                ..Song::default()
            },
            "http://p/0.flac",
            "audio/flac",
            12.0,
            4,
        );
        let v = Json::parse(&l).unwrap();
        assert_eq!(
            v.get("media")
                .and_then(|m| m.get("contentId"))
                .and_then(Json::str),
            Some("http://p/0.flac")
        );
        assert_eq!(
            v.get("media")
                .and_then(|m| m.get("metadata"))
                .and_then(|m| m.get("title"))
                .and_then(Json::str),
            Some("Say \"hi\"")
        );
        assert_eq!(v.get("currentTime").and_then(Json::num), Some(12.0));
    }

    /// A pretend Cast device on 127.0.0.1 (plain TCP; the TLS is openssl's): launches the receiver,
    /// plays the first song to its end, fetches each file it is given, pings, and is stopped.
    #[test]
    fn a_cast_device_plays_through_the_queue() {
        let _one = cast::tests::E2E.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cinder-gcast-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.flac");
        let b = dir.join("b.opus");
        std::fs::write(&a, b"FLAC data here").unwrap();
        std::fs::write(&b, b"Opus").unwrap();
        let songs = vec![
            Song {
                object_id: 1,
                path: a.to_string_lossy().into(),
                title: "One".into(),
                dur_ms: 1000,
                ..Song::default()
            },
            Song {
                object_id: 2,
                path: b.to_string_lossy().into(),
                title: "Two".into(),
                dur_ms: 1000,
                ..Song::default()
            },
        ];
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let (c, _) = l.accept().unwrap();
                let mut w = c.try_clone().unwrap();
                let mut r = BufReader::new(c);
                let mut polls = 0;
                let mut url = String::new();
                let say = |w: &mut std::net::TcpStream, src: &str, ns: &str, json: &str| {
                    let _ = w.write_all(&frame(src, "sender-cinder", ns, json));
                };
                say(&mut w, "receiver-0", NS_BEAT, "{\"type\":\"PING\"}");
                let mut len = [0u8; 4];
                while r.read_exact(&mut len).is_ok() {
                    let mut m = vec![0u8; u32::from_be_bytes(len) as usize];
                    r.read_exact(&mut m).unwrap();
                    let (_, ns, json) = unframe(&m).unwrap();
                    let v = Json::parse(&json).unwrap();
                    let ty = v.get("type").and_then(Json::str).unwrap_or("").to_string();
                    seen.lock().unwrap().push(ty.clone());
                    match ty.as_str() {
                        "LAUNCH" => say(
                            &mut w,
                            "receiver-0",
                            NS_RECV,
                            r#"{"type":"RECEIVER_STATUS","requestId":1,"status":{"applications":[{"appId":"CC1AD845","transportId":"web-5"}],"volume":{"level":0.3}}}"#,
                        ),
                        "LOAD" => {
                            url = v
                                .get("media")
                                .and_then(|m| m.get("contentId"))
                                .and_then(Json::str)
                                .unwrap()
                                .to_string();
                            let (code, body) =
                                cast::http("GET", &url, &[("Range", "bytes=0-3")], "").unwrap();
                            seen.lock().unwrap().push(format!("{code} {body}"));
                            polls = 0;
                            say(
                                &mut w,
                                "web-5",
                                NS_MEDIA,
                                r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":1,"playerState":"PLAYING","currentTime":0}]}"#,
                            );
                        }
                        "GET_STATUS" if ns == NS_MEDIA => {
                            polls += 1;
                            let st = if url.contains("/0.") && polls >= 1 {
                                r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":1,"playerState":"IDLE","idleReason":"FINISHED","currentTime":1.0}]}"#
                            } else {
                                r#"{"type":"MEDIA_STATUS","status":[{"mediaSessionId":2,"playerState":"PLAYING","currentTime":0.4,"media":{"duration":1.0}}]}"#
                            };
                            say(&mut w, "web-5", NS_MEDIA, st);
                        }
                        "CLOSE" if ns == NS_CONN => break,
                        _ => {}
                    }
                }
            });
        }
        let sp = Speaker {
            name: "Nest".into(),
            model: "fake".into(),
            av: format!("{SCHEME}127.0.0.1:{port}"),
            rc: None,
        };
        cast::start(sp, songs, 0, 0).unwrap();
        let t0 = Instant::now();
        while cast::status().index != 1 && t0.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            cast::status().index,
            1,
            "the second song followed the first"
        );
        assert_eq!(cast::status().volume, Some(30));
        assert!(cast::send(Cmd::Vol(4)));
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(cast::status().volume, Some(34));
        cast::stop();
        std::thread::sleep(Duration::from_millis(300));
        let seen = seen.lock().unwrap().clone();
        let count = |t: &str| seen.iter().filter(|x| *x == t).count();
        assert_eq!(count("CONNECT"), 2, "receiver-0 and the app: {seen:?}");
        assert_eq!(count("LOAD"), 2, "{seen:?}");
        assert!(
            seen.contains(&"PONG".to_string()),
            "the device's PING was answered: {seen:?}"
        );
        assert!(seen.contains(&"SET_VOLUME".to_string()), "{seen:?}");
        assert!(
            seen.contains(&"206 FLAC".to_string()) && seen.contains(&"206 Opus".to_string()),
            "{seen:?}"
        );
        assert!(
            seen.ends_with(&["STOP".to_string(), "CLOSE".to_string()]),
            "{seen:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
