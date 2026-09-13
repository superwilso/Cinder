//! Lyrics for a track, from a `.lrc` file next to it.
//!
//! `Song.flac` → `Song.lrc`, in the same folder — the convention Sony's own player uses, so files
//! people already made for it work unchanged. It needs no scan: drop the file in over USB and it is
//! there the next time the song starts.
//!
//! LRC is a loose format written by many tools, so the parser is lenient about everything that does
//! not change which words are shown when: fraction widths, repeated timestamps on one line,
//! metadata tags, enhanced (per-word) stamps, and the encoding. A file that cannot be read simply
//! has no lyrics; nothing here can fail loudly.

use cinder_ui::lyrics::{Line, Lyrics};
use std::path::{Path, PathBuf};

/// Anything bigger is not a lyrics file. A long song's LRC is a few kilobytes; the cap keeps a
/// mis-named file from being read into memory on the render thread.
const MAX_BYTES: u64 = 512 * 1024;

/// Lyrics for the track at `track_path`, or `None` when there is no readable `.lrc` with words in it.
pub fn load_for(track_path: &str) -> Option<Lyrics> {
    let path = sidecar(track_path)?;
    let len = std::fs::metadata(&path).ok()?.len();
    if len == 0 || len > MAX_BYTES {
        return None;
    }
    let lyr = parse(&decode(&std::fs::read(&path).ok()?));
    if lyr.lines.iter().any(|l| !l.text.trim().is_empty()) {
        Some(lyr)
    } else {
        None
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
}
