//! CUE sheets: one audio file holding a whole album, with a `.cue` beside it naming the tracks.
//!
//! The library still lists the file once (Sony's database has one row for it). While it plays,
//! Now Playing shows the CUE track under the playhead, and ◁/▷ move between CUE tracks with a seek
//! instead of leaving the file (`cinder_cue_skip`); at either end they act as usual. The file plays
//! straight through, so the album stays gapless, which is why people keep albums this way.
//!
//! Found beside the playing file: `<name>.cue`, `<name>.<ext>.cue`, or any `.cue` in the folder
//! whose `FILE` line names it. Text is UTF-8 (a BOM is fine) or, failing that, Latin-1, the two
//! encodings CUE rippers write. Only `TRACK nn AUDIO` with an `INDEX 01` counts; a sheet with fewer
//! than two tracks for this file is ignored.

use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub title: String,
    pub performer: String,
    pub start_ms: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    /// Ordered by start.
    pub entries: Vec<Entry>,
}

/// `"Some Title"` or a bare word, from the rest of a CUE line.
fn quoted(rest: &str) -> String {
    let r = rest.trim();
    match r.strip_prefix('"') {
        Some(inner) => inner.split('"').next().unwrap_or("").to_string(),
        None => r.split_whitespace().next().unwrap_or("").to_string(),
    }
}

/// `mm:ss:ff` (75 frames a second) in ms.
fn msf(s: &str) -> Option<i64> {
    let mut p = s.trim().split(':').map(|x| x.parse::<i64>().ok());
    let (m, sec, f) = (p.next()??, p.next()??, p.next()??);
    (sec < 60 && f < 75).then_some((m * 60 + sec) * 1000 + f * 1000 / 75)
}

fn same_file(named: &str, file_name: &str) -> bool {
    let base = |s: &str| s.rsplit(['/', '\\']).next().unwrap_or(s).to_lowercase();
    let stem = |s: &str| {
        let b = base(s);
        b.rsplit_once('.').map_or(b.clone(), |(s, _)| s.to_string())
    };
    base(named) == base(file_name) || stem(named) == stem(file_name)
}

/// The tracks a sheet gives `file_name`, or None when it names other files only, or fewer than
/// two tracks. A sheet with ONE `FILE` line is taken for whatever file it sits beside: rippers
/// often name the `.wav` they ripped to and the file was converted afterwards.
pub fn parse(text: &str, file_name: &str) -> Option<Sheet> {
    let files = text
        .lines()
        .filter(|l| l.trim_start().to_ascii_uppercase().starts_with("FILE "))
        .count();
    let mut album_performer = String::new();
    let mut ours = false;
    let mut cur: Option<Entry> = None;
    let mut out: Vec<Entry> = Vec::new();
    let mut in_track = false;
    for line in text.lines() {
        let l = line.trim();
        let (word, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
        match word.to_ascii_uppercase().as_str() {
            "FILE" => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
                ours = files == 1 || same_file(&quoted(rest), file_name);
                in_track = false;
            }
            "TRACK" => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
                in_track = true;
                let audio = rest.to_ascii_uppercase().contains("AUDIO");
                cur = (ours && audio).then(|| Entry {
                    title: String::new(),
                    performer: String::new(),
                    start_ms: -1,
                });
            }
            "TITLE" if in_track => {
                if let Some(e) = cur.as_mut() {
                    e.title = quoted(rest);
                }
            }
            "PERFORMER" if in_track => {
                if let Some(e) = cur.as_mut() {
                    e.performer = quoted(rest);
                }
            }
            "PERFORMER" => album_performer = quoted(rest),
            "INDEX" => {
                let mut p = rest.split_whitespace();
                if p.next() == Some("01") {
                    if let (Some(e), Some(ms)) = (cur.as_mut(), p.next().and_then(msf)) {
                        e.start_ms = ms;
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(e) = cur.take() {
        out.push(e);
    }
    out.retain(|e| e.start_ms >= 0);
    out.sort_by_key(|e| e.start_ms);
    for (i, e) in out.iter_mut().enumerate() {
        if e.performer.is_empty() {
            e.performer = album_performer.clone();
        }
        if e.title.is_empty() {
            e.title = format!("Track {}", i + 1);
        }
    }
    (out.len() >= 2).then_some(Sheet { entries: out })
}

fn decode(b: &[u8]) -> String {
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(), // Latin-1
    }
}

/// The sheet for the audio file at `path`, if one sits beside it.
pub fn load_for(path: &str) -> Option<Sheet> {
    let p = Path::new(path);
    let dir = p.parent()?;
    let name = p.file_name()?.to_str()?;
    let stem = p.file_stem()?.to_str()?;
    let read = |q: &Path| -> Option<Sheet> {
        let b = std::fs::read(q).ok()?;
        if b.len() > 256 * 1024 {
            return None; // not a cue sheet
        }
        parse(&decode(&b), name)
    };
    for cand in [format!("{stem}.cue"), format!("{name}.cue")] {
        if let Some(s) = read(&dir.join(cand)) {
            return Some(s);
        }
    }
    // Any other sheet in the folder that names this file. ponytail: a folder of thousands of
    // files is read once per track change; a cache keyed on the folder if that ever shows.
    let mut tried = 0;
    for ent in std::fs::read_dir(dir).ok()?.flatten() {
        let n = ent.file_name();
        let n = n.to_string_lossy();
        if !n.to_ascii_lowercase().ends_with(".cue") {
            continue;
        }
        tried += 1;
        if tried > 16 {
            break;
        }
        let Ok(b) = std::fs::read(ent.path()) else {
            continue;
        };
        let text = decode(&b);
        // Only a sheet that names this file by FILE line: a folder's other albums' sheets must
        // not attach to it through the one-FILE rule.
        let names_it = text.lines().any(|l| {
            let t = l.trim_start();
            t.to_ascii_uppercase().starts_with("FILE ") && same_file(&quoted(&t[5..]), name)
        });
        if names_it {
            if let Some(s) = parse(&text, name) {
                return Some(s);
            }
        }
    }
    None
}

impl Sheet {
    /// The track playing at `pos_ms`.
    pub fn index_at(&self, pos_ms: i64) -> usize {
        self.entries
            .iter()
            .rposition(|e| e.start_ms <= pos_ms)
            .unwrap_or(0)
    }

    /// Where ▷ (`dir` > 0) or ◁ goes inside the file, or None to leave it as usual. ◁ more than
    /// 3 s into a track goes to its start, else to the track before.
    pub fn skip(&self, pos_ms: i64, dir: i32) -> Option<i64> {
        let i = self.index_at(pos_ms);
        if dir > 0 {
            return self.entries.get(i + 1).map(|e| e.start_ms);
        }
        let start = self.entries[i].start_ms;
        if pos_ms - start > 3000 {
            return Some(start);
        }
        (i > 0).then(|| self.entries[i - 1].start_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = "\u{feff}REM GENRE Jazz
PERFORMER \"Miles Davis\"
TITLE \"Kind of Blue\"
FILE \"Kind of Blue.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"So What\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Freddie Freeloader\"
    INDEX 00 09:20:30
    INDEX 01 09:22:00
  TRACK 03 AUDIO
    TITLE \"Blue in Green\"
    PERFORMER \"Miles Davis Sextet\"
    INDEX 01 19:08:37
";

    #[test]
    fn parses_tracks_starts_and_performers() {
        let s = parse(SHEET, "Kind of Blue.flac").expect("one FILE: taken for the file beside it");
        assert_eq!(s.entries.len(), 3);
        assert_eq!(s.entries[1].title, "Freddie Freeloader");
        assert_eq!(
            s.entries[1].start_ms,
            (9 * 60 + 22) * 1000,
            "INDEX 01, not 00"
        );
        assert_eq!(s.entries[2].start_ms, (19 * 60 + 8) * 1000 + 37 * 1000 / 75);
        assert_eq!(
            s.entries[0].performer, "Miles Davis",
            "the album's performer"
        );
        assert_eq!(s.entries[2].performer, "Miles Davis Sextet");
    }

    #[test]
    fn a_sheet_for_several_files_gives_only_this_ones_tracks() {
        let two = "FILE \"a.flac\" WAVE\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n TRACK 02 AUDIO\n  INDEX 01 01:00:00\nFILE \"b.flac\" WAVE\n TRACK 03 AUDIO\n  INDEX 01 00:00:00\n";
        assert_eq!(parse(two, "a.flac").map(|s| s.entries.len()), Some(2));
        assert_eq!(parse(two, "b.flac"), None, "one track is not a sheet");
        assert_eq!(parse(two, "c.flac"), None);
    }

    #[test]
    fn skipping_inside_the_file() {
        let s = parse(SHEET, "x.flac").unwrap();
        let t2 = s.entries[1].start_ms;
        let t3 = s.entries[2].start_ms;
        assert_eq!(s.index_at(t2 + 5), 1);
        assert_eq!(s.skip(1000, 1), Some(t2));
        assert_eq!(
            s.skip(t3 + 10, 1),
            None,
            "the last track: ▷ leaves the file"
        );
        assert_eq!(
            s.skip(t2 + 10_000, -1),
            Some(t2),
            "◁ late in a track restarts it"
        );
        assert_eq!(
            s.skip(t2 + 1000, -1),
            Some(0),
            "◁ early goes to the one before"
        );
        assert_eq!(s.skip(1000, -1), None, "the first track: ◁ acts as usual");
    }

    #[test]
    fn latin1_and_bad_lines_do_not_break_it() {
        let bytes = b"FILE \"x.flac\" WAVE\n TRACK 01 AUDIO\n  TITLE \"Caf\xe9\"\n  INDEX 01 00:00:00\n TRACK 02 AUDIO\n  INDEX 01 xx:yy\n TRACK 03 AUDIO\n  INDEX 01 02:00:00\n";
        let s = parse(&decode(bytes), "x.flac").unwrap();
        assert_eq!(s.entries[0].title, "Caf\u{e9}");
        assert_eq!(
            s.entries.len(),
            2,
            "a track with no readable start is dropped"
        );
        assert_eq!(s.entries[1].title, "Track 2");
    }

    #[test]
    fn found_beside_the_file() {
        let dir = std::env::temp_dir().join(format!("cinder-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("Kind of Blue.flac");
        std::fs::write(&audio, b"").unwrap();
        std::fs::write(
            dir.join("other.cue"),
            SHEET.replace("Kind of Blue.wav", "Kind of Blue.flac"),
        )
        .unwrap();
        let got = load_for(audio.to_str().unwrap());
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got.map(|s| s.entries.len()), Some(3));
    }
}
