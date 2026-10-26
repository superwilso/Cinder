//! Audiobook series — which series a book belongs to, and which number it is in it.
//!
//! Sony's library has no series field, so this reads the tags of a book's first chapter file. The
//! names are the ones Audiobookshelf and the usual taggers write, so a collection tagged for one
//! reads here too:
//!
//! | format | series | number in the series |
//! |---|---|---|
//! | MP3 (ID3v2.2/2.3/2.4) | `TXXX` `series`, or `MVNM` | `TXXX` `series-part`, or `MVIN` |
//! | M4B / M4A / MP4 | freeform `series`, or `©mvn` | freeform `series-part`, or `mvin` |
//! | FLAC, Ogg Vorbis, Opus | `SERIES` | `SERIESPART` / `SERIES-PART` |
//!
//! Everything is read through `Read + Seek` and only the few small frames wanted are pulled in, so
//! a cover picture of several megabytes in front of the tag costs a seek, not a read. A file that
//! does not parse, or has no series, is `None`; it never costs the book its place in the list.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// A frame, atom or comment larger than this is not a series name.
const MAX_FIELD: u64 = 4096;
/// How much of an Ogg file is searched for the comment header.
const OGG_SCAN: usize = 64 * 1024;
/// A FLAC comment block larger than this is not read.
const MAX_BLOCK: u32 = 1 << 20;

/// A book's series.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    pub name: String,
    /// The number in the series as written (`"3"`, `"2.5"`), empty when the tag has none.
    pub part: String,
}

/// Is `key` one of the spellings of the series name?
fn is_name_key(key: &str) -> bool {
    key.trim().eq_ignore_ascii_case("series")
}

/// Is `key` one of the spellings of the number in the series?
fn is_part_key(key: &str) -> bool {
    let k: String = key
        .trim()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    k.eq_ignore_ascii_case("seriespart")
}

/// Tidy a number as tagged: `"03"` is `"3"`, `"3/10"` is `"3"`, `"2.50"` is `"2.5"`. Anything that
/// is not a number is dropped (`""`): a part that cannot be ordered must not be shown as one.
pub fn clean_part(raw: &str) -> String {
    let head = raw.split('/').next().unwrap_or("").trim();
    match head.parse::<f64>() {
        Ok(n) if n.is_finite() && n >= 0.0 => format!("{n}"),
        _ => String::new(),
    }
}

/// Collects what a tag says and turns it into a [`Series`].
#[derive(Default)]
struct Found {
    name: Option<String>,
    part: Option<String>,
}

impl Found {
    fn set_name(&mut self, v: &str) {
        let v = v.trim();
        if !v.is_empty() && self.name.is_none() {
            self.name = Some(v.to_string());
        }
    }
    fn set_part(&mut self, v: &str) {
        let v = clean_part(v);
        if !v.is_empty() && self.part.is_none() {
            self.part = Some(v);
        }
    }
    fn done(self) -> Option<Series> {
        Some(Series {
            name: self.name?,
            part: self.part.unwrap_or_default(),
        })
    }
}

fn read_exact_at<R: Read + Seek>(r: &mut R, off: u64, len: usize) -> Option<Vec<u8>> {
    r.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

// ── ID3v2 ──────────────────────────────────────────────────────────────────────────────────────

fn synchsafe(b: &[u8]) -> u64 {
    b.iter()
        .take(4)
        .fold(0, |a, &x| (a << 7) | u64::from(x & 0x7f))
}

fn be(b: &[u8]) -> u64 {
    b.iter().fold(0, |a, &x| (a << 8) | u64::from(x))
}

/// Decode `bytes` in ID3 text encoding `enc` (0 Latin-1, 1 UTF-16 with BOM, 2 UTF-16BE, 3 UTF-8).
fn id3_text(enc: u8, bytes: &[u8]) -> String {
    match enc {
        1 | 2 => {
            let (le, bytes) = match bytes {
                [0xff, 0xfe, rest @ ..] => (true, rest),
                [0xfe, 0xff, rest @ ..] => (false, rest),
                _ => (false, bytes),
            };
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| {
                    if le {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect();
            String::from_utf16_lossy(&units)
        }
        3 => String::from_utf8_lossy(bytes).into_owned(),
        _ => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Split a `TXXX` body (after the encoding byte) at the description's terminator.
fn split_desc(enc: u8, body: &[u8]) -> (&[u8], &[u8]) {
    if matches!(enc, 1 | 2) {
        let mut i = 0;
        while i + 1 < body.len() {
            if body[i] == 0 && body[i + 1] == 0 {
                return (&body[..i], &body[i + 2..]);
            }
            i += 2;
        }
        (body, &[])
    } else {
        match body.iter().position(|&b| b == 0) {
            Some(i) => (&body[..i], &body[i + 1..]),
            None => (body, &[]),
        }
    }
}

fn id3_trim(s: String) -> String {
    s.trim_end_matches('\0').to_string()
}

fn from_id3<R: Read + Seek>(r: &mut R) -> Option<Series> {
    let head = read_exact_at(r, 0, 10)?;
    if &head[..3] != b"ID3" {
        return None;
    }
    let major = head[3];
    if !(2..=4).contains(&major) {
        return None;
    }
    let end = 10 + synchsafe(&head[6..10]);
    let mut pos = 10u64;
    if major >= 3 && head[5] & 0x40 != 0 {
        let h = read_exact_at(r, pos, 4)?;
        pos += if major == 4 {
            synchsafe(&h)
        } else {
            4 + be(&h)
        };
    }
    let (id_len, hdr_len) = if major == 2 { (3, 6) } else { (4, 10) };
    let mut found = Found::default();
    while pos + hdr_len <= end {
        let h = read_exact_at(r, pos, hdr_len as usize)?;
        if h[0] == 0 {
            break; // padding
        }
        let id = &h[..id_len];
        let size = match (major, id_len) {
            (2, _) => be(&h[3..6]),
            (4, _) => synchsafe(&h[4..8]),
            _ => be(&h[4..8]),
        };
        let body_at = pos + hdr_len;
        pos = body_at + size;
        let wanted = matches!(id, b"TXXX" | b"TXX" | b"MVNM" | b"MVIN");
        if !wanted || size == 0 || size > MAX_FIELD || pos > end {
            continue;
        }
        let Some(body) = read_exact_at(r, body_at, size as usize) else {
            break;
        };
        let (enc, rest) = (body[0], &body[1..]);
        match id {
            b"TXXX" | b"TXX" => {
                let (desc, val) = split_desc(enc, rest);
                let (desc, val) = (id3_trim(id3_text(enc, desc)), id3_trim(id3_text(enc, val)));
                if is_name_key(&desc) {
                    found.set_name(&val);
                } else if is_part_key(&desc) {
                    found.set_part(&val);
                }
            }
            b"MVNM" => found.set_name(&id3_trim(id3_text(enc, rest))),
            _ => found.set_part(&id3_trim(id3_text(enc, rest))),
        }
    }
    found.done()
}

// ── Vorbis comments (FLAC, Ogg Vorbis, Opus) ───────────────────────────────────────────────────

/// Read a Vorbis comment block: vendor string, then `KEY=value` entries.
fn from_vorbis_comments(b: &[u8]) -> Option<Series> {
    let le32 = |at: usize| -> Option<usize> {
        let w = b.get(at..at + 4)?;
        Some(u32::from_le_bytes([w[0], w[1], w[2], w[3]]) as usize)
    };
    let vendor = le32(0)?;
    let mut at = 4usize.checked_add(vendor)?;
    let count = le32(at)?;
    at += 4;
    let mut found = Found::default();
    for _ in 0..count {
        let Some(len) = le32(at) else { break };
        at += 4;
        let Some(raw) = at.checked_add(len).and_then(|e| b.get(at..e)) else {
            break;
        };
        at += len;
        let entry = String::from_utf8_lossy(raw);
        if let Some((k, v)) = entry.split_once('=') {
            if is_name_key(k) {
                found.set_name(v);
            } else if is_part_key(k) {
                found.set_part(v);
            }
        }
    }
    found.done()
}

fn from_flac<R: Read + Seek>(r: &mut R) -> Option<Series> {
    if read_exact_at(r, 0, 4)? != b"fLaC" {
        return None;
    }
    let mut pos = 4u64;
    loop {
        let h = read_exact_at(r, pos, 4)?;
        let (last, kind, len) = (h[0] & 0x80 != 0, h[0] & 0x7f, be(&h[1..4]) as u32);
        pos += 4;
        if kind == 4 {
            if len > MAX_BLOCK {
                return None;
            }
            return from_vorbis_comments(&read_exact_at(r, pos, len as usize)?);
        }
        if last {
            return None;
        }
        pos += u64::from(len);
    }
}

fn from_ogg<R: Read + Seek>(r: &mut R) -> Option<Series> {
    r.seek(SeekFrom::Start(0)).ok()?;
    let mut buf = Vec::new();
    r.take(OGG_SCAN as u64).read_to_end(&mut buf).ok()?;
    if !buf.starts_with(b"OggS") {
        return None;
    }
    let find = |needle: &[u8]| buf.windows(needle.len()).position(|w| w == needle);
    let start = match (find(b"\x03vorbis"), find(b"OpusTags")) {
        (Some(i), _) => i + 7,
        (None, Some(i)) => i + 8,
        _ => return None,
    };
    from_vorbis_comments(&buf[start..])
}

// ── MP4 / M4B ──────────────────────────────────────────────────────────────────────────────────

/// One box: its type, and where its body starts and ends.
struct Atom {
    kind: [u8; 4],
    body: u64,
    end: u64,
}

fn atoms<R: Read + Seek>(r: &mut R, mut pos: u64, end: u64) -> Vec<Atom> {
    let mut out = Vec::new();
    while pos + 8 <= end {
        let Some(h) = read_exact_at(r, pos, 8) else {
            break;
        };
        let mut size = be(&h[..4]);
        let kind = [h[4], h[5], h[6], h[7]];
        let mut hdr = 8;
        if size == 1 {
            let Some(x) = read_exact_at(r, pos + 8, 8) else {
                break;
            };
            size = be(&x);
            hdr = 16;
        } else if size == 0 {
            size = end - pos;
        }
        // `checked_add`: a 64-bit size can wrap `pos` back to the start and loop for ever.
        if size < hdr || pos.checked_add(size).is_none_or(|e| e > end) {
            break;
        }
        out.push(Atom {
            kind,
            body: pos + hdr,
            end: pos + size,
        });
        pos += size;
    }
    out
}

/// The payload of an item's `data` box, as text. `mvin` is stored as a big-endian integer.
fn mp4_data<R: Read + Seek>(r: &mut R, item: &Atom) -> Option<String> {
    let d = atoms(r, item.body, item.end)
        .into_iter()
        .find(|a| &a.kind == b"data")?;
    let len = d.end.checked_sub(d.body + 8)?;
    if len == 0 || len > MAX_FIELD {
        return None;
    }
    let flags = read_exact_at(r, d.body, 4)?;
    let payload = read_exact_at(r, d.body + 8, len as usize)?;
    // Type 21 is a signed big-endian integer; 1 is UTF-8 text.
    if flags[3] == 21 {
        Some(be(&payload).to_string())
    } else {
        Some(String::from_utf8_lossy(&payload).into_owned())
    }
}

fn from_mp4<R: Read + Seek>(r: &mut R) -> Option<Series> {
    let total = r.seek(SeekFrom::End(0)).ok()?;
    let mut level = (0u64, total);
    for want in [&b"moov"[..], b"udta", b"meta", b"ilst"] {
        let a = atoms(r, level.0, level.1)
            .into_iter()
            .find(|a| &a.kind[..] == want)?;
        // `meta` is a full box: four bytes of version and flags come before its children.
        level = (if want == b"meta" { a.body + 4 } else { a.body }, a.end);
    }
    let mut found = Found::default();
    for item in atoms(r, level.0, level.1) {
        match &item.kind {
            [0xa9, b'm', b'v', b'n'] => {
                if let Some(v) = mp4_data(r, &item) {
                    found.set_name(&v);
                }
            }
            b"mvin" => {
                if let Some(v) = mp4_data(r, &item) {
                    found.set_part(&v);
                }
            }
            b"----" => {
                let kids = atoms(r, item.body, item.end);
                let name = kids.iter().find(|a| &a.kind == b"name").and_then(|a| {
                    let len = a.end.checked_sub(a.body + 4)?;
                    if len == 0 || len > MAX_FIELD {
                        return None;
                    }
                    let b = read_exact_at(r, a.body + 4, len as usize)?;
                    Some(String::from_utf8_lossy(&b).into_owned())
                });
                let Some(name) = name else { continue };
                if is_name_key(&name) || is_part_key(&name) {
                    let Some(v) = mp4_data(r, &item) else {
                        continue;
                    };
                    if is_name_key(&name) {
                        found.set_name(&v);
                    } else {
                        found.set_part(&v);
                    }
                }
            }
            _ => {}
        }
    }
    found.done()
}

// ── entry points ───────────────────────────────────────────────────────────────────────────────

/// The series a tagged stream names, whatever the format; `None` when it names none.
pub fn read<R: Read + Seek>(r: &mut R) -> Option<Series> {
    let magic = read_exact_at(r, 0, 12).unwrap_or_default();
    match magic.as_slice() {
        m if m.starts_with(b"ID3") => from_id3(r),
        m if m.starts_with(b"fLaC") => from_flac(r),
        m if m.starts_with(b"OggS") => from_ogg(r),
        m if m.len() >= 8 && &m[4..8] == b"ftyp" => from_mp4(r),
        _ => None,
    }
}

/// [`read`] for the file at `path`.
pub fn read_file(path: &str) -> Option<Series> {
    read(&mut File::open(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn series(name: &str, part: &str) -> Option<Series> {
        Some(Series {
            name: name.into(),
            part: part.into(),
        })
    }

    fn ss(n: usize) -> [u8; 4] {
        [
            ((n >> 21) & 0x7f) as u8,
            ((n >> 14) & 0x7f) as u8,
            ((n >> 7) & 0x7f) as u8,
            (n & 0x7f) as u8,
        ]
    }

    // ── ID3 ──
    fn frame(ver: u8, id: &str, body: &[u8]) -> Vec<u8> {
        let mut f = id.as_bytes().to_vec();
        if ver == 2 {
            f.extend(&(body.len() as u32).to_be_bytes()[1..]);
        } else if ver == 4 {
            f.extend(ss(body.len()));
            f.extend([0, 0]);
        } else {
            f.extend((body.len() as u32).to_be_bytes());
            f.extend([0, 0]);
        }
        f.extend(body);
        f
    }
    fn txxx(enc: u8, desc: &str, val: &str) -> Vec<u8> {
        let mut b = vec![enc];
        let put = |b: &mut Vec<u8>, s: &str| match enc {
            1 => {
                b.extend([0xff, 0xfe]);
                s.encode_utf16().for_each(|u| b.extend(u.to_le_bytes()));
            }
            _ => b.extend(s.as_bytes()),
        };
        put(&mut b, desc);
        b.extend(if enc == 1 { vec![0, 0] } else { vec![0] });
        put(&mut b, val);
        b
    }
    fn id3(ver: u8, frames: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = frames.concat();
        let mut t = vec![b'I', b'D', b'3', ver, 0, 0];
        t.extend(ss(body.len()));
        t.extend(body);
        t.extend([0; 16]); // padding, then "audio"
        t
    }
    fn parse(bytes: Vec<u8>) -> Option<Series> {
        read(&mut Cursor::new(bytes))
    }

    #[test]
    fn mp3_txxx_series_and_series_part_as_the_tagging_script_writes_them() {
        let t = id3(
            3,
            &[
                frame(3, "TXXX", &txxx(0, "artist", "TKKG")),
                frame(3, "TXXX", &txxx(0, "series", "TKKG")),
                frame(3, "TXXX", &txxx(0, "series-part", "12")),
            ],
        );
        assert_eq!(parse(t), series("TKKG", "12"));
    }

    #[test]
    fn mp3_v24_v22_and_utf16_are_read_too() {
        let v4 = id3(
            4,
            &[
                frame(4, "TXXX", &txxx(3, "SERIES", "Dune")),
                frame(4, "TXXX", &txxx(3, "SERIES-PART", "2")),
            ],
        );
        assert_eq!(parse(v4), series("Dune", "2"));
        let v2 = id3(
            2,
            &[
                frame(2, "TXX", &txxx(0, "series", "Dune")),
                frame(2, "TXX", &txxx(0, "series-part", "3")),
            ],
        );
        assert_eq!(parse(v2), series("Dune", "3"));
        let u16 = id3(
            3,
            &[
                frame(3, "TXXX", &txxx(1, "series", "Ärzte")),
                frame(3, "TXXX", &txxx(1, "series-part", "1")),
            ],
        );
        assert_eq!(parse(u16), series("Ärzte", "1"));
    }

    #[test]
    fn mp3_mvnm_and_mvin_frames_and_other_spellings_of_the_part() {
        let t = id3(
            4,
            &[
                frame(4, "MVNM", &[&[3u8][..], b"Discworld"].concat()),
                frame(4, "MVIN", &[&[3u8][..], b"05/41"].concat()),
            ],
        );
        assert_eq!(parse(t), series("Discworld", "5"));
        let t = id3(
            3,
            &[
                frame(3, "TXXX", &txxx(0, "Series", "S")),
                frame(3, "TXXX", &txxx(0, "SERIES_PART", "03")),
            ],
        );
        assert_eq!(parse(t), series("S", "3"));
    }

    #[test]
    fn a_series_with_no_number_is_still_a_series() {
        let t = id3(3, &[frame(3, "TXXX", &txxx(0, "series", "Loose"))]);
        assert_eq!(parse(t), series("Loose", ""));
    }

    #[test]
    fn a_number_alone_is_not_a_series() {
        let t = id3(3, &[frame(3, "TXXX", &txxx(0, "series-part", "4"))]);
        assert_eq!(parse(t), None);
    }

    #[test]
    fn a_big_cover_in_front_of_the_frames_is_skipped_not_read() {
        let apic = frame(3, "APIC", &vec![7u8; 3_000_000]);
        let t = id3(
            3,
            &[
                apic,
                frame(3, "TXXX", &txxx(0, "series", "Behind")),
                frame(3, "TXXX", &txxx(0, "series-part", "9")),
            ],
        );
        assert_eq!(parse(t), series("Behind", "9"));
    }

    #[test]
    fn untagged_and_unknown_and_truncated_files_have_no_series() {
        assert_eq!(parse(vec![]), None);
        assert_eq!(parse(b"RIFF....WAVEfmt ".to_vec()), None);
        assert_eq!(parse(id3(3, &[])), None);
        let mut cut = id3(3, &[frame(3, "TXXX", &txxx(0, "series", "Cut"))]);
        cut.truncate(14);
        assert_eq!(parse(cut), None);
    }

    #[test]
    fn parts_are_tidied_and_non_numbers_dropped() {
        assert_eq!(clean_part("03"), "3");
        assert_eq!(clean_part(" 2.50 "), "2.5");
        assert_eq!(clean_part("3/10"), "3");
        assert_eq!(clean_part("Book Three"), "");
        assert_eq!(clean_part("-1"), "");
        assert_eq!(clean_part(""), "");
    }

    // ── Vorbis comments ──
    fn comments(entries: &[&str]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(5u32.to_le_bytes());
        b.extend(b"cinde");
        b.extend((entries.len() as u32).to_le_bytes());
        for e in entries {
            b.extend((e.len() as u32).to_le_bytes());
            b.extend(e.as_bytes());
        }
        b
    }

    #[test]
    fn flac_series_comments() {
        let body = comments(&["TITLE=x", "SERIES=The Expanse", "SERIESPART=4"]);
        let mut f = b"fLaC".to_vec();
        f.extend([0x00, 0, 0, 4, 1, 2, 3, 4]); // a STREAMINFO-like block to skip
        f.extend([0x84]); // last block, type 4
        f.extend(&(body.len() as u32).to_be_bytes()[1..]);
        f.extend(body);
        assert_eq!(parse(f), series("The Expanse", "4"));
    }

    #[test]
    fn flac_without_comment_block_has_no_series() {
        let mut f = b"fLaC".to_vec();
        f.extend([0x80, 0, 0, 2, 9, 9]);
        assert_eq!(parse(f), None);
    }

    #[test]
    fn ogg_vorbis_and_opus_series_comments() {
        let body = comments(&["series=Foundation", "series-part=2"]);
        let mut vorbis = b"OggS\0\x02".to_vec();
        vorbis.extend([0u8; 30]);
        vorbis.extend(b"\x03vorbis");
        vorbis.extend(&body);
        assert_eq!(parse(vorbis), series("Foundation", "2"));
        let mut opus = b"OggS\0\x02".to_vec();
        opus.extend([0u8; 30]);
        opus.extend(b"OpusTags");
        opus.extend(&body);
        assert_eq!(parse(opus), series("Foundation", "2"));
    }

    #[test]
    fn a_comment_block_that_lies_about_its_length_is_survived() {
        let mut body = comments(&["SERIES=Ok", "SERIESPART=1"]);
        body.truncate(body.len() - 5);
        let mut f = b"OggS\0\x02".to_vec();
        f.extend(b"\x03vorbis");
        f.extend(&body);
        assert_eq!(parse(f), series("Ok", ""));
        assert_eq!(from_vorbis_comments(&[0xff; 8]), None);
    }

    // ── MP4 ──
    fn atom(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut a = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        a.extend(kind);
        a.extend(body);
        a
    }
    fn data(flags: u8, payload: &[u8]) -> Vec<u8> {
        atom(
            b"data",
            &[&[0, 0, 0, flags, 0, 0, 0, 0][..], payload].concat(),
        )
    }
    fn freeform(name: &str, value: &str) -> Vec<u8> {
        atom(
            b"----",
            &[
                atom(b"mean", b"\0\0\0\0com.apple.iTunes"),
                atom(b"name", &[&[0u8; 4][..], name.as_bytes()].concat()),
                data(1, value.as_bytes()),
            ]
            .concat(),
        )
    }
    fn m4b(items: &[Vec<u8>], moov_last: bool) -> Vec<u8> {
        let ilst = atom(b"ilst", &items.concat());
        let meta = atom(b"meta", &[&[0u8; 4][..], &ilst].concat());
        let moov = atom(b"moov", &atom(b"udta", &meta));
        let ftyp = atom(b"ftyp", b"M4B \0\0\0\0M4B ");
        let mdat = atom(b"mdat", &vec![0u8; 5000]);
        if moov_last {
            [ftyp, mdat, moov].concat()
        } else {
            [ftyp, moov, mdat].concat()
        }
    }

    #[test]
    fn m4b_freeform_series_atoms_wherever_moov_sits() {
        for moov_last in [false, true] {
            let f = m4b(
                &[
                    atom(b"\xa9nam", &data(1, b"Title")),
                    freeform("series", "Mistborn"),
                    freeform("series-part", "3"),
                ],
                moov_last,
            );
            assert_eq!(parse(f), series("Mistborn", "3"), "moov last: {moov_last}");
        }
    }

    #[test]
    fn m4b_movement_atoms() {
        let f = m4b(
            &[
                atom(b"\xa9mvn", &data(1, b"Witcher")),
                atom(b"mvin", &data(21, &[0, 0, 0, 2])),
            ],
            false,
        );
        assert_eq!(parse(f), series("Witcher", "2"));
    }

    #[test]
    fn m4b_without_series_or_without_ilst_has_none() {
        assert_eq!(parse(m4b(&[freeform("mood", "x")], false)), None);
        let bare = [atom(b"ftyp", b"M4B \0\0\0\0M4B "), atom(b"mdat", b"abc")].concat();
        assert_eq!(parse(bare), None);
    }

    #[test]
    fn an_m4b_atom_whose_size_wraps_ends_the_read() {
        // A 64-bit size that wraps the position back to the start: before the fix the box walk
        // looped for ever, growing its list until the player ran out of memory.
        let bad = [
            atom(b"ftyp", b"M4B \0\0\0\0"),
            [
                &1u32.to_be_bytes()[..],
                b"free",
                &(u64::MAX - 15).to_be_bytes(),
                &[0; 64],
            ]
            .concat(),
        ]
        .concat();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(parse(bad)));
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(3)), Ok(None));
    }
}
