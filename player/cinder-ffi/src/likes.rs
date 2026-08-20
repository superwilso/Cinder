//! Liked-songs **import** — the PC half of the likes sync landing on the device.
//!
//! Cinder already exports `/contents/cinder_loved.tsv` (`artist \t title`) whenever the liked
//! list changes, because the object ids in `cinder_liked.conf` mean nothing off-device. This is
//! the return path: a PC tool (`likesync`) writes `/contents/cinder_liked_import.tsv` in the same
//! two-column format, and on the next database open Cinder resolves those rows against its own
//! library and makes them the liked set.
//!
//! **The import is the whole list, not a delta.** It is produced by merging the device's own
//! export with Last.fm and MusicBee, so it already contains everything liked on the device; a
//! track missing from it was deliberately unliked somewhere. Merging instead of replacing would
//! make an unlike impossible to express and the device would slowly accumulate every like it had
//! ever seen.
//!
//! Two refusals guard the replace, because the failure mode is losing a hand-curated list:
//!
//! * a file without the `# artist\ttitle` header is not one of ours — ignored, left in place;
//! * a file with rows that resolve to **nothing** is either a library that has not finished
//!   loading or a wildly mismatched pair of tag sets — ignored, and left in place so the next
//!   boot can try again.
//!
//! An empty list *with* the header is honoured: that is "everything was unliked", and the file
//! is written atomically on the PC side so a zero-row file cannot be a torn write.
//!
//! Matching is by artist + title, normalised the same way the PC side normalises: the two sides
//! must agree or nothing lines up, so the rules here mirror `likesync/keys.py` deliberately —
//! case, curly punctuation, `feat.` credits and re-issue suffixes are folded away, and anything
//! that marks a *different recording* (live, remix, acoustic, demo) is left alone.

use std::collections::{BTreeMap, BTreeSet};

/// Name of the file the PC writes, beside `cinder_liked.conf`.
pub const IMPORT_NAME: &str = "cinder_liked_import.tsv";
/// What it is renamed to once consumed, so the PC can tell it landed.
pub const DONE_SUFFIX: &str = ".done";
const HEADER_PREFIX: &str = "# artist";

/// Re-issue markers: the same recording under another release. Deliberately excludes live,
/// remix, acoustic, demo and instrumental — those are different recordings.
const NOISE: &[&str] = &[
    "remaster",
    "remastered",
    "explicit",
    "explicit version",
    "clean",
    "clean version",
    "album version",
    "single version",
    "original version",
    "bonus track",
    "deluxe",
    "deluxe edition",
    "expanded",
    "expanded edition",
    "anniversary edition",
    "mono",
    "stereo",
];

const FEAT_MARKERS: &[&str] = &[
    "(feat.", "(feat ", "[feat.", "[feat ", " feat. ", " feat ", " ft. ", " ft ", " featuring ",
    "(ft.", "(ft ", "(featuring", "[featuring",
];

const ARTIST_SPLITS: &[&str] = &[" & ", ", ", "; ", " and ", " vs. ", " vs ", " x ", " / ", "/"];

fn fold_punctuation(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            '\u{00A0}' => ' ',
            other => other,
        })
        .collect()
}

fn collapse(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn base(input: &str) -> String {
    collapse(&fold_punctuation(input).to_lowercase())
}

/// Strip a trailing `feat. …` credit, in any of the spellings tag editors use.
fn strip_feat(input: &str) -> String {
    let lower = input;
    let mut cut = None;
    for marker in FEAT_MARKERS {
        if let Some(position) = lower.rfind(marker) {
            cut = Some(match cut {
                Some(existing) if existing < position => existing,
                _ => position,
            });
        }
    }
    match cut {
        Some(position) => input[..position].trim_end_matches([' ', '(', '[', '-']).trim().to_string(),
        None => input.to_string(),
    }
}

/// Is `tail` a re-issue marker rather than part of the title?
fn is_noise(tail: &str) -> bool {
    let tail = tail.trim().trim_matches(['(', ')', '[', ']']).trim();
    if tail.is_empty() {
        return false;
    }
    if NOISE.contains(&tail) {
        return true;
    }
    // "2009 remaster", "remastered 2011", "2021 mix", "2021 stereo mix" — year-anchored only, so
    // a bare "club mix" (a different recording) is never stripped.
    let words: Vec<&str> = tail.split(' ').collect();
    let has_year = words.iter().any(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()));
    if !has_year {
        return false;
    }
    words.iter().any(|w| {
        let w = *w;
        w.starts_with("remaster") || w == "mix" || w == "mixes"
    })
}

fn strip_noise(input: &str) -> String {
    let trimmed = input.trim_end();
    // "( … )" or "[ … ]" at the end
    if trimmed.ends_with(')') || trimmed.ends_with(']') {
        let open = if trimmed.ends_with(')') { '(' } else { '[' };
        if let Some(position) = trimmed.rfind(open) {
            if is_noise(&trimmed[position..]) {
                return trimmed[..position].trim().to_string();
            }
        }
    }
    // " - … " at the end
    if let Some(position) = trimmed.rfind(" - ") {
        if is_noise(&trimmed[position + 3..]) {
            return trimmed[..position].trim().to_string();
        }
    }
    trimmed.to_string()
}

pub fn norm_title(value: &str) -> String {
    let mut text = base(value);
    for _ in 0..3 {
        let stripped = strip_noise(&strip_feat(&text));
        if stripped == text || stripped.is_empty() {
            break;
        }
        text = stripped;
    }
    text
}

pub fn norm_artist(value: &str) -> String {
    strip_feat(&base(value)).trim().to_string()
}

/// First credited artist — the fallback when a collaboration is spelled differently on each side.
pub fn primary_artist(value: &str) -> String {
    let normalised = norm_artist(value);
    let mut cut = normalised.len();
    for separator in ARTIST_SPLITS {
        if let Some(position) = normalised.find(separator) {
            if position < cut {
                cut = position;
            }
        }
    }
    normalised[..cut].trim().to_string()
}

/// The join key. `\u{241F}` (unit separator) cannot occur in a tag.
pub fn key(artist: &str, title: &str) -> String {
    format!("{}\u{241F}{}", norm_artist(artist), norm_title(title))
}

fn loose_key(artist: &str, title: &str) -> String {
    format!("{}\u{241F}{}", primary_artist(artist), norm_title(title))
}

/// A parsed import file.
pub struct Import {
    pub tracks: Vec<(String, String)>,
    /// True when the file carries our header — the proof it was written by the PC tool and not
    /// left behind by something else.
    pub had_header: bool,
}

pub fn parse_import(body: &str) -> Import {
    let mut tracks = Vec::new();
    let mut had_header = false;
    for line in body.lines() {
        if line.starts_with('#') {
            if line.starts_with(HEADER_PREFIX) {
                had_header = true;
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.trim_end_matches(['\r', '\n']).split('\t');
        let (Some(artist), Some(title)) = (fields.next(), fields.next()) else { continue };
        let (artist, title) = (artist.trim(), title.trim());
        if !artist.is_empty() && !title.is_empty() {
            tracks.push((artist.to_string(), title.to_string()));
        }
    }
    Import { tracks, had_header }
}

/// Resolve import rows against the library. Returns the object ids and how many rows matched
/// nothing (a track the device does not hold — normal, and reported, not an error).
///
/// Each song is `(object_id, artist, title, album_artist)`. The album artist is the fourth
/// column because a featured-artist track is tagged to the *guest* — the device holds
/// "Cleo Sol — Woman" on a Little Simz album, and Last.fm calls the same track
/// "Little Simz feat. Cleo Sol". Indexing the album artist as well is what joins the two, and it
/// mirrors what the PC side does with the album folder name. Pass "" when there is none.
pub fn resolve<'a, I>(songs: I, import: &Import) -> (BTreeSet<i64>, usize)
where
    I: Iterator<Item = (i64, &'a str, &'a str, &'a str)>,
{
    let mut exact: BTreeMap<String, i64> = BTreeMap::new();
    let mut loose: BTreeMap<String, i64> = BTreeMap::new();
    for (id, artist, title, album_artist) in songs {
        // First writer wins, so a duplicate track resolves to the same id run after run.
        exact.entry(key(artist, title)).or_insert(id);
        loose.entry(loose_key(artist, title)).or_insert(id);
        if !album_artist.is_empty() {
            loose.entry(key(album_artist, title)).or_insert(id);
            loose.entry(loose_key(album_artist, title)).or_insert(id);
        }
    }

    let mut ids = BTreeSet::new();
    let mut missing = 0usize;
    for (artist, title) in &import.tracks {
        let candidates = [
            exact.get(&key(artist, title)),
            exact.get(&loose_key(artist, title)),
            loose.get(&key(artist, title)),
            loose.get(&loose_key(artist, title)),
        ];
        match candidates.into_iter().flatten().next() {
            Some(id) => {
                ids.insert(*id);
            }
            None => missing += 1,
        }
    }
    (ids, missing)
}

/// Outcome of one import attempt, for the log line and for the tests.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// No import file present — the normal case on almost every boot.
    None,
    /// Present but not ours (no header) or unreadable: left alone.
    Ignored(&'static str),
    /// Rows were present and none of them resolved: left alone so the next boot can retry.
    Unresolved(usize),
    /// Applied. `(liked, missing)` — ids now liked, and rows the library does not hold.
    Applied(usize, usize),
}

/// Full-file path of the import that sits beside `liked_path`.
pub fn import_path(liked_path: &str) -> String {
    match liked_path.rfind('/') {
        Some(position) => format!("{}/{}", &liked_path[..position], IMPORT_NAME),
        None => IMPORT_NAME.to_string(),
    }
}

/// Read, resolve and consume the import file. Returns the new liked set on success.
///
/// Consuming means a rename to `…​.tsv.done`, not a delete: the PC uses the file's absence as
/// the signal that the push landed, and keeping the content makes a failed sync inspectable over
/// USB. A rename is also atomic, so a power cut cannot apply the import twice.
pub fn apply_import<'a, I>(liked_path: &str, songs: I) -> (Outcome, Option<BTreeSet<i64>>)
where
    I: Iterator<Item = (i64, &'a str, &'a str, &'a str)>,
{
    let path = import_path(liked_path);
    let Ok(body) = std::fs::read_to_string(&path) else {
        return (Outcome::None, None);
    };
    let import = parse_import(&body);
    if !import.had_header {
        return (Outcome::Ignored("no likesync header"), None);
    }
    let (ids, missing) = resolve(songs, &import);
    if ids.is_empty() && !import.tracks.is_empty() {
        return (Outcome::Unresolved(import.tracks.len()), None);
    }
    let _ = std::fs::rename(&path, format!("{path}{DONE_SUFFIX}"));
    (Outcome::Applied(ids.len(), missing), Some(ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Vec<(i64, String, String, String)> {
        vec![
            (1, "The Beatles".into(), "Don't Let Me Down (2021 Mix)".into(), "The Beatles".into()),
            // Tagged to the guest artist, filed on the host artist's album — the real shape of
            // "02 - Cleo Sol - Woman.flac" inside "Little Simz - Sometimes I Might Be Introvert".
            (2, "Cleo Sol".into(), "Woman".into(), "Little Simz".into()),
            (3, "Bob Marley".into(), "No Woman No Cry".into(), "Bob Marley".into()),
            (4, "Bob Marley".into(), "No Woman No Cry (Live)".into(), "Bob Marley".into()),
            (5, "America, George Martin".into(), "Ventura Highway".into(), "America".into()),
        ]
    }

    fn resolve_rows(rows: &[(&str, &str)]) -> (BTreeSet<i64>, usize) {
        let owned = library();
        let import = Import {
            tracks: rows.iter().map(|(a, t)| (a.to_string(), t.to_string())).collect(),
            had_header: true,
        };
        resolve(
            owned.iter().map(|(id, a, t, aa)| (*id, a.as_str(), t.as_str(), aa.as_str())),
            &import,
        )
    }

    #[test]
    fn reissue_suffixes_fold_together() {
        for variant in [
            "Don't Let Me Down",
            "Don't Let Me Down - Remastered 2009",
            "Don\u{2019}t Let Me Down (Remastered)",
            "DON'T LET ME DOWN (2021 Mix)",
        ] {
            let (ids, missing) = resolve_rows(&[("The Beatles", variant)]);
            assert_eq!(missing, 0, "{variant}");
            assert!(ids.contains(&1), "{variant}");
        }
    }

    #[test]
    fn a_different_recording_stays_different() {
        // The live take must not be pulled in by liking the studio one.
        let (ids, _) = resolve_rows(&[("Bob Marley", "No Woman No Cry")]);
        assert!(ids.contains(&3) && !ids.contains(&4));
        let (ids, _) = resolve_rows(&[("Bob Marley", "No Woman No Cry (Live)")]);
        assert!(ids.contains(&4) && !ids.contains(&3));
    }

    #[test]
    fn featured_artist_credit_matches_the_album_tag() {
        let (ids, missing) = resolve_rows(&[("Little Simz feat. Cleo Sol", "Woman (feat. Cleo Sol)")]);
        assert_eq!(missing, 0);
        assert!(ids.contains(&2));
    }

    #[test]
    fn primary_artist_matches_a_longer_credit() {
        let (ids, missing) = resolve_rows(&[("America", "Ventura Highway")]);
        assert_eq!(missing, 0);
        assert!(ids.contains(&5));
    }

    #[test]
    fn unknown_rows_are_counted_not_fatal() {
        let (ids, missing) = resolve_rows(&[("Nobody", "Nothing"), ("Bob Marley", "No Woman No Cry")]);
        assert_eq!(missing, 1);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn parse_skips_junk_and_finds_the_header() {
        let import = parse_import("# artist\ttitle — pushed from the PC\n\nA\tB\nbroken\nC\tD\tE\n");
        assert!(import.had_header);
        assert_eq!(import.tracks, vec![("A".into(), "B".into()), ("C".into(), "D".into())]);
    }

    #[test]
    fn a_file_without_our_header_is_ignored() {
        let import = parse_import("A\tB\n");
        assert!(!import.had_header);
    }

    #[test]
    fn import_path_sits_beside_the_liked_list() {
        assert_eq!(import_path("/contents/cinder_liked.conf"),
                   "/contents/cinder_liked_import.tsv");
    }

    #[test]
    fn apply_import_consumes_the_file() {
        let dir = std::env::temp_dir().join(format!("cinder_likes_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let liked = dir.join("cinder_liked.conf");
        let import = dir.join(IMPORT_NAME);
        std::fs::write(&import, "# artist\ttitle\nBob Marley\tNo Woman No Cry\n").unwrap();

        let owned = library();
        let (outcome, ids) = apply_import(
            liked.to_str().unwrap(),
            owned.iter().map(|(id, a, t, aa)| (*id, a.as_str(), t.as_str(), aa.as_str())),
        );
        assert_eq!(outcome, Outcome::Applied(1, 0));
        assert_eq!(ids.unwrap().into_iter().collect::<Vec<_>>(), vec![3]);
        assert!(!import.exists());
        assert!(dir.join(format!("{IMPORT_NAME}{DONE_SUFFIX}")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_but_headed_file_clears_the_list() {
        let dir = std::env::temp_dir().join(format!("cinder_likes_empty_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let liked = dir.join("cinder_liked.conf");
        std::fs::write(dir.join(IMPORT_NAME), "# artist\ttitle\n").unwrap();

        let owned = library();
        let (outcome, ids) = apply_import(
            liked.to_str().unwrap(),
            owned.iter().map(|(id, a, t, aa)| (*id, a.as_str(), t.as_str(), aa.as_str())),
        );
        assert_eq!(outcome, Outcome::Applied(0, 0));
        assert!(ids.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rows_that_resolve_to_nothing_leave_the_file_alone() {
        let dir = std::env::temp_dir().join(format!("cinder_likes_unres_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let liked = dir.join("cinder_liked.conf");
        let import = dir.join(IMPORT_NAME);
        std::fs::write(&import, "# artist\ttitle\nSomeone\tElse\n").unwrap();

        let (outcome, ids) = apply_import(liked.to_str().unwrap(), std::iter::empty());
        assert_eq!(outcome, Outcome::Unresolved(1));
        assert!(ids.is_none());
        assert!(import.exists(), "an unresolved import must survive for the next boot");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
