//! Cinder's own library index (docs/PLAN_hardware_first.md, Phase 2).
//!
//! Sony's scanner writes `/db/MTPDB.dat`, and an unclean shutdown in the middle of a scan leaves it
//! damaged (0 tracks). This index is Cinder's own: a walk of the music folders, tags read only for
//! files whose size or modified time changed, kept in a small SQLite file. Each track keeps the
//! object id MTPDB gave it (matched by path), so queues, playlists, likes and play counts — all keyed
//! by those ids — carry on unchanged; a file MTPDB never saw gets an id above every MTPDB id.
//!
//! State 2026-11-18: built and checked against MTPDB on the player (`cinder-probe`-free: a dev flag in
//! cinder-home runs `scan` and logs the comparison). The UI still reads MTPDB; switching it over is
//! the next step. SensMe values stay in MTPDB (decided in the plan).
//!
//! ponytail: tags are read from FLAC only (the owner's library is all FLAC); other formats are
//! indexed by path with the file name as the title. Add ID3/MP4 readers when a library needs them.

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Extensions the player lists as music (Sony's scanner's set, lower case).
const AUDIO: [&str; 14] = [
    "flac", "mp3", "m4a", "mp4", "aac", "wav", "aif", "aiff", "dsf", "dff", "ape", "wma", "ogg",
    "opus",
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track: i64,
    pub disc: i64,
    pub year: String,
    pub genre: String,
    pub dur_ms: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub files: usize,
    pub read: usize,
    pub removed: usize,
    pub new_ids: usize,
    pub ms: u128,
}

fn audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

fn walk(dir: &Path, out: &mut Vec<(String, u64, i64)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            // Sony's and Cinder's own hidden folders hold no music.
            if !e.file_name().to_string_lossy().starts_with('.') {
                walk(&p, out);
            }
        } else if md.is_file() && audio(&p) {
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            out.push((p.to_string_lossy().into_owned(), md.len(), mtime));
        }
    }
}

/// FLAC tags: STREAMINFO's length and the Vorbis comment block. None if `r` is not a FLAC.
pub fn flac_tags<R: Read + Seek>(r: &mut R) -> Option<Tags> {
    let mut magic = [0u8; 10];
    r.read_exact(&mut magic[..4]).ok()?;
    // An ID3v2 tag in front of the stream (some taggers add one): skip its syncsafe size.
    if &magic[..3] == b"ID3" {
        r.read_exact(&mut magic[4..10]).ok()?;
        let sz = magic[6..10]
            .iter()
            .fold(0u64, |a, b| (a << 7) | (*b & 0x7f) as u64);
        r.seek(SeekFrom::Start(10 + sz)).ok()?;
        r.read_exact(&mut magic[..4]).ok()?;
    }
    if &magic[..4] != b"fLaC" {
        return None;
    }
    let mut t = Tags::default();
    loop {
        let mut h = [0u8; 4];
        r.read_exact(&mut h).ok()?;
        let (last, kind) = (h[0] & 0x80 != 0, h[0] & 0x7f);
        let len = u32::from_be_bytes([0, h[1], h[2], h[3]]) as usize;
        match kind {
            0 if len >= 18 => {
                let mut b = [0u8; 18];
                r.read_exact(&mut b).ok()?;
                r.seek(SeekFrom::Current(len as i64 - 18)).ok()?;
                let rate =
                    (u32::from(b[10]) << 12) | (u32::from(b[11]) << 4) | (u32::from(b[12]) >> 4);
                let total = (u64::from(b[13] & 0x0f) << 32)
                    | u64::from(u32::from_be_bytes([b[14], b[15], b[16], b[17]]));
                if rate > 0 {
                    t.dur_ms = (total * 1000 / u64::from(rate)) as i64;
                }
            }
            4 if len < 1 << 22 => {
                let mut b = vec![0u8; len];
                r.read_exact(&mut b).ok()?;
                vorbis_comments(&b, &mut t);
            }
            _ => {
                r.seek(SeekFrom::Current(len as i64)).ok()?;
            }
        }
        if last {
            return Some(t);
        }
    }
}

fn vorbis_comments(b: &[u8], t: &mut Tags) {
    let le32 = |at: usize| -> Option<usize> {
        let w = b.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as usize)
    };
    let Some(vendor) = le32(0) else { return };
    let Some(mut at) = 4usize.checked_add(vendor) else {
        return;
    };
    let Some(count) = le32(at) else { return };
    at += 4;
    for _ in 0..count {
        let Some(len) = le32(at) else { return };
        at += 4;
        let Some(raw) = at.checked_add(len).and_then(|e| b.get(at..e)) else {
            return;
        };
        at += len;
        let entry = String::from_utf8_lossy(raw);
        let Some((k, v)) = entry.split_once('=') else {
            continue;
        };
        let num = |v: &str| {
            v.split('/')
                .next()
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0)
        };
        // The first value of a repeated field wins, as Sony's scanner does.
        let set = |f: &mut String| {
            if f.is_empty() {
                *f = v.trim().to_string();
            }
        };
        match k.to_ascii_uppercase().as_str() {
            "TITLE" => set(&mut t.title),
            "ARTIST" => set(&mut t.artist),
            "ALBUM" => set(&mut t.album),
            "ALBUMARTIST" | "ALBUM ARTIST" => set(&mut t.album_artist),
            "TRACKNUMBER" if t.track == 0 => t.track = num(v),
            "DISCNUMBER" if t.disc == 0 => t.disc = num(v),
            "DATE" | "YEAR" => set(&mut t.year),
            "GENRE" => set(&mut t.genre),
            _ => {}
        }
    }
}

fn read_tags(path: &str) -> Tags {
    let mut t = File::open(path)
        .ok()
        .and_then(|mut f| flac_tags(&mut f))
        .unwrap_or_default();
    if t.title.is_empty() {
        t.title = Path::new(path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    }
    t
}

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS files(
            path TEXT PRIMARY KEY, size INTEGER, mtime INTEGER, id INTEGER UNIQUE,
            title TEXT, artist TEXT, album TEXT, album_artist TEXT, track INTEGER, disc INTEGER,
            year TEXT, genre TEXT, dur_ms INTEGER);",
    )?;
    Ok(c)
}

/// Walk `roots`, read tags for new or changed files, drop files that are gone. `mtpdb_ids` maps a
/// path to the id MTPDB gave it (from [`crate::Db::tracks_album_order`]); empty is fine. Each
/// changed file is its own small write, so a reboot in the middle loses only that file's row,
/// which the next scan reads again.
pub fn scan(
    c: &Connection,
    roots: &[&str],
    mtpdb_ids: &HashMap<String, i64>,
) -> rusqlite::Result<Report> {
    let t0 = std::time::Instant::now();
    let mut found = Vec::new();
    for r in roots {
        walk(Path::new(r), &mut found);
    }
    let mut rep = Report {
        files: found.len(),
        ..Default::default()
    };
    let known: HashMap<String, (i64, i64, i64)> = {
        let mut st = c.prepare("SELECT path, size, mtime, id FROM files")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut next_id = c
        .query_row("SELECT MAX(id) FROM files", [], |r| {
            r.get::<_, Option<i64>>(0)
        })?
        .unwrap_or(0)
        .max(mtpdb_ids.values().copied().max().unwrap_or(0))
        + 1;
    let mut used: HashSet<i64> = known.values().map(|v| v.2).collect();
    for (path, size, mtime) in &found {
        let id = match known.get(path) {
            Some(&(s, m, _)) if s == *size as i64 && m == *mtime => continue,
            Some(&(_, _, id)) => id,
            None => match mtpdb_ids.get(path) {
                Some(&id) if !used.contains(&id) => id,
                _ => {
                    rep.new_ids += 1;
                    next_id += 1;
                    next_id - 1
                }
            },
        };
        used.insert(id);
        let t = read_tags(path);
        rep.read += 1;
        c.execute(
            "INSERT OR REPLACE INTO files VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                path,
                *size as i64,
                mtime,
                id,
                t.title,
                t.artist,
                t.album,
                t.album_artist,
                t.track,
                t.disc,
                t.year,
                t.genre,
                t.dur_ms
            ],
        )?;
    }
    let seen: HashSet<&str> = found.iter().map(|f| f.0.as_str()).collect();
    for path in known.keys().filter(|p| !seen.contains(p.as_str())) {
        c.execute("DELETE FROM files WHERE path = ?1", [path])?;
        rep.removed += 1;
    }
    rep.ms = t0.elapsed().as_millis();
    Ok(rep)
}

/// Tracks, albums (album artist or artist, plus album) and artists in the index.
pub fn counts(c: &Connection) -> rusqlite::Result<(i64, i64, i64)> {
    c.query_row(
        "SELECT COUNT(*),
                COUNT(DISTINCT COALESCE(NULLIF(album_artist,''), artist) || char(31) || album),
                COUNT(DISTINCT COALESCE(NULLIF(album_artist,''), artist))
         FROM files",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
}

/// Paths whose title, artist or album differ from what MTPDB holds for the same path.
pub fn disagreements(c: &Connection, mtpdb: &[crate::Track]) -> rusqlite::Result<Vec<String>> {
    let mut st = c.prepare("SELECT title, artist, album FROM files WHERE path = ?1")?;
    let mut out = Vec::new();
    for m in mtpdb {
        let got: Option<(String, String, String)> = st
            .query_row([&m.filename], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        match got {
            None => out.push(format!("not indexed: {}", m.filename)),
            Some((t, a, al)) if t != m.title || a != m.artist || al != m.album => {
                out.push(format!(
                    "{}: index {t:?}/{a:?}/{al:?}, MTPDB {:?}/{:?}/{:?}",
                    m.filename, m.title, m.artist, m.album
                ))
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flac_with(comments: &[&str], rate: u32, total: u64) -> Vec<u8> {
        let mut b = b"fLaC".to_vec();
        // STREAMINFO (not last).
        b.extend_from_slice(&[0x00, 0, 0, 34]);
        let mut si = [0u8; 34];
        si[10] = (rate >> 12) as u8;
        si[11] = (rate >> 4) as u8;
        si[12] = ((rate & 0xf) << 4) as u8 | 0x02; // + channel bits, ignored
        si[13] = (total >> 32) as u8 & 0x0f;
        si[14..18].copy_from_slice(&(total as u32).to_be_bytes());
        b.extend_from_slice(&si);
        let mut vc = Vec::new();
        vc.extend_from_slice(&3u32.to_le_bytes());
        vc.extend_from_slice(b"ven");
        vc.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            vc.extend_from_slice(&(c.len() as u32).to_le_bytes());
            vc.extend_from_slice(c.as_bytes());
        }
        b.push(0x84); // last block, VORBIS_COMMENT
        b.extend_from_slice(&(vc.len() as u32).to_be_bytes()[1..]);
        b.extend_from_slice(&vc);
        b
    }

    #[test]
    fn reads_flac_tags_and_length() {
        let f = flac_with(
            &[
                "TITLE=Dreams",
                "artist=Fleetwood Mac",
                "ALBUM=Rumours",
                "TRACKNUMBER=2/11",
                "DATE=1977",
                "TITLE=dup",
            ],
            44100,
            44100 * 257,
        );
        let t = flac_tags(&mut std::io::Cursor::new(f)).unwrap();
        assert_eq!(
            (t.title.as_str(), t.artist.as_str(), t.album.as_str()),
            ("Dreams", "Fleetwood Mac", "Rumours")
        );
        assert_eq!((t.track, t.year.as_str(), t.dur_ms), (2, "1977", 257_000));
        assert!(flac_tags(&mut std::io::Cursor::new(
            b"ID3\x03\0\0\0\0\0\0not flac".to_vec()
        ))
        .is_none());
    }

    #[test]
    fn scan_is_incremental_keeps_mtpdb_ids_and_drops_gone_files() {
        let dir = std::env::temp_dir().join(format!("cinder_index_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("A")).unwrap();
        let a = dir.join("A/01.flac");
        let b = dir.join("A/02.flac");
        std::fs::write(
            &a,
            flac_with(&["TITLE=One", "ARTIST=X", "ALBUM=A"], 44100, 44100),
        )
        .unwrap();
        std::fs::write(
            &b,
            flac_with(&["TITLE=Two", "ARTIST=X", "ALBUM=A"], 44100, 44100),
        )
        .unwrap();
        std::fs::write(dir.join("A/cover.jpg"), b"x").unwrap();
        let c = open(":memory:").unwrap();
        let root = dir.to_str().unwrap();
        let ids = HashMap::from([(a.to_string_lossy().into_owned(), 500)]);

        let r = scan(&c, &[root], &ids).unwrap();
        assert_eq!((r.files, r.read, r.new_ids), (2, 2, 1));
        let id_of = |p: &Path| -> i64 {
            c.query_row(
                "SELECT id FROM files WHERE path=?1",
                [p.to_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(id_of(&a), 500, "MTPDB's id carried over");
        assert_eq!(id_of(&b), 501, "a new file gets an id above MTPDB's");
        assert_eq!(counts(&c).unwrap(), (2, 1, 1));

        let r = scan(&c, &[root], &ids).unwrap();
        assert_eq!(r.read, 0, "nothing changed, nothing read");

        std::fs::remove_file(&b).unwrap();
        let r = scan(&c, &[root], &ids).unwrap();
        assert_eq!((r.files, r.removed), (1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
