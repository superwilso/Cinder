//! User playlists — the ones made ON the device.
//!
//! Sony's playlists are containers in the MediaStore database (`Db::playlists`), and Cinder reads
//! them. It does not write them, for three reasons: the database is owned by services that hold it
//! open, it is rebuilt from scratch whenever the library is rescanned, and its `object_id`s are
//! re-issued by that rebuild. A playlist the owner made by hand must outlive all three.
//!
//! So Cinder keeps its own, as ordinary **`.m3u8` files** in `/contents/cinder_playlists/`:
//!
//! * they hold **file paths**, not object ids, so a database rebuild cannot lose them;
//! * `.m3u8` is what the PC-side sync already reads and writes, so a playlist made on the device
//!   can be opened in MusicBee and one made on the PC can be dropped into the folder;
//! * they sit at `/contents`, which is the volume Windows mounts, so they can be backed up and
//!   edited without this program — the same reasoning as `cinder_liked.conf`;
//! * the folder is NOT the music root, where the PC-side sync sweeps away playlists it did not
//!   plan (see `Sony sync/sync.py`, `MANAGED_PLAYLISTS`).
//!
//! Ids are **negative**: MediaStore object ids are positive, so the sign alone tells the shell
//! whether a playlist row came from Sony's database or from here, and `Action::PlayPlaylist(id)`
//! keeps working for both without a second channel. An id is a hash of the file stem, so it is
//! stable across reboots and rebuilds without storing an id anywhere.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the files live, beside the liked list.
pub const DIR: &str = "/contents/cinder_playlists";
const EXT: &str = "m3u8";
/// A playlist name that is longer than this is truncated for the FILE name only; the full name
/// still goes in the `#PLAYLIST:` directive, so nothing the user typed is lost on screen.
const MAX_STEM: usize = 48;
/// Guard against a stray file (or a corrupt one) taking the whole boot: the reference library is
/// 3,945 tracks, and no hand-made playlist is that long.
const MAX_TRACKS: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// The URI/path PlayerService keys on — the same string `cinder_db::Track::filename` carries.
    pub uri: String,
    /// "Artist - Title" for the `#EXTINF` comment. Display only; never used for matching.
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub file: PathBuf,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Default)]
pub struct Store {
    pub dir: PathBuf,
    pub lists: Vec<Playlist>,
}

impl Store {
    /// Read every `.m3u8` in `dir`. A missing directory is an empty store, not an error — the
    /// common case is a device that has never had a playlist made on it.
    pub fn open(dir: impl AsRef<Path>) -> Store {
        let dir = dir.as_ref().to_path_buf();
        let mut lists: Vec<Playlist> = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
                    != Some(EXT.to_string())
                {
                    continue;
                }
                if let Some(list) = parse_file(&path) {
                    lists.push(list);
                }
            }
        }
        lists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Store { dir, lists }
    }

    pub fn get(&self, id: i64) -> Option<&Playlist> {
        self.lists.iter().find(|p| p.id == id)
    }

    fn index_of(&self, id: i64) -> Option<usize> {
        self.lists.iter().position(|p| p.id == id)
    }

    /// Create an empty playlist and write it. Returns its id.
    ///
    /// The file is written immediately rather than on first use: an empty playlist that exists
    /// only in memory would vanish on a reboot, and the reboot might be the crash the user is
    /// about to cause by testing something else.
    pub fn create(&mut self, name: &str) -> std::io::Result<i64> {
        let name = clean_name(name);
        let stem = unique_stem(&name, &self.taken_stems());
        let file = self.dir.join(format!("{stem}.{EXT}"));
        let list = Playlist { id: id_for(&stem), name, file, entries: Vec::new() };
        write_file(&list)?;
        let id = list.id;
        self.lists.push(list);
        self.sort();
        Ok(id)
    }

    /// Rename in place. The FILE keeps its name — the id is derived from the stem, and renaming
    /// the file would change the id under the UI that is holding it. The display name lives in the
    /// `#PLAYLIST:` directive, which is what m3u readers use anyway.
    pub fn rename(&mut self, id: i64, name: &str) -> std::io::Result<()> {
        let Some(index) = self.index_of(id) else { return Ok(()) };
        self.lists[index].name = clean_name(name);
        write_file(&self.lists[index])?;
        self.sort();
        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> std::io::Result<()> {
        let Some(index) = self.index_of(id) else { return Ok(()) };
        let list = self.lists.remove(index);
        match fs::remove_file(&list.file) {
            Ok(()) => Ok(()),
            // Already gone is the outcome we wanted.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Append a track. A track already in the list is NOT added twice — a playlist is a set the
    /// user is curating, and the second tap is much more likely to be "did that register?" than
    /// "I want it twice".
    pub fn add(&mut self, id: i64, uri: &str, label: &str) -> std::io::Result<bool> {
        let Some(index) = self.index_of(id) else { return Ok(false) };
        if uri.trim().is_empty() || self.lists[index].entries.len() >= MAX_TRACKS {
            return Ok(false);
        }
        if self.lists[index].entries.iter().any(|e| e.uri == uri) {
            return Ok(false);
        }
        self.lists[index]
            .entries
            .push(Entry { uri: uri.to_string(), label: label.to_string() });
        write_file(&self.lists[index])?;
        Ok(true)
    }

    pub fn remove_at(&mut self, id: i64, position: usize) -> std::io::Result<bool> {
        let Some(index) = self.index_of(id) else { return Ok(false) };
        if position >= self.lists[index].entries.len() {
            return Ok(false);
        }
        self.lists[index].entries.remove(position);
        write_file(&self.lists[index])?;
        Ok(true)
    }

    fn taken_stems(&self) -> BTreeSet<String> {
        self.lists
            .iter()
            .filter_map(|p| p.file.file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .collect()
    }

    fn sort(&mut self) {
        self.lists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }
}

/// Display name → a safe file stem. Everything that is not a letter, digit, space, dash or
/// underscore becomes `_`, because this string becomes a path on a FAT volume.
pub fn safe_stem(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_alphanumeric() || ch == ' ' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
        if out.chars().count() >= MAX_STEM {
            break;
        }
    }
    let out = out.trim().trim_matches('.').trim().to_string();
    if out.is_empty() {
        "Playlist".to_string()
    } else {
        out
    }
}

fn unique_stem(name: &str, taken: &BTreeSet<String>) -> String {
    let base = safe_stem(name);
    if !taken.contains(&base) {
        return base;
    }
    for n in 2..1000 {
        let candidate = format!("{base} {n}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    format!("{base} {}", std::process::id())
}

/// Trim and collapse whitespace; keep it to something a row can show.
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '\t')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.chars().take(64).collect::<String>();
    if cleaned.is_empty() {
        "Playlist".to_string()
    } else {
        cleaned
    }
}

/// Stable, negative id from the file stem. Negative because MediaStore ids are positive, so the
/// sign is what tells a playlist row where it came from.
pub fn id_for(stem: &str) -> i64 {
    // FNV-1a, 64-bit.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in stem.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Keep it comfortably inside i64 and never 0, then flip the sign.
    -(((hash % (i64::MAX as u64 - 1)) + 1) as i64)
}

fn parse_file(path: &Path) -> Option<Playlist> {
    let body = fs::read_to_string(path).ok()?;
    let stem = path.file_stem()?.to_str()?.to_string();
    let mut name = stem.clone();
    let mut entries: Vec<Entry> = Vec::new();
    let mut pending_label = String::new();

    for line in body.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#PLAYLIST:") {
            let candidate = clean_name(rest);
            if !candidate.is_empty() {
                name = candidate;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#EXTINF:") {
            // "#EXTINF:<seconds>,<label>"
            pending_label = rest.split_once(',').map(|(_, l)| l.trim().to_string()).unwrap_or_default();
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        if entries.len() >= MAX_TRACKS {
            break;
        }
        entries.push(Entry { uri: trimmed.to_string(), label: std::mem::take(&mut pending_label) });
    }

    Some(Playlist { id: id_for(&stem), name, file: path.to_path_buf(), entries })
}

fn write_file(list: &Playlist) -> std::io::Result<()> {
    if let Some(parent) = list.file.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = String::from("#EXTM3U\n");
    body.push_str(&format!("#PLAYLIST:{}\n", list.name));
    for entry in &list.entries {
        if !entry.label.is_empty() {
            body.push_str(&format!("#EXTINF:-1,{}\n", entry.label));
        }
        body.push_str(&entry.uri);
        body.push('\n');
    }
    // Temp file + rename: this volume is exFAT on removable flash that gets unplugged, and a
    // half-written playlist would be a list of tracks with the tail missing rather than an
    // obvious failure.
    let tmp = list.file.with_extension("tmp");
    fs::write(&tmp, body)?;
    fs::rename(&tmp, &list.file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The member paths, in order — what every assertion below actually cares about.
    fn uris(list: &Playlist) -> Vec<String> {
        list.entries.iter().map(|e| e.uri.clone()).collect()
    }

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let path = std::env::temp_dir()
                .join(format!("cinder_pl_{tag}_{}_{:?}", std::process::id(), std::thread::current().id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn create_add_and_reopen() {
        let dir = Dir::new("create");
        let mut store = Store::open(&dir.0);
        assert!(store.lists.is_empty());

        let id = store.create("Night Bus").unwrap();
        assert!(id < 0, "user playlist ids must be negative");
        assert!(store.add(id, "/contents/MUSIC/a.flac", "A - One").unwrap());
        assert!(store.add(id, "/contents/MUSIC/b.flac", "B - Two").unwrap());

        let reopened = Store::open(&dir.0);
        assert_eq!(reopened.lists.len(), 1);
        assert_eq!(reopened.lists[0].name, "Night Bus");
        assert_eq!(reopened.lists[0].id, id, "the id must survive a reopen");
        assert_eq!(uris(&reopened.lists[0]),
                   vec!["/contents/MUSIC/a.flac", "/contents/MUSIC/b.flac"]);
        assert_eq!(reopened.lists[0].entries[1].label, "B - Two");
    }

    #[test]
    fn adding_the_same_track_twice_is_a_no_op() {
        let dir = Dir::new("dup");
        let mut store = Store::open(&dir.0);
        let id = store.create("Dupes").unwrap();
        assert!(store.add(id, "/x/a.flac", "A").unwrap());
        assert!(!store.add(id, "/x/a.flac", "A").unwrap());
        assert_eq!(store.get(id).unwrap().entries.len(), 1);
    }

    #[test]
    fn remove_at_takes_the_right_row() {
        let dir = Dir::new("remove");
        let mut store = Store::open(&dir.0);
        let id = store.create("Trim").unwrap();
        for name in ["a", "b", "c"] {
            store.add(id, &format!("/x/{name}.flac"), name).unwrap();
        }
        assert!(store.remove_at(id, 1).unwrap());
        assert_eq!(uris(store.get(id).unwrap()), vec!["/x/a.flac", "/x/c.flac"]);
        assert!(!store.remove_at(id, 9).unwrap(), "an out-of-range row must not remove anything");
        // and it survives the round trip
        assert_eq!(uris(&Store::open(&dir.0).lists[0]), vec!["/x/a.flac", "/x/c.flac"]);
    }

    #[test]
    fn rename_keeps_the_id_and_the_tracks() {
        let dir = Dir::new("rename");
        let mut store = Store::open(&dir.0);
        let id = store.create("Old").unwrap();
        store.add(id, "/x/a.flac", "A").unwrap();
        store.rename(id, "  New   Name  ").unwrap();

        let reopened = Store::open(&dir.0);
        assert_eq!(reopened.lists[0].name, "New Name", "whitespace is collapsed");
        assert_eq!(reopened.lists[0].id, id, "renaming must not move the id");
        assert_eq!(reopened.lists[0].entries.len(), 1);
    }

    #[test]
    fn delete_removes_the_file() {
        let dir = Dir::new("delete");
        let mut store = Store::open(&dir.0);
        let id = store.create("Gone").unwrap();
        let file = store.get(id).unwrap().file.clone();
        store.delete(id).unwrap();
        assert!(!file.exists());
        assert!(Store::open(&dir.0).lists.is_empty());
        // Deleting twice is not an error — the second call has nothing to do.
        store.delete(id).unwrap();
    }

    #[test]
    fn names_that_are_not_filenames_still_work() {
        let dir = Dir::new("names");
        let mut store = Store::open(&dir.0);
        let id = store.create("2 a.m. / rain :: ЛЕТО").unwrap();
        let list = store.get(id).unwrap();
        let stem = list.file.file_stem().unwrap().to_str().unwrap();
        assert!(!stem.contains('/') && !stem.contains(':'), "stem must be path-safe: {stem}");
        // The name the user typed survives in full, in the file, not in the file NAME.
        assert_eq!(Store::open(&dir.0).lists[0].name, "2 a.m. / rain :: ЛЕТО");
    }

    #[test]
    fn two_playlists_with_the_same_name_get_different_files() {
        let dir = Dir::new("clash");
        let mut store = Store::open(&dir.0);
        let first = store.create("Mix").unwrap();
        let second = store.create("Mix").unwrap();
        assert_ne!(first, second);
        assert_eq!(Store::open(&dir.0).lists.len(), 2);
    }

    #[test]
    fn an_empty_name_becomes_a_usable_one() {
        assert_eq!(clean_name("   "), "Playlist");
        assert_eq!(safe_stem("///"), "___");
    }

    #[test]
    fn junk_lines_are_skipped_when_reading() {
        let dir = Dir::new("junk");
        fs::write(dir.0.join("Hand Made.m3u8"),
                  "#EXTM3U\n#PLAYLIST:Hand Made\n\n# a comment\n#EXTINF:200,A - One\n/x/a.flac\n/x/b.flac\n")
            .unwrap();
        let store = Store::open(&dir.0);
        assert_eq!(store.lists.len(), 1);
        assert_eq!(store.lists[0].name, "Hand Made");
        assert_eq!(uris(&store.lists[0]), vec!["/x/a.flac", "/x/b.flac"]);
        assert_eq!(store.lists[0].entries[0].label, "A - One");
    }

    #[test]
    fn a_playlist_written_by_the_pc_without_our_directives_still_loads() {
        let dir = Dir::new("plain");
        fs::write(dir.0.join("From PC.m3u8"), "/x/a.flac\r\n/x/b.flac\r\n").unwrap();
        let store = Store::open(&dir.0);
        assert_eq!(store.lists[0].name, "From PC", "falls back to the file name");
        assert_eq!(uris(&store.lists[0]).len(), 2);
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(id_for("Night Bus"), id_for("Night Bus"));
        assert_ne!(id_for("Night Bus"), id_for("Night Bus 2"));
        assert!(id_for("anything") < 0);
    }
}
