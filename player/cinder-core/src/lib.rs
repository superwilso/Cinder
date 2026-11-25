//! Services shared by both Cinder apps: v0.4's `cinder-ffi` and v0.5's `cinder-app5`
//! (docs/PLAN_v0.5.md). Moved out of `cinder-ffi` unchanged; nothing here exports a C symbol or
//! draws, except `flac`: the decoder the audio engine calls by C name, which both apps link once
//! through this crate.

pub mod books;
pub mod cue;
pub mod display;
pub mod flac;
pub mod library;
pub mod likes;
pub mod mdns;
pub mod ovl;
pub mod pcm_tap;
pub mod present;
pub mod scrobble;
pub mod series;
pub mod shared;
pub mod stats;

/// Temp file + fsync + rename, for settings, views, stats, likes and playlists. `/contents` is
/// removable flash a user unplugs, so a torn write must leave the previous file, never half of one.
pub fn write_atomic(
    path: impl AsRef<std::path::Path>,
    body: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(body.as_ref())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}
