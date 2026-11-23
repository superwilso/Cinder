//! What an AirPlay sender says is playing. cinder-home reads shairport-sync's metadata pipe
//! (`airplay_meta.h`) on its AirPlay reader thread and hands the song's names and cover here; Now
//! Playing shows them while AirPlay has the sound (`live_np`).

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

/// The latest cover, decoded on the caller's thread (cinder-home's AirPlay reader), waiting for
/// Now Playing to take it: `Some(None)` = the sender's song has no cover (back to the gradient).
static ART: Mutex<Option<Option<cinder_ui::art::Image>>> = Mutex::new(None);

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a NUL-terminated string that lives for the call.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// The sender's title, artist and album; empty strings or NULL for none (a new session clears them).
#[no_mangle]
pub extern "C" fn cinder_set_airplay_meta(
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
) {
    let (t, a, al) = (text(title), text(artist), text(album));
    if let Ok(mut g) = crate::cell().lock() {
        if let Some(r) = g.as_mut() {
            r.app.set_airplay_meta(&t, &a, &al);
            r.dirty = true;
        }
    }
}

/// Which cover is the latest: a decode that finishes after a newer cover arrived is thrown away.
static ART_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The sender's cover (JPEG or PNG bytes), copied and decoded on a thread of its own: the caller is
/// cinder-home's AirPlay reader, which also feeds the jack, and a big cover decoded at the
/// screen-off clock outlasted the jack's 250 ms buffer (a glitch at every new song). A NULL or a
/// file that will not decode clears it.
///
/// # Safety
/// `data` must point to `len` readable bytes, or be NULL.
#[no_mangle]
pub unsafe extern "C" fn cinder_set_airplay_art(data: *const u8, len: c_int) {
    use std::sync::atomic::Ordering;
    let gen = ART_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    if data.is_null() || len <= 0 {
        store_art(gen, None);
        return;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `data`; copied before returning.
    let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) }.to_vec();
    let spawned = std::thread::Builder::new()
        .name("airplay-art".into())
        .spawn(move || store_art(gen, crate::art_load::decode(&bytes)));
    if spawned.is_err() {
        store_art(gen, None);
    }
}

fn store_art(gen: u64, img: Option<cinder_ui::art::Image>) {
    if ART_GEN.load(std::sync::atomic::Ordering::SeqCst) != gen {
        return; // a newer cover came in meanwhile
    }
    *ART.lock().unwrap() = Some(img);
    if let Ok(mut g) = crate::cell().lock() {
        if let Some(r) = g.as_mut() {
            r.dirty = true;
        }
    }
}

/// Where the sender's song is: (position ms, length ms, when that position was true). Paused, the
/// clock is moved up to the present each frame so the position stops with it.
static PROGRESS: Mutex<Option<(i64, i64, std::time::Instant)>> = Mutex::new(None);

/// Where the sender's song is (`pos_ms` -1 = not said, which keeps the count going) and its
/// length (0 = not known).
#[no_mangle]
pub extern "C" fn cinder_set_airplay_progress(pos_ms: i64, dur_ms: i64) {
    let now = std::time::Instant::now();
    let mut g = PROGRESS.lock().unwrap();
    if pos_ms < 0 && dur_ms <= 0 {
        *g = None; // a new session: nothing is known yet
        return;
    }
    let pos = if pos_ms >= 0 {
        pos_ms
    } else {
        g.map(|(p, _, at)| p + now.duration_since(at).as_millis() as i64)
            .unwrap_or(0)
    };
    *g = Some((pos, dur_ms.max(0), now));
    drop(g);
    if let Ok(mut g) = crate::cell().lock() {
        if let Some(r) = g.as_mut() {
            r.dirty = true;
        }
    }
}

/// The song's position and length now, if the sender said; the position stands still while
/// `playing` is false.
pub(crate) fn progress(playing: bool) -> Option<(i64, i64)> {
    let now = std::time::Instant::now();
    let mut g = PROGRESS.lock().unwrap();
    let (p, d, at) = (*g)?;
    let pos = p + now.duration_since(at).as_millis() as i64;
    if !playing {
        *g = Some((p, d, now));
        return Some((p, d));
    }
    Some((pos, d))
}

/// What arrived since the last call: `None` nothing new, `Some(None)` no cover, `Some(Some(_))` one.
pub(crate) fn take_art() -> Option<Option<cinder_ui::art::Image>> {
    ART.lock().unwrap().take()
}
