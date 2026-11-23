//! Artist photos for the Artists tab: `cinder_artists/<Artist>.jpg` (or `.png`) on the player's
//! drive or its card, named exactly as the artist shows in the Artists tab. A name a PC can't put
//! in a file name is percent-escaped: `AC%2FDC.jpg` is "AC/DC", `%25` is a literal `%`.
//!
//! The folder Patchman uses on Walkman One, `ARTIST-PHOTOS`, is read too, so a collection made
//! for it works here unchanged; `cinder_artists` wins when both have the same artist.
//!
//! Each photo is cropped to its centred square and kept in the art cache at the row's 48 px, so
//! a photo is decoded once, not every start. The key carries the file's size and time, so a
//! replaced photo is decoded again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// In priority order: the first folder that has an artist wins.
pub const DIRS: [&str; 3] = [
    "/contents/cinder_artists",
    "/contents_ext/cinder_artists",
    "/contents/ARTIST-PHOTOS",
];
/// A bigger file is ignored rather than decoded: a photo for a 48 px square needs nothing near it.
const MAX_BYTES: u64 = 8 << 20;

/// "AC%2FDC" -> "AC/DC". An escape that isn't two hex digits stays as written.
pub fn unescape(stem: &str) -> String {
    let b = stem.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One file per artist from `(folder rank, path)`: the earlier folder wins, then JPEG over PNG
/// (as Patchman does), and anything else is not a photo.
pub fn pick(files: impl IntoIterator<Item = (usize, PathBuf)>) -> BTreeMap<String, PathBuf> {
    let mut best: BTreeMap<String, ((usize, u8), PathBuf)> = BTreeMap::new();
    for (dir, p) in files {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let ext_rank = match ext.as_deref() {
            Some("jpg" | "jpeg") => 0,
            Some("png") => 1,
            _ => continue,
        };
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let name = unescape(stem);
        let rank = (dir, ext_rank);
        if best.get(&name).is_none_or(|(r, _)| rank < *r) {
            best.insert(name, (rank, p));
        }
    }
    best.into_iter().map(|(n, (_, p))| (n, p)).collect()
}

/// The photos on the drive, or `None` while the drive itself can't be read yet (at boot it is
/// mounted some seconds after the library opens), so the caller knows to try again.
pub fn scan() -> Option<BTreeMap<String, PathBuf>> {
    std::fs::read_dir("/contents").ok()?;
    let files = DIRS.iter().enumerate().flat_map(|(i, d)| {
        std::fs::read_dir(d)
            .into_iter()
            .flat_map(|rd| rd.flatten())
            .map(move |e| (i, e.path()))
    });
    Some(pick(files))
}

/// One photo at 48 px: from the art cache, or decoded, cropped and cached.
pub fn load(path: &Path) -> Option<cinder_ui::art::Image> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() == 0 || meta.len() > MAX_BYTES {
        return None;
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let key =
        crate::art_cache::key_of(&format!("{}#artist:{mtime}:{}", path.display(), meta.len()));
    crate::art_cache::load(key, crate::art_cache::T48).or_else(|| {
        let full = crate::art_load::decode(&std::fs::read(path).ok()?)?;
        crate::art_cache::store_image(key, &full.center_square())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_pc_cannot_store_are_unescaped() {
        assert_eq!(unescape("AC%2FDC"), "AC/DC");
        assert_eq!(unescape("100%25"), "100%");
        assert_eq!(unescape("Sigur R%C3%B3s"), "Sigur Rós");
        // Not an escape: left alone.
        assert_eq!(unescape("50%"), "50%");
        assert_eq!(unescape("a%zzb"), "a%zzb");
        assert_eq!(unescape("Pink Floyd"), "Pink Floyd");
    }

    #[test]
    fn the_first_folder_then_jpeg_wins_and_other_files_are_skipped() {
        let f = |d: usize, p: &str| (d, PathBuf::from(p));
        let got = pick([
            f(2, "/x/Queen.jpg"),
            f(0, "/a/Queen.png"),
            f(0, "/a/Björk.png"),
            f(0, "/a/Björk.JPG"),
            f(0, "/a/notes.txt"),
            f(1, "/b/AC%2FDC.jpeg"),
        ]);
        assert_eq!(got["Queen"], PathBuf::from("/a/Queen.png"));
        assert_eq!(got["Björk"], PathBuf::from("/a/Björk.JPG"));
        assert_eq!(got["AC/DC"], PathBuf::from("/b/AC%2FDC.jpeg"));
        assert_eq!(got.len(), 3);
    }
}
