//! Deleting music from the player: Settings ▸ Delete music, then DELETE on Track information or an
//! album page (`Action::DeleteMusic`). The files go here; the library catches up through the same
//! rescan that drive and phone uploads trigger ([`crate::remote::note_uploaded`]).
//!
//! Only files under the music roots are touched. After the songs go, a folder left holding only
//! what came with them (cover art, a .cue, lyrics, a rip log, Windows and macOS litter) goes too,
//! up to but never including the folder directly under a root (`/contents/MUSIC` stays).

use std::path::{Component, Path};

pub const ROOTS: [&str; 2] = ["/contents/", "/contents_ext/"];

/// What an album folder carries besides its songs. A folder holding anything else is kept.
const LEFTOVER: [&str; 17] = [
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "lrc", "cue", "txt", "nfo", "log", "m3u", "m3u8",
    "sfv", "md5", "ini", "db",
];

fn under_root<'a>(p: &Path, roots: &[&'a str]) -> Option<&'a str> {
    if p.components().any(|c| c == Component::ParentDir) {
        return None;
    }
    roots
        .iter()
        .copied()
        .find(|r| p.to_str().is_some_and(|s| s.starts_with(r)))
}

fn leftover(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with('.')
        || p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| LEFTOVER.contains(&e.to_ascii_lowercase().as_str()))
}

/// Remove `dir` if it holds only leftovers, then try its parent. Stops at the root's own folders.
fn tidy(dir: &Path, root: &str) {
    let mut dir = dir.to_path_buf();
    // `/contents/MUSIC/A/B` → components after the root: MUSIC, A, B. Keep MUSIC.
    while dir
        .strip_prefix(root)
        .map_or(0, |rest| rest.components().count())
        >= 2
    {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        if entries.iter().any(|e| e.is_dir() || !leftover(e)) {
            return;
        }
        for e in &entries {
            let _ = std::fs::remove_file(e);
        }
        if std::fs::remove_dir(&dir).is_err() {
            return;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return,
        }
    }
}

/// Delete `paths` (and each song's own .lrc). Returns (deleted, failed).
pub fn delete(paths: &[String], roots: &[&str]) -> (u32, u32) {
    let (mut ok, mut failed) = (0, 0);
    let mut dirs: Vec<(std::path::PathBuf, &str)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in paths {
        if !seen.insert(p.as_str()) {
            continue; // a CUE album lists its one file once per track
        }
        let path = Path::new(p);
        let Some(root) = under_root(path, roots) else {
            eprintln!("cinder-ffi: delete: not under a music root, kept: {p}");
            failed += 1;
            continue;
        };
        match std::fs::remove_file(path) {
            Ok(()) => {
                ok += 1;
                let _ = std::fs::remove_file(path.with_extension("lrc"));
                if let Some(d) = path.parent() {
                    if !dirs.iter().any(|(x, _)| x == d) {
                        dirs.push((d.to_path_buf(), root));
                    }
                }
            }
            Err(e) => {
                eprintln!("cinder-ffi: delete {p}: {e}");
                failed += 1;
            }
        }
    }
    for (d, root) in &dirs {
        tidy(d, root);
    }
    (ok, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn deletes_songs_tidies_the_album_folder_and_keeps_the_rest() {
        let base = std::env::temp_dir().join(format!("cinder_delete_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = format!("{}/", base.display());
        let album = base.join("MUSIC/Artist/Album");
        let other = base.join("MUSIC/Artist/Other");
        fs::create_dir_all(&album).unwrap();
        fs::create_dir_all(&other).unwrap();
        for f in [
            "01.flac",
            "01.lrc",
            "02.flac",
            "cover.jpg",
            "Thumbs.db",
            "._01.flac",
        ] {
            fs::write(album.join(f), b"x").unwrap();
        }
        fs::write(other.join("01.mp3"), b"x").unwrap();
        let s = |p: &Path| p.to_str().unwrap().to_string();
        let outside = std::env::temp_dir().join("cinder_delete_outside.flac");
        fs::write(&outside, b"x").unwrap();

        // One song: the other song keeps the folder.
        let (ok, bad) = delete(&[s(&album.join("01.flac"))], &[&root]);
        assert_eq!((ok, bad), (1, 0));
        assert!(!album.join("01.lrc").exists(), "its lyrics go with it");
        assert!(album.join("02.flac").exists());

        // The rest of the album, listed twice (a CUE sheet does that): the folder goes, the
        // artist folder stays for Other, MUSIC is never touched.
        let p = s(&album.join("02.flac"));
        assert_eq!(delete(&[p.clone(), p], &[&root]), (1, 0));
        assert!(!album.exists());
        assert!(other.join("01.mp3").exists());
        assert_eq!(delete(&[s(&other.join("01.mp3"))], &[&root]), (1, 0));
        assert!(!base.join("MUSIC/Artist").exists());
        assert!(
            base.join("MUSIC").exists(),
            "the folder under the root stays"
        );

        // Outside the roots, or climbing out of one: refused.
        assert_eq!(delete(&[s(&outside)], &[&root]), (0, 1));
        assert!(outside.exists());
        let sneaky = format!("{root}MUSIC/../../cinder_delete_outside.flac");
        assert_eq!(delete(&[sneaky], &[&root]), (0, 1));
        assert!(outside.exists());
        // Already gone: counted as a failure, nothing else happens.
        assert_eq!(delete(&[s(&album.join("02.flac"))], &[&root]), (0, 1));

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&base);
    }
}
