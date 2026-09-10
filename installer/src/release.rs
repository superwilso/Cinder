//! "Is there a newer Cinder?" — asked of the GitHub releases API, and only when the user asks.
//!
//! DELIBERATELY NOT AUTOMATIC. This program is run by someone plugging a music player into a PC;
//! it has no business making a network request they did not ask for. The check is a button, it
//! says where it is about to connect, and the whole rest of the installer works with no network
//! at all — the payload is embedded, so an install on a machine that has never been online is a
//! completely normal install.
//!
//! NO HTTP CRATE. On Windows the OS ships one (WinINet, which is what Windows Update itself
//! rides); elsewhere `curl` or `wget` is shelled out to. Adding an HTTPS stack to a dependency-
//! free binary in order to display a version number is not a trade worth making.

/// Where the check goes. Shown to the user before it is made.
pub const LATEST_URL: &str = "https://api.github.com/repos/superwilso/Cinder/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/superwilso/Cinder/releases/latest";

pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
}

/// Compare two release tags. `true` means `candidate` is strictly newer than `current`.
///
/// Tags are `v1.2.3`, optionally with a `-rc1`-style suffix. Numeric fields are compared as
/// NUMBERS: string comparison puts "v0.10.0" before "v0.9.0", which would hide exactly the
/// release a user most wants. A pre-release suffix sorts before the plain version of the same
/// numbers, so v1.0.0 beats v1.0.0-rc1.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (cn, cpre) = split_tag(candidate);
    let (un, upre) = split_tag(current);
    let len = cn.len().max(un.len());
    for i in 0..len {
        let (a, b) = (cn.get(i).copied().unwrap_or(0), un.get(i).copied().unwrap_or(0));
        if a != b {
            return a > b;
        }
    }
    // Same numbers: a release beats a pre-release, and neither beats itself.
    match (cpre.is_empty(), upre.is_empty()) {
        (true, false) => true,
        (false, true) => false,
        _ => cpre > upre,
    }
}

/// `v1.2.3-rc1` -> (`[1,2,3]`, `"rc1"`). Anything unparseable in the numeric part reads as 0
/// rather than failing the whole comparison.
fn split_tag(tag: &str) -> (Vec<u64>, String) {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let (nums, pre) = match t.split_once('-') {
        Some((a, b)) => (a, b.to_string()),
        None => (t, String::new()),
    };
    (nums.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect(), pre)
}

/// Pull one string field out of a flat JSON object.
///
/// A whole JSON parser is not needed to read three top-level strings, but the naive version —
/// find the key, take everything to the next quote — breaks on the first escaped quote or
/// backslash in a release name, and GitHub release names contain both. This walks the value
/// properly and unescapes it.
pub fn json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(rel) = body[from..].find(&needle) {
        let at = from + rel + needle.len();
        from = at;
        let rest = body[at..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue; // the key appeared as a VALUE somewhere, not as a key
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue; // null, or a number — not the string we are after
        };
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => {}
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            Some(ch) => out.push(ch),
                            None => return None,
                        }
                    }
                    Some(other) => out.push(other),
                    None => return None,
                },
                other => out.push(other),
            }
        }
        return None; // ran off the end of the body inside a string
    }
    None
}

pub fn parse_release(body: &str) -> Result<Release, String> {
    let tag = json_string(body, "tag_name")
        .ok_or_else(|| "the reply had no tag_name — GitHub may have rate-limited it".to_string())?;
    let name = json_string(body, "name").unwrap_or_else(|| tag.clone());
    let url = json_string(body, "html_url").unwrap_or_else(|| RELEASES_PAGE.to_string());
    Ok(Release { tag, name, url })
}

/// Ask GitHub what the latest release is. Blocking; callers run it off the UI thread.
pub fn latest() -> Result<Release, String> {
    parse_release(&fetch(LATEST_URL)?)
}

// ── the transport ──────────────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn fetch(url: &str) -> Result<String, String> {
    use std::os::raw::{c_char, c_void};

    type Handle = *mut c_void;
    const INTERNET_OPEN_TYPE_PRECONFIG: u32 = 0;
    const INTERNET_FLAG_RELOAD: u32 = 0x8000_0000;
    const INTERNET_FLAG_SECURE: u32 = 0x0080_0000;
    const INTERNET_FLAG_NO_UI: u32 = 0x0000_0200;

    #[link(name = "wininet")]
    extern "system" {
        fn InternetOpenA(
            agent: *const c_char,
            access: u32,
            proxy: *const c_char,
            bypass: *const c_char,
            flags: u32,
        ) -> Handle;
        fn InternetOpenUrlA(
            session: Handle,
            url: *const c_char,
            headers: *const c_char,
            headers_len: u32,
            flags: u32,
            context: usize,
        ) -> Handle;
        fn InternetReadFile(file: Handle, buf: *mut u8, len: u32, read: *mut u32) -> i32;
        fn InternetCloseHandle(h: Handle) -> i32;
    }

    // GitHub rejects requests with no User-Agent outright, and asks for an explicit API version.
    let agent = b"cinder-installer\0";
    let headers = b"Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n\0";
    let mut url_z = url.as_bytes().to_vec();
    url_z.push(0);

    // SAFETY: every pointer below is either null or a NUL-terminated local that outlives the call.
    // Both handles are closed on every path out, including the error paths.
    unsafe {
        let session = InternetOpenA(
            agent.as_ptr().cast(),
            INTERNET_OPEN_TYPE_PRECONFIG,
            std::ptr::null(),
            std::ptr::null(),
            0,
        );
        if session.is_null() {
            return Err("could not start a network session".into());
        }
        let req = InternetOpenUrlA(
            session,
            url_z.as_ptr().cast(),
            headers.as_ptr().cast(),
            (headers.len() - 1) as u32,
            INTERNET_FLAG_RELOAD | INTERNET_FLAG_SECURE | INTERNET_FLAG_NO_UI,
            0,
        );
        if req.is_null() {
            InternetCloseHandle(session);
            return Err("could not reach github.com — check the connection or a proxy".into());
        }
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let mut got: u32 = 0;
            if InternetReadFile(req, buf.as_mut_ptr(), buf.len() as u32, &mut got) == 0 {
                InternetCloseHandle(req);
                InternetCloseHandle(session);
                return Err("the connection dropped part-way through".into());
            }
            if got == 0 {
                break;
            }
            out.extend_from_slice(&buf[..got as usize]);
            // A release object is a few KB. Anything past this is not the reply we asked for.
            if out.len() > 1 << 20 {
                break;
            }
        }
        InternetCloseHandle(req);
        InternetCloseHandle(session);
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

#[cfg(not(windows))]
fn fetch(url: &str) -> Result<String, String> {
    use std::process::Command;

    // Order matters only in that curl is the likelier of the two to be present.
    let attempts: [(&str, Vec<&str>); 2] = [
        ("curl", vec!["-sSfL", "--max-time", "20", "-A", "cinder-installer", url]),
        ("wget", vec!["-qO-", "--timeout=20", "-U", "cinder-installer", url]),
    ];
    let mut why = String::new();
    for (bin, args) in attempts {
        match Command::new(bin).args(&args).output() {
            Ok(o) if o.status.success() => return Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
            Ok(o) => {
                why = format!("{bin} exited with {}: {}", o.status, String::from_utf8_lossy(&o.stderr).trim());
            }
            Err(e) => why = format!("{bin}: {e}"),
        }
    }
    Err(if why.is_empty() {
        "no curl or wget on PATH, so the version check cannot run here".into()
    } else {
        format!("could not reach github.com ({why})")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// String comparison puts "0.10.0" before "0.9.0". A user on 0.9.0 would then be told they
    /// were up to date on the day the release they wanted came out.
    #[test]
    fn ten_is_newer_than_nine() {
        assert!(is_newer("v0.10.0", "v0.9.0"));
        assert!(!is_newer("v0.9.0", "v0.10.0"));
    }

    #[test]
    fn the_same_version_is_not_newer() {
        assert!(!is_newer("v1.2.3", "v1.2.3"));
        assert!(!is_newer("v1.2.3", "1.2.3"), "the leading v is cosmetic");
    }

    #[test]
    fn missing_fields_read_as_zero() {
        assert!(is_newer("v1.1", "v1.0.9"));
        assert!(!is_newer("v1.0", "v1.0.0"));
    }

    #[test]
    fn a_release_beats_its_own_prerelease() {
        assert!(is_newer("v1.0.0", "v1.0.0-rc1"));
        assert!(!is_newer("v1.0.0-rc1", "v1.0.0"));
        assert!(is_newer("v1.0.0-rc2", "v1.0.0-rc1"));
    }

    #[test]
    fn a_later_prerelease_still_beats_an_older_release() {
        assert!(is_newer("v1.1.0-rc1", "v1.0.0"));
    }

    #[test]
    fn junk_in_a_tag_does_not_panic() {
        assert!(!is_newer("", ""));
        assert!(!is_newer("nightly", "v1.0.0"));
        assert!(is_newer("v2.x", "v1.0.0"), "the major still parses");
    }

    #[test]
    fn reads_the_fields_it_needs() {
        let body = r#"{"url":"x","tag_name":"v0.3.0","name":"Cinder 0.3.0","html_url":"https://e/1"}"#;
        let r = parse_release(body).unwrap();
        assert_eq!(r.tag, "v0.3.0");
        assert_eq!(r.name, "Cinder 0.3.0");
        assert_eq!(r.url, "https://e/1");
    }

    /// Release names contain quotes and backslashes. Taking everything up to the next `"` cuts
    /// the name in half and, worse, can run the scan past the end of the value.
    #[test]
    fn escapes_inside_a_string_do_not_end_it() {
        let body = r#"{"tag_name":"v1.0.0","name":"the \"big\" one \\ back to back"}"#;
        assert_eq!(json_string(body, "name").as_deref(), Some(r#"the "big" one \ back to back"#));
    }

    #[test]
    fn decodes_unicode_escapes() {
        let body = r#"{"name":"café"}"#;
        assert_eq!(json_string(body, "name").as_deref(), Some("café"));
    }

    /// GitHub's error replies are JSON too, and have no tag_name. Reporting "no tag_name" beats
    /// showing an empty version box.
    #[test]
    fn a_rate_limit_reply_is_an_error_not_an_empty_version() {
        let body = r#"{"message":"API rate limit exceeded","documentation_url":"https://d"}"#;
        assert!(parse_release(body).is_err());
    }

    /// The key name appearing inside somebody else's value must not be mistaken for the key.
    #[test]
    fn a_key_name_inside_a_value_is_not_the_key() {
        let body = r#"{"body":"mentions \"tag_name\" in prose","tag_name":"v9.9.9"}"#;
        assert_eq!(json_string(body, "tag_name").as_deref(), Some("v9.9.9"));
    }

    #[test]
    fn a_null_field_falls_back_rather_than_yielding_null() {
        let body = r#"{"tag_name":"v1.0.0","name":null}"#;
        let r = parse_release(body).unwrap();
        assert_eq!(r.name, "v1.0.0", "a null name falls back to the tag");
    }

    #[test]
    fn a_truncated_reply_is_an_error() {
        assert!(parse_release(r#"{"tag_name":"v1.0.0"#).is_err());
    }
}
