//! Track stats — the owner's ratings, and how often and when each track was last played.
//!
//! Sony's database has nowhere Cinder may write these (it is held open by Sony's services, rebuilt
//! by every rescan, and its object ids are re-issued with it — the reasons `playlists.rs` gives),
//! so Cinder keeps its own file, the same way it keeps liked songs and playlists:
//!
//! **`/contents/cinder_stats.tsv`** — one line per track that has anything to say:
//!
//! ```text
//! #CINDER-STATS/1
//! # path<TAB>rating<TAB>plays<TAB>last_played
//! /contents/MUSIC/Wunderhorse - Cub/06 - Teal.flac<TAB>5<TAB>12<TAB>1790000000
//! ```
//!
//! * keyed by **file path**, so a database rebuild cannot lose a rating;
//! * `rating` is 0..=5 stars (0 = not rated), `plays` a count, `last_played` unix seconds as the
//!   player's clock had them (the same local-time-as-epoch the scrobbler log carries), 0 = never;
//! * times are `i64` end to end — nothing here wraps in 2038;
//! * on `/contents`, the volume Windows mounts, so it can be backed up, read by Flint, and edited
//!   by hand;
//! * written through a temporary file and a rename, so a pulled cable leaves the old file or the
//!   new one and never half of either.
//!
//! A play is counted by the scrobbler's own rule (`scrobble::is_listened`): half the track or four
//! minutes, whichever comes first, on a track over thirty seconds. It is counted whether or not the
//! scrobble LOG is being written — that is an install option; this is the library's own memory.
//!
//! The full contract, including what a PC tool may do with the file, is `docs/TRACK_DATA.md`.

use cinder_ui::model::TrackStat;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PATH: &str = "/contents/cinder_stats.tsv";
const HEADER: &str = "#CINDER-STATS/1\n# path\trating\tplays\tlast_played\n";

/// The store: every track with a rating or a play, by path. A `BTreeMap`, so the file is written
/// in path order and two saves of the same state are the same bytes.
#[derive(Debug, Default)]
pub struct Store {
    path: PathBuf,
    by_path: BTreeMap<String, TrackStat>,
    /// Changed since the last write.
    dirty: bool,
    /// The paths changed since the last write — what a merge keeps from this side.
    touched: std::collections::BTreeSet<String>,
    /// Those of `touched` whose RATING was set here (a counted play alone does not vote on it).
    rated: std::collections::BTreeSet<String>,
    /// A fingerprint of the file as this store last read or wrote it, so `flush` can tell that it
    /// has changed underneath (0 = there was no file).
    disk_sig: u64,
}

/// FNV-1a over the file's bytes: enough to say "this is not the file I last saw".
fn sig(body: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in body {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h | 1
}

/// Parse the file's text. Lines that do not read — a comment, a blank, a short or non-numeric row —
/// are skipped, one at a time: a damaged line costs that track's stats, never the file's.
pub fn parse(body: &str) -> BTreeMap<String, TrackStat> {
    let mut out = BTreeMap::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut f = line.split('\t');
        let (Some(path), Some(rating), Some(plays)) = (f.next(), f.next(), f.next()) else { continue };
        let (Ok(rating), Ok(plays)) = (rating.trim().parse::<u8>(), plays.trim().parse::<u32>()) else { continue };
        // A fourth column missing is "never" rather than a bad line: a hand-made file of ratings
        // need not invent a date.
        let last_played = f.next().and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0).max(0);
        let st = TrackStat { rating: rating.min(5), plays, last_played };
        if !path.is_empty() && st != TrackStat::default() {
            out.insert(path.to_string(), st);
        }
    }
    out
}

/// The file's text for `stats`.
pub fn serialize(stats: &BTreeMap<String, TrackStat>) -> String {
    let mut s = String::from(HEADER);
    for (path, st) in stats {
        // A path cannot hold a tab or a newline on this volume; strip them anyway, because the
        // format is delimited by exactly those two.
        let path: String = path.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
        s.push_str(&format!("{path}\t{}\t{}\t{}\n", st.rating, st.plays, st.last_played));
    }
    s
}

impl Store {
    /// Read the store at `path`. A missing file is an empty store — the state of every player
    /// until its first rating or counted play.
    pub fn open(path: impl AsRef<Path>) -> Store {
        let path = path.as_ref().to_path_buf();
        let body = std::fs::read(&path).ok();
        let by_path = body.as_ref().map(|b| parse(&String::from_utf8_lossy(b))).unwrap_or_default();
        Store {
            path,
            by_path,
            dirty: false,
            touched: Default::default(),
            rated: Default::default(),
            disk_sig: body.as_ref().map_or(0, |b| sig(b)),
        }
    }

    pub fn get(&self, path: &str) -> TrackStat {
        self.by_path.get(path).copied().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    fn put(&mut self, path: &str, st: TrackStat) {
        if path.is_empty() || self.get(path) == st {
            return;
        }
        if st == TrackStat::default() {
            self.by_path.remove(path);
        } else {
            self.by_path.insert(path.to_string(), st);
        }
        self.touched.insert(path.to_string());
        self.dirty = true;
    }

    /// Set a rating, 0..=5 (0 clears it). Returns the track's stats after the change.
    pub fn rate(&mut self, path: &str, stars: u8) -> TrackStat {
        let st = TrackStat { rating: stars.min(5), ..self.get(path) };
        if self.get(path) != st {
            self.rated.insert(path.to_string());
        }
        self.put(path, st);
        st
    }

    /// Count one listen at `now` (unix seconds). Returns the track's stats after the change.
    pub fn count_play(&mut self, path: &str, now: i64) -> TrackStat {
        let old = self.get(path);
        let st = TrackStat { plays: old.plays.saturating_add(1), last_played: now.max(0), ..old };
        self.put(path, st);
        st
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Write the file if anything changed: a temporary file beside it, then a rename over it. On a
    /// failed write the store stays dirty, so the next call tries again — the volume may simply be
    /// with the PC for the moment.
    ///
    /// **The file on disk is merged first when it is not the one this store last saw.** Two ways
    /// that happens, and both would otherwise lose data: `/contents` is unreadable for the first
    /// seconds of a boot, so a store opened then is empty while the real file is not; and a PC tool
    /// may rewrite the file while the volume is handed over. The disk's version wins for every
    /// track this side has not changed since its last write; for the ones it has, this side's
    /// rating stands (if it was set here) and the play count and date take the larger of the two.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Ok(body) = std::fs::read(&self.path) {
            if sig(&body) != self.disk_sig {
                let mut merged = parse(&String::from_utf8_lossy(&body));
                for path in &self.touched {
                    let mine = self.get(path);
                    let theirs = merged.get(path).copied().unwrap_or_default();
                    let st = TrackStat {
                        rating: if self.rated.contains(path) { mine.rating } else { theirs.rating },
                        plays: mine.plays.max(theirs.plays),
                        last_played: mine.last_played.max(theirs.last_played),
                    };
                    if st == TrackStat::default() {
                        merged.remove(path);
                    } else {
                        merged.insert(path.clone(), st);
                    }
                }
                self.by_path = merged;
            }
        }
        let body = serialize(&self.by_path);
        let tmp = self.path.with_extension("tsv.tmp");
        std::fs::write(&tmp, &body)?;
        std::fs::rename(&tmp, &self.path)?;
        self.disk_sig = sig(body.as_bytes());
        self.touched.clear();
        self.rated.clear();
        self.dirty = false;
        Ok(())
    }

    /// The stats of the tracks in `tracks` — `(object id, path)` pairs — keyed by object id, for
    /// `cinder_ui::Library::stats`. Exact path matches only: a rating must never land on a
    /// different file that happens to share a name.
    pub fn by_object_id<'a>(
        &self,
        tracks: impl Iterator<Item = (i64, &'a str)>,
    ) -> std::collections::HashMap<i64, TrackStat> {
        if self.by_path.is_empty() {
            return Default::default();
        }
        tracks.filter_map(|(id, path)| self.by_path.get(path).map(|st| (id, *st))).collect()
    }
}

/// The listen clock: the scrobbler's rule, kept separately so a play is counted even when the
/// scrobble log is switched off at install.
#[derive(Debug, Default)]
pub struct Listen {
    /// `(path, length in seconds)` of the track being timed.
    cur: Option<(String, u32)>,
    played_ms: u64,
    counted: bool,
}

impl Listen {
    /// The now-playing track changed. The same track again is not a new listen.
    pub fn set_track(&mut self, path: &str, length_s: u32) {
        if self.cur.as_ref().is_some_and(|(p, _)| p == path) {
            return;
        }
        self.cur = Some((path.to_string(), length_s));
        self.played_ms = 0;
        self.counted = false;
    }

    /// Advance by real elapsed time. Returns the path of a track that has JUST become a listen —
    /// once per play, the moment the threshold is crossed, so a sudden power-off still counts it.
    pub fn tick_ms(&mut self, playing: bool, elapsed_ms: u64) -> Option<&str> {
        if !playing || self.counted {
            return None;
        }
        let (path, length_s) = self.cur.as_ref()?;
        self.played_ms = self.played_ms.saturating_add(elapsed_ms);
        if crate::scrobble::is_listened(*length_s, (self.played_ms / 1000) as u32) {
            self.counted = true;
            return Some(path.as_str());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cinder_stats_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("cinder_stats.tsv")
    }

    #[test]
    fn ratings_and_plays_survive_a_reopen() {
        let p = tmp("reopen");
        let mut s = Store::open(&p);
        assert_eq!(s.len(), 0, "no file is an empty store");
        s.rate("/contents/MUSIC/a.flac", 4);
        s.count_play("/contents/MUSIC/a.flac", 1_790_000_000);
        s.count_play("/contents/MUSIC/a.flac", 1_790_000_300);
        s.count_play("/contents/MUSIC/b.mp3", 5_000_000_000); // past 2038: i64, not i32
        s.flush().unwrap();
        assert!(!s.is_dirty());

        let r = Store::open(&p);
        assert_eq!(r.get("/contents/MUSIC/a.flac"), TrackStat { rating: 4, plays: 2, last_played: 1_790_000_300 });
        assert_eq!(r.get("/contents/MUSIC/b.mp3").last_played, 5_000_000_000);
        assert_eq!(r.get("/contents/MUSIC/never.flac"), TrackStat::default());
        // The temporary file is gone: the rename is the write.
        assert!(!p.with_extension("tsv.tmp").exists());
    }

    #[test]
    fn clearing_the_last_fact_removes_the_line() {
        let p = tmp("clear");
        let mut s = Store::open(&p);
        s.rate("/x/a.flac", 3);
        s.flush().unwrap();
        s.rate("/x/a.flac", 0);
        assert!(s.is_dirty());
        s.flush().unwrap();
        let body = std::fs::read_to_string(&p).unwrap();
        assert_eq!(body, HEADER, "an unrated, unplayed track has no line");
        // Setting what is already set is not a change, so it is not a write.
        s.rate("/x/a.flac", 0);
        assert!(!s.is_dirty());
    }

    #[test]
    fn a_damaged_line_costs_one_track_not_the_file() {
        let body = "#CINDER-STATS/1\n/a.flac\t5\t3\t100\nnot a row\n/b.flac\tfive\t1\t1\n/c.flac\t9\t2\n\r\n/d.flac\t0\t0\t0\n";
        let m = parse(body);
        assert_eq!(m.len(), 2, "{m:?}");
        assert_eq!(m["/a.flac"], TrackStat { rating: 5, plays: 3, last_played: 100 });
        assert_eq!(m["/c.flac"], TrackStat { rating: 5, plays: 2, last_played: 0 }, "clamped, and no date is never");
        assert_eq!(parse(&serialize(&m)), m, "round trip");
    }

    /// The boot window: the store opened while `/contents` could not be read, so it is empty, and
    /// the real file turns up before the first write. That write must add to it, not replace it.
    /// The same merge takes in a file a PC tool rewrote while it had the volume.
    #[test]
    fn a_file_that_changed_underneath_is_merged_not_overwritten() {
        let p = tmp("merge");
        let mut s = Store::open(&p); // nothing readable yet
        std::fs::write(&p, "#CINDER-STATS/1\n/a.flac\t5\t10\t100\n/b.flac\t2\t7\t900\n").unwrap();
        s.count_play("/b.flac", 500); // plays here: 1, there: 7
        s.rate("/c.flac", 3);
        s.flush().unwrap();
        let r = Store::open(&p);
        assert_eq!(r.get("/a.flac"), TrackStat { rating: 5, plays: 10, last_played: 100 }, "untouched: kept");
        assert_eq!(r.get("/b.flac"), TrackStat { rating: 2, plays: 7, last_played: 900 }, "the larger count and the later date");
        assert_eq!(r.get("/c.flac").rating, 3);
        // A second write with nothing changed underneath merges nothing and loses nothing.
        s.rate("/a.flac", 1);
        s.flush().unwrap();
        assert_eq!(Store::open(&p).get("/a.flac"), TrackStat { rating: 1, plays: 10, last_played: 100 });
    }

    #[test]
    fn stats_reach_the_ui_by_object_id_on_exact_paths_only() {
        let p = tmp("ids");
        let mut s = Store::open(&p);
        s.rate("/contents/MUSIC/A/01 Intro.flac", 5);
        let tracks = [(7i64, "/contents/MUSIC/A/01 Intro.flac"), (8, "/contents/MUSIC/B/01 Intro.flac")];
        let m = s.by_object_id(tracks.iter().map(|(id, p)| (*id, *p)));
        assert_eq!(m.len(), 1);
        assert_eq!(m[&7].rating, 5);
    }

    #[test]
    fn a_listen_is_counted_once_at_the_threshold() {
        let mut l = Listen::default();
        l.set_track("/a.flac", 60);
        for _ in 0..29 {
            assert_eq!(l.tick_ms(true, 1000), None);
        }
        assert_eq!(l.tick_ms(false, 60_000), None, "paused time is not listening");
        assert_eq!(l.tick_ms(true, 1000), Some("/a.flac"));
        assert_eq!(l.tick_ms(true, 1000), None, "once per play");
        // A re-set of the same track (a re-poll) does not start a second listen.
        l.set_track("/a.flac", 60);
        assert_eq!(l.tick_ms(true, 60_000), None);
        // A different track does.
        l.set_track("/b.flac", 20);
        assert_eq!(l.tick_ms(true, 60_000), None, "under 30 s never counts");
    }
}
