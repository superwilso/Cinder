//! cinder-ffi — C-ABI surface over the Rust Cinder UI, for the C++ easel shell
//! (`cinder-home`). One glibc process: the C++ shell does the appmgr/easel lifecycle
//! and the Sony IPC, then calls these `extern "C"` entry points to paint the panel.
//!
//! Frame model: the C++ pump calls `cinder_render_tick()` once per frame; the shell
//! pushes state via the setters. All state lives behind a Mutex; panics abort (the
//! workspace profile sets panic="abort"), so nothing unwinds across the FFI boundary.

// These are `#[no_mangle] extern "C"` entry points called from C++, so they legitimately take
// raw `*const c_char` args; every deref goes through `cstr()` which null-checks first. The lint
// (which assumes safe-Rust callers) is a false positive for this FFI surface.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod art_cache;
mod art_load;
mod gpu;
mod present;
mod scrobble;
mod spectrum;

use cinder_ui::now_playing::NowPlaying;
use cinder_ui::{Canvas, FontSet, H, W};
use std::ffi::c_char;
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// pub(crate): gpu.rs pokes the panel with the same ioctls after eglSwapBuffers (see gpu::PanelPoke).
// Shared rather than re-declared there — a second copy of these numbers and of VarInfo's layout is
// exactly the kind of duplicate that drifts.
pub(crate) const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
pub(crate) const FBIOPUT_VSCREENINFO: libc::Ioctl = 0x4601;
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;
/// fb_var_screeninfo.activate flag: force the driver to (re)apply the mode NOW. On mtkfb this is
/// what actually pushes the framebuffer to the panel — writing pixels into the mmap does NOTHING
/// on its own. icx_bootanimation's per-frame "flip" (disasm @0x1fae) is exactly
/// `var.activate |= 0x80; ioctl(fd, FBIOPUT_VSCREENINFO, &var)`; without it the glass keeps showing
/// whatever was pushed last, forever (the "frozen boot image" failure mode).
pub(crate) const FB_ACTIVATE_FORCE: u32 = 0x80;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub(crate) struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub(crate) struct VarInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct FixInfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}
impl Default for FixInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

/// /dev/graphics/fb0 mapping. `base` is held as usize so the struct is Send (we only
/// ever touch it under the global Mutex).
struct Framebuffer {
    _file: File,
    fd: libc::c_int,
    var: VarInfo, // kept for the per-blit flip ioctl (offsets pinned to 0)
    base: usize,
    stride: usize,
    pages: usize,
    map_len: usize,
}

impl Framebuffer {
    fn open() -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/graphics/fb0")
            .map_err(|e| format!("open fb0: {e}"))?;
        let fd = file.as_raw_fd();
        let mut var = VarInfo::default();
        let mut fix = FixInfo::default();
        unsafe {
            libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var as *mut _);
            // Same init sequence as icx_bootanimation: pin the visible window to page 0 and force
            // one mode (re)apply, THEN read the fixed info. This both claims the display for us and
            // guarantees the stride we compute below matches the applied mode.
            var.xoffset = 0;
            var.yoffset = 0;
            var.activate |= FB_ACTIVATE_FORCE;
            libc::ioctl(fd, FBIOPUT_VSCREENINFO, &mut var as *mut _);
            libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix as *mut _);
        }
        let stride = fix.line_length as usize;
        if stride == 0 {
            return Err("fb stride 0".into());
        }
        let map_len = stride * var.yres_virtual as usize;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err("mmap fb0 failed".into());
        }
        let pages = (var.yres_virtual / var.yres.max(1)).max(1) as usize;
        println!(
            "cinder-ffi: fb {}x{} {}bpp stride {} pages {} — flip-on-blit active (FBIOPUT+FORCE)",
            var.xres, var.yres, var.bits_per_pixel, stride, pages
        );
        Ok(Framebuffer { _file: file, fd, var, base: ptr as usize, stride, pages, map_len })
    }

    /// Blit one canvas to every page (the panel is triple-buffered).
    ///
    /// Bullet-proofing: we NEVER write past the mapped region. On the confirmed panel
    /// (480x800, virtual 2400 = 3x800) every row fits exactly, but if a unit/firmware ever reports
    /// a geometry where `pages*H` overruns `yres_virtual` (e.g. yres_virtual not a multiple of H, a
    /// rotated panel, or H > yres), an unchecked `(page*H+y)*stride` would write off the end of the
    /// mmap → SIGSEGV/corruption. So each row is bounded against `map_len`; an out-of-range row is
    /// skipped rather than written. Worst case is a cosmetically clipped frame, never a crash.
    fn blit(&mut self, buf: &[u32]) {
        let base = self.base as *mut u8;
        let copy_bytes = (W * 4).min(self.stride);
        for page in 0..self.pages {
            for y in 0..H {
                let dst_row = (page * H + y) * self.stride;
                if dst_row + copy_bytes > self.map_len || (y + 1) * W > buf.len() {
                    break; // this row (and any after, in this page) would overrun a mapping
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        buf.as_ptr().add(y * W) as *const u8,
                        base.add(dst_row),
                        copy_bytes,
                    );
                }
            }
        }
        // Push the frame to the glass. mtkfb does NOT scan the framebuffer continuously — the
        // panel only updates on this trigger ioctl (icx_bootanimation's flip, replicated exactly).
        // The dirty-flag gate above us means this runs only when a frame actually changed, so the
        // idle cost stays zero. Occasionally the driver blocks >33 ms here (the anim logs it as
        // "heavy ioctl") — harmless at our frame rate.
        self.var.xoffset = 0;
        self.var.yoffset = 0;
        self.var.activate |= FB_ACTIVATE_FORCE;
        let rc = unsafe { libc::ioctl(self.fd, FBIOPUT_VSCREENINFO, &mut self.var as *mut _) };
        if rc != 0 {
            // One-time diagnostic: a failing flip means an invisible UI, which is otherwise
            // indistinguishable from the old frozen-boot-image symptom on device.
            static FLIP_ERR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !FLIP_ERR.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "cinder-ffi: fb flip ioctl FAILED (errno {}) — UI will not reach the panel",
                    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                );
            }
        }
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.base as *mut libc::c_void, self.map_len);
        }
    }
}

/// Frame presentation backend. `Gl` is the GPU path (EGL + GLES2 on the device's Mali fbdev
/// driver — see `gpu.rs`); `Fb` is the original software path (mmap the framebuffer, memcpy each
/// page, force a mode re-apply). `cinder_render_init` prefers `Gl` and falls back to `Fb` if the
/// GPU won't initialise, so the panel always gets pixels.
enum Presenter {
    Gl(gpu::GlPresenter),
    Fb(Framebuffer),
}

impl present::PresentTarget for Presenter {
    fn present(&mut self, buf: &[u32]) {
        match self {
            Presenter::Gl(g) => g.present(buf),
            Presenter::Fb(f) => f.blit(buf),
        }
    }
}

/// Frames whose presentation has COMPLETED (blit + flip ioctl returned / swap + poke returned) —
/// i.e. pixels were pushed toward the glass, not merely queued. The shell reads this via
/// `cinder_frames_presented` to gate its "first frame painted" bad-boot health signal; with the
/// present running on its own thread, "cinder_render_tick returned" no longer implies that.
pub(crate) static FRAMES_PRESENTED: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// How frames leave the render thread: inline (the original serial path, kept as the flagged
/// escape because it is strictly less machinery) or through the present thread (see present.rs).
enum Sink {
    Sync(Presenter),
    Threaded(present::PresentThread),
}

impl Sink {
    fn present(&mut self, canvas: &mut Canvas) {
        use present::PresentTarget;
        match self {
            Sink::Sync(p) => {
                p.present(&canvas.buf);
                FRAMES_PRESENTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Sink::Threaded(t) => t.submit(&mut canvas.buf),
        }
    }

    /// Block until `target` frames have completed (no-op on the sync path, where completion is
    /// implied by `present` returning). Bench uses this to time the true present cost.
    fn wait_presented(&self, target: u64) {
        if let Sink::Threaded(t) = self {
            t.wait_presented(target);
        }
    }
}

/// Owned now-playing state (the FFI setters fill this; render borrows from it).
#[derive(Default)]
struct Np {
    title: String,
    artist: String,
    codec: String,
    badge: String,
    clock: String,
    elapsed: String,
    remaining: String,
    art: String,
    battery: u8,
    progress: f32,
    liked: bool,
    playing: bool,
    shuffle: bool,
    repeat: u8,
}

struct Render {
    /// Which album's cover is currently handed to the UI, so the 96x96 is loaded on change only.
    album_cover_id: Option<i64>,
    present: Sink,
    /// The frame buffer, allocated ONCE and reused every frame. Re-allocating it per frame
    /// is 1.5 MB of churn that fragmented the heap until an allocation failed outright on
    /// device (SIGABRT, 2026-07-26).
    canvas: Canvas,
    fonts: FontSet,
    night: bool,
    np: Np,
    db: Option<cinder_db::Db>,
    app: cinder_ui::nav::App,
    scrob: Option<scrobble::Scrobbler>,
    last_track: Option<cinder_db::Track>, // last resolved track (for scrobble metadata)
    // Now-playing position. Two sources, in priority order:
    //  1. REAL position from PlayerService's PlayEventListener::onPlayTimeUpdated, pushed in via
    //     cinder_set_play_position. It arrives about once a second, so `real_pos_at` records when,
    //     and clock_tick interpolates forward from it — drift-free, and it follows seeks and
    //     mid-track starts, which the estimate below cannot.
    //  2. The local play-clock ESTIMATE (duration from the DB, advance by wall-clock delta while
    //     playing). Used only until the first real update arrives, or if the listener goes quiet.
    play_pos_ms: i64,
    cur_duration_ms: i64,
    last_pos: std::time::Instant, // wall-clock anchor for the position estimate (rate-independent)
    real_pos_ms: i64,             // last position from the service; -1 = none seen yet
    real_pos_at: std::time::Instant, // when it arrived (interpolation anchor)
    // Drag-to-seek: Some(target_ms) while a finger is dragging the progress rail. While set, the
    // bar/labels show this pending target and incoming position updates are ignored, so the bar
    // does not fight the finger. The shell issues the actual SeekTime on release.
    scrub_ms: Option<i64>,
    // Screenshot request: Some(path) => the next rendered frame is also written to `path` as a PNG.
    // Captured from the Canvas BEFORE presentation, so it is identical on the software framebuffer
    // and the GPU/EGL path (under EGL the Mali swapchain owns the panel, so reading /dev/graphics/fb0
    // from outside does NOT reliably show what's on screen — this is the only faithful capture).
    pending_screenshot: Option<String>,
    // Sleep timer: counts DOWN in wall-clock ms (regardless of play/pause); 0 = inactive. When it
    // reaches 0 we raise sleep_fire, which the shell polls (cinder_sleep_should_pause) to pause.
    sleep_remaining_ms: i64,
    sleep_fire: bool,
    // Persisted UI preferences (theme night + visualiser type/on) so choices survive a reboot. The
    // shell points us at a file via cinder_settings_load; we re-save (best-effort) whenever one of
    // them changes. last_saved is the fingerprint we last wrote, to avoid redundant writes.
    settings_path: Option<String>,
    last_saved_body: String, // the file body we last wrote (compare to skip redundant writes)
    // Dirty-flag rendering (battery, goal #1): the pump ticks ~30-60x/s, but re-rendering +
    // blitting the whole framebuffer (~4.6 MB copy) when nothing changed is pure waste. We only
    // repaint when `dirty` is set — by input, a now-playing/theme change, or an active overlay
    // animation. Idle = near-zero CPU.
    dirty: bool,
    // Visualiser animation phase (advanced only while playing AND Now Playing is showing AND the
    // nav's viz is enabled — bounds the repaint cost). The viz TYPE + on/off live in nav (UI state,
    // settable from the Settings screen); cinder-ffi only owns the animation timing.
    viz_phase: f32,
    last_viz: std::time::Instant, // throttle the visualiser repaint to ~20fps (battery)
    viz_levels: Vec<f32>, // real spectrum bars (0..1) from the last set_pcm/set_spectrum; empty = synthetic
    viz_peak: f32,        // slow-decaying auto-gain peak for cinder_set_spectrum's linear branch
    // When the last spectrum frame arrived. Sony's analyzer streams at ~20 Hz WHILE IT RUNS, and it
    // now runs on demand — so it stops on every screen blank, pause and (possibly) track change,
    // and starts again up to a second later (housekeeping is 1 Hz) plus service latency. Without a
    // staleness check the last frame simply STAYS on screen: a held snapshot of a drum hit, which
    // is exactly as untrue as the synthetic animation it replaced, and would be visible on every
    // single screen wake. Frames older than VIZ_FRESH_MS decay to nothing and are then dropped.
    viz_at: std::time::Instant,
    /// The user queue was edited and PlayerService has not been told yet. Flushed at a track
    /// boundary — see `Action::QueueChanged` for why it cannot be flushed immediately.
    queue_pending: bool,
    /// A queue flush is sitting in `pending_play` waiting for the shell to collect it. Reported
    /// through `cinder_take_queue_flush` so the shell knows to hand it to PlayerService.
    queue_flush: bool,
    // Pending play request (Action::PlayIndex resolved through the DB): the chosen track's album
    // context — file URIs in play order + the start index. The shell drains it via
    // cinder_pending_play_* after a CINDER_ACT_PLAY_INDEX action and hands it to PlayerService
    // (NodeTrackSequence). Replaced wholesale on every new PlayIndex.
    pending_play: Vec<String>,
    // ── Liked songs ────────────────────────────────────────────────────────────────────────
    // Track object_ids the user has hearted. Kept as a set so the Now Playing heart is an O(log n)
    // lookup per track change, and persisted to its own file rather than the settings blob — it
    // grows with the library, and losing every preference because one liked-list line is corrupt
    // would be a bad trade. `liked_path` is None until cinder_db_open supplies it.
    duration_checked: bool,         // have we compared the DB duration against the service's yet?
    last_tick: std::time::Instant,  // real-time anchor for fling/HUD animation
    last_scrob: std::time::Instant, // real-time anchor for the scrobble play clock
    liked: std::collections::BTreeSet<i64>,
    liked_path: Option<String>,
    pending_play_start: usize,
    // Decoded album cover for the CURRENT track, pre-scaled to the two draw sizes (480 full-bleed,
    // 92 thumb). art_key = the object_id we last decoded for (skip re-decode on same-track polls);
    // None images = no art found → the UI draws its gradient fallback.
    art_full: Option<cinder_ui::art::Image>,
    art_thumb: Option<cinder_ui::art::Image>,
    art_key: Option<i64>,
    /// Path the library DB was opened from, so the cover decoder can open its own read-only
    /// handle instead of borrowing this one across a thread (same reasoning as start_art_cache).
    db_path: Option<String>,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Current wall-clock time as "HH:MM" in LOCAL time (libc localtime_r respects the device TZ).
/// Empty on failure. (Y2038 caveat: on glibc-2.23 32-bit time_t this breaks in 2038 — the
/// device-wide issue, see project goals.)
fn current_hhmm() -> String {
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}

static R: OnceLock<Mutex<Option<Render>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Render>> {
    R.get_or_init(|| Mutex::new(None))
}

/// Read a C string into an owned String (empty on null/invalid).
unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// Open the requested presenter, falling back from GPU to the software framebuffer. Runs on the
/// present thread in the default configuration (EGL thread affinity), inline under
/// /contents/cinder_nothread. Always names the live path in cinderhome.log: "the app ran but the
/// screen was stuck" is otherwise indistinguishable between these branches from the log alone.
fn open_presenter(want_gpu: bool) -> Result<Presenter, String> {
    if want_gpu {
        match gpu::GlPresenter::open(W as i32, H as i32) {
            Ok(g) => {
                println!("cinder-ffi: GPU present path active (EGL/GLES2 on Mali)");
                return Ok(Presenter::Gl(g));
            }
            Err(e) => {
                eprintln!("cinder-ffi: GPU init failed ({e}); falling back to software framebuffer")
            }
        }
    } else {
        println!("cinder-ffi: software framebuffer present path (GPU opt-in flag absent)");
    }
    Framebuffer::open().map(Presenter::Fb)
}

/// Open the framebuffer and initialise the renderer. Returns 0 on success, <0 on error.
#[no_mangle]
pub extern "C" fn cinder_render_init() -> libc::c_int {
    // First thing, before anything can panic: a hook that says WHERE in the UI it happened.
    // Idempotent in practice — render_init runs once per process.
    install_panic_hook();
    // GPU present path (EGL/GLES2 on Mali) is OPT-IN. It was briefly made the default on
    // 2026-07-26 and that flip is what wedged the two flashes that evening: the app booted
    // perfectly (deferred_up: DONE, "healthy: bad-boot counter cleared", no crash in the log)
    // while the panel still showed the boot animation. eglSwapBuffers returns success on this
    // fbdev build whether or not the compositor ever scans the buffer out, so a GPU present that
    // reaches no pixels is INVISIBLE to us — and worse, invisible to the bad-boot counter, which
    // this process clears on "a frame was rendered". Frozen glass therefore also disabled rung 1
    // of the escape ladder; only the cable escape (rung 0) got the device back.
    //   The software framebuffer does not have that hole: Framebuffer::blit ends in an explicit
    // FBIOPUT_VSCREENINFO(FB_ACTIVATE_FORCE), which is the only thing that makes mtkfb push pixels
    // to the panel, and it reports failure.
    //   So the proven path is the default and the unproven one costs a deliberate flag file. The
    // flag lives on /contents, which is reachable over USB-MSC from a stock boot — deleting it
    // needs strictly less than the app it rescues, per the escape-ladder rule.
    //   Enable:  /contents/cinder_gpu_on   (or CINDER_GPU=1)
    //   Disable: delete that file          (/contents/cinder_gpu_off and CINDER_GPU=0 also win)
    let force_off = std::path::Path::new("/contents/cinder_gpu_off").exists()
        || std::env::var("CINDER_GPU").map(|v| v == "0").unwrap_or(false);
    let opt_in = std::path::Path::new("/contents/cinder_gpu_on").exists()
        || std::env::var("CINDER_GPU").map(|v| v == "1").unwrap_or(false);
    let want_gpu = opt_in && !force_off;
    // The present runs on its own thread by default (raster and present overlap — see present.rs,
    // incl. why the watchdog contract survives the move). /contents/cinder_nothread or
    // CINDER_NOTHREAD=1 keeps the original in-line present: the escape depends on strictly less.
    let no_thread = std::path::Path::new("/contents/cinder_nothread").exists()
        || std::env::var("CINDER_NOTHREAD").map(|v| v == "1").unwrap_or(false);
    let present = if no_thread {
        println!("cinder-ffi: synchronous present (present thread disabled by flag)");
        match open_presenter(want_gpu) {
            Ok(p) => Sink::Sync(p),
            Err(e) => {
                eprintln!("cinder-ffi: {e}");
                return -1;
            }
        }
    } else {
        // The presenter is constructed ON the present thread (EGL contexts are thread-affine).
        match present::PresentThread::start(move || open_presenter(want_gpu)) {
            Ok(t) => {
                println!("cinder-ffi: present thread active (raster overlaps present)");
                Sink::Threaded(t)
            }
            Err(e) => {
                eprintln!("cinder-ffi: {e}");
                return -1;
            }
        }
    };
    let mut np = Np::default();
    np.codec = "—".into();
    np.battery = 100;
    *cell().lock().unwrap() = Some(Render {
        album_cover_id: None,
        present,
        canvas: Canvas::new(),
        fonts: FontSet::load(),
        night: false,
        np,
        db: None,
        app: cinder_ui::nav::App::unlocked(),
        scrob: None,
        last_track: None,
        play_pos_ms: 0,
        cur_duration_ms: 0,
        last_pos: std::time::Instant::now(),
        real_pos_ms: -1,
        real_pos_at: std::time::Instant::now(),
        scrub_ms: None,
        pending_screenshot: None,
        sleep_remaining_ms: 0,
        sleep_fire: false,
        settings_path: None,
        last_saved_body: String::new(),
        dirty: true, // paint the first frame
        viz_phase: 2.0,
        last_viz: std::time::Instant::now(),
        viz_levels: Vec::new(),
        viz_peak: 0.0,
        viz_at: std::time::Instant::now(),
        queue_pending: false,
        queue_flush: false,
        pending_play: Vec::new(),
        duration_checked: false,
        last_tick: std::time::Instant::now(),
        last_scrob: std::time::Instant::now(),
        liked: std::collections::BTreeSet::new(),
        liked_path: None,
        pending_play_start: 0,
        art_full: None,
        art_thumb: None,
        art_key: None,
        db_path: None,
    });
    0
}

/// Format milliseconds as `M:SS` (or `H:MM:SS`). `duration_raw` units are assumed ms —
/// calibrate on device (see cinder-db notes); only this one place needs changing if not ms.
fn fmt_time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Derive (codec line, status-bar badge) from the file extension + hi-res flag.
/// Bit-depth/sample-rate aren't in cinder-db yet (they're extra MediaStore ext props) —
/// extend here once those props are read; until then we show the container + a Hi-Res mark.
fn codec_label(filename: &str, is_hires: bool) -> (String, String) {
    let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_uppercase();
    let ext = if ext.is_empty() || ext.len() > 4 { "PCM".to_string() } else { ext };
    if is_hires {
        (format!("{ext} · Hi-Res"), format!("{ext} HR"))
    } else {
        (ext.clone(), ext)
    }
}

/// Fill the now-playing string fields from a resolved library Track + playback progress.
// Track metadata only (title/artist/codec/badge). Position is set separately by set_progress so the
// local play-clock can advance it each second.
fn apply_track(np: &mut Np, t: &cinder_db::Track) {
    np.title = if t.title.is_empty() {
        t.filename.rsplit('/').next().unwrap_or("").to_string()
    } else {
        t.title.clone()
    };
    np.artist = t.artist.clone();
    let (codec, badge) = codec_label(&t.filename, t.is_hires);
    np.codec = codec;
    np.badge = badge;
}

// Serialise the persisted UI preferences (theme + visualiser + EQ + sound effects) to the file body.
fn settings_body(r: &Render) -> String {
    let eq: Vec<String> = r.app.eq_bands().iter().map(|b| b.to_string()).collect();
    format!(
        "night={}\naccent={}\nviz_kind={}\nviz_size={}\nnp_page={}\nshuffle={}\nrepeat={}\neq={}\nsound={}\nonboarding={}\nbt_codec={}\nbt_ldac_quality={}\nvolume={}\nbt_volume={}\nbrightness={}\nscreen_off={}\n",
        r.app.night as u8,
        r.app.accent(),
        r.app.viz_kind(),
        r.app.viz_size(),
        r.app.np_page(),
        r.np.shuffle as u8,
        r.np.repeat,
        eq.join(","),
        r.app.sound_flags(),
        r.app.onboarding_seen() as u8,
        r.app.bt_codec(),
        r.app.bt_ldac_quality(),
        r.app.volume_level(),
        r.app.bt_volume_level(),
        r.app.brightness(),
        r.app.screen_off_s(),
    )
}

// Write the preferences to the configured file IF they changed since the last write (cheap body
// compare → most presses don't write). Best-effort: IO errors (RO/full fs) are ignored.
fn save_settings(r: &mut Render) {
    if r.settings_path.is_none() {
        return;
    }
    let body = settings_body(r);
    if body == r.last_saved_body {
        return;
    }
    if let Some(path) = r.settings_path.clone() {
        let _ = std::fs::write(&path, &body);
        r.last_saved_body = body;
    }
}

// Set the progress bar + elapsed/remaining from a position (ms) and duration (ms). Duration 0
// (unknown) → an empty/zero bar rather than a misleading one.
fn set_progress(np: &mut Np, pos_ms: i64, dur_ms: i64) {
    let pos = pos_ms.clamp(0, dur_ms.max(0));
    np.elapsed = fmt_time(pos);
    if dur_ms > 0 {
        np.remaining = format!("-{}", fmt_time(dur_ms - pos));
        np.progress = (pos as f32 / dur_ms as f32).clamp(0.0, 1.0);
    } else {
        np.remaining = String::new();
        np.progress = 0.0;
    }
}

/// Build the browsable `Library` view-model from the library DB in a single pass over the
/// tracks (+ the album list for accurate counts/order). Albums are grouped by artist; the
/// Songs tab gets every track; artists are derived with album/track counts. Playlists aren't
/// in cinder-db yet, so that tab is empty for now. Art keys use the album/title so each item
/// gets a distinct hashed gradient until real thumbnails are decoded.
/// Stand-in album id for a row that has no album behind it. Deliberately not 0 — 0 is a perfectly
/// valid SQLite primary key, and using it would make an artless row collide with a real album's
/// cached cover.
const NO_ALBUM_ID: i64 = i64::MIN;

fn build_library(db: &cinder_db::Db) -> cinder_ui::Library {
    use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, SongRow};
    use std::collections::BTreeMap;

    // Resolve release-year FKs once (best-effort; empty map if the table shape differs — years
    // then stay blank, exactly as before). Shared by the song-row builder + the album year label.
    let years = db.release_years();
    let year_num = |id: Option<i64>| -> i32 {
        id.and_then(|i| years.get(&i)).and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0)
    };

    // Only the release YEAR needs the FK map; everything else is on the track itself, so the
    // shared builder covers it and this closure just fills the one field in.
    let song_row = |t: &cinder_db::Track| SongRow {
        year: year_num(t.releaseyear_id),
        ..song_row_of(t)
    };

    let tracks = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
    let mut album_artist: BTreeMap<i64, String> = BTreeMap::new();
    // Per artist: their distinct albums as name → album id, plus a track count.
    //
    // The name and the id have to live in ONE ordered structure. They used to be a sorted
    // `BTreeSet<String>` of names beside an insertion-ordered `Vec<i64>` of ids, with a comment
    // claiming the two lined up — they did not, and could not: one was alphabetical and the other
    // was DB row order. The Artists tab draws the cover by id and the gradient by name, so 78 rows
    // on the test library showed one album's artwork labelled with another album's colours.
    let mut artist_albums: BTreeMap<String, (BTreeMap<String, i64>, u32)> = BTreeMap::new();
    let mut songs = Vec::with_capacity(tracks.len());
    // ALBUM ARTIST is what browsing groups by — it is the default, and the track artist is only a
    // fallback for files that carry no album artist at all. Grouping by the track artist shatters
    // compilations: on this library 24 albums span several track artists and one DJ mix spans 26,
    // so it showed up as 26 one-track albums under 26 different people. By album artist, ZERO
    // albums split. (The Songs tab still shows the TRACK artist — that is the right thing on a
    // song row, and it is where a featured guest belongs.)
    //
    // The old code also took the album's artist from whichever track sorted FIRST BY TITLE, which
    // for a compilation is simply an arbitrary pick.
    let group_artist = |t: &cinder_db::Track| -> String {
        if t.album_artist.trim().is_empty() { t.artist.clone() } else { t.album_artist.clone() }
    };
    for t in &tracks {
        if let Some(aid) = t.album_id {
            album_artist.entry(aid).or_insert_with(|| group_artist(t));
        }
        songs.push(song_row(t));
        let e = artist_albums.entry(group_artist(t)).or_default();
        if !t.album.is_empty() {
            // Keyed by name (that is what the album COUNT has always meant here). The id is
            // whichever track first supplies one — a track with no album_id leaves the slot at
            // NO_ALBUM_ID, which simply misses the thumbnail cache and draws the gradient.
            let slot = e.0.entry(t.album.clone()).or_insert(NO_ALBUM_ID);
            if *slot == NO_ALBUM_ID {
                if let Some(aid) = t.album_id {
                    *slot = aid;
                }
            }
        }
        e.1 += 1;
    }

    // Per-album track lists keyed by album ID (names can collide across artists), in the DB's
    // disc/track order — one query, grouped in a single pass. This is what the Album screen
    // shows under the header. Alongside, derive each album's "recently added" key (newest track
    // addedtime) and its release-year label (first track that resolves one) for the ORDER chip.
    let mut album_tracks: BTreeMap<i64, Vec<SongRow>> = BTreeMap::new();
    let mut album_added: BTreeMap<i64, i64> = BTreeMap::new();
    let mut album_year: BTreeMap<i64, String> = BTreeMap::new();
    for t in db.tracks_album_order().unwrap_or_default() {
        if let Some(aid) = t.album_id {
            let a = album_added.entry(aid).or_insert(0);
            if t.added > *a {
                *a = t.added;
            }
            if !album_year.contains_key(&aid) {
                if let Some(y) = t.releaseyear_id.and_then(|i| years.get(&i)) {
                    if !y.is_empty() {
                        album_year.insert(aid, y.clone());
                    }
                }
            }
            album_tracks.entry(aid).or_default().push(song_row(&t));
        }
    }

    // Album list (ordered, with track counts) → rows, grouped by artist.
    let mut album_rows: Vec<AlbumRow> = db
        .albums()
        .unwrap_or_default()
        .into_iter()
        .map(|a| AlbumRow {
            artist: album_artist.get(&a.id).cloned().unwrap_or_default(),
            year: album_year.get(&a.id).cloned().unwrap_or_default(),
            tracks: a.track_count.max(0) as u32,
            art: a.name.clone(),
            added: album_added.get(&a.id).copied().unwrap_or(0),
            track_list: album_tracks.remove(&a.id).unwrap_or_default(),
            name: a.name,
            album_id: a.id,
        })
        .collect();
    album_rows.sort_by(|x, y| x.artist.cmp(&y.artist).then_with(|| x.name.cmp(&y.name)));
    let mut album_groups: Vec<ArtistGroup> = Vec::new();
    for ar in album_rows {
        match album_groups.last_mut() {
            Some(g) if g.artist == ar.artist => g.albums.push(ar),
            _ => album_groups.push(ArtistGroup { artist: ar.artist.clone(), albums: vec![ar] }),
        }
    }

    let mut artists: Vec<ArtistRow> = artist_albums
        .into_iter()
        .filter(|(n, _)| !n.is_empty())
        .map(|(name, (albs, tr))| {
            // The cover stack draws at most two. Both arrays are taken from the SAME iterator in
            // the SAME order, so `arts[i]` and `album_ids[i]` are guaranteed to be one album.
            let (arts, album_ids): (Vec<String>, Vec<i64>) = if albs.is_empty() {
                (vec![name.clone()], vec![NO_ALBUM_ID])
            } else {
                albs.iter().take(2).map(|(n, id)| (n.clone(), *id)).unzip()
            };
            ArtistRow { albums: albs.len() as u32, tracks: tr, arts, album_ids, name }
        })
        .collect();
    artists.sort_by(|a, b| a.name.cmp(&b.name));

    // Playlists: real ones from the DB (Sony keeps them as containers in a second object tree —
    // see Db::playlists). Empty-but-honest if the DB has none, rather than sample data.
    let playlists = db
        .playlists()
        .unwrap_or_default()
        .into_iter()
        .map(|p| cinder_ui::model::PlaylistRow {
            id: p.id,
            name: p.name.clone(),
            tracks: p.track_count.max(0) as u32,
            // Members in the saved order, resolved once here so the drill-in page never touches
            // the DB. One query per playlist, and there are a handful of them — the same shape as
            // the per-album track lists above, which cost one query for the entire library.
            track_list: db
                .playlist_tracks(p.id)
                .unwrap_or_default()
                .iter()
                .map(&song_row)
                .collect(),
            // No cover of its own: hash the name so each playlist still gets distinct art,
            // the same fallback the album rows use.
            art: p.name,
        })
        .collect();

    // `thumbs` is filled separately by start_art_cache: the disk cache load is I/O, not model
    // building, and the rest arrives asynchronously from the decoder thread.
    cinder_ui::Library { songs, album_groups, artists, playlists, thumbs: Default::default() }
}

// ── Panic context ────────────────────────────────────────────────────────────────────────────
// `panic = "abort"` means a Rust panic kills the process, appmgr calls android_reboot, and the
// bad-boot counter takes a life. The panic message itself does reach cinderhome.log through the
// launcher's stderr redirect — but "panicked at lib.rs:1234" says nothing about what the user was
// doing, and on a device whose only symptom is "it rebooted", that is most of the diagnosis.
//
// The hook must not touch the renderer mutex: a panic raised while that lock is held would then
// deadlock instead of aborting, turning a clean reboot into a hang. So the context it prints is
// kept in plain atomics, updated once a frame, and the hook only ever reads them.
static PANIC_SCREEN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);
static PANIC_PAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static PANIC_TRACK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Screen names for the panic line, indexed by `screen_ord`. Static strings only — the hook
/// allocates nothing it does not have to.
const SCREEN_NAMES: [&str; 18] = [
    "Lock", "NowPlaying", "Menu", "Library", "Album", "Artist", "Playlist", "UpNext", "Eq",
    "Sound", "Bluetooth", "Settings", "Fm", "UsbDac", "Receiver", "Onboarding", "UsbStorage",
    "Shelf",
];

/// Exhaustive on purpose: adding a `Screen` variant without a name here fails the build rather
/// than silently printing the wrong screen in a crash report.
fn screen_ord(s: cinder_ui::nav::Screen) -> u8 {
    use cinder_ui::nav::Screen as S;
    match s {
        S::Lock => 0, S::NowPlaying => 1, S::Menu => 2, S::Library => 3, S::Album => 4,
        S::Artist => 5, S::Playlist => 6, S::UpNext => 7, S::Eq => 8, S::Sound => 9,
        S::Bluetooth => 10, S::Settings => 11, S::Fm => 12, S::UsbDac => 13, S::Receiver => 14,
        S::Onboarding => 15, S::UsbStorage => 16, S::Shelf => 17,
    }
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::sync::atomic::Ordering::Relaxed;
        let name = SCREEN_NAMES
            .get(PANIC_SCREEN.load(Relaxed) as usize)
            .copied()
            .unwrap_or("<pre-init>");
        eprintln!(
            "[cinder-ffi] PANIC — screen={} np_page={} track_id={} frames_presented={}",
            name,
            PANIC_PAGE.load(Relaxed),
            PANIC_TRACK.load(Relaxed),
            FRAMES_PRESENTED.load(Relaxed),
        );
        prev(info); // then the standard message + location, which is the other half of the story
    }));
}

/// A spectrum frame is "current" for this long. Sony's analyzer is asked for 20 Hz, so a live
/// stream delivers one every ~50 ms; 250 ms is five missed frames, comfortably past jitter but
/// short enough that a stopped stream is caught within a frame or two of the user noticing.
const VIZ_FRESH_MS: u128 = 250;
/// How long the bars take to fall from full to nothing once the stream has stopped. They decay
/// rather than vanishing: bars dropping away reads as "the music stopped", bars blinking out reads
/// as the UI breaking.
const VIZ_DECAY_MS: f32 = 400.0;

/// Age out the spectrum. Returns true if anything moved (so the caller repaints).
///
/// This is what stops a stale frame being displayed as if it were live. It runs unconditionally —
/// including while the visualiser is off screen — so that coming back to Now Playing can never
/// show bars left over from the last time it was open.
/// Sentinel `art_key` meaning "the current URI resolved to no library track". Distinct from every
/// real object_id, so the baked fallback below is built once for that state rather than on every
/// poll of the same unresolved URI.
const ART_KEY_UNRESOLVED: i64 = i64::MIN;

/// Bake the gradient fallback into `art_full`/`art_thumb` when there is no decoded cover, so the
/// render blits it instead of recomputing it.
///
/// The gradient costs real per-pixel work — a ramp lookup, a squared-distance test, and a `sqrt`
/// inside the highlight disc. At 480×480 that measured ~3.3 ms a frame on the host even after
/// being optimised (~8 ms before), and it was paid on EVERY frame, which while the visualiser
/// animates means 20 times a second, for a picture that only changes when the track does.
///
/// NEITHER bake depends on the live theme, so a Day/Night switch never has to rebuild them: the
/// full-bleed one is drawn at opacity 1.0, where the gradient maths never reads `t.bg` at all, and
/// the 92px thumb — which is blended toward the background — is only ever drawn by the night
/// layout, so it is baked against the night palette by construction.
fn bake_gradient_art(r: &mut Render) {
    if r.art_full.is_some() {
        return;
    }
    let day = cinder_ui::Theme::day();
    let night = cinder_ui::Theme::night();
    r.art_full = Some(cinder_ui::art::gradient_image(&day, 480, 480, &r.np.art, 1.0));
    r.art_thumb = Some(cinder_ui::art::gradient_image(&night, 92, 92, &r.np.art, 0.32));
}

fn viz_decay(r: &mut Render, dt_ms: u32) -> bool {
    // Empty check FIRST: this runs on every frame for the whole life of the process, and the
    // overwhelmingly common state (no analyzer running) has nothing to decay. `Instant::elapsed`
    // is a clock_gettime, so testing it first would buy a syscall 60 times a second to learn that
    // there is no work.
    if r.viz_levels.is_empty() || r.viz_at.elapsed().as_millis() <= VIZ_FRESH_MS {
        return false;
    }
    viz_decay_levels(&mut r.viz_levels, dt_ms)
}

/// The decay itself, split out so it can be tested without building a whole `Render`. Steps every
/// bar down and clears the buffer once they are all at zero.
fn viz_decay_levels(levels: &mut Vec<f32>, dt_ms: u32) -> bool {
    if levels.is_empty() {
        return false;
    }
    // Clamped: a long stall (screen off for an hour) must collapse the bars, not wrap or spike.
    let step = (dt_ms.min(1000) as f32 / VIZ_DECAY_MS).clamp(0.0, 1.0);
    let mut any = false;
    for v in levels.iter_mut() {
        if *v > 0.0 {
            *v = (*v - step).max(0.0);
            any = true;
        }
    }
    if levels.iter().all(|v| *v <= 0.0) {
        // Fully faded: drop the buffer so `viz_levels` is None again and the visualiser is simply
        // absent, which is the honest state when no analyzer is feeding it.
        levels.clear();
        any = true;
    }
    any
}

/// Render the current state to the panel (call once per frame from the pump). No-op when
/// nothing has changed (dirty-flag rendering) — that keeps the device idle at near-zero CPU
/// instead of re-blitting ~4.6 MB every tick (battery, goal #1).
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return };
    // An active overlay (volume HUD) animates, so it keeps us dirty until it fades.
    // Real elapsed time since the last tick, so the fling and the HUD countdowns run at the same
    // wall-clock speed whatever the frame rate happens to be (a scrolling frame measures ~31 ms on
    // device, not the 16.7 ms the constants were written against).
    let now = std::time::Instant::now();
    let dt_ms = now.saturating_duration_since(r.last_tick).as_millis() as u32;
    r.last_tick = now;
    if r.app.tick_dt(dt_ms) {
        r.dirty = true;
    }
    {
        // Panic context, refreshed once a frame. Three relaxed stores — no ordering is needed
        // because nothing reads them except a hook that is already past the point of no return.
        use std::sync::atomic::Ordering::Relaxed;
        PANIC_SCREEN.store(screen_ord(r.app.current()), Relaxed);
        PANIC_PAGE.store(r.app.np_page(), Relaxed);
        PANIC_TRACK.store(r.last_track.as_ref().map_or(0, |t| t.object_id as u64), Relaxed);
    }
    if viz_decay(r, dt_ms) {
        r.dirty = true;
    }
    // Visualiser: advance + force a repaint ONLY while playing on the Now Playing screen (and
    // enabled), and at most ~20 fps (the pump may tick at 60) — that bounds the battery cost.
    let animate = r.app.wants_spectrum() && r.np.playing && r.app.is_now_playing();
    if animate {
        let since = r.last_viz.elapsed().as_millis() as f32;
        if since >= 50.0 {
            // Advance by the REAL time that passed, not a flat step. The 50 ms gate caps the repaint
            // rate at ~20 fps, but ticks arrive whenever the frame loop gets round to it — 50 ms on
            // an idle screen, ~31 ms-aligned to 62 ms while a list is scrolling — so a fixed +0.18
            // made the visualiser drift slower exactly when the device was busy. Clamped so a stall
            // can't fast-forward the animation on the frame after it.
            r.viz_phase += 0.18 * (since.min(250.0) / 50.0);
            r.last_viz = std::time::Instant::now();
            r.dirty = true;
        }
    }
    if !r.dirty {
        return; // nothing changed — skip the render + framebuffer blit entirely
    }
    // Reuse the frame buffer. This used to be a fresh `Canvas::new()` EVERY painted frame — a
    // 480×800×4 = 1,536,000-byte allocation at up to 60 fps. On device that eventually failed
    // outright ("memory allocation of 1536000 bytes failed" → Rust's allocator aborts → SIGABRT
    // → reboot), because the churn fragments a heap that also holds the Mali/EGL surfaces, the
    // 3350-track library and the decoded cover art. Every screen's render begins with
    // `c.fill(theme.bg)`, so the previous frame's pixels are always fully overwritten; only the
    // clip band has to be reset.
    // Album drill-in cover: load the 96x96 out of the art cache when the open album changes.
    // Polled here rather than pushed from nav because the cache lives on this side; it is one
    // 27 KB read off ext4, only on a change, so it costs nothing per frame.
    let open_album = r.app.open_album_id();
    if open_album != r.album_cover_id {
        r.album_cover_id = open_album;
        r.app.set_album_cover(open_album.and_then(|id| art_cache::load(id, art_cache::T96)));
    }
    r.canvas.clear_clip();
    let np = NowPlaying {
        title: &r.np.title,
        artist: &r.np.artist,
        codec: &r.np.codec,
        badge: &r.np.badge,
        clock: &r.np.clock,
        battery: r.np.battery,
        elapsed: &r.np.elapsed,
        remaining: &r.np.remaining,
        progress: r.np.progress,
        art: &r.np.art,
        art_full: r.art_full.as_ref(),
        art_thumb: r.art_thumb.as_ref(),
        liked: r.np.liked,
        playing: r.np.playing,
        shuffle: r.np.shuffle,
        repeat: r.np.repeat,
        viz_seed: if animate { r.viz_phase } else { 2.0 },
        viz_kind: 0, // nav injects the real viz type on the NowPlaying render
        viz_size: r.app.viz_size(), // nav re-injects this too; kept honest here
        page: r.app.np_page(),
        // real FFT spectrum if the shell is feeding PCM AND we're animating; else None (synthetic)
        viz_levels: if animate && !r.viz_levels.is_empty() { Some(&r.viz_levels) } else { None },
        scrubbing: r.scrub_ms.is_some(),
    };
    // The navigator decides which screen is showing; it draws Now Playing from `np` and
    // the list/menu screens from their own state.
    r.app.render(&mut r.canvas, &r.fonts, &np);
    if let Some(path) = r.pending_screenshot.take() {
        match write_png(&path, &r.canvas) {
            Ok(()) => println!("cinder-ffi: screenshot written to {path}"),
            Err(e) => eprintln!("cinder-ffi: screenshot failed ({path}): {e}"),
        }
    }
    r.present.present(&mut r.canvas);
    r.dirty = false;
}

/// Frames whose presentation has COMPLETED — pixels pushed to the panel, not merely submitted to
/// the present thread. The shell gates "first frame painted" (the bad-boot health signal) on this
/// going nonzero; see FRAMES_PRESENTED for why submission alone must not count.
#[no_mangle]
pub extern "C" fn cinder_frames_presented() -> libc::c_ulonglong {
    FRAMES_PRESENTED.load(std::sync::atomic::Ordering::SeqCst) as libc::c_ulonglong
}

/// Frame-time bench: render `frames` frames and report where the time goes, split into the
/// software rasterize (cinder-ui drawing the whole 480×800 canvas) and the present (memcpy to the
/// framebuffer pages + flip, or texture upload + swap on the GPU path).
///
/// Exists because "scrolling is choppy" has at least three unrelated candidate causes — a slow
/// rasterizer, a slow present, or a render loop that isn't repainting often enough — and they
/// need completely different fixes. Called from `cinder-probe --bench`, which runs standalone: no
/// easel lifecycle, no boot risk, no flash needed to measure.
///
/// `scroll != 0` drives the library list past `frames` pixels while it measures, so the numbers
/// describe the case the user is actually complaining about rather than a static screen.
#[no_mangle]
pub extern "C" fn cinder_render_bench(frames: libc::c_int, scroll: libc::c_int) {
    let frames = frames.max(1) as usize;
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else {
        eprintln!("cinder-ffi: bench: renderer not initialised");
        return;
    };
    // Bench the LIBRARY, not whatever screen happened to be up: that's the screen the choppiness
    // report is about, and it's the most expensive one to rasterize (rows of text + art blocks).
    // Driven through the same taps a finger would use, so no diagnostic-only nav API is needed.
    r.app.tap(344, 22); // status strip → Menu
    r.app.tap(200, cinder_ui::chrome::HEADER_BOTTOM + 63 + 8); // the Library row
    let np = Np::default();
    let mut raster = Vec::with_capacity(frames);
    let mut present = Vec::with_capacity(frames);
    for i in 0..frames {
        if scroll != 0 {
            // One list row every few frames, wrapping — a realistic drag, not a teleport.
            r.app.scroll_px(scroll);
            if i % 120 == 119 {
                r.app.scroll_px(-scroll * 120); // wrap back so a long run stays on real rows
            }
        }
        let np2 = NowPlaying {
            title: &np.title, artist: &np.artist, codec: &np.codec, badge: &np.badge,
            clock: &np.clock, battery: np.battery, elapsed: &np.elapsed, remaining: &np.remaining,
            progress: np.progress, art: &np.art, art_full: None, art_thumb: None,
            liked: np.liked, playing: np.playing, shuffle: np.shuffle, repeat: np.repeat,
            viz_seed: 2.0, viz_kind: 0, viz_size: 0, page: 0, viz_levels: None, scrubbing: false,
        };
        r.canvas.clear_clip();
        let t0 = std::time::Instant::now();
        r.app.render(&mut r.canvas, &r.fonts, &np2);
        let t1 = std::time::Instant::now();
        // Submit AND wait for completion, so "present" is the true cost through the present
        // thread (submit alone returns in microseconds and would time nothing).
        let target = FRAMES_PRESENTED.load(std::sync::atomic::Ordering::SeqCst) + 1;
        r.present.present(&mut r.canvas);
        r.present.wait_presented(target);
        let t2 = std::time::Instant::now();
        raster.push(t1.duration_since(t0).as_micros() as u64);
        present.push(t2.duration_since(t1).as_micros() as u64);
    }
    // What the pump actually achieves with the present thread: raster and present overlap, so a
    // frame costs max(raster, present), not their sum. (Measured serially above on purpose — the
    // split still says WHERE time goes; this line says what it adds up to in production.)
    let pipelined: u64 =
        raster.iter().zip(present.iter()).map(|(a, b)| *a.max(b)).sum();
    let threaded = matches!(r.present, Sink::Threaded(_));
    let report = |name: &str, v: &mut Vec<u64>| {
        // A zero-frame bench would index an empty vector three times below. This runs only from
        // cinder-probe (no easel lifecycle, so it cannot cost a boot), but an aborted diagnostic
        // is still a diagnostic you don't get.
        if v.is_empty() {
            println!("cinder-ffi: bench {name:8} no samples");
            return;
        }
        v.sort_unstable();
        let sum: u64 = v.iter().sum();
        println!(
            "cinder-ffi: bench {name:8} avg {:6.2} ms   median {:6.2}   p95 {:6.2}   max {:6.2}",
            sum as f64 / v.len() as f64 / 1000.0,
            v[v.len() / 2] as f64 / 1000.0,
            v[v.len() * 95 / 100] as f64 / 1000.0,
            v[v.len() - 1] as f64 / 1000.0,
        );
    };
    let total: u64 = raster.iter().sum::<u64>() + present.iter().sum::<u64>();
    report("raster", &mut raster);
    report("present", &mut present);
    println!(
        "cinder-ffi: bench serial  avg {:6.2} ms/frame  =>  {:5.1} fps ceiling",
        total as f64 / frames as f64 / 1000.0,
        1e6 * frames as f64 / total as f64,
    );
    if threaded {
        println!(
            "cinder-ffi: bench PIPELINED avg {:6.2} ms/frame  =>  {:5.1} fps (present thread)",
            pipelined as f64 / frames as f64 / 1000.0,
            1e6 * frames as f64 / pipelined as f64,
        );
    }
}

/// Diagnostic: resolve + decode album art for `object_id` and report what happened. Prints the
/// `images`-row shape and the decoded size, or the reason it stopped. `cinder-probe --art`.
///
/// Worth its own entry point because the art path is otherwise only exercised on a track change,
/// so on a device that has not played anything the log says nothing at all about it.
#[no_mangle]
pub extern "C" fn cinder_art_probe(object_id: libc::c_longlong) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -1 };
    let Some(db) = r.db.as_ref() else {
        eprintln!("cinder-ffi: art probe: no DB open");
        return -1;
    };
    let t0 = std::time::Instant::now();
    match art_load::load(db, object_id as i64) {
        Some(img) => {
            let ms = t0.elapsed().as_millis();
            println!("cinder-ffi: art probe obj={object_id}: decoded {}x{} in {ms} ms", img.w, img.h);
            let t1 = std::time::Instant::now();
            let _ = img.scaled_to(92, 92);
            println!("cinder-ffi: art probe: scale to 92x92 took {} ms", t1.elapsed().as_millis());
            0
        }
        None => {
            println!("cinder-ffi: art probe obj={object_id}: NO IMAGE (see the art: line above)");
            1
        }
    }
}

/// Encode the Canvas to a PNG. Canvas is `0x00RRGGBB`; `to_rgb_bytes()` already unpacks it to
/// packed RGB triples, which is exactly PNG's RGB8 layout. Uses the `png` crate already vendored
/// for album-art decode — no new dependency, stays glibc-2.23-clean.
fn write_png(path: &str, canvas: &Canvas) -> Result<(), String> {
    // Write to a temp file then rename, so a host puller can never read a half-written PNG.
    let tmp = format!("{path}.part");
    let file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .map_err(|e| e.to_string())?
        .write_image_data(&canvas.to_rgb_bytes())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Request that the next rendered frame be saved as a PNG at `path` (NUL-terminated C string).
/// Also marks the UI dirty, because the dirty-flag renderer skips identical frames — without this
/// an idle UI would never repaint and the screenshot would never be produced. Returns 0 on accept.
///
/// This is the agent-facing "show me what's on screen" primitive: it captures the Canvas before
/// presentation, so it works identically on the software framebuffer and the GPU/EGL path.
#[no_mangle]
pub extern "C" fn cinder_request_screenshot(path: *const libc::c_char) -> libc::c_int {
    if path.is_null() {
        return -1;
    }
    let p = unsafe { std::ffi::CStr::from_ptr(path) };
    let p = match p.to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return -1,
    };
    match cell().lock().unwrap().as_mut() {
        Some(r) => {
            r.pending_screenshot = Some(p);
            r.dirty = true; // guarantee a repaint even if the UI is otherwise idle
            0
        }
        None => -1,
    }
}

/// Deliver a logical button press to the navigator. `button` is a `cinder_button_t`
/// (see cinder.h). Theme changes are applied internally; the return value is a
/// `cinder_action_t` the shell should carry out via cinder-audio (0 = nothing).
#[no_mangle]
pub extern "C" fn cinder_input(button: libc::c_int) -> libc::c_int {
    use cinder_ui::nav::Button;
    let b = match button {
        0 => Button::Up,
        1 => Button::Down,
        2 => Button::Left,
        3 => Button::Right,
        4 => Button::Select,
        5 => Button::Back,
        6 => Button::Option,
        7 => Button::Play,
        8 => Button::Home,
        9 => Button::VolUp,
        10 => Button::VolDown,
        11 => Button::Power,
        13 => Button::Next,
        14 => Button::Prev,
        _ => return 0,
    };
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let actions = r.app.press(b);
    r.dirty = true; // a press changes cursor/screen/HUD — repaint next tick
    // Keep the renderer's theme in sync with navigator-driven theme changes.
    r.night = r.app.night;
    // Persist UI preferences (theme/visualiser) if this press changed one (no-op otherwise).
    save_settings(r);
    // Report the first actionable result to the shell (audio/USB are the shell's job).
    for a in &actions {
        if let Some(code) = carry_action(r, a) {
            return code;
        }
    }
    0
}

/// Cheap non-cryptographic RNG (xorshift64*) seeded from the clock. Shuffling a play queue has
/// no security requirement, and this avoids pulling a `rand` dependency into a binary that has to
/// stay glibc-2.23-clean.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        Rng(nanos | 1) // never seed 0: xorshift is stuck there
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Fisher-Yates, unbiased enough for a shuffle queue.
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, (self.next() % (i as u64 + 1)) as usize);
        }
    }
}

/// Apply the Now Playing SHUFFLE toggle to a queue that is about to be handed to PlayerService.
///
/// This is what made the toggle real. It set a flag and lit an icon, and nothing ever read it: with
/// shuffle showing ON you could tap a track and get its album in strict order — the control was
/// telling you something about the next hour of listening that simply was not true.
///
/// The chosen track stays FIRST and everything else is shuffled behind it. You tapped that track;
/// shuffling it away from you would be obeying the toggle and disobeying the tap, and the tap is
/// the more specific instruction. (The Library's own "Shuffle …" bands are different — nothing was
/// chosen there, so those shuffle the whole scope and start at the top. They already worked.)
///
/// Shuffling here rather than asking PlayerService to do it is deliberate: Cinder builds the URI
/// list itself, so the order it hands over IS the play order. Sony's own shuffle lives in a
/// permutation over the sequence's children (`SetupPermutation`), which would mean more ABI surface
/// for a result we can produce exactly by reordering a `Vec`.
fn apply_shuffle(on: bool, mut seq: Vec<cinder_db::Track>, start: usize) -> (Vec<cinder_db::Track>, usize) {
    if !on || seq.len() < 2 || start >= seq.len() {
        return (seq, start);
    }
    let chosen = seq.remove(start);
    Rng::new().shuffle(&mut seq);
    seq.insert(0, chosen);
    (seq, 0)
}

/// One DB track as the owned UI row. Shared by the library build and by every play action, so a
/// track shows the same title/artist/duration wherever it appears. `year` is left 0: it is only a
/// Songs-tab sort key and resolving it needs the release-year FK map, which the library build has
/// and a play action does not.
fn song_row_of(t: &cinder_db::Track) -> cinder_ui::model::SongRow {
    let title = if t.title.is_empty() {
        t.filename.rsplit('/').next().unwrap_or("").to_string()
    } else {
        t.title.clone()
    };
    let art = if t.album.is_empty() { title.clone() } else { t.album.clone() };
    cinder_ui::model::SongRow {
        title,
        artist: t.artist.clone(),
        dur: t.duration_raw.map(fmt_time).unwrap_or_default(),
        art,
        object_id: t.object_id,
        album_id: t.album_id.unwrap_or(0),
        disc: t.disc_no as i32,
        track: t.track_no as i32,
        added: t.added,
        year: 0,
    }
}

/// Hand a resolved play sequence to BOTH consumers: the shell gets the file URIs it feeds to
/// PlayerService, and the UI gets the same tracks as its Up Next queue.
///
/// Filling the queue here is the point. Up Next used to show ONLY the tracks the user had
/// hand-swiped in, and fell back to a guess at the current album when that was empty — so playing
/// an album or hitting a Shuffle band produced a real 200-track sequence that the queue screen
/// knew nothing about. One resolution, one order, both surfaces.
fn set_pending(r: &mut Render, seq: Vec<cinder_db::Track>, start: usize) {
    r.app.set_play_queue(seq.iter().map(song_row_of).collect());
    r.pending_play = seq.into_iter().map(|t| t.filename).collect();
    r.pending_play_start = start;
}

/// Resolve a Library shuffle band into the URIs to play, in the order to play them. `None` when
/// there is no DB or the scope is empty — the caller then emits no action rather than handing the
/// shell an empty sequence.
///
/// Each arm matches the sub-label the band draws, so what the button promises is what it does.
/// Every track by one named artist, shuffled. Matches on ALBUM ARTIST with the track artist as the
/// fallback — the same `group_artist` rule the Artists tab is built with, so the row's track count
/// and what this plays are the same set.
fn artist_tracks(db: Option<&cinder_db::Db>, name: &str) -> Option<Vec<cinder_db::Track>> {
    let db = db?;
    let mut v: Vec<cinder_db::Track> = db
        .tracks(cinder_db::Sort::Artist)
        .ok()?
        .into_iter()
        .filter(|t| {
            let group = if t.album_artist.trim().is_empty() { &t.artist } else { &t.album_artist };
            group == name
        })
        .collect();
    if v.is_empty() {
        return None;
    }
    Rng::new().shuffle(&mut v);
    Some(v)
}

fn shuffle_tracks(db: Option<&cinder_db::Db>, scope: cinder_ui::nav::ShuffleScope) -> Option<Vec<cinder_db::Track>> {
    use cinder_ui::nav::ShuffleScope as S;
    let db = db?;
    let mut rng = Rng::new();

    let seq: Vec<cinder_db::Track> = match scope {
        // "N TRACKS · RANDOM ORDER"
        S::AllSongs => {
            let mut v = db.tracks(cinder_db::Sort::Title).ok()?;
            rng.shuffle(&mut v);
            v
        }
        // "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE" — shuffle the albums, keep each album's
        // tracks in their disc/track order.
        S::ByAlbum => {
            let tracks = db.tracks_album_order().ok()?;
            let mut albums: Vec<Vec<cinder_db::Track>> = Vec::new();
            let mut cur_id: Option<i64> = None;
            for t in tracks {
                if Some(t.album_id.unwrap_or(0)) != cur_id {
                    cur_id = Some(t.album_id.unwrap_or(0));
                    albums.push(Vec::new());
                }
                albums.last_mut().expect("pushed above").push(t);
            }
            rng.shuffle(&mut albums);
            albums.into_iter().flatten().collect()
        }
        // "RANDOM ARTIST · SHUFFLED WITHIN ARTIST" — one artist, their tracks shuffled.
        S::ByArtist => {
            let tracks = db.tracks(cinder_db::Sort::Artist).ok()?;
            let mut by_artist: std::collections::BTreeMap<String, Vec<cinder_db::Track>> = Default::default();
            for t in tracks {
                if !t.artist.is_empty() {
                    by_artist.entry(t.artist.clone()).or_default().push(t);
                }
            }
            let mut names: Vec<String> = by_artist.keys().cloned().collect();
            if names.is_empty() {
                return None;
            }
            let pick = &names[(rng.next() % names.len() as u64) as usize];
            let mut v = by_artist.remove(pick).unwrap_or_default();
            names.clear();
            rng.shuffle(&mut v);
            v
        }
        // "RANDOM PLAYLIST · SHUFFLED"
        S::Playlist => {
            let pls = db.playlists().ok()?;
            if pls.is_empty() {
                return None;
            }
            let pick = &pls[(rng.next() % pls.len() as u64) as usize];
            let mut v = playlist_tracks(Some(db), pick.id)?;
            rng.shuffle(&mut v);
            v
        }
    };
    (!seq.is_empty()).then_some(seq)
}

/// Member file URIs of a playlist, in the user's saved order. `None` when there's no DB, the id
/// isn't a live playlist, or nothing in it still resolves to a playable track — the caller then
/// emits no action rather than handing the shell an empty sequence.
fn playlist_tracks(db: Option<&cinder_db::Db>, playlist_id: i64) -> Option<Vec<cinder_db::Track>> {
    let tracks = db?.playlist_tracks(playlist_id).ok()?;
    (!tracks.is_empty()).then_some(tracks)
}

/// Map a navigator `Action` to the `cinder_action_t` the shell carries out (Some = return this
/// code), applying the internal-only ones in place and returning None for them (theme is applied by
/// the caller; the sleep timer arms here; BtToggle is UI-only). Shared by cinder_input + cinder_tap.
fn carry_action(r: &mut Render, a: &cinder_ui::nav::Action) -> Option<libc::c_int> {
    use cinder_ui::nav::Action;
    Some(match a {
        Action::PlayPause => 1,
        Action::Next => 2,
        Action::Prev => 3,
        Action::NextAlbum => 4,
        Action::PrevAlbum => 5,
        Action::VolUp => 6,
        Action::VolDown => 7,
        Action::PlayIndex(object_id) => {
            // Resolve the chosen track to its album context (URIs in play order + start index)
            // so the shell can hand PlayerService a real sequence. No DB / no match -> no action.
            let ctx = r.db.as_ref().and_then(|db| db.album_context(*object_id).ok().flatten());
            match ctx {
                Some((tracks, idx)) if !tracks.is_empty() => {
                    let (seq, start) = apply_shuffle(r.np.shuffle, tracks, idx);
                    set_pending(r, seq, start);
                    8
                }
                _ => {
                    eprintln!("cinder-ffi: PlayIndex({object_id}): no DB context — ignored");
                    return None;
                }
            }
        }
        Action::PlayPlaylist(playlist_id) => {
            // Same channel as PlayIndex — the members become the pending sequence, starting at
            // the top — so the shell keeps handling exactly one "play these URIs" action and
            // needs no new code or FFI symbol for playlists.
            match playlist_tracks(r.db.as_ref(), *playlist_id) {
                Some(seq) => {
                    let (seq, start) = apply_shuffle(r.np.shuffle, seq, 0);
                    set_pending(r, seq, start);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: PlayPlaylist({playlist_id}): empty or unknown — ignored");
                    return None;
                }
            }
        }
        Action::Shuffle(scope) => {
            // Same pending-play channel again: we pre-shuffle the URI list ourselves, so the
            // order is genuinely random regardless of what PlayerService's own shuffle does.
            match shuffle_tracks(r.db.as_ref(), *scope) {
                Some(seq) => {
                    set_pending(r, seq, 0);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: Shuffle({scope:?}): nothing to play — ignored");
                    return None;
                }
            }
        }
        Action::ShufflePlaylist(playlist_id) => {
            // One NAMED playlist, shuffled — the band on the playlist page. Distinct from
            // `ShuffleScope::Playlist`, which picks a random playlist and shuffles that.
            match playlist_tracks(r.db.as_ref(), *playlist_id) {
                Some(mut seq) => {
                    Rng::new().shuffle(&mut seq);
                    set_pending(r, seq, 0);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: ShufflePlaylist({playlist_id}): nothing playable in it");
                    return None;
                }
            }
        }
        Action::ShuffleArtist(idx) => {
            // One named artist, their tracks shuffled — the Artists-row button and the band on the
            // artist page. Same pending-play channel as every other "play these URIs" action.
            //
            // Grouped by ALBUM ARTIST (falling back to the track artist), which is what the
            // Artists tab itself is built from. `ShuffleScope::ByArtist` groups by TRACK artist
            // instead, so on a compilation it would shuffle a different set of tracks than the row
            // you pressed claims to contain.
            let name = match r.app.artist_name_at(*idx) {
                Some(n) => n.to_string(),
                None => {
                    eprintln!("cinder-ffi: ShuffleArtist({idx}): no such artist — ignored");
                    return None;
                }
            };
            match artist_tracks(r.db.as_ref(), &name) {
                Some(seq) => {
                    set_pending(r, seq, 0);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: ShuffleArtist({name}): nothing to play — ignored");
                    return None;
                }
            }
        }
        Action::ThemeChanged(_) => 16, // shell also drives the backlight (night = minimal light)
        Action::Sleep => 10,
        Action::EnterUsbMsc => 11,
        Action::ExitUsbMsc => 19,
        Action::EqChanged(_) => 12,
        Action::BtToggle(_) => 26, // shell drives SetRfOnOff + reconnects the last device
        Action::SleepTimer(m) => {
            // internal: arm/cancel the countdown (no Sony service to start it)
            r.sleep_remaining_ms = *m as i64 * 60_000;
            r.sleep_fire = false;
            return None;
        }
        Action::BatteryCareChanged(_) => 13,
        Action::SoundChanged => 14,
        Action::SoundBypass(_) => 15,
        Action::ShuffleToggle => {
            // UI-only for now: hold the state here so the icon reflects it. Telling PlayerService to
            // actually shuffle the queue is device-gated (PlayController, same as PlayIndex).
            r.np.shuffle = !r.np.shuffle;
            return None;
        }
        Action::RepeatCycle => {
            // Two states, both of which do something. It used to cycle off → all → one and tell
            // PlayerService nothing at all; "all" has no known primitive, so a third position would
            // still be decorative. The shell reads cinder_get_repeat_one() and applies it.
            r.np.repeat = if r.np.repeat == 0 { 1 } else { 0 };
            23
        }
        Action::BtCodecChanged => 17, // shell reads cinder_get_bt_codec/quality + applies via BtTransmitter
        Action::UsbDacToggle(_) => 18, // shell reads cinder_get_usb_dac() + starts/stops the LDAC bridge
        Action::BrightnessChanged(_) => 20, // shell reads cinder_get_brightness() + writes the backlight
        Action::ScreenOffTimer(_) => 21,    // shell reads cinder_get_screen_off_s() + counts idle
        Action::BootToStock => 22,          // shell arms the one-shot flag + restarts into stock
        Action::QueueChanged => {
            // DO NOT re-issue now. Measured on device 2026-07-28 with the in-app probe: a
            // SetTrackSequence during playback restarts the sequence — position went 9000 → 0 and
            // the track stopped. PlayerService has no insert, so the queue can only be changed by
            // handing it a whole new sequence, and the only moment that is free is a track
            // boundary, where the position is already ~0 and a reset is invisible.
            //
            // So the change is recorded and `apply_pending_queue` flushes it when the track
            // changes (or when nothing is playing, where there is nothing to disturb).
            r.queue_pending = true;
            return None;
        }
        Action::Restart => 24,              // PowerMgrServiceClient::Reboot — back into Cinder
        Action::PowerOff => 25,             // PowerMgrServiceClient::SetStatus(PowerOff)
        Action::ToggleLiked => {
            // Handled entirely in-process: the set and its file live here, so there is nothing for
            // the shell to carry out.
            liked_toggle_current(r);
            return None;
        }
    })
}

/// A touchscreen TAP at UI coordinates (0..480, 0..800). The NW-A55 has no d-pad — this is the
/// primary navigation. Mirrors cinder_input: routes the tap through the navigator, applies internal
/// state, persists, and returns the first cinder_action_t for the shell to carry out (0 = nothing).
#[no_mangle]
pub extern "C" fn cinder_tap(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let actions = r.app.tap(x as i32, y as i32);
    r.dirty = true;
    r.night = r.app.night;
    save_settings(r);
    for a in &actions {
        if let Some(code) = carry_action(r, a) {
            return code;
        }
    }
    0
}

/// A horizontal touch SWIPE (dir: negative = leftward, else rightward) with the gesture's START
/// point in UI coordinates, classified by the shell. Onboarding pages through (left = next/finish,
/// right = back); Now Playing skips track; a RIGHTWARD swipe on a Library/Album song row queues
/// that song (the start y picks the row). Returns the action code for the shell to carry out.
#[no_mangle]
pub extern "C" fn cinder_swipe(dir: libc::c_int, x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let actions = r.app.swipe(if dir < 0 { -1 } else { 1 }, x as i32, y as i32);
    r.dirty = true;
    r.night = r.app.night;
    save_settings(r);
    for a in &actions {
        if let Some(code) = carry_action(r, a) {
            return code;
        }
    }
    0
}

/// LIVE horizontal drag on a list row: `dx_px` is total travel from the gesture's start point,
/// `y` that start point (UI coords). The row under `y` slides with the finger and reveals the
/// action behind it. Returns 1 if a track row took the gesture, so the shell can commit the
/// contact to the swipe instead of still weighing it against a vertical scroll.
///
/// Nothing is queued here — that still happens on release, in `cinder_swipe`. This is purely the
/// feedback the gesture never had.
#[no_mangle]
pub extern "C" fn cinder_swipe_track(dx_px: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let took = r.app.swipe_track(dx_px as i32, y as i32);
    r.dirty = true;
    took as libc::c_int
}

/// The finger came off a live row swipe: the row animates back to rest. Safe to call for any
/// contact — it is a no-op if no row was moving.
#[no_mangle]
pub extern "C" fn cinder_swipe_release() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.swipe_release();
        r.dirty = true;
    }
}

/// Does a vertical drag starting at `(x, y)` pick up an Up Next queue row for reordering?
///
/// Asked ONCE, at the moment the shell classifies a contact as mostly-vertical. A non-zero return
/// means the row owns this contact for the rest of its life: the shell must stream it to
/// [`cinder_reorder_track`] instead of to the scroll, and end it with [`cinder_reorder_release`].
#[no_mangle]
pub extern "C" fn cinder_reorder_begin(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let took = r.app.reorder_begin(x as i32, y as i32);
    r.dirty = true;
    took as libc::c_int
}

/// Stream a reorder drag: `dy_px` is TOTAL travel from the gesture's start point.
#[no_mangle]
pub extern "C" fn cinder_reorder_track(dy_px: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.reorder_track(dy_px as i32);
        r.dirty = true;
    }
}

/// Drop the dragged row where it now sits. No-op if no drag was in progress. The reorder is
/// recorded in-process (`Action::QueueChanged` → `queue_pending`) and flushed to PlayerService at
/// the next track boundary, for the same reason a swipe-to-queue is.
#[no_mangle]
pub extern "C" fn cinder_reorder_release() {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return };
    let actions = r.app.reorder_release();
    r.dirty = true;
    for a in &actions {
        carry_action(r, a);
    }
}

/// Does a vertical drag starting at `(x, y)` grab the SCROLLBAR? Same contract as
/// [`cinder_reorder_begin`], and asked right after it — a queue row's grab handle wins where the
/// two strips would overlap. The content then tracks the THUMB, not the finger.
#[no_mangle]
pub extern "C" fn cinder_sbar_begin(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let took = r.app.sbar_begin(x as i32, y as i32);
    r.dirty = true;
    took as libc::c_int
}

/// Stream a scrollbar drag: `dy_px` is TOTAL travel from the gesture's start point.
#[no_mangle]
pub extern "C" fn cinder_sbar_track(dy_px: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.sbar_track(dy_px as i32);
        r.dirty = true;
    }
}

/// The finger came off the scrollbar. No-op if no drag was in progress.
#[no_mangle]
pub extern "C" fn cinder_sbar_release() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.sbar_release();
        r.dirty = true;
    }
}

/// LIVE drag-scroll: move the current list by `dy_px` pixels (positive = show later rows).
/// The shell calls this every pump tick while a vertical drag is in progress, so the list
/// tracks the finger 1:1.
#[no_mangle]
pub extern "C" fn cinder_touch_drag(dy_px: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.scroll_px(dy_px as i32);
        r.dirty = true;
    }
}

/// Momentum fling at drag release: `velocity_px_s` in px/s (same sign as cinder_touch_drag).
/// The UI integrates + decays it each frame until it stops (frames stay dirty meanwhile).
#[no_mangle]
pub extern "C" fn cinder_touch_fling(velocity_px_s: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.fling(velocity_px_s as f32);
        r.dirty = true;
    }
}

/// A new finger contact: kill any in-flight fling so the list stops under the finger.
#[no_mangle]
pub extern "C" fn cinder_touch_down() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.stop_fling();
    }
}

/// Pending play request (set when a CINDER_ACT_PLAY_INDEX action was returned): how many track
/// URIs are queued. The shell reads them with cinder_pending_play_uri and starts playback at
/// cinder_pending_play_start. The list stays until the next PlayIndex replaces it.
#[no_mangle]
pub extern "C" fn cinder_pending_play_count() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.pending_play.len() as libc::c_int)
}

/// Copy pending-play URI `i` (0-based) into `buf` (NUL-terminated). Returns the FULL byte length
/// of the URI — snprintf semantics — so a return >= `cap` means the value was TRUNCATED and the
/// caller must not use it. Returns -1 for a bad index/args.
///
/// The full length matters: a truncated path still looks like a perfectly good path, and handing
/// one to PlayerService queues a file that doesn't exist. Silently playing the wrong thing is worse
/// than skipping the track.
#[no_mangle]
pub extern "C" fn cinder_pending_play_uri(i: libc::c_int, buf: *mut c_char, cap: libc::c_int) -> libc::c_int {
    if buf.is_null() || cap <= 0 {
        return -1;
    }
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return -1 };
    let Some(uri) = r.pending_play.get(i as usize) else { return -1 };
    unsafe { copy_str_into(uri, buf, cap) }
}

/// Copy `s` into a C buffer, NUL-terminated, and return `s`'s FULL length (snprintf semantics):
/// a result >= `cap` means the value was truncated. Split out from the FFI wrapper so the length
/// contract — the part that had the bug — is unit-testable without a live renderer.
///
/// # Safety
/// `buf` must be valid for `cap` bytes and `cap` must be > 0.
unsafe fn copy_str_into(s: &str, buf: *mut c_char, cap: libc::c_int) -> libc::c_int {
    let n = s.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, n);
    *buf.add(n) = 0;
    s.len() as libc::c_int
}

/// The start index within the pending-play list (the track the user actually tapped).
#[no_mangle]
pub extern "C" fn cinder_pending_play_start() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.pending_play_start as libc::c_int)
}

/// The Hold/lock SWITCH changed state (held != 0 = locked). The navigator disables the touchscreen
/// while locked and shows the Lock screen; only held=0 unlocks. Power never unlocks.
#[no_mangle]
pub extern "C" fn cinder_set_hold(held: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_hold(held != 0);
        r.night = r.app.night;
        r.dirty = true;
    }
}

/// The Power button has been held down past the long-press threshold: open the Power menu
/// (Power off / Restart / Cancel), Sony's own gesture. Returns 1 if the menu opened, 0 if it was
/// refused (Hold engaged, or a modal is already up). The shell uses the answer to decide whether
/// the eventual RELEASE should still toggle the screen — it must not, if the menu is now showing.
#[no_mangle]
pub extern "C" fn cinder_power_held() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    if r.app.power_held() {
        r.dirty = true;
        1
    } else {
        0
    }
}

/// Is a modal dialog up? The shell asks so the idle screen-blank timer does not blank a
/// "Power off?" prompt out from under the finger that is about to answer it.
#[no_mangle]
pub extern "C" fn cinder_modal_open() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.modal_open() as libc::c_int)
}

/// Force the next `cinder_render_tick` to repaint + blit even if nothing changed. The shell calls
/// this to overwrite anything an external process drew on the framebuffer (e.g. the boot
/// animation's last frame, which survives its kill and would otherwise sit on screen forever
/// because the dirty-flag renderer skips identical frames).
#[no_mangle]
pub extern "C" fn cinder_force_dirty() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.dirty = true;
    }
}

/// Raise the USB mass-storage modal from the shell. Called when the shell auto-detects a PC host
/// (before it flips the gadget to MSC) so the UI shows the same modal a manual settings-row tap
/// would. Idempotent — safe to call every auto-detect poll. Returns 1 if the modal is up.
#[no_mangle]
pub extern "C" fn cinder_show_usb_storage() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    r.app.show_usb_storage();
    r.dirty = true;
    r.app.is_usb_storage() as libc::c_int
}

#[no_mangle]
pub extern "C" fn cinder_render_shutdown() {
    *cell().lock().unwrap() = None; // Framebuffer::drop unmaps
}

/// Copy the current 10-band EQ gains (dB) into `out` (must point to >= 10 `int8`). The shell
/// calls this after a CINDER_ACT_EQ_CHANGED action and applies the bands to the device DSP via
/// the effect shim. No-op on a null pointer / uninitialised renderer.
#[no_mangle]
pub extern "C" fn cinder_get_eq_bands(out: *mut i8) {
    if out.is_null() {
        return;
    }
    if let Some(r) = cell().lock().unwrap().as_ref() {
        let bands = r.app.eq_bands();
        unsafe {
            for (i, b) in bands.iter().enumerate() {
                *out.add(i) = *b;
            }
        }
    }
}

/// Sync the UI's "Battery care" toggle to the device's REAL state. The shell reads
/// PowerMgrServiceClient::IsItawariChargingEnabled() at boot and pushes it here (1 = on, 0 = off);
/// values < 0 (service unavailable) are ignored. Repaints only on a change.
#[no_mangle]
pub extern "C" fn cinder_set_battery_care(on: libc::c_int) {
    if on < 0 {
        return;
    }
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let b = on != 0;
        if b != r.app.battery_care() {
            r.app.set_battery_care(b);
            r.dirty = true;
        }
    }
}

/// Push the real storage usage label (e.g. "12.4 / 58 GB") for the Settings Storage row. The shell
/// formats it from statvfs of the music mount; NULL/empty leaves the neutral placeholder.
#[no_mangle]
pub extern "C" fn cinder_set_storage(label: *const c_char) {
    let s = unsafe { cstr(label) };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        if s != r.app.storage_label() {
            r.app.set_storage(&s);
            r.dirty = true;
        }
    }
}

/// Read the UI's current "Battery care" desired value (1 = on, 0 = off). The shell calls this after
/// a CINDER_ACT_BATTERY_CARE_CHANGED action and applies it via PowerMgrServiceClient. Returns 0 if
/// the renderer isn't up.
#[no_mangle]
pub extern "C" fn cinder_get_battery_care() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.battery_care() => 1,
        _ => 0,
    }
}

/// Is night theme currently active? (1 = night, 0 = day.) The shell reads this after a
/// CINDER_ACT_THEME_CHANGED action (and at boot) to set the panel backlight: night = minimal light.
/// Did a queue flush become ready at the last track boundary? Clears on read. When it returns 1 the
/// shell drains `cinder_pending_play_*` and hands the result to PlayerService exactly as it does
/// for a normal play request — the sequence is rebuilt with the track that just started at index 0
/// followed by the user queue.
#[no_mangle]
pub extern "C" fn cinder_take_queue_flush() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let f = r.queue_flush;
    r.queue_flush = false;
    f as libc::c_int
}

/// Repeat-one on/off (1/0). Read after a CINDER_ACT_REPEAT_CHANGED action; the shell hands it to
/// cinder_audio_set_repeat_one.
#[no_mangle]
pub extern "C" fn cinder_get_repeat_one() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.np.repeat == 1 => 1,
        _ => 0,
    }
}

/// Does the visualiser want the analyzer streaming right now? 1 when the user has it enabled, the
/// Now Playing screen is showing, and something is actually playing.
///
/// The shell polls this and starts/stops Sony's AudioAnalyzerService to match, so the service only
/// runs while its output is on screen. Combined with the shell's own screen-on check that means no
/// FFT, no IPC and no wakeups while the panel is dark or while you are browsing the library —
/// which is most of the time a music player is switched on.
#[no_mangle]
pub extern "C" fn cinder_viz_wants_analyzer() -> libc::c_int {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return 0 };
    (r.app.wants_spectrum() && r.app.is_now_playing() && r.np.playing) as libc::c_int
}

#[no_mangle]
pub extern "C" fn cinder_get_night() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.night => 1,
        _ => 0,
    }
}

/// Read the device-wide BT transmit codec preference (index: 0 LDAC, 1 aptX HD, 2 aptX, 3 SBC) and
/// the LDAC sound-quality tier (index: 0 Auto, 1 990, 2 660, 3 330). The shell reads these after a
/// CINDER_ACT_BT_CODEC_CHANGED action (and at boot) and applies them via BtTransmitterService; the
/// same values feed the USB-DAC→LDAC bridge so the codec choice is consistent everywhere.
#[no_mangle]
pub extern "C" fn cinder_get_bt_codec() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.bt_codec() as libc::c_int,
        None => 0,
    }
}
#[no_mangle]
pub extern "C" fn cinder_get_bt_ldac_quality() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.bt_ldac_quality() as libc::c_int,
        None => 0,
    }
}

/// Is USB-DAC mode engaged? (1/0). The shell reads this after a CINDER_ACT_USBDAC_LDAC action to
/// start/stop the LDAC bridge (and switch the USB gadget to UAC) without disconnecting Bluetooth.
#[no_mangle]
pub extern "C" fn cinder_get_usb_dac() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.usb_dac_on() => 1,
        _ => 0,
    }
}

/// Is the Bluetooth switch on? (1/0). The shell reads this after a CINDER_ACT_BT_TOGGLE action to
/// decide whether to power the radio up (and reconnect the last device) or down.
#[no_mangle]
pub extern "C" fn cinder_get_bt_on() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.bt_on() => 1,
        _ => 0,
    }
}

/// Force the Bluetooth switch to match the radio's real state (from GetBtStatus). Sets state only;
/// raises no action. Called at startup so the switch cannot claim the radio is on when it is not.
#[no_mangle]
pub extern "C" fn cinder_set_bt_on(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_on(on != 0);
    }
}

/// Force the USB-DAC toggle to match the gadget's real mode. The shell calls this at startup with
/// the result of reading `sys.sony.config`, so a mode set outside Cinder (a probe, a crash between
/// the property write and our state, a stock-side change) cannot leave Settings reporting the
/// opposite of the hardware. Sets state only — it raises no action, because the gadget is already
/// there and re-applying would switch USB mode for real.
#[no_mangle]
pub extern "C" fn cinder_set_usb_dac(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_usb_dac(on != 0);
    }
}

/// The UI's panel-brightness level, 1..=5. Read after a CINDER_ACT_BRIGHTNESS_CHANGED action (and
/// at boot) and map it onto the backlight node. Never returns 0 — the shell's lowest level must
/// stay readable, or the screen you'd use to turn it back up is unreadable. Defaults to 4 (which
/// matches the shell's ~70% day level) if the renderer isn't up yet.
#[no_mangle]
pub extern "C" fn cinder_get_brightness() -> libc::c_int {
    cell()
        .lock()
        .unwrap()
        .as_ref()
        .map_or(4, |r| r.app.brightness() as libc::c_int)
        .clamp(1, 5)
}

/// The UI's idle screen-off timeout in SECONDS; 0 = disabled (the default). The shell owns the idle
/// countdown because only it sees every input event. Read after a CINDER_ACT_SCREEN_OFF_CHANGED
/// action and at boot.
#[no_mangle]
pub extern "C" fn cinder_get_screen_off_s() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.screen_off_s() as libc::c_int)
}

/// Seed the UI volume from the device's REAL level (raw 0..120 steps — the stock scale, 1:1 with
/// ALSA 'master volume'), without popping the HUD. The shell calls this at boot after restoring
/// the saved level (or reading the mixer), so the first Vol± press nudges from the actual level.
#[no_mangle]
pub extern "C" fn cinder_set_volume(level: libc::c_int) {
    let level = level.clamp(0, cinder_ui::overlay::VOL_MAX as libc::c_int);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_volume(level as u8);
    }
}

/// Read the current UI volume as the raw 0..120 step level. The shell writes it 1:1 to the device
/// mixer ('master volume', also 0..120) after a VOLUP/VOLDOWN action.
#[no_mangle]
pub extern "C" fn cinder_get_volume() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.volume_level() as libc::c_int,
        None => 0,
    }
}

/// Read the UI's Bluetooth volume as a raw AVRCP step (0..30). Separate from `cinder_get_volume`
/// on purpose: that one is the 3.5 mm codec level and must not move while audio is on headphones.
#[no_mangle]
pub extern "C" fn cinder_get_bt_volume() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.bt_volume_level() as libc::c_int,
        None => 0,
    }
}

/// Seed the UI's Bluetooth volume (0..30 AVRCP steps) without popping the HUD.
#[no_mangle]
pub extern "C" fn cinder_set_bt_volume(level: libc::c_int) {
    let level = level.clamp(0, cinder_ui::overlay::BT_VOL_MAX as libc::c_int);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_volume(level as u8);
    }
}

/// Tell the UI which output the volume rocker should drive: nonzero = Bluetooth, 0 = the 3.5 mm
/// jack. The shell owns this because only it can see the radio. Changing it never moves either
/// level — it just decides which one the next press touches and which one the HUD shows.
#[no_mangle]
pub extern "C" fn cinder_set_bt_route(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_route(on != 0);
    }
}

/// 1 if the rocker is currently driving the Bluetooth route.
#[no_mangle]
pub extern "C" fn cinder_get_bt_route() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.bt_route() as libc::c_int,
        None => 0,
    }
}

/// Read the UI's Sound-effect toggles as a bitmask (bit0 DSEE · bit1 Vinyl · bit2 VPT ·
/// bit3 DC-Phase · bit4 Normalizer · bit5 ClearAudio+). The shell calls this after a
/// CINDER_ACT_SOUND_CHANGED action and applies each bit via the effect shim. 0 if renderer not up.
#[no_mangle]
pub extern "C" fn cinder_get_sound_flags() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.sound_flags() as libc::c_int,
        None => 0,
    }
}

/// Read the Sound screen's A/B compare state (1 = B / chain bypassed, 0 = A / active). The shell
/// calls this after a CINDER_ACT_SOUND_BYPASS action and applies it via cinder_effects_set_bypass.
#[no_mangle]
pub extern "C" fn cinder_get_sound_bypass() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.sound_bypass() => 1,
        _ => 0,
    }
}

/// Enable the built-in scrobbler, appending an Audioscrobbler/1.1 `.scrobbler.log` at `path`
/// (typically the storage root, e.g. "/contents/.scrobbler.log"). `client` is the
/// #CLIENT id. Call after `cinder_db_open`. Returns 0, or -2 if the renderer isn't up.
#[no_mangle]
pub extern "C" fn cinder_scrobble_open(path: *const c_char, client: *const c_char) -> libc::c_int {
    let p = unsafe { cstr(path) };
    let c = unsafe { cstr(client) };
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -2 };
    let client = if c.is_empty() { "Cinder NW-A55".to_string() } else { c };
    r.scrob = Some(scrobble::Scrobbler::new(p, client));
    0
}

/// Enable/disable the Now Playing visualiser animation (1 = on, 0 = off). Off keeps the device
/// idle while watching a playing track (battery). Default on.
#[no_mangle]
pub extern "C" fn cinder_set_visualizer(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_viz_on(on != 0);
        r.dirty = true;
    }
}

/// Select the visualiser TYPE (0..cinder_visualizer_count()-1): Bars/Mirror/Segments/Dots/Wave.
#[no_mangle]
pub extern "C" fn cinder_set_visualizer_type(kind: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_viz_kind(kind.max(0) as u8);
        r.dirty = true;
    }
}

/// Number of visualiser types available.
#[no_mangle]
pub extern "C" fn cinder_visualizer_count() -> libc::c_int {
    cinder_ui::viz::COUNT as libc::c_int
}

/// Feed a mono PCM window (i16 samples) for a REAL audio-reactive visualiser: we FFT it into the
/// 36 spectrum bars the Now Playing visualiser draws. Call from the pump while playing (the shell
/// taps PCM from Sony's AudioAnalyzerService). No-op on null/empty. This is the only thing needed
/// to turn the visualiser from synthetic motion into real spectrum — no other change.
#[no_mangle]
pub extern "C" fn cinder_set_pcm(samples: *const i16, n: libc::c_int) {
    if samples.is_null() || n <= 0 {
        return;
    }
    let pcm = unsafe { std::slice::from_raw_parts(samples, n as usize) };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let prev = std::mem::take(&mut r.viz_levels);
        r.viz_levels = spectrum::levels(pcm, 36, &prev);
        r.viz_at = std::time::Instant::now();
        // Only force a repaint when the visualiser is actually on screen — the audio source may
        // stream continuously, but off Now Playing the new levels are unused, so don't burn a frame.
        if r.app.wants_spectrum() && r.app.is_now_playing() {
            r.dirty = true;
        }
    }
}

/// Feed PRE-COMPUTED spectrum bands (Sony's `AudioAnalyzerService::OnSpectrumUpdate` gives a
/// `vector<int>` of band magnitudes) for a real audio-reactive visualiser. This is the PREFERRED
/// real-data path: Sony already did the FFT, so there is no FFT cost on our side. We resample the
/// `n` source bands into the 36 bars the visualiser draws and auto-normalise (see
/// spectrum::from_bands). No-op on null/empty. The analyzer shim (cinder_analyzer.h) calls this
/// from its listener callback, behind the shell's guard. Use cinder_set_pcm instead only when you
/// have raw PCM and no analyzer (e.g. the USB-DAC tap).
#[no_mangle]
pub extern "C" fn cinder_set_spectrum(bands: *const libc::c_int, n: libc::c_int) {
    if bands.is_null() || n <= 0 {
        return;
    }
    let src = unsafe { std::slice::from_raw_parts(bands as *const i32, n as usize) };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let prev = std::mem::take(&mut r.viz_levels);
        let mut peak = r.viz_peak;
        r.viz_levels = spectrum::from_bands(src, 36, &prev, &mut peak);
        r.viz_peak = peak;
        r.viz_at = std::time::Instant::now();
        // Only force a repaint when the visualiser is on screen (the analyzer streams continuously).
        if r.app.wants_spectrum() && r.app.is_now_playing() {
            r.dirty = true;
        }
    }
}

/// Push the battery percentage (0..100) for the status bar. Cheap; repaints only on a change.
/// (cinder-home reads it from sysfs periodically — battery moves slowly, so ~every 10s.)
#[no_mangle]
pub extern "C" fn cinder_set_battery(pct: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let b = pct.clamp(0, 100) as u8;
        if b != r.np.battery {
            r.np.battery = b;
            r.dirty = true;
        }
    }
}

/// Refresh the status-bar / lock-screen clock from the system's local time (call ~1x/sec from
/// the pump). Only repaints when the minute actually changes (dirty-flag friendly).
#[no_mangle]
pub extern "C" fn cinder_clock_tick() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let hhmm = current_hhmm();
        if !hhmm.is_empty() && hhmm != r.np.clock {
            r.np.clock = hhmm;
            r.dirty = true;
        }
        // Advance the now-playing position estimate by REAL elapsed time (rate-independent: this is
        // called from the pump's ~1x/sec housekeeping, but the cadence varies, so we use a wall-clock
        // delta rather than a fixed +1s). We re-anchor every tick so paused time isn't counted; only
        // add the delta while playing. Moves the bar + elapsed/remaining; stops at the track end.
        let now = std::time::Instant::now();
        let dt = now.saturating_duration_since(r.last_pos).as_millis() as i64;
        r.last_pos = now;
        if r.scrub_ms.is_none() && r.np.playing && r.cur_duration_ms > 0
            && r.play_pos_ms < r.cur_duration_ms
        {
            // Prefer the real service position, interpolated forward from when it arrived (it
            // lands ~1x/sec, which would otherwise make the bar step visibly). Falling back to
            // the local estimate keeps the bar alive if the listener goes quiet.
            r.play_pos_ms = if r.real_pos_ms >= 0 {
                let since = now.saturating_duration_since(r.real_pos_at).as_millis() as i64;
                (r.real_pos_ms + since).min(r.cur_duration_ms)
            } else {
                (r.play_pos_ms + dt).min(r.cur_duration_ms)
            };
            let (pos, dur) = (r.play_pos_ms, r.cur_duration_ms);
            set_progress(&mut r.np, pos, dur);
            // Repaint only when Now Playing is on screen (the bar/labels are only visible there); the
            // position still advances off-screen so it's correct when you return.
            if r.app.is_now_playing() {
                r.dirty = true;
            }
        }
        // Sleep timer: count down in wall-clock (regardless of play/pause). Push the remaining
        // minutes (ceil) to the nav for the Settings row; repaint only when the displayed minute
        // changes. On reaching 0, raise sleep_fire (the shell pauses) and clear the display.
        if r.sleep_remaining_ms > 0 {
            r.sleep_remaining_ms = (r.sleep_remaining_ms - dt).max(0);
            let rem_min = ((r.sleep_remaining_ms + 59_999) / 60_000) as u32;
            if rem_min != r.app.sleep_min() {
                r.app.set_sleep_min(rem_min);
                r.dirty = true;
            }
            if r.sleep_remaining_ms == 0 {
                r.sleep_fire = true;
                r.app.set_sleep_min(0);
                r.dirty = true;
            }
        }
    }
}

/// Has the sleep timer just expired? Returns 1 once (then clears), so the shell can pause playback.
/// Polled ~1x/sec by the pump.
#[no_mangle]
pub extern "C" fn cinder_sleep_should_pause() -> libc::c_int {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        if r.sleep_fire {
            r.sleep_fire = false;
            return 1;
        }
    }
    0
}

/// Advance the scrobbler's play clock by one second (call ~1x/sec from the pump). `playing`
/// is 0 (paused) / non-zero (playing) — paused time doesn't accrue listen credit. No-op if
/// the scrobbler isn't enabled.
#[no_mangle]
pub extern "C" fn cinder_scrobble_tick(playing: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        // Measure the REAL gap since the last tick here rather than trusting the caller to arrive
        // exactly once a second. The shell's housekeeping fires when *at least* 1000 ms have
        // passed, and its loop runs at 10 Hz while the panel is dark, so the true interval there is
        // 1000-1100 ms. Deriving it means the scrobble clock can't drift when the loop rate changes
        // — and the C ABI stays the same, so the shell can't get it wrong.
        let now = std::time::Instant::now();
        let dt = now.saturating_duration_since(r.last_scrob).as_millis() as u64;
        r.last_scrob = now;
        if let Some(s) = r.scrob.as_mut() {
            // Clamp: a long stall (deferred init, USB-MSC) must not credit minutes of listening
            // that never happened.
            s.tick_ms(playing != 0, dt.min(5000));
        }
    }
}

#[no_mangle]
pub extern "C" fn cinder_set_theme_night(night: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.night = night != 0;
        r.app.night = r.night; // the navigator's render() is the source of truth for theme
        r.dirty = true;
    }
}

/// Load persisted UI preferences (theme + visualiser + EQ + sound effects + volume) from `path`,
/// apply them, and remember the path so later changes auto-save. Call once at boot after
/// cinder_render_init. Returns a bitmask: bit0 = a settings file was read (shell should re-apply
/// EQ/sound to the DSP), bit1 = a persisted volume level was restored (shell should apply it to
/// the mixer instead of seeding the UI from hardware). 0 = no file. Best-effort: a
/// missing/garbage file is ignored (defaults stand); robust line parser.
#[no_mangle]
pub extern "C" fn cinder_settings_load(path: *const c_char) -> libc::c_int {
    let p = unsafe { cstr(path) };
    if p.is_empty() {
        return 0;
    }
    let mut loaded = 0;
    if let Some(r) = cell().lock().unwrap().as_mut() {
        if let Ok(body) = std::fs::read_to_string(&p) {
            loaded = 1;
            for line in body.lines() {
                let mut it = line.splitn(2, '=');
                let k = it.next().unwrap_or("").trim();
                let v = it.next().unwrap_or("").trim();
                match k {
                    "night" => r.app.night = v == "1",
                    "accent" => {
                        // set_accent snaps an unknown index to the default, so a hand-edited or
                        // corrupt value can't leave the UI on a colour the picker can't reach.
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_accent(n);
                        }
                    }
                    "viz_kind" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_kind(n);
                        }
                    }
                    // Written by builds before the visualiser gained sizes. Map it so an upgrade
                    // keeps the user's choice instead of silently resetting it: on => FULL (the
                    // only "on" that existed), off => OFF. A `viz_size` line, if present, is read
                    // after this and wins.
                    "viz_on" => r.app.set_viz_on(v == "1"),
                    "viz_size" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_size(n);
                        }
                    }
                    "np_page" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_np_page(n);
                        }
                    }
                    // Shuffle and repeat persist now that they do something. Losing them on every
                    // reboot would mean re-enabling shuffle each morning, and everything else on
                    // this screen already survives a restart.
                    "shuffle" => r.np.shuffle = v == "1",
                    // Clamped: only 0 and 1 exist (repeat-all has no primitive), so a stale file
                    // written by a build that cycled three states cannot restore a dead value.
                    "repeat" => r.np.repeat = u8::from(v == "1"),
                    "eq" => {
                        let mut arr = r.app.eq_bands();
                        for (i, part) in v.split(',').enumerate().take(10) {
                            if let Ok(g) = part.trim().parse::<i8>() {
                                arr[i] = g;
                            }
                        }
                        r.app.set_eq_bands(arr);
                    }
                    "sound" => {
                        if let Ok(f) = v.parse::<u8>() {
                            r.app.set_sound_flags(f);
                        }
                    }
                    "onboarding" => r.app.set_onboarding_seen(v == "1"),
                    "bt_codec" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_codec(n);
                        }
                    }
                    "bt_ldac_quality" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_ldac_quality(n);
                        }
                    }
                    "volume" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_volume(n);
                            loaded |= 2; // bit1: a persisted volume level was restored
                        }
                    }
                    "bt_volume" => {
                        // Deliberately does NOT set bit1. That bit means "push this to the
                        // hardware", and there is no way to push an absolute level over AVRCP —
                        // only up/down steps. The sink keeps its own volume across reconnects, so
                        // this is restored as the UI's BELIEF about where the headphones are, and
                        // it can be stale until the user nudges the rocker.
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_volume(n);
                        }
                    }
                    "screen_off" => {
                        // set_screen_off_s snaps to a known preset, so a hand-edited value can't
                        // leave the Settings row showing something it can't cycle away from.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_screen_off_s(n);
                        }
                    }
                    "brightness" => {
                        // set_brightness clamps to 1..5, so a corrupt or out-of-range file can
                        // never restore a level the shell would map to an unreadable screen.
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_brightness(n);
                        }
                    }
                    _ => {}
                }
            }
            r.night = r.app.night;
            r.dirty = true;
        }
        r.settings_path = Some(p);
        r.last_saved_body = settings_body(r);
        // First run (intro not completed / no settings file yet) → show onboarding before anything.
        if !r.app.onboarding_seen() {
            r.app.start_onboarding();
            r.dirty = true;
        }
    }
    loaded
}

/// Push the currently-playing track. Strings are copied; NULL = empty.
/// `progress` is 0..1; `playing`/`battery` as shown in the status/transport.
/// Open the library DB (read-only). Call after `cinder_render_init`. Returns 0 on success,
/// -1 on open failure, -2 if the renderer isn't initialised. Path is e.g. "/db/MTPDB.dat".
/// Load whatever cover thumbnails are already cached into the live library, then start the
/// background builder for the rest.
///
/// Called with the renderer lock held (from `cinder_db_open`), so it must not block: the disk load
/// is a few MB of sequential reads off ext4, and the decoding — 365 ms per album — happens on the
/// spawned thread, which takes the lock only to hand over each finished thumbnail.
fn start_art_cache(r: &mut Render, db_path: &str) {
    if !art_cache::ensure_dir() {
        return;
    }
    let sources = match r.db.as_ref().map(|db| db.album_cover_sources()) {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            eprintln!("cinder-ffi: art cache: album source query failed: {e}");
            return;
        }
        None => return,
    };
    // Anything already on disk shows up on this first frame.
    let cached = art_cache::load_all(sources.iter().map(|(aid, _)| *aid));
    let have = cached.len();
    r.app.library_mut().thumbs = cached;
    let todo: Vec<(i64, i64)> = sources
        .into_iter()
        .filter(|(aid, _)| !art_cache::is_cached(*aid))
        .collect();
    eprintln!(
        "cinder-ffi: art cache: {have} cached, {} to decode (~{} s of background work)",
        todo.len(),
        todo.len() * 2 / 5,
    );
    if todo.is_empty() {
        return;
    }
    if ART_BUILDER_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return; // a rebuild is already in flight (cinder_db_open can be called again)
    }
    let path = db_path.to_string();
    std::thread::spawn(move || {
        // The thread opens its OWN read-only DB handle rather than sharing the renderer's: no
        // lifetime plumbing, no lock held across a 365 ms decode, and a read-only SQLite handle
        // per thread is exactly what rusqlite wants.
        let db = match cinder_db::Db::open(&path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("cinder-ffi: art cache: builder can't open {path}: {e}");
                ART_BUILDER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        };
        let total = todo.len();
        let mut done = 0usize;
        for (album_id, object_id) in todo {
            // Decode OUTSIDE the lock. This is the expensive part and the UI must keep painting
            // through it.
            let Some(t48) = art_cache::build_one(&db, album_id, object_id) else {
                continue;
            };
            done += 1;
            if let Ok(mut g) = cell().lock() {
                let Some(r) = g.as_mut() else { break }; // renderer gone (shutdown) — stop
                r.app.library_mut().thumbs.insert(album_id, t48);
                r.dirty = true; // the row this belongs to may be on screen right now
            }
            // Yield between albums. The builder is strictly background work: a cover that shows up
            // a minute later costs the user nothing, whereas competing with the render thread for
            // this single core would be visible immediately as scroll stutter.
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        eprintln!("cinder-ffi: art cache: builder finished — {done}/{total} covers decoded");
        ART_BUILDER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

static ART_BUILDER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ── The now-playing cover, decoded off the render thread ──────────────────────────────────────
//
// The library-wide art cache above builds 48 px thumbnails in the background. Now Playing needs a
// 480 px cover, which is not cached, so it used to be decoded INLINE at every track change — on the
// render thread, holding the global lock, for ~365 ms. That is a visible freeze on every skip and
// on every track that ends, and it is the larger half of the "playing a song is laggy" report.
//
// One long-lived worker, woken by a slot holding only the LATEST request. A slot rather than a
// queue is the point: while a decode is in flight the user may skip five times, and every cover
// but the last is already garbage by the time it would be drawn. The worker overwrites, so a run
// of skips costs one decode, not five.
static COVER_REQ: std::sync::Mutex<Option<i64>> = std::sync::Mutex::new(None);
static COVER_WAKE: std::sync::Condvar = std::sync::Condvar::new();
static COVER_THREAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask for `object_id`'s cover. Returns immediately; the worker installs it when it is ready.
fn request_cover(r: &Render, object_id: i64) {
    let Some(path) = r.db_path.clone() else { return };
    *COVER_REQ.lock().unwrap() = Some(object_id);
    COVER_WAKE.notify_one();
    if COVER_THREAD.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return; // already running
    }
    std::thread::spawn(move || {
        // Own read-only handle, exactly as the thumbnail builder does: no lifetime plumbing, and
        // no chance of holding the renderer's DB across a long decode.
        let db = match cinder_db::Db::open(&path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("cinder-ffi: cover decoder can't open {path}: {e}");
                COVER_THREAD.store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        };
        loop {
            let want = {
                let mut slot = COVER_REQ.lock().unwrap();
                while slot.is_none() {
                    slot = COVER_WAKE.wait(slot).unwrap();
                }
                slot.take().unwrap()
            };
            // Decode with NO lock held — the whole reason this thread exists.
            let native = art_load::load(&db, want);
            let full = native.as_ref().map(|img| img.scaled_to(480, 480));
            let thumb = native.as_ref().map(|img| img.scaled_to(92, 92));
            if full.is_none() {
                continue; // no embedded cover; the gradient already on screen is the right answer
            }
            let Ok(mut g) = cell().lock() else { break };
            let Some(r) = g.as_mut() else { break }; // renderer gone (shutdown)
            // STILL WANTED? The track may have changed while we decoded. Installing anyway would
            // paint the previous song's cover over the current one and leave it there until the
            // next change — worse than the gradient it replaced.
            if r.art_key == Some(want) {
                r.art_full = full;
                r.art_thumb = thumb;
                r.dirty = true;
            }
        }
        COVER_THREAD.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}



#[no_mangle]
pub extern "C" fn cinder_db_open(path: *const c_char) -> libc::c_int {
    let p = unsafe { cstr(path) };
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -2 };
    r.dirty = true; // the library (or its absence) changed -> repaint
    match cinder_db::Db::open(&p) {
        Ok(db) => {
            // Build the browsable library now so the Library screen shows real music.
            let lib = build_library(&db);
            eprintln!(
                "cinder-ffi: library loaded — {} tracks, {} albums, {} artists",
                lib.songs.len(),
                lib.album_count(),
                lib.artists.len()
            );
            r.app.set_library(lib);
            r.db = Some(db);
            // Liked list lives beside the user's music, not next to the DB: /contents is the
            // partition they can actually reach over USB-MSC to back it up or edit it.
            let liked_path = String::from("/contents/cinder_liked.conf");
            r.liked = liked_load(&liked_path);
            eprintln!("cinder-ffi: liked songs: {} loaded", r.liked.len());
            r.liked_path = Some(liked_path);
            r.app.set_liked_count(r.liked.len());
            r.db_path = Some(p.clone());
            start_art_cache(r, &p);
            0
        }
        Err(e) => {
            eprintln!("cinder-ffi: db open {p}: {e}");
            // Don't leave the demo sample library showing on device — that would look like the
            // user's music when the DB didn't actually load. Show an empty library instead so
            // it's honest (Menu shows "Empty", the Library tabs are blank). (The path is a
            // guess — confirm /db/MTPDB.dat on device.)
            r.app.set_library(cinder_ui::Library::default());
            -1
        }
    }
}

/// Would a touch at (`x`,`y`) start a drag-to-seek? True only on Now Playing, inside the progress
/// rail's grab band, and only when a track with a known duration is loaded (there is nothing to
/// seek within otherwise). The shell calls this on finger-DOWN and, if it returns 1, routes the
/// whole contact to the scrub instead of the usual tap / list-drag / swipe classification.
#[no_mangle]
pub extern "C" fn cinder_scrub_hit(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return 0 };
    if !r.app.is_now_playing() || r.cur_duration_ms <= 0 {
        return 0;
    }
    let (x, y) = (x as i32, y as i32);
    let in_band = y >= cinder_ui::now_playing::RAIL_GRAB_TOP
        && y <= cinder_ui::now_playing::RAIL_GRAB_BOT;
    // Horizontally generous: the full width is the rail plus its end caps, so a finger slightly
    // past either end still scrubs to 0% / 100% rather than doing nothing.
    if in_band && x >= 0 && x <= 480 { 1 } else { 0 }
}

/// Move an in-progress drag-to-seek to UI x. Returns the target position in ms (>= 0), or -1 if
/// there is nothing to scrub. Also starts the scrub if it wasn't started yet, so the shell can
/// just call this on down and on every move.
#[no_mangle]
pub extern "C" fn cinder_scrub_to(x: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -1 };
    if r.cur_duration_ms <= 0 {
        return -1;
    }
    let frac = cinder_ui::now_playing::rail_fraction(x as i32);
    let target = (r.cur_duration_ms as f32 * frac) as i64;
    r.scrub_ms = Some(target);
    r.play_pos_ms = target;
    let dur = r.cur_duration_ms;
    set_progress(&mut r.np, target, dur);
    r.dirty = true;
    target as libc::c_int
}

/// Finish a drag-to-seek. Returns the target ms the shell should SeekTime to, or -1 if no scrub
/// was active. Clears the scrub so position updates resume; the bar keeps showing the target
/// until the service reports a position (which it does within ~1 s of the seek landing).
#[no_mangle]
pub extern "C" fn cinder_scrub_end() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -1 };
    match r.scrub_ms.take() {
        Some(target) => {
            // Re-anchor the interpolator on the seek target. Without this, clock_tick would keep
            // extrapolating from the pre-seek anchor and the bar would jump backwards for the
            // ~1 s until the next onPlayTimeUpdated lands.
            r.real_pos_ms = target;
            r.real_pos_at = std::time::Instant::now();
            r.dirty = true;
            target as libc::c_int
        }
        None => -1,
    }
}

// ── Liked songs: load / save / toggle ─────────────────────────────────────────────────────────
// One decimal object_id per line. A plain-text list survives partial writes gracefully (a torn
// line is skipped, not fatal) and is trivially inspectable over USB-MSC, which matters for
// something the user has curated by hand and cannot otherwise back up.
fn liked_load(path: &str) -> std::collections::BTreeSet<i64> {
    std::fs::read_to_string(path)
        .map(|body| body.lines().filter_map(|l| l.trim().parse::<i64>().ok()).collect())
        .unwrap_or_default()
}

fn liked_save(r: &Render) {
    let Some(path) = r.liked_path.as_ref() else { return };
    let body: String = r.liked.iter().map(|id| format!("{id}\n")).collect();
    // Write via a temp file + rename so a power cut mid-write can't truncate the existing list.
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
    liked_export_tsv(r);
}

/// Export the liked list as `artist \t title`, one per line, next to the music.
///
/// LAST.FM: this device has no WiFi — Bluetooth only — so nothing here can ever call the Last.fm
/// API directly, and the offline `.scrobbler.log` cannot carry loves either: its rating column is
/// AS/1.1 `L` = *Listened* / `S` = *Skipped*, which is not the same thing as Last.fm's `track.love`.
/// So syncing takes the same shape scrobbling already does — the device writes a file, a tool on
/// the PC uploads it on the next USB connection. `artist` + `title` is exactly the pair `track.love`
/// takes, which is why this is a separate human-readable file rather than the object_id list (those
/// ids mean nothing off-device).
fn liked_export_tsv(r: &Render) {
    let Some(path) = r.liked_path.as_ref() else { return };
    let tsv = path.replace("cinder_liked.conf", "cinder_loved.tsv");
    let lib = r.app.library();
    let mut body = String::from("# artist\ttitle — liked in Cinder; feed to Last.fm track.love\n");
    for id in &r.liked {
        if let Some(song) = lib.songs.iter().find(|s| s.object_id == *id) {
            body.push_str(&format!("{}\t{}\n", song.artist, song.title));
        }
    }
    let tmp = format!("{tsv}.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &tsv);
    }
}

/// Is the CURRENTLY PLAYING track liked? 1/0. (-1 if the renderer isn't up.)
#[no_mangle]
pub extern "C" fn cinder_is_liked() -> libc::c_int {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return -1 };
    match r.last_track.as_ref() {
        Some(t) => r.liked.contains(&t.object_id) as libc::c_int,
        None => 0,
    }
}

/// Toggle the current track's liked state. Takes `&mut Render` because the action mapper already
/// holds the lock — calling the FFI wrapper from there would deadlock.
fn liked_toggle_current(r: &mut Render) -> libc::c_int {
    let Some(id) = r.last_track.as_ref().map(|t| t.object_id) else { return -1 };
    let now_liked = if r.liked.remove(&id) {
        false
    } else {
        r.liked.insert(id);
        true
    };
    r.np.liked = now_liked;
    r.dirty = true;
    let n = r.liked.len();
    r.app.set_liked_count(n);
    liked_save(r);
    now_liked as libc::c_int
}

/// Toggle the liked state of the currently playing track and persist. Returns the NEW state
/// (1 liked, 0 not), or -1 when nothing is playing / no renderer.
#[no_mangle]
pub extern "C" fn cinder_toggle_liked() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -1 };
    liked_toggle_current(r)
}

/// How many tracks are liked (for the Library's "Liked songs" row).
#[no_mangle]
pub extern "C" fn cinder_liked_count() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.liked.len() as libc::c_int)
}

/// Push the REAL play position/duration from PlayerService's PlayEventListener
/// (`onPlayTimeUpdated(cur_ms, total_ms)`), plus the real play/pause state. This is what makes the
/// progress bar truthful: it follows seeks and mid-track starts, which the local play-clock
/// estimate cannot, and `total_ms` from the service beats the DB duration for files whose tag
/// metadata is wrong.
///
/// `cur_ms` < 0 means "no update yet" and is ignored (the estimate keeps running). Marking the
/// arrival time rather than resetting `last_pos` is deliberate: `last_pos` is the shared anchor
/// the sleep-timer countdown also uses, so re-anchoring it here would make the sleep timer lose
/// time on every position update.
///
/// Returns 0 on success, -2 if the renderer isn't initialised.
#[no_mangle]
pub extern "C" fn cinder_set_play_position(
    cur_ms: libc::c_int,
    total_ms: libc::c_int,
    playing: libc::c_int,
) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -2 };
    let was_playing = r.np.playing;
    r.np.playing = playing != 0;
    if total_ms > 0 {
        // SELF-CHECK for the one remaining unverified assumption in the metadata path: the library
        // DB's `duration_raw` is *assumed* to be milliseconds (fmt_time, and every track length the
        // Library lists, depend on it). PlayerService reports the true duration, so the first time
        // the two disagree materially, say so — one line in the boot log settles a guess that has
        // otherwise been carried since the DB was first read. Logged once, not per second.
        if let Some(db_ms) = r.last_track.as_ref().and_then(|t| t.duration_raw) {
            if db_ms > 0 && !r.duration_checked {
                r.duration_checked = true;
                let ratio = total_ms as f64 / db_ms as f64;
                if !(0.95..1.05).contains(&ratio) {
                    eprintln!(
                        "cinder-ffi: DURATION UNIT MISMATCH — service says {total_ms} ms, DB says                          {db_ms} (ratio {ratio:.3}); duration_raw is not milliseconds"
                    );
                }
            }
        }
        r.cur_duration_ms = total_ms as i64;
    }
    // A drag-to-seek owns the bar until the finger lifts: applying the service's (pre-seek)
    // position here would yank the rail out from under the finger every second.
    if r.scrub_ms.is_some() {
        return 0;
    }
    if cur_ms >= 0 {
        r.real_pos_ms = cur_ms as i64;
        r.real_pos_at = std::time::Instant::now();
        r.play_pos_ms = (cur_ms as i64).min(r.cur_duration_ms.max(0));
        let (pos, dur) = (r.play_pos_ms, r.cur_duration_ms);
        set_progress(&mut r.np, pos, dur);
    }
    // Repaint when the bar is on screen, or whenever play/pause flipped (the transport glyph
    // changes on every screen that shows it).
    if r.app.is_now_playing() || was_playing != r.np.playing {
        r.dirty = true;
    }
    0
}

/// Set now-playing from the track URI PlayerService reports (PlayStatus.uri): resolves
/// title/artist/codec/duration from the library DB and derives elapsed/remaining from
/// `progress` (0..1). Returns 0 if the track resolved, -1 if not (falls back to the
/// filename as title so the screen isn't blank), -2 if the renderer isn't initialised.
#[no_mangle]
pub extern "C" fn cinder_set_now_playing_uri(
    uri: *const c_char,
    progress: f32,
    playing: libc::c_int,
    battery: libc::c_int,
) -> libc::c_int {
    let u = unsafe { cstr(uri) };
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -2 };
    r.np.playing = playing != 0;
    r.np.battery = battery.clamp(0, 100) as u8;
    r.dirty = true; // now-playing changed -> repaint
    let track = r.db.as_ref().and_then(|db| db.track_by_filename(&u).ok().flatten());
    match track {
        Some(t) => {
            // Reset the local play-clock only on a genuine track change; seed it from the passed
            // progress hint (usually 0; >0 once the shell can supply a real PlayStatus position).
            let changed = r.last_track.as_ref().map_or(true, |p| p.object_id != t.object_id);
            apply_track(&mut r.np, &t);
            if changed {
                // Decode the album cover ONCE per track change (never on same-track re-polls:
                // art_key remembers the object we last decoded for). Pre-scale to the two draw
                // sizes so render is a plain blit. Failure → gradient fallback stays.
                if r.art_key != Some(t.object_id) {
                    // DO NOT DECODE HERE. art_load::load plus the two rescales measures ~365 ms on
                    // device, and this runs on the RENDER THREAD holding the global lock — so every
                    // track change froze the whole UI for a third of a second, which is most of
                    // what "playing a song from the library is laggy" was.
                    //
                    // Show the gradient immediately and let the decoder thread replace it when it
                    // lands. art_key is claimed NOW, so a re-poll of the same track does not queue
                    // the work twice; the decoder re-checks it before installing, so a fast run of
                    // skips cannot paint an earlier track's cover over a later one.
                    r.art_full = None;
                    r.art_thumb = None;
                    bake_gradient_art(r);
                    r.art_key = Some(t.object_id);
                    request_cover(r, t.object_id);
                }
                // TRACK BOUNDARY — the one moment a queue change is free. The new track has just
                // begun, so re-issuing the sequence resets a position that is already ~0 and the
                // reset is invisible. Doing it any other time restarts the music (device-measured;
                // see Action::QueueChanged).
                if r.queue_pending {
                    r.queue_pending = false;
                    let mut uris: Vec<String> = vec![t.filename.clone()];
                    uris.extend(r.app.queue().iter().filter_map(|q| {
                        r.db.as_ref()
                            .and_then(|db| db.track_by_object_id(q.object_id).ok().flatten())
                            .map(|t| t.filename)
                    }));
                    if uris.len() > 1 {
                        eprintln!(
                            "cinder-ffi: queue flush at track boundary — {} tracks",
                            uris.len()
                        );
                        r.pending_play = uris;
                        r.pending_play_start = 0;
                        r.queue_flush = true;
                    }
                }
                r.np.liked = r.liked.contains(&t.object_id);
                r.cur_duration_ms = t.duration_raw.unwrap_or(0).max(0);
                r.play_pos_ms = (r.cur_duration_ms as f32 * progress.clamp(0.0, 1.0)) as i64;
                // Drop the previous track's service-position anchor: interpolating the new track's
                // bar from the old track's position would show a wrong (often near-full) bar until
                // the next onPlayTimeUpdated lands.
                r.real_pos_ms = -1;
                // NOTE: do NOT reset `last_pos` here — it's the shared clock_tick anchor used for
                // BOTH the position estimate and the sleep-timer countdown. Resetting it on a track
                // change would make the sleep timer lose the sub-second gap each track change (drift
                // long). Resetting only play_pos_ms is enough; the next tick adds a normal ~1 s dt.
            }
            set_progress(&mut r.np, r.play_pos_ms, r.cur_duration_ms);
            // Feed the scrobbler on a genuine track change (not a re-poll of the same track).
            if let Some(s) = r.scrob.as_mut() {
                let meta = scrobble::Track {
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    title: r.np.title.clone(),
                    track_no: t.track_no.max(0) as u32,
                    length_s: (t.duration_raw.unwrap_or(0).max(0) / 1000) as u32,
                };
                if !s.is_current(&meta) {
                    s.set_track(meta, now_unix());
                }
            }
            r.last_track = Some(t);
            0
        }
        None => {
            r.np.title = u.rsplit('/').next().unwrap_or(&u).to_string();
            r.np.artist.clear();
            // Unknown track → unknown duration: no estimated bar. And no cover.
            r.cur_duration_ms = 0;
            r.play_pos_ms = 0;
            set_progress(&mut r.np, 0, 0);
            r.last_track = None;
            // A URI the library doesn't know still shows a gradient, and it was being recomputed
            // per frame here exactly as it was for a track with no artwork. Bake it once for this
            // state; the sentinel key keeps a re-poll of the same unresolved URI from rebuilding
            // it, and any real track that follows replaces the key with its object_id.
            if r.art_key != Some(ART_KEY_UNRESOLVED) {
                r.art_full = None;
                r.art_thumb = None;
                bake_gradient_art(r);
                r.art_key = Some(ART_KEY_UNRESOLVED);
            }
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn cinder_set_now_playing(
    title: *const c_char,
    artist: *const c_char,
    codec: *const c_char,
    elapsed: *const c_char,
    remaining: *const c_char,
    progress: f32,
    playing: libc::c_int,
    battery: libc::c_int,
) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        unsafe {
            r.np.title = cstr(title);
            r.np.artist = cstr(artist);
            r.np.codec = cstr(codec);
            r.np.elapsed = cstr(elapsed);
            r.np.remaining = cstr(remaining);
        }
        r.np.progress = progress.clamp(0.0, 1.0);
        r.np.playing = playing != 0;
        r.np.battery = battery.clamp(0, 100) as u8;
        r.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stopped analyzer must not leave its last frame on screen. The bars fall to nothing and the
    /// buffer is dropped, so the visualiser goes absent rather than holding a snapshot — the same
    /// reason the synthetic animation was removed.
    /// The panic hook's screen table must line up with `screen_ord`, or a crash report names the
    /// wrong screen — which is worse than naming none, because it sends the reader somewhere else.
    #[test]
    fn every_screen_has_a_distinct_panic_name() {
        use cinder_ui::nav::Screen as S;
        let all = [
            S::Lock, S::NowPlaying, S::Menu, S::Library, S::Album, S::Artist, S::Playlist, S::UpNext, S::Eq,
            S::Sound, S::Bluetooth, S::Settings, S::Fm, S::UsbDac, S::Receiver, S::Onboarding,
            S::UsbStorage, S::Shelf,
        ];
        assert_eq!(all.len(), SCREEN_NAMES.len(), "table and variant list disagree");
        let mut seen = std::collections::BTreeSet::new();
        for sc in all {
            let i = screen_ord(sc) as usize;
            assert!(i < SCREEN_NAMES.len(), "{sc:?} maps past the end of the name table");
            assert!(seen.insert(i), "{sc:?} shares an ordinal with another screen");
        }
    }

    #[test]
    fn a_stale_spectrum_decays_away_instead_of_freezing() {
        let mut lv = vec![1.0f32, 0.5, 0.25];
        // One 100 ms frame: every bar steps down by 100/400 = 0.25.
        assert!(viz_decay_levels(&mut lv, 100));
        assert!((lv[0] - 0.75).abs() < 1e-6, "got {lv:?}");
        assert!((lv[1] - 0.25).abs() < 1e-6, "got {lv:?}");
        assert!(lv[2] <= 0.0, "the smallest bar should have bottomed out: {lv:?}");
        // Keep going: it must reach empty, not hover just above zero forever.
        for _ in 0..10 {
            viz_decay_levels(&mut lv, 100);
        }
        assert!(lv.is_empty(), "bars never cleared: {lv:?}");
    }

    /// Nothing to decay = nothing to repaint. A visualiser that reported "changed" every frame
    /// while empty would defeat the dirty-flag render and repaint a static screen forever.
    #[test]
    fn decaying_an_empty_spectrum_is_not_a_change() {
        let mut lv: Vec<f32> = Vec::new();
        assert!(!viz_decay_levels(&mut lv, 100));
        let mut lv = vec![0.0f32; 4];
        assert!(viz_decay_levels(&mut lv, 100), "all-zero bars still need clearing once");
        assert!(lv.is_empty());
        assert!(!viz_decay_levels(&mut lv, 100), "and then never again");
    }

    /// A long stall — the panel was dark for an hour — must collapse the bars in one step, not
    /// produce a negative level or a spike when the screen comes back.
    #[test]
    fn a_huge_frame_gap_collapses_the_bars_cleanly() {
        let mut lv = vec![1.0f32, 0.4];
        assert!(viz_decay_levels(&mut lv, 3_600_000));
        assert!(lv.is_empty(), "a long gap should clear outright: {lv:?}");
    }

    #[test]
    fn time_formatting() {
        assert_eq!(fmt_time(0), "0:00");
        assert_eq!(fmt_time(7000), "0:07");
        assert_eq!(fmt_time(272_000), "4:32");
        assert_eq!(fmt_time(3_661_000), "1:01:01");
        assert_eq!(fmt_time(-5), "0:00"); // clamps
    }

    #[test]
    fn codec_from_extension() {
        assert_eq!(codec_label("/music/x.flac", true), ("FLAC · Hi-Res".into(), "FLAC HR".into()));
        assert_eq!(codec_label("/music/x.mp3", false), ("MP3".into(), "MP3".into()));
        assert_eq!(codec_label("/music/noext", false), ("PCM".into(), "PCM".into()));
        assert_eq!(codec_label("/a/b.DSF", true), ("DSF · Hi-Res".into(), "DSF HR".into()));
    }

    #[test]
    fn track_fills_now_playing_and_times() {
        let t = cinder_db::Track {
            object_id: 1,
            title: "Atlas Hands".into(),
            artist: "Benjamin Francis Leftwich".into(),
            album_artist: "Benjamin Francis Leftwich".into(),
            album: "Last Smoke".into(),
            filename: "/music/atlas.flac".into(),
            disc_no: 1,
            track_no: 1,
            duration_raw: Some(272_000),
            is_hires: true,
            album_id: Some(10),
            othumb_id: Some(100),
            added: 5000,
            releaseyear_id: Some(30),
        };
        let mut np = Np::default();
        apply_track(&mut np, &t);
        set_progress(&mut np, 136_000, 272_000); // 50% of 4:32
        assert_eq!(np.title, "Atlas Hands");
        assert_eq!(np.artist, "Benjamin Francis Leftwich");
        assert_eq!(np.badge, "FLAC HR");
        assert_eq!(np.elapsed, "2:16"); // 50% of 4:32
        assert_eq!(np.remaining, "-2:16");
        assert!((np.progress - 0.5).abs() < 1e-6);
    }

    fn fixture_db() -> cinder_db::Db {
        let db = cinder_db::Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
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
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO releaseyears VALUES (30,0,'2012','2012','2012');
            INSERT INTO releaseyears VALUES (31,0,'1992','1992','1992');
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (1,1,1,'Atlas Hands','/music/atlas.flac',1,1,1,10,20,30,5000);
            UPDATE object_body SET albumartist_id=20 WHERE object_id=1;
            -- A GUEST on Last Smoke: its TRACK artist is someone else, its ALBUM artist is not.
            -- Grouping by track artist is what shattered compilations on the real device.
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,albumartist_id,releaseyear_id,addedtime)
              VALUES (2,1,1,'Box of Stones','/music/box.flac',2,1,1,10,21,20,30,5001);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (3,1,1,'Harvest Moon','/music/harvest.flac',1,1,0,11,21,31,4000);
            UPDATE object_body SET albumartist_id=21 WHERE object_id=3;
            -- non-audio rows that must NOT appear in the library (folder mt=0, cover image mt=3)
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,album_id)
              VALUES (7,3,0,'MUSIC','MUSIC',NULL);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,album_id)
              VALUES (8,2,3,'Cover','/music/Cover.jpg',10);
            -- A playlist container + its entries (Sony's real shape: type-1 container, type-3
            -- entries referencing tracks by object_id, ordered by child_index). Object 7 above is
            -- a type-3 row with no parent — an orphan, which must not become a ghost playlist.
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (60,1,0,0,'Night Bus',NULL);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (61,3,60,3,0);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (62,3,60,1,1);
            INSERT INTO object_ext_int VALUES (1,7,272000);
            "#,
            )
            .unwrap();
        db
    }

    #[test]
    fn build_library_from_db() {
        let db = fixture_db();
        let lib = build_library(&db);
        assert_eq!(lib.songs.len(), 3);
        assert_eq!(lib.album_count(), 2);
        assert_eq!(lib.artists.len(), 2);
        // albums grouped under their artist
        let bfl = lib
            .album_groups
            .iter()
            .find(|g| g.artist == "Benjamin Francis Leftwich")
            .unwrap();
        assert_eq!(bfl.albums.len(), 1);
        assert_eq!(bfl.albums[0].name, "Last Smoke");
        assert_eq!(bfl.albums[0].tracks, 2);
        // release year resolved from releaseyears + newest addedtime carried for the ORDER chip
        assert_eq!(bfl.albums[0].year, "2012");
        assert_eq!(bfl.albums[0].added, 5001);
        // the song rows carry the sort keys used by the Songs SORT chip
        let atlas_row = lib.songs.iter().find(|s| s.title == "Atlas Hands").unwrap();
        assert_eq!(atlas_row.year, 2012);
        assert_eq!(atlas_row.album_id, 10);
        // the resolved song carries the object_id the shell plays
        let atlas = lib.songs.iter().find(|s| s.title == "Atlas Hands").unwrap();
        assert_eq!(atlas.object_id, 1);
        // (duration formatting is covered by `track_fills_now_playing_and_times`; the
        // in-memory fixture can't exercise it because Db caches the DURATION akey at open,
        // before this test populates the schema table.)
        // artist track counts
        let bfl_artist = lib.artists.iter().find(|a| a.name == "Benjamin Francis Leftwich").unwrap();
        assert_eq!(bfl_artist.tracks, 2);
        assert_eq!(bfl_artist.albums, 1);
    }

    /// Playlists reach the browsable library, and the orphan type-3 row (object 7, no parent)
    /// does not become a ghost playlist.
    #[test]
    fn build_library_includes_real_playlists() {
        let lib = build_library(&fixture_db());
        assert_eq!(lib.playlists.len(), 1, "one real playlist, no ghosts");
        assert_eq!(lib.playlists[0].name, "Night Bus");
        assert_eq!(lib.playlists[0].id, 60);
        assert_eq!(lib.playlists[0].tracks, 2);
    }

    /// File URIs of a resolved sequence — what the shell actually hands PlayerService.
    fn uris_of(seq: Vec<cinder_db::Track>) -> Vec<String> {
        seq.into_iter().map(|t| t.filename).collect()
    }

    /// The URIs handed to the shell are in saved (child_index) order, NOT title order — this is
    /// what makes a playlist play as the user arranged it.
    #[test]
    fn play_playlist_resolves_uris_in_saved_order() {
        let db = fixture_db();
        let uris: Vec<String> =
            playlist_tracks(Some(&db), 60).unwrap().into_iter().map(|t| t.filename).collect();
        assert_eq!(uris, vec!["/music/harvest.flac".to_string(), "/music/atlas.flac".to_string()]);
    }

    /// Browsing groups by ALBUM ARTIST. The fixture's second track has a different TRACK artist
    /// (a guest) but the same album artist — exactly the shape that shattered compilations on the
    /// real device, where 24 albums spanned several track artists and one DJ mix spanned 26,
    /// producing 26 one-track "albums" under 26 different people.
    #[test]
    fn browsing_groups_by_album_artist_not_track_artist() {
        let lib = build_library(&fixture_db());
        // One album, one artist group — not one group per guest.
        assert_eq!(lib.album_groups.len(), 2, "expected one group per ALBUM artist");
        let smoke = lib
            .album_groups
            .iter()
            .find(|g| g.albums.iter().any(|a| a.name == "Last Smoke"))
            .expect("Last Smoke must appear under an album-artist group");
        assert_eq!(smoke.artist, "Benjamin Francis Leftwich");
        assert_eq!(
            smoke.albums.iter().filter(|a| a.name == "Last Smoke").count(),
            1,
            "the album was split by a guest track artist"
        );
        // The guest IS a real album artist in its own right (it owns Harvest Moon), so it belongs
        // in the tab — but its guest appearance on someone else's album must not credit it with a
        // second album. That is the precise damage track-artist grouping did.
        let guest_artist = lib
            .artists
            .iter()
            .find(|a| a.name == "Cold Stone & Sea")
            .expect("an album artist must appear in the Artists tab");
        assert_eq!(
            guest_artist.albums, 1,
            "a guest appearance was counted as an album: {:?}",
            guest_artist.arts
        );
        // And its track count is its OWN album's, not inflated by the guest track.
        assert_eq!(guest_artist.tracks, 1, "a guest track was counted against the wrong artist");
        // …but the SONG row still credits the guest, which is where it belongs.
        let guest = lib.songs.iter().find(|s| s.title == "Box of Stones").unwrap();
        assert_eq!(guest.artist, "Cold Stone & Sea", "song rows must show the track artist");
    }

    /// Every shuffle scope resolves to a non-empty queue drawn only from real tracks, and none of
    /// them leak the non-audio rows (folder / cover image) the library filters out.
    #[test]
    fn shuffle_scopes_resolve_to_real_tracks() {
        use cinder_ui::nav::ShuffleScope as S;
        let db = fixture_db();
        let all: std::collections::BTreeSet<&str> =
            ["/music/atlas.flac", "/music/box.flac", "/music/harvest.flac"].into_iter().collect();
        for scope in [S::AllSongs, S::ByAlbum, S::ByArtist, S::Playlist] {
            let uris = uris_of(shuffle_tracks(Some(&db), scope)
                .unwrap_or_else(|| panic!("{scope:?} empty")));
            assert!(!uris.is_empty());
            for u in &uris {
                assert!(all.contains(u.as_str()), "{scope:?} produced a non-track: {u}");
            }
        }
        // AllSongs is the whole library; ByAlbum keeps every track too (it only reorders albums).
        assert_eq!(shuffle_tracks(Some(&db), S::AllSongs).unwrap().len(), 3);
        assert_eq!(shuffle_tracks(Some(&db), S::ByAlbum).unwrap().len(), 3);
    }

    /// "TRACKS IN SEQUENCE": ByAlbum may reorder albums but must never split one up or reorder
    /// the tracks inside it.
    #[test]
    fn shuffle_by_album_keeps_albums_intact_and_in_sequence() {
        use cinder_ui::nav::ShuffleScope as S;
        let db = fixture_db();
        // Album 10 = atlas then box (series_no 1,2); album 11 = harvest alone.
        for _ in 0..25 {
            let uris = uris_of(shuffle_tracks(Some(&db), S::ByAlbum).unwrap());
            let atlas = uris.iter().position(|u| u == "/music/atlas.flac").unwrap();
            let boxs = uris.iter().position(|u| u == "/music/box.flac").unwrap();
            assert_eq!(boxs, atlas + 1, "album 10 was split or reordered: {uris:?}");
        }
    }

    /// The toggle has to reach the queue. It used to light an icon and change nothing: with shuffle
    /// showing ON, tapping a track played its album in strict order.
    #[test]
    fn the_shuffle_toggle_actually_reorders_the_queue() {
        let seq: Vec<cinder_db::Track> = (0..40)
            .map(|i| cinder_db::Track {
                filename: format!("/contents/{i:02}.flac"),
                object_id: i,
                ..Default::default()
            })
            .collect();
        let uris = uris_of(seq.clone());

        // Off: byte-for-byte untouched, and the start index is preserved.
        let (off, start) = apply_shuffle(false, seq.clone(), 7);
        assert_eq!(uris_of(off), uris);
        assert_eq!(start, 7);

        // On: the TAPPED track leads (the tap is more specific than the toggle) and playback
        // starts at 0, because the queue was reordered to put it there.
        let (on, start) = apply_shuffle(true, seq.clone(), 7);
        assert_eq!(start, 0);
        let on = uris_of(on);
        assert_eq!(on[0], uris[7], "the track you tapped must still be the one that plays");

        // Same multiset — a shuffle must not drop, duplicate or invent a track.
        let mut a = uris.clone();
        let mut b = on.clone();
        a.sort();
        b.sort();
        assert_eq!(a, b, "shuffle changed which tracks are in the queue");

        // And it is actually shuffled. With 39 tracks behind the leader, the odds of the original
        // order surviving by chance are 1/39! — if this fires, the shuffle did not run.
        assert_ne!(on, uris, "queue came back in its original order");
    }

    /// Degenerate inputs must not panic — a panic here aborts the process, and on this device an
    /// abort is a reboot.
    #[test]
    fn shuffling_degenerate_queues_is_safe() {
        let trk = |n: &str| cinder_db::Track { filename: n.into(), ..Default::default() };
        assert_eq!(apply_shuffle(true, Vec::new(), 0), (Vec::new(), 0));
        let one = vec![trk("/a.flac")];
        assert_eq!(apply_shuffle(true, one.clone(), 0), (one.clone(), 0));
        // A start index past the end (a stale index against a shorter queue) is left alone rather
        // than indexing out of bounds.
        let two = vec![trk("/a.flac"), trk("/b.flac")];
        assert_eq!(apply_shuffle(true, two.clone(), 9), (two, 9));
    }

    /// No DB → no action (rather than an empty queue).
    #[test]
    fn shuffle_without_db_is_ignored() {
        assert!(shuffle_tracks(None, cinder_ui::nav::ShuffleScope::AllSongs).is_none());
    }

    /// Unknown id, or no DB at all → no action, rather than handing the shell an empty sequence.
    #[test]
    fn play_playlist_unknown_is_ignored() {
        let db = fixture_db();
        assert!(playlist_tracks(Some(&db), 999).is_none());
        assert!(playlist_tracks(None, 60).is_none());
    }

    #[test]
    fn empty_title_falls_back_to_filename() {
        let t = cinder_db::Track {
            object_id: 2,
            title: String::new(),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            filename: "/music/box.flac".into(),
            disc_no: 0,
            track_no: 0,
            duration_raw: None,
            is_hires: false,
            othumb_id: None,
            album_id: None,
            added: 0,
            releaseyear_id: None,
        };
        let mut np = Np::default();
        apply_track(&mut np, &t);
        set_progress(&mut np, 0, 0); // unknown duration → blank remaining, zero bar
        assert_eq!(np.title, "box.flac");
        assert_eq!(np.elapsed, "0:00");
        assert_eq!(np.remaining, "");
        assert_eq!(np.progress, 0.0);
    }

    /// The URI copy must report the FULL length, not the copied length, so the caller can tell a
    /// truncated path from a complete one. A truncated path looks perfectly valid and would queue a
    /// file that doesn't exist; the shell relies on `len >= cap` to skip those. Reachable in
    /// practice with deep UTF-8 (CJK) paths, which is why the buffer size is not a safe assumption.
    #[test]
    fn uri_copy_reports_full_length_so_truncation_is_detectable() {
        let mut buf = [0i8; 32];
        let long = "/contents/MUSIC/".to_string() + &"ま".repeat(50) + "/track.flac";
        assert!(long.len() > buf.len(), "test needs a URI longer than the buffer");

        let got = unsafe { copy_str_into(&long, buf.as_mut_ptr(), buf.len() as libc::c_int) };
        assert_eq!(got as usize, long.len(), "must return the FULL length, not the copied one");
        assert!(got >= buf.len() as libc::c_int, "caller must be able to detect truncation");
        assert_eq!(buf[buf.len() - 1], 0, "still NUL-terminated inside the buffer");

        // One that fits reports its own length, which is < cap — the "safe to use" signal.
        let short = "/contents/a.flac";
        let got = unsafe { copy_str_into(short, buf.as_mut_ptr(), buf.len() as libc::c_int) };
        assert_eq!(got, short.len() as libc::c_int);
        assert!(got < buf.len() as libc::c_int);
        let back = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(back.to_str().unwrap(), short);
    }

    /// The liked list is plain text on purpose: a torn line from a power cut must be SKIPPED, not
    /// take the whole list with it. This is a hand-curated list the user cannot otherwise back up.
    #[test]
    fn liked_load_skips_garbage_instead_of_failing() {
        let dir = std::env::temp_dir().join(format!("cinder_liked_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("liked.conf");
        // A realistic torn write: a valid run, a half-written line, a blank, then more valid ids.
        std::fs::write(&p, "101\n202\n\nnot-a-number\n  303  \n\u{0}\n404").unwrap();
        let set = liked_load(p.to_str().unwrap());
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), vec![101, 202, 303, 404]);
        // A missing file is an empty list, not an error — first run has no file.
        assert!(liked_load("/nonexistent/cinder/liked.conf").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_progress_advances_and_clamps() {
        let mut np = Np::default();
        set_progress(&mut np, 60_000, 180_000); // 1:00 of 3:00
        assert_eq!(np.elapsed, "1:00");
        assert_eq!(np.remaining, "-2:00");
        assert!((np.progress - (1.0 / 3.0)).abs() < 1e-4);
        // clamps past the end
        set_progress(&mut np, 999_000, 180_000);
        assert_eq!(np.elapsed, "3:00");
        assert_eq!(np.remaining, "-0:00");
        assert!((np.progress - 1.0).abs() < 1e-6);
    }

    // ── drag-to-seek geometry ────────────────────────────────────────────────────────────────
    // The rail fraction is what turns a finger x into a seek target, so it has to agree exactly
    // with the drawn rail at both ends. An off-by-a-few-pixels mapping is the difference between
    // "seek to the end" and "seek to 99%, then the track ends on its own a moment later".
    #[test]
    fn rail_fraction_maps_the_drawn_rail_and_clamps_outside_it() {
        use cinder_ui::now_playing::{rail_fraction, RAIL_W, RAIL_X0};
        assert_eq!(rail_fraction(RAIL_X0), 0.0);
        assert_eq!(rail_fraction(RAIL_X0 + RAIL_W), 1.0);
        assert!((rail_fraction(RAIL_X0 + RAIL_W / 2) - 0.5).abs() < 1e-3);
        // Outside the rail clamps rather than extrapolating: a thumb landing left of x=24 or
        // right of the end must mean 0% / 100%, never a negative or >1 seek target.
        assert_eq!(rail_fraction(0), 0.0);
        assert_eq!(rail_fraction(479), 1.0);
        assert_eq!(rail_fraction(-500), 0.0);
    }

    // The grab band must be thick enough for a thumb but must never reach the transport row —
    // the play/pause circle is centred at y=692 with radius 44, so anything from 648 is its.
    #[test]
    fn rail_grab_band_is_thumb_sized_and_clears_the_transport_row() {
        use cinder_ui::now_playing::{RAIL_GRAB_BOT, RAIL_GRAB_TOP, RAIL_Y};
        assert!(RAIL_GRAB_TOP <= RAIL_Y, "band must include the rail itself");
        assert!(RAIL_GRAB_BOT >= RAIL_Y + 4);
        assert!(RAIL_GRAB_BOT - RAIL_GRAB_TOP >= 40, "too thin to hit with a thumb");
        assert!(RAIL_GRAB_BOT < 692 - 44, "band overlaps the play/pause target");
    }
}

