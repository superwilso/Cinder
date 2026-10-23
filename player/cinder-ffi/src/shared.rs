//! Songs Flint stored once for two albums (Flint's `shared.rs`, owner idea 2026-10-23).
//!
//! A recording on an album and again on a best-of is copied to the player once. Flint writes the
//! other album's entry into `flint-shared.tsv` in the folder it syncs into (`MUSIC` on either
//! drive), one line per entry:
//!
//!   also \t keep \t title \t artist \t album \t album_artist \t track \t year
//!
//! `also` and `keep` are relative to that folder; `keep` is the file that is there. Each entry
//! joins the library as a copy of the kept file's row with the entry's own tags, so the song is
//! listed under both albums and either row plays the one file. Sony's player does not read the
//! list and shows the kept copy only.
//!
//! The copy shares the kept file's object id: plays, likes and ratings count to that file, and Now
//! Playing shows the kept album's tags, whichever row started it.

use std::collections::{HashMap, HashSet};

pub const LIST_NAME: &str = "flint-shared.tsv";

/// Where Flint syncs into, on each drive: `MUSIC` when the drive has one, else the drive itself.
const DIRS: [&str; 6] = [
    "/contents/MUSIC",
    "/contents/Music",
    "/contents",
    "/contents_ext/MUSIC",
    "/contents_ext/Music",
    "/contents_ext",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub keep: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track: i64,
}

/// A list's entries. The header and any line that is not eight fields are skipped.
pub fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let [_also, keep, title, artist, album, album_artist, track, _year] = f[..] else {
                return None;
            };
            // "3/12" or "3": the number before the slash.
            let track = track
                .split('/')
                .next()
                .unwrap_or("")
                .trim()
                .parse()
                .unwrap_or(0);
            Some(Entry {
                keep: keep.into(),
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                album_artist: album_artist.into(),
                track,
            })
        })
        .collect()
}

/// Every list on the player, with the folder it was found in.
pub fn read_all() -> Vec<(String, Vec<Entry>)> {
    DIRS.iter()
        .filter_map(|d| {
            let text = std::fs::read_to_string(format!("{d}/{LIST_NAME}")).ok()?;
            Some((d.to_string(), parse(&text)))
        })
        .collect()
}

/// The path a track is known by, with Sony's second spelling of each mount folded onto the first.
fn canonical(path: &str) -> String {
    for (alias, root) in [
        ("/data/mnt/internal/", "/contents/"),
        ("/data/mnt/external/", "/contents_ext/"),
    ] {
        if let Some(rest) = path.strip_prefix(alias) {
            return format!("{root}{rest}");
        }
    }
    path.to_string()
}

/// An album id for an album with no track of its own on the player: stable for its name and
/// artist, negative, and clear of the ids Sony's store hands out and of `NO_ALBUM_ID`.
fn made_album_id(album: &str, artist: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in album
        .to_lowercase()
        .bytes()
        .chain([0])
        .chain(artist.to_lowercase().bytes())
    {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    -((h >> 24) as i64) - 1
}

/// The tracks the lists add: each entry as a copy of its kept file's track, with the entry's tags.
/// An entry whose kept file is not in `tracks` (not scanned yet, or removed) adds nothing.
pub fn tracks(
    lists: &[(String, Vec<Entry>)],
    tracks: &[cinder_db::Track],
) -> Vec<cinder_db::Track> {
    if lists.is_empty() {
        return Vec::new();
    }
    let by_path: HashMap<String, &cinder_db::Track> =
        tracks.iter().map(|t| (canonical(&t.filename), t)).collect();
    // An album already on the player keeps its id, so the entry joins it and its cover.
    let group = |album_artist: &str, artist: &str| {
        if album_artist.trim().is_empty() {
            artist
        } else {
            album_artist
        }
        .to_lowercase()
    };
    let albums: HashMap<(String, String), i64> = tracks
        .iter()
        .filter_map(|t| {
            Some((
                (t.album.to_lowercase(), group(&t.album_artist, &t.artist)),
                t.album_id?,
            ))
        })
        .collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (dir, entries) in lists {
        for e in entries {
            let Some(kept) = by_path.get(&format!("{dir}/{}", e.keep)) else {
                continue;
            };
            let artist = if e.artist.is_empty() {
                kept.artist.clone()
            } else {
                e.artist.clone()
            };
            let key = (e.album.to_lowercase(), group(&e.album_artist, &artist));
            if !seen.insert((kept.object_id, key.clone())) {
                continue;
            }
            let album_id = albums
                .get(&key)
                .copied()
                .unwrap_or_else(|| made_album_id(&key.0, &key.1));
            out.push(cinder_db::Track {
                title: if e.title.is_empty() {
                    kept.title.clone()
                } else {
                    e.title.clone()
                },
                artist,
                album: e.album.clone(),
                album_artist: e.album_artist.clone(),
                track_no: e.track,
                disc_no: 0,
                album_id: Some(album_id),
                // The year is the kept album's on Sony's store; the entry's own is not resolvable
                // to its id, so none rather than the wrong one.
                releaseyear_id: None,
                ..(*kept).clone()
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, path: &str, album: &str, album_id: i64) -> cinder_db::Track {
        cinder_db::Track {
            object_id: id,
            title: "Atlas".into(),
            artist: "Bicep".into(),
            album: album.into(),
            filename: path.into(),
            album_artist: String::new(),
            disc_no: 1,
            track_no: 1,
            duration_raw: Some(300_000),
            is_hires: false,
            othumb_id: None,
            album_id: Some(album_id),
            added: 0,
            releaseyear_id: Some(9),
            genre_id: None,
        }
    }

    #[test]
    fn an_entry_lists_the_kept_file_under_its_own_album() {
        let list = "#FLINT-SHARED/1\talso\tkeep\ttitle\tartist\talbum\talbum_artist\ttrack\tyear\n\
                    Best Of/04 Atlas.flac\tIsles/01 Atlas.flac\tAtlas\tBicep\tBest Of\t\t4/12\t2022\n\
                    Gone/01 x.flac\tGone/01 x.flac\tX\tY\tZ\t\t1\t\n\
                    short line\n";
        let entries = parse(list);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].track, 4);
        let lib = [
            track(
                7,
                "/data/mnt/internal/MUSIC/Isles/01 Atlas.flac",
                "Isles",
                3,
            ),
            track(8, "/contents/MUSIC/Other/01.flac", "Other", 4),
        ];
        let got = tracks(&[("/contents/MUSIC".into(), entries)], &lib);
        assert_eq!(
            got.len(),
            1,
            "an entry whose file is not there adds nothing"
        );
        let t = &got[0];
        assert_eq!(
            (t.object_id, t.filename.as_str()),
            (7, lib[0].filename.as_str()),
            "plays the kept file"
        );
        assert_eq!(
            (t.album.as_str(), t.track_no, t.releaseyear_id),
            ("Best Of", 4, None)
        );
        assert!(
            t.album_id.unwrap() < 0 && t.album_id != Some(i64::MIN),
            "an album of its own"
        );
    }

    /// The list as Flint writes it (`contracts/flint-shared.tsv`; Flint's `shared::list_body` is
    /// held to the same file): the header skipped, `11/18` read as track 11, an empty album artist.
    #[test]
    fn flints_list_reads_as_written() {
        let e = parse(include_str!("../../../contracts/flint-shared.tsv"));
        let row =
            |keep: &str, title: &str, artist: &str, album: &str, album_artist: &str, track| Entry {
                keep: keep.into(),
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                album_artist: album_artist.into(),
                track,
            };
        assert_eq!(
            e,
            [
                row(
                    "Isles/01 Atlas.flac",
                    "Atlas",
                    "Bicep",
                    "Best Of",
                    "Various Artists",
                    11
                ),
                row(
                    "Takk.../04 Hoppipolla.flac",
                    "Hoppípolla",
                    "Sigur Rós",
                    "Takk (Live)",
                    "",
                    4
                ),
            ]
        );
    }

    #[test]
    fn an_entry_joins_its_album_when_the_album_is_there() {
        let lib = [
            track(7, "/contents/MUSIC/Isles/01 Atlas.flac", "Isles", 3),
            track(8, "/contents/MUSIC/Best Of/01 Glue.flac", "Best Of", 5),
        ];
        let entries = parse("a\tIsles/01 Atlas.flac\tAtlas\tBicep\tBest Of\t\t2\t\n");
        let got = tracks(
            &[
                ("/contents/MUSIC".into(), entries.clone()),
                ("/contents/MUSIC".into(), entries),
            ],
            &lib,
        );
        assert_eq!(got.len(), 1, "listed once however many lists name it");
        assert_eq!(got[0].album_id, Some(5));
    }
}
