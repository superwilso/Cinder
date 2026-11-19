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
//! Tags: FLAC (Vorbis comments), MP3 (ID3v2.2–2.4, ID3v1), MP4/M4A/ALAC (ilst). Other formats are
//! indexed by path with the file name as the title.

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
        // The LAST value of a repeated field wins and spaces are kept, as in Sony's scanner (MTPDB on
        // the owner's player, 11-19: `ARTIST=Kelela` then `ARTIST=Fousheé` is listed as Fousheé, and
        // a title's trailing space survives). Matching it keeps the Library the same on either source.
        let set = |f: &mut String| {
            if !v.is_empty() {
                *f = v.to_string();
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

/// Text of an ID3v2 text frame body: encoding byte, then the text (Latin-1, UTF-16 with BOM,
/// UTF-16BE, UTF-8); the first of several NUL-separated values.
fn id3_text(b: &[u8]) -> String {
    let Some((&enc, body)) = b.split_first() else {
        return String::new();
    };
    let s = match enc {
        1 | 2 => {
            let (be, body) = match body {
                [0xfe, 0xff, rest @ ..] => (true, rest),
                [0xff, 0xfe, rest @ ..] => (false, rest),
                _ => (enc == 2, body),
            };
            let units: Vec<u16> = body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| {
                    if be {
                        u16::from_be_bytes([c[0], c[1]])
                    } else {
                        u16::from_le_bytes([c[0], c[1]])
                    }
                })
                .collect();
            String::from_utf16_lossy(&units)
        }
        3 => String::from_utf8_lossy(body).into_owned(),
        _ => body.iter().map(|&c| c as char).collect(),
    };
    s.split('\0').next().unwrap_or("").trim().to_string()
}

/// MP3: the tags (below) and the length from the first MPEG Layer III frame.
pub fn id3_tags<R: Read + Seek>(r: &mut R) -> Option<Tags> {
    let t = id3_text_tags(r);
    let mut h = [0u8; 10];
    r.seek(SeekFrom::Start(0)).ok()?;
    let start = match r.read_exact(&mut h) {
        Ok(()) if &h[..3] == b"ID3" => {
            let size = h[6..10]
                .iter()
                .fold(0u64, |a, b| (a << 7) | u64::from(*b & 0x7f));
            10 + size + if h[5] & 0x10 != 0 { 10 } else { 0 }
        }
        _ => 0,
    };
    let ms = mp3_ms(r, start).unwrap_or(0);
    if t.is_none() && ms == 0 {
        return None;
    }
    let mut t = t.unwrap_or_default();
    t.dur_ms = ms;
    Some(t)
}

/// Length of an MPEG Layer III stream whose audio starts near `start`: the frame count in a Xing,
/// Info or VBRI header when there is one, else the constant bitrate over the audio bytes.
/// ponytail: Layers I/II (rare in a music library) report 0, as does a free-format bitrate.
fn mp3_ms<R: Read + Seek>(r: &mut R, start: u64) -> Option<i64> {
    let end = r.seek(SeekFrom::End(0)).ok()?;
    r.seek(SeekFrom::Start(start)).ok()?;
    let mut b = vec![0u8; 64 << 10];
    let n = r.read(&mut b).ok()?;
    let b = &b[..n];
    // The first frame sync with a Layer III header that parses.
    let (at, mpeg1, rate, kbps, mono) = (0..b.len().saturating_sub(4)).find_map(|i| {
        let h = &b[i..i + 4];
        if h[0] != 0xff || h[1] & 0xe0 != 0xe0 || h[1] & 0x06 != 0x02 {
            return None;
        }
        let ver = (h[1] >> 3) & 3; // 3 = MPEG-1, 2 = MPEG-2, 0 = MPEG-2.5
        let (bi, si) = ((h[2] >> 4) as usize, ((h[2] >> 2) & 3) as usize);
        if ver == 1 || bi == 0 || bi == 15 || si == 3 {
            return None;
        }
        const V1: [u32; 15] = [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ];
        const V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
        let rate = [44100u32, 48000, 32000][si] >> [2, 0, 1, 0][ver as usize];
        let kbps = if ver == 3 { V1[bi] } else { V2[bi] };
        Some((i, ver == 3, rate, kbps, h[3] >> 6 == 3))
    })?;
    let spf: u64 = if mpeg1 { 1152 } else { 576 };
    let side = match (mpeg1, mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let be = |o: usize| {
        b.get(o..o + 4)
            .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
    };
    let x = at + 4 + side;
    let frames = match b.get(x..x + 4) {
        Some(b"Xing") | Some(b"Info") if be(x + 4)? & 1 != 0 => be(x + 8),
        _ if b.get(at + 36..at + 40) == Some(b"VBRI") => be(at + 50),
        _ => None,
    };
    if let Some(f) = frames {
        return Some((u64::from(f) * spf * 1000 / u64::from(rate)) as i64);
    }
    // Constant bitrate: the audio bytes (less an ID3v1 tag) at the first frame's bitrate.
    let mut v1 = [0u8; 3];
    let tag = r.seek(SeekFrom::End(-128)).is_ok() && r.read_exact(&mut v1).is_ok() && &v1 == b"TAG";
    let audio = end.saturating_sub(start + at as u64 + if tag { 128 } else { 0 });
    Some((audio * 8 / u64::from(kbps)) as i64)
}

/// MP3 tags: ID3v2.3/2.4 text frames (v2.2's three-letter frames too), ID3v1 at the end if there
/// is no v2.
fn id3_text_tags<R: Read + Seek>(r: &mut R) -> Option<Tags> {
    let mut t = Tags::default();
    let mut h = [0u8; 10];
    r.seek(SeekFrom::Start(0)).ok()?;
    if r.read_exact(&mut h).is_ok() && &h[..3] == b"ID3" {
        let ver = h[3];
        let size = h[6..10]
            .iter()
            .fold(0usize, |a, b| (a << 7) | (*b & 0x7f) as usize);
        let mut b = vec![0u8; size.min(4 << 20)];
        r.read_exact(&mut b).ok()?;
        let (idlen, hdr) = if ver == 2 { (3, 6) } else { (4, 10) };
        let mut at = 0;
        while at + hdr <= b.len() && b[at] != 0 {
            let id = &b[at..at + idlen];
            let len = match ver {
                2 => (b[at + 3] as usize) << 16 | (b[at + 4] as usize) << 8 | b[at + 5] as usize,
                4 => b[at + 4..at + 8]
                    .iter()
                    .fold(0usize, |a, x| (a << 7) | (*x & 0x7f) as usize),
                _ => u32::from_be_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize,
            };
            let Some(body) = (at + hdr).checked_add(len).and_then(|e| b.get(at + hdr..e)) else {
                break;
            };
            let v = id3_text(body);
            let num = |v: &str| {
                v.split('/')
                    .next()
                    .and_then(|n| n.trim().parse().ok())
                    .unwrap_or(0)
            };
            match id {
                b"TIT2" | b"TT2" => t.title = v,
                b"TPE1" | b"TP1" => t.artist = v,
                b"TALB" | b"TAL" => t.album = v,
                b"TPE2" | b"TP2" => t.album_artist = v,
                b"TRCK" | b"TRK" => t.track = num(&v),
                b"TPOS" | b"TPA" => t.disc = num(&v),
                b"TDRC" | b"TYER" | b"TYE" => t.year = v.chars().take(4).collect(),
                b"TCON" | b"TCO" => t.genre = v,
                _ => {}
            }
            at += hdr + len;
        }
        return Some(t);
    }
    // ID3v1: the last 128 bytes.
    let mut v1 = [0u8; 128];
    r.seek(SeekFrom::End(-128)).ok()?;
    r.read_exact(&mut v1).ok()?;
    if &v1[..3] != b"TAG" {
        return None;
    }
    let field = |a: usize, n: usize| -> String {
        v1[a..a + n]
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as char)
            .collect::<String>()
            .trim()
            .to_string()
    };
    t.title = field(3, 30);
    t.artist = field(33, 30);
    t.album = field(63, 30);
    t.year = field(93, 4);
    if v1[125] == 0 && v1[126] != 0 {
        t.track = i64::from(v1[126]);
    }
    Some(t)
}

/// Called for each MP4 atom: (reader, kind, body start, end).
type AtomFn<'a, R> = dyn FnMut(&mut R, [u8; 4], u64, u64) -> Option<()> + 'a;

/// MP4/M4A/ALAC tags: `moov/udta/meta/ilst` (©nam ©ART ©alb aART trkn disk ©day ©gen) and the
/// length from `moov/mvhd`.
pub fn mp4_tags<R: Read + Seek>(r: &mut R) -> Option<Tags> {
    fn atoms<R: Read + Seek>(r: &mut R, start: u64, end: u64, f: &mut AtomFn<R>) -> Option<()> {
        let mut at = start;
        while at + 8 <= end {
            r.seek(SeekFrom::Start(at)).ok()?;
            let mut h = [0u8; 8];
            r.read_exact(&mut h).ok()?;
            let mut size = u64::from(u32::from_be_bytes([h[0], h[1], h[2], h[3]]));
            let mut body = at + 8;
            if size == 1 {
                let mut x = [0u8; 8];
                r.read_exact(&mut x).ok()?;
                size = u64::from_be_bytes(x);
                body += 8;
            } else if size == 0 {
                size = end - at;
            }
            let Some(stop) = at.checked_add(size) else {
                return Some(());
            };
            if size < 8 || stop > end || body > stop {
                return Some(());
            }
            f(r, [h[4], h[5], h[6], h[7]], body, stop)?;
            at = stop;
        }
        Some(())
    }
    let end = r.seek(SeekFrom::End(0)).ok()?;
    let mut t = Tags::default();
    let mut is_mp4 = false;
    atoms(r, 0, end, &mut |r, kind, b, e| {
        match &kind {
            b"ftyp" => is_mp4 = true,
            b"moov" => {
                atoms(r, b, e, &mut |r, kind, b, e| {
                    match &kind {
                        b"mvhd" => {
                            let mut v = [0u8; 32];
                            r.seek(SeekFrom::Start(b)).ok()?;
                            r.read_exact(&mut v).ok()?;
                            let (scale, dur) = if v[0] == 1 {
                                (
                                    u64::from(u32::from_be_bytes([v[20], v[21], v[22], v[23]])),
                                    u64::from_be_bytes([
                                        v[24], v[25], v[26], v[27], v[28], v[29], v[30], v[31],
                                    ]),
                                )
                            } else {
                                (
                                    u64::from(u32::from_be_bytes([v[12], v[13], v[14], v[15]])),
                                    u64::from(u32::from_be_bytes([v[16], v[17], v[18], v[19]])),
                                )
                            };
                            if let Some(ms) = dur.saturating_mul(1000).checked_div(scale) {
                                t.dur_ms = ms as i64;
                            }
                        }
                        b"udta" => {
                            atoms(r, b, e, &mut |r, kind, b, e| {
                                if &kind == b"meta" {
                                    // meta is a full box: 4 bytes of version and flags first.
                                    atoms(r, b + 4, e, &mut |r, kind, b, e| {
                                        if &kind == b"ilst" {
                                            atoms(r, b, e, &mut |r, item, b, e| {
                                                atoms(r, b, e, &mut |r, kind, b, e| {
                                                    if &kind != b"data" || e < b + 8 {
                                                        return Some(());
                                                    }
                                                    let mut v =
                                                        vec![0u8; (e - b - 8).min(4096) as usize];
                                                    r.seek(SeekFrom::Start(b + 8)).ok()?;
                                                    r.read_exact(&mut v).ok()?;
                                                    let text = || {
                                                        String::from_utf8_lossy(&v)
                                                            .trim()
                                                            .to_string()
                                                    };
                                                    let pair = || {
                                                        v.get(2..4).map_or(0, |x| {
                                                            i64::from(u16::from_be_bytes([
                                                                x[0], x[1],
                                                            ]))
                                                        })
                                                    };
                                                    match &item {
                                                        b"\xa9nam" => t.title = text(),
                                                        b"\xa9ART" => t.artist = text(),
                                                        b"\xa9alb" => t.album = text(),
                                                        b"aART" => t.album_artist = text(),
                                                        b"\xa9day" => {
                                                            t.year =
                                                                text().chars().take(4).collect()
                                                        }
                                                        b"\xa9gen" => t.genre = text(),
                                                        b"trkn" => t.track = pair(),
                                                        b"disk" => t.disc = pair(),
                                                        _ => {}
                                                    }
                                                    Some(())
                                                })
                                            })?;
                                        }
                                        Some(())
                                    })?;
                                }
                                Some(())
                            })?;
                        }
                        _ => {}
                    }
                    Some(())
                })?;
            }
            _ => {}
        }
        Some(())
    })?;
    is_mp4.then_some(t)
}

fn read_tags(path: &str) -> Tags {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut t = File::open(path)
        .ok()
        .and_then(|mut f| match ext.as_str() {
            "mp3" => id3_tags(&mut f),
            "m4a" | "mp4" | "aac" => mp4_tags(&mut f),
            _ => flac_tags(&mut f),
        })
        .unwrap_or_default();
    if t.title.is_empty() {
        t.title = Path::new(path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    }
    // Sony's scanner keeps 255 UTF-16 units of a name (a longer album title in MTPDB, 11-19).
    for f in [
        &mut t.title,
        &mut t.artist,
        &mut t.album,
        &mut t.album_artist,
        &mut t.genre,
    ] {
        let mut units = 0;
        if let Some((i, _)) = f.char_indices().find(|(_, ch)| {
            units += ch.len_utf16();
            units > SONY_NAME_UNITS
        }) {
            f.truncate(i);
        }
    }
    t
}

const SONY_NAME_UNITS: usize = 255;

/// Rollback journal, not WAL: one writer (the scan) and readers that open, read and close.
pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode=DELETE;
         CREATE TABLE IF NOT EXISTS files(
            path TEXT PRIMARY KEY, size INTEGER, mtime INTEGER, id INTEGER UNIQUE,
            title TEXT, artist TEXT, album TEXT, album_artist TEXT, track INTEGER, disc INTEGER,
            year TEXT, genre TEXT, dur_ms INTEGER);",
    )?;
    // A change to how tags are read re-reads every file at the next scan (ids kept): bump this.
    // 2 = Sony's rules for repeated fields, spaces and 255-unit names, and MP3 lengths (11-19).
    const RULES: i64 = 2;
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v < RULES {
        c.execute_batch(&format!(
            "UPDATE files SET mtime = -1; PRAGMA user_version = {RULES};"
        ))?;
    }
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
    // Writes go in batches of BATCH files: on /contents (FAT) every commit is a flush, and one
    // per file made a first scan of 4,914 files take 91 s against 43 s. A reboot mid-scan loses at
    // most one batch, which the next scan reads again (those rows were never committed).
    const BATCH: usize = 200;
    let mut pending = 0;
    c.execute_batch("BEGIN")?;
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
        pending += 1;
        if pending == BATCH {
            c.execute_batch("COMMIT; BEGIN")?;
            pending = 0;
        }
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
    c.execute_batch("COMMIT")?;
    let seen: HashSet<&str> = found.iter().map(|f| f.0.as_str()).collect();
    // A root that is not there right now (the drive shared over USB, the boot window, a card out)
    // keeps its rows: an unmounted folder is not a deleted library.
    // ponytail: a card taken out for good stays listed until it is back; drop its rows on a real
    // card-removal event if that matters.
    let missing: Vec<&str> = roots
        .iter()
        .copied()
        .filter(|r| !Path::new(r).is_dir())
        .collect();
    for path in known
        .keys()
        .filter(|p| !seen.contains(p.as_str()) && !missing.iter().any(|m| p.starts_with(m)))
    {
        c.execute("DELETE FROM files WHERE path = ?1", [path])?;
        rep.removed += 1;
    }
    rep.ms = t0.elapsed().as_millis();
    Ok(rep)
}

/// A stable album id for a file MTPDB never saw, from (album artist or artist, album). Above 2^40, so
/// it cannot meet one of MTPDB's small ids.
fn album_key(album_artist: &str, artist: &str, album: &str) -> i64 {
    let who = if album_artist.is_empty() {
        artist
    } else {
        album_artist
    };
    let h = format!("{who}\u{1f}{album}")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
    (1i64 << 40) + (h % (1u64 << 40)) as i64
}

/// The track list for the Library, from the index: MTPDB's rows (their order, album ids, art,
/// years, genres and SensMe keys, all by id) with the index's tags; files the index no longer has
/// left out; files only the index has added after them, by title. `mtpdb` is
/// [`crate::Db::tracks`]'s list (empty when MTPDB is unreadable).
pub fn tracks(c: &Connection, mtpdb: Vec<crate::Track>) -> rusqlite::Result<Vec<crate::Track>> {
    struct Row {
        id: i64,
        tags: Tags,
    }
    let mut rows: HashMap<String, Row> = {
        let mut st = c.prepare(
            "SELECT path, id, title, artist, album, album_artist, track, disc, year, genre, dur_ms FROM files",
        )?;
        let it = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Row {
                    id: r.get(1)?,
                    tags: Tags {
                        title: r.get(2)?,
                        artist: r.get(3)?,
                        album: r.get(4)?,
                        album_artist: r.get(5)?,
                        track: r.get(6)?,
                        disc: r.get(7)?,
                        year: r.get(8)?,
                        genre: r.get(9)?,
                        dur_ms: r.get(10)?,
                    },
                },
            ))
        })?;
        it.collect::<rusqlite::Result<_>>()?
    };
    let apply = |t: &mut crate::Track, g: &Tags| {
        t.title = g.title.clone();
        t.artist = g.artist.clone();
        t.album = g.album.clone();
        t.album_artist = g.album_artist.clone();
        t.track_no = g.track;
        t.disc_no = g.disc;
        if g.dur_ms > 0 {
            t.duration_raw = Some(g.dur_ms);
        }
    };
    let mut out = Vec::with_capacity(rows.len());
    for mut t in mtpdb {
        if let Some(r) = rows.remove(&t.filename) {
            apply(&mut t, &r.tags);
            out.push(t);
        }
    }
    let mut rest: Vec<(String, Row)> = rows.into_iter().collect();
    rest.sort_by(|a, b| {
        a.1.tags
            .title
            .to_lowercase()
            .cmp(&b.1.tags.title.to_lowercase())
    });
    for (path, r) in rest {
        let g = &r.tags;
        let mut t = crate::Track {
            object_id: r.id,
            filename: path,
            album_id: Some(album_key(&g.album_artist, &g.artist, &g.album)),
            ..Default::default()
        };
        apply(&mut t, g);
        out.push(t);
    }
    Ok(out)
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
                "TITLE=Dreams ",
                "artist=Kelela",
                "ALBUM=Rumours",
                "TRACKNUMBER=2/11",
                "DATE=1977",
                "ARTIST=Fousheé",
                "TITLE=",
            ],
            44100,
            44100 * 257,
        );
        let t = flac_tags(&mut std::io::Cursor::new(f)).unwrap();
        assert_eq!(
            (t.title.as_str(), t.artist.as_str(), t.album.as_str()),
            // As Sony lists them: the last ARTIST, the trailing space, an empty repeat ignored.
            ("Dreams ", "Fousheé", "Rumours")
        );
        assert_eq!((t.track, t.year.as_str(), t.dur_ms), (2, "1977", 257_000));
        assert!(flac_tags(&mut std::io::Cursor::new(
            b"ID3\x03\0\0\0\0\0\0not flac".to_vec()
        ))
        .is_none());
    }

    #[test]
    fn reads_id3_tags() {
        let frame = |id: &[u8], text: &[u8]| {
            let mut f = id.to_vec();
            f.extend_from_slice(&((text.len() + 1) as u32).to_be_bytes());
            f.extend_from_slice(&[0, 0, 3]); // flags, then UTF-8
            f.extend_from_slice(text);
            f
        };
        let mut body = Vec::new();
        for (id, v) in [
            (&b"TIT2"[..], &b"Song"[..]),
            (b"TPE1", b"Band"),
            (b"TALB", b"Record"),
            (b"TRCK", b"3/9"),
            (b"TYER", b"1999"),
        ] {
            body.extend(frame(id, v));
        }
        let mut f = b"ID3\x03\x00\x00".to_vec();
        let n = body.len();
        f.extend_from_slice(&[
            (n >> 21) as u8 & 0x7f,
            (n >> 14) as u8 & 0x7f,
            (n >> 7) as u8 & 0x7f,
            n as u8 & 0x7f,
        ]);
        f.extend(body);
        f.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        let t = id3_tags(&mut std::io::Cursor::new(f)).unwrap();
        assert_eq!(
            (
                t.title.as_str(),
                t.artist.as_str(),
                t.album.as_str(),
                t.track,
                t.year.as_str()
            ),
            ("Song", "Band", "Record", 3, "1999")
        );
        // ID3v1 only.
        let mut v1 = vec![0u8; 300];
        let tag = v1.len() - 128;
        v1[tag..tag + 3].copy_from_slice(b"TAG");
        v1[tag + 3..tag + 7].copy_from_slice(b"Old!");
        let t = id3_tags(&mut std::io::Cursor::new(v1)).unwrap();
        assert_eq!(t.title, "Old!");
    }

    #[test]
    fn reads_mp4_tags_and_length() {
        fn atom(kind: &[u8], body: &[u8]) -> Vec<u8> {
            let mut a = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            a.extend_from_slice(kind);
            a.extend_from_slice(body);
            a
        }
        let data = |v: &[u8]| atom(b"data", &[&[0u8, 0, 0, 1, 0, 0, 0, 0][..], v].concat());
        let ilst = atom(
            b"ilst",
            &[
                atom(b"\xa9nam", &data(b"Tune")),
                atom(b"\xa9ART", &data(b"Singer")),
                atom(b"trkn", &data(&[0, 0, 0, 4, 0, 10, 0, 0])),
            ]
            .concat(),
        );
        let meta = atom(b"meta", &[&[0u8; 4][..], &ilst].concat());
        let mut mvhd = vec![0u8; 100];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&65_432u32.to_be_bytes());
        let moov = atom(
            b"moov",
            &[atom(b"mvhd", &mvhd), atom(b"udta", &meta)].concat(),
        );
        let f = [atom(b"ftyp", b"M4A \0\0\0\0"), moov].concat();
        let t = mp4_tags(&mut std::io::Cursor::new(f)).unwrap();
        assert_eq!(
            (t.title.as_str(), t.artist.as_str(), t.track, t.dur_ms),
            ("Tune", "Singer", 4, 65_432)
        );
        assert!(mp4_tags(&mut std::io::Cursor::new(b"not an mp4 file".to_vec())).is_none());
    }

    /// Damaged files end in None or odd tags, never a panic or a hang (the device aborts on panic).
    #[test]
    fn damaged_tags_never_panic() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let heads: [&[u8]; 4] = [
            b"fLaC",
            b"ID3\x03\x00\x00",
            b"\0\0\0\x10ftypM4A ",
            b"ID3\x04\x00\x00",
        ];
        for i in 0..4000 {
            let mut b = heads[i % 4].to_vec();
            let n = (rnd() % 600) as usize;
            b.extend((0..n).map(|_| rnd() as u8));
            let _ = flac_tags(&mut std::io::Cursor::new(b.clone()));
            let _ = id3_tags(&mut std::io::Cursor::new(b.clone()));
            let _ = mp4_tags(&mut std::io::Cursor::new(b));
        }
    }

    #[test]
    fn mp3_length_from_xing_vbri_or_bitrate() {
        // MPEG-1 Layer III, 128 kb/s, 44.1 kHz, stereo.
        let hdr = [0xffu8, 0xfb, 0x90, 0x00];
        let cbr = [&hdr[..], &[0u8; 159_996]].concat(); // 160,000 bytes at 128 kb/s = 10 s
        assert_eq!(
            id3_tags(&mut std::io::Cursor::new(cbr)).unwrap().dur_ms,
            10_000
        );
        let mut xing = [&hdr[..], &[0u8; 2000]].concat();
        xing[36..40].copy_from_slice(b"Xing");
        xing[43] = 1; // flags: frame count present
        xing[44..48].copy_from_slice(&1000u32.to_be_bytes()); // 1000 x 1152 / 44100
        assert_eq!(
            id3_tags(&mut std::io::Cursor::new(xing)).unwrap().dur_ms,
            26_122
        );
        let mut vbri = [&hdr[..], &[0u8; 2000]].concat();
        vbri[36..40].copy_from_slice(b"VBRI");
        vbri[50..54].copy_from_slice(&500u32.to_be_bytes());
        assert_eq!(
            id3_tags(&mut std::io::Cursor::new(vbri)).unwrap().dur_ms,
            13_061
        );
        // An ID3v2 tag in front is skipped; not an MP3 at all stays None.
        let tagged = [
            &b"ID3\x03\0\0\0\0\0\x0a"[..],
            &[0u8; 10],
            &hdr,
            &[0u8; 15_996],
        ]
        .concat();
        assert_eq!(
            id3_tags(&mut std::io::Cursor::new(tagged)).unwrap().dur_ms,
            1_000
        );
        assert!(id3_tags(&mut std::io::Cursor::new(vec![0u8; 500])).is_none());
    }

    #[test]
    fn long_names_are_cut_where_sonys_scanner_cuts_them() {
        let p = std::env::temp_dir().join(format!("cinder_long_{}.flac", std::process::id()));
        let album = format!("ALBUM={}’{}", "a".repeat(250), "b".repeat(20));
        std::fs::write(&p, flac_with(&["TITLE=t", &album], 44100, 44100)).unwrap();
        let t = read_tags(p.to_str().unwrap());
        let _ = std::fs::remove_file(&p);
        assert_eq!(t.album.encode_utf16().count(), 255);
        assert!(t.album.ends_with("’bbbb"));
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

        // An index written under older tag rules is re-read once, every id kept.
        let db = std::env::temp_dir().join(format!("cinder_index_v_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        let old = open(db.to_str().unwrap()).unwrap();
        scan(&old, &[root], &ids).unwrap();
        old.execute_batch("PRAGMA user_version = 1").unwrap();
        drop(old);
        let reopened = open(db.to_str().unwrap()).unwrap();
        let r = scan(&reopened, &[root], &ids).unwrap();
        assert_eq!((r.read, r.new_ids), (2, 0), "re-read, no new ids");
        assert_eq!(scan(&reopened, &[root], &ids).unwrap().read, 0);
        drop(reopened);
        let _ = std::fs::remove_file(&db);

        // The Library's list: MTPDB's row for the file it knows (its album id kept, the index's
        // tags on it), and the new file after it with an album id of its own.
        let known = crate::Track {
            object_id: 500,
            filename: a.to_string_lossy().into_owned(),
            title: "old title".into(),
            album_id: Some(7),
            ..Default::default()
        };
        let gone = crate::Track {
            object_id: 9,
            filename: "/nowhere.flac".into(),
            ..Default::default()
        };
        let list = tracks(&c, vec![known, gone]).unwrap();
        assert_eq!(list.len(), 2, "the file that is gone is left out");
        assert_eq!(
            (list[0].object_id, list[0].title.as_str(), list[0].album_id),
            (500, "One", Some(7))
        );
        assert_eq!((list[1].object_id, list[1].title.as_str()), (501, "Two"));
        assert!(list[1].album_id.unwrap() > 1 << 40);

        std::fs::remove_file(&b).unwrap();
        let r = scan(&c, &[root], &ids).unwrap();
        assert_eq!((r.files, r.removed), (1, 1));

        // The folder unmounted: nothing found, nothing dropped.
        let away = dir.with_extension("away");
        std::fs::rename(&dir, &away).unwrap();
        let r = scan(&c, &[root], &ids).unwrap();
        assert_eq!((r.files, r.removed, counts(&c).unwrap().0), (0, 0, 1));
        std::fs::rename(&away, &dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
