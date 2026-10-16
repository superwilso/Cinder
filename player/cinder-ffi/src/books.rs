//! Audiobooks — which tracks are one, and where the listener got to in each book.
//!
//! **What is an audiobook.** A track in a `LEARNING` folder at the root of either volume
//! (`/contents/LEARNING/…`, `/contents_ext/LEARNING/…`). That is Sony's own language-study folder:
//! its scanner already indexes it and its music tabs already keep it out, so a transfer that works
//! for Sony's player works here. The match ignores case, because the folder lives on a FAT volume.
//!
//! **What is a book.** The folder that directly holds the audio files, so
//! `LEARNING/author_1/book_1/*.mp3` and `LEARNING/author_1/book_2/*.mp3` are two books by
//! `author_1`, each with its own place. The files in it are its chapters.
//!
//! **Where the place is kept.** `/contents/cinder_books.tsv`, one line per book that has been
//! started, keyed by the book's FOLDER PATH — not by Sony's object ids, which are re-issued by
//! every rescan (the reasoning `stats.rs` gives, and the same cure):
//!
//! ```text
//! #CINDER-BOOKS/1
//! # folder<TAB>chapter file<TAB>position_ms<TAB>finished
//! /contents/LEARNING/Le Guin/Earthsea/02.mp3<TAB>…
//! ```
//!
//! Written through a temporary file and a rename, so a pulled cable leaves the old file or the new
//! one. Positions are `i64` milliseconds end to end.

use cinder_ui::model::{BookRow, SongRow};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::stats::sig;

pub const PATH: &str = "/contents/cinder_books.tsv";
const HEADER: &str = "#CINDER-BOOKS/1\n# folder\tchapter\tposition_ms\tfinished\n";

/// The installer's Audiobooks option leaves this flag (`install_cinderhome.sh`), a file in
/// `/data/cinder` — machine-written state, off the volume a PC can edit. Without it nothing is set
/// apart: the files under `LEARNING` stay in the music lists and the Library has no book button.
pub const FLAG: &str = "/data/cinder/audiobooks_on";

/// Is the component on: does `flag` exist?
pub fn enabled_at(flag: impl AsRef<Path>) -> bool {
    flag.as_ref().exists()
}

/// [`enabled_at`] for the real flag. Read when the library is built, not later by the shell: the
/// option changes what the music lists CONTAIN, so it has to be known before they are made.
pub fn enabled() -> bool {
    enabled_at(FLAG)
}

/// The name of the folder that makes a track an audiobook.
pub const FOLDER: &str = "LEARNING";

/// The volume roots a book can live under.
const ROOTS: [&str; 4] = [
    "/contents/",
    "/contents_ext/",
    "/data/mnt/internal/",
    "/data/mnt/external/",
];

/// If `path` is a chapter of an audiobook, the folder of its book (no trailing slash).
///
/// `None` for everything else: music, a path that only has `LEARNING` further down, and the folder
/// itself.
pub fn book_dir(path: &str) -> Option<&str> {
    let rest = ROOTS.iter().find_map(|r| path.strip_prefix(r))?;
    let (first, below) = rest.split_once('/')?;
    if !first.eq_ignore_ascii_case(FOLDER) || below.is_empty() {
        return None;
    }
    // The file's parent: everything up to its last slash.
    let slash = path.rfind('/')?;
    Some(&path[..slash])
}

/// Is `path` a chapter of an audiobook?
pub fn is_audiobook(path: &str) -> bool {
    book_dir(path).is_some()
}

/// A book's title: the last component of its folder.
pub fn title_of(dir: &str) -> &str {
    dir.rsplit('/').next().unwrap_or(dir)
}

/// A book's author: the folder above it, when that is a folder inside `LEARNING` and not
/// `LEARNING` itself. Empty for a book that sits straight in `LEARNING`.
pub fn author_of(dir: &str) -> &str {
    let Some((parent, _)) = dir.rsplit_once('/') else {
        return "";
    };
    let name = title_of(parent);
    if name.eq_ignore_ascii_case(FOLDER) || !is_audiobook(&format!("{parent}/x")) {
        ""
    } else {
        name
    }
}

/// File-name order with digit runs compared as numbers, so `Chapter 2` comes before `Chapter 10`
/// and `02.mp3` before `10.mp3`. Case does not matter.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let num = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.next_if(|c| c.is_ascii_digit()) {
                        digits.push(c);
                    }
                    digits.trim_start_matches('0').to_string()
                };
                let (da, db) = (num(&mut a), num(&mut b));
                // A longer run without leading zeros is the bigger number.
                let ord = da.len().cmp(&db.len()).then_with(|| da.cmp(&db));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Group audiobook tracks into books: one per folder, chapters in file-name order, the books by
/// author then title. Tracks that are not audiobook chapters are ignored. The listener's place is
/// not set here — [`Store::apply`] does that.
pub fn rows(
    tracks: &[cinder_db::Track],
    song_row: impl Fn(&cinder_db::Track) -> SongRow,
) -> Vec<BookRow> {
    let mut by_dir: BTreeMap<&str, Vec<&cinder_db::Track>> = BTreeMap::new();
    for t in tracks {
        if let Some(dir) = book_dir(&t.filename) {
            by_dir.entry(dir).or_default().push(t);
        }
    }
    let mut books: Vec<BookRow> = by_dir
        .into_iter()
        .map(|(dir, mut ch)| {
            ch.sort_by(|a, b| natural_cmp(&a.filename, &b.filename));
            BookRow {
                title: title_of(dir).to_string(),
                author: author_of(dir).to_string(),
                dir: dir.to_string(),
                paths: ch.iter().map(|t| t.filename.clone()).collect(),
                chapters: ch.into_iter().map(&song_row).collect(),
                ..Default::default()
            }
        })
        .collect();
    books.sort_by(|a, b| {
        cinder_ui::collate::cmp(&a.author, &b.author)
            .then_with(|| cinder_ui::collate::cmp(&a.title, &b.title))
    });
    books
}

/// Where the listener is in one book, as the store keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// The chapter's file path.
    pub chapter: String,
    pub pos_ms: i64,
    /// Listened to the end.
    pub finished: bool,
}

/// The store: every book that has been started, by folder path.
#[derive(Debug, Default)]
pub struct Store {
    path: PathBuf,
    by_dir: BTreeMap<String, Progress>,
    dirty: bool,
    /// The books changed here since the last write — what a merge keeps from this side.
    touched: BTreeSet<String>,
    /// A fingerprint of the file as this store last read or wrote it (0 = there was no file).
    disk_sig: u64,
}

/// Parse the file's text. A line that does not read is skipped on its own; it never costs the
/// rest of the file.
pub fn parse(body: &str) -> BTreeMap<String, Progress> {
    let mut out = BTreeMap::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut f = line.split('\t');
        let (Some(dir), Some(chapter), Some(pos)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let Ok(pos_ms) = pos.trim().parse::<i64>() else {
            continue;
        };
        let finished = f.next().is_some_and(|v| v.trim() == "1");
        if !dir.is_empty() && !chapter.is_empty() {
            out.insert(
                dir.to_string(),
                Progress {
                    chapter: chapter.to_string(),
                    pos_ms: pos_ms.max(0),
                    finished,
                },
            );
        }
    }
    out
}

/// The file's text.
pub fn serialize(books: &BTreeMap<String, Progress>) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .collect()
    };
    let mut s = String::from(HEADER);
    for (dir, p) in books {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            clean(dir),
            clean(&p.chapter),
            p.pos_ms,
            u8::from(p.finished)
        ));
    }
    s
}

impl Store {
    /// Read the store at `path`. A missing file is an empty store.
    pub fn open(path: impl AsRef<Path>) -> Store {
        let path = path.as_ref().to_path_buf();
        let body = std::fs::read(&path).ok();
        Store {
            by_dir: body
                .as_ref()
                .map(|b| parse(&String::from_utf8_lossy(b)))
                .unwrap_or_default(),
            disk_sig: body.as_ref().map_or(0, |b| sig(b)),
            path,
            dirty: false,
            touched: Default::default(),
        }
    }

    pub fn get(&self, dir: &str) -> Option<&Progress> {
        self.by_dir.get(dir)
    }

    fn put(&mut self, dir: &str, p: Progress) {
        if dir.is_empty() || self.by_dir.get(dir) == Some(&p) {
            return;
        }
        self.by_dir.insert(dir.to_string(), p);
        self.touched.insert(dir.to_string());
        self.dirty = true;
    }

    /// The listener is `pos_ms` into `chapter` of the book in `dir`. Playing a finished book
    /// again is a new listen, so this clears `finished`.
    pub fn record(&mut self, dir: &str, chapter: &str, pos_ms: i64) {
        self.put(
            dir,
            Progress {
                chapter: chapter.to_string(),
                pos_ms: pos_ms.max(0),
                finished: false,
            },
        );
    }

    /// The book's last chapter has ended.
    pub fn finish(&mut self, dir: &str, last_chapter: &str) {
        self.put(
            dir,
            Progress {
                chapter: last_chapter.to_string(),
                pos_ms: 0,
                finished: true,
            },
        );
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Write the file if anything changed: a temporary file beside it, then a rename. On a failed
    /// write the store stays dirty and the next call tries again.
    ///
    /// If the file on disk is not the one this store last saw (the volume was unreadable when the
    /// store opened, or a PC tool rewrote it), the disk's books are merged first: for a book
    /// changed here, this side wins; every other book is the disk's.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Ok(body) = std::fs::read(&self.path) {
            if sig(&body) != self.disk_sig {
                let mut merged = parse(&String::from_utf8_lossy(&body));
                for dir in &self.touched {
                    if let Some(p) = self.by_dir.get(dir) {
                        merged.insert(dir.clone(), p.clone());
                    }
                }
                self.by_dir = merged;
            }
        }
        let body = serialize(&self.by_dir);
        crate::write_atomic(&self.path, &body)?;
        self.disk_sig = sig(body.as_bytes());
        self.touched.clear();
        self.dirty = false;
        Ok(())
    }

    /// Where to resume the book in `dir` whose chapters are `paths`: the chapter's index and the
    /// position in it. A book never started, one finished, and one whose saved chapter is no longer
    /// in it all resume at the start; so does a saved place the book can no longer honour.
    pub fn place(&self, dir: &str, paths: &[String]) -> (usize, i64, bool) {
        let Some(p) = self.get(dir) else {
            return (0, 0, false);
        };
        let Some(i) = paths.iter().position(|c| *c == p.chapter) else {
            return (0, 0, false);
        };
        if p.finished {
            (i, 0, true)
        } else {
            (i, p.pos_ms, false)
        }
    }

    /// Put each book's saved place on its row.
    pub fn apply(&self, books: &mut [BookRow]) {
        for b in books {
            (b.chapter, b.pos_ms, b.finished) = self.place(&b.dir, &b.paths);
        }
    }
}

/// Which of the `resolved` chapter files (the book's files that still exist, in order) to start at,
/// and how far into it, for a book whose saved place is in `places`.
///
/// A book never started starts at its first chapter; so does one finished, and one whose saved
/// chapter has gone from the card — in each case at 0 ms.
pub fn start_point(places: &Store, book: &BookRow, resolved: &[String]) -> (usize, i64) {
    let (chapter, pos, finished) = places.place(&book.dir, &book.paths);
    if finished {
        return (0, 0);
    }
    book.paths
        .get(chapter)
        .and_then(|want| resolved.iter().position(|f| f == want))
        .map_or((0, 0), |i| (i, pos))
}

/// Is a position about to be saved merely the start of a chapter that is still being sought to a
/// saved place? `held` is the file and place a play of the book began with, `age_s` how long ago.
/// For a few seconds after a book is started, playback sits at the top of the chapter until the
/// seek lands; saving that would replace the place being resumed with the chapter's first second.
pub fn holds_back(held: &str, want_ms: i64, age_s: u64, file: &str, pos_ms: i64) -> bool {
    held == file && age_s < 15 && pos_ms + 1000 < want_ms
}

/// How close to the end of a book's last chapter counts as having finished it. A listener who
/// stops in the closing credits has finished the book; asking them to reach the last second would
/// resume a finished book at its final ten seconds.
pub const FINISHED_WITHIN_MS: i64 = 10_000;

/// Has the listener finished the book: in its last chapter, within [`FINISHED_WITHIN_MS`] of the
/// end (or within a tenth of a chapter shorter than that)? `duration_ms` is the chapter's length
/// (0 when unknown, which never finishes a book).
pub fn is_finished(chapter_idx: usize, chapters: usize, pos_ms: i64, duration_ms: i64) -> bool {
    chapters > 0
        && chapter_idx + 1 == chapters
        && duration_ms > 0
        && pos_ms >= duration_ms - FINISHED_WITHIN_MS.min(duration_ms / 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_in_learning_belongs_to_the_folder_that_holds_it() {
        assert_eq!(
            book_dir("/contents/LEARNING/author_1/book1/01.mp3"),
            Some("/contents/LEARNING/author_1/book1")
        );
        assert_eq!(
            book_dir("/contents/LEARNING/author_1/book_2/01.mp3"),
            Some("/contents/LEARNING/author_1/book_2"),
            "two books by one author are two books"
        );
        assert_eq!(
            book_dir("/contents/LEARNING/loose.mp3"),
            Some("/contents/LEARNING"),
            "a file straight in LEARNING is a book called LEARNING"
        );
    }

    #[test]
    fn both_volumes_count_and_case_does_not_matter() {
        assert!(is_audiobook("/contents_ext/LEARNING/a/b/01.m4b"));
        assert!(is_audiobook("/contents/Learning/a/b/01.m4b"));
        assert!(is_audiobook("/data/mnt/external/learning/a/01.m4b"));
    }

    #[test]
    fn music_and_look_alikes_are_not_audiobooks() {
        assert!(!is_audiobook("/contents/MUSIC/Artist/Album/01.flac"));
        assert!(
            !is_audiobook("/contents/MUSIC/LEARNING/01.flac"),
            "only at the root"
        );
        assert!(!is_audiobook("/contents/LEARNINGS/01.mp3"));
        assert!(!is_audiobook("/contents/LEARNING"), "the folder itself");
        assert!(!is_audiobook("/contents/LEARNING/"));
        assert!(!is_audiobook("/elsewhere/LEARNING/a/01.mp3"));
        assert!(!is_audiobook(""));
    }

    #[test]
    fn a_book_has_a_title_and_an_author_from_its_folders() {
        let d = "/contents/LEARNING/author_1/book1";
        assert_eq!(title_of(d), "book1");
        assert_eq!(author_of(d), "author_1");
        assert_eq!(title_of("/contents/LEARNING"), "LEARNING");
        assert_eq!(
            author_of("/contents/LEARNING/book1"),
            "",
            "no author folder"
        );
        assert_eq!(author_of("/contents/LEARNING"), "");
    }

    #[test]
    fn chapters_sort_by_number_not_by_character() {
        let mut v = vec![
            "Chapter 10.mp3",
            "Chapter 2.mp3",
            "chapter 1.mp3",
            "Chapter 02b.mp3",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            [
                "chapter 1.mp3",
                "Chapter 2.mp3",
                "Chapter 02b.mp3",
                "Chapter 10.mp3"
            ]
        );
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
        assert_eq!(
            natural_cmp("01", "1"),
            Ordering::Equal,
            "leading zeros are not a difference"
        );
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cinder_books_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("cinder_books.tsv")
    }

    const A: &str = "/contents/LEARNING/Author One/Book A";
    const B: &str = "/contents/LEARNING/Author One/Book B";

    #[test]
    fn each_book_keeps_its_own_place_across_a_reopen() {
        let p = tmp("reopen");
        let mut s = Store::open(&p);
        assert!(s.get(A).is_none(), "no file is an empty store");
        s.record(A, &format!("{A}/03.mp3"), 754_000);
        s.record(B, &format!("{B}/01.mp3"), 5_000_000_000_000); // past 2038 in ms: i64
        s.flush().unwrap();
        assert!(!s.is_dirty());
        assert!(
            !p.with_extension("tsv.tmp").exists(),
            "the rename is the write"
        );

        let r = Store::open(&p);
        assert_eq!(r.get(A).unwrap().chapter, format!("{A}/03.mp3"));
        assert_eq!(r.get(A).unwrap().pos_ms, 754_000);
        assert_eq!(r.get(B).unwrap().pos_ms, 5_000_000_000_000);
        assert!(r.get("/contents/LEARNING/Other/Book").is_none());
    }

    #[test]
    fn saying_the_same_place_again_is_not_a_write() {
        let p = tmp("same");
        let mut s = Store::open(&p);
        s.record(A, &format!("{A}/01.mp3"), 1000);
        s.flush().unwrap();
        s.record(A, &format!("{A}/01.mp3"), 1000);
        assert!(!s.is_dirty());
        s.record(A, &format!("{A}/01.mp3"), 2000);
        assert!(s.is_dirty());
    }

    #[test]
    fn finishing_then_playing_again_starts_a_new_listen() {
        let p = tmp("finish");
        let mut s = Store::open(&p);
        s.finish(A, &format!("{A}/09.mp3"));
        assert!(s.get(A).unwrap().finished);
        s.record(A, &format!("{A}/01.mp3"), 3000);
        assert!(!s.get(A).unwrap().finished);
    }

    #[test]
    fn a_damaged_line_costs_that_book_and_nothing_else() {
        let body = format!(
            "#CINDER-BOOKS/1\n{A}\t{A}/02.mp3\t1500\t0\nnot a line\n{B}\t{B}/01.mp3\tabc\t0\n\
             {B}\t{B}/01.mp3\t-5\t1\n"
        );
        let m = parse(&body);
        assert_eq!(m.len(), 2);
        assert_eq!(m[A].pos_ms, 1500);
        assert_eq!(m[B].pos_ms, 0, "a negative position reads as the start");
        assert!(m[B].finished);
    }

    fn book(dir: &str, files: &[&str]) -> BookRow {
        BookRow {
            dir: dir.to_string(),
            paths: files.iter().map(|f| format!("{dir}/{f}")).collect(),
            chapters: files.iter().map(|_| SongRow::default()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_saved_place_goes_back_onto_the_book_it_belongs_to() {
        let p = tmp("apply");
        let mut s = Store::open(&p);
        s.record(A, &format!("{A}/02.mp3"), 90_000);
        s.finish(B, &format!("{B}/02.mp3"));
        let mut books = vec![
            book(A, &["01.mp3", "02.mp3"]),
            book(B, &["01.mp3", "02.mp3"]),
        ];
        s.apply(&mut books);
        assert_eq!(
            (books[0].chapter, books[0].pos_ms, books[0].finished),
            (1, 90_000, false)
        );
        assert!(books[1].finished, "finished survives");
        assert_eq!((books[1].chapter, books[1].pos_ms), (1, 0));
    }

    #[test]
    fn a_chapter_that_is_gone_falls_back_to_the_start() {
        let p = tmp("gone");
        let mut s = Store::open(&p);
        s.record(A, &format!("{A}/07.mp3"), 90_000);
        let mut books = vec![book(A, &["01.mp3", "02.mp3"])];
        books[0].chapter = 1; // stale values must be reset, not kept
        s.apply(&mut books);
        assert_eq!((books[0].chapter, books[0].pos_ms), (0, 0));
    }

    #[test]
    fn a_pc_edit_in_the_meantime_is_merged_not_overwritten() {
        let p = tmp("merge");
        let mut s = Store::open(&p); // opened before the real file was readable
        std::fs::write(
            &p,
            format!("{HEADER}{B}\t{B}/04.mp3\t42000\t0\n{A}\t{A}/01.mp3\t1000\t0\n"),
        )
        .unwrap();
        s.record(A, &format!("{A}/05.mp3"), 7000);
        s.flush().unwrap();
        let r = Store::open(&p);
        assert_eq!(
            r.get(B).unwrap().pos_ms,
            42_000,
            "the other book is the disk's"
        );
        assert_eq!(
            r.get(A).unwrap().chapter,
            format!("{A}/05.mp3"),
            "this side wins for its own"
        );
    }

    #[test]
    fn a_finished_book_resumes_at_the_start_of_its_last_chapter_row_but_reads_finished() {
        let mut s = Store::open(tmp("place"));
        let paths: Vec<String> = ["01.mp3", "02.mp3"]
            .iter()
            .map(|f| format!("{A}/{f}"))
            .collect();
        assert_eq!(s.place(A, &paths), (0, 0, false), "never started");
        s.record(A, &paths[1], 33_000);
        assert_eq!(s.place(A, &paths), (1, 33_000, false));
        s.finish(A, &paths[1]);
        assert_eq!(s.place(A, &paths), (1, 0, true));
    }

    #[test]
    fn the_last_ten_seconds_of_the_last_chapter_finish_the_book() {
        assert!(is_finished(2, 3, 590_000, 600_000));
        assert!(is_finished(2, 3, 600_000, 600_000));
        assert!(!is_finished(2, 3, 589_999, 600_000), "not yet");
        assert!(!is_finished(1, 3, 600_000, 600_000), "not the last chapter");
        assert!(
            !is_finished(0, 1, 5_000, 0),
            "an unknown length never finishes a book"
        );
        assert!(!is_finished(0, 0, 0, 0));
        assert!(
            !is_finished(0, 1, 3_000, 8_000),
            "a very short chapter is not over when it starts"
        );
        assert!(is_finished(0, 1, 7_300, 8_000));
    }

    #[test]
    fn a_book_starts_where_it_was_left_unless_that_is_gone_or_over() {
        let p = tmp("start");
        let mut s = Store::open(&p);
        let b = book(A, &["01.mp3", "02.mp3", "03.mp3"]);
        let all: Vec<String> = b.paths.clone();
        assert_eq!(start_point(&s, &b, &all), (0, 0), "never started");
        s.record(A, &b.paths[1], 90_000);
        assert_eq!(start_point(&s, &b, &all), (1, 90_000));
        // chapter 1's file is gone from the card: indices shift, the saved chapter is still found
        let without_first: Vec<String> = all[1..].to_vec();
        assert_eq!(start_point(&s, &b, &without_first), (0, 90_000));
        // the saved chapter itself is gone: the start of what is left
        let without_saved: Vec<String> = vec![all[0].clone(), all[2].clone()];
        assert_eq!(start_point(&s, &b, &without_saved), (0, 0));
        s.finish(A, &b.paths[2]);
        assert_eq!(
            start_point(&s, &b, &all),
            (0, 0),
            "a finished book starts again"
        );
    }

    #[test]
    fn the_top_of_a_chapter_does_not_overwrite_the_place_being_resumed() {
        let f = format!("{A}/02.mp3");
        assert!(holds_back(&f, 90_000, 2, &f, 0), "seek has not landed yet");
        assert!(!holds_back(&f, 90_000, 2, &f, 89_500), "it has");
        assert!(
            !holds_back(&f, 90_000, 2, &f, 120_000),
            "the listener is past it"
        );
        assert!(
            !holds_back(&f, 90_000, 20, &f, 0),
            "after 15 s the hold lapses"
        );
        assert!(
            !holds_back(&f, 90_000, 2, &format!("{A}/03.mp3"), 0),
            "another chapter"
        );
    }

    #[test]
    fn the_component_is_on_exactly_when_the_installer_left_its_flag() {
        let p = tmp("flag");
        assert!(!enabled_at(&p), "no flag, no audiobooks");
        std::fs::write(&p, "1\n").unwrap();
        assert!(enabled_at(&p));
        std::fs::remove_file(&p).unwrap();
        assert!(
            !enabled_at(&p),
            "an Update that turns it off removes the flag"
        );
    }
}
