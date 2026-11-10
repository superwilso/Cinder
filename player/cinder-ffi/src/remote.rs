//! Settings ▸ Wi-Fi ▸ Phone remote: a page on the home network (`http://<player>:8080`) that does
//! most of what Flint does, from a phone: the song playing with its cover, the transport, the
//! library (search, albums, artists, playlists) to play now, play next or add to Up Next, Up Next
//! itself, likes, sending music to the player, and signing the player in to Last.fm and
//! ListenBrainz. Opt-in, off by default; once switched on it stays on and serves whenever Wi-Fi
//! is connected.
//!
//! PAIRED ONCE. A new 6-digit code is drawn each time the server starts and shown on the player's
//! Wi-Fi ▸ Phone remote page. A phone that enters it gets a random token in a cookie (a year,
//! SameSite=Strict, HttpOnly), kept in /data/cinder/remote_phones (never on the drive a PC sees),
//! and comes straight in from then on; "Forget phones" on the player clears them all. Five wrong
//! codes lock the login for a minute. Every request that does something also needs the `X-Cinder`
//! header, which a page on another site cannot send without a CORS preflight this server never
//! answers — so a link elsewhere cannot press Play or upload. Plain HTTP: anyone on the same
//! network who can read its traffic can read the token; the Wi-Fi password is what keeps them out.
//!
//! The server is a thread here, std only, one connection at a time. READS take the renderer's
//! lock briefly (`super::read`) and build JSON from the library the player already holds. WRITES
//! never touch the navigator from this thread: buttons ([`take_button`]) and everything else
//! ([`take_cmd`]) are queued, and the shell's frame loop carries them out through the same paths a
//! tap takes. Uploads finished are counted for a rescan ([`take_uploaded`]).

use cinder_ui::model::{Library, SongRow};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Where uploads land: a folder of their own in the player's music folder.
pub const UPLOAD_DIR: &str = "/contents/MUSIC/Uploads";
/// The audio files the player plays; anything else is refused before a byte is written.
const AUDIO_EXT: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "wav", "aif", "aiff", "wma", "dsf", "dff", "ape", "ogg", "opus",
];
const MAX_UPLOAD: u64 = 2 << 30; // 2 GB, the largest file vfat on the player is happy with
/// Paired phones, one token per line. /data: the PC never sees it, a rescan never touches it.
const PHONES: &str = "/data/cinder/remote_phones";
const MAX_PHONES: usize = 8;

/// What the page asks the player to do. The shell's frame loop carries each out on the render
/// thread (`cinder_remote_take_action`).
#[derive(Debug, PartialEq)]
pub enum Cmd {
    /// Play these tracks from row `.1`, in place of what was playing.
    Play(Vec<i64>, usize),
    /// Play next (`true`) or add to the end of Up Next.
    Queue(Vec<i64>, bool),
    /// Row `.0` of the playing list.
    Jump(usize),
    /// A playlist by id, shuffled or not.
    Playlist(i64, bool),
    /// Every song by artist `.0` (index into `Library::artists`), shuffled.
    ShuffleArtist(usize),
    /// Like or unlike the playing track.
    Like,
    /// An account was just linked: switch on Send plays.
    SendPlays,
}

#[derive(Default)]
struct Shared {
    code: String,
    buttons: VecDeque<i32>,
    cmds: VecDeque<Cmd>,
    uploaded: u32,
    fails: u32,
    locked_until: Option<Instant>,
    /// A Last.fm sign-in waiting for "allow": API key, secret, token.
    lastfm: Option<(String, String, String)>,
}

static RUN: AtomicBool = AtomicBool::new(false);
/// The server would not start (no free port): not tried again every frame until the switch is
/// flipped ([`retry`]).
static GAVE_UP: AtomicBool = AtomicBool::new(false);
/// Paired phones; `usize::MAX` until the file has been read once.
static PHONES_N: AtomicUsize = AtomicUsize::new(usize::MAX);
static STATE: OnceLock<Mutex<Shared>> = OnceLock::new();
fn state() -> &'static Mutex<Shared> {
    STATE.get_or_init(|| Mutex::new(Shared::default()))
}

/// `n` bytes from the kernel's random pool (no crate for it).
fn random(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b
}

/// A 6-digit code.
pub fn new_code() -> String {
    let b = random(4);
    format!(
        "{:06}",
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000
    )
}

// ── Paired phones ────────────────────────────────────────────────────────────────────────────────

fn phones_path() -> String {
    std::env::var("CINDER_REMOTE_PHONES").unwrap_or_else(|_| PHONES.to_string())
}

/// A token as [`pair`] makes them: 32 hex digits.
fn token_ok(t: &str) -> bool {
    t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

fn phones() -> Vec<String> {
    let list: Vec<String> = std::fs::read_to_string(phones_path())
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| token_ok(l))
        .map(String::from)
        .collect();
    PHONES_N.store(list.len(), Ordering::Relaxed);
    list
}

fn save_phones(list: &[String]) {
    use std::os::unix::fs::OpenOptionsExt;
    let path = phones_path();
    let tmp = format!("{path}.part");
    let body: String = list.iter().map(|t| format!("{t}\n")).collect();
    let ok = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(body.as_bytes()).and_then(|_| f.sync_all()))
        .and_then(|_| std::fs::rename(&tmp, &path));
    if ok.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    PHONES_N.store(list.len(), Ordering::Relaxed);
}

/// Remember one more phone; the oldest goes past [`MAX_PHONES`]. Returns its token.
fn pair() -> String {
    let token: String = random(16).iter().map(|b| format!("{b:02x}")).collect();
    let mut list = phones();
    list.push(token.clone());
    let over = list.len().saturating_sub(MAX_PHONES);
    list.drain(..over);
    save_phones(&list);
    token
}

/// Settings ▸ Wi-Fi ▸ Phone remote ▸ Forget phones.
pub fn forget_phones() {
    let _ = std::fs::remove_file(phones_path());
    PHONES_N.store(0, Ordering::Relaxed);
}

/// How many phones come in without the code.
pub fn phone_count() -> usize {
    match PHONES_N.load(Ordering::Relaxed) {
        usize::MAX => phones().len(),
        n => n,
    }
}

// ── The server ───────────────────────────────────────────────────────────────────────────────────

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
    let Some((l, port)) = listener else {
        GAVE_UP.store(true, Ordering::SeqCst);
        return Err("no free port".into());
    };
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    {
        let mut s = state().lock().unwrap();
        s.code = code.to_string();
        s.fails = 0;
        s.locked_until = None;
    }
    phones();
    RUN.store(true, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("cinder-remote".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            // Idle, it looks for a request four times a second; for a few seconds after one it
            // looks every 20 ms, so the page's next requests are not each held up to 250 ms.
            let mut last = Instant::now() - Duration::from_secs(60);
            while RUN.load(Ordering::SeqCst) {
                match l.accept() {
                    Ok((c, _)) => {
                        last = Instant::now();
                        let _ = c.set_nonblocking(false);
                        let _ = c.set_read_timeout(Some(Duration::from_secs(15)));
                        let _ = c.set_write_timeout(Some(Duration::from_secs(15)));
                        serve(c);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(
                        if last.elapsed() < Duration::from_secs(5) {
                            20
                        } else {
                            250
                        },
                    )),
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
    s.cmds.clear();
    s.lastfm = None;
}

pub fn running() -> bool {
    RUN.load(Ordering::SeqCst)
}

/// Did the last start fail? Then the frame loop leaves it until the switch is flipped.
pub fn gave_up() -> bool {
    GAVE_UP.load(Ordering::SeqCst)
}
pub fn retry() {
    GAVE_UP.store(false, Ordering::SeqCst);
}

/// The next button the page pressed (a `cinder_button_t`), for the shell's input path.
pub fn take_button() -> Option<i32> {
    state().lock().unwrap().buttons.pop_front()
}

/// The next thing the page asked for, for the render thread.
pub fn take_cmd() -> Option<Cmd> {
    state().lock().unwrap().cmds.pop_front()
}

fn push_cmd(c: Cmd) -> bool {
    let mut s = state().lock().unwrap();
    if s.cmds.len() >= 16 {
        return false;
    }
    s.cmds.push_back(c);
    true
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

use crate::net::json_str;

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
    let cache = if extra.contains("Cache-Control") {
        ""
    } else {
        "Cache-Control: no-store\r\n"
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{cache}\
         X-Frame-Options: DENY\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = c.write_all(head.as_bytes());
    let _ = c.write_all(body);
}

fn json(c: &mut TcpStream, body: &str) {
    reply(c, "200 OK", "application/json", "", body.as_bytes())
}
fn done(c: &mut TcpStream) {
    reply(c, "204 No Content", "text/plain", "", b"")
}
fn refuse(c: &mut TcpStream, status: &str, why: &str) {
    reply(c, status, "text/plain; charset=utf-8", "", why.as_bytes())
}

/// A paired phone: its cookie is one of the tokens on file.
fn authed(req: &Request, phones: &[String]) -> bool {
    req.cookie("cinder")
        .is_some_and(|t| token_ok(t) && phones.iter().any(|p| p == t))
}

/// `key=value` from a query string, decoded.
fn query_get(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| url_decode(v))
    })
}

/// The request body, up to `max` bytes, as text.
fn body_text(r: &mut impl Read, req: &Request, max: u64) -> String {
    let mut b = vec![0u8; req.len().min(max) as usize];
    let _ = r.read_exact(&mut b);
    String::from_utf8_lossy(&b).into_owned()
}

/// `1,2,3` → ids; anything that is not a number is dropped. At most 2,000.
fn ids_of(text: &str) -> Vec<i64> {
    text.split(',')
        .filter_map(|t| t.trim().parse().ok())
        .take(2000)
        .collect()
}

// ── JSON from the library the player holds ───────────────────────────────────────────────────────

/// The cover's art-cache key as the page asks for it (`/api/cover/<hex>`), or "" for none. The key
/// is the cover file's path hash, so a cached picture never belongs to another album after a rescan.
fn cover_of(keys: &HashMap<i64, u64>, album_id: i64) -> String {
    keys.get(&album_id)
        .map(|k| format!("{k:016x}"))
        .unwrap_or_default()
}

fn song_json(s: &SongRow) -> String {
    format!(
        "{{\"id\":{},\"t\":{},\"a\":{},\"d\":{}}}",
        s.object_id,
        json_str(&s.title),
        json_str(&s.artist),
        json_str(&s.dur)
    )
}

fn album_item(i: usize, a: &cinder_ui::model::AlbumRow, keys: &HashMap<i64, u64>) -> String {
    format!(
        "{{\"i\":{i},\"n\":{},\"a\":{},\"y\":{},\"k\":{},\"c\":\"{}\"}}",
        json_str(&a.name),
        json_str(&a.artist),
        json_str(&a.year),
        a.tracks,
        cover_of(keys, a.album_id)
    )
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
}

/// Every album, or one artist's (`artist` = index into `lib.artists`).
pub fn albums_json(lib: &Library, keys: &HashMap<i64, u64>, artist: Option<usize>) -> String {
    let name = artist
        .and_then(|i| lib.artists.get(i))
        .map(|a| a.name.as_str());
    list(
        lib.albums_flat()
            .iter()
            .enumerate()
            .filter(|(_, a)| name.is_none_or(|n| a.artist == n))
            .map(|(i, a)| album_item(i, a, keys)),
    )
}

pub fn artists_json(lib: &Library) -> String {
    list(lib.artists.iter().enumerate().map(|(i, a)| {
        format!(
            "{{\"i\":{i},\"n\":{},\"k\":{},\"t\":{}}}",
            json_str(&a.name),
            a.albums,
            a.tracks
        )
    }))
}

pub fn playlists_json(lib: &Library) -> String {
    list(lib.playlists.iter().map(|p| {
        format!(
            "{{\"id\":{},\"n\":{},\"t\":{}}}",
            p.id,
            json_str(&p.name),
            p.tracks
        )
    }))
}

/// Album `i` of `albums_flat()` with its tracks, or None.
pub fn album_json(lib: &Library, keys: &HashMap<i64, u64>, i: usize) -> Option<String> {
    let flat = lib.albums_flat();
    let a = flat.get(i)?;
    Some(format!(
        "{{\"album\":{},\"tracks\":{}}}",
        album_item(i, a, keys),
        list(a.track_list.iter().map(song_json))
    ))
}

/// Artists, albums and songs whose names hold `q` (any case), as the player's own Search finds
/// them: songs by title, artist or album name.
pub fn search_json(lib: &Library, keys: &HashMap<i64, u64>, q: &str) -> String {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return "{\"artists\":[],\"albums\":[],\"songs\":[]}".into();
    }
    let has = |s: &str| s.to_lowercase().contains(&q);
    let flat = lib.albums_flat();
    let album_hit: std::collections::HashSet<i64> = flat
        .iter()
        .filter(|a| has(&a.name))
        .map(|a| a.album_id)
        .collect();
    let artists = list(
        lib.artists
            .iter()
            .enumerate()
            .filter(|(_, a)| has(&a.name))
            .take(20)
            .map(|(i, a)| {
                format!(
                    "{{\"i\":{i},\"n\":{},\"k\":{},\"t\":{}}}",
                    json_str(&a.name),
                    a.albums,
                    a.tracks
                )
            }),
    );
    let albums = list(
        flat.iter()
            .enumerate()
            .filter(|(_, a)| album_hit.contains(&a.album_id))
            .take(50)
            .map(|(i, a)| album_item(i, a, keys)),
    );
    let songs = list(
        lib.songs
            .iter()
            .filter(|s| has(&s.title) || has(&s.artist) || album_hit.contains(&s.album_id))
            .take(150)
            .map(song_json),
    );
    format!("{{\"artists\":{artists},\"albums\":{albums},\"songs\":{songs}}}")
}

/// Up Next: the playing row and what comes after it (and the three before, for context).
pub fn queue_json(ctx: &[SongRow], at: usize) -> String {
    let from = at.saturating_sub(3).min(ctx.len());
    let to = (at + 200).min(ctx.len());
    format!(
        "{{\"at\":{at},\"from\":{from},\"rows\":{}}}",
        list(ctx[from..to].iter().map(song_json))
    )
}

fn now_json(r: &super::Render) -> String {
    let t = r.last_track.as_ref();
    let cover = t
        .and_then(|t| t.album_id)
        .map(|id| cover_of(&r.art_cache_keys, id))
        .unwrap_or_default();
    format!(
        "{{\"title\":{},\"artist\":{},\"album\":{},\"cover\":\"{cover}\",\"playing\":{},\"liked\":{},\"pos\":{},\"dur\":{},\"shuffle\":{},\"repeat\":{}}}",
        json_str(&r.np.title),
        json_str(&r.np.artist),
        json_str(t.map_or("", |t| t.album.as_str())),
        r.np.playing,
        r.np.liked,
        r.play_pos_ms.max(0),
        r.cur_duration_ms.max(0),
        r.np.shuffle,
        r.np.repeat
    )
}

fn accounts_json(send: bool, pending: bool) -> String {
    let lf = crate::net::lf_user().map_or("null".into(), |u| json_str(&u));
    let lb = std::path::Path::new(crate::net::LB_TOKEN).exists();
    format!("{{\"lastfm\":{lf},\"lb\":{lb},\"send\":{send},\"pending\":{pending}}}")
}

/// A 24-bit BMP of `img`, top row first: what a browser shows with no encoder on this side.
pub fn bmp(img: &cinder_ui::art::Image) -> Vec<u8> {
    let (w, h) = (img.w, img.h);
    let row = (w * 3 + 3) & !3;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(size as u32).to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(-(h as i32)).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]);
    for y in 0..h {
        for px in img.rgb[y * w * 3..(y + 1) * w * 3].as_chunks::<3>().0 {
            b.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        b.resize(b.len() + row - w * 3, 0);
    }
    b
}

// ── Requests ─────────────────────────────────────────────────────────────────────────────────────

fn serve(c: TcpStream) {
    let Ok(read_half) = c.try_clone() else {
        return;
    };
    let mut c = c;
    let mut r = BufReader::new(read_half);
    let Some(req) = read_head(&mut r) else {
        return;
    };
    let (path, query) = req.path.split_once('?').unwrap_or((req.path.as_str(), ""));
    let (path, query) = (path.to_string(), query.to_string());
    if req.method == "POST" && path == "/login" {
        return login(&mut c, &mut r, &req);
    }
    let ok = authed(&req, &phones());
    if req.method == "GET" && path == "/" {
        let page = if ok { PAGE } else { LOGIN };
        return reply(
            &mut c,
            "200 OK",
            "text/html; charset=utf-8",
            "",
            page.as_bytes(),
        );
    }
    if !ok {
        return refuse(
            &mut c,
            "403 Forbidden",
            "Enter the code from the player first.",
        );
    }
    if req.method == "GET" {
        return get(&mut c, &path, &query);
    }
    // Anything that acts needs the header a cross-site form cannot send.
    if req.header("x-cinder") != Some("1") {
        return refuse(&mut c, "403 Forbidden", "Not from this page.");
    }
    if req.method == "PUT" {
        if let Some(name) = path.strip_prefix("/api/upload/") {
            return match upload(name, req.len(), &mut r) {
                Ok(name) => reply(&mut c, "201 Created", "text/plain", "", name.as_bytes()),
                Err((st, why)) => refuse(&mut c, st, &why),
            };
        }
    }
    if req.method == "POST" {
        return post(&mut c, &mut r, &req, &path, &query);
    }
    refuse(&mut c, "404 Not Found", "Not found")
}

fn login(c: &mut TcpStream, r: &mut impl Read, req: &Request) {
    let form = body_text(r, req, 256);
    let given = form
        .split('&')
        .find_map(|kv| kv.strip_prefix("code="))
        .map(url_decode)
        .unwrap_or_default();
    let mut s = state().lock().unwrap();
    if s.locked_until.is_some_and(|t| Instant::now() < t) {
        drop(s);
        return refuse(c, "429 Too Many Requests", "Wait a minute and try again.");
    }
    if !s.code.is_empty() && given.trim() == s.code {
        s.fails = 0;
        drop(s);
        let token = pair();
        let cookie = format!(
            "Set-Cookie: cinder={token}; Path=/; Max-Age=34560000; SameSite=Strict; HttpOnly\r\nLocation: /\r\n"
        );
        reply(c, "303 See Other", "text/plain", &cookie, b"");
    } else {
        s.fails += 1;
        if s.fails >= 5 {
            s.fails = 0;
            s.locked_until = Some(Instant::now() + Duration::from_secs(60));
        }
        drop(s);
        reply(
            c,
            "303 See Other",
            "text/plain",
            "Location: /?wrong\r\n",
            b"",
        );
    }
}

fn get(c: &mut TcpStream, path: &str, query: &str) {
    if let Some(hex) = path.strip_prefix("/api/cover/") {
        let img = u64::from_str_radix(hex, 16)
            .ok()
            .and_then(crate::art_cache::load_cover);
        return match img {
            Some(img) => reply(
                c,
                "200 OK",
                "image/bmp",
                "Cache-Control: max-age=604800\r\n",
                &bmp(&img),
            ),
            None => refuse(c, "404 Not Found", "No cover"),
        };
    }
    let body = super::read(|r| {
        let lib = r.app.library();
        let keys = &r.art_cache_keys;
        Some(match path {
            "/api/now" => now_json(r),
            "/api/queue" => queue_json(r.app.context(), r.app.context_idx()),
            "/api/search" => search_json(lib, keys, &query_get(query, "q").unwrap_or_default()),
            "/api/albums" => albums_json(
                lib,
                keys,
                query_get(query, "artist").and_then(|a| a.parse().ok()),
            ),
            "/api/artists" => artists_json(lib),
            "/api/playlists" => playlists_json(lib),
            "/api/accounts" => accounts_json(
                r.app.opt(cinder_ui::opt::Opt::WifiLb),
                state().lock().unwrap().lastfm.is_some(),
            ),
            p => {
                let i = p.strip_prefix("/api/album/")?.parse().ok()?;
                album_json(lib, keys, i)?
            }
        })
    });
    match body {
        Some(b) => json(c, &b),
        None => refuse(c, "404 Not Found", "Not found"),
    }
}

fn post(c: &mut TcpStream, r: &mut impl Read, req: &Request, path: &str, query: &str) {
    let n = |p: &str| p.parse::<usize>().ok();
    let cmd = match path {
        "/api/play" => Some(Cmd::Play(
            ids_of(&body_text(r, req, 64 * 1024)),
            query_get(query, "at").and_then(|a| n(&a)).unwrap_or(0),
        )),
        "/api/queue" => Some(Cmd::Queue(
            ids_of(&body_text(r, req, 64 * 1024)),
            query_get(query, "next").is_some_and(|v| v == "1"),
        )),
        "/api/like" => Some(Cmd::Like),
        p if p.starts_with("/api/jump/") => n(&p[10..]).map(Cmd::Jump),
        p if p.starts_with("/api/artist/") => n(&p[12..]).map(Cmd::ShuffleArtist),
        p if p.starts_with("/api/playlist/") => p[14..]
            .parse()
            .ok()
            .map(|id| Cmd::Playlist(id, query_get(query, "shuffle").is_some_and(|v| v == "1"))),
        _ => None,
    };
    if let Some(cmd) = cmd {
        return if push_cmd(cmd) {
            done(c)
        } else {
            refuse(
                c,
                "503 Service Unavailable",
                "The player is busy; try again.",
            )
        };
    }
    if let Some(name) = path.strip_prefix("/api/button/") {
        return match button(name) {
            Some(b) => {
                let mut s = state().lock().unwrap();
                if s.buttons.len() < 16 {
                    s.buttons.push_back(b);
                }
                drop(s);
                done(c)
            }
            None => refuse(c, "404 Not Found", "no such button"),
        };
    }
    match account(path, &body_text(r, req, 512)) {
        Ok(Some(j)) => json(c, &j),
        Ok(None) => done(c),
        Err(why) => refuse(c, "400 Bad Request", &why),
    }
}

/// Last.fm and ListenBrainz: link and unlink. Ok(Some(json)) for an answer, Ok(None) for done.
fn account(path: &str, body: &str) -> Result<Option<String>, String> {
    use crate::net;
    let wrote = |p: &str, text: &str| {
        std::fs::write(p, text)
            .map_err(|_| "Could not write to the player (is it in USB storage?)".to_string())
    };
    match path {
        "/api/lb" => {
            let t = net::clean_token(body).ok_or("That is not a ListenBrainz token")?;
            wrote(net::LB_TOKEN, &format!("{t}\n"))?;
            push_cmd(Cmd::SendPlays);
            Ok(None)
        }
        "/api/lb/unlink" => {
            let _ = std::fs::remove_file(net::LB_TOKEN);
            Ok(None)
        }
        "/api/lastfm/start" => {
            let mut it = body.lines().map(str::trim);
            let (key, secret) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            if !net::lf_key_ok(key) || !net::lf_key_ok(secret) {
                return Err("The API key and the secret are 32 letters and digits each".into());
            }
            let (token, url) = net::lf_token(key, secret)?;
            state().lock().unwrap().lastfm = Some((key.into(), secret.into(), token));
            Ok(Some(format!("{{\"url\":{}}}", json_str(&url))))
        }
        "/api/lastfm/finish" => {
            let (key, secret, token) = state()
                .lock()
                .unwrap()
                .lastfm
                .clone()
                .ok_or("Start again: no sign-in is waiting")?;
            let (sk, user) = net::lf_session(&key, &secret, &token)?;
            wrote(net::LF_CREDS, &net::lf_conf(&key, &secret, &sk, &user))?;
            state().lock().unwrap().lastfm = None;
            push_cmd(Cmd::SendPlays);
            Ok(Some(format!("{{\"user\":{}}}", json_str(&user))))
        }
        "/api/lastfm/unlink" => {
            let _ = std::fs::remove_file(net::LF_CREDS);
            Ok(None)
        }
        _ => Err("Not found".into()),
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
<form method=post action=/login><h1>Cinder</h1><p>Enter the code shown on the player, in Settings ▸ Wi-Fi ▸ Phone remote. This phone is remembered after that.</p>
<input name=code inputmode=numeric autocomplete=off maxlength=6 autofocus aria-label="Code"><button>Open</button>
<p id=w></p></form><script>if(location.search=="?wrong")w.textContent="That code is not the one on the player."</script>"#;

const PAGE: &str = include_str!("remote.html");

/// Tests that start the server or point it at a phones file take this first: both are global.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

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
        assert!(!authed(&q, &["123456".into()]), "the code is not a token");
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
        assert_eq!(ids_of("1, 2,x,-3"), vec![1, 2, -3]);
        assert_eq!(query_get("q=a+b%26c&at=2", "q").as_deref(), Some("a b&c"));
        assert_eq!(query_get("next", "next").as_deref(), Some(""));
    }

    /// A phone pairs once: its token gets in, a made-up one does not, Forget clears them all, and
    /// only the newest eight are kept.
    #[test]
    fn phones_pair_and_forget() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cinder-phones-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CINDER_REMOTE_PHONES", dir.join("phones"));
        let t = pair();
        assert!(token_ok(&t));
        let req = |cookie: &str| Request {
            headers: vec![("Cookie".into(), format!("cinder={cookie}"))],
            ..Default::default()
        };
        assert!(authed(&req(&t), &phones()));
        assert!(!authed(&req(&"0".repeat(32)), &phones()));
        assert_eq!(phone_count(), 1);
        for _ in 0..10 {
            pair();
        }
        assert_eq!(phones().len(), MAX_PHONES);
        assert!(!authed(&req(&t), &phones()), "the oldest went");
        forget_phones();
        assert_eq!((phones().len(), phone_count()), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn library_json_for_the_page() {
        let lib = Library::sample();
        let keys: HashMap<i64, u64> = lib
            .albums_flat()
            .first()
            .map(|a| (a.album_id, 0xabc))
            .into_iter()
            .collect();
        let all = albums_json(&lib, &keys, None);
        assert!(all.starts_with("[{\"i\":0,") && all.contains("\"c\":\"0000000000000abc\""));
        let a0 = &lib.artists[0].name;
        let mine = albums_json(&lib, &keys, Some(0));
        assert!(mine.matches("\"i\":").count() >= 1);
        assert!(!mine.contains("\"a\":") || mine.contains(&json_str(a0)));
        let first = &lib.songs[0];
        let found = search_json(&lib, &keys, &first.title.to_uppercase());
        assert!(
            found.contains(&format!("\"id\":{}", first.object_id)),
            "{found}"
        );
        assert_eq!(
            search_json(&lib, &keys, "  "),
            "{\"artists\":[],\"albums\":[],\"songs\":[]}"
        );
        assert!(album_json(&lib, &keys, 0).unwrap().contains("\"tracks\":["));
        assert!(album_json(&lib, &keys, 99_999).is_none());
        let q = queue_json(&lib.songs[..5], 4);
        assert!(q.starts_with("{\"at\":4,\"from\":1,"));
        assert_eq!(queue_json(&[], 0), "{\"at\":0,\"from\":0,\"rows\":[]}");
    }

    #[test]
    fn covers_are_bmps() {
        let img = cinder_ui::art::Image {
            w: 3,
            h: 2,
            rgb: (0..18).collect(),
        };
        let b = bmp(&img);
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 54 + 12 * 2, "rows padded to 4 bytes");
        assert_eq!(&b[54..57], &[2, 1, 0], "BGR");
    }
}
