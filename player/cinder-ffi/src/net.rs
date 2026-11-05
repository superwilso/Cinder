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
/// flat answer; not a general parser.
pub fn json_field(body: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\"");
    let mut rest = &body[body.find(&k)? + k.len()..];
    rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut it = rest.chars();
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

#[cfg(test)]
mod tests {
    use super::*;

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
