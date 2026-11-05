//! Over Wi-Fi: send finished plays to ListenBrainz, and ask GitHub whether a newer Cinder is out.
//! Both opt-in (Settings ▸ Wi-Fi), both off unless switched on.
//!
//! The network is curl, from the Wi-Fi install option (`/system/vendor/cinder/wifi/curl`, with
//! Mozilla's CA list beside it). cinder-home may open sockets itself — the kernel's paranoid
//! networking lets group 3003 (inet) through, and appmgr starts it in that group — so curl runs as
//! cinder-home's own user and the setuid helper is not involved.
//!
//! Everything that decides WHAT is sent is pure and tested here: which log lines count, the UTC
//! time of each, the JSON, and the version comparison. Only [`run_curl`] touches the network, and
//! these run on the shell's Wi-Fi worker thread, never the render thread.

use std::path::Path;
use std::process::Command;

pub const WIFI_DIR: &str = "/system/vendor/cinder/wifi";
/// The user's ListenBrainz token (listenbrainz.org/settings), one line, put on the drive by them.
pub const LB_TOKEN: &str = "/contents/cinder_listenbrainz.txt";
/// The newest `listened_at` (log time, before the zone is taken off) already sent. Beside the log,
/// so a log cleared by an uploader on the computer does not resend what came before it.
pub const LB_STATE: &str = "/contents/.scrobbler.log.listenbrainz";
const LB_URL: &str = "https://api.listenbrainz.org/1/submit-listens";
const LATEST_URL: &str = "https://api.github.com/repos/superwilso/Cinder/releases/latest";
/// ListenBrainz takes up to 1000 listens a request; 100 keeps each request small on a weak link.
const BATCH: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listen {
    /// The log's time: local wall time, as the player's clock holds it.
    pub log_ts: u64,
    pub artist: String,
    pub album: String,
    pub title: String,
    pub track_no: u32,
    pub length_s: u32,
}

/// The listened (`L`) lines of an Audioscrobbler/1.1 log newer than `after`, oldest first, at most
/// `max`. Comment lines, skipped (`S`) plays, lines without an artist or title, and a last line
/// still being written (no newline) are left out.
pub fn listens(log: &str, after: u64, max: usize) -> Vec<Listen> {
    let mut out: Vec<Listen> = Vec::new();
    let complete = match log.rfind('\n') {
        Some(i) => &log[..i],
        None => return out,
    };
    for line in complete.lines() {
        if line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 || f[5] != "L" {
            continue;
        }
        let Ok(ts) = f[6].trim().parse::<u64>() else {
            continue;
        };
        if ts <= after || f[0].trim().is_empty() || f[2].trim().is_empty() {
            continue;
        }
        out.push(Listen {
            log_ts: ts,
            artist: f[0].trim().to_string(),
            album: f[1].trim().to_string(),
            title: f[2].trim().to_string(),
            track_no: f[3].trim().parse().unwrap_or(0),
            length_s: f[4].trim().parse().unwrap_or(0),
        });
    }
    out.sort_by_key(|l| l.log_ts);
    out.truncate(max);
    out
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
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

/// The submit-listens body. `offset_s` is the player's clock minus UTC (what `cinder-wifi status`
/// reports as `utc_offset=`), taken off every time so ListenBrainz gets real UTC.
pub fn lb_json(ls: &[Listen], offset_s: i64, version: &str) -> String {
    let mut o = String::from("{\"listen_type\":\"import\",\"payload\":[");
    for (i, l) in ls.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let at = (l.log_ts as i64 - offset_s).max(0);
        o.push_str(&format!(
            "{{\"listened_at\":{at},\"track_metadata\":{{\"artist_name\":{},\"track_name\":{}",
            json_str(&l.artist),
            json_str(&l.title)
        ));
        if !l.album.is_empty() {
            o.push_str(&format!(",\"release_name\":{}", json_str(&l.album)));
        }
        o.push_str(&format!(
            ",\"additional_info\":{{\"media_player\":\"Cinder\",\"submission_client\":\"Cinder\",\"submission_client_version\":{}",
            json_str(version)
        ));
        if l.length_s > 0 {
            o.push_str(&format!(",\"duration\":{}", l.length_s));
        }
        if l.track_no > 0 {
            o.push_str(&format!(",\"tracknumber\":{}", l.track_no));
        }
        o.push_str("}}}");
    }
    o.push_str("]}");
    o
}

/// A ListenBrainz user token as the user pasted it: one line, letters, digits and dashes.
pub fn clean_token(raw: &str) -> Option<String> {
    let t = raw.lines().next().unwrap_or("").trim();
    (t.len() >= 8 && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then(|| t.to_string())
}

/// `v0.3.23` / `0.3.23-rc1` → (0, 3, 23). Anything that is not three numbers is None.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(v)
}

/// The `tag_name` of GitHub's latest-release answer.
pub fn tag_name(body: &str) -> Option<String> {
    let i = body.find("\"tag_name\"")?;
    let rest = &body[i + 10..];
    let rest = &rest[rest.find('"')? + 1..];
    Some(rest[..rest.find('"')?].to_string())
}

/// Newer than this build? `None` when the answer cannot be read.
pub fn newer(latest_tag: &str, running: &str) -> Option<bool> {
    Some(parse_version(latest_tag)? > parse_version(running)?)
}

/// What a curl exit code means to the person holding the player.
pub fn curl_error(code: i32) -> &'static str {
    match code {
        6 => "No internet: the name did not resolve",
        7 => "No internet: could not connect",
        28 => "No answer in 30 seconds",
        35 | 60 => "The secure connection failed (is the clock right?)",
        _ => "No answer from the server",
    }
}

/// Run curl with the CA list, a 30 s cap, and `args`. Returns (HTTP status, body).
fn run_curl(args: &[&str]) -> Result<(u32, String), String> {
    let curl = format!("{WIFI_DIR}/curl");
    if !Path::new(&curl).exists() {
        return Err("Wi-Fi is not installed".into());
    }
    let out = Command::new(&curl)
        .args(["-sS", "--max-time", "30", "--cacert"])
        .arg(format!("{WIFI_DIR}/cacert.pem"))
        .args(["-w", "\n%{http_code}"])
        .args(args)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
    let code: u32 = code.trim().parse().unwrap_or(0);
    if code == 0 {
        return Err(curl_error(out.status.code().unwrap_or(-1)).into());
    }
    Ok((code, body.to_string()))
}

/// Send every listen not yet sent. Ok(n) = n sent (0 = nothing new); Err = why not, for the page.
pub fn lb_submit(log_path: &str, offset_s: i64, version: &str) -> Result<usize, String> {
    let token = std::fs::read_to_string(LB_TOKEN)
        .ok()
        .and_then(|t| clean_token(&t))
        .ok_or_else(|| "No token in cinder_listenbrainz.txt".to_string())?;
    let log = std::fs::read_to_string(log_path).unwrap_or_default();
    let mut after: u64 = std::fs::read_to_string(LB_STATE)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let tmp = "/tmp/cinder-lb";
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let _ = std::fs::DirBuilder::new().mode(0o700).create(tmp);
        let _ = std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(0o700));
    }
    let (hdr, body) = (format!("{tmp}/h"), format!("{tmp}/b"));
    let mut sent = 0;
    for _ in 0..20 {
        let batch = listens(&log, after, BATCH);
        let Some(last) = batch.last().map(|l| l.log_ts) else {
            break;
        };
        // The token goes in a file curl reads, not in argv, where any process could read it.
        std::fs::write(
            &hdr,
            format!("Authorization: Token {token}\nContent-Type: application/json\n"),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(&body, lb_json(&batch, offset_s, version)).map_err(|e| e.to_string())?;
        let r = run_curl(&[
            "-H",
            &format!("@{hdr}"),
            "--data-binary",
            &format!("@{body}"),
            LB_URL,
        ]);
        let _ = std::fs::remove_file(&hdr);
        if !matches!(r, Ok((200 | 400, _))) {
            let _ = std::fs::remove_file(&body);
        }
        match r? {
            (200, _) => {}
            (401, _) => return Err("ListenBrainz refused the token".into()),
            // A batch ListenBrainz will never take must not block every play after it.
            (400, _) => {}
            (code, _) => return Err(format!("ListenBrainz answered {code}")),
        }
        after = last;
        let _ = std::fs::write(LB_STATE, format!("{after}\n"));
        sent += batch.len();
    }
    let _ = std::fs::remove_file(&body);
    Ok(sent)
}

/// Ask GitHub for the latest release. Ok(Some(tag)) when it is newer than `version`.
pub fn update_check(version: &str) -> Result<Option<String>, String> {
    let ua = format!("User-Agent: Cinder/{version}");
    let (code, body) = run_curl(&[
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        &ua,
        LATEST_URL,
    ])?;
    if code != 200 {
        return Err(format!("GitHub answered {code}"));
    }
    let tag = tag_name(&body).ok_or("no release found")?;
    match newer(&tag, version) {
        Some(true) => Ok(Some(tag)),
        Some(false) => Ok(None),
        None => Err(format!("cannot read the release name {tag}")),
    }
}

// ── Lyrics from LRCLIB (lrclib.net: free, no account, synced lyrics) ─────────────────────────────

/// A JSON string field's value (`null` and absent are None), unescaped. Enough JSON for LRCLIB's
/// flat answer; nested answers go through [`Json`].
pub fn json_field(body: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\"");
    let mut rest = &body[body.find(&k)? + k.len()..];
    rest = rest.trim_start().strip_prefix(':')?.trim_start();
    read_str(&mut rest.strip_prefix('"')?.chars())
}

/// The rest of a JSON string, after its opening quote, unescaped (\uXXXX and surrogate pairs too).
fn read_str<I: Iterator<Item = char>>(it: &mut I) -> Option<String> {
    let mut out = String::new();
    while let Some(c) = it.next() {
        match c {
            '"' => return Some(out),
            '\\' => match it.next()? {
                'n' => out.push('\n'),
                'r' => {}
                't' => out.push('\t'),
                'u' => {
                    let hex: String = it.by_ref().take(4).collect();
                    let mut cp = u32::from_str_radix(&hex, 16).ok()?;
                    if (0xD800..0xDC00).contains(&cp) {
                        // A surrogate pair: the low half follows as \uXXXX.
                        let lo: String = it.by_ref().skip(2).take(4).collect();
                        let lo = u32::from_str_radix(&lo, 16).ok()?;
                        cp = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                    }
                    out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
    None
}

/// A JSON value: enough of a reader for MusicBrainz's nested answers, with no dependency.
#[derive(Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// None for anything that is not one whole JSON value (or nests deeper than 64).
    pub fn parse(text: &str) -> Option<Json> {
        let mut it = text.chars().peekable();
        let v = Self::value(&mut it, 0)?;
        Self::ws(&mut it);
        it.next().is_none().then_some(v)
    }
    fn ws(it: &mut std::iter::Peekable<std::str::Chars>) {
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
        }
    }
    fn value(it: &mut std::iter::Peekable<std::str::Chars>, depth: u32) -> Option<Json> {
        if depth > 64 {
            return None;
        }
        Self::ws(it);
        match *it.peek()? {
            '"' => {
                it.next();
                read_str(it).map(Json::Str)
            }
            open @ ('{' | '[') => {
                it.next();
                let close = if open == '{' { '}' } else { ']' };
                let (mut obj, mut arr) = (Vec::new(), Vec::new());
                Self::ws(it);
                if it.peek() == Some(&close) {
                    it.next();
                } else {
                    loop {
                        Self::ws(it);
                        if open == '{' {
                            if it.next()? != '"' {
                                return None;
                            }
                            let k = read_str(it)?;
                            Self::ws(it);
                            if it.next()? != ':' {
                                return None;
                            }
                            obj.push((k, Self::value(it, depth + 1)?));
                        } else {
                            arr.push(Self::value(it, depth + 1)?);
                        }
                        Self::ws(it);
                        match it.next()? {
                            ',' => continue,
                            c if c == close => break,
                            _ => return None,
                        }
                    }
                }
                Some(if open == '{' {
                    Json::Obj(obj)
                } else {
                    Json::Arr(arr)
                })
            }
            _ => {
                let mut w = String::new();
                while let Some(&c) = it.peek() {
                    if !(c.is_ascii_alphanumeric() || "+-.".contains(c)) {
                        break;
                    }
                    w.push(c);
                    it.next();
                }
                match w.as_str() {
                    "null" => Some(Json::Null),
                    "true" => Some(Json::Bool(true)),
                    "false" => Some(Json::Bool(false)),
                    _ => w.parse().ok().map(Json::Num),
                }
            }
        }
    }
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    /// The items of an array; none for anything else.
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }
}

/// `/a/b/Song.flac` -> `/a/b/Song.lrc`, the sidecar lyrics.rs reads first.
pub fn lrc_path(track: &str) -> String {
    let dot = track
        .rfind('.')
        .filter(|&d| d > track.rfind('/').map_or(0, |s| s + 1));
    format!("{}.lrc", &track[..dot.unwrap_or(track.len())])
}

/// The .lrc to write from LRCLIB's answer: the synced lyrics when it has them, else the plain
/// ones; None for an instrumental or an answer without words.
pub fn lrc_from_answer(body: &str) -> Option<String> {
    if body.contains("\"instrumental\":true") {
        return None;
    }
    let text = json_field(body, "syncedLyrics")
        .filter(|t| !t.trim().is_empty())
        .or_else(|| json_field(body, "plainLyrics"))
        .filter(|t| !t.trim().is_empty())?;
    Some(format!("[re:LRCLIB via Cinder]\n{}\n", text.trim_end()))
}

/// Look the song up on LRCLIB and write its .lrc beside it. Ok(true) = written, Ok(false) = LRCLIB
/// has none (remember the miss).
pub fn fetch_lyrics(
    track: &str,
    artist: &str,
    title: &str,
    album: &str,
    length_s: u32,
    version: &str,
) -> Result<bool, String> {
    let ua = format!("User-Agent: Cinder/{version} (https://github.com/superwilso/Cinder)");
    let mut args: Vec<String> = vec!["-H".into(), ua, "--get".into()];
    for (k, v) in [
        ("artist_name", artist),
        ("track_name", title),
        ("album_name", album),
    ] {
        if !v.is_empty() {
            args.push("--data-urlencode".into());
            args.push(format!("{k}={v}"));
        }
    }
    if length_s > 0 {
        args.push("--data-urlencode".into());
        args.push(format!("duration={length_s}"));
    }
    args.push("https://lrclib.net/api/get".into());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, body) = run_curl(&refs)?;
    match code {
        200 => match lrc_from_answer(&body) {
            Some(lrc) => {
                std::fs::write(lrc_path(track), lrc)
                    .map_err(|_| "cannot write the lyrics".to_string())?;
                Ok(true)
            }
            None => Ok(false),
        },
        // The exact lookup wants the album and the length to match; the search does not. Its first
        // hit is the best match LRCLIB has for the artist and title.
        404 => {
            let ua = format!("User-Agent: Cinder/{version} (https://github.com/superwilso/Cinder)");
            let a = format!("artist_name={artist}");
            let t = format!("track_name={title}");
            let (code, body) = run_curl(&[
                "-H",
                &ua,
                "--get",
                "--data-urlencode",
                &a,
                "--data-urlencode",
                &t,
                "https://lrclib.net/api/search",
            ])?;
            match (code, lrc_from_answer(&body)) {
                (200, Some(lrc)) => {
                    std::fs::write(lrc_path(track), lrc)
                        .map_err(|_| "cannot write the lyrics".to_string())?;
                    Ok(true)
                }
                (200, None) => Ok(false),
                (c, _) => Err(format!("LRCLIB answered {c}")),
            }
        }
        c => Err(format!("LRCLIB answered {c}")),
    }
}

// ── Album art from the Cover Art Archive (MusicBrainz finds the album; both free, no account) ──

/// "The Dark Side of the Moon (2011 Remaster)" -> "The Dark Side of the Moon": a trailing (…) or
/// […] edition note off, as often as there is one.
pub fn bare_title(s: &str) -> &str {
    let mut t = s.trim();
    while t.ends_with(')') || t.ends_with(']') {
        match t.rfind(['(', '[']) {
            Some(i) if i > 0 => t = t[..i].trim_end(),
            _ => break,
        }
    }
    t
}

/// Two titles are the same album when their letters and digits are, ignoring case, edition notes
/// and a leading "The".
fn title_key(s: &str) -> String {
    let low = bare_title(s).to_lowercase();
    let low = low.strip_prefix("the ").unwrap_or(&low);
    low.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The MusicBrainz search for an album: both names as phrases (Lucene: `\` and `"` escaped).
pub fn mb_query(artist: &str, album: &str) -> String {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "releasegroup:\"{}\" AND artist:\"{}\"",
        q(bare_title(album)),
        q(artist)
    )
}

/// From a MusicBrainz release-group search, the album's id: its title must be the album's and its
/// score 90 or more, and a plain album (no secondary type) wins over a compilation, live record or
/// a fan release with the same name — MusicBrainz scored "Dark Side of the Moon [RESTORED]", a
/// compilation, as high as the album itself (2026-11-05).
pub fn mb_pick(body: &str, album: &str) -> Option<String> {
    let want = title_key(album);
    if want.is_empty() {
        return None;
    }
    let v = Json::parse(body)?;
    let mut best: Option<(bool, &str)> = None;
    for g in v.get("release-groups")?.items() {
        let (Some(id), Some(title)) = (
            g.get("id").and_then(Json::str),
            g.get("title").and_then(Json::str),
        ) else {
            continue;
        };
        let uuid = id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if !uuid
            || g.get("score").and_then(Json::num).unwrap_or(0.0) < 90.0
            || title_key(title) != want
        {
            continue;
        }
        let plain = g
            .get("secondary-types")
            .is_none_or(|t| t.items().is_empty());
        if best.is_none_or(|(was_plain, _)| plain && !was_plain) {
            best = Some((plain, id));
        }
    }
    best.map(|(_, id)| id.to_string())
}

/// "jpg" or "png" from a file's first bytes; None for anything else (an error page).
pub fn image_ext(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if head.starts_with(b"\x89PNG") {
        Some("png")
    } else {
        None
    }
}

/// Find the album on MusicBrainz and write its front cover (500 px) from the Cover Art Archive as
/// `cover.jpg` (or .png) in `dir`. Ok(true) = written, Ok(false) = none to be had (remember the miss).
pub fn fetch_cover(dir: &str, artist: &str, album: &str, version: &str) -> Result<bool, String> {
    let ua = format!("User-Agent: Cinder/{version} ( https://github.com/superwilso/Cinder )");
    let query = format!("query={}", mb_query(artist, album));
    let (code, body) = run_curl(&[
        "-H",
        &ua,
        "--get",
        "--data-urlencode",
        &query,
        "--data-urlencode",
        "fmt=json",
        "--data-urlencode",
        "limit=10",
        "https://musicbrainz.org/ws/2/release-group/",
    ])?;
    if code != 200 {
        return Err(format!("MusicBrainz answered {code}"));
    }
    let Some(id) = mb_pick(&body, album) else {
        return Ok(false);
    };
    let part = format!("{dir}/.cinder-cover.part");
    let url = format!("https://coverartarchive.org/release-group/{id}/front-500");
    let got = run_curl(&[
        "-H",
        &ua,
        "-L",
        "--max-filesize",
        "4000000",
        "-o",
        &part,
        &url,
    ]);
    let out = got.and_then(|(code, _)| match code {
        404 => Ok(false),
        200 => {
            let head: Vec<u8> = std::fs::read(&part)
                .unwrap_or_default()
                .into_iter()
                .take(8)
                .collect();
            let ext = image_ext(&head).ok_or("the cover is not an image")?;
            std::fs::rename(&part, format!("{dir}/cover.{ext}"))
                .map_err(|_| "cannot write the cover")?;
            Ok(true)
        }
        c => Err(format!("the Cover Art Archive answered {c}")),
    });
    let _ = std::fs::remove_file(&part);
    out
}

// ── Internet radio ▸ FIND: radio-browser.info (free, no account, community-kept) ─────────────────

const RADIO_BROWSER: &str = "https://all.api.radio-browser.info/json/stations/search";

/// The stations in a radio-browser.info answer that this player can play: MP3, AAC and AAC+
/// streams, and HLS (which it lists as "UNKNOWN" more often than not, and is AAC on the stations
/// that use it); Ogg, FLAC and Opus are left out. Most-listened first, as asked; one per stream.
pub fn parse_radio_browser(body: &str) -> Vec<cinder_ui::netradio::Found> {
    let Some(v) = Json::parse(body) else {
        return Vec::new();
    };
    let mut out: Vec<cinder_ui::netradio::Found> = Vec::new();
    for st in v.items() {
        let text = |k: &str| {
            st.get(k)
                .and_then(Json::str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let url = Some(text("url_resolved"))
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| text("url"));
        let codec = text("codec").to_ascii_uppercase();
        let hls = st.get("hls").and_then(Json::num) == Some(1.0) || url.contains(".m3u8");
        let playable = hls || codec == "MP3" || codec == "AAC" || codec == "AAC+";
        let ok_url = (url.starts_with("http://") || url.starts_with("https://"))
            && url.len() <= 512
            && url.chars().all(|c| c.is_ascii_graphic());
        let name: String = text("name")
            .chars()
            .filter(|c| !c.is_control() && *c != '|')
            .take(60)
            .collect();
        if !playable || !ok_url || name.trim().is_empty() || out.iter().any(|f| f.url == url) {
            continue;
        }
        let kind = if hls {
            "HLS".to_string()
        } else {
            codec.clone()
        };
        let rate = st
            .get("bitrate")
            .and_then(Json::num)
            .filter(|b| *b > 0.0)
            .map(|b| format!(" {b}"))
            .unwrap_or_default();
        let mut sub = format!("{kind}{rate}");
        let cc = text("countrycode");
        if !cc.is_empty() {
            sub.push_str(&format!(" · {cc}"));
        }
        let tags = text("tags");
        if let Some(tag) = tags.split(',').map(str::trim).find(|t| {
            !t.is_empty() && !t.eq_ignore_ascii_case(&codec) && !t.eq_ignore_ascii_case("m3u8")
        }) {
            sub.push_str(&format!(" · {}", tag.chars().take(24).collect::<String>()));
        }
        out.push(cinder_ui::netradio::Found {
            name: name.trim().to_string(),
            url,
            sub,
        });
        if out.len() == 30 {
            break;
        }
    }
    out
}

/// Search radio-browser.info by station name, and by genre too when the names find few.
pub fn radio_search(query: &str, version: &str) -> Result<Vec<cinder_ui::netradio::Found>, String> {
    let ua = format!("User-Agent: Cinder/{version}");
    let ask = |field: &str| -> Result<Vec<cinder_ui::netradio::Found>, String> {
        let q = format!("{field}={query}");
        let (code, body) = run_curl(&[
            "-H",
            &ua,
            "--get",
            "--data-urlencode",
            &q,
            "--data-urlencode",
            "limit=60",
            "--data-urlencode",
            "hidebroken=true",
            "--data-urlencode",
            "order=clickcount",
            "--data-urlencode",
            "reverse=true",
            RADIO_BROWSER,
        ])?;
        if code != 200 {
            return Err(format!("radio-browser.info answered {code}"));
        }
        Ok(parse_radio_browser(&body))
    };
    let mut found = ask("name")?;
    if found.len() < 10 {
        for f in ask("tag").unwrap_or_default() {
            if found.len() < 30 && !found.iter().any(|g| g.url == f.url) {
                found.push(f);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four stations as radio-browser.info sent them (2026-11-05, searches "jazz" and "fip"; the
    /// fields read, as they came).
    const RB: &str = r#"[{"name": "Jazz Radio Blues", "url": "http://jazzblues.ice.infomaniak.ch/jazzblues-high.mp3", "url_resolved": "http://jazzblues.ice.infomaniak.ch/jazzblues-high.mp3", "codec": "MP3", "bitrate": 128, "countrycode": "FR", "tags": "blues,jazz", "hls": 0, "lastcheckok": 1}, {"name": "Adroit Jazz Underground HD Opus", "url": "https://icecast.walmradio.com:8443/jazz_opus", "url_resolved": "https://icecast.walmradio.com:8443/jazz_opus", "codec": "OGG", "bitrate": 192, "countrycode": "US", "tags": "avant-garde,bebop,big band", "hls": 0, "lastcheckok": 1}, {"name": "FIP", "url": "http://icecast.radiofrance.fr/fip-hifi.aac", "url_resolved": "http://icecast.radiofrance.fr/fip-hifi.aac", "codec": "AAC", "bitrate": 192, "countrycode": "FR", "tags": "aac,music,public radio,radio france", "hls": 0, "lastcheckok": 1}, {"name": "FIP (no pub)", "url": "https://stream.radiofrance.fr/fip/fip_hifi.m3u8", "url_resolved": "https://stream.radiofrance.fr/fip/fip_hifi.m3u8", "codec": "UNKNOWN", "bitrate": 0, "countrycode": "FR", "tags": "eclectic,m3u8,public radio,radio france", "hls": 1, "lastcheckok": 1}]"#;

    #[test]
    fn radio_browser_answers_become_stations_this_player_plays() {
        let f = parse_radio_browser(RB);
        let names: Vec<&str> = f.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["Jazz Radio Blues", "FIP", "FIP (no pub)"],
            "the Ogg/Opus one is left out"
        );
        assert_eq!(f[0].sub, "MP3 128 · FR · blues");
        assert_eq!(
            f[1].sub, "AAC 192 · FR · music",
            "the codec is not repeated as a tag"
        );
        assert_eq!(
            f[2].sub, "HLS · FR · eclectic",
            "HLS though it says UNKNOWN"
        );
        assert_eq!(f[2].url, "https://stream.radiofrance.fr/fip/fip_hifi.m3u8");
        let dup = format!("[{0},{0}]", &RB[1..RB.find("}, ").unwrap() + 1]);
        assert_eq!(parse_radio_browser(&dup).len(), 1, "one per stream");
        let piped = RB.replace("Jazz Radio Blues", "Jazz | Blues");
        assert_eq!(
            parse_radio_browser(&piped)[0].name,
            "Jazz  Blues",
            "a | would split the station file's line"
        );
        assert!(parse_radio_browser("not json").is_empty());
    }

    #[test]
    fn json_reads_nested_answers() {
        let v =
            Json::parse(r#" {"a": [1, -2.5e1, true, null, {"b": "x\"yé"}], "c": {}, "d": []} "#)
                .unwrap();
        let a = v.get("a").unwrap().items();
        assert_eq!(a[0].num(), Some(1.0));
        assert_eq!(a[1].num(), Some(-25.0));
        assert_eq!(a[2], Json::Bool(true));
        assert_eq!(a[3], Json::Null);
        assert_eq!(a[4].get("b").and_then(Json::str), Some("x\"y\u{e9}"));
        assert_eq!(v.get("c"), Some(&Json::Obj(vec![])));
        assert!(v.get("d").unwrap().items().is_empty());
        for bad in ["", "{", "[1,]", r#"{"a" 1}"#, "[1] 2", "nul", r#""open"#] {
            assert_eq!(Json::parse(bad), None, "{bad:?}");
        }
        assert_eq!(Json::parse(&"[".repeat(100)), None, "deep nesting refused");
    }

    /// The top of MusicBrainz's real answer for Pink Floyd's album (2026-11-05, trimmed to the
    /// fields read, the nesting kept): a fan compilation scores as high as the album.
    const MB_DSOTM: &str = r#"{"created":"2026-11-05T10:32:03.239Z","count":8,"offset":0,"release-groups":[
        {"id":"5dbefdbd-8bdc-4611-91e5-f0c0a2695612","score":100,"title":"Dark Side of the Moon [RESTORED]","primary-type":"Album","secondary-types":["Compilation"],"artist-credit":[{"name":"Pink Floyd","artist":{"id":"83d91898-7763-47d7-b03b-b92132375c47","name":"Pink Floyd","aliases":[{"name":"Pink Floid","locale":null,"primary":null}]}}]},
        {"id":"f5093c06-23e3-404f-aeaa-40f72885ee3a","score":100,"title":"The Dark Side of the Moon","primary-type":"Album","artist-credit":[{"name":"Pink Floyd","artist":{"id":"83d91898-7763-47d7-b03b-b92132375c47","name":"Pink Floyd"}}]},
        {"id":"ddb7c3c7-09b1-4bb3-bbcb-7c207cb2af06","score":95,"title":"The Dark Side of the Moon (demos)","primary-type":"Album","secondary-types":["Compilation","Demo"]},
        {"id":"2e4fb3a5-3e47-3e2a-bdd4-888944793e7c","score":92,"title":"The Dark Side of the Moon Live 1974","primary-type":"Album","secondary-types":["Live"]},
        {"id":"6cadcf33-98b6-3da1-b274-e3655bfb4361","score":88,"title":"Dark Side of the Moon: Limited Edition Trance Remix","primary-type":"Album","secondary-types":["Remix"]}]}"#;

    #[test]
    fn the_album_itself_not_a_compilation_of_the_same_name() {
        let album = "f5093c06-23e3-404f-aeaa-40f72885ee3a";
        for tag in [
            "Dark Side of the Moon",
            "The Dark Side of the Moon",
            "the dark side of the moon (2011 Remaster)",
            "The Dark Side Of The Moon [Remastered] (Deluxe)",
        ] {
            assert_eq!(mb_pick(MB_DSOTM, tag).as_deref(), Some(album), "{tag}");
        }
        assert_eq!(
            mb_pick(MB_DSOTM, "Wish You Were Here"),
            None,
            "no title match, no cover"
        );
        assert_eq!(
            mb_pick(MB_DSOTM, "(Live)"),
            None,
            "nothing left to match on"
        );
        let only_fan = MB_DSOTM.replace(
            "f5093c06-23e3-404f-aeaa-40f72885ee3a\",\"score\":100",
            "f5093c06-23e3-404f-aeaa-40f72885ee3a\",\"score\":80",
        );
        assert_eq!(
            mb_pick(&only_fan, "Dark Side of the Moon").as_deref(),
            Some("5dbefdbd-8bdc-4611-91e5-f0c0a2695612"),
            "with the album scored low, the same-titled compilation is the best there is"
        );
        let bad_id = MB_DSOTM.replace("f5093c06-23e3-404f-aeaa-40f72885ee3a", "../../x");
        assert_ne!(
            mb_pick(&bad_id, "The Dark Side of the Moon").as_deref(),
            Some("../../x"),
            "only a UUID reaches the URL"
        );
    }

    #[test]
    fn the_search_and_the_file_it_saves() {
        assert_eq!(
            mb_query(
                "Simon & Garfunkel",
                "Bridge Over \"Troubled\" Water (Remastered)"
            ),
            r#"releasegroup:"Bridge Over \"Troubled\" Water" AND artist:"Simon & Garfunkel""#
        );
        assert_eq!(
            bare_title("[Untitled]"),
            "[Untitled]",
            "a title that is all brackets stays"
        );
        assert_eq!(image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(image_ext(b"<html>"), None);
    }

    /// The scrobbler's own lines (scrobble::format_line) plus the shapes an uploader leaves.
    const LOG: &str = "#AUDIOSCROBBLER/1.1\n#TZ/UNKNOWN\n#CLIENT/Cinder\n\
        Massive Attack\tMezzanine\tTeardrop\t3\t330\tL\t1762300000\t\n\
        Burial\tUntrue\tArchangel\t2\t238\tS\t1762300400\t\n\
        Björk\t\tJóga \"live\"\t\t305\tL\t1762300800\t\n\
        \t\tNo artist\t1\t200\tL\t1762301000\t\n\
        Portishead\tDummy\tRoads\t10\t305\tL\t1762301200\t";

    #[test]
    fn only_finished_listens_newer_than_the_last_sent() {
        let l = listens(LOG, 0, 100);
        assert_eq!(
            l.len(),
            2,
            "skips, the artistless line and the unfinished last line are out"
        );
        assert_eq!(l[0].title, "Teardrop");
        assert_eq!((l[0].track_no, l[0].length_s), (3, 330));
        assert_eq!(l[1].artist, "Björk");
        assert_eq!(
            listens(LOG, 1762300000, 100).len(),
            1,
            "already sent is not sent again"
        );
        assert_eq!(listens(LOG, 0, 1).len(), 1, "batches are capped");
        assert!(listens("", 0, 10).is_empty());
    }

    #[test]
    fn the_json_is_utc_and_escaped() {
        let l = listens(LOG, 0, 100);
        let j = lb_json(&l, 3600, "0.3.22");
        assert!(
            j.starts_with("{\"listen_type\":\"import\",\"payload\":[{\"listened_at\":1762296400,")
        );
        assert!(j.contains("\"track_name\":\"Jóga \\\"live\\\"\""));
        assert!(j.contains("\"release_name\":\"Mezzanine\""));
        assert!(!j.contains("\"release_name\":\"\""), "no empty album");
        assert!(j.contains("\"tracknumber\":3") && j.contains("\"duration\":330"));
        assert_eq!(j.matches("listened_at").count(), 2);
        assert!(j.ends_with("}}}]}"));
        assert_eq!(json_str("a\tb"), "\"a\\u0009b\"");
    }

    #[test]
    fn tokens_and_versions() {
        assert_eq!(
            clean_token("  6a1c5e0e-7f1b-4c1d-9e4a-0a1b2c3d4e5f \r\nsecond line"),
            Some("6a1c5e0e-7f1b-4c1d-9e4a-0a1b2c3d4e5f".into())
        );
        assert_eq!(clean_token("short"), None);
        assert_eq!(clean_token("has space in it ok"), None);
        assert_eq!(parse_version("v0.3.23"), Some((0, 3, 23)));
        assert_eq!(parse_version("0.3.23-rc1"), Some((0, 3, 23)));
        assert_eq!(parse_version("edge"), None);
        assert_eq!(newer("v0.3.23", "0.3.22"), Some(true));
        assert_eq!(newer("v0.3.22", "0.3.22"), Some(false));
        assert_eq!(newer("v0.4.0", "0.3.22"), Some(true));
        assert_eq!(newer("v0.3.9", "0.3.22"), Some(false), "numbers, not text");
        let body =
            "{\n  \"url\": \"x\",\n  \"tag_name\": \"v0.3.23\",\n  \"name\": \"Cinder 0.3.23\"\n}";
        assert_eq!(tag_name(body).as_deref(), Some("v0.3.23"));
        assert_eq!(tag_name("{}"), None);
        assert!(
            curl_error(60).contains("clock"),
            "a certificate failure points at the clock"
        );
        assert!(curl_error(6).starts_with("No internet"));
    }

    /// An answer shaped like lrclib.net/api/get's (copied 2026-11-05, words shortened).
    #[test]
    fn lyrics_from_an_lrclib_answer() {
        let body = r#"{"id":3396226,"trackName":"Teardrop","artistName":"Massive Attack","albumName":"Mezzanine","duration":330.0,"instrumental":false,"plainLyrics":"Love, love is a verb\nLove is a doing word","syncedLyrics":"[00:59.89] Love, love is a verb\n[01:03.04] Love is a doing word é🎵"}"#;
        let lrc = lrc_from_answer(body).unwrap();
        assert!(lrc.starts_with("[re:LRCLIB via Cinder]\n[00:59.89] Love, love is a verb\n"));
        assert!(
            lrc.contains("word é🎵"),
            "\\u escapes and a surrogate pair: {lrc}"
        );
        let plain = r#"{"instrumental":false,"plainLyrics":"Just words","syncedLyrics":null}"#;
        assert_eq!(
            lrc_from_answer(plain).unwrap(),
            "[re:LRCLIB via Cinder]\nJust words\n"
        );
        assert_eq!(
            lrc_from_answer(r#"{"instrumental":true,"plainLyrics":null}"#),
            None
        );
        assert_eq!(
            lrc_from_answer(r#"{"plainLyrics":"","syncedLyrics":""}"#),
            None
        );
        assert_eq!(
            lrc_path("/contents/MUSIC/A/01 Song.flac"),
            "/contents/MUSIC/A/01 Song.lrc"
        );
        assert_eq!(
            lrc_path("/contents/MUSIC/A.b/Song"),
            "/contents/MUSIC/A.b/Song.lrc"
        );
        assert_eq!(
            json_field(r#"{"a" : "x\"y"}"#, "a").as_deref(),
            Some("x\"y")
        );
        assert_eq!(json_field(r#"{"a":null}"#, "a"), None);
    }
}
