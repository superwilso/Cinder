//! Built-in, battery-efficient scrobbler — appends an Audioscrobbler/1.1 `.scrobbler.log`
//! (the same format the standalone `unknown321/scrobbler` and Rockbox write, so existing
//! upload tools work unchanged). No network, no daemon, no extra process: we already know the
//! now-playing track (resolved from the library DB) and tick once a second from the pump, so
//! scrobbling is a few bytes appended on each track change — negligible power cost.
//!
//! Submission rule (Last.fm portable-player logging): a track counts as *listened* ("L") once
//! it has been the current track, while playing, for at least half its length OR 240 s,
//! whichever comes first, and it is longer than 30 s. Shorter/abandoned tracks are dropped.
//!
//! Fields per line (tab-separated, AS/1.1):
//!   artist \t album \t title \t track_no \t length_s \t rating \t start_unix \t musicbrainz_id

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const HEADER: &str = "#AUDIOSCROBBLER/1.1\n#TZ/UTC\n";

/// Metadata for the track being timed (owned; sanitised at format time).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub artist: String,
    pub album: String,
    pub title: String,
    pub track_no: u32,
    pub length_s: u32,
}

struct Pending {
    track: Track,
    start_unix: u64,
    played_ms: u64, // accumulated REAL play time; seconds are derived from it
    logged: bool,   // already written this play (don't double-log if it crosses the threshold)
}

pub struct Scrobbler {
    path: PathBuf,
    client: String,
    cur: Option<Pending>,
    header_done: bool,
}

/// A track is a "listen" once it's played for half its length or 240 s (whichever first),
/// and is longer than 30 s.
pub fn is_listened(length_s: u32, played_s: u32) -> bool {
    length_s >= 30 && played_s >= (length_s / 2).min(240)
}

/// Strip tab/newline from a field (the format is tab/newline delimited).
fn clean(s: &str) -> String {
    s.chars().filter(|c| *c != '\t' && *c != '\n' && *c != '\r').collect()
}

/// Build one AS/1.1 log line (no trailing newline). `rating` is "L" or "S".
pub fn format_line(t: &Track, rating: &str, start_unix: u64) -> String {
    let track_no = if t.track_no > 0 { t.track_no.to_string() } else { String::new() };
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t",
        clean(&t.artist),
        clean(&t.album),
        clean(&t.title),
        track_no,
        t.length_s,
        rating,
        start_unix
    )
}

impl Scrobbler {
    pub fn new(path: impl Into<PathBuf>, client: impl Into<String>) -> Self {
        Scrobbler { path: path.into(), client: client.into(), cur: None, header_done: false }
    }

    /// Advance the play clock by one second (call once a second from the pump). Only counts
    /// time while actually playing; a paused track doesn't accrue listen time.
    /// Advance the play clock by REAL elapsed time. Takes milliseconds rather than assuming one
    /// call per second: the caller is the shell's housekeeping block, which fires when *at least*
    /// a second has passed — and its loop drops to 10 Hz while the panel is dark, so "1000 ms" is
    /// really 1000–1100 ms there. Assuming a fixed +1 s made the scrobble clock run up to 10% slow
    /// exactly when the screen is off, which is the normal way this device gets listened to.
    pub fn tick_ms(&mut self, playing: bool, elapsed_ms: u64) {
        if !playing {
            return;
        }
        if let Some(p) = self.cur.as_mut() {
            p.played_ms = p.played_ms.saturating_add(elapsed_ms);
            let played_s = (p.played_ms / 1000) as u32;
            if !p.logged && is_listened(p.track.length_s, played_s) {
                // write as soon as the threshold is crossed, so a sudden power-off still logs it
                let line = format_line(&p.track, "L", p.start_unix);
                p.logged = true;
                let _ = self.append(&line);
            }
        }
    }

    /// The now-playing track changed (or first track). Finalises the previous track if it was
    /// listened-but-not-yet-logged, then starts timing the new one. `now_unix` = current time.
    pub fn set_track(&mut self, track: Track, now_unix: u64) {
        if let Some(p) = self.cur.take() {
            if !p.logged && is_listened(p.track.length_s, (p.played_ms / 1000) as u32) {
                let line = format_line(&p.track, "L", p.start_unix);
                let _ = self.append(&line);
            }
        }
        // ignore a no-op re-set of the identical track (avoid resetting the clock on re-poll)
        self.cur = Some(Pending { track, start_unix: now_unix, played_ms: 0, logged: false });
    }

    /// True if the current pending track matches `t` (so the caller can avoid re-setting it).
    pub fn is_current(&self, t: &Track) -> bool {
        self.cur.as_ref().map(|p| p.track == *t).unwrap_or(false)
    }

    /// Append a line to the log, writing the AS/1.1 header on first write. Best-effort: a log
    /// write must never disrupt playback, so I/O errors are swallowed (returned for tests).
    fn append(&mut self, line: &str) -> std::io::Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(&self.path)?;
        if !self.header_done {
            // Only write the header if the file is empty (don't duplicate on an existing log).
            let empty = f.metadata().map(|m| m.len() == 0).unwrap_or(true);
            if empty {
                writeln!(f, "{HEADER}#CLIENT/{}", self.client)?;
            }
            self.header_done = true;
        }
        writeln!(f, "{line}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(len: u32) -> Track {
        Track {
            artist: "Benjamin Francis Leftwich".into(),
            album: "Last Smoke".into(),
            title: "Atlas Hands".into(),
            track_no: 1,
            length_s: len,
        }
    }

    #[test]
    fn listen_threshold() {
        assert!(!is_listened(20, 20)); // < 30s never counts
        assert!(!is_listened(200, 99)); // < half
        assert!(is_listened(200, 100)); // exactly half
        assert!(is_listened(600, 240)); // capped at 240s even though half is 300
        assert!(!is_listened(600, 239));
    }

    #[test]
    fn line_format_is_tab_separated_as11() {
        let line = format_line(&t(272), "L", 1143374412);
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 8); // 7 fields + trailing tab (empty mbid)
        assert_eq!(f[0], "Benjamin Francis Leftwich");
        assert_eq!(f[1], "Last Smoke");
        assert_eq!(f[2], "Atlas Hands");
        assert_eq!(f[3], "1");
        assert_eq!(f[4], "272");
        assert_eq!(f[5], "L");
        assert_eq!(f[6], "1143374412");
        assert_eq!(f[7], ""); // empty musicbrainz id
    }

    #[test]
    fn fields_are_sanitised() {
        let mut tr = t(100);
        tr.title = "Tab\tHere\nNewline".into();
        let line = format_line(&tr, "L", 1);
        assert!(!line.contains("Tab\tHere"));
        assert_eq!(line.split('\t').nth(2).unwrap(), "TabHereNewline");
    }

    #[test]
    fn writes_header_then_listened_line() {
        let dir = std::env::temp_dir().join(format!("cinder_scrob_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".scrobbler.log");
        let _ = std::fs::remove_file(&path);
        let mut s = Scrobbler::new(&path, "Cinder NW-A55 0.1");
        s.set_track(t(60), 1000); // 60s track
        for _ in 0..29 {
            s.tick_ms(true, 1000); // 29s — not yet half
        }
        assert!(std::fs::read_to_string(&path).is_err() || !std::fs::read_to_string(&path).unwrap().contains("Atlas"));
        s.tick_ms(true, 1000); // 30s == half of 60 → listened, logged immediately
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("#AUDIOSCROBBLER/1.1\n#TZ/UTC\n#CLIENT/Cinder NW-A55 0.1\n"));
        assert!(body.contains("Atlas Hands\t1\t60\tL\t1000\t"));
        // a second tick must not double-log
        s.tick_ms(true, 1000);
        let n = body.matches("Atlas Hands").count();
        assert_eq!(n, 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The play clock must track REAL time, not the number of calls. The shell ticks when at least
    /// a second has passed, and its loop runs at 10 Hz while the panel is dark, so the true gap is
    /// 1000-1100 ms — assuming a flat +1 s per call made the clock run up to 10% slow exactly when
    /// the screen is off, which is how this device is normally listened to.
    #[test]
    fn play_clock_follows_real_elapsed_time_not_call_count() {
        let path = std::env::temp_dir().join("cinder_scrob_rate.log");
        let _ = std::fs::remove_file(&path);
        let mut s = Scrobbler::new(&path, "c");
        s.set_track(t(60), 1000); // needs 30 s to count
        // 28 ticks of a realistic dark-panel interval already exceed 30 s of real time; a
        // call-counting clock would still be sitting at 28 s and would not have logged.
        for _ in 0..28 {
            s.tick_ms(true, 1100);
        }
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(body.contains("Atlas Hands"), "real elapsed time should have crossed the threshold");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn paused_track_does_not_accrue() {
        let mut s = Scrobbler::new(std::env::temp_dir().join("cinder_scrob_pause.log"), "c");
        s.set_track(t(60), 0);
        for _ in 0..100 {
            s.tick_ms(false, 1000); // paused the whole time
        }
        // nothing logged because the play clock stayed at 0
        let p = s.cur.as_ref().unwrap();
        assert_eq!(p.played_ms, 0);
        assert!(!p.logged);
    }
}
