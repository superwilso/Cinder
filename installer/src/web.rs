//! The window: a page in your browser, served by this program to this computer alone.
//!
//! WHY A PAGE. The installer runs on Windows, macOS and Linux, and every one of them has a
//! browser. A native window on all three would mean a GUI toolkit — the largest dependency in the
//! project by far, in the one file a stranger downloads to plug into their music player — and the
//! Win32 window this replaced existed on Windows only, so macOS and Linux got a terminal. One page
//! means one design, and the browser brings what a hand-drawn window never had: text that wraps
//! and zooms, screen readers, keyboard navigation, dark mode.
//!
//! SAFETY OF THE SERVER. It listens on 127.0.0.1 only, on a random port, and every request must
//! carry the random token from the address it opened; a request without it, or for any other host
//! name (DNS rebinding), is refused. The page loads nothing from the internet.
//!
//! ONE CORE. Every button ends in `stage`, `device` and `recover` — the same calls the text front
//! end makes — so the page can do nothing the command line cannot.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::catalogue::{self, Comp, Kind, Start};
use crate::device;
use crate::recover::{self, Fix};
use crate::stage::{self, Action};

const PAGE: &str = include_str!("page.html");
/// The Cinder this installer puts on a player (build.rs), e.g. "0.4.1-rc3".
pub const PLAYER_VERSION: &str = env!("CINDER_PLAYER_VERSION");

static BUSY: AtomicBool = AtomicBool::new(false);
static DRY: AtomicBool = AtomicBool::new(false);
/// Lines of the current job, for the page's progress panel.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// How the last job ended: "", "ok", "sent", "staged" or "failed", and a sentence about it.
static RESULT: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
/// The release check: "checking", "newer", "current" or "failed", and the tag/url when known.
static RELEASE: Mutex<(String, String, String)> =
    Mutex::new((String::new(), String::new(), String::new()));
/// The player the page is looking at, when more than one is plugged in.
static CHOSEN: Mutex<Option<PathBuf>> = Mutex::new(None);

fn say(s: impl Into<String>) {
    if let Ok(mut l) = LOG.lock() {
        l.push(s.into());
    }
}

fn finish(kind: &str, msg: &str) {
    *RESULT.lock().unwrap() = (kind.to_string(), msg.to_string());
    BUSY.store(false, Ordering::SeqCst);
}

pub fn serve(dry: bool) -> i32 {
    DRY.store(dry, Ordering::Relaxed);
    let l = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Could not open the installer's page on this computer (127.0.0.1): {e}");
            eprintln!("Use the text version instead:  cinder-installer --console");
            return 1;
        }
    };
    let port = l.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = token();
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    println!(
        "Cinder installer {} is open in your browser.",
        crate::VERSION
    );
    println!("If it did not open, copy this address into your browser:\n\n  {url}\n");
    println!("Keep this window open while you use it. Closing it closes the installer.");
    *RELEASE.lock().unwrap() = ("checking".into(), String::new(), String::new());
    std::thread::spawn(|| {
        let r = match crate::release::latest() {
            Ok(r) if crate::release::is_newer(&r.tag, crate::VERSION) => {
                ("newer".into(), r.tag, r.url)
            }
            Ok(r) => ("current".into(), r.tag, String::new()),
            Err(_) => ("failed".into(), String::new(), String::new()),
        };
        *RELEASE.lock().unwrap() = r;
    });
    // The screenshot script and tests drive the page themselves.
    if std::env::var_os("CINDER_NO_BROWSER").is_none() {
        open_browser(&url);
    }
    for s in l.incoming().flatten() {
        let _ = handle(s, port, &token);
    }
    0
}

fn token() -> String {
    // Not a secret from this computer's own user, only from other pages in the browser: the
    // address is the capability. Mixed from the clock, the process and a stack address, then
    // hashed so nothing about them shows.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seed = format!("{t:?} {} {:p}", std::process::id(), &t);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut out = String::new();
    for round in 0..2u64 {
        for b in seed.bytes().chain(round.to_le_bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        out.push_str(&format!("{h:016x}"));
    }
    out
}

fn open_browser(url: &str) {
    use std::process::Command;
    let _ = if cfg!(windows) {
        // Through Explorer, not `start`: this process runs as administrator (the USB command
        // needs it), and a browser started from it would run as administrator too. Explorer hands
        // the address to the person's own, ordinary session.
        Command::new("explorer.exe").arg(url).spawn()
    } else if let Ok(user) = std::env::var("SUDO_USER") {
        // Run with sudo for the USB command: the browser opens as the person, not as root.
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new("sudo")
            .args(["-u", &user, opener, url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
}

// ── HTTP, the small part of it this needs ─────────────────────────────────────────────────────

struct Request {
    method: String,
    path: String,
    query: String,
    host: String,
    body: String,
}

fn read_request(s: &TcpStream) -> Result<Request, String> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let (mut host, mut len) = (String::new(), 0usize);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).map_err(|e| e.to_string())? == 0 || h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap_or((&h, ""));
        match k.trim().to_ascii_lowercase().as_str() {
            "host" => host = v.trim().to_string(),
            "content-length" => len = v.trim().parse().unwrap_or(0).min(1 << 16),
            _ => {}
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    Ok(Request {
        method,
        path: path.to_string(),
        query: query.to_string(),
        host,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// The request came from this page: right host (no DNS rebinding) and the token.
fn authorised(req: &Request, port: u16, token: &str) -> bool {
    (req.host == format!("127.0.0.1:{port}") || req.host == format!("localhost:{port}"))
        && query_get(&req.query, "t") == Some(token)
}

fn query_get<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

fn reply(s: &mut TcpStream, status: &str, ctype: &str, body: &str) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'unsafe-inline'; img-src 'self' data:\r\n\
         X-Frame-Options: DENY\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())
        .and_then(|_| s.write_all(body.as_bytes()))
        .map_err(|e| e.to_string())
}

fn handle(s: TcpStream, port: u16, token: &str) -> Result<(), String> {
    let req = read_request(&s)?;
    let mut s = s;
    if !authorised(&req, port, token) {
        return reply(
            &mut s,
            "403 Forbidden",
            "text/plain",
            "Not this page's address.",
        );
    }
    let json = "application/json";
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => reply(&mut s, "200 OK", "text/html; charset=utf-8", PAGE),
        ("GET", "/state") => {
            let since = query_get(&req.query, "since")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            reply(&mut s, "200 OK", json, &state_json(since))
        }
        ("GET", "/options") => {
            // The options as a fresh install would start them (`start`), or as the player's saved
            // answers have them (`update`).
            let mode = query_get(&req.query, "mode").unwrap_or("install");
            match options(mode) {
                Ok(comps) => reply(&mut s, "200 OK", json, &comps_json(&comps)),
                Err(e) => reply(&mut s, "500 Internal Server Error", "text/plain", &e),
            }
        }
        ("POST", "/choose") => {
            let p = PathBuf::from(req.body.trim());
            if device::find_players().contains(&p) {
                *CHOSEN.lock().unwrap() = Some(p);
                reply(&mut s, "200 OK", "text/plain", "")
            } else {
                reply(&mut s, "400 Bad Request", "text/plain", "Not a player.")
            }
        }
        ("POST", "/plan") => {
            match parse_job(&req.body, false).and_then(|(a, c)| plan_json(a, &c)) {
                Ok(j) => reply(&mut s, "200 OK", json, &j),
                Err(e) => reply(&mut s, "400 Bad Request", "text/plain", &e),
            }
        }
        ("POST", "/run") => {
            if BUSY.load(Ordering::SeqCst) {
                return reply(&mut s, "409 Conflict", "text/plain", "Already working.");
            }
            LOG.lock().unwrap().clear();
            let job = match parse_job(&req.body, true) {
                Ok(j) => j,
                Err(e) => return reply(&mut s, "400 Bad Request", "text/plain", &e),
            };
            let Some(target) = current_player() else {
                return reply(
                    &mut s,
                    "409 Conflict",
                    "text/plain",
                    "No player is connected.",
                );
            };
            if BUSY.swap(true, Ordering::SeqCst) {
                return reply(&mut s, "409 Conflict", "text/plain", "Already working.");
            }
            *RESULT.lock().unwrap() = (String::new(), String::new());
            std::thread::spawn(move || run_job(job, &target));
            reply(&mut s, "202 Accepted", "text/plain", "")
        }
        ("POST", "/quit") => {
            let _ = reply(&mut s, "200 OK", "text/plain", "");
            std::process::exit(0)
        }
        _ => reply(&mut s, "404 Not Found", "text/plain", "Not found."),
    }
}

// ── jobs ──────────────────────────────────────────────────────────────────────────────────────

enum Job {
    Stage(Action, Vec<Comp>),
    Fix(Fix),
    Clean,
    Report,
}

/// First line: the job (`install`, `update`, `uninstall`, `fix stock-once`…, `clean`, `report`).
/// For install and update, then one `VAR=value` per option; anything the catalogue does not know
/// or would not accept is refused, so the page cannot write a conf the player would misread.
fn parse_job(body: &str, tell: bool) -> Result<(Job, Vec<Comp>), String> {
    let mut lines = body.lines();
    let head = lines.next().unwrap_or("").trim();
    let mut comps = load_comps()?;
    let job = match head {
        "install" | "update" => {
            for l in lines.map(str::trim).filter(|l| !l.is_empty()) {
                let (k, v) = l.split_once('=').ok_or("bad option line")?;
                let c = comps
                    .iter_mut()
                    .find(|c| c.var == k)
                    .ok_or_else(|| format!("unknown option {k}"))?;
                if !c.valid(v) {
                    return Err(format!("{k} cannot be {v}"));
                }
                c.value = v.to_string();
            }
            for (who, on) in catalogue::apply_needs(&mut comps) {
                if tell {
                    say(format!("{on} is switched on too: {who} needs it."));
                }
            }
            let a = if head == "install" {
                Action::Install
            } else {
                Action::Update
            };
            Job::Stage(a, comps.clone())
        }
        "uninstall" => Job::Stage(Action::Uninstall, Vec::new()),
        "clean" => Job::Clean,
        "report" => Job::Report,
        other => match other.strip_prefix("fix ").and_then(Fix::from_word) {
            Some(f) => Job::Fix(f),
            None => return Err("not a job".into()),
        },
    };
    Ok((job, comps))
}

fn load_comps() -> Result<Vec<Comp>, String> {
    let text = crate::CATALOGUE.ok_or("no component catalogue in this build")?;
    let mut comps = catalogue::parse_catalogue(text)?;
    crate::payload::fit_catalogue(&mut comps);
    Ok(comps)
}

fn options(mode: &str) -> Result<Vec<Comp>, String> {
    let mut comps = load_comps()?;
    match mode {
        "update" => {
            if let Some(conf) = current_player()
                .and_then(|p| device::read_installed(&p, &stage::payload_names()).conf)
            {
                catalogue::apply_saved(&mut comps, &conf);
            }
        }
        "sound" => catalogue::apply_start(&mut comps, Start::Sound),
        "everything" => catalogue::apply_start(&mut comps, Start::Everything),
        _ => catalogue::apply_start(&mut comps, Start::Cinder),
    }
    // Saved answers from before an option's needs were written down can hold one without the
    // other; the page starts from what will actually be installed.
    catalogue::apply_needs(&mut comps);
    Ok(comps)
}

fn current_player() -> Option<PathBuf> {
    let found = device::find_players();
    let chosen = CHOSEN.lock().unwrap().clone();
    match chosen {
        Some(p) if found.contains(&p) => Some(p),
        _ => found.into_iter().next(),
    }
}

fn run_job((job, _): (Job, Vec<Comp>), target: &Path) {
    let dry = DRY.load(Ordering::Relaxed);
    match job {
        Job::Fix(f) => {
            if dry {
                say(format!(
                    "Practice mode: would change the start-up switches for {f:?}."
                ));
                return finish("ok", f.done());
            }
            match recover::apply(target, f) {
                Ok(()) => finish("ok", f.done()),
                Err(e) => finish(
                    "failed",
                    &format!("Could not write to the player's drive: {e}"),
                ),
            }
        }
        Job::Clean => {
            let st = device::read_installed(target, &stage::payload_names());
            if dry {
                return finish("ok", "Practice mode: nothing was deleted.");
            }
            let mut bad = 0;
            for (name, r) in stage::clean_leftovers(target, &st.leftovers) {
                match r {
                    Ok(()) => say(format!("deleted {name}")),
                    Err(e) => {
                        bad += 1;
                        say(format!("could not delete {name}: {e}"));
                    }
                }
            }
            if bad == 0 {
                finish(
                    "ok",
                    "The leftover install files are gone. Your music was not touched.",
                )
            } else {
                finish(
                    "failed",
                    "Some files could not be deleted; the list above says which.",
                )
            }
        }
        Job::Report => {
            let text = crate::report::build(Some(target), &LOG.lock().unwrap().join("\n"));
            match crate::report::save(&text) {
                Ok(p) => finish(
                    "ok",
                    &format!(
                        "Saved {}. Attach it to an issue on GitHub so the problem can be found.",
                        p.display()
                    ),
                ),
                Err(e) => finish("failed", &format!("Could not save the report: {e}")),
            }
        }
        Job::Stage(action, comps) => stage_job(action, &comps, target, dry),
    }
}

fn stage_job(action: Action, comps: &[Comp], target: &Path, dry: bool) {
    say(format!("Copying to the player ({})…", target.display()));
    let r = stage::write_payload(action, comps, target, dry, |name, n| {
        say(format!("  {name}  {} KB", n.div_ceil(1024)));
    });
    if let Err(e) = r {
        return finish(
            "failed",
            &format!(
                "Copying to the player stopped: {e}. The player still runs what it ran before; \
                 try again, and if it fails the same way, save a report from \u{201c}Fix a problem\u{201d}."
            ),
        );
    }
    if dry {
        return finish(
            "ok",
            "Practice mode: every step ran, nothing was written and the player was not touched.",
        );
    }
    say("Everything is copied. Asking the player to restart into its updater…");
    match stage::trigger_fw_upgrade(target, |m| say(format!("  {m}"))) {
        Ok(()) => finish(
            "sent",
            if action.is_removal() {
                "The player is restarting into its updater. It removes Cinder and starts Sony's player by itself, in about two minutes."
            } else {
                "The player is restarting into its updater. It installs Cinder and starts it by itself, in about two minutes."
            },
        ),
        Err(e) => {
            say(format!("  reason: {e}"));
            let why = if e.kind() == std::io::ErrorKind::PermissionDenied {
                if cfg!(windows) {
                    "Windows did not give this program access to the player. Close the installer, then right-click it and choose \u{201c}Run as administrator\u{201d}."
                } else {
                    "Sending the command needs administrator rights. Close the installer and start it again with sudo from a terminal."
                }
            } else if cfg!(target_os = "macos") {
                "macOS would not hand over the player's USB connection. Start the installer with sudo from a terminal, or finish on a Windows or Linux computer."
            } else {
                "The player did not take the restart command. Unplug it, plug it in again, set it to USB storage, and try again."
            };
            finish(
                "staged",
                &format!("The files are on the player, but it was not told to install them yet. {why} Running it again is safe."),
            )
        }
    }
}

// ── JSON, written by hand (no dependencies) ──────────────────────────────────────────────────

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                o.push_str(&format!("\\u{:04x}", c as u32))
            }
            // `</` would let a value close a <script> if this were ever inlined; cheap to rule out.
            '<' => o.push_str("\\u003c"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn opt(s: &Option<String>) -> String {
    s.as_deref().map_or("null".into(), esc)
}

fn os_word() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn state_json(since: usize) -> String {
    let players = device::find_players();
    let cur = current_player();
    let info = cur.as_ref().map_or("null".into(), |p| {
        let st = device::read_installed(p, &stage::payload_names());
        let last = match st.last {
            None => "none",
            Some(device::LastRun::Installed) => "installed",
            Some(device::LastRun::Uninstalled) => "uninstalled",
            Some(device::LastRun::Aborted) => "aborted",
            Some(device::LastRun::Unfinished) => "unfinished",
        };
        let (rv, rc) = st.running.clone().map_or((None, None), |(v, c)| (Some(v), Some(c)));
        let vs = rv.as_deref().map_or("unknown", |v| compare_to_this(v));
        format!(
            "{{\"present\":{},\"last\":{},\"when\":{},\"channel\":{},\"staged_version\":{},\
             \"running_version\":{},\"running_channel\":{},\"running_vs_this\":{},\"walkman_one\":{},\"advisory\":{},\
             \"leftovers\":{},\"off\":{},\"once\":{},\"clear_pending\":{},\"latched\":{}}}",
            st.present(),
            esc(last),
            opt(&st.when),
            opt(&st.channel),
            opt(&st.installer_version),
            opt(&rv),
            opt(&rc),
            esc(vs),
            st.walkman_one,
            opt(&st.advisory()),
            st.leftovers.len(),
            st.off,
            st.once,
            st.clear_pending,
            st.latched,
        )
    });
    let log = LOG.lock().unwrap();
    let lines: Vec<String> = log.iter().skip(since).map(|l| esc(l)).collect();
    let (rk, rm) = RESULT.lock().unwrap().clone();
    let (rs, rt, ru) = RELEASE.lock().unwrap().clone();
    format!(
        "{{\"version\":{},\"carries\":{},\"channels\":{},\"os\":{},\"dry\":{},\"busy\":{},\"result\":{},\"result_msg\":{},\
         \"log_total\":{},\"log\":[{}],\"release\":{{\"state\":{},\"tag\":{},\"url\":{}}},\
         \"players\":[{}],\"player\":{},\"info\":{}}}",
        esc(crate::VERSION),
        esc(PLAYER_VERSION),
        esc(&crate::channels()),
        esc(os_word()),
        DRY.load(Ordering::Relaxed),
        BUSY.load(Ordering::SeqCst),
        esc(&rk),
        esc(&rm),
        log.len(),
        lines.join(","),
        esc(&rs),
        esc(&rt),
        esc(&ru),
        players.iter().map(|p| esc(&p.display().to_string())).collect::<Vec<_>>().join(","),
        cur.map_or("null".into(), |p| esc(&p.display().to_string())),
        info,
    )
}

/// The player's running version against this installer's: "older", "same" or "newer".
fn compare_to_this(running: &str) -> &'static str {
    let (r, me) = (format!("v{running}"), format!("v{PLAYER_VERSION}"));
    if crate::release::is_newer(&me, &r) {
        "older"
    } else if crate::release::is_newer(&r, &me) {
        "newer"
    } else {
        "same"
    }
}

fn comps_json(comps: &[Comp]) -> String {
    let items: Vec<String> = comps
        .iter()
        .map(|c| {
            let (kind, choices) = match &c.kind {
                Kind::Bool => ("bool", String::new()),
                Kind::Enum(_) => (
                    "enum",
                    c.choices
                        .iter()
                        .map(|ch| {
                            format!(
                                "{{\"value\":{},\"label\":{},\"help\":{}}}",
                                esc(&ch.value),
                                esc(&ch.label),
                                esc(&ch.help)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                ),
            };
            format!(
                "{{\"id\":{},\"var\":{},\"kind\":{},\"title\":{},\"desc\":{},\"risk\":{},\"group\":{},\
                 \"start\":{},\"default\":{},\"value\":{},\"choices\":[{}],\"needs\":[{}],\"language\":{},\
                 \"lang_name\":{},\"lang_native\":{}}}",
                esc(&c.id),
                esc(&c.var),
                esc(kind),
                esc(&c.title),
                esc(&c.desc),
                esc(&c.risk),
                esc(&c.group),
                esc(&c.start),
                esc(&c.default),
                esc(&c.value),
                choices,
                c.needs.iter().map(|n| esc(n)).collect::<Vec<_>>().join(","),
                c.is_language(),
                esc(if c.is_language() { c.language_name() } else { "" }),
                esc(if c.is_language() { c.language_native() } else { "" }),
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn plan_json(job: Job, _comps: &[Comp]) -> Result<String, String> {
    let (action, comps) = match job {
        Job::Stage(a, c) => (a, c),
        _ => return Err("only installs, updates and removals have a plan".into()),
    };
    let w1 = current_player().is_some_and(|p| device::running_walkman_one(&p));
    let p = stage::plan(action, &comps, w1)?;
    Ok(format!(
        "{{\"files\":{},\"kb\":{},\"channel\":{}}}",
        p.count(),
        p.total_bytes().div_ceil(1024),
        esc(crate::payload::chosen(&comps).name)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(host: &str, query: &str) -> Request {
        Request {
            method: "GET".into(),
            path: "/".into(),
            query: query.into(),
            host: host.into(),
            body: String::new(),
        }
    }

    #[test]
    fn only_this_page_is_answered() {
        assert!(authorised(&req("127.0.0.1:5000", "t=abc"), 5000, "abc"));
        assert!(authorised(
            &req("localhost:5000", "since=3&t=abc"),
            5000,
            "abc"
        ));
        assert!(
            !authorised(&req("127.0.0.1:5000", "t=abd"), 5000, "abc"),
            "wrong token"
        );
        assert!(
            !authorised(&req("127.0.0.1:5000", ""), 5000, "abc"),
            "no token"
        );
        assert!(
            !authorised(&req("evil.example:5000", "t=abc"), 5000, "abc"),
            "rebinding"
        );
        assert!(
            !authorised(&req("127.0.0.1:5001", "t=abc"), 5000, "abc"),
            "other port"
        );
    }

    #[test]
    fn json_strings_cannot_break_out() {
        assert_eq!(
            esc("a\"b\\c\n</script>"),
            "\"a\\\"b\\\\c\\n\\u003c/script>\""
        );
        assert_eq!(esc("\u{1}"), "\"\\u0001\"");
    }

    fn parse_job_t(b: &str) -> Result<(Job, Vec<Comp>), String> {
        parse_job(b, false)
    }

    #[test]
    fn the_page_cannot_ask_for_an_option_the_catalogue_rejects() {
        if crate::CATALOGUE.is_none() {
            return;
        }
        assert!(parse_job_t("install\nNOT_A_VAR=1").is_err());
        assert!(parse_job_t("rm -rf /").is_err());
        assert!(parse_job_t("fix nonsense").is_err());
        assert!(matches!(
            parse_job_t("fix try-again"),
            Ok((Job::Fix(Fix::TryAgain), _))
        ));
        assert!(matches!(
            parse_job_t("uninstall"),
            Ok((Job::Stage(Action::Uninstall, _), _))
        ));
        let comps = load_comps().unwrap();
        let b = comps.iter().find(|c| matches!(c.kind, Kind::Bool)).unwrap();
        assert!(
            parse_job_t(&format!("install\n{}=7", b.var)).is_err(),
            "a bool is 0 or 1"
        );
        assert!(parse_job_t(&format!("install\n{}=1", b.var)).is_ok());
    }

    #[test]
    fn every_option_reaches_the_page() {
        if crate::CATALOGUE.is_none() {
            return;
        }
        let comps = load_comps().unwrap();
        let j = comps_json(&comps);
        for c in &comps {
            assert!(j.contains(&esc(&c.var)), "{} missing", c.var);
        }
    }

    #[test]
    fn tokens_differ_and_are_long() {
        let a = token();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_eq!(a.len(), 32);
        assert_ne!(a, token());
    }
}
