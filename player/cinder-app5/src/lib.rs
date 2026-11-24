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
mod stubs;

use backend::{Backend, Cmd, Snapshot};
use cinder_core::display::{Framebuffer, Sink, FRAMES_PRESENTED};
use cinder_core::present;
use cinder_ui::theme::Theme;
use cinder_ui::{now_playing, Canvas, FontSet};
use std::ffi::{c_char, CStr};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

/// The UI half: touched only by the shell's render thread (render tick and input).
struct Ui {
    sink: Option<Sink>,
    canvas: Canvas,
    fonts: FontSet,
    theme: Theme,
    /// The snapshot version last drawn; a newer one means repaint.
    drawn: u64,
    dirty: bool,
}

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

fn draw(u: &mut Ui, s: &Snapshot) {
    let np = now_playing::NowPlaying {
        skin: None,
        tape: None,
        title: &s.title,
        artist: "",
        codec: "",
        badge: "",
        clock: "",
        battery: s.battery,
        elapsed: "",
        remaining: "",
        progress: s.progress,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: s.playing,
        shuffle: false,
        repeat: 0,
        viz_seed: 0.0,
        viz_kind: 0,
        viz_size: 0,
        page: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        scrubbing: false,
        lyrics: false,
    };
    now_playing::render(&mut u.canvas, &u.theme, &u.fonts, &np);
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
    *ui().lock().unwrap() = Some(Ui {
        sink,
        canvas: Canvas::new(),
        fonts: FontSet::load(),
        theme: Theme::day(),
        drawn: u64::MAX,
        dirty: true,
    });
    0
}

/// Draw a frame if the snapshot moved on. The backend can be busy for any length of time; this
/// never waits for it.
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let s = backend().snapshot();
    let mut g = ui().lock().unwrap();
    let Some(u) = g.as_mut() else { return };
    if !u.dirty && u.drawn == s.version {
        return;
    }
    draw(u, &s);
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

/// For tests: the latest snapshot.
#[cfg(test)]
fn latest() -> std::sync::Arc<Snapshot> {
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
        for _ in 0..30 {
            cinder_force_dirty();
            let t = Instant::now();
            cinder_render_tick();
            assert!(
                t.elapsed() < Duration::from_millis(20),
                "frame took {:?}",
                t.elapsed()
            );
        }
        // The title arrives once the backend is free.
        let t = Instant::now();
        while latest().title != "Atlas Hands" {
            assert!(t.elapsed() < Duration::from_secs(2), "the command was lost");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
