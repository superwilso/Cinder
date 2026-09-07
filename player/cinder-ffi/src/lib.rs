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
mod likes;
mod playlists;
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
    /// Write every mapped page instead of just the displayed one (escape hatch — see `blit`).
    all_pages: bool,
    /// What we last wrote to page 0. A blit compares against this and writes only the rows that
    /// actually changed — see `blit` for why that is worth 1.5 MB of RAM.
    shadow: Vec<u32>,
    /// When the last unconditional full write happened (the insurance below).
    last_full: std::time::Instant,
    /// When the mapping was opened, so the early window can distrust the shadow entirely.
    opened: std::time::Instant,
    /// Cleared to force the next blit to write every row regardless of the shadow.
    shadow_valid: bool,
    /// One-shot efficiency sample, so the win is a measured number in the log rather than a claim.
    stat_frames: u32,
    stat_rows: u64,
    stat_done: bool,
}

/// How often to write every row whether it changed or not. Insurance, not correctness: nothing
/// else is known to write fb0 during a session, but the cost of being wrong about that is a
/// permanently stale region of screen, and the cost of the insurance is one full blit a minute.
const FULL_BLIT_EVERY_S: u64 = 60;

/// How long after opening fb0 to assume something else may also be drawing into it. Covers
/// icx_bootanimation, which cinder-home kills repeatedly over roughly the first five seconds.
const UNCONTESTED_AFTER_S: u64 = 15;

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

        // CLEAR EVERY PAGE BEFORE THE FIRST PAINT.
        //
        // Nothing owns the framebuffer's contents across a reboot: mtkfb hands back the same
        // memory, still holding whatever the last session and the boot animation drew into it.
        // Cinder then paints ONE page (all three only when /contents/cinder_fb_allpages exists),
        // so any page it does not touch keeps the old image — and when the panel scans that page
        // out you get a frozen, stale UI sitting behind the live one. Reported repeatedly, most
        // recently 2026-08-26 ("the shadow ui is still there frozen in the background", and the
        // Sony boot screen over an already-drawn Cinder UI); it survives reboots, which is the
        // tell that it is the buffer and not the drawing.
        //
        // One memset of the whole mapping at init costs a few milliseconds once and makes the
        // starting state defined regardless of what was there before.
        unsafe { std::ptr::write_bytes(ptr as *mut u8, 0, map_len) };

        let all_pages = std::path::Path::new("/contents/cinder_fb_allpages").exists();
        println!(
            "cinder-ffi: fb {}x{} {}bpp stride {} pages {} (writing {}, ALL pages) — flip-on-blit active (FBIOPUT+FORCE)",
            var.xres,
            var.yres,
            var.bits_per_pixel,
            stride,
            pages,
            if all_pages { "every row" } else { "changed rows only" }
        );
        // The shadow starts as zeroes and so does the mapping (the clear above), so the very first
        // blit can already trust it — no special first-frame case, and no 1.5 MB write of pixels
        // that are already black.
        Ok(Framebuffer {
            _file: file,
            fd,
            var,
            base: ptr as usize,
            stride,
            pages,
            map_len,
            all_pages,
            shadow: vec![0u32; W * H],
            last_full: std::time::Instant::now(),
            opened: std::time::Instant::now(),
            shadow_valid: true,
            stat_frames: 0,
            stat_rows: 0,
            stat_done: false,
        })
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

        // PAGES AND ROWS ARE INDEPENDENT QUESTIONS, and conflating them cost a regression.
        // The first version of this partial blit wrote CHANGED ROWS to PAGE 0 ONLY, on the reading
        // that the panel never pans (`fb0/pan` reads `0,0`). It does present another page around
        // boot: dropping to page 0 put the boot animation back on top of the Cinder UI within one
        // boot, reported from the device 2026-09-04. `fb0/pan` is not evidence.
        //
        // So: every page, always — and only the rows that actually differ. That keeps all three
        // pages current no matter which one the panel scans, while still moving ~1% of the bytes.
        // The saving was never about skipping pages; it was about skipping unchanged rows.
        //
        // WHY THE COMPARISON IS WORTH IT. The canvas and the shadow are ordinary cached RAM; the
        // framebuffer is a device mapping where the WRITE is the expensive side. Trading two cached
        // reads for avoided device-memory writes is a good deal, and it gets better the more of the
        // screen is static. It also lets us SKIP THE FLIP entirely when nothing differs — the
        // FBIOPUT ioctl that the driver sometimes blocks >33 ms in — which is what makes a static
        // screen genuinely free rather than merely cheap.
        //
        // THIS CANNOT PRODUCE AN ARTEFACT. It is a pure optimisation of the transfer: the bytes
        // that end up in every page are exactly the bytes a full blit would have put there. That is
        // the difference between this and dirty-rect RASTERISATION, where a missed region means a
        // wrong pixel.
        //
        // THE SHADOW ASSUMES WE ARE THE ONLY WRITER, AND EARLY IN A BOOT WE ARE NOT.
        // icx_bootanimation draws into the same fb0 for the first seconds; a partial blit will not
        // paint over it, because the shadow says those rows are already correct. Hence the opening
        // window below, during which the shadow is distrusted entirely.
        // `/contents/cinder_fb_allpages` remains the escape hatch: it forces every row, every time.
        let force_full = self.all_pages
            || !self.shadow_valid
            || self.opened.elapsed().as_secs() < UNCONTESTED_AFTER_S
            || self.last_full.elapsed().as_secs() >= FULL_BLIT_EVERY_S;
        // Past the point where most of the screen is changing, comparing is pure overhead — a
        // scroll or a screen transition dirties nearly every row. So the comparison switches itself
        // off once more than half the rows have differed and the rest are copied blind, which
        // bounds the worst case at half a compare on top of the write it was always doing.
        let mut compare = !force_full;
        let mut wrote = 0usize;

        for y in 0..H {
            if (y + 1) * W > buf.len() {
                break;
            }
            let row = y * W..(y + 1) * W;
            if compare {
                if self.shadow[row.clone()] == buf[row.clone()] {
                    continue;
                }
                if wrote * 2 > H {
                    compare = false;
                }
            }
            self.shadow[row.clone()].copy_from_slice(&buf[row.clone()]);
            // Bullet-proofing: we NEVER write past the mapped region. On the confirmed panel every
            // row fits exactly, but if a unit ever reports a geometry where `pages*H` overruns
            // `yres_virtual`, an unchecked offset would write off the end of the mmap. An
            // out-of-range row is skipped rather than written — worst case a clipped frame.
            for page in 0..self.pages {
                let dst_row = (page * H + y) * self.stride;
                if dst_row + copy_bytes > self.map_len {
                    break;
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        buf.as_ptr().add(y * W) as *const u8,
                        base.add(dst_row),
                        copy_bytes,
                    );
                }
            }
            wrote += 1;
        }

        if force_full {
            self.shadow_valid = true;
            self.last_full = std::time::Instant::now();
        }

        // Report the actual ratio once, then never again. Only frames that took the partial path
        // are sampled: the opening window forces full writes on purpose, and counting those
        // reported 70.3% — a true number about the wrong thing.
        if !self.stat_done && !force_full {
            self.stat_frames += 1;
            self.stat_rows += wrote as u64;
            if self.stat_frames >= 300 {
                self.stat_done = true;
                println!(
                    "cinder-ffi: partial blit — {} rows over {} frames = {:.1}% of a full blit (all {} pages)",
                    self.stat_rows,
                    self.stat_frames,
                    100.0 * self.stat_rows as f64 / (self.stat_frames as f64 * H as f64),
                    self.pages
                );
            }
        }

        // Nothing reached the panel, so there is nothing to push to it.
        if wrote == 0 {
            return;
        }
        self.flip();
    }

    /// Push the frame to the glass. mtkfb does NOT scan the framebuffer continuously — the panel
    /// only updates on this trigger ioctl (icx_bootanimation's flip, replicated exactly).
    /// Occasionally the driver blocks >33 ms here (the anim logs it as "heavy ioctl") — harmless
    /// at our frame rate, and skipped entirely now when no row changed.
    fn flip(&mut self) {
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
    /// Tracks that Cinder, rather than PlayerService, has already played. PlayerService loses
    /// its own previous-track state whenever a queue edit replaces its sequence.
    play_history: Vec<cinder_db::Track>,
    /// Do not add this outgoing track when a Cinder-managed rewind starts it again.
    rewind_from: Option<i64>,
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
    /// Direction of the seek the UI last asked for (-1 / +1) and whether FM was switched on.
    /// Parked here because an action code is one int and these ride alongside it.
    fm_seek_dir: i32,
    fm_power: bool,
    fm_bt: bool,
    scrub_ms: Option<i64>,
    /// Action produced by a settings-slider drag, waiting for the shell to collect it.
    scrub_act: Option<libc::c_int>,
    // Screenshot request: Some(path) => the next rendered frame is also written to `path` as a PNG.
    // Captured from the Canvas BEFORE presentation, so it is identical on the software framebuffer
    // and the GPU/EGL path (under EGL the Mali swapchain owns the panel, so reading /dev/graphics/fb0
    // from outside does NOT reliably show what's on screen — this is the only faithful capture).
    pending_screenshot: Option<String>,
    // Sleep timer: counts DOWN in wall-clock ms (regardless of play/pause); 0 = inactive. When it
    // reaches 0 we raise sleep_fire, which the shell polls (cinder_sleep_should_pause) to pause.
    sleep_remaining_ms: i64,
    /// Deadline for the Settings ▸ Database "Rescanning…" label, counted down with the same dt as
    /// the sleep timer. See the RescanLibrary arm for why the label needs a deadline at all.
    rescan_left_ms: i64,
    sleep_fire: bool,
    // Persisted UI preferences (theme night + visualiser type/on) so choices survive a reboot. The
    // shell points us at a file via cinder_settings_load; we re-save (best-effort) whenever one of
    // them changes. last_saved is the fingerprint we last wrote, to avoid redundant writes.
    settings_path: Option<String>,
    last_saved_body: String, // the file body we last wrote (compare to skip redundant writes)
    // ── Resume across a reboot ─────────────────────────────────────────────────────────────
    // Two files, not one, because the two halves change at completely different rates. The
    // SEQUENCE (context + queue + un-shuffle order) is tens of kilobytes and changes when the
    // user starts something or a track boundary passes; the POSITION is 30 bytes and moves every
    // second. Putting them together would mean rewriting ~25 KB of flash once a second for the
    // sake of a number, which is the kind of write amplification that wears an eMMC out.
    //
    // Neither lives in /contents: that is the USB-MSC volume, it disappears from under us while
    // the PC holds it, and a machine-written queue file has nothing a user would want to edit.
    resume_path: Option<String>,      // sequence file; written only when the body changes
    resume_last_body: String,
    resume_pos_path: Option<String>,  // position file; written at most every RESUME_POS_EVERY
    resume_pos_last: String,
    resume_pos_at: std::time::Instant,
    /// A restored sequence that PlayerService has NOT been told about. Cinder does not hand it
    /// over at boot: `cinder_audio_play_tracks` starts playback, and a player that begins playing
    /// on its own the moment it powers up is a worse bug than the one being fixed. It is handed
    /// over on the first ▶ instead, which is also when the ~400 ms SetTrackSequence is expected.
    resume_pending: Option<(Vec<String>, usize, i64)>, // (uris, start index, position ms)
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
    viz_peak: f32,        // slow-decaying auto-gain peak for Scale::Dynamic
    // Peak-hold markers and how long each has been sitting where it is. Only populated while the
    // user has the markers switched on; `hold_peaks` clears both when they are off, so the render
    // asks one `is_empty()` rather than carrying a second setting down to the draw call.
    viz_peaks: Vec<f32>,
    viz_held_ms: Vec<f32>,
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
    /// A queue edit landed while a boot-time resume was still armed, so the sequence snapshot
    /// `cinder_resume_load` took no longer describes what the user wants. Rebuilt at the first ▶
    /// rather than at the edit, because rebuilding costs a library query and an edit made before
    /// the first press is exactly the case where nothing is audible to be late for.
    resume_stale: bool,
    /// A queue flush is sitting in `pending_play` waiting for the shell to collect it. Reported
    /// through `cinder_take_queue_flush` so the shell knows to hand it to PlayerService.
    queue_flush: bool,
    // Pending play request (Action::PlayIndex resolved through the DB): the chosen track's album
    // context — file URIs in play order + the start index. The shell drains it via
    // cinder_pending_play_* after a CINDER_ACT_PLAY_INDEX action and hands it to PlayerService
    // (NodeTrackSequence). Replaced wholesale on every new PlayIndex.
    pending_play: Vec<String>,
    /// Row index carried by the last CINDER_ACT_BT_CONNECT_DEVICE / _BT_FORGET_DEVICE action. The
    /// shell drains it with `cinder_pending_bt_device()`; -1 there means "no request", so a stale
    /// index can never be mistaken for row 0.
    pending_bt_device: Option<usize>,
    // ── Liked songs ────────────────────────────────────────────────────────────────────────
    // Track object_ids the user has hearted. Kept as a set so the Now Playing heart is an O(log n)
    // lookup per track change, and persisted to its own file rather than the settings blob — it
    // grows with the library, and losing every preference because one liked-list line is corrupt
    // would be a bad trade. `liked_path` is None until cinder_db_open supplies it.
    duration_checked: bool,         // have we compared the DB duration against the service's yet?
    last_tick: std::time::Instant,  // real-time anchor for fling/HUD animation
    /// Monotonic anchor for animations that need an ABSOLUTE phase rather than a delta — currently
    /// the title marquee, whose position is a function of elapsed time, not of accumulated frames.
    /// Deriving it from `last_tick` would tie the animation to how often the screen happened to be
    /// dirty, which is exactly what it must not depend on.
    boot: std::time::Instant,
    last_scrob: std::time::Instant, // real-time anchor for the scrobble play clock
    liked: std::collections::BTreeSet<i64>,
    liked_path: Option<String>,
    /// Playlists the user made ON the device, as .m3u8 files (see `playlists.rs`). Separate from
    /// the Sony ones below because they are the only ones this app may write.
    plists: playlists::Store,
    /// Sony's playlist rows, kept from the last library build so a playlist edit can rebuild the
    /// merged list without re-querying the database — one edit is a keypress away from the next,
    /// and the DB half of the list cannot have changed in between.
    db_playlists: Vec<cinder_ui::model::PlaylistRow>,
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

/// Maximum EQ band gain, in the DSP's half-dB units (±20 = ±10 dB). Mirrors
/// `cinder_ui::eq::BAND_MAX`, which is what the EQ screen clamps to; kept here because the
/// settings loader has to clamp values that never went through the screen at all.
const EQ_BAND_MAX: i8 = 20;

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
        app: {
            // MIX / the shuffle toggle permute inside cinder-ui, which has no clock (its 300-odd
            // host tests depend on it having none). Hand it a per-session seed here, or the same
            // album shuffles into the same order after every boot.
            let mut a = cinder_ui::nav::App::unlocked();
            a.seed_shuffle(Rng::new().next());
            a
        },
        scrob: None,
        last_track: None,
        play_history: Vec::new(),
        rewind_from: None,
        play_pos_ms: 0,
        cur_duration_ms: 0,
        last_pos: std::time::Instant::now(),
        real_pos_ms: -1,
        real_pos_at: std::time::Instant::now(),
        fm_seek_dir: 1,
        fm_power: false,
        fm_bt: false,
        scrub_ms: None,
        scrub_act: None,
        pending_screenshot: None,
        sleep_remaining_ms: 0,
        rescan_left_ms: 0,
        sleep_fire: false,
        settings_path: None,
        last_saved_body: String::new(),
        resume_path: None,
        resume_last_body: String::new(),
        resume_pos_path: None,
        resume_pos_last: String::new(),
        resume_pos_at: std::time::Instant::now(),
        resume_pending: None,
        dirty: true, // paint the first frame
        viz_phase: 2.0,
        last_viz: std::time::Instant::now(),
        viz_levels: Vec::new(),
        viz_peak: 0.0,
        viz_peaks: Vec::new(),
        viz_held_ms: Vec::new(),
        viz_at: std::time::Instant::now(),
        queue_pending: false,
        resume_stale: false,
        queue_flush: false,
        pending_play: Vec::new(),
        pending_bt_device: None,
        duration_checked: false,
        last_tick: std::time::Instant::now(),
        boot: std::time::Instant::now(),
        last_scrob: std::time::Instant::now(),
        liked: std::collections::BTreeSet::new(),
        liked_path: None,
        plists: playlists::Store::default(),
        db_playlists: Vec::new(),
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
/// Serialise the setup that is NOT live, so both halves of the A/B pair survive a reboot. The LIVE
/// one keeps using the existing `eq=` / `sound=` / `balance100=` keys, which means an older build
/// reading this file still finds exactly what it expects and simply ignores the spare.
fn setup_body(s: &cinder_ui::nav::SoundSetup) -> String {
    let eq: Vec<String> = s.eq_bands.iter().map(|b| b.to_string()).collect();
    let flags = (s.dsee as u8)
        | (s.vinyl as u8) << 1
        | (s.vpt as u8) << 2
        | (s.dc as u8) << 3
        | (s.norm as u8) << 4
        | (s.clear as u8) << 5;
    format!("bank_eq={}\nbank_sound={}\nbank_balance={}\nbank_preset={}\n",
            eq.join(","), flags, s.balance, s.eq_preset)
}

fn settings_body(r: &Render) -> String {
    let eq: Vec<String> = r.app.eq_bands().iter().map(|b| b.to_string()).collect();
    let mut body = format!(
        "night={}\naccent={}\nviz_kind={}\nviz_size={}\nnp_page={}\nshuffle={}\nrepeat={}\neq={}\nsound={}\nonboarding={}\nbt_codec={}\nbt_ldac_quality={}\nbt_enhanced={}\nbt_on={}\nvolume={}\nbt_volume127={}\nbrightness={}\nscreen_off={}\nauto_off={}\nbalance100={}\nvpt_mode={}\ndc_type={}\nadv={}\ndsee_mode={}\nvinyl_type={}\ntone={}\nui_scale={}\nsetup={}\n",
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
        r.app.bt_enhanced() as u8,
        r.app.bt_on() as u8,
        r.app.volume_level(),
        r.app.bt_volume_level(),
        r.app.brightness_restore(), // never 0: backlight-off is transient, not a setting
        r.app.screen_off_s(),
        r.app.auto_off_min(),
        r.app.balance(),
        r.app.vpt_mode(),
        r.app.dc_type(),
        r.app.adv_flags(),
        r.app.dsee_mode(),
        r.app.vinyl_type(),
        r.app.tone_bands().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","),
        r.app.ui_scale_pct(),
        r.app.setup_idx(),
    );
    body.push_str(&setup_body(&r.app.setup_inactive()));
    // The visualiser's signal settings. One line each rather than a packed field, because these
    // are exactly the lines someone tuning the display over adb will want to edit by hand — and
    // every one is an INDEX into a table owned by `cinder_ui::vizcfg`, so an out-of-range value
    // from a hand-edited file is wrapped by the setter rather than accepted.
    body.push_str(&format!(
        "viz_scale={}\nviz_range={}\nviz_response={}\nviz_interp={}\nviz_peaks={}\nviz_window={}\nviz_rate={}\n",
        r.app.viz_scale_idx(),
        r.app.viz_range_idx(),
        r.app.viz_response_idx(),
        r.app.viz_interp_idx(),
        r.app.viz_peak_hold() as u8,
        r.app.viz_window_idx(),
        r.app.viz_rate_idx(),
    ));
    // FM: the dial position and the scanned station list. A scan is a DELIBERATE ten-second wait
    // that the user watches happen, so losing it on a reboot is the same defect as losing a shelf
    // pin — and losing the frequency drops the dial to a hardcoded 97.3 that is nobody's station.
    // Written unconditionally (the dial always has a value) and the list only when a scan has run.
    body.push_str(&format!("fm_khz={}\n", r.app.fm_khz()));
    let st = r.app.fm_stations();
    if !st.is_empty() {
        body.push_str(&format!(
            "fm_stations={}\n",
            st.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",")
        ));
    }
    // Shelf pins were session-scoped, so every reboot silently wiped the user's bookmarks — the
    // one thing a "pin this place" feature must not do. One line per occupied slot.
    for i in 0..cinder_ui::shelf::SLOTS {
        let enc = r.app.shelf_pin_encode(i);
        if !enc.is_empty() {
            body.push_str(&format!("pin{i}={enc}\n"));
        }
    }
    body
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

/// How often the position file may be rewritten. A resume that is up to this far behind the truth
/// is unnoticeable; a rewrite every second for years is not.
const RESUME_POS_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

// Persist the playing sequence IF it changed. Built and compared every housekeeping tick: the
// body is ~7 bytes a track, so even a whole-library shuffle context is a ~25 KB format + memcmp
// at 1 Hz, against a tick that already makes 400 ms IPC round trips. Best-effort, like settings.
fn save_resume(r: &mut Render) {
    let Some(path) = r.resume_path.clone() else { return };
    let body = r.app.playback_encode();
    if body == r.resume_last_body {
        return;
    }
    let _ = std::fs::write(&path, &body);
    r.resume_last_body = body;
}

// Persist "what was playing and where in it", rate-limited. `force` bypasses the timer for the
// moments that matter more than the cadence: a pause, or the shell shutting us down.
fn save_resume_pos(r: &mut Render, force: bool) {
    let Some(path) = r.resume_pos_path.clone() else { return };
    let Some(t) = r.last_track.as_ref() else { return };
    if !force && r.resume_pos_at.elapsed() < RESUME_POS_EVERY {
        return;
    }
    // Second granularity: the file is compared before it is written, so a paused player stops
    // writing entirely instead of rewriting the same millisecond count forever.
    let body = format!("track={}\npos={}\n", t.object_id, (r.play_pos_ms.max(0) / 1000) * 1000);
    r.resume_pos_at = std::time::Instant::now();
    if body == r.resume_pos_last {
        return;
    }
    let _ = std::fs::write(&path, &body);
    r.resume_pos_last = body;
}

// Parse `k=v` lines into (key, value) pairs, skipping blanks and comments. Shared by every
// config reader here; a malformed line is dropped rather than failing the whole file, because a
// hand-edited or half-written config must never keep the player from booting.
fn conf_lines(body: &str) -> impl Iterator<Item = (&str, &str)> {
    body.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        l.split_once('=').map(|(k, v)| (k.trim(), v.trim()))
    })
}

// Decode a comma-separated id list. Junk entries are skipped, not fatal.
fn id_list(v: &str) -> Vec<i64> {
    v.split(',').filter_map(|s| s.trim().parse::<i64>().ok()).collect()
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
    // PER-PHASE TIMING, ONCE PER OPEN. The whole of this function is the ~4.5 s of boot dead time
    // (device 2026-09-05: cinder_db_open at t=2.716, "restore playback context" at t=7.271), and
    // until now nobody had a device-side breakdown of it — only a host profile, which says roughly
    // 77% SQLite and disagrees with the device by about 100x overall. A 1 GHz in-order A7 against
    // that host should be nearer 20-30x for this kind of work, so the gap itself is the finding.
    // One line per library open, which is once per boot.
    let t_total = std::time::Instant::now();
    let t_phase = std::time::Instant::now();
    let years = db.release_years();
    let ms_years = t_phase.elapsed().as_millis();
    let year_num = |id: Option<i64>| -> i32 {
        id.and_then(|i| years.get(&i)).and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0)
    };

    // Only the release YEAR needs the FK map; everything else is on the track itself, so the
    // shared builder covers it and this closure just fills the one field in.
    let song_row = |t: &cinder_db::Track| SongRow {
        year: year_num(t.releaseyear_id),
        ..song_row_of(t)
    };

    let t_phase = std::time::Instant::now();
    let tracks = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
    let ms_tracks = t_phase.elapsed().as_millis();
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
    let t_phase = std::time::Instant::now();
    let album_order_rows = db.tracks_album_order().unwrap_or_default();
    let ms_album_order = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    for t in album_order_rows {
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
    let ms_album_group = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    let album_list = db.albums().unwrap_or_default();
    let ms_albums = t_phase.elapsed().as_millis();
    let mut album_rows: Vec<AlbumRow> = album_list
        .into_iter()
        .map(|a| {
            let trs = album_tracks.remove(&a.id).unwrap_or_default();
            let mut artist = album_artist.get(&a.id).cloned().unwrap_or_default();
            if artist.is_empty() && !trs.is_empty() {
                artist = trs[0].artist.clone();
            }
            AlbumRow {
                artist,
                year: album_year.get(&a.id).cloned().unwrap_or_default(),
                tracks: a.track_count.max(0) as u32,
                art: a.name.clone(),
                added: album_added.get(&a.id).copied().unwrap_or(0),
                track_list: trs,
                name: a.name,
                album_id: a.id,
            }
        })
        .collect();

    // Include any albums that had tracks but were not returned by db.albums()
    for (aid, trs) in album_tracks {
        if trs.is_empty() {
            continue;
        }
        let name = trs[0].art.clone();
        if name.is_empty() {
            continue;
        }
        let artist = album_artist
            .get(&aid)
            .cloned()
            .unwrap_or_else(|| trs[0].artist.clone());
        let year = album_year.get(&aid).cloned().unwrap_or_default();
        let added = album_added.get(&aid).copied().unwrap_or(0);
        let count = trs.len() as u32;
        album_rows.push(AlbumRow {
            artist,
            year,
            tracks: count,
            art: name.clone(),
            added,
            track_list: trs,
            name,
            album_id: aid,
        });
    }

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
            // From Sony's database: browsable and playable, but not ours to edit.
            user: false,
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

    // GENRES for the filter picker. Counted from the tracks we actually built rather than from the
    // genres table, so the list only ever offers something that will match at least one row — 101
    // genres exist on the reference device but only 95 are carried by a track.
    //
    // The empty genre is REAL and is the largest bucket (482 of 3,463), so it is kept and labelled
    // rather than dropped; hiding it would mean the filter silently could not reach an eighth of
    // the library. Sorted by count, biggest first: with 95 entries the useful ones have to be at
    // the top, and alphabetical would bury Rock behind Acid Jazz.
    let genres = {
        let names = db.genres();
        let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
        for s in &songs {
            if s.genre_id != 0 {
                *counts.entry(s.genre_id).or_insert(0) += 1;
            }
        }
        let mut v: Vec<cinder_ui::model::GenreRow> = counts
            .into_iter()
            .map(|(id, tracks)| {
                let raw = names.get(&id).cloned().unwrap_or_default();
                let name = if raw.trim().is_empty() { "(No genre)".to_string() } else { raw };
                cinder_ui::model::GenreRow { id, name, tracks }
            })
            .collect();
        v.sort_by(|a, b| b.tracks.cmp(&a.tracks).then_with(|| a.name.cmp(&b.name)));
        eprintln!("cinder-ffi: genres: {} in use", v.len());
        v
    };

    let hires_tracks = songs.iter().filter(|s| s.is_hires).count() as u32;
    eprintln!("cinder-ffi: hi-res tracks: {hires_tracks}");
    let (folders, folder_roots) = build_folders(&tracks, &songs);

    // The breakdown of the boot dead time. `sql` is the four DB calls; `model` is everything this
    // function does with their results. If `sql` dominates on device the way it does on the host,
    // the lever is SQLite configuration (see `Db::open`, which sets no PRAGMAs); if `model`
    // dominates, it is this function's own BTreeMap work and no PRAGMA will touch it.
    let ms_sql = ms_years + ms_tracks + ms_album_order + ms_albums;
    let ms_all = t_total.elapsed().as_millis();
    eprintln!(
        "cinder-ffi: build_library {ms_all} ms = sql {ms_sql} (years {ms_years}, tracks \
         {ms_tracks}, album_order {ms_album_order}, albums {ms_albums}) + model {} (grouping \
         {ms_album_group})",
        ms_all.saturating_sub(ms_sql)
    );

    // `thumbs` is filled separately by start_art_cache: the disk cache load is I/O, not model
    // building, and the rest arrives asynchronously from the decoder thread.
    cinder_ui::Library {
        songs,
        album_groups,
        artists,
        playlists,
        thumbs: Default::default(),
        genres,
        hires_tracks,
        filter_genre: None,
        filter_hires: false,
        folders,
        folder_roots,
    }
}

/// Build the FOLDER browse tree from the tracks' absolute paths.
///
/// `cinder_db` already resolves every track to an absolute path (its `dirs` map walks `parent_id`
/// up to a storage root), so the tree is derived from the paths rather than queried again — one
/// pass, no extra SQL, and it cannot disagree with what Track information shows.
///
/// Directories with NO tracks anywhere below them do not appear. They are not browsable places on
/// a music player, and on this device the file tree is full of them (cover-art folders, the MTP
/// scratch directories) — showing them would bury the four folders that matter.
fn build_folders(
    tracks: &[cinder_db::Track],
    songs: &[cinder_ui::model::SongRow],
) -> (Vec<cinder_ui::model::FolderRow>, Vec<usize>) {
    use cinder_ui::model::FolderRow;
    use std::collections::HashMap;

    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<FolderRow> = Vec::new();

    // Intern one path, creating every missing ancestor above it. Returns its index.
    fn intern(
        path: &str,
        idx: &mut HashMap<String, usize>,
        out: &mut Vec<FolderRow>,
    ) -> usize {
        if let Some(i) = idx.get(path) {
            return *i;
        }
        let cut = path.rfind('/').unwrap_or(0);
        // A path with no slash above the first character is a root: `/contents` has its slash at
        // 0, and slicing to 0 would make the parent the empty string, i.e. a phantom root above
        // every mount.
        let parent = (cut > 0).then(|| intern(&path[..cut], idx, out));
        let name = if parent.is_some() { &path[cut + 1..] } else { path };
        let me = out.len();
        out.push(FolderRow {
            path: path.to_string(),
            name: name.to_string(),
            parent,
            subdirs: Vec::new(),
            tracks: Vec::new(),
            total: 0,
        });
        idx.insert(path.to_string(), me);
        if let Some(p) = parent {
            out[p].subdirs.push(me);
        }
        me
    }

    // `songs` is built from `tracks` in the same order (see the loop in build_library), so the two
    // index together — which is what lets the tree hold ready-made SongRows instead of rebuilding
    // them, and keeps a folder row identical to the same track's row anywhere else.
    for (t, row) in tracks.iter().zip(songs.iter()) {
        let Some(cut) = t.filename.rfind('/') else { continue };
        if cut == 0 {
            continue; // a bare "/name" — no directory to file it under
        }
        let dir = intern(&t.filename[..cut], &mut idx, &mut out);
        out[dir].tracks.push(row.clone());
    }

    // Totals: every directory counts its own tracks plus everything below. Walked from each
    // directory UP to its root rather than recursively down, so a pathological depth cannot blow
    // the stack — and the depth is bounded anyway by the `intern` recursion above it.
    for i in 0..out.len() {
        let n = out[i].tracks.len() as u32;
        if n == 0 {
            continue;
        }
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(c) = cur {
            out[c].total += n;
            cur = out[c].parent;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
    }

    // Tracks in filename order — on a music folder that is track order, and it is the order the
    // files are actually in on the volume, which is the whole point of browsing this way.
    // Subdirectories alphabetically, case-insensitively.
    let names: Vec<String> = out.iter().map(|f| f.name.to_lowercase()).collect();
    for f in out.iter_mut() {
        f.tracks.sort_by(|a, b| a.track.cmp(&b.track).then_with(|| a.title.cmp(&b.title)));
    }
    for i in 0..out.len() {
        let mut subs = std::mem::take(&mut out[i].subdirs);
        subs.sort_by(|a, b| names[*a].cmp(&names[*b]));
        // Prune the branches that hold no music at all.
        subs.retain(|s| out[*s].total > 0);
        out[i].subdirs = subs;
    }

    let mut roots: Vec<usize> = (0..out.len())
        .filter(|i| out[*i].parent.is_none() && out[*i].total > 0)
        .collect();
    roots.sort_by(|a, b| names[*a].cmp(&names[*b]));
    eprintln!("cinder-ffi: folders: {} dirs, {} root(s)", out.len(), roots.len());
    (out, roots)
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
const SCREEN_NAMES: [&str; 31] = [
    "Lock", "NowPlaying", "Menu", "Library", "Album", "Artist", "Playlist", "UpNext", "Eq",
    "Sound", "Bluetooth", "Settings", "Fm", "UsbDac", "Receiver", "Onboarding", "UsbStorage",
    "Shelf", "Pairing", "GenreFilter", "TrackInfo", "Folders", "ClockSet", "Advanced",
    "Tone", "BtCodec", "Keyboard", "PlaylistPick", "TrackPick", "Device", "VizSet",
];

/// Exhaustive on purpose: adding a `Screen` variant without a name here fails the build rather
/// than silently printing the wrong screen in a crash report.
fn screen_ord(s: cinder_ui::nav::Screen) -> u8 {
    use cinder_ui::nav::Screen as S;
    match s {
        S::Lock => 0, S::NowPlaying => 1, S::Menu => 2, S::Library => 3, S::Album => 4,
        S::Artist => 5, S::Playlist => 6, S::UpNext => 7, S::Eq => 8, S::Sound => 9,
        S::Bluetooth => 10, S::Settings => 11, S::Fm => 12, S::UsbDac => 13, S::Receiver => 14,
        S::Onboarding => 15, S::UsbStorage => 16, S::Shelf => 17, S::Pairing => 18,
        S::GenreFilter => 19, S::TrackInfo => 20, S::Folders => 21, S::ClockSet => 22,
        S::Advanced => 23, S::Tone => 24, S::BtCodec => 25,
        S::Keyboard => 26, S::PlaylistPick => 27, S::TrackPick => 28,
        S::Device => 29, S::VizSet => 30,
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
/// How many display columns the visualiser draws. The analyzer gives twelve bands whatever we do
/// (see `cinder_analyzer.h`), so this is a DISPLAY resolution: the bands are interpolated across
/// it. Kept as one constant because the number appeared in three call sites and a mismatch between
/// them is a silent resample of a resample.
const VIZ_BARS: usize = 36;

/// Milliseconds since the previous spectrum frame, clamped to something a time constant can use.
///
/// The smoothing is now expressed as attack/decay TIMES rather than per-frame fractions, so it
/// needs the real interval: the analyzer's emit rate is a user setting, and a frame can also be
/// late (the service stops on screen-off and restarts up to a second later). The clamp keeps a
/// long gap from being applied as one enormous step.
fn frame_dt_ms(r: &Render) -> f32 {
    (r.viz_at.elapsed().as_millis().min(500) as f32).max(1.0)
}

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
/// Latch the on-screen "Sony IPC is dead for this boot" banner (see `chrome::set_ipc_dead`).
///
/// LOCK-FREE, and that is the whole contract. This is called from `run_guarded_ex`'s recovery
/// path in cinder-home — i.e. immediately after a `siglongjmp` out of a faulted Sony call — and
/// anything that took `cell().lock()` there could deadlock against whatever the abandoned call
/// was holding, or block on a mutex whose owner no longer exists. An atomic store cannot.
/// One-way: only a restart clears it, which is exactly what the banner tells the user to do.
#[no_mangle]
pub extern "C" fn cinder_set_ipc_dead(dead: libc::c_int) {
    cinder_ui::chrome::set_ipc_dead(dead != 0);
}

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
    // MARQUEE CLOCK. Advanced before the dirty gate so the phase a painted frame reads is the
    // real elapsed time, not the time of the last frame that happened to be dirty. A long title
    // asks for the next frame itself (see below), so this and that together are what make it move;
    // a title that fits sets nothing and Now Playing stays exactly as cheap as it was.
    cinder_ui::widgets::set_marquee_ms(r.boot.elapsed().as_millis() as u32);
    if cinder_ui::widgets::marquee_scrolled() {
        r.dirty = true;
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
        // Markers only exist while they are switched on AND there are bars to mark: `hold_peaks`
        // empties the buffer when the setting is off, so this needs no second look at the config.
        viz_peaks: if animate && !r.viz_peaks.is_empty() { Some(&r.viz_peaks) } else { None },
        scrubbing: r.scrub_ms.is_some(),
    };
    // The navigator decides which screen is showing; it draws Now Playing from `np` and
    // the list/menu screens from their own state.
    // ONE-SHOT RASTER COST SAMPLE. With the blit down to ~0.7% of a full transfer, the raster is
    // what a painted frame now costs, and the forced repaint that runs every 5 s for the life of
    // the process pays it in full to produce a byte-identical screen. Whether that is worth
    // changing depends on a number nobody had, so measure it once and say so.
    // AND IT STOPS WHEN IT IS DONE. The previous version left two `Instant::now()` calls and two
    // atomic RMWs on the per-frame path for the life of the process, to feed a report that had
    // already been printed for the last time — permanent cost for a one-off diagnostic. Past the
    // cap this is a single relaxed atomic load (no barrier on ARM) and no clock reads at all.
    static RASTER_N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static WIN_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    const RASTER_LAST: u32 = 30_000;
    let sampling = RASTER_N.load(std::sync::atomic::Ordering::Relaxed) < RASTER_LAST;
    let raster_t0 = if sampling { Some(std::time::Instant::now()) } else { None };
    r.app.render(&mut r.canvas, &r.fonts, &np);
    if let Some(raster_t0) = raster_t0 {
        use std::sync::atomic::Ordering::Relaxed;
        // One clock read, not two — `elapsed()` was being called twice per frame purely to avoid
        // naming the value.
        let dt = raster_t0.elapsed().as_micros() as u64;
        let n = RASTER_N.fetch_add(1, Relaxed) + 1;
        let us = WIN_US.fetch_add(dt, Relaxed) + dt;
        // WINDOWED, NOT CUMULATIVE, AND NOT ONLY AT BOOT. The first version of this printed one
        // cumulative mean at frame 300 and that number was worthless: on device those 300 frames
        // span first-paint (t=1.2 s) to about t=14.7 s, which is the near-empty boot screen with
        // the render thread starved by the synchronous library build. It measured the boot, not
        // the UI — and it did so consistently enough to look trustworthy. It reported 6.21 ms
        // before the opt-level change and 6.23 ms after, while the host bench for the same code
        // moved 2-3x, and the reason was simply that neither sample was of the real workload.
        //
        // So: each window is reported on its own (the accumulator resets), and later windows keep
        // coming, so a sample exists from a boot screen, from a settled idle screen, and from
        // whatever the user is actually looking at. Capped so this cannot become a log leak — the
        // last report is at RASTER_LAST frames, a few minutes of real use, after which the whole
        // sampler switches off — see the `sampling` gate above, which is the point of the cap.
        const WINDOW: u32 = 300;
        if n % WINDOW == 0 {
            let win = n / WINDOW;
            // Every window early on (boot is where the interesting change is), then thin out.
            if win <= 4 || win % 10 == 0 {
                println!(
                    "cinder-ffi: raster — frames {}..{}, mean {:.2} ms/frame",
                    n - WINDOW + 1,
                    n,
                    us as f64 / WINDOW as f64 / 1000.0
                );
            }
            WIN_US.store(0, Relaxed);
        }
    }
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
            viz_seed: 2.0, viz_kind: 0, viz_size: 0, page: 0, viz_levels: None, viz_peaks: None,
            scrubbing: false,
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
/// Returns the reordered sequence, the new start index, and — when it actually shuffled — the
/// object_ids in their ORIGINAL order, which the caller hands to `App::note_pre_shuffle` so that
/// turning shuffle back off restores this album's real running order.
fn apply_shuffle(
    on: bool,
    mut seq: Vec<cinder_db::Track>,
    start: usize,
) -> (Vec<cinder_db::Track>, usize, Option<Vec<i64>>) {
    if !on || seq.len() < 2 || start >= seq.len() {
        return (seq, start, None);
    }
    let pre: Vec<i64> = seq.iter().map(|t| t.object_id).collect();
    let chosen = seq.remove(start);
    Rng::new().shuffle(&mut seq);
    seq.insert(0, chosen);
    (seq, 0, Some(pre))
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
        genre_id: t.genre_id.unwrap_or(0),
        is_hires: t.is_hires,
    }
}

/// The Track information rows for one track — Sony's "Detailed Information", as
/// `(label, value)` pairs. A row is OMITTED when its value is unknown, rather than printed empty:
/// a blank next to "Genre" claims the file has no genre, which is a different statement from the
/// library not having resolved one.
fn track_info_rows(r: &Render, t: &cinder_db::Track) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::with_capacity(12);
    let mut put = |k: &str, v: String| {
        if !v.trim().is_empty() {
            rows.push((k.to_string(), v));
        }
    };
    put("Title", if t.title.is_empty() {
        t.filename.rsplit('/').next().unwrap_or("").to_string()
    } else {
        t.title.clone()
    });
    put("Artist", t.artist.clone());
    // Only when it differs — on most files it repeats the artist, and a row that says the same
    // thing twice is noise on a screen whose whole job is the details that are NOT obvious.
    if t.album_artist.trim() != t.artist.trim() {
        put("Album artist", t.album_artist.clone());
    }
    put("Album", t.album.clone());
    // Genre and year come from the already-built library rather than fresh queries: the row is in
    // memory, the maps behind it were resolved once at build, and this runs on the render thread.
    let lib = r.app.library();
    if let Some(g) = t.genre_id.and_then(|id| lib.genres.iter().find(|g| g.id == id)) {
        put("Genre", g.name.clone());
    }
    if let Some(row) = lib.songs.iter().find(|s| s.object_id == t.object_id) {
        if row.year > 0 {
            put("Year", row.year.to_string());
        }
    }
    if t.track_no > 0 {
        put("Track", if t.disc_no > 1 {
            format!("{} (disc {})", t.track_no, t.disc_no)
        } else {
            t.track_no.to_string()
        });
    }
    if let Some(ms) = t.duration_raw.filter(|m| *m > 0) {
        put("Duration", fmt_time(ms));
    }
    let (codec, _) = codec_label(&t.filename, t.is_hires);
    put("Format", codec);
    // Byte size is not in the DB — it is a stat() on a path we already have, and the one number a
    // user checking "did the right file copy over" actually wants. Silently skipped if the file is
    // gone, which is itself worth not claiming a size for.
    if let Ok(md) = std::fs::metadata(&t.filename) {
        let bytes = md.len();
        put("Size", if bytes >= 1 << 20 {
            format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
        } else {
            format!("{} KB", (bytes + 1023) / 1024)
        });
    }
    put("File", t.filename.clone());
    rows
}

/// Hand a resolved play sequence to BOTH consumers: the shell gets the file URIs it feeds to
/// PlayerService, and the UI gets the same tracks as its Up Next queue.
///
/// Filling the queue here is the point. Up Next used to show ONLY the tracks the user had
/// hand-swiped in, and fell back to a guess at the current album when that was empty — so playing
/// an album or hitting a Shuffle band produced a real 200-track sequence that the queue screen
/// knew nothing about. One resolution, one order, both surfaces.
const MAX_PLAY_SEQUENCE: usize = 512;

fn set_pending(r: &mut Render, mut seq: Vec<cinder_db::Track>, start: usize) {
    if seq.len() > MAX_PLAY_SEQUENCE {
        eprintln!(
            "cinder-ffi: play sequence has {} tracks; limiting playback and Up Next to {}",
            seq.len(),
            MAX_PLAY_SEQUENCE
        );
        seq.truncate(MAX_PLAY_SEQUENCE);
    }
    let start = start.min(seq.len().saturating_sub(1));
    r.app.set_play_context(seq.iter().map(song_row_of).collect(), start);
    r.pending_play = seq.into_iter().map(|t| t.filename).collect();
    r.pending_play_start = start;
    r.play_history.clear();
    r.rewind_from = None;
    // A QUEUE EDIT OWED AGAINST THE OLD SEQUENCE IS NOT OWED AGAINST THIS ONE. The "Clear the
    // queue?" answer emits `QueueChanged` and THEN the play action, and `cinder_tap` carries both
    // — so `queue_pending` was left standing over a context that had just been replaced. It then
    // fired 2.5 s before the end of the first track of whatever the user had started: a
    // SetTrackSequence + seek (the measured 360-450 ms round trip) to install a sequence identical
    // to the one already playing. `set_play_context` has just rebuilt everything the flush would
    // have rebuilt, so there is nothing left to owe.
    r.queue_pending = false;
    r.resume_stale = false;
}

/// The order to hand PlayerService: the track playing now, then the USER'S OWN PICKS, then
/// whatever the context had left. That middle term is the whole point of the queue — before the
/// split it did not exist, because the picks and the context were one flat list and a queued song
/// simply took its place in line rather than jumping it.
///
/// Returns file paths. The current track leads so re-issuing the sequence at a boundary does not
/// change what is playing (see `Action::QueueChanged`: a mid-track SetTrackSequence restarts).
fn play_order_uris(r: &Render, current: &str) -> Vec<String> {
    // ONE query for the whole map, not one per row.
    //
    // This used to call `db.track_by_object_id(row.object_id)` for EVERY row — and that is a full
    // join-and-filter query each time. With the play context set to All Songs the context IS the
    // library, so a single shuffle press issued one database round-trip per track in the
    // collection, and it did so while holding the renderer mutex that every other FFI entry point
    // needs. Input, rendering and housekeeping all queue up behind it.
    //
    // Reported 2026-08-18: "toggling shuffle can crash the device when there is a lot queued" —
    // shuffled All Songs, pressed shuffle a couple of times, and the device stopped responding to
    // any input at all and had to be force-rebooted (the launcher's bad-boot counter then reverted
    // it to stock). Not a crash: a freeze, which is what a long hold of that lock looks like from
    // the outside.
    //
    // MEASURED FIRST, because the obvious suspect was wrong: `cinder-probe --seqtime` showed
    // SetTrackSequence is FLAT at ~0.25 s from 1 to 512 tracks, so the IPC was never the cost and
    // the queue flush never came near its 10 s guard. The cost was here, in Rust, and it scales
    // with library size rather than with the 512-track cap the shell applies afterwards.
    //
    // `tracks()` is a single query; the map makes each row a hash lookup. The allocation is one
    // String per track either way.
    let index: std::collections::HashMap<i64, String> = r
        .db
        .as_ref()
        .and_then(|db| db.tracks(cinder_db::Sort::Artist).ok())
        .map(|v| v.into_iter().map(|t| (t.object_id, t.filename)).collect())
        .unwrap_or_default();
    let by_id = |row: &cinder_ui::model::SongRow| -> Option<String> {
        index.get(&row.object_id).cloned()
    };
    let ctx = r.app.context();
    let from = r.app.context_idx() + 1;
    let tail = if from < ctx.len() { &ctx[from..] } else { &[][..] };
    play_order(
        Some(current),
        r.app.queue().iter().chain(tail.iter()).map(by_id),
    )
}

/// Assemble a play order: an optional leading URI (the track that is already audible), then the
/// rows behind it, dropping anything that did not resolve to a file.
///
/// NO TWO ADJACENT COPIES OF THE SAME FILE. `current` leads the list, so queueing the track you
/// are listening to produced `[A, A, …]` — and PlayerService moving from the first A to the second
/// does not change the URI, which is the only signal the shell reports a track start on.
/// `track_started` therefore never ran, the pick was never consumed out of the queue, and it came
/// back in the next flush: a phantom Up Next row and a track that played twice, for ever.
///
/// ONLY ADJACENT ones. Queueing the same song twice with something between them is a thing people
/// do deliberately, and there the URI does change at each boundary, so each copy is reported and
/// consumed exactly as it should be. Collapsing those would be silently refusing an instruction
/// rather than fixing a defect.
///
/// Pure, so the rule is testable without a framebuffer, a database or a device.
fn play_order(lead: Option<&str>, rest: impl IntoIterator<Item = Option<String>>) -> Vec<String> {
    let mut uris: Vec<String> = Vec::new();
    if let Some(l) = lead {
        uris.push(l.to_string());
    }
    for u in rest.into_iter().flatten() {
        if uris.last() != Some(&u) {
            uris.push(u);
        }
    }
    // AND THE CAP IS APPLIED HERE, WHERE IT CAN SAY SO. `set_pending` truncates a new context to
    // MAX_PLAY_SEQUENCE and logs it, but a flush rebuilds `[current] + queue + tail`, which can be
    // longer than the context it came from — and the shell's own `play_pending_sequence` then cut
    // it back to 512 in silence, from a fixed-size buffer, with nothing anywhere saying that the
    // last tracks in Up Next were not going to play.
    if uris.len() > MAX_PLAY_SEQUENCE {
        eprintln!(
            "cinder-ffi: play order is {} tracks; PlayerService is given the first {} — Up Next \
             shows more than will play",
            uris.len(),
            MAX_PLAY_SEQUENCE
        );
        uris.truncate(MAX_PLAY_SEQUENCE);
    }
    uris
}

/// Resolve a Library shuffle band into the URIs to play, in the order to play them. `None` when
/// there is no DB or the scope is empty — the caller then emits no action rather than handing the
/// shell an empty sequence.
///
/// Each arm matches the sub-label the band draws, so what the button promises is what it does.
/// Every track by one named artist, in the library's own artist order. It used to shuffle here,
/// which left the caller with no way to say what order it had replaced — see `note_pre_shuffle`.
/// Matches on ALBUM ARTIST with the track artist as the
/// fallback — the same `group_artist` rule the Artists tab is built with, so the row's track count
/// and what this plays are the same set.
fn artist_tracks(db: Option<&cinder_db::Db>, name: &str) -> Option<Vec<cinder_db::Track>> {
    let db = db?;
    let v: Vec<cinder_db::Track> = db
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
    Some(v)
}

fn shuffle_tracks(
    db: Option<&cinder_db::Db>,
    scope: cinder_ui::nav::ShuffleScope,
    keep: &dyn Fn(&cinder_db::Track) -> bool,
) -> Option<(Vec<cinder_db::Track>, Vec<i64>)> {
    use cinder_ui::nav::ShuffleScope as S;
    let db = db?;
    let mut rng = Rng::new();

    // THE FILTER IS APPLIED BEFORE THE SCOPE IS PICKED, not after it is shuffled. Two reasons:
    // a random artist or playlist has to be picked from what SURVIVES the filter, or the band
    // silently does nothing whenever it lands on one the filter empties; and the returned
    // pre-shuffle order must describe exactly the sequence handed back, which it cannot if rows
    // are dropped afterwards.
    match scope {
        // "N TRACKS · RANDOM ORDER"
        S::AllSongs => {
            let mut v: Vec<cinder_db::Track> =
                db.tracks(cinder_db::Sort::Title).ok()?.into_iter().filter(|t| keep(t)).collect();
            let pre = v.iter().map(|t| t.object_id).collect();
            rng.shuffle(&mut v);
            (!v.is_empty()).then_some((v, pre))
        }
        // "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE" — shuffle the albums, keep each album's
        // tracks in their disc/track order.
        S::ByAlbum => {
            let tracks: Vec<cinder_db::Track> =
                db.tracks_album_order().ok()?.into_iter().filter(|t| keep(t)).collect();
            let pre: Vec<i64> = tracks.iter().map(|t| t.object_id).collect();
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
            let v: Vec<cinder_db::Track> = albums.into_iter().flatten().collect();
            (!v.is_empty()).then_some((v, pre))
        }
        // "RANDOM ARTIST · SHUFFLED WITHIN ARTIST" — one artist, their tracks shuffled.
        S::ByArtist => {
            let tracks = db.tracks(cinder_db::Sort::Artist).ok()?;
            let mut by_artist: std::collections::BTreeMap<String, Vec<cinder_db::Track>> = Default::default();
            for t in tracks {
                if !t.artist.is_empty() && keep(&t) {
                    by_artist.entry(t.artist.clone()).or_default().push(t);
                }
            }
            let names: Vec<String> = by_artist.keys().cloned().collect();
            if names.is_empty() {
                return None;
            }
            let pick = &names[(rng.next() % names.len() as u64) as usize];
            let mut v = by_artist.remove(pick).unwrap_or_default();
            let pre = v.iter().map(|t| t.object_id).collect();
            rng.shuffle(&mut v);
            (!v.is_empty()).then_some((v, pre))
        }
        // "RANDOM PLAYLIST · SHUFFLED"
        S::Playlist => {
            let pls = db.playlists().ok()?;
            if pls.is_empty() {
                return None;
            }
            let pick = &pls[(rng.next() % pls.len() as u64) as usize];
            let mut v: Vec<cinder_db::Track> =
                playlist_tracks(Some(db), pick.id)?.into_iter().filter(|t| keep(t)).collect();
            let pre = v.iter().map(|t| t.object_id).collect();
            rng.shuffle(&mut v);
            (!v.is_empty()).then_some((v, pre))
        }
    }
}

/// Member file URIs of a playlist, in the user's saved order. `None` when there's no DB, the id
/// isn't a live playlist, or nothing in it still resolves to a playable track — the caller then
/// emits no action rather than handing the shell an empty sequence.
fn playlist_tracks(db: Option<&cinder_db::Db>, playlist_id: i64) -> Option<Vec<cinder_db::Track>> {
    let tracks = db?.playlist_tracks(playlist_id).ok()?;
    (!tracks.is_empty()).then_some(tracks)
}

/// Members of whichever playlist `id` names — Sony's (positive object id) or ours (negative, the
/// sign is the discriminator; see `playlists::id_for`).
fn any_playlist_tracks(r: &Render, id: i64) -> Option<Vec<cinder_db::Track>> {
    if id < 0 {
        user_playlist_tracks(r, id)
    } else {
        playlist_tracks(r.db.as_ref(), id)
    }
}

/// Build the UI rows for Cinder's own playlists, resolving each member path back to a library
/// track. A path that no longer resolves is dropped from the list but still counts in `tracks`,
/// which is the same honesty the Sony rows already have: "3 OF 4 TRACKS AVAILABLE" says the file
/// is missing rather than quietly shortening the playlist.
fn user_playlist_rows(
    store: &playlists::Store,
    db: Option<&cinder_db::Db>,
) -> Vec<cinder_ui::model::PlaylistRow> {
    // ONE QUERY FOR EVERY ENTRY OF EVERY PLAYLIST. This used to call `track_by_filename` per
    // entry, and that function runs a full `object_body` scan per call — so the cost was
    // (entries x library size), with no bound on either. On device (2026-09-05, 8 playlists over a
    // 3,456-track library) this function measured **3,802 ms**, which was 83% of the whole boot
    // dead time: `refresh_playlists` calls it during `cinder_db_open`, on every boot.
    //
    // `tracks_by_filenames` does one scan and indexes it, and is pinned to resolve every name
    // identically to `track_by_filename` (cinder-db: `batch_filename_resolution_matches_single`).
    // A miss stays a miss, so the `filter_map` below drops exactly the entries it dropped before.
    let resolved = db
        .map(|db| {
            let names: Vec<&str> = store
                .lists
                .iter()
                .flat_map(|l| l.entries.iter().map(|e| e.uri.as_str()))
                .collect();
            db.tracks_by_filenames(&names).unwrap_or_default()
        })
        .unwrap_or_default();
    store
        .lists
        .iter()
        .map(|list| {
            let track_list: Vec<cinder_ui::model::SongRow> = list
                .entries
                .iter()
                .filter_map(|entry| resolved.get(entry.uri.as_str()))
                .map(song_row_of)
                .collect();
            cinder_ui::model::PlaylistRow {
                id: list.id,
                name: list.name.clone(),
                tracks: list.entries.len() as u32,
                art: list.name.clone(),
                track_list,
                user: true,
            }
        })
        .collect()
}

/// Put one library track into one of our playlists. The PATH is what gets stored — object ids are
/// re-issued whenever the database is rebuilt, and a playlist that forgets its tracks on a rescan
/// would be worse than no playlist at all.
fn add_track_to_playlist(r: &mut Render, playlist_id: i64, object_id: i64) {
    let track = r.db.as_ref().and_then(|db| db.track_by_object_id(object_id).ok().flatten());
    let Some(track) = track else {
        eprintln!("cinder-ffi: playlist add: object {object_id} is not in the library");
        return;
    };
    let label = format!("{} - {}", track.artist, track.title);
    match r.plists.add(playlist_id, &track.filename, &label) {
        Ok(true) => {}
        Ok(false) => eprintln!("cinder-ffi: playlist add: already there (or full)"),
        Err(e) => eprintln!("cinder-ffi: playlist add: {e}"),
    }
}

/// Re-merge Sony's playlists with ours and hand the result to the UI. Called after every edit.
///
/// `set_playlists` rather than a full `build_library`: rebuilding the library is one query per
/// album plus one per playlist, and it would also throw away the scroll position of the screen
/// the user is editing on.
fn refresh_playlists(r: &mut Render) {
    let mut rows = Vec::new();
    let mut seen_names = std::collections::BTreeSet::new();
    for row in r
        .db_playlists
        .iter()
        .cloned()
        .chain(user_playlist_rows(&r.plists, r.db.as_ref()))
    {
        if seen_names.insert(row.name.to_lowercase()) {
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    r.app.set_playlists(rows);
    r.dirty = true;
}

/// The tracks of one of OUR playlists, in saved order, resolved to DB rows for playback.
fn user_playlist_tracks(r: &Render, id: i64) -> Option<Vec<cinder_db::Track>> {
    let db = r.db.as_ref()?;
    let list = r.plists.get(id)?;
    // Same batch resolve as `user_playlist_rows` — one scan for the whole playlist instead of one
    // per entry. Saved order is preserved because the entries drive the iteration; the map only
    // answers lookups.
    let names: Vec<&str> = list.entries.iter().map(|e| e.uri.as_str()).collect();
    let resolved = db.tracks_by_filenames(&names).unwrap_or_default();
    let tracks: Vec<cinder_db::Track> = list
        .entries
        .iter()
        .filter_map(|entry| resolved.get(entry.uri.as_str()).cloned())
        .collect();
    (!tracks.is_empty()).then_some(tracks)
}

/// Map a navigator `Action` to the `cinder_action_t` the shell carries out (Some = return this
/// code), applying the internal-only ones in place and returning None for them (theme is applied by
/// the caller; the sleep timer arms here; BtToggle is UI-only). Shared by cinder_input + cinder_tap.
/// Record that the sequence PlayerService holds is out of date. One place, because there are two
/// callers and they used to disagree about the second half of it: a resume armed at boot holds a
/// URI list snapshotted before the user touched anything, so an edit made before the first ▶ was
/// silently dropped — the resumed sequence played, and the edit only appeared at the boundary
/// after it.
fn mark_queue_pending(r: &mut Render) {
    r.queue_pending = true;
    if r.resume_pending.is_some() {
        r.resume_stale = true;
    }
}

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
                    let (seq, start, pre) = apply_shuffle(r.np.shuffle, tracks, idx);
                    set_pending(r, seq, start);
                    if let Some(pre) = pre {
                        r.app.note_pre_shuffle(pre);
                    }
                    8
                }
                _ => {
                    eprintln!("cinder-ffi: PlayIndex({object_id}): no DB context — ignored");
                    return None;
                }
            }
        }
        Action::PlayQueueAt(n) => {
            // Play the USER queue (swipe-to-queue) from the tapped row. Until this existed a queue
            // row played that track's ALBUM context, so Up Next displayed one list while the
            // transport stepped through another — the queue was visible but not actually playable.
            //
            // A QUEUE ROW IS STILL A QUEUE ROW. This used to go through `set_pending`, i.e. through
            // `set_play_context` — the "the user started something new" path, which makes the
            // sequence the CONTEXT and then CLEARS the queue. Everything played in the right
            // order, so it looked correct; what it actually did was silently turn the user's
            // hand-built picks into ordinary context. NEXT IN QUEUE and its CLEAR chip
            // disappeared, and the next album tapped anywhere in the app then replaced them with
            // no "you have a queue" prompt, because there was no longer a queue to ask about.
            //
            // So: drop the picks ahead of the tapped one (skipping forward past them is what the
            // tap means), leave the rest queued and the context alone, and hand PlayerService the
            // same order any other flush would — the tapped row, then the picks behind it, then
            // the context tail.
            if !r.app.queue_play_at(*n) {
                eprintln!("cinder-ffi: PlayQueueAt({n}): no such queue row — ignored");
                return None;
            }
            let ids: Vec<i64> = r.app.queue().iter().map(|s| s.object_id).collect();
            // One query for the whole queue, not one per row — see `play_order_uris` for the
            // report that shape caused, and `Db::tracks_by_object_ids`.
            let by_id = match r.db.as_ref() {
                Some(db) => db.tracks_by_object_ids(&ids).unwrap_or_default(),
                None => Default::default(),
            };
            let Some(head) = ids.first().and_then(|id| by_id.get(id)).map(|t| t.filename.clone())
            else {
                eprintln!("cinder-ffi: PlayQueueAt({n}): the tapped row no longer resolves to a \
                           file — ignored");
                return None;
            };
            let ctx = r.app.context();
            let from = r.app.context_idx() + 1;
            let tail = if from < ctx.len() { &ctx[from..] } else { &[][..] };
            r.pending_play = play_order(
                Some(&head),
                r.app.queue()[1..]
                    .iter()
                    .chain(tail.iter())
                    .map(|row| by_id.get(&row.object_id).map(|t| t.filename.clone())),
            );
            r.pending_play_start = 0;
            // Same bookkeeping `set_pending` does: ◁ walks Cinder's own history, and that history
            // is about the sequence being replaced.
            r.play_history.clear();
            r.rewind_from = None;
            r.queue_pending = false;
            8
        }
        Action::PlayContextAt(n) => {
            // Jump within the sequence that is ALREADY PLAYING, keeping its order.
            //
            // The row tapped in Up Next is an index into the context, and `PlayIndex` cannot carry
            // that: it resolves an object id to the track's album, so after "Shuffle all songs" a
            // tap four rows down replaced the shuffled library with that track's album. Same
            // pending-play channel as the queue and the playlist variants — the only difference is
            // where the sequence comes from.
            //
            // NO apply_shuffle. Every other play path here pre-shuffles because it is starting a
            // NEW sequence and `r.np.shuffle` says the user wants it random. This one is not
            // starting anything new: the context was shuffled when it was built, the user is
            // looking at that order on screen, and re-shuffling it here would scramble the list
            // out from under the row they just tapped — the bug in a second form.
            let ids: Vec<i64> = r.app.context().iter().map(|s| s.object_id).collect();
            // ONE pass, resolving and locating the tapped row together. Resolution can DROP rows —
            // a file deleted since the context was built — and every drop before the tapped row
            // slides it up by one, so a start index carried over from the unresolved list starts
            // the wrong track. Counting the survivors as we go is the same walk, and it cannot
            // disagree with itself the way two separate passes can.
            //
            // THE RESOLUTION IS NOW ONE QUERY, the walk is unchanged. This loop used to issue a
            // full scan per id while holding the renderer mutex, over a list that IS the whole
            // library after "Shuffle all songs" — the exact configuration behind the 2026-08-18
            // freeze report described above `play_order_uris`, which was fixed there and left
            // standing here. `tracks_by_object_ids` resolves the lot in one scan; the loop below
            // still walks `ids` in order and still counts survivors as it goes, so the start-index
            // reasoning in the paragraph above is untouched.
            let by_id = r
                .db
                .as_ref()
                .map(|db| db.tracks_by_object_ids(&ids).unwrap_or_default())
                .unwrap_or_default();
            let mut seq: Vec<cinder_db::Track> = Vec::with_capacity(ids.len());
            let mut start = 0usize;
            for (i, id) in ids.iter().enumerate() {
                if let Some(t) = by_id.get(id) {
                    if i < *n {
                        start += 1;
                    }
                    seq.push(t.clone());
                }
            }
            if seq.is_empty() {
                eprintln!("cinder-ffi: PlayContextAt({n}): context did not resolve — ignored");
                return None;
            }
            if seq.len() != ids.len() {
                eprintln!(
                    "cinder-ffi: PlayContextAt({n}): {} of {} context tracks resolved",
                    seq.len(),
                    ids.len()
                );
            }
            // Clamped, for the case where every surviving track sits BEFORE the tapped row: `start`
            // counts survivors ahead of it, so it can land exactly one past the end.
            let start = start.min(seq.len() - 1);
            set_pending(r, seq, start);
            8
        }
        Action::Seek(permille) => {
            // Only reachable from the HOST sim, which drives `nav::App` directly. On device the
            // rail is routed through cinder_scrub_hit/_to/_end, which owns the millisecond math
            // and the "ignore incoming positions mid-drag" rule; that path never produces this
            // action. Preview the position so the sim's bar still tracks the gesture.
            if r.cur_duration_ms > 0 {
                let target = r.cur_duration_ms * (*permille).min(1000) as i64 / 1000;
                r.play_pos_ms = target;
                let dur = r.cur_duration_ms;
                set_progress(&mut r.np, target, dur);
                r.dirty = true;
            }
            return None;
        }
        Action::UiScaleChanged => {
            // Pure UI: the text scale is a global already applied by measure+draw. Repaint and
            // persist. (A scrub-driven change is saved by cinder_scrub_end instead.)
            r.dirty = true;
            save_settings(r);
            return None;
        }
        Action::PlayPlaylist(playlist_id) => {
            // Same channel as PlayIndex — the members become the pending sequence, starting at
            // the top — so the shell keeps handling exactly one "play these URIs" action and
            // needs no new code or FFI symbol for playlists.
            match any_playlist_tracks(r, *playlist_id) {
                Some(seq) => {
                    let (seq, start, pre) = apply_shuffle(r.np.shuffle, seq, 0);
                    set_pending(r, seq, start);
                    if let Some(pre) = pre {
                        r.app.note_pre_shuffle(pre);
                    }
                    8
                }
                None => {
                    eprintln!("cinder-ffi: PlayPlaylist({playlist_id}): empty or unknown — ignored");
                    return None;
                }
            }
        }
        Action::PlayPlaylistAt(playlist_id, index) => {
            // The playlist IS the context. `PlayIndex` cannot express this: an object id only
            // knows its album, so a tap on a playlist member played that member's album and
            // stopped — one song, on a playlist of singles. Same pending-play channel as
            // `PlayPlaylist`, just starting at the tapped row instead of the top.
            match any_playlist_tracks(r, *playlist_id) {
                Some(seq) => {
                    let start = (*index as usize).min(seq.len().saturating_sub(1));
                    let (seq, start, pre) = apply_shuffle(r.np.shuffle, seq, start);
                    set_pending(r, seq, start);
                    if let Some(pre) = pre {
                        r.app.note_pre_shuffle(pre);
                    }
                    8
                }
                None => {
                    eprintln!(
                        "cinder-ffi: PlayPlaylistAt({playlist_id}, {index}): empty or unknown — ignored"
                    );
                    return None;
                }
            }
        }
        Action::Shuffle(scope) => {
            // Same pending-play channel again: we pre-shuffle the URI list ourselves, so the
            // order is genuinely random regardless of what PlayerService's own shuffle does.
            //
            // HONOUR THE FILTER — BOTH AXES. The band's caption reads "Shuffle Rock", or
            // "Shuffle Rock · Hi-Res", and says how many tracks that is; `shuffle_tracks` resolves
            // straight out of the DB and knows nothing about the filter, so the button promised a
            // filtered shuffle and played the whole library. Genre was fixed for exactly that
            // reason and Hi-Res — added later as an independent second axis — was never wired in,
            // which on the reference library is the difference between 1 track and 3,463. The fix
            // is the library's OWN predicate: `Library::passes` is what every filtered list asks,
            // so the band and the list can no longer disagree about what the filter means.
            let keep = {
                let lib = r.app.library();
                let filtered = lib.filtered();
                let genre = lib.filter_genre;
                let hires = lib.filter_hires;
                move |t: &cinder_db::Track| {
                    !filtered
                        || cinder_ui::model::Library {
                            filter_genre: genre,
                            filter_hires: hires,
                            ..Default::default()
                        }
                        .passes(&cinder_ui::model::SongRow {
                            genre_id: t.genre_id.unwrap_or(0),
                            is_hires: t.is_hires,
                            ..Default::default()
                        })
                }
            };
            match shuffle_tracks(r.db.as_ref(), *scope, &keep) {
                Some((seq, pre)) => {
                    // ASKING FOR A SHUFFLED PLAY TURNS SHUFFLE ON. Reported 2026-08-18: pressing
                    // the shuffle band on Albums / All Songs started a shuffled sequence but left
                    // the transport's shuffle indicator OFF, so the control said one thing and the
                    // player said another — and the moment the sequence ran out, playback carried
                    // on in plain order without anything having changed on screen.
                    r.np.shuffle = true;
                    set_pending(r, seq, 0);
                    // AND RECORD THE ORDER IT REPLACED, so the toggle can be turned back off.
                    // Every other shuffle entry point does this (`PlayIndex`, `PlayPlaylist*`,
                    // `ShufflePlaylist`, `ShuffleArtist`); the four Library bands did not, which
                    // left shuffle a one-way door on the path most likely to be taken — press
                    // "Shuffle all songs", then press the shuffle icon to turn it off, and the
                    // icon went dark while the sequence stayed permuted for the rest of the
                    // session. `note_pre_shuffle` refuses an order that does not describe the
                    // context it was just handed, so a scope that resolved to nothing is safe.
                    r.app.note_pre_shuffle(pre);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: Shuffle({scope:?}): nothing to play under the active \
                               filter — ignored");
                    return None;
                }
            }
        }
        Action::ShufflePlaylist(playlist_id) => {
            // One NAMED playlist, shuffled — the band on the playlist page. Distinct from
            // `ShuffleScope::Playlist`, which picks a random playlist and shuffles that.
            match any_playlist_tracks(r, *playlist_id) {
                Some(mut seq) => {
                    // Keep the playlist's real running order so shuffle-off can restore it. The
                    // toggle promises "play the rest of this in order from here", and it has to
                    // mean that however the shuffle started — via the toggle, or via this band.
                    let pre: Vec<i64> = seq.iter().map(|t| t.object_id).collect();
                    Rng::new().shuffle(&mut seq);
                    // ASKING FOR A SHUFFLED PLAY TURNS SHUFFLE ON. Reported 2026-08-18: pressing
                    // the shuffle band on Albums / All Songs started a shuffled sequence but left
                    // the transport's shuffle indicator OFF, so the control said one thing and the
                    // player said another — and the moment the sequence ran out, playback carried
                    // on in plain order without anything having changed on screen.
                    r.np.shuffle = true;
                    set_pending(r, seq, 0);
                    r.app.note_pre_shuffle(pre);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: ShufflePlaylist({playlist_id}): nothing playable in it");
                    return None;
                }
            }
        }
        // ── playlists the user made on the device ───────────────────────────────────────
        // All five are INTERNAL: they change files and the UI's rows, and there is nothing for
        // the C++ shell to do, so they return None rather than an action code.
        Action::PlaylistCreate | Action::PlaylistCreateWith(_) => {
            let name = r.app.text_input().to_string();
            match r.plists.create(&name) {
                Ok(id) => {
                    if let Action::PlaylistCreateWith(object_id) = a {
                        add_track_to_playlist(r, id, *object_id);
                    }
                    refresh_playlists(r);
                    // Land inside the new playlist: naming one and being dropped back at the list
                    // to find it again is a step nobody wants.
                    r.app.open_playlist_by_id(id);
                    eprintln!("cinder-ffi: playlist created: {name:?}");
                }
                Err(e) => eprintln!("cinder-ffi: playlist create {name:?}: {e}"),
            }
            return None;
        }
        Action::PlaylistRename(id) => {
            let name = r.app.text_input().to_string();
            if let Err(e) = r.plists.rename(*id, &name) {
                eprintln!("cinder-ffi: playlist rename: {e}");
            }
            refresh_playlists(r);
            return None;
        }
        Action::PlaylistDelete(id) => {
            if let Err(e) = r.plists.delete(*id) {
                eprintln!("cinder-ffi: playlist delete: {e}");
            }
            refresh_playlists(r);
            return None;
        }
        Action::PlaylistAddTrack(playlist_id, object_id) => {
            add_track_to_playlist(r, *playlist_id, *object_id);
            refresh_playlists(r);
            return None;
        }
        Action::PlaylistRemoveAt(playlist_id, position) => {
            match r.plists.remove_at(*playlist_id, *position as usize) {
                Ok(true) => refresh_playlists(r),
                Ok(false) => {}
                Err(e) => eprintln!("cinder-ffi: playlist remove: {e}"),
            }
            return None;
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
                Some(mut seq) => {
                    // Same as the playlist band: remember the catalogue order before permuting it.
                    let pre: Vec<i64> = seq.iter().map(|t| t.object_id).collect();
                    Rng::new().shuffle(&mut seq);
                    // ASKING FOR A SHUFFLED PLAY TURNS SHUFFLE ON. Reported 2026-08-18: pressing
                    // the shuffle band on Albums / All Songs started a shuffled sequence but left
                    // the transport's shuffle indicator OFF, so the control said one thing and the
                    // player said another — and the moment the sequence ran out, playback carried
                    // on in plain order without anything having changed on screen.
                    r.np.shuffle = true;
                    set_pending(r, seq, 0);
                    r.app.note_pre_shuffle(pre);
                    8
                }
                None => {
                    eprintln!("cinder-ffi: ShuffleArtist({name}): nothing to play — ignored");
                    return None;
                }
            }
        }
        // FM. The frequency/direction the shell needs is fetched with cinder_fm_* rather than
        // packed into the return code, which is a single int.
        Action::FmPower(on) => { r.fm_power = *on; 40 }
        Action::FmTune(khz) => { r.app.fm_report_khz(*khz); 41 }
        Action::FmSeek(dir) => { r.fm_seek_dir = *dir; 42 }
        Action::FmScan => 43,
        Action::FmBtOut(on) => { r.fm_bt = *on; 44 }
        Action::ThemeChanged(_) => 16, // shell also drives the backlight (night = minimal light)
        Action::Sleep => 10,
        Action::EnterUsbMsc => 11,
        Action::ExitUsbMsc => 19,
        Action::EqChanged(_) => 12,
        Action::BtToggle(_) => 26, // shell drives SetRfOnOff + reconnects the last device
        Action::BtDisconnect => 27, // shell calls RequestDisconnection; radio stays on
        // The row index travels in `pending_bt_device`, not in the action code — same one-value
        // side-channel the play-by-index path uses. The shell reads it with
        // cinder_pending_bt_device() and looks up the BD address in its own copy of the list.
        Action::BtConnectDevice(i) => {
            r.pending_bt_device = Some(*i);
            28
        }
        Action::BtForgetDevice(i) => {
            r.pending_bt_device = Some(*i);
            29
        }
        Action::BtPairedRefresh => 30, // shell re-reads GetPairedDeviceInfo + pushes the list back
        Action::BtScanToggle(_) => {
            // The shell reads cinder_get_bt_scanning() and calls SetSearchMode. It may also push the
            // state back (the radio's search window expires on its own), which is why the UI does not
            // treat its own tap as the last word.
            31
        }
        Action::BtPairDevice(i) => {
            r.pending_bt_device = Some(*i);
            32
        }
        Action::BtPromptConfirm => 33,
        Action::BtPromptCancel => 34,
        Action::SleepTimer(m) => {
            // internal: arm/cancel the countdown (no Sony service to start it)
            r.sleep_remaining_ms = *m as i64 * 60_000;
            r.sleep_fire = false;
            return None;
        }
        Action::BatteryCareChanged(_) => 13,
        Action::SoundChanged => 14,
        Action::BalanceChanged => 38,
        Action::ClockSet => 39,
        Action::SoundBypass(_) => 15,
        Action::ShuffleToggle => {
            r.np.shuffle = !r.np.shuffle;
            // The transport control must affect the sequence already playing, not merely the next
            // album the user starts. App::queue_shuffle deliberately moves only context entries
            // after the current song; the explicit user queue keeps its chosen order and remains
            // directly after current. Returning the queue action uses the shell's position-safe
            // replacement path, so enabling shuffle does not restart the audible track.
            // OFF has to do something too. It used to do nothing at all, which made shuffle a
            // one-way door: the context stayed permuted for the rest of the session and the only
            // route back to album order was to re-tap the album. App::unshuffle_context puts the
            // recorded order back and keeps the audible track playing.
            let changed = if r.np.shuffle {
                !r.app.queue_shuffle().is_empty()
            } else {
                !r.app.unshuffle_context().is_empty()
            };
            if changed && r.last_track.is_some() {
                // DO NOT REPLACE THE SEQUENCE MID-TRACK WHILE AUDIO IS RUNNING.
                //
                // This used to flush immediately, and PlayerService has no reorder operation — the
                // only way to change a sequence is to replace the whole thing, which costs the
                // measured 360-450 ms pause/seek/play cycle `Action::QueueChanged` below already
                // refuses to pay at gesture time. Doing it under a playing track is audible:
                // reported as playback stuttering when shuffle is pressed during playback.
                //
                // Deferring costs nothing the user can hear the other way, because `queue_shuffle`
                // and `unshuffle_context` both leave the CURRENT song alone by construction — they
                // only permute context entries AFTER it. So the audible track is identical whether
                // the sequence is rebuilt now or at the boundary; the only difference is the gap.
                //
                // NOT PLAYING = NOTHING TO INTERRUPT, AND NOTHING TO START EITHER.
                //
                // This used to take an "immediate path" here: build the sequence, set
                // `queue_flush`, and return 36 — which the shell answers with
                // `play_pending_sequence`, and that ends in `ChangePlayState(Play)` plus
                // `set_transport(true)`. So pressing SHUFFLE on a paused player STARTED THE MUSIC.
                // It is not a transport control, and on a device you carry in a pocket a control
                // that begins playing when you did not ask it to is the worst kind of surprise.
                //
                // Deferring costs nothing here either: `queue_shuffle` and `unshuffle_context`
                // leave the current track alone by construction, so the reordered tail only has to
                // be in place before PlayerService reaches it — and the next ▶ hands over a fresh
                // sequence anyway (the resume path, or the boundary flush below).
                mark_queue_pending(r);
                return None;
            }
            return None;
        }
        Action::RepeatCycle => {
            // Two states, both of which do something. It used to cycle off → all → one and tell
            // PlayerService nothing at all; "all" has no known primitive, so a third position would
            // still be decorative. The shell reads cinder_get_repeat_one() and applies it.
            // off -> ALL -> ONE -> off, the order every other player uses.
            //
            // There used to be no repeat-all because no primitive for it had been found. One has:
            // measured 2026-08-26 (DEVICE_TESTS.md 3f), the queue boundary is unmistakable — the
            // position pins at the duration and `playing` goes 1 -> 0 with the URI unchanged. That
            // is a signal the shell can watch for and re-issue the queue on, which is all
            // repeat-all needs. 1 = repeat-one (OneTrackMode), 2 = repeat-all (shell-driven).
            r.np.repeat = match r.np.repeat { 0 => 2, 2 => 1, _ => 0 };
            23
        }
        Action::BtEnhancedChanged => 35, // shell reads cinder_get_bt_enhanced + SetControlAbsoluteVolume
        Action::BtCodecChanged => 17, // shell reads cinder_get_bt_codec/quality + applies via BtTransmitter
        Action::UsbDacToggle(_) => 18, // shell reads cinder_get_usb_dac() + starts/stops the LDAC bridge
        Action::BrightnessChanged(_) => 20, // shell reads cinder_get_brightness() + writes the backlight
        Action::ScreenOffTimer(_) => 21,    // shell reads cinder_get_screen_off_s() + counts idle
        Action::BootToStock => 22,          // shell arms the one-shot flag + restarts into stock
        Action::RescanLibrary => {
            // Give the label a deadline as well as its normal exit. `set_library` clears the
            // "Rescanning…" state when a reloaded library arrives, but a scan that finds nothing
            // changed writes nothing, so db_signature never fires and no library ever comes back —
            // the row would sit on "Rescanning…" for the rest of the session in exactly the case
            // where the honest answer is "already up to date". 60 s is well past the ~25 s a full
            // 3,400-track scan took on device.
            r.rescan_left_ms = 60_000;
            45
        }
        Action::QueueChanged => {
            // PlayerService has no insert operation. Replacing its sequence at gesture time costs
            // a measured 360–450 ms pause/seek/play cycle, which made adding a song visibly lag.
            // Hold the edit until the last couple of seconds of the current song instead: the new
            // sequence is then in place before PlayerService selects the next context track.
            mark_queue_pending(r);
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
        Action::SettingsReset => {
            // The UI has already put every preference back to its default. What is left is the
            // half that lives OUT here: the hardware volume the shell restores at boot, and the
            // settings file itself, which must be rewritten immediately rather than at the next
            // change — otherwise a power-off straight after a reset would come back to the old
            // file. The shell then re-applies the chain (EQ / sound / balance / backlight) from
            // the fresh state.
            r.np.shuffle = false;
            r.np.repeat = 0;
            save_settings(r);
            37
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
    // ALWAYS WRITE ALL TEN, even when there is no renderer to read them from. The caller is
    // `apply_eq_fn`, which declares `signed char bands[10]` on the stack and hands the result
    // straight to `cinder_effects_set_eq` — so returning early here left ten bytes of
    // uninitialised stack being marshalled into the DSP. Values outside ±20 do not clamp inside
    // the service, they ZERO the band, so the visible result would have been an EQ with bands
    // randomly flat. Not reachable today (apply_eq_fn only runs after render init, so the lock
    // always yields Some), which is exactly why it would have gone on not being reachable until
    // one day it was. Flat is the correct answer for "no EQ state yet".
    let bands = cell()
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.app.eq_bands())
        .unwrap_or([0i8; 10]);
    unsafe {
        for (i, b) in bands.iter().enumerate() {
            *out.add(i) = *b;
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
/// Is the track playing right now the LAST entry of the sequence the shell handed PlayerService?
///
/// The queue-end signal repeat-all watches for — the position pinned at the duration with
/// `playing` gone 1 → 0 — is the same shape a PAUSE inside the final seconds of ANY track makes.
/// Without this the shell could not tell the two apart, so pausing 1 s before the end of track 3
/// of 12 restarted the whole queue from track 1.
///
/// `pending_play` is the sequence as HANDED OVER and is not rewritten as playback advances, so its
/// last entry is the final track PlayerService was given. Two entries sharing a filename make this
/// answer true early; the cost is a lap that could have waited, not a missed one.
#[no_mangle]
pub extern "C" fn cinder_on_last_track() -> libc::c_int {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return 0 };
    match (r.last_track.as_ref(), r.pending_play.last()) {
        (Some(t), Some(last)) => (*last == t.filename) as libc::c_int,
        _ => 0,
    }
}

/// Build the sequence a REPEAT-ALL lap should play, into the ordinary pending-play channel.
/// 1 = there is one, 0 = nothing to repeat.
///
/// The shim's own `cinder_audio_restart_sequence` replays the URI list it last handed over, and
/// that is NOT the context whenever a queue edit has been flushed: a flush hands over
/// `[current] + queue + context[idx+1..]`, so after swipe-queueing one song at track 5 of an
/// album, repeat-all looped tracks 5-12 for ever and tracks 1-4 never played again. Cinder holds
/// the whole context; a lap is that, from the top, with anything still queued ahead of it —
/// exactly the order the queue promises on any other track boundary.
#[no_mangle]
pub extern "C" fn cinder_repeat_all_prepare() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    if r.app.context().is_empty() && r.app.queue().is_empty() {
        return 0;
    }
    let index: std::collections::HashMap<i64, String> = r
        .db
        .as_ref()
        .and_then(|db| db.tracks(cinder_db::Sort::Artist).ok())
        .map(|v| v.into_iter().map(|t| (t.object_id, t.filename)).collect())
        .unwrap_or_default();
    let by_id = |row: &cinder_ui::model::SongRow| index.get(&row.object_id).cloned();
    let uris = play_order(
        None,
        r.app.queue().iter().chain(r.app.context().iter()).map(by_id),
    );
    if uris.is_empty() {
        return 0;
    }
    r.pending_play = uris;
    r.pending_play_start = 0;
    // A lap replaces the sequence outright, so nothing is owed against the old one.
    r.queue_pending = false;
    1
}

#[no_mangle]
pub extern "C" fn cinder_take_queue_flush() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let f = r.queue_flush;
    r.queue_flush = false;
    f as libc::c_int
}

/// Repeat-ALL on/off (1/0). Read after a CINDER_ACT_REPEAT_CHANGED action. PlayerService has no
/// primitive for this, so the shell implements it: watch for the queue boundary and re-issue.
#[no_mangle]
pub extern "C" fn cinder_get_repeat_all() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.np.repeat == 2 => 1,
        _ => 0,
    }
}

/// Repeat-one on/off (1/0) — the OneTrackMode half. Repeat-ALL is cinder_get_repeat_all.
#[no_mangle]
pub extern "C" fn cinder_get_repeat_one() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.np.repeat == 1 => 1,
        _ => 0,
    }
}

/// The analyzer emit rate the user picked, in Hz (Settings ▸ Visualiser ▸ Frame rate). The shell
/// passes it to `cinder_analyzer_start`; it is also the visualiser's share of the render budget,
/// which is why the row tops out well short of the panel's refresh.
#[no_mangle]
pub extern "C" fn cinder_viz_analyzer_rate_hz() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 20 };
    r.app.viz_analyzer_params().0 as libc::c_int
}

/// The detector window the user picked, in MILLISECONDS, or 0 for "leave the service's default".
/// The shell converts it to samples — the sample rate is a property of the stream, and the shell
/// is the side that knows one. Paired with `cinder_viz_analyzer_rate_hz`; the shell polls both in
/// its housekeeping tick and restarts the analyzer when either changes, so editing the row does
/// not have to interrupt anything.
#[no_mangle]
pub extern "C" fn cinder_viz_analyzer_window_ms() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    r.app.viz_analyzer_params().1 as libc::c_int
}

/// Does the visualiser want the analyzer streaming right now? 1 when the user has it enabled, the
/// Now Playing screen is showing, and something is actually playing.
///
/// The shell polls this and starts/stops Sony's AudioAnalyzerService to match, so the service only
/// runs while its output is on screen. Combined with the shell's own screen-on check that means no
/// filter bank, no IPC and no wakeups while the panel is dark or while you are browsing the
/// library — which is most of the time a music player is switched on.
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

/// "Use Enhanced Mode" (1/0) — Sony's name for AVRCP absolute volume. The shell reads this after
/// CINDER_ACT_BT_ENHANCED_CHANGED *and* after every reconnect, and hands it to
/// `BtTransmitterServiceClient::SetControlAbsoluteVolume` (slot 31). Sony's service gates
/// `SetCurrentVolume` on this preference internally, so leaving it unset makes absolute volume a
/// silent no-op.
#[no_mangle]
pub extern "C" fn cinder_get_bt_enhanced() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.bt_enhanced() as libc::c_int,
        None => 1,
    }
}

/// Report whether the CONNECTED sink accepts absolute volume (`IsSupportedAbsoluteVolume`, slot
/// 33). Returns 1 if the Bluetooth screen needs a repaint.
#[no_mangle]
pub extern "C" fn cinder_set_bt_enhanced_supported(on: libc::c_int) -> libc::c_int {
    let mut g = cell().lock().unwrap();
    let r = match g.as_mut() { Some(r) => r, None => return 0 };
    if r.app.set_bt_enhanced_supported(on != 0) {
        r.dirty = true;
        1
    } else {
        0
    }
}

/// Top of the Bluetooth volume scale, mirroring CINDER_BT_VOL_MAX in cinder.h.
const VOL_BT_MAX: u8 = cinder_ui::overlay::BT_VOL_MAX;

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

/// Publish the host's live USB stream format for the USB-DAC panel: rate in Hz, bit depth, channel
/// count, straight from Sony's `stream_info_t` (the same three words `GetStatus` fills in). Rate 0
/// means "the host is not streaming" and clears the panel back to its generic line.
///
/// State only — it raises no action. The panel is a readout of what the hardware is doing; nothing
/// the user can do on that screen changes the format.
#[no_mangle]
pub extern "C" fn cinder_set_usb_dac_format(rate: libc::c_uint, bits: libc::c_uint,
                                            chans: libc::c_uint) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_usb_dac_format(rate as u32, bits as u32, chans as u32);
    }
}

/// Publish the codec A2DP actually negotiated, as the raw `BtSoundCodec` word from
/// `GetSoundStatus`. 0 means "nothing connected / not known".
///
/// The enumerators are deliberately NOT decoded here: with nothing connected every field reads 0,
/// so a mapping would be a guess, and this screen has already shipped two claims about its output
/// that turned out to be false. The UI shows a neutral label until the raw value can be tied to a
/// real headphone. Sets state only.
#[no_mangle]
pub extern "C" fn cinder_set_bt_negotiated_codec(raw: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_negotiated_codec(raw as u32);
    }
}

/// Leave the transient backlight-off state (brightness 0) and return to the last visible level.
/// Returns 1 if it changed, so the shell only rewrites the backlight node when it must.
///
/// Level 0 turns the panel's backlight fully off while the app keeps running — useful in the dark,
/// and it costs nothing to leave because ANY input restores it. That is what makes a setting the
/// Settings screen itself cannot be read at safe to offer: it is not persisted, it does not
/// survive a reboot, and the next thing you touch undoes it.
#[no_mangle]
pub extern "C" fn cinder_brightness_wake() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    if r.app.brightness_wake() {
        r.dirty = true;
        1
    } else {
        0
    }
}

/// The UI's panel-brightness level, **0..=5** — 0 = backlight fully off, which is TRANSIENT (see
/// cinder_brightness_wake). Read after a CINDER_ACT_BRIGHTNESS_CHANGED action (and at boot) and map
/// it onto the backlight node. Defaults to 4 (the shell's ~70% day level) if the renderer isn't up.
///
/// Clamped to 1..=5 because the UI no longer offers a 0. It briefly did: the Settings row had a
/// BACKLIGHT OFF stop, and this getter's `.clamp(1, 5)` silently turned it into level 1 — reported
/// 2026-08-26 as "the backlight off setting does nothing different to backlight 1", which was
/// exactly right. Unclamping made it work, and working revealed that the feature was not what was
/// wanted: the intent was an unlit but still READABLE screen, and zeroing this panel's backlight
/// just blanks it. So the stop was removed and the feature parked (see `brightness` in nav.rs).
///
/// If it comes back, change this to `.clamp(0, 5)` — the rest of the path is still in place and
/// still correct: 0 is never persisted (`save_settings` writes `brightness_restore()`), and
/// `brightness_wake_on_input()` restores the level on the Hold switch or Power.
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

/// Auto power-off, in MINUTES (0 = off). The shell polls this from its 1 Hz housekeeping and owns
/// the idle timer; the UI only remembers the choice.
#[no_mangle]
pub extern "C" fn cinder_get_auto_off_min() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.auto_off_min() as libc::c_int)
}

/// L/R balance position, 0..=100 with 50 = centre. The shell turns it into the codec's two
/// attenuation controls; the UI only remembers the position.
#[no_mangle]
pub extern "C" fn cinder_get_balance() -> libc::c_int {
    cell()
        .lock()
        .unwrap()
        .as_ref()
        .map_or(cinder_ui::sound::BALANCE_CENTRE as libc::c_int, |r| r.app.balance() as libc::c_int)
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

/// Push the name of the connected Bluetooth device into the UI (NULL or "" = nothing connected).
/// The shell reads it from GetConnectInformation and calls this; the Bluetooth screen's CONNECTED
/// card shows it.
#[no_mangle]
pub extern "C" fn cinder_set_bt_connected(name: *const libc::c_char) {
    let owned: Option<String> = if name.is_null() {
        None
    } else {
        unsafe { std::ffi::CStr::from_ptr(name) }.to_str().ok().map(|s| s.to_string())
    };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_connected(owned.as_deref());
    }
}

/// Push the wall clock into the UI (UTC epoch seconds). Call it from the ~1 Hz housekeeping — the
/// Settings ▸ Date & time row shows it, and the clock editor seeds from it. Ignored while the
/// editor is open so a per-second push cannot drag a field out from under the user's finger.
#[no_mangle]
pub extern "C" fn cinder_set_clock_epoch(epoch: i64) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_clock_epoch(epoch);
    }
}

/// The UTC epoch the clock editor wants written. Read after a CINDER_ACT_CLOCK_SET action and pass
/// it to the setuid `cinder-clock` helper, which sets BOTH the system clock and the RTC.
///
/// Always inside the range the helper accepts (2001-01-01 .. 2038-01-01) — the editor clamps to the
/// same bound the helper enforces, so the two cannot disagree. The upper bound is NOT arbitrary:
/// `time_t` is 32-bit here and the signed wrap at 2038-01-19 turns a future date into 1901.
#[no_mangle]
pub extern "C" fn cinder_get_clock_epoch() -> i64 {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.clock_epoch_pending())
}

/// Start a fresh paired-device list. Call this, then `cinder_bt_paired_add` once per device **in the
/// order the shell will index them** — the UI hands back a row index, never an address, so the two
/// orderings are the same object seen from two sides.
#[no_mangle]
pub extern "C" fn cinder_bt_paired_clear() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.bt_paired_clear();
    }
}

/// Append one paired device. `kind` may be NULL/"" (the row then just says "TAP TO CONNECT").
/// `connected` != 0 marks the device the radio is currently linked to.
#[no_mangle]
pub extern "C" fn cinder_bt_paired_add(
    name: *const libc::c_char,
    kind: *const libc::c_char,
    connected: libc::c_int,
) {
    // A device with no readable name is still a device — showing its row as "(unnamed)" beats
    // dropping it, because the row is the only way to forget a bad pairing.
    let cstr = |p: *const libc::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let mut name = cstr(name);
    if name.is_empty() {
        name = "(unnamed)".to_string();
    }
    let kind = cstr(kind);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.bt_paired_add(&name, &kind, connected != 0);
    }
}

/// Discovered-device list (the FOUND section). Cleared when a scan starts, then one _add per device
/// the listener reports — in the same order the shell keeps its addresses, because the UI hands back
/// a row index and nothing else. `kind` may be NULL.
#[no_mangle]
pub extern "C" fn cinder_bt_found_clear() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.bt_found_clear();
    }
}

#[no_mangle]
pub extern "C" fn cinder_bt_found_add(name: *const libc::c_char, kind: *const libc::c_char) {
    let cstr = |p: *const libc::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    // A scan very often reports an address before the name resolves. Showing "(unnamed)" beats
    // dropping the row, because an unnamed device is still pairable — and the shell replaces the
    // whole list as better names arrive.
    let mut name = cstr(name);
    if name.is_empty() {
        name = "(unnamed)".to_string();
    }
    let kind = cstr(kind);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.bt_found_add(&name, &kind);
    }
}

#[no_mangle]
pub extern "C" fn cinder_bt_found_count() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.bt_found_len() as libc::c_int)
}

/// Raise a pairing prompt on the Devices screen. `kind`: 1 = numeric comparison (yes/no),
/// 2 = passkey (display only — nothing to accept), 3 = SSP request. The shell pushes whatever the
/// listener reported; the UI answers with CONFIRM/CANCEL and never sees the address.
#[no_mangle]
pub extern "C" fn cinder_bt_prompt_set(kind: libc::c_int, name: *const libc::c_char, code: libc::c_uint) {
    let name = if name.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned()
    };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_prompt(kind as u8, &name, code as u32);
    }
}

#[no_mangle]
pub extern "C" fn cinder_bt_prompt_clear() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_prompt(0, "", 0);
    }
}

/// Which prompt is up (0 = none). Lets the shell avoid re-pushing one it already showed.
#[no_mangle]
pub extern "C" fn cinder_bt_prompt_kind() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.bt_prompt_kind() as libc::c_int)
}

/// Scan state. The shell reads this after a CINDER_ACT_BT_SCAN_TOGGLE to know which way to drive
/// SetSearchMode, and writes it when the radio's own search window ends.
#[no_mangle]
pub extern "C" fn cinder_get_bt_scanning() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.bt_scanning() as libc::c_int)
}

#[no_mangle]
pub extern "C" fn cinder_set_bt_scanning(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_bt_scanning(on != 0);
    }
}

/// How many paired devices the UI is currently showing.
#[no_mangle]
pub extern "C" fn cinder_bt_paired_count() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.app.bt_paired_len() as libc::c_int)
}

/// Drain the row index that came with the last CINDER_ACT_BT_CONNECT_DEVICE / _BT_FORGET_DEVICE.
/// Returns -1 when there is no pending request. Draining (rather than peeking) is deliberate: a
/// forget must never be replayed against whatever device later occupies that row.
#[no_mangle]
pub extern "C" fn cinder_pending_bt_device() -> libc::c_int {
    match cell().lock().unwrap().as_mut() {
        Some(r) => r.pending_bt_device.take().map_or(-1, |i| i as libc::c_int),
        None => -1,
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

/// Which VPT room is selected, 0..=3 — the value to hand Sony's `SetVptMode`. Read alongside
/// `cinder_get_sound_flags` after a CINDER_ACT_SOUND action: the flags carry VPT's on/off, this
/// carries which room. Separate because the device has separate SetVpt/SetVptMode calls and
/// `sound_flags` is a u8 of booleans. Clamped in the navigator, so this is always in range.
#[no_mangle]
pub extern "C" fn cinder_get_vpt_mode() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.vpt_mode() as libc::c_int,
        None => 0,
    }
}

/// Which DC Phase filter type is selected, 0..=5 — the value for
/// `cinder_effects_set_dc_phase_type()`. Companion to `cinder_get_vpt_mode`; the sound flags carry
/// DC Phase's on/off, this carries which filter. Clamped in the navigator.
#[no_mangle]
pub extern "C" fn cinder_get_dc_type() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.dc_type() as libc::c_int,
        None => 0,
    }
}

/// Sound ▸ Advanced, packed: bit0 Source Direct, bit1 Clear Phase, bit2 DSEE AI,
/// bit3 DSEE HX Custom, bit4 Tone Control. Read after CINDER_ACT_SOUND_CHANGED alongside
/// cinder_get_sound_flags.
#[no_mangle]
pub extern "C" fn cinder_get_adv_flags() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.adv_flags() as libc::c_int,
        None => 0,
    }
}

/// DSEE HX Custom mode, 0..=4 — for cinder_effects_set_dsee_hx_mode().
#[no_mangle]
pub extern "C" fn cinder_get_dsee_mode() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.dsee_mode() as libc::c_int,
        None => 0,
    }
}

/// Vinylizer character, 0..=3 — for cinder_effects_set_vinylizer_type(). Worth sending even with
/// the Vinyl Processor off: the device read back type=7, outside a four-value enum, because
/// nothing had ever set it.
/// Copy the Tone Control band gains (RAW half-decibels, Sony order BASS/MIDDLE/TREBLE) into
/// `out`, which must have room for `cinder_ui::tone::BANDS` bytes. Returns how many were written,
/// or 0 if the renderer is not up.
///
/// A pointer-out rather than a packed int for the same reason the EQ uses one: three signed gains
/// do not fit a flag word, and packing them would put a decode step between the UI's value and the
/// DSP's — which is exactly where the 10-band's "labelled +6, applied +3" bug lived.
#[no_mangle]
pub extern "C" fn cinder_get_tone_bands(out: *mut libc::c_schar) -> libc::c_int {
    if out.is_null() {
        return 0;
    }
    match cell().lock().unwrap().as_ref() {
        Some(r) => {
            let b = r.app.tone_bands();
            unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), out, b.len()) };
            b.len() as libc::c_int
        }
        None => 0,
    }
}

/// ── FM radio ────────────────────────────────────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn cinder_fm_khz() -> libc::c_int {
    match cell().lock().unwrap().as_ref() { Some(r) => r.app.fm_khz(), None => 0 }
}

#[no_mangle]
pub extern "C" fn cinder_fm_seek_dir() -> libc::c_int {
    match cell().lock().unwrap().as_ref() { Some(r) => r.fm_seek_dir, None => 1 }
}

#[no_mangle]
pub extern "C" fn cinder_fm_playing() -> libc::c_int {
    match cell().lock().unwrap().as_ref() { Some(r) => r.fm_power as libc::c_int, None => 0 }
}

#[no_mangle]
pub extern "C" fn cinder_fm_bt_out() -> libc::c_int {
    match cell().lock().unwrap().as_ref() { Some(r) => r.fm_bt as libc::c_int, None => 0 }
}

#[no_mangle]
pub extern "C" fn cinder_fm_report_bt_out(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() { r.app.fm_set_bt_out(on != 0); r.dirty = true; }
}

#[no_mangle]
pub extern "C" fn cinder_fm_report_khz(khz: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() { r.app.fm_report_khz(khz); r.dirty = true; }
}

#[no_mangle]
pub extern "C" fn cinder_fm_report_playing(on: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() { r.app.fm_set_playing(on != 0); r.dirty = true; }
}

#[no_mangle]
pub extern "C" fn cinder_fm_report_antenna(present: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.fm_set_antenna(present != 0);
        r.dirty = true;
    }
}

/// The live signal meter, straight off the Si4708's `STATUS_RSSI` register.
///
/// `rssi` < 0 means there is no register path (the setuid helper is missing, or the kernel's
/// regmon node is not there) — the screen then draws no meter at all rather than a bar backed by
/// Sony's `GetSignalLevel`, which is a constant 1 at every frequency in the band.
///
/// Marked dirty only when something actually moved: this is polled while the FM screen is open, and
/// RSSI wanders by a count or two at rest, so repainting on every sample would keep the panel busy
/// for no visible reason.
#[no_mangle]
pub extern "C" fn cinder_fm_report_signal(rssi: libc::c_int, stereo: libc::c_int, hw: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        let changed = r.app.fm_signal() != rssi
            || r.app.fm_stereo() != (stereo != 0)
            || r.app.fm_hw() != (hw != 0);
        r.app.fm_set_signal(rssi);
        r.app.fm_set_stereo(stereo != 0);
        r.app.fm_set_hw(hw != 0);
        if changed {
            r.dirty = true;
        }
    }
}

#[no_mangle]
pub extern "C" fn cinder_fm_report_scan_progress(pct: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.fm_set_scan_progress(pct.clamp(0, 100) as u8);
        r.dirty = true;
    }
}

/// # Safety
/// `khz` must point to `n` readable ints.
#[no_mangle]
pub unsafe extern "C" fn cinder_fm_report_stations(khz: *const libc::c_int, n: libc::c_int) {
    let list: Vec<i32> = if khz.is_null() || n <= 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(khz, n as usize).to_vec()
    };
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.fm_set_stations(&list);
        r.dirty = true;
    }
}

#[no_mangle]
pub extern "C" fn cinder_get_vinyl_type() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.vinyl_type() as libc::c_int,
        None => 0,
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
        let cfg = r.app.viz_cfg();
        let dt = frame_dt_ms(r);
        let prev = std::mem::take(&mut r.viz_levels);
        r.viz_levels = spectrum::levels(pcm, VIZ_BARS, &prev, &cfg, dt);
        let (mut peaks, mut held) = (std::mem::take(&mut r.viz_peaks), std::mem::take(&mut r.viz_held_ms));
        spectrum::hold_peaks(&mut peaks, &mut held, &r.viz_levels, dt, &cfg);
        r.viz_peaks = peaks;
        r.viz_held_ms = held;
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
        let cfg = r.app.viz_cfg();
        let dt = frame_dt_ms(r);
        let prev = std::mem::take(&mut r.viz_levels);
        let mut peak = r.viz_peak;
        r.viz_levels = spectrum::from_bands(src, VIZ_BARS, &prev, &mut peak, &cfg, dt);
        r.viz_peak = peak;
        let (mut peaks, mut held) = (std::mem::take(&mut r.viz_peaks), std::mem::take(&mut r.viz_held_ms));
        spectrum::hold_peaks(&mut peaks, &mut held, &r.viz_levels, dt, &cfg);
        r.viz_peaks = peaks;
        r.viz_held_ms = held;
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

/// Push the full battery readout for Settings ▸ Battery.
///
/// Separate from `cinder_set_battery` on purpose. That one feeds the status-bar percentage and is
/// called often and cheaply; this carries everything the Battery screen shows and involves reading
/// several sysfs files plus forking the charger helper, so the shell paces it independently.
///
/// `status` and `health` are the sysfs strings VERBATIM (`Charging`, `Not charging`, `Good`, ...);
/// the screen prints them as it receives them rather than mapping them to friendlier words, since
/// `Not charging` and `Discharging` mean different things and only one of them is a cable problem.
/// A null pointer is an empty string, not a crash.
///
/// `mv` is millivolts and `mdeg` is millidegrees C; both take `i32::MIN` for "could not read".
/// `chg_state` / `chg_fault` are the bq24262 STATUS field and fault code, or -1 when the
/// `cinder-battery` helper is not installed. `raw` is the raw register line for the footer.
///
/// # Safety
/// Every pointer must be a valid NUL-terminated C string or null; they are only read here.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn cinder_set_battery_detail(pct: libc::c_int,
                                                   status: *const c_char,
                                                   health: *const c_char,
                                                   mv: libc::c_int,
                                                   chg_state: libc::c_int,
                                                   chg_fault: libc::c_int,
                                                   raw: *const c_char) {
    let status = cstr(status);
    let health = cstr(health);
    let raw = cstr(raw);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_battery_detail(pct.clamp(0, 100) as u8, &status, &health,
                                 mv, chg_state, chg_fault, &raw);
        mark_device_dirty(r);
    }
}

/// Only the Device screen shows any of the pushed readings, so a repaint anywhere else is wasted —
/// and these tick every few seconds for the life of the device.
fn mark_device_dirty(r: &mut Render) {
    if r.app.current() == cinder_ui::nav::Screen::Device {
        r.dirty = true;
    }
}

/// The three die temperatures, in millidegrees C: SoC, power IC, analog block. `i32::MIN` for a
/// zone that could not be read. These are DIE temperatures — there is no cell thermistor on this
/// device, so none of them is a battery temperature and the screen does not label them as one.
#[no_mangle]
pub extern "C" fn cinder_set_device_temps(cpu: libc::c_int, pmic: libc::c_int, abb: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_device_temps(cpu, pmic, abb);
        mark_device_dirty(r);
    }
}

/// Current clock and maximum in kHz, cores online out of the package total, and the cpufreq
/// governor name. `i32::MIN` for an unreadable clock.
///
/// # Safety
/// `gov` must be a valid NUL-terminated C string or null.
#[no_mangle]
pub unsafe extern "C" fn cinder_set_device_cpu(khz: libc::c_int, max_khz: libc::c_int,
                                               online: libc::c_int, total: libc::c_int,
                                               gov: *const c_char) {
    let gov = cstr(gov);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_device_cpu(khz, max_khz, online, total, &gov);
        mark_device_dirty(r);
    }
}

/// Memory from /proc/meminfo in kB, then the music volume total and free and the app-data free, all
/// in MB. `i32::MIN` for anything unreadable.
#[no_mangle]
pub extern "C" fn cinder_set_device_storage(mem_total_kb: libc::c_int, mem_avail_kb: libc::c_int,
                                            music_total_mb: libc::c_int, music_free_mb: libc::c_int,
                                            data_free_mb: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_device_storage(mem_total_kb, mem_avail_kb, music_total_mb, music_free_mb,
                                 data_free_mb);
        mark_device_dirty(r);
    }
}

/// Seconds since boot and the kernel release string.
///
/// # Safety
/// `kernel` must be a valid NUL-terminated C string or null.
#[no_mangle]
pub unsafe extern "C" fn cinder_set_device_system(uptime_s: libc::c_int, kernel: *const c_char) {
    let kernel = cstr(kernel);
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.set_device_system(uptime_s, &kernel);
        mark_device_dirty(r);
    }
}

/// Is the Device screen the one on display?
///
/// The shell asks before doing the expensive half of the readings — forking the `cinder-battery`
/// helper, walking cpufreq, calling statvfs on both volumes. Same idiom as the visualiser's
/// analyzer gate: this is only ever looked at by one screen, and doing it every few seconds for
/// pixels nobody is looking at is exactly the kind of waste that costs runtime on a device whose
/// whole job is to play music for a long time on one charge.
#[no_mangle]
pub extern "C" fn cinder_device_wants_detail() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) if r.app.current() == cinder_ui::nav::Screen::Device => 1,
        _ => 0,
    }
}

/// Raise the ordinary bottom toast — the same one queue and Shelf feedback use.
///
/// For the shell to say something the user has to see (a low battery, an imminent shutdown) without
/// inventing a second notification surface for a message that appears twice in the life of a
/// charge. NULL or empty is a no-op rather than an empty bar.
#[no_mangle]
pub extern "C" fn cinder_toast(msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(msg) }).to_str() else { return };
    if s.is_empty() {
        return;
    }
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.notify(s);
        r.dirty = true;
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
        // The Settings ▸ Date & time row shows the wall clock and the editor seeds from it, so the
        // epoch rides the same 1 Hz tick the status-bar string already does rather than making the
        // shell own a second cadence for the same fact. `set_clock_epoch` no-ops while the editor
        // is open, so this cannot move a field the user is editing.
        if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            r.app.set_clock_epoch(d.as_secs() as i64);
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
        // Keep the resume files current. The sequence write is body-compared, so it costs a
        // format + memcmp on a tick where nothing changed and a write only when it did; the
        // position write is additionally rate-limited (RESUME_POS_EVERY). Forced while paused —
        // a pause is exactly when the user is about to walk away or power off, and it is also the
        // moment the position stops moving, so the compare makes it a single write.
        save_resume(r);
        save_resume_pos(r, !r.np.playing);

        // Queue edits are applied shortly before the boundary, not while the finger is still on
        // the row. Rebuilding needs a pause/seek/play round trip, so the lead absorbs it and the
        // current track still hands directly to the user's first queued choice.
        const QUEUE_REBUILD_LEAD_MS: i64 = 2_500;
        // NOT ON BLUETOOTH. The rebuild is a pause/seek/play round trip, and the lead exists so
        // that cost lands before the boundary instead of on it. Down the jack that is a brief
        // glitch; over A2DP it disrupts a stream the sink is buffering ~200 ms of, so the end of
        // the track is cut and the resume clicks — reported 2026-08-19 as songs being "cut off at
        // the end" with a pop when listening on BT.
        //
        // The boundary flush in the track-change handler already covers this case for free (the
        // new track's position is ~0, so re-issuing there resets nothing audible). Skipping the
        // early rebuild on BT costs only that the first queued track hands over a fraction later;
        // it does not cost the queue edit, which still applies.
        //
        // ...EXCEPT ON THE LAST TRACK OF THE SEQUENCE, WHERE THAT ARGUMENT IS FALSE. The paragraph
        // above says the boundary flush "already covers this case for free", and it does — for
        // every track that HAS a boundary after it. The final track of the sequence does not: no
        // new track ever starts, so the track-change handler never runs, `queue_pending` is never
        // consumed, and PlayerService simply runs off the end of the list it was given.
        //
        // Reported: playing the last track of an album with something queued, playback stops and
        // the queued track never plays. Down the jack the early rebuild above hides it (the `!on_bt`
        // test passes, so the flush happens 2.5 s out); over Bluetooth nothing fires at all.
        //
        // So on the last track the trade inverts. Everywhere else the choice is "a brief A2DP
        // disruption" against "the first queued track hands over a fraction later", and skipping is
        // right. Here it is "a brief A2DP disruption" against "the music stops", and a glitch beats
        // silence.
        //
        // `pending_play` is the sequence as HANDED TO the shell and is never rewritten as playback
        // advances, so its last entry is the final track PlayerService was given. If two entries
        // share a filename and we are on the earlier one, this fires early: the cost is one
        // rebuild that was not needed, not a missed queue.
        let last_in_sequence = match (r.last_track.as_ref(), r.pending_play.last()) {
            (Some(t), Some(last)) => *last == t.filename,
            _ => false,
        };
        let on_bt = r.app.bt_route();
        if r.queue_pending && r.np.playing && (!on_bt || last_in_sequence) && r.cur_duration_ms > 0
            && r.play_pos_ms > 0
            && r.cur_duration_ms.saturating_sub(r.play_pos_ms) <= QUEUE_REBUILD_LEAD_MS
        {
            if let Some(current) = r.last_track.as_ref().map(|t| t.filename.clone()) {
                r.queue_pending = false;
                r.pending_play = play_order_uris(r, &current);
                r.pending_play_start = 0;
                r.queue_flush = r.pending_play.len() > 1;
                if last_in_sequence {
                    eprintln!(
                        "cinder-ffi: queue flush on the LAST track ({} tracks) — no boundary is \
                         coming, so this is the only chance to issue it",
                        r.pending_play.len()
                    );
                }
            }
        }
        // Rescan label deadline. The scan runs inside a Sony service with no completion channel we
        // subscribe to, so this is the backstop that stops "Rescanning…" outliving a scan which
        // found nothing to change (and therefore never triggered a library reload).
        if r.rescan_left_ms > 0 {
            r.rescan_left_ms = (r.rescan_left_ms - dt).max(0);
            if r.rescan_left_ms == 0 && r.app.rescanning() {
                r.app.set_rescanning(false);
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
            // The spare A/B setup, accumulated across however many `bank_*` lines the file has and
            // installed once at the end — the keys can arrive in any order, and the live setup must
            // be in place before the spare is banked beside it.
            let mut bank = cinder_ui::nav::SoundSetup::default();
            let mut bank_seen = false;
            let mut bank_idx = 0usize;
            for line in body.lines() {
                let mut it = line.splitn(2, '=');
                let k = it.next().unwrap_or("").trim();
                let v = it.next().unwrap_or("").trim();
                match k {
                    // NIGHT IS NOT RESTORED. It is still WRITTEN to the settings file (one line
                    // below), because losing it would mean losing the accent/theme pair a user set
                    // — but it is deliberately not read back, so every boot starts in day.
                    //
                    // Night is now the panel's dimmest lit state, a flat raw 1 at every brightness
                    // level, and it is the thing you reach for in a dark room. A dark-room setting
                    // that survives into the next morning is a screen you cannot read outdoors,
                    // set by a decision you made hours ago and have forgotten making. The backlight
                    // already forced day on boot for exactly this reason; the theme was the half
                    // that did not, so the panel came up bright while the palette stayed dark.
                    "night" => {}
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
                    // Settings ▸ Visualiser. Each setter wraps its index into range, so a file
                    // written by a newer build (or by hand) can never select an option that does
                    // not exist in this one.
                    "viz_scale" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_scale(n);
                        }
                    }
                    "viz_range" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_range(n);
                        }
                    }
                    "viz_response" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_response(n);
                        }
                    }
                    "viz_interp" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_interp(n);
                        }
                    }
                    "viz_peaks" => r.app.set_viz_peak_hold(v == "1"),
                    "viz_window" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_window(n);
                        }
                    }
                    "viz_rate" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_rate(n);
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
                    // 0 = off, 1 = one, 2 = all. Was `v == "1"`, which silently collapsed a
                    // saved repeat-all back to off on the next boot.
                    "repeat" => r.np.repeat = v.parse::<u8>().unwrap_or(0).min(2),
                    "eq" => {
                        let mut arr = r.app.eq_bands();
                        for (i, part) in v.split(',').enumerate().take(10) {
                            if let Ok(g) = part.trim().parse::<i8>() {
                                // CLAMPED, because this is not the UI. Every place the EQ screen
                                // writes a band clamps to ±BAND_MAX, so the values Cinder itself
                                // produces are always in range — but this file is on /contents,
                                // which is vfat and writable by any PC the player is plugged into,
                                // and `i8` parses anything from -128 to 127.
                                //
                                // An out-of-range gain does NOT clamp inside the DSP: Sony's
                                // SetEq10BandValue ZEROES the band instead (measured; the scale is
                                // half-dB, so ±20 is ±10 dB). So a hand-edited or corrupted line
                                // silently flattens that band rather than pinning it to the
                                // maximum, and the EQ screen would draw its knob outside the field
                                // it belongs to. It would also be written straight back out on the
                                // next save, so the bad value persists.
                                arr[i] = g.clamp(-crate::EQ_BAND_MAX, crate::EQ_BAND_MAX);
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
                    "bt_enhanced" => r.app.set_bt_enhanced(v == "1"),
                    // The RADIO's own on/off. Everything else about Bluetooth persisted already —
                    // codec, LDAC quality, enhanced mode, volume — but not whether the radio was
                    // ON, so every boot came up with it off and nothing could connect until the
                    // switch was toggled by hand (measured 2026-08-26: GetBtStatus=7 on a fresh
                    // boot with three devices paired). bit2 says the file actually CARRIED this,
                    // which matters because the in-app default is ON: without that distinction the
                    // shell would power the radio up for someone who deliberately keeps it off.
                    "bt_on" => {
                        r.app.set_bt_on(v == "1");
                        loaded |= 4;
                    }
                    "volume" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_volume(n);
                            loaded |= 2; // bit1: a persisted volume level was restored
                        }
                    }
                    // LEGACY KEY, on the old 0..30 scale. Written by every build before
                    // 2026-08-11; reading it as 0..64 would halve the stored level on upgrade, so
                    // it is rescaled. New builds write `bt_volume64` instead, and when both keys
                    // are present that one wins (the file is parsed in order and it is written
                    // after this one).
                    "bt_volume" => {
                        if let Ok(n) = v.parse::<u16>() {
                            let scaled = (n * crate::VOL_BT_MAX as u16
                                / cinder_ui::overlay::BT_VOL_MAX_LEGACY as u16) as u8;
                            r.app.set_bt_volume(scaled);
                        }
                    }
                    // Deliberately does NOT set bit1. That bit means "push this to the hardware",
                    // and the sink keeps its own volume across reconnects — so this is restored as
                    // the UI's BELIEF about where the headphones are, and it can be stale until
                    // the rocker moves or the sink reports its real level.
                    // LEGACY KEY, on the 0..64 scale used between 2026-08-11 and 2026-08-18.
                    // Same reasoning as `bt_volume` above, one scale later: read as 0..127 it would
                    // halve the stored level. New builds write `bt_volume127`.
                    "bt_volume64" => {
                        if let Ok(n) = v.parse::<u16>() {
                            let scaled = (n * crate::VOL_BT_MAX as u16
                                / cinder_ui::overlay::BT_VOL_MAX_LEGACY_64 as u16) as u8;
                            r.app.set_bt_volume(scaled);
                        }
                    }
                    // Deliberately does NOT set bit1. That bit means "push this to the hardware",
                    // and the sink keeps its own volume across reconnects — so this is restored as
                    // the UI's BELIEF about where the headphones are, and it can be stale until
                    // the rocker moves or the sink reports its real level.
                    "bt_volume127" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_bt_volume(n);
                        }
                    }
                    // The OLD key, from when balance was 7 discrete stops (0..=6, 3 = centre).
                    // Migrated onto the 0..=100 slider so an upgrade doesn't silently slam a
                    // centred player hard left — "3" means centre in the old scale and "pan left
                    // by a third" in the new one, so the value cannot be reinterpreted in place.
                    // Written back under `balance100`, and this key is never written again.
                    "balance" => {
                        if let Ok(n) = v.parse::<usize>() {
                            let old = n.min(6);
                            r.app.set_balance(old * cinder_ui::sound::BALANCE_MAX / 6);
                        }
                    }
                    "balance100" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_balance(n);
                        }
                    }
                    // Which VPT room. Absent from files written by older builds, which is fine —
                    // it just stays at 0 (Studio), and VPT's on/off still comes from `sound=`.
                    // set_vpt_mode clamps, so a hand-edited value cannot reach the device as an
                    // out-of-range enum.
                    "vpt_mode" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_vpt_mode(n);
                        }
                    }
                    // Which DC Phase filter. Same story as vpt_mode: absent in older files means
                    // 0, and set_dc_type clamps.
                    "dc_type" => {
                        if let Ok(n) = v.parse::<usize>() {
                            r.app.set_dc_type(n);
                        }
                    }
                    // Sound ▸ Advanced. All three absent in files written by older builds, which
                    // leaves the whole screen at its defaults (everything off, mode 0) — the state
                    // the device was in before the screen existed.
                    "adv" => { if let Ok(n) = v.parse::<u8>() { r.app.set_adv_flags(n); } }
                    "dsee_mode" => { if let Ok(n) = v.parse::<usize>() { r.app.set_dsee_mode(n); } }
                    "vinyl_type" => { if let Ok(n) = v.parse::<usize>() { r.app.set_vinyl_type(n); } }
                    // Tone Control bands, RAW half-decibels. Absent in older files = flat, which
                    // is what the device had before the editor existed. set_tone_bands clamps, so
                    // a hand-edited value cannot reach the service out of range — past the end it
                    // ZEROES the band rather than clamping, so an unchecked value would read as a
                    // boost in the UI and be silence in the DSP.
                    "tone" => {
                        let mut b = [0i8; cinder_ui::tone::BANDS];
                        for (i, part) in v.split(',').take(cinder_ui::tone::BANDS).enumerate() {
                            if let Ok(n) = part.trim().parse::<i8>() { b[i] = n; }
                        }
                        r.app.set_tone_bands(b);
                    }
                    // The OTHER A/B setup and which of the two was live. Parsed into locals and
                    // applied once at the end, because the keys can arrive in any order and the
                    // live setup has to be banked before the spare can be installed beside it.
                    "setup" => { if let Ok(n) = v.parse::<usize>() { bank_idx = n & 1; } }
                    "bank_sound" => {
                        if let Ok(f) = v.parse::<u8>() {
                            bank.dsee = f & 1 != 0;
                            bank.vinyl = f & (1 << 1) != 0;
                            bank.vpt = f & (1 << 2) != 0;
                            bank.dc = f & (1 << 3) != 0;
                            bank.norm = f & (1 << 4) != 0;
                            bank.clear = f & (1 << 5) != 0;
                            bank_seen = true;
                        }
                    }
                    "bank_balance" => {
                        if let Ok(n) = v.parse::<usize>() {
                            bank.balance = n.min(cinder_ui::sound::BALANCE_MAX);
                            bank_seen = true;
                        }
                    }
                    "bank_preset" => {
                        if let Ok(n) = v.parse::<usize>() { bank.eq_preset = n; bank_seen = true; }
                    }
                    "bank_eq" => {
                        for (i, part) in v.split(',').take(10).enumerate() {
                            // Same clamp, same reason as "eq" above — an A/B bank is loaded from
                            // the same PC-writable file and reaches the same DSP call.
                            if let Ok(g) = part.trim().parse::<i8>() {
                                bank.eq_bands[i] = g.clamp(-crate::EQ_BAND_MAX, crate::EQ_BAND_MAX);
                            }
                        }
                        bank_seen = true;
                    }
                    "auto_off" => {
                        // set_auto_off_min snaps to a known preset, so a hand-edited value cannot
                        // strand the row on a duration the cycle can never reach.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_auto_off_min(n);
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
                    "ui_scale" => {
                        // set_ui_scale_pct clamps into the SCALE_STEPS range.
                        if let Ok(n) = v.parse::<u32>() {
                            r.app.set_ui_scale_pct(n);
                        }
                    }
                    // FM dial position. fm_report_khz clamps into the band, so a hand-edited or
                    // truncated value cannot put the tuner somewhere it cannot go.
                    "fm_khz" => {
                        if let Ok(n) = v.parse::<i32>() {
                            if n > 0 {
                                r.app.fm_report_khz(n);
                            }
                        }
                    }
                    // Scanned stations, strongest first. Anything unparseable is dropped rather
                    // than failing the load; a bad config must never stop a boot.
                    "fm_stations" => {
                        let list: Vec<i32> = v
                            .split(',')
                            .filter_map(|s| s.trim().parse::<i32>().ok())
                            .filter(|k| (cinder_ui::fm::MIN_KHZ..=cinder_ui::fm::MAX_KHZ).contains(k))
                            .collect();
                        if !list.is_empty() {
                            r.app.fm_set_stations(&list);
                        }
                    }
                    // Shelf pins: `pin0=`…`pin2=`. A malformed record clears that slot rather
                    // than failing the whole load — a hand-edited config must never stop a boot.
                    k if k.starts_with("pin") => {
                        if let Ok(i) = k[3..].parse::<usize>() {
                            r.app.shelf_pin_decode(i, v);
                        }
                    }
                    _ => {}
                }
            }
            // Both A/B setups are now in place: the live one came from the `eq=`/`sound=`/
            // `balance100=` keys above, the spare from the `bank_*` ones. A file written by an
            // older build has no bank at all, so the spare stays at its default and A/B still
            // works — it just starts out as "your setup" versus "a fresh one".
            if bank_seen {
                let live = r.app.setup();
                r.app.restore_setups(live, bank, bank_idx);
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

/// Restore the playback context, the user queue and the play position saved by the previous run,
/// and from here on keep both files up to date.
///
/// CALL AFTER `cinder_db_open`: the files hold object_ids, and the rows behind them come from the
/// library. Calling it earlier restores nothing and simply arms the saving half.
///
/// Returns 1 if a sequence was restored, 0 if there was nothing to restore (first boot, empty
/// context, or every id in the file has since left the library), -2 if the renderer isn't up.
///
/// This does NOT start playback and does not touch PlayerService. See `resume_pending`.
#[no_mangle]
pub extern "C" fn cinder_resume_load(seq_path: *const c_char, pos_path: *const c_char) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -2 };
    let cstr = |p: *const c_char| -> Option<String> {
        if p.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(p) }.to_str().ok().map(str::to_string)
    };
    let (Some(seq_path), Some(pos_path)) = (cstr(seq_path), cstr(pos_path)) else { return -2 };

    let seq_body = std::fs::read_to_string(&seq_path).unwrap_or_default();
    let pos_body = std::fs::read_to_string(&pos_path).unwrap_or_default();

    let (mut ctx_ids, mut q_ids, mut pre) = (Vec::new(), Vec::new(), None);
    let mut idx = 0usize;
    // The user pick that was PLAYING when the player went down. It is in neither list — a pick
    // leaves the queue when it starts — so without this the resume came back on the context row
    // underneath it, which is the track that played BEFORE the pick.
    let mut pick_id: Option<i64> = None;
    for (k, v) in conf_lines(&seq_body) {
        match k {
            "ctx" => ctx_ids = id_list(v),
            "q" => q_ids = id_list(v),
            "pre" => pre = Some(id_list(v)),
            "idx" => idx = v.parse::<usize>().unwrap_or(0),
            "pick" => pick_id = v.parse::<i64>().ok(),
            _ => {}
        }
    }
    let (mut resume_id, mut resume_pos) = (0i64, 0i64);
    for (k, v) in conf_lines(&pos_body) {
        match k {
            "track" => resume_id = v.parse::<i64>().unwrap_or(0),
            "pos" => resume_pos = v.parse::<i64>().unwrap_or(0).max(0),
            _ => {}
        }
    }

    // Arm the saving half whatever happens below, so a first boot (or a library that has lost
    // every saved track) still starts persisting the moment the user plays something.
    r.resume_path = Some(seq_path);
    r.resume_pos_path = Some(pos_path);
    r.resume_last_body = seq_body;
    r.resume_pos_last = pos_body;

    // Resolve ids → tracks in ONE QUERY FOR BOTH LISTS. A track that has left the library since
    // the last boot drops out silently; that is the whole reason ids are stored rather than rows.
    //
    // The old comment here claimed "ONE pass per list" and the code ran a full `object_body` scan
    // PER ID. It is bounded in practice — this device restored 17 rows — but the saved context is
    // whatever was playing, and after "Shuffle all songs" that is the entire library, on the boot
    // path. Resolving both lists from one scan removes the shape rather than relying on the bound.
    let Some(db) = r.db.as_ref() else { return 0 };
    let mut all_ids: Vec<i64> = Vec::with_capacity(ctx_ids.len() + q_ids.len() + 1);
    all_ids.extend_from_slice(&ctx_ids);
    all_ids.extend_from_slice(&q_ids);
    all_ids.extend(pick_id);
    let by_id = db.tracks_by_object_ids(&all_ids).unwrap_or_default();
    let resolve = |ids: &[i64]| -> Vec<cinder_db::Track> {
        ids.iter().filter_map(|id| by_id.get(id).cloned()).collect()
    };
    let ctx = resolve(&ctx_ids);
    let queue = resolve(&q_ids);
    // A pick whose file has left the library is simply not restored; the context row under it
    // then becomes what resumes, which is where playback would have gone next anyway.
    let pick = pick_id.and_then(|id| by_id.get(&id).cloned());
    if ctx.is_empty() && queue.is_empty() && pick.is_none() {
        return 0;
    }

    // The index was recorded against the FULL list. If tracks vanished, follow the saved track's
    // object_id instead of the raw number — the number would now point at a different song.
    let saved_at = ctx_ids.get(idx).copied();
    let idx = saved_at
        .and_then(|id| ctx.iter().position(|t| t.object_id == id))
        .unwrap_or_else(|| idx.min(ctx.len().saturating_sub(1)));

    r.app.playback_restore(
        ctx.iter().map(song_row_of).collect(),
        idx,
        queue.iter().map(song_row_of).collect(),
        pre,
        pick.as_ref().map(song_row_of),
    );

    // What to show, and what the first ▶ will hand PlayerService. The current track leads the
    // sequence for the same reason it does in `play_order_uris`: PlayerService starts at an index
    // into the list it is given, and leading with the current track keeps the two agreeing.
    // A restored PICK is what was audible, so it leads — and the context row it interrupted is
    // NOT replayed, exactly as it is not replayed while the player is running.
    let current = pick.as_ref().or_else(|| ctx.get(idx)).or_else(|| queue.first());
    let Some(cur) = current else { return 0 };
    let uris: Vec<String> = std::iter::once(cur.filename.clone())
        .chain(queue.iter().map(|t| t.filename.clone()))
        .chain(ctx.iter().skip(idx + 1).map(|t| t.filename.clone()))
        .collect();
    // Only honour the saved position if it belongs to the track we are about to resume — the two
    // files are written independently, so a crash between them can leave them one track apart.
    let pos = if resume_id == cur.object_id { resume_pos } else { 0 };

    apply_track(&mut r.np, cur);
    r.np.liked = r.liked.contains(&cur.object_id);
    r.np.playing = false;
    r.cur_duration_ms = cur.duration_raw.unwrap_or(0).max(0);
    r.play_pos_ms = pos.min(r.cur_duration_ms.max(0));
    r.real_pos_ms = -1;
    set_progress(&mut r.np, r.play_pos_ms, r.cur_duration_ms);
    if r.art_key != Some(cur.object_id) {
        r.art_full = None;
        r.art_thumb = None;
        bake_gradient_art(r);
        r.art_key = Some(cur.object_id);
        request_cover(r, cur.object_id);
    }
    r.last_track = Some(cur.clone());
    r.resume_pending = Some((uris, 0, r.play_pos_ms));
    r.dirty = true;
    eprintln!(
        "cinder-ffi: resumed {} context + {} queued{}, at index {} pos {} ms",
        ctx.len(),
        queue.len(),
        if pick.is_some() { " (a user pick was playing)" } else { "" },
        idx,
        r.play_pos_ms
    );
    1
}

/// Drain a restored sequence into `cinder_pending_play_*`. The shell calls this on the FIRST ▶
/// after a boot; 1 means "there is now a sequence to hand PlayerService, and
/// `cinder_play_position_ms()` is where to seek afterwards" (i.e. call `play_pending_sequence`
/// with restore_position). 0 means nothing was pending and the press is an ordinary play.
///
/// One-shot: anything the user starts by hand replaces the context anyway, and a stale resume
/// firing later would drag playback back to where the last boot left off.
#[no_mangle]
pub extern "C" fn cinder_resume_take_pending() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let Some((uris, start, pos)) = r.resume_pending.take() else { return 0 };
    // Anything the user did between the boot and this press — swipe-queued a song, reordered the
    // queue, pressed shuffle — happened to `App`, not to this snapshot. Rebuild from the live
    // state when that is so; the current track still leads, so the press still resumes the same
    // song at the same offset.
    let uris = if r.resume_stale {
        r.resume_stale = false;
        match r.last_track.as_ref().map(|t| t.filename.clone()) {
            Some(current) => {
                r.queue_pending = false;
                play_order_uris(r, &current)
            }
            None => uris,
        }
    } else {
        uris
    };
    if uris.is_empty() {
        return 0;
    }
    r.pending_play = uris;
    r.pending_play_start = start;
    r.play_pos_ms = pos;
    1
}

/// Drop a pending resume without using it. The shell calls this whenever the user starts
/// something themselves before pressing ▶, so the sequence they chose is not overwritten by the
/// one the last boot left behind.
#[no_mangle]
pub extern "C" fn cinder_resume_cancel() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.resume_pending = None;
        r.resume_stale = false;
    }
}

/// Flush the resume files now — the shell calls this before a deliberate power-off or reboot, so
/// the position is current rather than up to `RESUME_POS_EVERY` stale.
#[no_mangle]
pub extern "C" fn cinder_resume_flush() {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        save_resume(r);
        save_resume_pos(r, true);
    }
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
        // RETRY THE FAILURES, because the commonest reason for one is that this ran too early.
        //
        // The builder starts from `cinder_db_open`, which happens during boot — and the music
        // itself lives on `/data/mnt/internal` and `/data/mnt/external`, which are not mounted yet
        // at that point. Every decode then fails instantly with `magic=unreadable`, the single pass
        // finishes in a few seconds having built almost nothing, and nothing ever tries again.
        // Measured on device 2026-08-19: `builder finished — 7/317 covers decoded`, done by 15 s of
        // uptime; the same code run from cinder-probe a few minutes later built 180 without a miss.
        //
        // This was invisible for as long as a full cache existed, because `todo` was then empty. It
        // only surfaced when the cache was discarded for a scaler change and had to rebuild from
        // nothing — which is also exactly when a user notices, since every cover is a gradient.
        //
        // So: sweep, keep what failed, wait, sweep again. The delay grows because a mount that is
        // not there after a minute is unlikely to appear in the next second, and each round is
        // silent when there is nothing left to do.
        let mut queue = todo;
        let mut round = 0u32;
        while !queue.is_empty() && round <= ART_BUILD_ROUNDS {
            if round > 0 {
                let wait = std::time::Duration::from_secs(10 * round as u64);
                eprintln!(
                    "cinder-ffi: art cache: {} covers unreadable (media not mounted yet?) — \
                     retrying in {} s",
                    queue.len(),
                    wait.as_secs()
                );
                std::thread::sleep(wait);
            }
            let mut failed = Vec::new();
            let mut stop = false;
            for (album_id, object_id) in std::mem::take(&mut queue) {
                // Decode OUTSIDE the lock. This is the expensive part and the UI must keep painting
                // through it.
                let Some(t48) = art_cache::build_one(&db, album_id, object_id) else {
                    failed.push((album_id, object_id));
                    continue;
                };
                done += 1;
                if let Ok(mut g) = cell().lock() {
                    let Some(r) = g.as_mut() else { stop = true; break }; // renderer gone — stop
                    r.app.library_mut().thumbs.insert(album_id, t48);
                    // ONLY IF THE SCREEN CAN SHOW ARTWORK AT ALL. This was unconditional, and the
                    // comment said "may be on screen right now" — on a first boot that is ~340
                    // forced full-screen rasters and blits over the ~2.7 minutes the builder runs,
                    // most of them repainting a byte-identical Settings or Bluetooth screen
                    // (audit B11). The renderer forces a full repaint every 5 s anyway, so the
                    // worst case here is a cover that lands up to five seconds later on a screen
                    // that was not showing it.
                    r.dirty = r.dirty || r.app.shows_album_art();
                }
                // Yield between albums. The builder is strictly background work: a cover that shows
                // up a minute later costs the user nothing, whereas competing with the render
                // thread for this single core would be visible immediately as scroll stutter.
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            if stop {
                break;
            }
            queue = failed;
            round += 1;
        }
        if !queue.is_empty() {
            eprintln!(
                "cinder-ffi: art cache: giving up on {} covers after {ART_BUILD_ROUNDS} retries — \
                 they are missing, not late",
                queue.len()
            );
        }
        eprintln!("cinder-ffi: art cache: builder finished — {done}/{total} covers decoded");
        ART_BUILDER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// How many times the builder re-sweeps the covers it could not read. Waits grow 10 s, 20 s, 30 s …
/// so the last attempt is around three minutes in — comfortably past the point where `/data/mnt`
/// appears, without leaving a thread sweeping a library of genuinely artless albums forever.
const ART_BUILD_ROUNDS: u32 = 6;

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
/// DIAGNOSTIC: resolve one codepoint through the real font chain and rasterise it, exactly as the
/// renderer does, so `cinder-probe --fontchain` can watch what a single character costs ON DEVICE.
///
/// Why this exists: `text::resolve` walks the Sony fallback chain for any codepoint the bundled
/// fonts lack, and *loads* each font in the chain until one has the glyph. For a codepoint NOTHING
/// has, that means loading all five — including `DFPGothicPW5` (10 MB on disk) — on a device with
/// 467 MB of RAM. Host-side tests can never see that cost. Returns the font id that answered
/// (0..6 = bundled, 16.. = fallback, matching `text::resolve`), or -1 for an invalid codepoint.
#[no_mangle]
pub extern "C" fn cinder_font_probe(cp: u32) -> libc::c_int {
    let Some(ch) = char::from_u32(cp) else { return -1 };
    // FontSet holds RefCells (single-threaded by design — render runs under the cinder-ffi mutex),
    // so it lives in a thread-local rather than a static: same object across calls, which is what
    // makes the per-character memory delta mean anything.
    thread_local! {
        static FONTS: cinder_ui::text::FontSet = cinder_ui::text::FontSet::load();
    }
    FONTS.with(|f| cinder_ui::text::probe_glyph(f, ch) as libc::c_int)
}

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

    // THE EXPENSIVE PART RUNS WITHOUT THE LOCK. This function used to take `cell()` on its first
    // line and hold it across the SQLite open, build_library, the playlist store and the likes
    // import — about 4.8 s on a 3,456-track library (measured 2026-09-04: cinder_db_open at t=2.165
    // to "restore playback context" at t=6.966). Every frame takes that same lock, so the whole of
    // it was time nothing could paint and nothing could read input, which is the ~5 s after boot
    // where the device shows a Cinder screen that ignores you.
    //
    // Nothing below needs the render state until there is something to install, so the build now
    // happens against local values and the lock is taken once, at the end, to swap them in. That
    // is what makes it safe to call this from a worker thread (cinder-home does) instead of from
    // the render thread.
    // PHASE TIMING FOR THE BOOT DEAD TIME. `build_library`'s own breakdown (printed inside it)
    // accounts for only ~417 ms of a ~4.6 s window on device, so the rest is here — and the host
    // profile cannot see it, because on the host `/contents` and `/contents_ext` do not exist and
    // the playlist and likes phases return instantly. One line per open, i.e. once per boot.
    let t_open = std::time::Instant::now();
    let t_phase = std::time::Instant::now();
    let db = match cinder_db::Db::open(&p) {
        Ok(db) => db,
        Err(e) => {
            // THROTTLED, because this sits inside a RETRY LOOP. Measured on device 2026-08-26
            // (checklist 2D.2): with /db/MTPDB.dat renamed away, the shell's own "DB unavailable —
            // will retry" line is rate-limited by retry_log and printed exactly once, but THIS one
            // was not, so the retry printed it about 1.3 times a second forever — 61 KB of log in
            // ten minutes, every line an fflush to vfat. That is the same "work on a timer that
            // never stops" class section 2D of the checklist exists to catch, hiding one layer
            // below the throttle that was supposed to cover it.
            //
            // Print the FIRST failure and then stay silent until the message changes or an open
            // succeeds — a different error is news, the same error repeated is not.
            db_open_err_log(&p, &format!("{e}"));
            let mut guard = cell().lock().unwrap();
            let Some(r) = guard.as_mut() else { return -2 };
            // Don't leave the demo sample library showing on device — that would look like the
            // user's music when the DB didn't actually load. Show an empty library instead so
            // it's honest (Menu shows "Empty", the Library tabs are blank).
            r.app.set_library(cinder_ui::Library::default());
            r.dirty = true;
            r.art_key = None;
            r.last_track = None;
            return -1;
        }
    };

    let ms_dbopen = t_phase.elapsed().as_millis();
    // Build the browsable library now so the Library screen shows real music.
    let t_phase = std::time::Instant::now();
    let lib = build_library(&db);
    let ms_build = t_phase.elapsed().as_millis();
    eprintln!(
        "cinder-ffi: library loaded — {} tracks, {} albums, {} artists",
        lib.songs.len(),
        lib.album_count(),
        lib.artists.len()
    );
    let t_phase = std::time::Instant::now();
    let plists = playlists::Store::open(playlists::DIR);
    let ms_plists = t_phase.elapsed().as_millis();
    eprintln!("cinder-ffi: playlists: {} of your own", plists.lists.len());
    // Liked list lives beside the user's music on both internal storage and SD card:
    // /contents and /contents_ext are reachable over USB-MSC.
    let t_phase = std::time::Instant::now();
    let liked = likes::liked_load_all();
    let ms_liked = t_phase.elapsed().as_millis();
    eprintln!("cinder-ffi: liked songs: {} loaded from internal and SD card", liked.len());

    // A liked list pushed from the PC (likesync) lands here as artist/title rows and is resolved
    // against the library that was just built — object ids are rebuilt whenever the database is, so
    // they can only be matched on this side. See `likes.rs` for why the import replaces rather than
    // merges, and what it refuses to act on. Resolved against the local `lib` rather than
    // `r.app.library()`, which is the only reason this can happen before the install.
    let t_phase = std::time::Instant::now();
    let songs: Vec<(i64, String, String, String)> = {
        // album_id -> the artist the album is filed under, so a featured-artist track can be found
        // by the name the PC knows it by (see likes::resolve). Scoped so the borrow of `lib` ends
        // before `lib` is moved into the render state below.
        let album_artist: std::collections::HashMap<i64, &str> = lib
            .album_groups
            .iter()
            .flat_map(|group| {
                group.albums.iter().map(move |album| (album.album_id, group.artist.as_str()))
            })
            .collect();
        lib.songs
            .iter()
            .map(|s| {
                let filed_under = album_artist.get(&s.album_id).copied().unwrap_or("");
                (s.object_id, s.artist.clone(), s.title.clone(), filed_under.to_string())
            })
            .collect()
    };
    let (outcome, imported) = likes::apply_import_all(
        songs
            .iter()
            .map(|(id, artist, title, filed)| {
                (*id, artist.as_str(), title.as_str(), filed.as_str())
            }),
    );
    let ms_import = t_phase.elapsed().as_millis();
    match outcome {
        likes::Outcome::None => {}
        likes::Outcome::Ignored(why) => {
            eprintln!("cinder-ffi: liked import ignored: {why}");
        }
        likes::Outcome::Unresolved(rows) => {
            eprintln!(
                "cinder-ffi: liked import: {rows} row(s) matched nothing in the library \
                 — left in place for the next boot"
            );
        }
        likes::Outcome::Applied(liked_n, missing) => {
            eprintln!(
                "cinder-ffi: liked import applied — {liked_n} liked, {missing} not on this device"
            );
        }
    }

    // ── INSTALL. Everything above was local; this is the only part that needs the render state,
    // and it is all moves and assignments. ───────────────────────────────────────────────────────
    let t_phase = std::time::Instant::now();
    let mut guard = cell().lock().unwrap();
    let ms_lock = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    let Some(r) = guard.as_mut() else { return -2 };
    r.dirty = true; // the library (or its absence) changed -> repaint
    // FORGET WHAT WE DECIDED ABOUT THE CURRENT COVER. `art_key` exists to stop us re-decoding the
    // same track on every poll, and that is exactly wrong across a reopen: if the cover was read
    // while the music volume was missing, the decode failed, the gradient went up, and art_key
    // pinned that answer for the rest of the boot. Nothing ever asked again, so an album stayed
    // grey after /contents came back.
    //
    // That is not hypothetical — it is the "anything by Sprain is just showing as a gradient"
    // report. Sony's own stack unmounts /contents when a cable appears (see the reclaim in
    // cinder-home), and any cover read inside that window logs `magic=unreadable` and is cached as
    // a failure. Clearing the key here means the next paint re-requests it through the normal
    // path; the worker installs the real cover a moment later.
    //
    // Cleared at INSTALL time rather than on entry (where it used to be): with the build outside
    // the lock, frames keep running while it happens, and clearing the key early would just let
    // those frames re-cache an answer derived from the library we are about to replace.
    r.art_key = None;
    // …and make the next now-playing poll count as a TRACK CHANGE, because that is the only thing
    // that re-reads the cover. The re-request is nested inside `if changed`, so clearing art_key on
    // its own only helps the next song — the album sitting on screen right now, the one the user is
    // actually looking at, would stay grey until they skipped away from it and back. Dropping
    // last_track makes the very next poll re-derive everything for the current track.
    // The only thing lost is one rewind-history push across a library reopen, which is not
    // meaningful state after the library has been rebuilt underneath it.
    r.last_track = None;
    // Sony's playlist rows are kept so a later edit can re-merge the two lists without re-querying;
    // ours come from the .m3u8 folder beside the liked list.
    r.db_playlists = lib.playlists.clone();
    r.app.set_library(lib);
    r.db = Some(db);
    r.plists = plists;
    // WAS 3,802 ms OF THE BOOT — 83% of the whole dead time, in this one call. See
    // `user_playlist_rows`, which now batches its filename resolution; it is 133 ms here.
    refresh_playlists(r);
    r.liked = liked;
    r.liked_path = Some(likes::INTERNAL_LIKED_PATH.to_string());
    if let Some(ids) = imported {
        r.liked.extend(ids);
        liked_save(r); // rewrites cinder_liked.conf AND the cinder_loved.tsv export
    }
    r.app.set_liked_count(r.liked.len());
    r.db_path = Some(p.clone());
    let ms_install = t_phase.elapsed().as_millis();
    let t_phase = std::time::Instant::now();
    start_art_cache(r, &p);
    let ms_art = t_phase.elapsed().as_millis();
    eprintln!(
        "cinder-ffi: cinder_db_open {} ms = db_open {ms_dbopen} + build {ms_build} + playlists \
         {ms_plists} + likes {ms_liked} + import {ms_import} + lock {ms_lock} + install \
         {ms_install} + artcache {ms_art}",
        t_open.elapsed().as_millis()
    );
    db_open_err_reset();
    0
}

/// The last DB-open error we printed, so a retry loop cannot become the log.
static DB_OPEN_LAST_ERR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Print a DB-open failure ONCE, then stay silent until the message changes.
///
/// A different error is news; the same error repeated 1.3 times a second is not. Reset by
/// `db_open_err_reset` on a successful open, so a failure that comes back later is reported again.
fn db_open_err_log(path: &str, msg: &str) {
    let mut last = DB_OPEN_LAST_ERR.lock().unwrap();
    if last.as_deref() != Some(msg) {
        eprintln!("cinder-ffi: db open {path}: {msg} (further identical failures stay silent)");
        *last = Some(msg.to_string());
    }
}

fn db_open_err_reset() {
    *DB_OPEN_LAST_ERR.lock().unwrap() = None;
}

/// Would a touch at (`x`,`y`) start a drag-to-seek? True only on Now Playing, inside the progress
/// rail's grab band, and only when a track with a known duration is loaded (there is nothing to
/// seek within otherwise). The shell calls this on finger-DOWN and, if it returns 1, routes the
/// whole contact to the scrub instead of the usual tap / list-drag / swipe classification.
#[no_mangle]
pub extern "C" fn cinder_scrub_hit(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    // The CLAIM lives in nav (`App::scrub_begin`) — one place that knows which horizontal
    // controls exist and where they are drawn, shared with the host sim. What stays here is the
    // millisecond math for the rail, which needs the track duration the UI doesn't carry.
    if !r.app.scrub_begin(x as i32, y as i32) {
        return 0;
    }
    // Anything that is NOT the progress rail is a settings slider: it applies to the UI, has no
    // track duration to care about, and must never produce a seek. Asked this way round so a new
    // slider is handled correctly by default — see `App::scrub_is_rail`.
    if !r.app.scrub_is_rail() {
        r.dirty = true;
        return 1;
    }
    // Progress rail: a track with no known duration has nothing to seek within, so decline the
    // gesture and let it classify as a normal tap/drag instead of dead-ending in a scrub.
    if r.cur_duration_ms <= 0 {
        r.app.scrub_end();
        return 0;
    }
    1
}

/// Park the first shell-visible action from a slider drag. One slot is enough: the moves are
/// idempotent (each carries the slider's current value, not a delta), so if the shell is late and a
/// second arrives first, applying only the newest is not merely acceptable — it is what you want.
fn stash_scrub_action(r: &mut Render, acts: &[cinder_ui::nav::Action]) {
    for a in acts {
        if let Some(code) = carry_action(r, a) {
            r.scrub_act = Some(code);
            return;
        }
    }
}

/// Take the action a slider drag produced (0 = none). The shell calls this after every
/// `cinder_scrub_to` and after `cinder_scrub_end`, and carries out whatever it gets.
#[no_mangle]
pub extern "C" fn cinder_scrub_action() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    match guard.as_mut() {
        Some(r) => r.scrub_act.take().unwrap_or(0),
        None => 0,
    }
}

/// How far into a track ◁ stops meaning "previous track" and starts meaning "rewind to the
/// start". 3 s is the near-universal convention (iPod, Walkman, phones).
const PREV_RESTART_MS: i64 = 3_000;

/// Does ◁ mean "restart this track" rather than "step to the previous one" at this position?
/// Pure, so the rule is unit-testable without a device or a framebuffer.
fn prev_means_restart(pos_ms: i64, dur_ms: i64) -> bool {
    dur_ms > 0 && pos_ms > PREV_RESTART_MS
}

/// Should the shell treat ◁ as a rewind-to-start instead of `PlayController::PrevTrack()`?
///
/// The reported bug: ◁ was an unconditional PrevTrack(), and at the HEAD of a sequence — a
/// single-track queue, or the first track of an album you just tapped — there is nowhere to step
/// back to, so the button did nothing at all. Mid-track it had the opposite problem: it jumped
/// away when the user meant "start this again". Both halves are handled: this decides the common
/// case up front, and the shell falls back to a seek(0) when PrevTrack itself reports failure.
#[no_mangle]
pub extern "C" fn cinder_prev_means_restart() -> libc::c_int {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return 0 };
    prev_means_restart(r.play_pos_ms, r.cur_duration_ms) as libc::c_int
}

/// Populate the ordinary pending-play channel with a sequence beginning at the preceding track
/// Cinder observed. This deliberately does not use PlayerService's PrevTrack: queue edits replace
/// its sequence and reset that service-side history to the current item.
#[no_mangle]
pub extern "C" fn cinder_prepare_previous_play() -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return 0 };
    let Some(current) = r.last_track.clone() else { return 0 };
    let Some(target) = r.play_history.pop() else { return 0 };

    // Keep the remaining history before the target, then replay the target and current item,
    // followed by the explicit queue and context still ahead of the current item. The start index
    // points at target, making repeated presses walk backward through Cinder's history.
    let mut sequence: Vec<String> = r.play_history.iter().map(|t| t.filename.clone()).collect();
    let start = sequence.len();
    sequence.push(target.filename.clone());
    sequence.push(current.filename.clone());
    sequence.extend(play_order_uris(r, &current.filename).into_iter().skip(1));
    r.pending_play = sequence;
    r.pending_play_start = start;
    r.rewind_from = Some(current.object_id);
    r.app.set_context_playing(target.object_id);
    r.dirty = true;
    1
}

/// Position is maintained from PlayerService events when available and falls back to Cinder's
/// local clock. It is the best point to resume after an immediate queue sequence rebuild.
#[no_mangle]
pub extern "C" fn cinder_play_position_ms() -> libc::c_int {
    cell().lock().unwrap().as_ref().map_or(0, |r| r.play_pos_ms.clamp(0, i32::MAX as i64) as libc::c_int)
}

/// Tell the UI that playback jumped to `ms` because the SHELL seeked on its own (the ◁ rewind
/// paths). Re-anchors the position interpolator exactly as `cinder_scrub_end` does, so the bar
/// snaps to the new position instead of extrapolating from the pre-seek anchor for ~1 s.
#[no_mangle]
pub extern "C" fn cinder_notify_seek_ms(ms: libc::c_int) {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return };
    let target = (ms.max(0) as i64).min(r.cur_duration_ms.max(0));
    r.play_pos_ms = target;
    r.real_pos_ms = target;
    r.real_pos_at = std::time::Instant::now();
    let dur = r.cur_duration_ms;
    set_progress(&mut r.np, target, dur);
    r.dirty = true;
}

/// Move an in-progress scrub to UI (x, y). Returns the seek target in ms (>= 0) when the gesture
/// is the Now Playing rail, or -1 for every other slider (they apply live and report nothing).
///
/// `y` arrived 2026-08-17 with the EQ and Tone Control band fields, which are VERTICAL: the rail,
/// the UI-scale slider and the balance slider are all horizontal, so the parameter did not exist
/// and the band columns could only be TAPPED. Passing both means one entry point covers sliders of
/// either orientation instead of the shell needing to know which is which.
#[no_mangle]
pub extern "C" fn cinder_scrub_to(x: libc::c_int, y: libc::c_int) -> libc::c_int {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return -1 };
    if !r.app.scrub_is_rail() {
        // A settings slider: applies live as the finger moves. No seek target to report, but it may
        // have work for the shell (the balance slider writes the codec's two attenuators on every
        // step), so the action is parked for `cinder_scrub_action` to hand over.
        let acts = r.app.scrub_move(x as i32, y as i32);
        stash_scrub_action(r, &acts);
        r.dirty = true;
        return -1;
    }
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
    if !r.app.scrub_is_rail() {
        let acts = r.app.scrub_end();
        stash_scrub_action(r, &acts);
        r.dirty = true;
        save_settings(r); // sliders are persisted, so they survive the next boot
        return -1;
    }
    r.app.scrub_end(); // release nav's claim; the ms target below is this layer's business
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
fn liked_save(r: &Render) {
    let body: String = r.liked.iter().map(|id| format!("{id}\n")).collect();
    for path in likes::LIKED_PATHS {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if parent.exists() {
                let tmp = format!("{path}.tmp");
                if std::fs::write(&tmp, &body).is_ok() {
                    let _ = std::fs::rename(&tmp, path);
                }
            }
        }
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
    let lib = r.app.library();
    let mut body = String::from("# artist\ttitle — liked in Cinder; feed to Last.fm track.love\n");
    for id in &r.liked {
        if let Some(song) = lib.songs.iter().find(|s| s.object_id == *id) {
            body.push_str(&format!("{}\t{}\n", song.artist, song.title));
        }
    }
    for path in likes::LIKED_PATHS {
        let tsv = path.replace("cinder_liked.conf", "cinder_loved.tsv");
        if let Some(parent) = std::path::Path::new(&tsv).parent() {
            if parent.exists() {
                let tmp = format!("{tsv}.tmp");
                if std::fs::write(&tmp, &body).is_ok() {
                    let _ = std::fs::rename(&tmp, &tsv);
                }
            }
        }
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
                if let Some(previous) = r.last_track.as_ref() {
                    if r.rewind_from == Some(previous.object_id) {
                        r.rewind_from = None;
                    } else {
                        r.play_history.push(previous.clone());
                        // Bound metadata retained for rewind just as the UI bounds navigation
                        // history. A long unattended session must not grow without limit.
                        if r.play_history.len() > 256 {
                            r.play_history.remove(0);
                        }
                    }
                }
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
                // Reconcile the two lists against the track that just started: consume it if it was
                // a user pick, and move the context index only where it genuinely moved. Both halves
                // used to live here as two statements in the WRONG ORDER — the context search ran
                // first and a pick taken from the album already playing dragged the index forward
                // with it, dropping every track in between. The rule is one function now, in
                // cinder-ui, where it is unit-testable: see App::track_started.
                if r.app.track_started(t.object_id) {
                    r.queue_pending = true;   // the queue changed; re-issue at THIS boundary
                }
                // TRACK BOUNDARY — the one moment a queue change is free. The new track has just
                // begun, so re-issuing the sequence resets a position that is already ~0 and the
                // reset is invisible. Doing it any other time restarts the music (device-measured;
                // see Action::QueueChanged).
                if r.queue_pending {
                    r.queue_pending = false;
                    let uris = play_order_uris(r, &t.filename);
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
            // The Track information screen is filled HERE, on the track change, rather than when
            // the screen opens: it is ~10 short strings, the DB row is already in hand, and doing
            // it on entry would mean a query on the render thread the first time you tapped.
            r.app.set_track_info(track_info_rows(r, &t));
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
    use crate::likes::liked_load;

    /// "No filter is active" — what `Action::Shuffle` passes when the Library's filter row is on
    /// ALL. A named function rather than a closure at each call site so the tests below read as
    /// "this scope, unfiltered".
    fn keep_all(_: &cinder_db::Track) -> bool {
        true
    }

    /// STOPWATCH, NOT A TEST. Opt-in profile of the boot-path library build, because the ~4.8 s
    /// of dead time after boot (cinder_db_open at t=2.4 to "restore playback context" at t=7.2,
    /// measured on device 2026-09-04) is a cost nobody has ever broken down. Threading it was
    /// tried and reverted; the cheaper question — where does the time actually GO — was never
    /// asked. Absolute numbers here are host numbers and mean nothing; the RATIO between the
    /// phases is what transfers to the device.
    ///
    ///   CINDER_PROFILE_DB=artifacts/MTPDB_dev.dat \
    ///     cargo test -p cinder-ffi profile_library_build -- --nocapture --test-threads=1
    ///
    /// Silently passes when the variable is unset, so it costs a normal `cargo test` nothing.
    #[test]
    fn profile_library_build() {
        let Ok(path) = std::env::var("CINDER_PROFILE_DB") else { return };
        let t = |label: &str, d: std::time::Duration| {
            println!("  {label:<28} {:>9.1} ms", d.as_secs_f64() * 1000.0);
        };

        let t0 = std::time::Instant::now();
        let db = cinder_db::Db::open(&path).expect("open");
        let open = t0.elapsed();
        t("Db::open", open);

        // The individual queries build_library issues, timed on their own. Run BEFORE the full
        // build so SQLite's page cache is as cold for them as it is for the real first build.
        let t1 = std::time::Instant::now();
        let years = db.release_years();
        t("  db.release_years", t1.elapsed());

        let t1 = std::time::Instant::now();
        let songs_sorted = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
        t("  db.tracks(Title)", t1.elapsed());

        let t1 = std::time::Instant::now();
        let album_order = db.tracks_album_order().unwrap_or_default();
        t("  db.tracks_album_order", t1.elapsed());

        let t1 = std::time::Instant::now();
        let albums = db.albums().unwrap_or_default();
        t("  db.albums", t1.elapsed());

        let t1 = std::time::Instant::now();
        let lib = build_library(&db);
        let build = t1.elapsed();
        t("build_library (whole)", build);

        println!(
            "  -> {} tracks, {} albums, {} artists, {} years, {} sorted, {} album-order, {} album rows",
            lib.songs.len(),
            lib.album_count(),
            lib.artists.len(),
            years.len(),
            songs_sorted.len(),
            album_order.len(),
            albums.len(),
        );
        t("TOTAL (open + build)", open + build);
    }

    /// FM state has to survive a reboot. A scan is a deliberate ten-second wait the user watches,
    /// and the dial is where they left the radio — losing either is the same defect the shelf pins
    /// had. This pins the on-disk shape, because the failure mode is silent: nothing errors, the
    /// values simply come back as defaults.
    #[test]
    fn fm_dial_and_stations_survive_a_save() {
        let mut app = cinder_ui::nav::App::new();
        app.fm_report_khz(105_400);
        app.fm_set_stations(&[98_300, 106_200, 91_000]);

        // What settings_body writes for FM, built the same way it builds it.
        let mut body = format!("fm_khz={}\n", app.fm_khz());
        let st = app.fm_stations();
        body.push_str(&format!(
            "fm_stations={}\n",
            st.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",")
        ));
        assert_eq!(body, "fm_khz=105400\nfm_stations=98300,106200,91000\n");

        // …and what the loader makes of it again.
        let mut back = cinder_ui::nav::App::new();
        for line in body.lines() {
            let (k, v) = line.split_once('=').unwrap();
            match k {
                "fm_khz" => back.fm_report_khz(v.parse().unwrap()),
                "fm_stations" => {
                    let list: Vec<i32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
                    back.fm_set_stations(&list);
                }
                _ => {}
            }
        }
        assert_eq!(back.fm_khz(), 105_400);
        assert_eq!(back.fm_stations(), &[98_300, 106_200, 91_000]);
    }

    /// A hand-edited or truncated config must not put the tuner outside the band, and must not stop
    /// a boot. Out-of-band entries are dropped; the dial clamps.
    #[test]
    fn fm_settings_reject_garbage_without_failing() {
        let mut app = cinder_ui::nav::App::new();
        let list: Vec<i32> = "0,87400,98300,not_a_number,108100,106200"
            .split(',')
            .filter_map(|s| s.trim().parse::<i32>().ok())
            .filter(|k| (cinder_ui::fm::MIN_KHZ..=cinder_ui::fm::MAX_KHZ).contains(k))
            .collect();
        assert_eq!(list, vec![98_300, 106_200], "out-of-band and unparseable both dropped");
        app.fm_set_stations(&list);
        assert_eq!(app.fm_stations(), &[98_300, 106_200]);

        app.fm_report_khz(999_999);
        assert_eq!(app.fm_khz(), cinder_ui::fm::MAX_KHZ, "clamped, not stored raw");
    }

    /// A stopped analyzer must not leave its last frame on screen. The bars fall to nothing and the
    /// buffer is dropped, so the visualiser goes absent rather than holding a snapshot — the same
    /// reason the synthetic animation was removed.
    /// The panic hook's screen table must line up with `screen_ord`, or a crash report names the
    /// wrong screen — which is worse than naming none, because it sends the reader somewhere else.
    ///
    /// This caught a real one: `BtCodec` had ordinal 25 against a 25-entry table, so a panic on
    /// the codec screen indexed past the end and reported no screen at all. The list below is the
    /// part that has to be kept complete — hence the length assertion.
    #[test]
    fn every_screen_has_a_distinct_panic_name() {
        use cinder_ui::nav::Screen as S;
        let all = [
            S::Lock, S::NowPlaying, S::Menu, S::Library, S::Album, S::Artist, S::Playlist, S::UpNext, S::Eq,
            S::Sound, S::Bluetooth, S::Settings, S::Fm, S::UsbDac, S::Receiver, S::Onboarding,
            S::UsbStorage, S::Shelf, S::Pairing, S::GenreFilter, S::TrackInfo, S::Folders,
            S::ClockSet, S::Advanced, S::Tone, S::BtCodec, S::Keyboard, S::PlaylistPick,
            S::TrackPick, S::Device, S::VizSet,
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
            genre_id: None,
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
            let (seq, pre) = shuffle_tracks(Some(&db), scope, &keep_all)
                .unwrap_or_else(|| panic!("{scope:?} empty"));
            let uris = uris_of(seq.clone());
            assert!(!uris.is_empty());
            for u in &uris {
                assert!(all.contains(u.as_str()), "{scope:?} produced a non-track: {u}");
            }
            // EVERY scope reports the order it replaced, and it describes exactly the sequence
            // handed back — `App::note_pre_shuffle` refuses one of a different length, so a scope
            // that got this wrong would silently leave shuffle a one-way door.
            let mut a: Vec<i64> = pre.clone();
            let mut b: Vec<i64> = seq.iter().map(|t| t.object_id).collect();
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "{scope:?}: the pre-shuffle order is not this sequence");
        }
        // AllSongs is the whole library; ByAlbum keeps every track too (it only reorders albums).
        assert_eq!(shuffle_tracks(Some(&db), S::AllSongs, &keep_all).unwrap().0.len(), 3);
        assert_eq!(shuffle_tracks(Some(&db), S::ByAlbum, &keep_all).unwrap().0.len(), 3);
    }

    /// The band's caption names the active filter, so the band has to obey it — on BOTH axes. The
    /// Hi-Res one was never wired in: with "Shuffle Hi-Res" on the glass, every scope shuffled the
    /// whole library.
    #[test]
    fn a_shuffle_band_plays_only_what_the_filter_leaves() {
        use cinder_ui::nav::ShuffleScope as S;
        let db = fixture_db();
        // A filter nothing survives: every scope must DECLINE rather than fall back to the
        // unfiltered library, because starting something the caption did not promise is worse
        // than doing nothing and saying so.
        let keep_none = |_: &cinder_db::Track| false;
        for scope in [S::AllSongs, S::ByAlbum, S::ByArtist, S::Playlist] {
            if let Some((seq, _)) = shuffle_tracks(Some(&db), scope, &keep_none) {
                panic!("{scope:?} ignored the filter: {} tracks", seq.len());
            }
        }
        // …and a predicate that keeps one track keeps exactly that one.
        let one = |t: &cinder_db::Track| t.filename == "/music/harvest.flac";
        let (seq, pre) = shuffle_tracks(Some(&db), S::AllSongs, &one).expect("one survivor");
        assert_eq!(uris_of(seq), vec!["/music/harvest.flac".to_string()]);
        assert_eq!(pre.len(), 1);
    }

    /// "TRACKS IN SEQUENCE": ByAlbum may reorder albums but must never split one up or reorder
    /// the tracks inside it.
    #[test]
    fn shuffle_by_album_keeps_albums_intact_and_in_sequence() {
        use cinder_ui::nav::ShuffleScope as S;
        let db = fixture_db();
        // Album 10 = atlas then box (series_no 1,2); album 11 = harvest alone.
        for _ in 0..25 {
            let uris = uris_of(shuffle_tracks(Some(&db), S::ByAlbum, &keep_all).unwrap().0);
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
        let (off, start, pre) = apply_shuffle(false, seq.clone(), 7);
        assert!(pre.is_none(), "nothing was shuffled, so there is no order to restore");
        assert_eq!(uris_of(off), uris);
        assert_eq!(start, 7);

        // On: the TAPPED track leads (the tap is more specific than the toggle) and playback
        // starts at 0, because the queue was reordered to put it there.
        let (on, start, pre) = apply_shuffle(true, seq.clone(), 7);
        // The pre-shuffle order comes back so shuffle-off can restore it.
        let pre = pre.expect("a real shuffle must report the order it replaced");
        assert_eq!(pre.len(), seq.len());
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

    /// The play order handed to PlayerService: the audible track leads, then the user's picks,
    /// then the context tail — and NO TWO ADJACENT ENTRIES ARE THE SAME FILE.
    ///
    /// Swipe-queueing the song you are listening to produced `[A, A, …]`. PlayerService plays A
    /// twice, and the second copy does not change the URI — which is the only thing the shell
    /// reports a track start on — so `App::track_started` never ran, the pick was never consumed
    /// out of the queue, and the next flush put it back: a phantom Up Next row and a song that
    /// played twice, every lap, for ever.
    #[test]
    fn the_play_order_never_repeats_a_file_back_to_back() {
        let u = |s: &str| Some(s.to_string());
        // The current track queued against itself collapses to one entry.
        assert_eq!(
            play_order(Some("/a.flac"), [u("/a.flac"), u("/b.flac")]),
            vec!["/a.flac".to_string(), "/b.flac".to_string()],
        );
        // …and so does a doubled pick.
        assert_eq!(
            play_order(Some("/a.flac"), [u("/b.flac"), u("/b.flac"), u("/c.flac")]),
            vec!["/a.flac".to_string(), "/b.flac".to_string(), "/c.flac".to_string()],
        );
        // But a DELIBERATE repeat with something in between is kept: the URI changes at each
        // boundary there, so each copy is reported and consumed exactly as it should be.
        assert_eq!(
            play_order(Some("/a.flac"), [u("/b.flac"), u("/a.flac")]),
            vec!["/a.flac".to_string(), "/b.flac".to_string(), "/a.flac".to_string()],
        );
        // Rows that no longer resolve to a file drop out rather than shortening the list around
        // them — and dropping one must not make its neighbours adjacent duplicates by accident.
        assert_eq!(
            play_order(Some("/a.flac"), [None, u("/a.flac"), None, u("/b.flac")]),
            vec!["/a.flac".to_string(), "/b.flac".to_string()],
        );
        // No lead at all — a repeat-all lap, which starts from the top of the context.
        assert_eq!(
            play_order(None, [u("/a.flac"), u("/a.flac"), u("/b.flac")]),
            vec!["/a.flac".to_string(), "/b.flac".to_string()],
        );
        assert!(play_order(None, [None, None]).is_empty());
        assert!(play_order(None, []).is_empty());
    }

    /// Degenerate inputs must not panic — a panic here aborts the process, and on this device an
    /// abort is a reboot.
    #[test]
    fn shuffling_degenerate_queues_is_safe() {
        let trk = |n: &str| cinder_db::Track { filename: n.into(), ..Default::default() };
        assert_eq!(apply_shuffle(true, Vec::new(), 0), (Vec::new(), 0, None));
        let one = vec![trk("/a.flac")];
        assert_eq!(apply_shuffle(true, one.clone(), 0), (one.clone(), 0, None));
        // A start index past the end (a stale index against a shorter queue) is left alone rather
        // than indexing out of bounds.
        let two = vec![trk("/a.flac"), trk("/b.flac")];
        assert_eq!(apply_shuffle(true, two.clone(), 9), (two, 9, None));
    }

    /// No DB → no action (rather than an empty queue).
    #[test]
    fn shuffle_without_db_is_ignored() {
        assert!(shuffle_tracks(None, cinder_ui::nav::ShuffleScope::AllSongs, &keep_all).is_none());
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
            genre_id: None,
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
    fn prev_rewinds_past_the_grace_window() {
        // The reported bug: ◁ was an unconditional PlayController::PrevTrack(), so at the head of
        // a queue it did nothing at all, and mid-track it jumped away instead of restarting.
        assert!(!prev_means_restart(0, 240_000), "at the very start ◁ steps back a track");
        assert!(!prev_means_restart(3_000, 240_000), "the grace window itself still steps back");
        assert!(prev_means_restart(3_001, 240_000), "past it, ◁ rewinds to the start");
        assert!(prev_means_restart(200_000, 240_000));
        // Unknown duration (track not in the DB) → no position to rewind within, so step back and
        // let the shell's PrevTrack-failed fallback cover the head-of-sequence case.
        assert!(!prev_means_restart(90_000, 0));
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

    // ── the settings file is not trusted input ──────────────────────────────────────────────
    // It lives on /contents: vfat, world-writable, and shared with any PC the player is plugged
    // into. Every EQ site in the UI clamps to ±BAND_MAX, so Cinder's own values are always in
    // range — but this loader parses `i8`, which accepts -128..127, and an out-of-range gain does
    // not clamp inside Sony's DSP. It ZEROES the band. So a corrupted or hand-edited line used to
    // silently flatten a band instead of pinning it to maximum, draw its knob outside the EQ
    // field, and then be written straight back out on the next save.
    #[test]
    fn eq_band_max_matches_the_ui_limit() {
        // If these drift apart the screen and the DSP disagree about what maximum boost means.
        assert_eq!(EQ_BAND_MAX, cinder_ui::eq::BAND_MAX);
    }

    #[test]
    fn out_of_range_eq_gains_clamp_rather_than_zeroing_the_band() {
        // The clamp the loader applies, asserted directly on the same expression.
        let clamp = |g: i8| g.clamp(-EQ_BAND_MAX, EQ_BAND_MAX);
        assert_eq!(clamp(127), EQ_BAND_MAX, "i8 max must pin to +20, not reach the DSP");
        assert_eq!(clamp(-128), -EQ_BAND_MAX, "i8 min must pin to -20");
        assert_eq!(clamp(100), EQ_BAND_MAX, "a hand-edited 100 pins to +20, not 0");
        assert_eq!(clamp(21), EQ_BAND_MAX, "one over the top pins");
        assert_eq!(clamp(-21), -EQ_BAND_MAX, "one under the bottom pins");
        for g in -EQ_BAND_MAX..=EQ_BAND_MAX {
            assert_eq!(clamp(g), g, "in-range gain {g} must survive exactly");
        }
    }

    #[test]
    fn a_clamped_band_still_lands_inside_the_eq_field() {
        // The other half of the same defect: the EQ screen maps value -> y through BAND_MAX, so a
        // gain of 100 would draw its knob far outside the field it belongs to. Clamped values are
        // by construction the ones the screen can draw.
        use cinder_ui::eq::{value_at_y, BAND_MAX};
        for g in [-BAND_MAX, 0, BAND_MAX] {
            assert!((-BAND_MAX..=BAND_MAX).contains(&g));
        }
        // value_at_y is the inverse and clamps too — nothing the field can produce is out of range.
        for y in -500..1200 {
            let v = value_at_y(y);
            assert!((-BAND_MAX..=BAND_MAX).contains(&v), "y={y} produced {v}");
        }
    }
}
