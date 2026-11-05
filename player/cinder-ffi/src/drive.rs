//! Settings ▸ Wi-Fi ▸ Options ▸ Share the drive: the player's drive as a WebDAV share, so a
//! computer maps it as a drive letter (Windows: Map network drive, `http://<player>:8081/<code>`;
//! macOS: Finder ▸ Connect to Server; Linux: davfs2 or the file manager) and copies music, or syncs
//! with Flint (`flint sync Z:\MUSIC`), over Wi-Fi as it would over the cable.
//!
//! OFF AT EACH START: the switch is on until the player restarts, like the phone remote, because
//! anyone who has the address can write to the drive. The address carries an 8-character code
//! (saved, so a computer's mapping keeps working the next time); ten wrong codes in a minute lock
//! the share for a minute. Plain HTTP: the Wi-Fi password is what keeps strangers off the network.
//!
//! WebDAV class 1 and 2 (RFC 4918) as Windows' WebClient and macOS's Finder use it: OPTIONS,
//! PROPFIND (depth 0 and 1, with the drive's free space, RFC 4331), GET/HEAD with ranges, PUT
//! (into a hidden part file, renamed into place when complete, so a dropped connection never
//! leaves half a song), DELETE, MKCOL, MOVE, COPY of a file, PROPPATCH (Windows' file times; the
//! modified time is applied) and LOCK/UNLOCK (granted, not enforced: one user's computer). Each
//! connection has its own thread, 32 at most, and holds no file between requests, because USB
//! storage mode cannot unmount `/contents` while anything has a file open there; while it is
//! unmounted every request is refused, so nothing lands on the player's own filesystem.
//!
//! Finished writes count as uploads ([`crate::remote::note_uploaded`]): the library rescans once
//! they have stopped for 20 s, as after the phone remote.

use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::remote::{read_head, Request};

/// Windows' WebClient keeps a pool of idle connections open; each holds a thread until it goes quiet.
const MAX_CONNECTIONS: usize = 32;
const PART: &str = ".cinder-part";

#[derive(Default)]
struct Shared {
    root: PathBuf,
    code: String,
    /// Refuse every request unless `root` is a mount of its own (USB storage mode unmounts it).
    check_mount: bool,
    fails: u32,
    fails_since: Option<Instant>,
    locked_until: Option<Instant>,
}

static RUN: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static STATE: OnceLock<Mutex<Shared>> = OnceLock::new();
fn state() -> &'static Mutex<Shared> {
    STATE.get_or_init(|| Mutex::new(Shared::default()))
}

pub fn running() -> bool {
    RUN.load(Ordering::SeqCst)
}

/// A new code: 8 characters from an alphabet with no look-alikes (no 0/o, 1/l/i).
pub fn new_code() -> String {
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut b = [0u8; 8];
    let _ = fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b.iter().map(|x| A[*x as usize % A.len()] as char).collect()
}

/// Share `root` under `/<code>/`. Returns the port.
pub fn start(root: &Path, code: &str, check_mount: bool) -> Result<u16, String> {
    if running() {
        stop();
        std::thread::sleep(Duration::from_millis(600));
    }
    let mut bound = None;
    for port in 8081..8090u16 {
        if let Ok(l) = TcpListener::bind(("0.0.0.0", port)) {
            bound = Some((l, port));
            break;
        }
    }
    let (l, port) = bound.ok_or("no free port")?;
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    *state().lock().unwrap() = Shared {
        root: root.to_path_buf(),
        code: code.to_string(),
        check_mount,
        ..Shared::default()
    };
    RUN.store(true, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("cinder-drive".into())
        .stack_size(128 * 1024)
        .spawn(move || {
            while running() {
                match l.accept() {
                    Ok((mut c, _)) => {
                        if ACTIVE.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                            reply(&mut c, "503 Service Unavailable", &[], b"");
                            continue;
                        }
                        ACTIVE.fetch_add(1, Ordering::SeqCst);
                        let spawned = std::thread::Builder::new()
                            .name("cinder-drive-c".into())
                            .stack_size(256 * 1024)
                            .spawn(move || {
                                connection(c);
                                ACTIVE.fetch_sub(1, Ordering::SeqCst);
                            });
                        if spawned.is_err() {
                            ACTIVE.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(port)
}

pub fn stop() {
    RUN.store(false, Ordering::SeqCst);
}

// ── Paths ─────────────────────────────────────────────────────────────────────────────────────────

/// `%XX` decoding of a URL path (no `+` for space, unlike a form). None when it is not UTF-8.
pub fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Every byte outside the unreserved set and `/` as `%XX`, for hrefs.
pub fn pct_encode(s: &str) -> String {
    let mut o = String::new();
    for &c in s.as_bytes() {
        if c.is_ascii_alphanumeric() || b"-._~/".contains(&c) {
            o.push(c as char);
        } else {
            o.push_str(&format!("%{c:02X}"));
        }
    }
    o
}

/// The share-relative path a request names: `/<code>/a/b` → `a/b` (segments checked: no `..`, no
/// backslash, no NUL). Err(true) = the code is wrong; Err(false) = a bad path.
pub fn relative(url_path: &str, code: &str) -> Result<String, bool> {
    // A Destination header is a full URL.
    let p = match url_path.find("://") {
        Some(i) => url_path[i + 3..]
            .find('/')
            .map(|j| &url_path[i + 3 + j..])
            .unwrap_or("/"),
        None => url_path,
    };
    let p = p.split(['?', '#']).next().unwrap_or("");
    let rest = p.strip_prefix('/').unwrap_or(p);
    let (first, rest) = rest.split_once('/').unwrap_or((rest, ""));
    if first != code {
        return Err(true);
    }
    let rest = pct_decode(rest).ok_or(false)?;
    let mut segs = Vec::new();
    for s in rest.split('/') {
        match s {
            "" | "." => {}
            ".." => return Err(false),
            s if s.contains(['\\', '\0']) || s.ends_with(PART) => return Err(false),
            s => segs.push(s),
        }
    }
    Ok(segs.join("/"))
}

// ── Dates ─────────────────────────────────────────────────────────────────────────────────────────

fn secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// RFC 1123, as `getlastmodified` and HTTP's Last-Modified want it.
pub fn http_date(s: i64) -> String {
    let days = s.div_euclid(86400);
    let t = s.rem_euclid(86400);
    let (y, m, d) = cinder_ui::clockset::civil_from_days(days);
    const DAY: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {d:02} {} {y} {:02}:{:02}:{:02} GMT",
        DAY[days.rem_euclid(7) as usize],
        MON[(m - 1) as usize],
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

fn iso_date(s: i64) -> String {
    let (y, m, d) = cinder_ui::clockset::civil_from_days(s.div_euclid(86400));
    let t = s.rem_euclid(86400);
    format!(
        "{y}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

/// The inverse of [`http_date`] (Windows sends Win32LastModifiedTime that way).
pub fn parse_http_date(v: &str) -> Option<i64> {
    let mut it = v.split_whitespace().skip(1);
    let d: i32 = it.next()?.parse().ok()?;
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mon = it.next()?;
    let m = MON.iter().position(|x| *x == mon)? as i32 + 1;
    let y: i32 = it.next()?.parse().ok()?;
    let mut hms = it.next()?.split(':').map(|x| x.parse::<i64>().ok());
    let (h, mi, s) = (hms.next()??, hms.next()??, hms.next()??);
    Some(cinder_ui::clockset::days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + s)
}

// ── HTTP ──────────────────────────────────────────────────────────────────────────────────────────

fn reply(c: &mut TcpStream, status: &str, headers: &[(&str, String)], body: &[u8]) {
    let mut h = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    let _ = c.write_all(h.as_bytes());
    let _ = c.write_all(body);
}

/// The request's body: Content-Length bytes, or chunked (macOS sends PUTs that way), into `sink`.
/// Returns the bytes taken, or None when the connection broke or the body is malformed.
fn body(req: &Request, r: &mut impl BufRead, sink: &mut impl Write) -> Option<u64> {
    if req
        .header("transfer-encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        let mut total = 0;
        loop {
            let mut line = String::new();
            r.by_ref().take(256).read_line(&mut line).ok()?;
            let n = u64::from_str_radix(line.trim().split(';').next()?.trim(), 16).ok()?;
            if n == 0 {
                // Trailers, to the blank line.
                loop {
                    line.clear();
                    r.by_ref().take(4096).read_line(&mut line).ok()?;
                    if line.trim().is_empty() {
                        return Some(total);
                    }
                }
            }
            if std::io::copy(&mut r.by_ref().take(n), sink).ok()? != n {
                return None;
            }
            total += n;
            line.clear();
            r.by_ref().take(4).read_line(&mut line).ok()?;
        }
    }
    let n: u64 = req
        .header("content-length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    (std::io::copy(&mut r.by_ref().take(n), sink).ok()? == n).then_some(n)
}

fn body_text(req: &Request, r: &mut impl BufRead) -> String {
    let mut v = Vec::new();
    let _ = body(req, r, &mut Limit(&mut v, 256 * 1024));
    String::from_utf8_lossy(&v).into_owned()
}

/// A Write that stops taking bytes past a limit (a PROPFIND body has no business being large).
struct Limit<'a>(&'a mut Vec<u8>, usize);
impl Write for Limit<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let room = self.1.saturating_sub(self.0.len());
        self.0.extend_from_slice(&b[..b.len().min(room)]);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn connection(c: TcpStream) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = c.set_write_timeout(Some(Duration::from_secs(60)));
    let Ok(rc) = c.try_clone() else {
        return;
    };
    let mut r = BufReader::with_capacity(64 * 1024, rc);
    let mut c = c;
    // One request per connection, every answer saying `Connection: close`: Windows' WebClient
    // opens a new connection for most requests anyway and leaves the old ones idle, so kept-alive
    // connections only pile up until the cap refuses it (seen 11-05, Flint over a mapped share).
    if let Some(req) = read_head(&mut r) {
        handle(&req, &mut r, &mut c);
    }
}

/// One request. False when the connection must close (its body was not read to the end).
fn handle(req: &Request, r: &mut impl BufRead, c: &mut TcpStream) -> bool {
    const DAV: [(&str, &str); 2] = [("DAV", "1, 2"), ("MS-Author-Via", "DAV")];
    let dav: Vec<(&str, String)> = DAV.iter().map(|(k, v)| (*k, v.to_string())).collect();
    if req.method == "OPTIONS" {
        let mut h = dav.clone();
        h.push((
            "Allow",
            "OPTIONS, GET, HEAD, PUT, DELETE, PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, LOCK, UNLOCK"
                .into(),
        ));
        reply(c, "200 OK", &h, b"");
        return discard(req, r);
    }
    let (root, code, check_mount, locked) = {
        let s = state().lock().unwrap();
        (
            s.root.clone(),
            s.code.clone(),
            s.check_mount,
            s.locked_until.is_some_and(|t| Instant::now() < t),
        )
    };
    if locked {
        reply(
            c,
            "429 Too Many Requests",
            &[("Retry-After", "60".into())],
            b"",
        );
        return discard(req, r);
    }
    if check_mount && !mounted(&root) {
        reply(
            c,
            "503 Service Unavailable",
            &[("Retry-After", "30".into())],
            b"The drive is on USB storage",
        );
        return discard(req, r);
    }
    let rel = match relative(&req.path, &code) {
        Ok(rel) => rel,
        Err(wrong_code) => {
            // "/" itself (a client looking for the share) is not a wrong guess.
            if wrong_code && !req.path.trim_end_matches('/').is_empty() {
                wrong(&code);
            }
            reply(
                c,
                if wrong_code {
                    "404 Not Found"
                } else {
                    "400 Bad Request"
                },
                &[],
                b"",
            );
            return discard(req, r);
        }
    };
    let path = if rel.is_empty() {
        root.clone()
    } else {
        root.join(&rel)
    };
    let href = |rel: &str, dir: bool| {
        let mut h = format!("/{code}/{}", pct_encode(rel));
        if dir && !h.ends_with('/') {
            h.push('/');
        }
        h
    };
    match req.method.as_str() {
        "GET" | "HEAD" => {
            if !discard(req, r) {
                return false;
            }
            get(req, c, &path);
            true
        }
        "PROPFIND" => {
            let _ = body_text(req, r);
            let depth1 = req.header("depth").map(|d| d.trim() != "0").unwrap_or(true);
            match propfind(&root, &rel, &path, depth1, &href) {
                Some(xml) => reply(c, "207 Multi-Status", &xml_type(&dav), xml.as_bytes()),
                None => reply(c, "404 Not Found", &[], b""),
            }
            true
        }
        "PROPPATCH" => {
            let text = body_text(req, r);
            if !path.exists() {
                reply(c, "404 Not Found", &[], b"");
                return true;
            }
            if let Some(t) = win32_mtime(&text) {
                if let Ok(f) = fs::OpenOptions::new().write(true).open(&path) {
                    let _ = f.set_modified(UNIX_EPOCH + Duration::from_secs(t.max(0) as u64));
                }
            }
            let xml = proppatch_answer(&text, &href(&rel, path.is_dir()));
            reply(c, "207 Multi-Status", &xml_type(&dav), xml.as_bytes());
            true
        }
        "PUT" => put(req, r, c, &root, &path),
        "DELETE" => {
            if !discard(req, r) {
                return false;
            }
            let res = if rel.is_empty() {
                reply(c, "403 Forbidden", &[], b"");
                return true;
            } else if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            match res {
                Ok(()) => {
                    crate::remote::note_uploaded(1);
                    reply(c, "204 No Content", &[], b"")
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    reply(c, "404 Not Found", &[], b"")
                }
                Err(_) => reply(c, "403 Forbidden", &[], b""),
            }
            true
        }
        "MKCOL" => {
            let n = body(req, r, &mut std::io::sink());
            if n.is_none() {
                return false;
            }
            let st = if n != Some(0) {
                "415 Unsupported Media Type"
            } else if path.exists() {
                "405 Method Not Allowed"
            } else if !path.parent().is_some_and(Path::is_dir) {
                "409 Conflict"
            } else if fs::create_dir(&path).is_ok() {
                crate::remote::note_uploaded(1);
                "201 Created"
            } else {
                "403 Forbidden"
            };
            reply(c, st, &[], b"");
            true
        }
        "MOVE" | "COPY" => {
            if !discard(req, r) {
                return false;
            }
            let dest = req.header("destination").map(|d| relative(d, &code));
            let Some(Ok(drel)) = dest else {
                reply(c, "400 Bad Request", &[], b"");
                return true;
            };
            let dpath = if drel.is_empty() {
                root.clone()
            } else {
                root.join(&drel)
            };
            let overwrite = !req
                .header("overwrite")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("F"));
            reply(
                c,
                move_or_copy(
                    req.method == "MOVE",
                    &path,
                    &dpath,
                    overwrite,
                    rel.is_empty(),
                ),
                &[],
                b"",
            );
            true
        }
        "LOCK" => {
            let _ = body_text(req, r);
            let mut st = "200 OK";
            if !path.exists() {
                // A lock on a name that is not there yet makes it, empty (RFC 4918 §9.10.4).
                if !path.parent().is_some_and(Path::is_dir) || fs::File::create(&path).is_err() {
                    reply(c, "409 Conflict", &[], b"");
                    return true;
                }
                st = "201 Created";
            }
            let token = format!("opaquelocktoken:{}-{}", new_code(), new_code());
            let xml = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:prop xmlns:D=\"DAV:\"><D:lockdiscovery><D:activelock>\
                 <D:locktype><D:write/></D:locktype><D:lockscope><D:exclusive/></D:lockscope><D:depth>0</D:depth>\
                 <D:timeout>Second-3600</D:timeout><D:locktoken><D:href>{token}</D:href></D:locktoken>\
                 <D:lockroot><D:href>{}</D:href></D:lockroot></D:activelock></D:lockdiscovery></D:prop>",
                href(&rel, path.is_dir())
            );
            let mut h = xml_type(&dav);
            h.push(("Lock-Token", format!("<{token}>")));
            reply(c, st, &h, xml.as_bytes());
            true
        }
        "UNLOCK" => {
            reply(c, "204 No Content", &[], b"");
            discard(req, r)
        }
        _ => {
            reply(c, "405 Method Not Allowed", &[], b"");
            discard(req, r)
        }
    }
}

fn xml_type(dav: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    let mut h = dav.to_vec();
    h.push(("Content-Type", "application/xml; charset=\"utf-8\"".into()));
    h
}

/// Read and drop a body the method does not use. False when that failed.
fn discard(req: &Request, r: &mut impl BufRead) -> bool {
    body(req, r, &mut std::io::sink()).is_some()
}

fn wrong(code: &str) {
    let mut s = state().lock().unwrap();
    if s.code != code {
        return;
    }
    if !s
        .fails_since
        .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
    {
        s.fails = 0;
        s.fails_since = Some(Instant::now());
    }
    s.fails += 1;
    if s.fails >= 10 {
        s.locked_until = Some(Instant::now() + Duration::from_secs(60));
        s.fails = 0;
        s.fails_since = None;
    }
}

#[cfg(unix)]
fn mounted(root: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(root), root.parent().map(fs::metadata)) {
        (Ok(a), Some(Ok(b))) => a.dev() != b.dev(),
        _ => false,
    }
}
#[cfg(not(unix))]
fn mounted(_: &Path) -> bool {
    true
}

/// Free and used bytes of the filesystem `root` is on.
#[cfg(unix)]
fn space(root: &Path) -> Option<(u64, u64)> {
    let p = std::ffi::CString::new(root.as_os_str().to_string_lossy().as_bytes()).ok()?;
    let mut v: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(p.as_ptr(), &mut v) } != 0 {
        return None;
    }
    let f = v.f_frsize as u64;
    Some((
        v.f_bavail as u64 * f,
        (v.f_blocks as u64 - v.f_bfree as u64) * f,
    ))
}
#[cfg(not(unix))]
fn space(_: &Path) -> Option<(u64, u64)> {
    None
}

fn get(req: &Request, c: &mut TcpStream, path: &Path) {
    let Ok(mut f) = fs::File::open(path) else {
        reply(c, "404 Not Found", &[], b"");
        return;
    };
    let Ok(md) = f.metadata() else {
        reply(c, "404 Not Found", &[], b"");
        return;
    };
    if md.is_dir() {
        reply(c, "403 Forbidden", &[], b"");
        return;
    }
    let len = md.len();
    let ctype = crate::cast::mime(&path.to_string_lossy()).unwrap_or("application/octet-stream");
    let modified = md.modified().map(secs).unwrap_or(0);
    let (status, start, end) = match crate::cast::range(req.header("range"), len) {
        Ok(None) => ("200 OK", 0, len.saturating_sub(1)),
        Ok(Some((a, b))) => ("206 Partial Content", a, b),
        Err(()) => {
            reply(
                c,
                "416 Range Not Satisfiable",
                &[("Content-Range", format!("bytes */{len}"))],
                b"",
            );
            return;
        }
    };
    let n = if len == 0 { 0 } else { end - start + 1 };
    let mut h = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {n}\r\nAccept-Ranges: bytes\r\n\
         Last-Modified: {}\r\nETag: \"{len:x}-{modified:x}\"\r\nConnection: close\r\n",
        http_date(modified)
    );
    if status.starts_with("206") {
        h.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    h.push_str("\r\n");
    if c.write_all(h.as_bytes()).is_err() || req.method == "HEAD" {
        return;
    }
    if f.seek(SeekFrom::Start(start)).is_ok() {
        let _ = std::io::copy(&mut f.take(n), c);
    }
}

fn put(req: &Request, r: &mut impl BufRead, c: &mut TcpStream, root: &Path, path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        reply(c, "409 Conflict", &[], b"");
        return discard(req, r);
    };
    if path.is_dir() || !dir.is_dir() {
        reply(
            c,
            if path.is_dir() {
                "405 Method Not Allowed"
            } else {
                "409 Conflict"
            },
            &[],
            b"",
        );
        return discard(req, r);
    }
    let want: u64 = req
        .header("content-length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    if want > 0 && space(root).is_some_and(|(free, _)| want > free) {
        reply(c, "507 Insufficient Storage", &[], b"");
        return discard(req, r);
    }
    let part = dir.join(format!(".{}{PART}", name.to_string_lossy()));
    let existed = path.exists();
    let got = match fs::File::create(&part) {
        Ok(mut f) => body(req, r, &mut f).filter(|_| f.flush().is_ok()),
        Err(_) => {
            reply(c, "403 Forbidden", &[], b"");
            return discard(req, r);
        }
    };
    if got.is_none() {
        let _ = fs::remove_file(&part);
        return false; // the connection broke mid-file: nothing to answer
    }
    if fs::rename(&part, path).is_err() {
        let _ = fs::remove_file(&part);
        reply(c, "500 Internal Server Error", &[], b"");
        return true;
    }
    crate::remote::note_uploaded(1);
    reply(
        c,
        if existed {
            "204 No Content"
        } else {
            "201 Created"
        },
        &[],
        b"",
    );
    true
}

fn move_or_copy(
    is_move: bool,
    from: &Path,
    to: &Path,
    overwrite: bool,
    from_root: bool,
) -> &'static str {
    if from_root || !from.exists() {
        return if from_root {
            "403 Forbidden"
        } else {
            "404 Not Found"
        };
    }
    if from == to {
        return "403 Forbidden";
    }
    if !to.parent().is_some_and(Path::is_dir) {
        return "409 Conflict";
    }
    let existed = to.exists();
    if existed {
        if !overwrite {
            return "412 Precondition Failed";
        }
        let gone = if to.is_dir() {
            fs::remove_dir_all(to)
        } else {
            fs::remove_file(to)
        };
        if gone.is_err() {
            return "403 Forbidden";
        }
    }
    let done = if is_move {
        fs::rename(from, to).is_ok()
    } else if from.is_dir() {
        return "501 Not Implemented"; // ponytail: folder COPY; Explorer and Flint copy files
    } else {
        let part = to.with_file_name(format!(
            ".{}{PART}",
            to.file_name().unwrap_or_default().to_string_lossy()
        ));
        let ok = fs::copy(from, &part).is_ok() && fs::rename(&part, to).is_ok();
        if !ok {
            let _ = fs::remove_file(&part);
        }
        ok
    };
    if !done {
        return "403 Forbidden";
    }
    crate::remote::note_uploaded(1);
    if existed {
        "204 No Content"
    } else {
        "201 Created"
    }
}

fn escape(s: &str) -> String {
    crate::cast::escape(s)
}

/// The 207 for a PROPFIND: `rel` itself and, at depth 1, what a folder holds (part files left out).
fn propfind(
    root: &Path,
    rel: &str,
    path: &Path,
    depth1: bool,
    href: &dyn Fn(&str, bool) -> String,
) -> Option<String> {
    let md = fs::metadata(path).ok()?;
    let quota = space(root);
    let entry = |rel: &str, md: &fs::Metadata| {
        let dir = md.is_dir();
        let name = rel.rsplit('/').next().unwrap_or("");
        let modified = md.modified().map(secs).unwrap_or(0);
        let mut p = format!(
            "<D:response><D:href>{}</D:href><D:propstat><D:prop><D:displayname>{}</D:displayname>\
             <D:getlastmodified>{}</D:getlastmodified><D:creationdate>{}</D:creationdate>",
            escape(&href(rel, dir)),
            escape(name),
            http_date(modified),
            iso_date(modified)
        );
        if dir {
            p.push_str("<D:resourcetype><D:collection/></D:resourcetype>");
            if let Some((free, used)) = quota {
                p.push_str(&format!(
                    "<D:quota-available-bytes>{free}</D:quota-available-bytes><D:quota-used-bytes>{used}</D:quota-used-bytes>"
                ));
            }
        } else {
            let ctype = crate::cast::mime(name).unwrap_or("application/octet-stream");
            p.push_str(&format!(
                "<D:resourcetype/><D:getcontentlength>{}</D:getcontentlength><D:getcontenttype>{ctype}</D:getcontenttype>\
                 <D:getetag>\"{:x}-{modified:x}\"</D:getetag>",
                md.len(),
                md.len()
            ));
        }
        p.push_str(
            "<D:supportedlock><D:lockentry><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype>\
             </D:lockentry></D:supportedlock><D:lockdiscovery/></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
        );
        p
    };
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">",
    );
    out.push_str(&entry(rel, &md));
    if depth1 && md.is_dir() {
        let mut kids: Vec<_> = fs::read_dir(path).ok()?.flatten().collect();
        kids.sort_by_key(|e| e.file_name());
        for e in kids {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(PART) {
                continue;
            }
            if let Ok(m) = e.metadata() {
                let r = if rel.is_empty() {
                    name
                } else {
                    format!("{rel}/{name}")
                };
                out.push_str(&entry(&r, &m));
            }
        }
    }
    out.push_str("</D:multistatus>");
    Some(out)
}

/// Windows' Win32LastModifiedTime in a PROPPATCH, as Unix seconds.
fn win32_mtime(body: &str) -> Option<i64> {
    crate::cast::tag(body, "Win32LastModifiedTime")
        .or_else(|| crate::cast::tag(body, "getlastmodified"))
        .and_then(|v| parse_http_date(&v))
}

/// A 207 that says yes to every property the PROPPATCH set or removed, with the request's own
/// namespace prefixes (Windows checks that each of its properties is answered).
fn proppatch_answer(body: &str, href: &str) -> String {
    let mut decls = String::new();
    for part in body.split("xmlns:").skip(1) {
        if let Some((prefix, rest)) = part.split_once('=') {
            let q = rest.chars().next().unwrap_or('"');
            if let Some(ns) = rest.get(1..).and_then(|r| r.split(q).next()) {
                if prefix != "D" && !decls.contains(&format!("xmlns:{prefix}=")) {
                    decls.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape(ns)));
                }
            }
        }
    }
    let mut props = String::new();
    for inner in crate::cast::elements(body, "prop") {
        let mut at = 0;
        while let Some(i) = inner[at..].find('<') {
            let s = at + i + 1;
            let Some(e) = inner[s..].find(['>', ' ', '/']) else {
                break;
            };
            let q = &inner[s..s + e];
            at = s + e;
            if q.is_empty() || q.starts_with(['/', '!', '?']) {
                continue;
            }
            // Only the prop's direct children: skip to this element's end.
            let close = format!("</{q}>");
            props.push_str(&format!("<{q}/>"));
            match inner[at..].find(&close) {
                Some(j) => at += j + close.len(),
                None => {
                    at = inner[at..]
                        .find('>')
                        .map(|j| at + j + 1)
                        .unwrap_or(inner.len())
                }
            }
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\"{decls}><D:response>\
         <D:href>{}</D:href><D:propstat><D:prop>{props}</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>\
         </D:response></D:multistatus>",
        escape(href)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_codes_and_dates() {
        assert_eq!(
            relative("/k7m2qx9p/MUSIC/A%20B/x.flac", "k7m2qx9p"),
            Ok("MUSIC/A B/x.flac".into())
        );
        assert_eq!(relative("/k7m2qx9p", "k7m2qx9p"), Ok(String::new()));
        assert_eq!(relative("/k7m2qx9p/", "k7m2qx9p"), Ok(String::new()));
        assert_eq!(
            relative("http://10.0.0.5:8081/k7m2qx9p/a+b.mp3", "k7m2qx9p"),
            Ok("a+b.mp3".into()),
            "+ is a plus"
        );
        assert_eq!(relative("/wrong/MUSIC", "k7m2qx9p"), Err(true));
        assert_eq!(relative("/k7m2qx9p/../etc", "k7m2qx9p"), Err(false));
        assert_eq!(
            relative("/k7m2qx9p/a/%2e%2e/b", "k7m2qx9p"),
            Err(false),
            "an encoded .. too"
        );
        assert_eq!(relative("/k7m2qx9p/a%5Cb", "k7m2qx9p"), Err(false));
        assert_eq!(relative("/k7m2qx9p/.x.cinder-part", "k7m2qx9p"), Err(false));
        assert_eq!(
            pct_encode("MUSIC/Björk/a b.flac"),
            "MUSIC/Bj%C3%B6rk/a%20b.flac"
        );
        assert_eq!(pct_decode("%E2%99%AA%20x").as_deref(), Some("♪ x"));
        assert_eq!(pct_decode("%ff"), None);
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(http_date(1_793_893_760), "Thu, 05 Nov 2026 15:49:20 GMT");
        assert_eq!(
            parse_http_date("Thu, 05 Nov 2026 15:49:20 GMT"),
            Some(1_793_893_760)
        );
        assert_eq!(iso_date(1_793_893_760), "2026-11-05T15:49:20Z");
        let c = new_code();
        assert!(
            c.len() == 8
                && c.chars()
                    .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit())
        );
    }

    /// One request on a new connection; the whole answer.
    fn ask(port: u16, req: &[u8]) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(req).unwrap();
        let mut out = Vec::new();
        let _ = c.read_to_end(&mut out);
        String::from_utf8_lossy(&out).into_owned()
    }
    fn req(m: &str, path: &str, extra: &str, body: &[u8]) -> Vec<u8> {
        let mut v = format!(
            "{m} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn a_computer_maps_the_drive_and_syncs() {
        let _g = crate::cast::tests::E2E
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("cinder-drive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("MUSIC")).unwrap();
        fs::write(root.join("MUSIC/old.mp3"), b"0123456789").unwrap();
        crate::remote::take_uploaded();
        let port = start(&root, "k7m2qx9p", false).unwrap();
        let p = |s: &str| format!("/k7m2qx9p{s}");

        let o = ask(port, &req("OPTIONS", "/", "", b""));
        assert!(
            o.starts_with("HTTP/1.1 200") && o.contains("DAV: 1, 2"),
            "{o}"
        );
        let root_list = ask(port, &req("PROPFIND", &p("/"), "Depth: 1\r\n", b""));
        assert!(root_list.starts_with("HTTP/1.1 207"), "{root_list}");
        assert!(root_list.contains("<D:href>/k7m2qx9p/MUSIC/</D:href>"));
        assert!(
            root_list.contains("<D:quota-available-bytes>"),
            "free space for the drive letter"
        );
        let one = ask(
            port,
            &req("PROPFIND", &p("/MUSIC/old.mp3"), "Depth: 0\r\n", b""),
        );
        assert!(
            one.contains("<D:getcontentlength>10</D:getcontentlength>"),
            "{one}"
        );

        // Flint's way: a part file of its own, then a rename.
        assert!(ask(port, &req("MKCOL", &p("/MUSIC/Björk"), "", b"")).starts_with("HTTP/1.1 201"));
        assert!(ask(port, &req("MKCOL", &p("/MUSIC/Björk"), "", b"")).starts_with("HTTP/1.1 405"));
        assert!(ask(port, &req("MKCOL", &p("/MUSIC/no/such"), "", b"")).starts_with("HTTP/1.1 409"));
        let put = ask(
            port,
            &req(
                "PUT",
                &p("/MUSIC/Bj%C3%B6rk/.t.flac.tmp"),
                "",
                b"fLaC-song-bytes",
            ),
        );
        assert!(put.starts_with("HTTP/1.1 201"), "{put}");
        let mv = ask(
            port,
            &req(
                "MOVE",
                &p("/MUSIC/Bj%C3%B6rk/.t.flac.tmp"),
                &format!("Destination: http://x:{port}/k7m2qx9p/MUSIC/Bj%C3%B6rk/t.flac\r\n"),
                b"",
            ),
        );
        assert!(mv.starts_with("HTTP/1.1 201"), "{mv}");
        assert_eq!(
            fs::read(root.join("MUSIC/Björk/t.flac")).unwrap(),
            b"fLaC-song-bytes"
        );
        assert!(
            fs::read_dir(root.join("MUSIC/Björk")).unwrap().count() == 1,
            "no part file left"
        );
        // Overwrite: F refuses an existing name.
        let mv2 = ask(
            port,
            &req(
                "COPY",
                &p("/MUSIC/old.mp3"),
                "Destination: /k7m2qx9p/MUSIC/Bj%C3%B6rk/t.flac\r\nOverwrite: F\r\n",
                b"",
            ),
        );
        assert!(mv2.starts_with("HTTP/1.1 412"), "{mv2}");
        let cp = ask(
            port,
            &req(
                "COPY",
                &p("/MUSIC/old.mp3"),
                "Destination: /k7m2qx9p/MUSIC/copy.mp3\r\n",
                b"",
            ),
        );
        assert!(
            cp.starts_with("HTTP/1.1 201")
                && fs::read(root.join("MUSIC/copy.mp3")).unwrap() == b"0123456789"
        );

        // macOS sends chunked PUTs.
        let chunked = b"PUT /k7m2qx9p/MUSIC/c.mp3 HTTP/1.1\r\nHost: x\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert!(ask(port, chunked).starts_with("HTTP/1.1 201"));
        assert_eq!(fs::read(root.join("MUSIC/c.mp3")).unwrap(), b"hello world");

        // Ranges, for a player reading a song from the share.
        let g = ask(
            port,
            &req("GET", &p("/MUSIC/old.mp3"), "Range: bytes=2-5\r\n", b""),
        );
        assert!(
            g.starts_with("HTTP/1.1 206")
                && g.ends_with("2345")
                && g.contains("Content-Range: bytes 2-5/10"),
            "{g}"
        );

        // Windows: LOCK makes the empty file, PROPPATCH sets its times, UNLOCK.
        let lk = ask(
            port,
            &req(
                "LOCK",
                &p("/MUSIC/new.mp3"),
                "",
                b"<?xml version=\"1.0\"?><D:lockinfo xmlns:D=\"DAV:\"/>",
            ),
        );
        assert!(
            lk.starts_with("HTTP/1.1 201") && lk.contains("Lock-Token: <opaquelocktoken:"),
            "{lk}"
        );
        let pp = b"<?xml version=\"1.0\" encoding=\"utf-8\" ?><D:propertyupdate xmlns:D=\"DAV:\" xmlns:Z=\"urn:schemas-microsoft-com:\"><D:set><D:prop><Z:Win32CreationTime>Thu, 05 Nov 2026 15:49:20 GMT</Z:Win32CreationTime><Z:Win32LastModifiedTime>Thu, 05 Nov 2026 15:49:20 GMT</Z:Win32LastModifiedTime><Z:Win32FileAttributes>00000020</Z:Win32FileAttributes></D:prop></D:set></D:propertyupdate>";
        let ans = ask(port, &req("PROPPATCH", &p("/MUSIC/new.mp3"), "", pp));
        assert!(ans.starts_with("HTTP/1.1 207"), "{ans}");
        assert!(ans.contains("xmlns:Z=\"urn:schemas-microsoft-com:\""));
        assert!(
            ans.contains(
                "<Z:Win32CreationTime/><Z:Win32LastModifiedTime/><Z:Win32FileAttributes/>"
            ),
            "{ans}"
        );
        let mt = fs::metadata(root.join("MUSIC/new.mp3"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(secs(mt), 1_793_893_760, "the computer's file time is kept");

        // Every answer closes its connection (WebClient would otherwise leave them idle).
        let h = ask(port, &req("HEAD", &p("/MUSIC/old.mp3"), "", b""));
        assert!(
            h.contains("Connection: close") && h.contains("Content-Type: audio/mpeg"),
            "{h}"
        );
        assert!(
            ask(port, &req("PROPFIND", &p("/MUSIC"), "Depth: 0\r\n", b""))
                .contains("Connection: close")
        );

        assert!(
            ask(port, &req("DELETE", &p("/MUSIC/old.mp3"), "", b"")).starts_with("HTTP/1.1 204")
        );
        assert!(!root.join("MUSIC/old.mp3").exists());
        assert!(
            ask(port, &req("DELETE", &p("/"), "", b"")).starts_with("HTTP/1.1 403"),
            "the drive itself stays"
        );
        assert!(ask(port, &req("GET", &p("/../x"), "", b"")).starts_with("HTTP/1.1 400"));
        assert!(
            crate::remote::take_uploaded() >= 6,
            "writes count toward the rescan"
        );

        // Ten wrong codes lock the share for a minute.
        for _ in 0..10 {
            assert!(ask(port, &req("PROPFIND", "/guess/", "", b"")).starts_with("HTTP/1.1 404"));
        }
        assert!(ask(port, &req("PROPFIND", &p("/"), "", b"")).starts_with("HTTP/1.1 429"));
        stop();
        std::thread::sleep(Duration::from_millis(300));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_share_while_the_drive_is_not_mounted() {
        // A plain folder is not a mount of its own: exactly the state USB storage mode leaves.
        let root = std::env::temp_dir().join(format!("cinder-drive-um-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(!mounted(&root));
        let _ = fs::remove_dir_all(&root);
    }
}
