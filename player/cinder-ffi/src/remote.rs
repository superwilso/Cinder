//! Settings ▸ Wi-Fi ▸ Phone remote: a page on the home network (`http://<player>:8080`) with the
//! song playing, the transport and volume buttons, and an upload box that puts music on the player
//! without a cable. Opt-in, off by default, and only while Wi-Fi is on.
//!
//! LOCKED BY A CODE. Each time the switch goes on, a new 6-digit code is drawn and shown on the
//! Wi-Fi page; the page asks for it once and keeps it in a cookie (SameSite=Strict). Five wrong
//! codes lock the login for a minute. Every request that does something also needs the `X-Cinder`
//! header, which a page on another site cannot send without a CORS preflight this server never
//! answers — so a link elsewhere cannot press Play or upload. Plain HTTP: anyone on the same
//! network who can read its traffic can read the code; the Wi-Fi password is what keeps them out.
//!
//! The server is a thread here, std only, one connection at a time. It never calls into the
//! navigator: button presses are queued for the shell ([`take_button`]), which feeds them through
//! the same `cinder_input` path as the side keys, and the shell is told when uploads finished so it
//! can rescan the library ([`take_uploaded`]).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Where uploads land: a folder of their own in the player's music folder.
pub const UPLOAD_DIR: &str = "/contents/MUSIC/Uploads";
/// The audio files the player plays; anything else is refused before a byte is written.
const AUDIO_EXT: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "wav", "aif", "aiff", "wma", "dsf", "dff", "ape", "ogg", "opus",
];
const MAX_UPLOAD: u64 = 2 << 30; // 2 GB, the largest file vfat on the player is happy with

#[derive(Default)]
struct Shared {
    code: String,
    buttons: VecDeque<i32>,
    uploaded: u32,
    title: String,
    artist: String,
    playing: bool,
    fails: u32,
    locked_until: Option<Instant>,
}

static RUN: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<Shared>> = OnceLock::new();
fn state() -> &'static Mutex<Shared> {
    STATE.get_or_init(|| Mutex::new(Shared::default()))
}

/// A 6-digit code from the kernel's random pool (no crate for it).
pub fn new_code() -> String {
    let mut b = [0u8; 4];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    let n = u32::from_le_bytes(b);
    format!("{:06}", n % 1_000_000)
}

/// Start serving with `code`. Returns the port, or why it could not.
pub fn start(code: &str) -> Result<u16, String> {
    if RUN.load(Ordering::SeqCst) {
        stop();
        std::thread::sleep(Duration::from_millis(600));
    }
    let mut listener = None;
    for port in 8080..8090u16 {
        if let Ok(l) = TcpListener::bind(("0.0.0.0", port)) {
            listener = Some((l, port));
            break;
        }
    }
    let (l, port) = listener.ok_or("no free port")?;
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    {
        let mut s = state().lock().unwrap();
        s.code = code.to_string();
        s.fails = 0;
        s.locked_until = None;
    }
    RUN.store(true, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("cinder-remote".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            while RUN.load(Ordering::SeqCst) {
                match l.accept() {
                    Ok((c, _)) => {
                        let _ = c.set_nonblocking(false);
                        let _ = c.set_read_timeout(Some(Duration::from_secs(15)));
                        let _ = c.set_write_timeout(Some(Duration::from_secs(15)));
                        serve(c);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(250)),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(port)
}

pub fn stop() {
    RUN.store(false, Ordering::SeqCst);
    let mut s = state().lock().unwrap();
    s.code.clear();
    s.buttons.clear();
}

pub fn running() -> bool {
    RUN.load(Ordering::SeqCst)
}

/// The next button the page pressed (a `cinder_button_t`), for the shell's input path.
pub fn take_button() -> Option<i32> {
    state().lock().unwrap().buttons.pop_front()
}

/// How many uploads finished since the last call (the shell rescans the library when it is > 0).
/// Files that arrived another way (the shared drive): counted toward the same rescan.
pub fn note_uploaded(n: u32) {
    let mut s = state().lock().unwrap();
    s.uploaded = s.uploaded.saturating_add(n);
}

pub fn take_uploaded() -> u32 {
    std::mem::take(&mut state().lock().unwrap().uploaded)
}

/// What the page shows as playing.
pub fn set_now(title: &str, artist: &str, playing: bool) {
    let mut s = state().lock().unwrap();
    if s.title != title {
        s.title = title.to_string();
    }
    if s.artist != artist {
        s.artist = artist.to_string();
    }
    s.playing = playing;
}

// ── HTTP ─────────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').find_map(|c| {
            let (k, v) = c.trim().split_once('=')?;
            (k == name).then_some(v)
        })
    }
    fn len(&self) -> u64 {
        self.header("content-length")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    }
}

/// The request line and headers (at most 64 lines of 4 KB). None = not HTTP.
pub fn read_head(r: &mut impl BufRead) -> Option<Request> {
    let mut line = String::new();
    r.by_ref().take(4096).read_line(&mut line).ok()?;
    let mut it = line.split_whitespace();
    let (method, path) = (it.next()?.to_string(), it.next()?.to_string());
    let mut req = Request {
        method,
        path,
        headers: Vec::new(),
    };
    for _ in 0..64 {
        line.clear();
        r.by_ref().take(4096).read_line(&mut line).ok()?;
        let l = line.trim_end();
        if l.is_empty() {
            return Some(req);
        }
        if let Some((k, v)) = l.split_once(':') {
            req.headers
                .push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    None
}

/// `%XX` and `+` decoding of one path segment or form value.
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
                {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file name an upload may use: no folders, no dots in front, no control characters, a known
/// audio extension, at most 180 bytes. None = refused.
pub fn upload_name(raw: &str) -> Option<String> {
    let n = url_decode(raw);
    let n = n.trim();
    if n.is_empty()
        || n.len() > 180
        || n.starts_with('.')
        || n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || n.chars().any(|c| c.is_control())
    {
        return None;
    }
    let ext = n.rsplit_once('.')?.1.to_ascii_lowercase();
    AUDIO_EXT.contains(&ext.as_str()).then(|| n.to_string())
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '<' => o.push_str("\\u003c"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn button(name: &str) -> Option<i32> {
    Some(match name {
        "play" => 7,
        "next" => 13,
        "prev" => 14,
        "volup" => 9,
        "voldown" => 10,
        _ => return None,
    })
}

fn reply(c: &mut TcpStream, status: &str, ctype: &str, extra: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         X-Frame-Options: DENY\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = c.write_all(head.as_bytes());
    let _ = c.write_all(body);
}

/// What the request may do: the code in its cookie, and for anything that acts, the X-Cinder header.
fn authed(req: &Request, code: &str) -> bool {
    !code.is_empty() && req.cookie("cinder") == Some(code)
}

fn serve(c: TcpStream) {
    let Ok(read_half) = c.try_clone() else {
        return;
    };
    let mut c = c;
    let mut r = BufReader::new(read_half);
    let Some(req) = read_head(&mut r) else {
        return;
    };
    let code = state().lock().unwrap().code.clone();
    let (path, query) = req.path.split_once('?').unwrap_or((req.path.as_str(), ""));
    let acting = req.header("x-cinder") == Some("1");
    match (req.method.as_str(), path) {
        ("GET", "/") if authed(&req, &code) => reply(
            &mut c,
            "200 OK",
            "text/html; charset=utf-8",
            "",
            PAGE.as_bytes(),
        ),
        ("GET", "/") => reply(
            &mut c,
            "200 OK",
            "text/html; charset=utf-8",
            "",
            LOGIN.as_bytes(),
        ),
        ("POST", "/login") => {
            let mut body = vec![0u8; req.len().min(256) as usize];
            let _ = r.read_exact(&mut body);
            let form = String::from_utf8_lossy(&body);
            let given = form
                .split('&')
                .find_map(|kv| kv.strip_prefix("code="))
                .map(url_decode)
                .unwrap_or_default();
            let mut s = state().lock().unwrap();
            if s.locked_until.is_some_and(|t| Instant::now() < t) {
                drop(s);
                return reply(
                    &mut c,
                    "429 Too Many Requests",
                    "text/plain",
                    "",
                    b"Wait a minute and try again.",
                );
            }
            if !s.code.is_empty() && given.trim() == s.code {
                s.fails = 0;
                drop(s);
                let cookie = format!("Set-Cookie: cinder={code}; Path=/; SameSite=Strict; HttpOnly\r\nLocation: /\r\n");
                reply(&mut c, "303 See Other", "text/plain", &cookie, b"");
            } else {
                s.fails += 1;
                if s.fails >= 5 {
                    s.fails = 0;
                    s.locked_until = Some(Instant::now() + Duration::from_secs(60));
                }
                drop(s);
                reply(
                    &mut c,
                    "303 See Other",
                    "text/plain",
                    "Location: /?wrong\r\n",
                    b"",
                );
            }
        }
        ("GET", "/api/now") if authed(&req, &code) => {
            let s = state().lock().unwrap();
            let j = format!(
                "{{\"title\":{},\"artist\":{},\"playing\":{}}}",
                json_str(&s.title),
                json_str(&s.artist),
                s.playing
            );
            drop(s);
            reply(&mut c, "200 OK", "application/json", "", j.as_bytes())
        }
        ("POST", p) if p.starts_with("/api/button/") && authed(&req, &code) && acting => {
            match button(&p[12..]) {
                Some(b) => {
                    let mut s = state().lock().unwrap();
                    if s.buttons.len() < 16 {
                        s.buttons.push_back(b);
                    }
                    drop(s);
                    reply(&mut c, "204 No Content", "text/plain", "", b"")
                }
                None => reply(&mut c, "404 Not Found", "text/plain", "", b"no such button"),
            }
        }
        ("PUT", p) if p.starts_with("/api/upload/") && authed(&req, &code) && acting => {
            let _ = query;
            match upload(&p[12..], req.len(), &mut r) {
                Ok(name) => reply(&mut c, "201 Created", "text/plain", "", name.as_bytes()),
                Err((st, why)) => reply(&mut c, st, "text/plain", "", why.as_bytes()),
            }
        }
        (_, "/api/now" | "/") | ("POST", _) | ("PUT", _) => reply(
            &mut c,
            "403 Forbidden",
            "text/plain",
            "",
            b"Enter the code from the player first.",
        ),
        _ => reply(&mut c, "404 Not Found", "text/plain", "", b"Not found"),
    }
}

/// Stream an upload to UPLOAD_DIR under a temporary name, then rename it into place. A name that is
/// taken gets " (2)", " (3)"… — an upload never replaces a file.
fn upload(raw: &str, len: u64, r: &mut impl Read) -> Result<String, (&'static str, String)> {
    let name = upload_name(raw).ok_or((
        "400 Bad Request",
        "Only music files, by name, can be sent".into(),
    ))?;
    if len == 0 || len > MAX_UPLOAD {
        return Err((
            "413 Payload Too Large",
            "That file is empty or too big".into(),
        ));
    }
    std::fs::create_dir_all(UPLOAD_DIR).map_err(|_| {
        (
            "500 Internal Server Error",
            "Cannot make the Uploads folder".into(),
        )
    })?;
    let tmp = format!("{UPLOAD_DIR}/.upload.part");
    let mut f = std::fs::File::create(&tmp).map_err(|_| {
        (
            "500 Internal Server Error",
            "Cannot write to the player".into(),
        )
    })?;
    let copied = std::io::copy(&mut r.take(len), &mut f);
    let ok = matches!(copied, Ok(n) if n == len) && f.sync_all().is_ok();
    drop(f);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err((
            "400 Bad Request",
            "The upload stopped part-way; nothing was kept".into(),
        ));
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
    let mut dest = format!("{UPLOAD_DIR}/{name}");
    for i in 2..1000 {
        if !std::path::Path::new(&dest).exists() {
            break;
        }
        dest = format!("{UPLOAD_DIR}/{stem} ({i}).{ext}");
    }
    std::fs::rename(&tmp, &dest)
        .map_err(|_| ("500 Internal Server Error", "Cannot save the file".into()))?;
    state().lock().unwrap().uploaded += 1;
    Ok(dest.rsplit('/').next().unwrap_or("").to_string())
}

const LOGIN: &str = r#"<!doctype html><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Cinder</title><style>body{font:18px system-ui,sans-serif;background:#111;color:#eee;display:grid;place-items:center;min-height:90vh;margin:0}
form{display:grid;gap:12px;width:min(320px,90vw)}input,button{font:inherit;padding:14px;border-radius:10px;border:1px solid #444;background:#1c1c1c;color:#eee}
button{background:#f26a21;border:0;color:#111;font-weight:600}p{color:#aaa;margin:0}</style>
<form method=post action=/login><h1>Cinder</h1><p>Enter the code shown on the player, in Settings ▸ Wi-Fi ▸ Phone remote.</p>
<input name=code inputmode=numeric autocomplete=off maxlength=6 autofocus aria-label="Code"><button>Open</button>
<p id=w></p></form><script>if(location.search=="?wrong")w.textContent="That code is not the one on the player."</script>"#;

const PAGE: &str = r#"<!doctype html><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Cinder</title><style>body{font:18px system-ui,sans-serif;background:#111;color:#eee;margin:0;padding:20px;max-width:480px;margin:auto}
h1{font-size:15px;color:#888;font-weight:500;margin:0 0 16px}#t{font-size:26px;font-weight:600}#a{color:#aaa;margin:4px 0 24px}
.row{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-bottom:10px}
button{font:inherit;font-size:22px;padding:18px 0;border-radius:12px;border:1px solid #333;background:#1c1c1c;color:#eee}
button:active{background:#2a2a2a}#p{background:#f26a21;color:#111;border:0}
section{margin-top:28px;border-top:1px solid #333;padding-top:20px}label{display:block;padding:18px;border:1px dashed #555;border-radius:12px;text-align:center;color:#aaa}
input[type=file]{display:none}#s{color:#aaa;font-size:15px;margin-top:10px;white-space:pre-line}</style>
<h1>Cinder</h1><div id=t>…</div><div id=a></div>
<div class=row><button onclick="b('prev')" aria-label="Previous">⏮</button><button id=p onclick="b('play')" aria-label="Play or pause">⏯</button><button onclick="b('next')" aria-label="Next">⏭</button></div>
<div class=row><button onclick="b('voldown')" aria-label="Volume down">−</button><span></span><button onclick="b('volup')" aria-label="Volume up">+</button></div>
<section><label>Send music to the player<input type=file multiple accept="audio/*,.flac,.dsf,.dff,.ape" onchange="up(this.files)"></label><div id=s></div></section>
<script>
const H={"X-Cinder":"1"};
function b(n){fetch("/api/button/"+n,{method:"POST",headers:H}).then(now)}
function now(){fetch("/api/now").then(r=>r.json()).then(j=>{t.textContent=j.title||"Nothing playing";a.textContent=j.artist})}
async function up(fs){for(const f of fs){s.textContent="Sending "+f.name+"…";
const r=await fetch("/api/upload/"+encodeURIComponent(f.name),{method:"PUT",headers:H,body:f});
s.textContent=(r.ok?"Sent: ":"Not sent: ")+(await r.text())}s.textContent+="\nThe library updates in a minute or two."}
now();setInterval(now,3000)</script>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_head_is_read_and_bounded() {
        let raw = b"PUT /api/upload/A%20b.flac HTTP/1.1\r\nHost: p\r\nCookie: x=1; cinder=123456\r\nContent-Length: 42\r\nX-Cinder: 1\r\n\r\nBODY";
        let mut r = BufReader::new(&raw[..]);
        let q = read_head(&mut r).unwrap();
        assert_eq!(
            (q.method.as_str(), q.path.as_str()),
            ("PUT", "/api/upload/A%20b.flac")
        );
        assert_eq!(q.cookie("cinder"), Some("123456"));
        assert_eq!(q.len(), 42);
        assert!(authed(&q, "123456"));
        assert!(!authed(&q, "654321"));
        assert!(!authed(&q, ""), "no code, no way in");
        let mut rest = String::new();
        r.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "BODY", "the body is left for the upload");
        let endless = vec![b'a'; 100_000];
        assert!(read_head(&mut BufReader::new(&endless[..])).is_none());
    }

    #[test]
    fn upload_names_are_music_files_and_nothing_else() {
        assert_eq!(
            upload_name("Song%20One.FLAC").as_deref(),
            Some("Song One.FLAC")
        );
        assert_eq!(
            upload_name("Björk%20-%20Jóga.mp3").as_deref(),
            Some("Björk - Jóga.mp3")
        );
        assert_eq!(
            upload_name("..%2F..%2Fetc%2Fpasswd.mp3"),
            None,
            "no folders"
        );
        assert_eq!(upload_name("a%2Fb.mp3"), None);
        assert_eq!(upload_name(".hidden.mp3"), None);
        assert_eq!(upload_name("evil.sh"), None, "not music");
        assert_eq!(upload_name("noext"), None);
        assert_eq!(upload_name("a%0Ab.mp3"), None, "no control characters");
        assert_eq!(upload_name(&"x".repeat(200)), None);
    }

    #[test]
    fn buttons_codes_and_json() {
        assert_eq!(button("play"), Some(7));
        assert_eq!(button("next"), Some(13));
        assert_eq!(
            button("power"),
            None,
            "the page cannot power the player off"
        );
        assert_eq!(url_decode("a+b%21%zz"), "a b!%zz");
        assert_eq!(json_str("<script>\"x\""), "\"\\u003cscript>\\\"x\\\"\"");
        let c = new_code();
        assert!(c.len() == 6 && c.chars().all(|d| d.is_ascii_digit()));
    }
}
