//! The window: a page in your browser, served by this program to this PC alone.
//!
//! WHY A PAGE. cinder-rescue runs on Windows, Linux and macOS, and every one of them has a browser.
//! A native window on all three would mean a GUI toolkit, the largest dependency by far, for six
//! buttons and a log. So the program listens on 127.0.0.1 only, with a random port and a random
//! token in the address it opens; a request without the token, or for any other host name (DNS
//! rebinding), is refused. Each button sends the same arguments the terminal takes, to the same
//! `run`, so the page can do nothing the command line cannot and the other way round.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub static CANCEL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
/// How the last command ended: "", "ok" or "failed".
static LAST: Mutex<&str> = Mutex::new("");

const PAGE: &str = include_str!("page.html");
/// What the page may ask for: the terminal's commands, nothing else.
const COMMANDS: &[&str] = &["info", "dump", "verify", "restore", "driver"];

pub fn serve() -> Result<(), String> {
    // Backups are listed from, and written beside, the program, wherever it was started from
    // (a double-click on macOS starts in the home folder).
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.to_owned()))
    {
        let _ = std::env::set_current_dir(dir);
    }
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("127.0.0.1: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let token = token();
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    // The terminal only: the page has no use for its own address.
    println!("cinder-rescue is open in your browser. If it is not, open this address:\n  {url}");
    println!("Keep this window open while you use it; close it to quit.");
    open_browser(&url);
    for s in l.incoming().flatten() {
        let _ = handle(s, port, &token);
    }
    Ok(())
}

fn token() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seed = format!("{t:?} {} {:p}", std::process::id(), &t);
    crate::fetch::sha(seed.as_bytes())[..32].to_string()
}

fn open_browser(url: &str) {
    use std::process::Command;
    let _ = if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else if let Ok(user) = std::env::var("SUDO_USER") {
        // Run with sudo for USB access: the browser opens as the person, not as root.
        Command::new("sudo")
            .args(["-u", &user, "xdg-open", url])
            .spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
}

fn handle(s: TcpStream, port: u16, token: &str) -> Result<(), String> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut r = BufReader::new(&s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let (method, target) = line
        .split_once(' ')
        .map(|(m, rest)| {
            (
                m.to_string(),
                rest.split(' ').next().unwrap_or("").to_string(),
            )
        })
        .ok_or("bad request")?;
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
    let body = String::from_utf8_lossy(&body).into_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let authorised = (host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}"))
        && query.split('&').any(|kv| kv == format!("t={token}"));
    let mut s = s;
    if !authorised {
        return reply(
            &mut s,
            "403 Forbidden",
            "text/plain",
            "Not this page's address.",
        );
    }
    match (method.as_str(), path) {
        ("GET", "/") => reply(&mut s, "200 OK", "text/html; charset=utf-8", PAGE),
        ("GET", "/state") => reply(&mut s, "200 OK", "application/json", &state()),
        ("POST", "/run") => {
            // One argument per line, exactly as the terminal would pass them.
            let args: Vec<String> = body.lines().map(str::to_string).collect();
            if !args.first().is_some_and(|c| COMMANDS.contains(&c.as_str())) {
                return reply(&mut s, "400 Bad Request", "text/plain", "Not a command.");
            }
            if BUSY.swap(true, Ordering::SeqCst) {
                return reply(&mut s, "409 Conflict", "text/plain", "Already working.");
            }
            CANCEL.store(false, Ordering::Relaxed);
            *LAST.lock().unwrap() = "";
            std::thread::spawn(move || {
                sayln!("\n> cinder-rescue {}", args.join(" "));
                let ok = match crate::run(&args) {
                    Ok(()) => true,
                    Err(e) => {
                        sayln!("\ncinder-rescue: {e}");
                        false
                    }
                };
                *LAST.lock().unwrap() = if ok { "ok" } else { "failed" };
                BUSY.store(false, Ordering::SeqCst);
            });
            reply(&mut s, "202 Accepted", "text/plain", "")
        }
        ("POST", "/cancel") => {
            CANCEL.store(true, Ordering::Relaxed);
            reply(&mut s, "200 OK", "text/plain", "")
        }
        _ => reply(&mut s, "404 Not Found", "text/plain", ""),
    }
}

fn reply(s: &mut TcpStream, status: &str, kind: &str, body: &str) -> Result<(), String> {
    write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())
}

fn json(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o += "\\\"",
            '\\' => o += "\\\\",
            '\n' => o += "\\n",
            '\r' => o += "\\r",
            c if (c as u32) < 0x20 => o += &format!("\\u{:04x}", c as u32),
            c => o.push(c),
        }
    }
    o + "\""
}

/// What the page draws from: the log, whether a command is running, and the backups here.
fn state() -> String {
    let mut files = vec![];
    if let Ok(dir) = std::fs::read_dir(".") {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".bin") || name == "DA.bin" {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            let m = crate::backup::read_manifest(&format!("{name}.manifest")).ok();
            let id = m
                .as_ref()
                .is_some_and(|m| m.header.iter().any(|h| h.starts_with("emmc-cid ")));
            let whole = m.as_ref().is_some_and(|m| m.addr == 0 && m.sums.len() > 30);
            files.push(format!(
                "{{\"name\":{},\"size\":{size},\"manifest\":{},\"id\":{id},\"whole\":{whole}}}",
                json(&name),
                m.is_some()
            ));
        }
    }
    files.sort();
    format!(
        "{{\"busy\":{},\"last\":{},\"os\":{},\"dir\":{},\"log\":{},\"files\":[{}]}}",
        BUSY.load(Ordering::SeqCst),
        json(&LAST.lock().unwrap()),
        json(std::env::consts::OS),
        json(
            &std::env::current_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        ),
        json(&crate::LOG.lock().unwrap()),
        files.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request from another page, or without the token, gets nothing; with both it gets the page.
    #[test]
    fn only_this_address_with_its_token_is_served() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let ask = |req: String| {
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            c.write_all(req.as_bytes()).unwrap();
            let (s, _) = l.accept().unwrap();
            handle(s, port, "tok").unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).unwrap();
            out
        };
        let host = format!("127.0.0.1:{port}");
        assert!(ask(format!("GET /?t=tok HTTP/1.1\r\nHost: {host}\r\n\r\n")).contains("200 OK"));
        assert!(ask(format!("GET /?t=bad HTTP/1.1\r\nHost: {host}\r\n\r\n")).contains("403"));
        assert!(
            ask("GET /?t=tok HTTP/1.1\r\nHost: evil.example:80\r\n\r\n".into()).contains("403")
        );
        let run = |body: &str| {
            format!(
                "POST /run?t=tok HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        };
        assert!(ask(run("rm\n-rf")).contains("400"));
        assert!(
            ask(format!("GET /state?t=tok HTTP/1.1\r\nHost: {host}\r\n\r\n"))
                .contains("\"busy\":false")
        );
    }
}
