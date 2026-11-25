//! Cinder v0.5 (docs/PLAN_v0.5.md). It exports the same `cinder.h` as v0.4's `cinder-ffi`, so the
//! shell does not change. The difference is who waits for whom:
//!
//! - **Shell calls that change something** only queue a command for the backend thread (`backend.rs`).
//! - **The UI** is the shell's render thread. `cinder_render_tick` and input run there. It draws from
//!   the latest snapshot and never takes a lock the backend holds while working.
//! - **Frames** leave through `cinder-core`'s present thread, as in v0.4.
//!
//! Exports land here one by one as screens are built. Until then the shell harness stubs the rest
//! (`cinder-home/harness/gen_stubs.py --no-ffi=`).

mod backend;
mod settings;
mod stubs;
mod ui;

use backend::{Backend, Cmd};
use cinder_core::display::{Framebuffer, Sink, FRAMES_PRESENTED};
use cinder_core::present;
use std::ffi::{c_char, CStr};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use ui::Ui;

fn backend() -> &'static Backend {
    static B: OnceLock<Backend> = OnceLock::new();
    B.get_or_init(Backend::start)
}

fn ui() -> &'static Mutex<Option<Ui>> {
    static U: Mutex<Option<Ui>> = Mutex::new(None);
    &U
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// Open the panel and start the backend. 0 = ok, < 0 = error.
#[no_mangle]
pub extern "C" fn cinder_render_init() -> libc::c_int {
    backend();
    let sink = if std::env::var_os("CINDER_FFI_HEADLESS").is_some() {
        None
    } else {
        match present::PresentThread::start(Framebuffer::open) {
            Ok(t) => Some(Sink::Threaded(t)),
            Err(e) => {
                eprintln!("cinder-app5: no panel: {e}");
                return -1;
            }
        }
    };
    *ui().lock().unwrap() = Some(Ui::new(sink));
    0
}

/// Draw a frame if the snapshot moved on. The backend can be busy for any length of time; this
/// never waits for it.
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let s = backend().snapshot();
    let mut g = ui().lock().unwrap();
    let Some(u) = g.as_mut() else { return };
    // While the library builds the busy ring turns, so every tick draws.
    let spinning = s.lib == backend::LoadState::Loading;
    if !u.dirty && !spinning && u.drawn == s.version {
        return;
    }
    u.draw(&s);
    u.drawn = s.version;
    u.dirty = false;
    let Ui { sink, canvas, .. } = u;
    if let Some(sink) = sink {
        sink.present(canvas);
    } else {
        FRAMES_PRESENTED.fetch_add(1, Ordering::SeqCst);
    }
}

#[no_mangle]
pub extern "C" fn cinder_frames_presented() -> u64 {
    FRAMES_PRESENTED.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn cinder_force_dirty() {
    if let Some(u) = ui().lock().unwrap().as_mut() {
        u.dirty = true;
    }
}

#[no_mangle]
pub extern "C" fn cinder_render_shutdown() {
    *ui().lock().unwrap() = None;
}

/// The shell's position report. It only queues; the title follows on the next frame after the
/// backend has applied it.
///
/// # Safety
/// `uri` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cinder_set_now_playing_uri(
    uri: *const c_char,
    progress: f32,
    playing: libc::c_int,
    battery: libc::c_int,
) -> libc::c_int {
    backend().send(Cmd::NowPlaying {
        uri: cstr(uri),
        progress,
        playing: playing != 0,
        battery: battery.clamp(0, 100) as u8,
    });
    1
}

/// Open the library store and build the library. The shell calls this on its own worker thread and
/// reads the code (0 ok, -1 cannot open, -3 damaged: it then restores its known-good copy and calls
/// again), so the build runs here, in the caller. No UI or backend lock is held meanwhile: the
/// result goes to the backend as one `Arc`, and frames keep drawing throughout.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cinder_db_open(path: *const c_char) -> libc::c_int {
    backend().send(Cmd::LibLoading);
    let rc = match cinder_db::Db::open(&cstr(path)) {
        Err(e) => {
            eprintln!("cinder-app5: library store won't open: {e}");
            Err(-1)
        }
        Ok(db) => match db.health() {
            Err(e) => {
                eprintln!("cinder-app5: library store is damaged: {e}");
                Err(-3)
            }
            // ponytail: no stats, likes or playlist store yet; they join when lib5 shows them.
            Ok(_) => Ok(std::sync::Arc::new(
                cinder_core::library::build_library_with(&db, None, true),
            )),
        },
    };
    let code = rc.as_ref().map_or_else(|c| *c, |_| 0);
    backend().send(Cmd::Library(rc));
    code
}

/// What the library is built from, as one number (0 = cannot tell). Read-only, any thread.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cinder_db_content_signature(path: *const c_char) -> u64 {
    if path.is_null() {
        return 0;
    }
    cinder_db::content_signature(&cstr(path)).unwrap_or(0)
}

/// A tap at UI coordinates; returns a `cinder_action_t`.
#[no_mangle]
pub extern "C" fn cinder_tap(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    ui().lock().unwrap().as_mut().map_or(0, |u| u.tap(x, y))
}

/// A horizontal swipe (`dir` < 0 = leftward) starting at `(x, y)`.
#[no_mangle]
pub extern "C" fn cinder_swipe(dir: libc::c_int, _x: libc::c_int, _y: libc::c_int) -> libc::c_int {
    ui().lock().unwrap().as_mut().map_or(0, |u| u.swipe(dir))
}

/// A hardware button (`cinder_button_t`).
#[no_mangle]
pub extern "C" fn cinder_input(button: libc::c_int) -> libc::c_int {
    ui().lock().unwrap().as_mut().map_or(0, |u| u.input(button))
}

/// For tests: the latest snapshot.
#[cfg(test)]
fn latest() -> std::sync::Arc<backend::Snapshot> {
    backend().snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// The plan's stall test: while the backend is busy for 500 ms, the UI keeps drawing (no frame
    /// over 20 ms) and a shell call returns at once (under 1 ms).
    #[test]
    fn a_busy_backend_never_stalls_a_frame_or_a_call() {
        std::env::set_var("CINDER_FFI_HEADLESS", "1");
        assert_eq!(cinder_render_init(), 0);
        backend().send(Cmd::Sleep(500));
        let t = Instant::now();
        unsafe { cinder_set_now_playing_uri(c"/music/Atlas Hands.flac".as_ptr(), 0.4, 1, 78) };
        assert!(
            t.elapsed() < Duration::from_millis(1),
            "a call waited on the backend"
        );
        // A frame that waited on the backend would take its 500 ms; the limit sits well under that
        // and well over a raster (~5 ms release, ~50 ms unoptimised), so a busy test machine cannot
        // fail it. The 16 ms raster budget is measured on the player (`devctl.sh stalls`).
        let budget = Duration::from_millis(if cfg!(debug_assertions) { 200 } else { 100 });
        for _ in 0..30 {
            cinder_force_dirty();
            let t = Instant::now();
            cinder_render_tick();
            assert!(t.elapsed() < budget, "frame took {:?}", t.elapsed());
        }
        // The title arrives once the backend is free.
        let t = Instant::now();
        while latest().title != "Atlas Hands" {
            assert!(t.elapsed() < Duration::from_secs(2), "the command was lost");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    /// Wait for the backend to publish a snapshot that passes `ok`.
    fn wait_for(ok: impl Fn(&backend::Snapshot) -> bool) -> std::sync::Arc<backend::Snapshot> {
        let t = Instant::now();
        loop {
            let s = latest();
            if ok(&s) {
                return s;
            }
            assert!(
                t.elapsed() < Duration::from_secs(2),
                "the backend never got there"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A store file built from SQL, the way Sony's scanner lays out the tables the build reads.
    fn store(name: &str, sql: &str) -> std::ffi::CString {
        let path = std::env::temp_dir().join(format!("app5-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = cinder_db::Db::open_in_memory_with(|c| c.execute_batch(sql)).unwrap();
        let p = path.to_str().unwrap();
        db.conn()
            .execute_batch(&format!("VACUUM INTO '{p}'"))
            .unwrap();
        std::ffi::CString::new(p).unwrap()
    }

    /// The shell reads the code (0 / -1 / -3: it restores its known-good copy on -3), and the
    /// library reaches the snapshot as one piece. A failed reopen keeps what is already shown.
    #[test]
    fn opening_the_store_publishes_the_library_and_answers_the_shells_codes() {
        let good = store(
            "good",
            r#"
            CREATE TABLE albums  (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE artists (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT, imagefile TEXT, face_x INTEGER, face_y INTEGER, face_w INTEGER, face_h INTEGER);
            CREATE TABLE schema  (prop_type INTEGER, akey INTEGER, data_type INTEGER, prop_name TEXT, PRIMARY KEY(prop_type,akey));
            CREATE TABLE object_ext_int (object_id INTEGER, akey INTEGER, value INTEGER DEFAULT 0, PRIMARY KEY(object_id,akey));
            CREATE TABLE images  (id INTEGER PRIMARY KEY, dataform INTEGER, dataoffset INTEGER, datasize INTEGER, value TEXT, digest TEXT, bmpfile TEXT, bmpwidth INTEGER, bmpheight INTEGER);
            CREATE TABLE releaseyears (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE object_body (
                object_id INTEGER PRIMARY KEY AUTOINCREMENT, object_type INTEGER NOT NULL,
                parent_id INTEGER, reference_id INTEGER,
                child_index INTEGER, media_type INTEGER DEFAULT 0, format INTEGER DEFAULT 0,
                initial INTEGER, sort_str TEXT, search_str TEXT, title TEXT DEFAULT "",
                addedtime INTEGER DEFAULT 0, filename TEXT, filesize INTEGER,
                series_no INTEGER, disc_no INTEGER, is_high_resolution INTEGER,
                album_id INTEGER, artist_id INTEGER, albumartist_id INTEGER, releaseyear_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,album_id,artist_id,albumartist_id)
              VALUES (1,1,1,'Atlas Hands','/music/atlas.flac',1,1,10,20,20);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,album_id,artist_id,albumartist_id)
              VALUES (2,1,1,'Box of Stones','/music/box.flac',2,1,10,20,20);
            "#,
        );
        assert_eq!(unsafe { cinder_db_open(good.as_ptr()) }, 0);
        let s = wait_for(|s| s.lib == backend::LoadState::Ready);
        assert_eq!(s.library.songs.len(), 2);
        assert_eq!(s.library.album_count(), 1);

        assert_eq!(
            unsafe { cinder_db_open(c"/nonexistent/MTPDB.dat".as_ptr()) },
            -1
        );
        let s = wait_for(|s| s.lib == backend::LoadState::Failed(-1));
        assert_eq!(
            s.library.songs.len(),
            2,
            "a failed reopen blanked the library"
        );

        let damaged = store("damaged", "CREATE TABLE albums (id INTEGER PRIMARY KEY);");
        assert_eq!(unsafe { cinder_db_open(damaged.as_ptr()) }, -3);
        wait_for(|s| s.lib == backend::LoadState::Failed(-3));
        assert_ne!(unsafe { cinder_db_content_signature(good.as_ptr()) }, 0);
        for f in [good, damaged] {
            let _ = std::fs::remove_file(f.to_str().unwrap());
        }
    }
}
