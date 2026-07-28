//! Persistent album-art thumbnail cache.
//!
//! WHY THIS EXISTS: the covers on this device are ~1425x1425 JPEGs embedded inside each FLAC, and
//! decoding one costs **365 ms** (measured on device, `cinder-probe --art`). Scaling is cheap
//! afterwards (5 ms). So a library list — eight rows on screen, repainting while a finger drags —
//! can never decode covers on demand; the only workable shape is to decode each album's cover
//! exactly once, ever, and keep the small result.
//!
//! LAYOUT: one raw file per album per size under `/data/cinder/artcache`:
//!   `<album_id>.t48`  48x48 RGB, 6912 B   — what every list row draws
//!   `<album_id>.t96`  96x96 RGB, 27648 B  — what the album drill-in draws
//! Raw packed RGB, no header: the size IS the validation (a short/corrupt file is rejected by
//! length and simply re-decoded). One file per album rather than a pack file so a partial build is
//! resumable, corruption is isolated to one album, and no rewrite of a large file is ever needed.
//!
//! WHY /data AND NOT /contents: `/contents` is vfat, is handed to the PC during USB-MSC (so it
//! vanishes mid-session), and is the partition that already ate the bad-boot counter once. `/data`
//! is ext4 and USB-MSC never touches it. Same reasoning, same conclusion as the boot counter.
//!
//! The whole cache is disposable: delete the directory and it rebuilds. Nothing here is allowed to
//! fail loudly — every I/O error degrades to "no thumbnail", and the UI draws its gradient.

use cinder_ui::art::Image;
use std::os::unix::fs::PermissionsExt;

const DEFAULT_DIR: &str = "/data/cinder/artcache";

/// Cache directory. `CINDER_ART_CACHE` overrides it — used by the tests, and handy on device for
/// pointing a run at a throwaway directory without disturbing the real cache.
pub fn dir() -> String {
    std::env::var("CINDER_ART_CACHE").unwrap_or_else(|_| DEFAULT_DIR.to_string())
}
/// List-row thumbnail edge (must match what `library::thumb` asks for).
pub const T48: usize = 48;
/// Album drill-in cover edge (must match `library::album_view`).
pub const T96: usize = 96;

// "Must match" is now enforced rather than asked for. `library::thumb` falls back to the gradient
// when the cached image is not EXACTLY the requested size, which is silent: the Artists tab asked
// for 44 px against this 48 and drew gradients for every artist, with no error anywhere and the
// decoded covers sitting in memory unused. A mismatch is a build failure now.
const _: () = assert!(T48 == cinder_ui::library::THUMB_PX as usize);
const _: () = assert!(T96 == cinder_ui::library::COVER_PX as usize);

fn path(album_id: i64, edge: usize) -> String {
    format!("{}/{album_id}.t{edge}", dir())
}

/// Read one cached thumbnail. None if absent, unreadable, or the wrong length.
pub fn load(album_id: i64, edge: usize) -> Option<Image> {
    let want = edge * edge * 3;
    let rgb = match std::fs::read(path(album_id, edge)) {
        Ok(b) => b,
        Err(e) => {
            // Unreadable (wrong owner/mode — see `store`). Drop it so the builder can replace it
            // with one we own; the directory is world-writable precisely so this can work.
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                let _ = std::fs::remove_file(path(album_id, edge));
            }
            return None;
        }
    };
    if rgb.len() != want {
        // Truncated (a write interrupted by power loss). Drop it; the builder will redo it.
        let _ = std::fs::remove_file(path(album_id, edge));
        return None;
    }
    Some(Image { w: edge, h: edge, rgb })
}

/// Write one thumbnail. Temp file + rename, so a reader can never see a half-written cover and an
/// interrupted write leaves the old file (or none) rather than a corrupt one.
fn store(album_id: i64, img: &Image) -> std::io::Result<()> {
    let final_path = path(album_id, img.w);
    let tmp = format!("{final_path}.part");
    std::fs::write(&tmp, &img.rgb)?;
    // World-readable ON PURPOSE. cinder-home runs as uid 100, but cinder-probe (and anything else
    // run over adb) runs as root, so whichever builds the cache first decides who can use it —
    // and root's default 0600 locks the app out of its own covers. Same reason ensure_dir opens
    // the directory up: a root-created 0755 directory is one the app cannot write into.
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o666))?;
    std::fs::rename(&tmp, &final_path)
}

/// True if this album's thumbnails are already cached at both sizes AND we can actually read them.
///
/// Readability is part of the question, not a detail: `metadata` succeeds on a file this process
/// has no permission to open, so a length-only check would mark a cover "cached" that we can never
/// draw — and the builder would skip it forever. That is a live scenario here, because the cache
/// may be built by cinder-probe as root or by cinder-home as uid 100.
fn usable(album_id: i64, edge: usize) -> bool {
    match std::fs::File::open(path(album_id, edge)) {
        Ok(f) => f.metadata().map(|m| m.len() as usize == edge * edge * 3).unwrap_or(false),
        Err(_) => false,
    }
}

pub fn is_cached(album_id: i64) -> bool {
    usable(album_id, T48) && usable(album_id, T96)
}

/// Load every already-cached 48x48 thumbnail for the given albums. Called once at library build,
/// before the builder thread starts, so a device that has run before shows covers immediately.
pub fn load_all(album_ids: impl Iterator<Item = i64>) -> std::collections::HashMap<i64, Image> {
    let mut out = std::collections::HashMap::new();
    for id in album_ids {
        if let Some(img) = load(id, T48) {
            out.insert(id, img);
        }
    }
    out
}

/// Decode one album's cover and write both sizes. Returns the 48x48 for the live UI map.
///
/// `object_id` is any track on the album — they all embed the same picture.
pub fn build_one(db: &cinder_db::Db, album_id: i64, object_id: i64) -> Option<Image> {
    let native = crate::art_load::load(db, object_id)?;
    let t96 = native.scaled_to(T96, T96);
    let t48 = native.scaled_to(T48, T48);
    // The native decode is the big allocation (a 1425x1425 cover is 6 MB of RGB). Drop it before
    // touching the filesystem so the peak doesn't overlap with anything else this thread does —
    // this process has already died once from allocation failure under fragmentation.
    drop(native);
    if let Err(e) = store(album_id, &t96) {
        eprintln!("cinder-ffi: art cache: write {album_id}.t96 failed: {e}");
    }
    if let Err(e) = store(album_id, &t48) {
        eprintln!("cinder-ffi: art cache: write {album_id}.t48 failed: {e}");
        return None;
    }
    Some(t48)
}

/// Create the cache directory. Returns false if it isn't usable (then the whole feature no-ops).
pub fn ensure_dir() -> bool {
    let d = dir();
    if let Err(e) = std::fs::create_dir_all(&d) {
        eprintln!("cinder-ffi: art cache: {d} unusable ({e}) — covers stay as gradients");
        return false;
    }
    // See `store`: the builder may run as root (probe) or uid 100 (the app), and both must be able
    // to add files. Best-effort — it fails harmlessly when we don't own the directory.
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o777));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CINDER_ART_CACHE` is process-global, so these tests cannot run concurrently with each
    /// other — without this they interleave and one test reads another's directory.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn tmp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("cinder_art_test_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    fn img(edge: usize, fill: u8) -> Image {
        Image { w: edge, h: edge, rgb: vec![fill; edge * edge * 3] }
    }

    #[test]
    fn round_trips_and_reports_cached() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tmp("roundtrip");
        std::env::set_var("CINDER_ART_CACHE", &d);
        assert!(ensure_dir());
        assert!(!is_cached(7));
        store(7, &img(T48, 0xAB)).unwrap();
        store(7, &img(T96, 0xCD)).unwrap();
        assert!(is_cached(7));
        let back = load(7, T48).expect("cached 48 reads back");
        assert_eq!((back.w, back.h), (T48, T48));
        assert!(back.rgb.iter().all(|&b| b == 0xAB));
        assert_eq!(load(7, T96).unwrap().rgb[0], 0xCD);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A write cut short by power loss must not be served as a cover — and must not be sticky
    /// either, or that album would never get another chance.
    #[test]
    fn truncated_file_is_rejected_and_removed() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tmp("truncated");
        std::env::set_var("CINDER_ART_CACHE", &d);
        assert!(ensure_dir());
        std::fs::write(format!("{d}/9.t48"), vec![0u8; 100]).unwrap();
        assert!(load(9, T48).is_none());
        assert!(!std::path::Path::new(&format!("{d}/9.t48")).exists(), "corrupt file left behind");
        assert!(!is_cached(9));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn load_all_skips_missing() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = tmp("loadall");
        std::env::set_var("CINDER_ART_CACHE", &d);
        assert!(ensure_dir());
        store(1, &img(T48, 1)).unwrap();
        store(3, &img(T48, 3)).unwrap();
        let m = load_all([1i64, 2, 3].into_iter());
        assert_eq!(m.len(), 2);
        assert!(m.contains_key(&1) && m.contains_key(&3) && !m.contains_key(&2));
        std::fs::remove_dir_all(&d).ok();
    }
}
