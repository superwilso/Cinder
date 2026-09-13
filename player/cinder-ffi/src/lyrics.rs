//! Lyrics for a track: a `.lrc` file next to it, else the lyrics embedded in its tags.
//!
//! `Song.flac` → `Song.lrc`, in the same folder — the convention Sony's own player uses, so files
//! people already made for it work unchanged. It needs no scan: drop the file in over USB and it is
//! there the next time the song starts. A `.lrc` wins over the tags, so it can correct them.
//!
//! Embedded lyrics are what taggers actually write, and the reference library has them in more than
//! half its tracks (160 of 285 sampled FLACs, 2026-09-13) and no `.lrc` at all. Read from: FLAC
//! Vorbis comments (`LYRICS`, `UNSYNCEDLYRICS`), an ID3v2 `USLT` frame (MP3, or a FLAC with an ID3
//! header in front), and MP4 `©lyr`. Taggers put LRC text in these as often as plain text — both
//! shapes are in the reference library — so the tag value goes through the same parser. Not read:
//! ID3 `SYLT` (binary synced lyrics, rare), Ogg and DSF tags.
//!
//! LRC is a loose format written by many tools, so the parser is lenient about everything that does
//! not change which words are shown when: fraction widths, repeated timestamps on one line,
//! metadata tags, enhanced (per-word) stamps, and the encoding. A file that cannot be read simply
//! has no lyrics; nothing here can fail loudly.

use cinder_ui::lyrics::{Line, Lyrics};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Anything bigger is not lyrics. A long song's LRC is a few kilobytes; the cap keeps a mis-named
/// file, or a corrupt tag length, from being read into memory on the render thread.
const MAX_BYTES: u64 = 512 * 1024;

/// Lyrics for the track at `track_path`, or `None` when neither a `.lrc` nor the tags have words.
pub fn load_for(track_path: &str) -> Option<Lyrics> {
    let has_words = |l: &Lyrics| l.lines.iter().any(|l| !l.text.trim().is_empty());
    let from_lrc = sidecar(track_path).and_then(|p| {
        let len = std::fs::metadata(&p).ok()?.len();
        if len == 0 || len > MAX_BYTES {
            return None;
        }
        Some(parse(&decode(&std::fs::read(&p).ok()?)))
    });
    if let Some(l) = from_lrc.filter(has_words) {
        return Some(l);
    }
    // A file can carry more than one lyrics tag (plain in one, LRC in another): synced wins.
    let mut best: Option<Lyrics> = None;
    for text in embedded(track_path) {
        let l = parse(&text);
        if !has_words(&l) {
            continue;
        }
        if l.is_synced() {
            return Some(l);
        }
        best.get_or_insert(l);
    }
    best
}

/// Every lyrics value in the file's tags, in file order. Only headers and the lyrics themselves are
/// read; pictures and audio are seeked over.
fn embedded(track_path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(mut f) = File::open(track_path) else { return out };
    let mut head = [0u8; 12];
    if f.read_exact(&mut head).is_err() {
        return out;
    }
    let mut at = 0u64;
    if &head[..3] == b"ID3" {
        at = id3_uslt(&mut f, &head, &mut out).unwrap_or(0);
        if at == 0 || f.seek(SeekFrom::Start(at)).is_err() || f.read_exact(&mut head[..4]).is_err() {
            return out;
        }
    }
    if &head[..4] == b"fLaC" {
        flac_comments(&mut f, at + 4, &mut out);
    } else if at == 0 && &head[4..8] == b"ftyp" {
        let end = f.metadata().map(|m| m.len()).unwrap_or(0);
        mp4_lyr(&mut f, 0, end, 0, &mut out);
    }
    out
}

/// Read `len` bytes at the current position, refusing anything over `cap`.
fn take(f: &mut File, len: u64, cap: u64) -> Option<Vec<u8>> {
    if len > cap {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// A Vorbis comment block holds every tag, not only the lyrics — and some taggers put a whole
/// base64 cover in it — so it gets more room than the lyrics alone.
const MAX_COMMENT_BLOCK: u64 = 4 * 1024 * 1024;

/// FLAC metadata blocks from `pos`: the Vorbis comment block's lyrics keys. Field names are
/// case-insensitive by the Vorbis spec, and the value is always UTF-8.
fn flac_comments(f: &mut File, mut pos: u64, out: &mut Vec<String>) {
    // FLAC allows 127 block types but real files have a handful; the bound stops a corrupt chain.
    for _ in 0..64 {
        let mut h = [0u8; 4];
        if f.seek(SeekFrom::Start(pos)).is_err() || f.read_exact(&mut h).is_err() {
            return;
        }
        let len = u32::from_be_bytes([0, h[1], h[2], h[3]]) as u64;
        if h[0] & 0x7F == 4 {
            let Some(b) = take(f, len, MAX_COMMENT_BLOCK) else { return };
            // Lengths come from the file: checked, so a corrupt one ends the walk instead of
            // overflowing a 32-bit usize on the device.
            let u32_at = |i: usize| {
                b.get(i..i.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
            };
            let Some(vendor) = u32_at(0) else { return };
            let Some(mut i) = vendor.checked_add(4) else { return };
            let Some(count) = u32_at(i) else { return };
            i += 4;
            for _ in 0..count {
                let Some(n) = u32_at(i) else { return };
                let Some(c) = (i + 4).checked_add(n).and_then(|e| b.get(i + 4..e)) else { return };
                i += 4 + n;
                if let Some(eq) = c.iter().position(|&x| x == b'=') {
                    let key = std::str::from_utf8(&c[..eq]).unwrap_or("");
                    if ["LYRICS", "UNSYNCEDLYRICS", "UNSYNCED LYRICS"].iter().any(|k| k.eq_ignore_ascii_case(key)) {
                        out.push(String::from_utf8_lossy(&c[eq + 1..]).into_owned());
                    }
                }
            }
            return;
        }
        if h[0] & 0x80 != 0 {
            return;
        }
        pos += 4 + len;
    }
}

/// An ID3v2 tag at the start of the file: every `USLT` frame's text. Returns where the tag ends, so
/// a FLAC behind it can be read too.
fn id3_uslt(f: &mut File, head: &[u8; 12], out: &mut Vec<String>) -> Option<u64> {
    let syncsafe = |b: &[u8]| b.iter().fold(0u64, |a, &x| (a << 7) | (x & 0x7F) as u64);
    let (ver, flags) = (head[3], head[5]);
    let end = 10 + syncsafe(&head[6..10]) + if flags & 0x10 != 0 { 10 } else { 0 };
    // Whole-tag unsynchronisation (v2.2/2.3) rewrites every frame's bytes; such tags are rare
    // enough to skip rather than undo. The tag's end is still known.
    if !(2..=4).contains(&ver) || (ver < 4 && flags & 0x80 != 0) {
        return Some(end);
    }
    let mut pos = 10u64;
    if flags & 0x40 != 0 && ver >= 3 {
        f.seek(SeekFrom::Start(10)).ok()?;
        let mut e = [0u8; 4];
        f.read_exact(&mut e).ok()?;
        pos += if ver == 4 { syncsafe(&e) } else { 4 + u32::from_be_bytes(e) as u64 };
    }
    let (hdr, id) = if ver == 2 { (6u64, &b"ULT"[..]) } else { (10u64, &b"USLT"[..]) };
    while pos + hdr <= end {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut h = [0u8; 10];
        f.read_exact(&mut h[..hdr as usize]).ok()?;
        if h[0] == 0 {
            break; // padding
        }
        let idl = id.len();
        let len = match ver {
            2 => u32::from_be_bytes([0, h[3], h[4], h[5]]) as u64,
            3 => u32::from_be_bytes([h[4], h[5], h[6], h[7]]) as u64,
            _ => syncsafe(&h[4..8]),
        };
        // Compressed, encrypted or (v2.4) unsynchronised frames: skip the frame.
        let mangled = (ver == 4 && h[9] & 0x0E != 0) || (ver == 3 && h[9] & 0xC0 != 0);
        if &h[..idl] == id && !mangled {
            if let Some(b) = take(f, len, MAX_BYTES) {
                // v2.4's data-length indicator puts four bytes of size before the frame's own.
                let body = if ver == 4 && h[9] & 0x01 != 0 { b.get(4..) } else { Some(&b[..]) };
                if let Some(text) = body.and_then(uslt_text) {
                    out.push(text);
                }
            }
        }
        pos += hdr + len;
    }
    Some(end)
}

/// `USLT` body: encoding, language, a terminated description, then the lyrics.
fn uslt_text(b: &[u8]) -> Option<String> {
    let (&enc, rest) = b.split_first()?;
    let rest = rest.get(3..)?;
    let wide = enc == 1 || enc == 2;
    let body = if wide {
        let i = rest.chunks_exact(2).position(|u| u == [0, 0])?;
        &rest[(i + 1) * 2..]
    } else {
        &rest[rest.iter().position(|&x| x == 0)? + 1..]
    };
    Some(match enc {
        0 => body.iter().map(|&x| x as char).collect(),
        1 => decode(body), // UTF-16 with a BOM
        2 => decode(&[&[0xFE, 0xFF][..], body].concat()), // UTF-16BE, no BOM
        _ => String::from_utf8_lossy(body).into_owned(),
    })
}

/// MP4 atoms in `[pos, end)`: descend `moov › udta › meta › ilst` to `©lyr › data`.
fn mp4_lyr(f: &mut File, mut pos: u64, end: u64, depth: u32, out: &mut Vec<String>) {
    const PATH: [&[u8; 4]; 5] = [b"moov", b"udta", b"meta", b"ilst", b"\xA9lyr"];
    while pos + 8 <= end {
        let mut h = [0u8; 16];
        if f.seek(SeekFrom::Start(pos)).is_err() || f.read_exact(&mut h[..8]).is_err() {
            return;
        }
        let (mut size, mut hlen) = (u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64, 8u64);
        if size == 1 {
            if f.read_exact(&mut h[8..16]).is_err() {
                return;
            }
            size = u64::from_be_bytes(h[8..16].try_into().unwrap());
            hlen = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < hlen || pos + size > end {
            return;
        }
        let name = &h[4..8];
        if depth == PATH.len() as u32 {
            // Inside ©lyr: the `data` atom is type(4) + locale(4) + UTF-8 text.
            if name == b"data" {
                if let Some(b) = take(f, size - hlen, MAX_BYTES) {
                    if b.len() > 8 {
                        out.push(String::from_utf8_lossy(&b[8..]).into_owned());
                    }
                }
            }
        } else if name == PATH[depth as usize] {
            // `meta` is a full box: four bytes of version and flags before its children.
            let skip = if name == b"meta" { 4 } else { 0 };
            mp4_lyr(f, pos + hlen + skip, pos + size, depth + 1, out);
            return;
        }
        pos += size;
    }
}

/// The `.lrc` beside a track. Tried in the spellings people actually use rather than by listing the
/// folder: a large album folder would otherwise be read on every track change.
fn sidecar(track_path: &str) -> Option<PathBuf> {
    let p = Path::new(track_path);
    p.file_stem()?;
    ["lrc", "LRC", "Lrc"].iter().map(|ext| p.with_extension(ext)).find(|c| c.is_file())
}

/// Bytes to text. A BOM decides when there is one; otherwise UTF-8 when it is valid, and Windows-1252
/// when it is not — the encoding older lyric tools on Windows wrote.
fn decode(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(le) = match b {
        [0xFF, 0xFE, ..] => Some(true),
        [0xFE, 0xFF, ..] => Some(false),
        _ => None,
    } {
        let units = b[2..].chunks_exact(2).map(|u| {
            if le {
                u16::from_le_bytes([u[0], u[1]])
            } else {
                u16::from_be_bytes([u[0], u[1]])
            }
        });
        return char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect();
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&x| cp1252(x)).collect(),
    }
}

/// Windows-1252: Latin-1 with punctuation in 0x80..0x9F, where Latin-1 has control codes.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
        '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
        '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx`, in milliseconds.
fn timestamp(tag: &str) -> Option<u32> {
    let digits = |s: &str, max: usize| !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit());
    let (min, rest) = tag.split_once(':')?;
    let (sec, frac) = match rest.find(['.', ':']) {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    if !digits(min, 3) || !digits(sec, 2) {
        return None;
    }
    let sec: u32 = sec.parse().ok()?;
    if sec > 59 {
        return None;
    }
    let frac_ms = match frac {
        None => 0,
        Some(f) if digits(f, 3) => f.parse::<u32>().ok()? * [100, 10, 1][f.len() - 1],
        Some(_) => return None,
    };
    Some(min.parse::<u32>().ok()? * 60_000 + sec * 1000 + frac_ms)
}

/// Metadata tags LRC tools write. Only `offset` changes anything; the rest are dropped. A bracket
/// that is not one of these — `[Chorus]`, `[Verse 1: Name]` — is part of the words.
const META_KEYS: &[&str] = &["ar", "al", "ti", "au", "by", "length", "offset", "re", "ve", "tool", "id", "la", "lang", "#"];

/// Remove enhanced-LRC word stamps (`<00:12.34>`), leaving any other angle brackets alone.
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(close) if timestamp(&after[..close]).is_some() => rest = &after[close + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn parse(text: &str) -> Lyrics {
    let mut offset_ms: i64 = 0;
    let mut timed: Vec<(u32, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();

    // iTunes writes `©lyr` with bare CR line breaks, which `lines()` alone would run together.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for raw in text.lines() {
        let mut rest = raw.trim_start_matches('\u{FEFF}').trim();
        let mut stamps: Vec<u32> = Vec::new();
        let mut had_meta = false;
        while let Some(body) = rest.strip_prefix('[') {
            let Some(close) = body.find(']') else { break };
            let tag = &body[..close];
            if let Some(ms) = timestamp(tag) {
                stamps.push(ms);
            } else if let Some((key, val)) = tag.split_once(':').filter(|(k, _)| {
                META_KEYS.iter().any(|m| m.eq_ignore_ascii_case(k.trim()))
            }) {
                if key.trim().eq_ignore_ascii_case("offset") {
                    offset_ms = val.trim().trim_start_matches('+').parse().unwrap_or(0);
                }
                had_meta = true;
            } else {
                break;
            }
            rest = body[close + 1..].trim_start();
        }
        let words: String = strip_word_stamps(rest).chars().filter(|c| !c.is_control()).collect();
        let words = words.trim().to_string();
        if !stamps.is_empty() {
            timed.extend(stamps.into_iter().map(|ms| (ms, words.clone())));
        } else if !(had_meta && words.is_empty()) {
            plain.push(words);
        }
    }

    if !timed.is_empty() {
        // Stable, so two lines stamped at the same moment keep the order the file gave them.
        timed.sort_by_key(|(ms, _)| *ms);
        // A positive offset shows the words EARLIER — the LRC convention.
        let lines = timed
            .into_iter()
            .map(|(ms, text)| Line { at_ms: Some((ms as i64 - offset_ms).clamp(0, u32::MAX as i64) as u32), text })
            .collect();
        return Lyrics { lines };
    }

    // Plain text: keep single blank lines (they separate verses), collapse runs, trim both ends.
    let mut lines: Vec<Line> = Vec::new();
    for words in plain {
        if words.is_empty() && lines.last().map_or(true, |l: &Line| l.text.is_empty()) {
            continue;
        }
        lines.push(Line { at_ms: None, text: words });
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    Lyrics { lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamps(l: &Lyrics) -> Vec<(Option<u32>, &str)> {
        l.lines.iter().map(|l| (l.at_ms, l.text.as_str())).collect()
    }

    #[test]
    fn a_synced_file_reads_as_timed_lines() {
        let l = parse("[ti:Song]\n[ar:Someone]\n[00:12.00]Line one\r\n[00:15.50] Line two\n");
        assert_eq!(stamps(&l), vec![(Some(12_000), "Line one"), (Some(15_500), "Line two")]);
    }

    #[test]
    fn repeated_stamps_become_one_line_each_in_time_order() {
        let l = parse("[00:30.00][00:10.00]Chorus\n[00:20.00]Verse");
        assert_eq!(stamps(&l), vec![(Some(10_000), "Chorus"), (Some(20_000), "Verse"), (Some(30_000), "Chorus")]);
    }

    #[test]
    fn every_fraction_width_is_read_as_what_it_means() {
        assert_eq!(timestamp("01:02"), Some(62_000));
        assert_eq!(timestamp("01:02.3"), Some(62_300));
        assert_eq!(timestamp("01:02.34"), Some(62_340));
        assert_eq!(timestamp("01:02.345"), Some(62_345));
        assert_eq!(timestamp("01:02:34"), Some(62_340));
        assert_eq!(timestamp("100:00.00"), Some(6_000_000));
        assert_eq!(timestamp("01:60.00"), None);
        assert_eq!(timestamp("ar:Someone"), None);
        assert_eq!(timestamp("01:02.3456"), None);
    }

    #[test]
    fn a_positive_offset_shows_the_words_earlier() {
        assert_eq!(parse("[offset:+500]\n[00:10.00]a").lines[0].at_ms, Some(9_500));
        assert_eq!(parse("[offset:-500]\n[00:10.00]a").lines[0].at_ms, Some(10_500));
        assert_eq!(parse("[offset:2000]\n[00:01.00]a").lines[0].at_ms, Some(0), "clamped, not wrapped");
    }

    #[test]
    fn brackets_that_are_not_tags_stay_in_the_words() {
        let l = parse("[00:05.00][Chorus] Hey\n[00:06.00][Verse 1: Name] Ho");
        assert_eq!(stamps(&l), vec![(Some(5_000), "[Chorus] Hey"), (Some(6_000), "[Verse 1: Name] Ho")]);
    }

    #[test]
    fn enhanced_word_stamps_are_removed() {
        let l = parse("[00:01.00]<00:01.00>Hello <00:01.50>world <3");
        assert_eq!(l.lines[0].text, "Hello world <3");
    }

    #[test]
    fn an_empty_timed_line_is_kept_as_a_gap() {
        let l = parse("[00:01.00]Words\n[00:09.00]\n[00:20.00]More");
        assert_eq!(stamps(&l), vec![(Some(1_000), "Words"), (Some(9_000), ""), (Some(20_000), "More")]);
    }

    #[test]
    fn plain_text_keeps_verse_breaks_and_drops_the_rest_of_the_blank_space() {
        let l = parse("\n\nFirst verse\nline two\n\n\n\nSecond verse\n\n");
        assert_eq!(stamps(&l), vec![(None, "First verse"), (None, "line two"), (None, ""), (None, "Second verse")]);
        assert!(!l.is_synced());
    }

    #[test]
    fn every_encoding_a_lyric_tool_writes_decodes() {
        assert_eq!(decode(b"\xEF\xBB\xBFBj\xC3\xB6rk"), "Björk");
        assert_eq!(decode(b"\xFF\xFEB\x00j\x00\xF6\x00"), "Bjö");
        assert_eq!(decode(b"\xFE\xFF\x00B\x00j"), "Bj");
        assert_eq!(decode(b"Bj\xF6rk"), "Björk", "Latin-1");
        assert_eq!(decode(b"don\x92t \x93stop\x94"), "don\u{2019}t \u{201C}stop\u{201D}", "Windows-1252 punctuation");
    }

    #[test]
    fn the_lrc_beside_the_track_is_found_and_nothing_else_is() {
        let dir = std::env::temp_dir().join(format!("cinder-lyrics-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let track = dir.join("01 Song.flac");
        std::fs::write(&track, b"not audio").unwrap();
        let track = track.to_str().unwrap();
        assert_eq!(load_for(track), None, "no .lrc yet");

        std::fs::write(dir.join("01 Song.LRC"), "[00:01.00]Hello").unwrap();
        assert_eq!(load_for(track).map(|l| l.lines.len()), Some(1), "found in upper case");

        std::fs::write(dir.join("01 Song.LRC"), "[ti:Only metadata]\n").unwrap();
        assert_eq!(load_for(track), None, "a file with no words is no lyrics");

        std::fs::write(dir.join("01 Song.LRC"), vec![b'a'; MAX_BYTES as usize + 1]).unwrap();
        assert_eq!(load_for(track), None, "an oversized file is not read");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── Embedded tags ────────────────────────────────────────────────────────────────────────
    //
    // The containers are built here byte by byte in the block order real files use (the reference
    // library's FLACs: STREAMINFO, SEEKTABLE or PICTURE, VORBIS_COMMENT, PICTURE, PADDING). The
    // lyric texts copy the two shapes found in those files — LRC with `[ti:]`/`[ar:]`/`[by:]` and a
    // blank line, and plain text with CRLF and runs of blank lines — with the words replaced.

    /// A scratch file per test. The tests run in parallel in one process, so each names its own.
    fn scratch(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cinder-lyrics-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn load(p: &Path) -> Option<Lyrics> {
        load_for(p.to_str().unwrap())
    }

    fn flac_block(v: &mut Vec<u8>, kind: u8, body: &[u8]) {
        v.push(kind);
        v.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        v.extend_from_slice(body);
    }

    fn flac(comments: &[&str]) -> Vec<u8> {
        let mut vc = 6u32.to_le_bytes().to_vec();
        vc.extend_from_slice(b"vendor");
        vc.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            vc.extend_from_slice(&(c.len() as u32).to_le_bytes());
            vc.extend_from_slice(c.as_bytes());
        }
        let mut v = b"fLaC".to_vec();
        flac_block(&mut v, 0, &[0; 34]);
        flac_block(&mut v, 6, &[0xAB; 5000]);
        flac_block(&mut v, 4, &vc);
        flac_block(&mut v, 0x80 | 1, &[0; 128]);
        v.extend_from_slice(&[0xFF, 0xF8, 0x69, 0x08]);
        v
    }

    fn syncsafe(n: u32) -> [u8; 4] {
        [(n >> 21) as u8 & 0x7F, (n >> 14) as u8 & 0x7F, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]
    }

    fn id3(ver: u8, frames: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, data) in frames {
            body.extend_from_slice(*id);
            let n = data.len() as u32;
            body.extend_from_slice(&if ver == 4 { syncsafe(n) } else { n.to_be_bytes() });
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(data);
        }
        body.resize(body.len() + 256, 0);
        let mut v = b"ID3".to_vec();
        v.extend_from_slice(&[ver, 0, 0]);
        v.extend_from_slice(&syncsafe(body.len() as u32));
        v.extend(body);
        v
    }

    fn atom(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(name);
        v.extend_from_slice(body);
        v
    }

    fn mp4_data(text: &str) -> Vec<u8> {
        atom(b"data", &[&[0, 0, 0, 1, 0, 0, 0, 0][..], text.as_bytes()].concat())
    }

    #[test]
    fn a_flac_tag_is_read_in_both_shapes_taggers_write() {
        let synced = "LYRICS=[ti:Song]\n[ar:Someone]\n[by:Tool]\n\n[00:05.94]First line\n[00:10.60] Second line\n";
        let p = scratch("synced.flac", &flac(&["TITLE=Song", synced, "replaygain_track_gain=-7.1 dB"]));
        let l = load(&p).expect("LYRICS tag read");
        assert_eq!(stamps(&l), vec![(Some(5_940), "First line"), (Some(10_600), "Second line")]);

        let plain = "unsyncedlyrics=Verse one\r\nline two\r\n\r\n\r\n\r\nVerse two\r\n";
        let p = scratch("plain.flac", &flac(&["ARTIST=Someone", plain]));
        let l = load(&p).expect("key matched case-insensitively");
        assert_eq!(stamps(&l), vec![(None, "Verse one"), (None, "line two"), (None, ""), (None, "Verse two")]);

        let p = scratch("none.flac", &flac(&["TITLE=Song", "LYRICIST=Someone", "LYRICS="]));
        assert_eq!(load(&p), None, "LYRICIST is not lyrics, and an empty tag is none");
    }

    #[test]
    fn synced_tags_win_over_plain_and_a_lrc_wins_over_both() {
        let tags = flac(&["LYRICS=Plain words", "UNSYNCEDLYRICS=[00:01.00]Timed words"]);
        let p = scratch("both.flac", &tags);
        assert_eq!(stamps(&load(&p).unwrap()), vec![(Some(1_000), "Timed words")]);

        std::fs::write(p.with_extension("lrc"), "[00:02.00]From the file").unwrap();
        assert_eq!(stamps(&load(&p).unwrap()), vec![(Some(2_000), "From the file")]);

        std::fs::write(p.with_extension("lrc"), "[ti:No words]\n").unwrap();
        assert_eq!(stamps(&load(&p).unwrap()), vec![(Some(1_000), "Timed words")], "an empty .lrc hides nothing");
        std::fs::remove_file(p.with_extension("lrc")).ok();
    }

    #[test]
    fn an_id3_uslt_frame_is_read_and_so_is_a_flac_behind_an_id3_tag() {
        let latin1 = [&[0u8][..], b"eng", b"desc\0", b"Bj\xF6rk\nline two"].concat();
        let mp3 = [id3(3, &[(b"TIT2", vec![0, b'x']), (b"APIC", vec![0; 4000]), (b"USLT", latin1)]), vec![0xFF, 0xFB, 0x90, 0]].concat();
        let p = scratch("latin1.mp3", &mp3);
        assert_eq!(stamps(&load(&p).unwrap()), vec![(None, "Björk"), (None, "line two")]);

        let utf16: Vec<u8> = "[00:03.00]Bjö".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let wide = [&[1u8][..], b"eng", &[0xFF, 0xFE, 0, 0], &[0xFF, 0xFE], &utf16].concat();
        let p = scratch("utf16.mp3", &[id3(4, &[(b"USLT", wide)]), vec![0xFF, 0xFB, 0x90, 0]].concat());
        assert_eq!(stamps(&load(&p).unwrap()), vec![(Some(3_000), "Bjö")]);

        let p = scratch("id3.flac", &[id3(3, &[(b"TIT2", vec![0, b'x'])]), flac(&["LYRICS=Behind the ID3"])].concat());
        assert_eq!(stamps(&load(&p).unwrap()), vec![(None, "Behind the ID3")]);
    }

    #[test]
    fn an_mp4_lyr_atom_is_read_with_its_bare_cr_line_breaks() {
        let ilst = [atom(b"\xA9nam", &mp4_data("Song")), atom(b"\xA9lyr", &mp4_data("Line one\rLine two"))].concat();
        let meta = [&[0u8, 0, 0, 0][..], &atom(b"hdlr", &[0; 25]), &atom(b"ilst", &ilst)].concat();
        let moov = [atom(b"mvhd", &[0; 100]), atom(b"udta", &atom(b"meta", &meta))].concat();
        let m4a = [atom(b"ftyp", b"M4A \0\0\0\0"), atom(b"mdat", &[0; 3000]), atom(b"moov", &moov)].concat();
        let p = scratch("song.m4a", &m4a);
        assert_eq!(stamps(&load(&p).unwrap()), vec![(None, "Line one"), (None, "Line two")]);
    }

    #[test]
    fn corrupt_tag_lengths_are_no_lyrics_not_a_panic() {
        // A comment whose length runs past the block, and a count far beyond what is there.
        let mut vc = 0u32.to_le_bytes().to_vec();
        vc.extend_from_slice(&u32::MAX.to_le_bytes());
        vc.extend_from_slice(&u32::MAX.to_le_bytes());
        vc.extend_from_slice(b"LYRICS=x");
        let mut f = b"fLaC".to_vec();
        flac_block(&mut f, 0x80 | 4, &vc);
        assert_eq!(load(&scratch("bad-count.flac", &f)), None);

        // A vendor length that points past the end of the block.
        let mut f = b"fLaC".to_vec();
        flac_block(&mut f, 0x80 | 4, &(u32::MAX - 2).to_le_bytes());
        assert_eq!(load(&scratch("bad-vendor.flac", &f)), None);

        // A block claiming 16 MB in a tiny file.
        let mut f = b"fLaC".to_vec();
        f.extend_from_slice(&[4, 0xFF, 0xFF, 0xFF, 0, 0]);
        assert_eq!(load(&scratch("bad-block.flac", &f)), None);

        // An ID3 frame longer than the tag, and a tag longer than the file.
        let mut t = id3(3, &[(b"USLT", [&[0u8][..], b"eng\0words"].concat())]);
        t[14..18].copy_from_slice(&0x7FFF_FFF0u32.to_be_bytes());
        assert_eq!(load(&scratch("bad-frame.mp3", &t)), None);
        let mut t = id3(4, &[]);
        t[6..10].copy_from_slice(&[0x7F; 4]);
        assert_eq!(load(&scratch("bad-tag.mp3", &t)), None);

        // An MP4 atom that runs past its parent, and a file too short to have a header.
        let m4a = [atom(b"ftyp", b"M4A "), vec![0x7F, 0, 0, 0], b"moov".to_vec()].concat();
        assert_eq!(load(&scratch("bad-atom.m4a", &m4a)), None);
        assert_eq!(load(&scratch("short.flac", b"fLa")), None);
    }
}
