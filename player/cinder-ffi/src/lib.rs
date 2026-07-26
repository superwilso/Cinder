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

mod art_load;
mod gpu;
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

const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
const FBIOPUT_VSCREENINFO: libc::Ioctl = 0x4601;
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;
/// fb_var_screeninfo.activate flag: force the driver to (re)apply the mode NOW. On mtkfb this is
/// what actually pushes the framebuffer to the panel — writing pixels into the mmap does NOTHING
/// on its own. icx_bootanimation's per-frame "flip" (disasm @0x1fae) is exactly
/// `var.activate |= 0x80; ioctl(fd, FBIOPUT_VSCREENINFO, &var)`; without it the glass keeps showing
/// whatever was pushed last, forever (the "frozen boot image" failure mode).
const FB_ACTIVATE_FORCE: u32 = 0x80;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct VarInfo {
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
    fn blit(&mut self, canvas: &Canvas) {
        let base = self.base as *mut u8;
        let copy_bytes = (W * 4).min(self.stride);
        for page in 0..self.pages {
            for y in 0..H {
                let dst_row = (page * H + y) * self.stride;
                if dst_row + copy_bytes > self.map_len {
                    break; // this row (and any after, in this page) would overrun the mapping
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        canvas.buf.as_ptr().add(y * W) as *const u8,
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

impl Presenter {
    fn present(&mut self, canvas: &Canvas) {
        match self {
            Presenter::Gl(g) => g.present(canvas),
            Presenter::Fb(f) => f.blit(canvas),
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
    present: Presenter,
    fonts: FontSet,
    night: bool,
    np: Np,
    db: Option<cinder_db::Db>,
    app: cinder_ui::nav::App,
    scrob: Option<scrobble::Scrobbler>,
    last_track: Option<cinder_db::Track>, // last resolved track (for scrobble metadata)
    // Now-playing position ESTIMATE: the track duration is known from the DB, so we advance a local
    // play-clock 1 s per tick while playing (reset on track change) to drive a live progress bar +
    // elapsed/remaining. This is a play-through estimate (it can't see seeks or a mid-track start);
    // on device it will be replaced by the real PlayStatus position once those offsets are RE'd.
    play_pos_ms: i64,
    cur_duration_ms: i64,
    last_pos: std::time::Instant, // wall-clock anchor for the position estimate (rate-independent)
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
    // Pending play request (Action::PlayIndex resolved through the DB): the chosen track's album
    // context — file URIs in play order + the start index. The shell drains it via
    // cinder_pending_play_* after a CINDER_ACT_PLAY_INDEX action and hands it to PlayerService
    // (NodeTrackSequence). Replaced wholesale on every new PlayIndex.
    pending_play: Vec<String>,
    pending_play_start: usize,
    // Decoded album cover for the CURRENT track, pre-scaled to the two draw sizes (480 full-bleed,
    // 92 thumb). art_key = the object_id we last decoded for (skip re-decode on same-track polls);
    // None images = no art found → the UI draws its gradient fallback.
    art_full: Option<cinder_ui::art::Image>,
    art_thumb: Option<cinder_ui::art::Image>,
    art_key: Option<i64>,
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

/// Open the framebuffer and initialise the renderer. Returns 0 on success, <0 on error.
#[no_mangle]
pub extern "C" fn cinder_render_init() -> libc::c_int {
    // GPU present path (EGL/GLES2 on Mali) is now the DEFAULT (2026-07-26). It was opt-in while the
    // failure mode was a driver-level HANG: cinder runs as uid 100 (CapEff=0) and the Mali EGL stack
    // needs four nodes that ship root-only (/dev/ion, /dev/mtkfb_vsync, /dev/mtk_disp, /dev/sw_sync).
    // Opening EGL without them didn't return an error — it blocked inside the driver, wedging a boot
    // that can't fall through to software, which is how the bad-boot counter got tripped.
    //   That class is now GONE: gpu::GlPresenter::open() preflights every node with access(R|W) and
    //   refuses to enter EGL unless they're all open, running the setuid-root cinder-gpunode helper
    //   once to widen them. A blocked node is therefore a clean Err → software framebuffer, proven
    //   on-device in BOTH directions as uid 100 via `cinder-probe --gpu`.
    //   Escapes kept for debugging: CINDER_GPU=0 or /contents/cinder_gpu_off force software.
    let force_off = std::path::Path::new("/contents/cinder_gpu_off").exists()
        || std::env::var("CINDER_GPU").map(|v| v == "0").unwrap_or(false);
    let want_gpu = !force_off;
    let present = if want_gpu {
        match gpu::GlPresenter::open(W as i32, H as i32) {
            Ok(g) => {
                println!("cinder-ffi: GPU present path active (EGL/GLES2 on Mali, vsync)");
                Presenter::Gl(g)
            }
            Err(e) => {
                eprintln!("cinder-ffi: GPU init failed ({e}); falling back to software framebuffer");
                match Framebuffer::open() {
                    Ok(fb) => Presenter::Fb(fb),
                    Err(e2) => {
                        eprintln!("cinder-ffi: {e2}");
                        return -1;
                    }
                }
            }
        }
    } else {
        match Framebuffer::open() {
            Ok(fb) => Presenter::Fb(fb),
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
        present,
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
        pending_play: Vec::new(),
        pending_play_start: 0,
        art_full: None,
        art_thumb: None,
        art_key: None,
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
        "night={}\nviz_kind={}\nviz_on={}\neq={}\nsound={}\nonboarding={}\nbt_codec={}\nbt_ldac_quality={}\nvolume={}\n",
        r.app.night as u8,
        r.app.viz_kind(),
        r.app.viz_on() as u8,
        eq.join(","),
        r.app.sound_flags(),
        r.app.onboarding_seen() as u8,
        r.app.bt_codec(),
        r.app.bt_ldac_quality(),
        r.app.volume_level(),
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
fn build_library(db: &cinder_db::Db) -> cinder_ui::Library {
    use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, SongRow};
    use std::collections::{BTreeMap, BTreeSet};

    // Resolve release-year FKs once (best-effort; empty map if the table shape differs — years
    // then stay blank, exactly as before). Shared by the song-row builder + the album year label.
    let years = db.release_years();
    let year_num = |id: Option<i64>| -> i32 {
        id.and_then(|i| years.get(&i)).and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0)
    };

    let song_row = |t: &cinder_db::Track| {
        let title = if t.title.is_empty() {
            t.filename.rsplit('/').next().unwrap_or("").to_string()
        } else {
            t.title.clone()
        };
        let art = if t.album.is_empty() { title.clone() } else { t.album.clone() };
        SongRow {
            title,
            artist: t.artist.clone(),
            dur: t.duration_raw.map(fmt_time).unwrap_or_default(),
            art,
            object_id: t.object_id,
            album_id: t.album_id.unwrap_or(0),
            disc: t.disc_no as i32,
            track: t.track_no as i32,
            added: t.added,
            year: year_num(t.releaseyear_id),
        }
    };

    let tracks = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
    let mut album_artist: BTreeMap<i64, String> = BTreeMap::new();
    let mut artist_albums: BTreeMap<String, (BTreeSet<String>, u32)> = BTreeMap::new();
    let mut songs = Vec::with_capacity(tracks.len());
    for t in &tracks {
        if let Some(aid) = t.album_id {
            album_artist.entry(aid).or_insert_with(|| t.artist.clone());
        }
        songs.push(song_row(t));
        let e = artist_albums.entry(t.artist.clone()).or_default();
        if !t.album.is_empty() {
            e.0.insert(t.album.clone());
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
            let arts: Vec<String> = if albs.is_empty() {
                vec![name.clone()]
            } else {
                albs.iter().take(2).cloned().collect()
            };
            ArtistRow { albums: albs.len() as u32, tracks: tr, arts, name }
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
            // No cover of its own: hash the name so each playlist still gets distinct art,
            // the same fallback the album rows use.
            art: p.name,
        })
        .collect();

    cinder_ui::Library { songs, album_groups, artists, playlists }
}

/// Render the current state to the panel (call once per frame from the pump). No-op when
/// nothing has changed (dirty-flag rendering) — that keeps the device idle at near-zero CPU
/// instead of re-blitting ~4.6 MB every tick (battery, goal #1).
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return };
    // An active overlay (volume HUD) animates, so it keeps us dirty until it fades.
    if r.app.tick() {
        r.dirty = true;
    }
    // Visualiser: advance + force a repaint ONLY while playing on the Now Playing screen (and
    // enabled), and at most ~20 fps (the pump may tick at 60) — that bounds the battery cost.
    let animate = r.app.viz_on() && r.np.playing && r.app.is_now_playing();
    if animate && r.last_viz.elapsed() >= std::time::Duration::from_millis(50) {
        r.viz_phase += 0.18;
        r.last_viz = std::time::Instant::now();
        r.dirty = true;
    }
    if !r.dirty {
        return; // nothing changed — skip the render + framebuffer blit entirely
    }
    let mut canvas = Canvas::new();
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
        viz_on: r.app.viz_on(), // nav re-injects this too; kept honest here
        // real FFT spectrum if the shell is feeding PCM AND we're animating; else None (synthetic)
        viz_levels: if animate && !r.viz_levels.is_empty() { Some(&r.viz_levels) } else { None },
    };
    // The navigator decides which screen is showing; it draws Now Playing from `np` and
    // the list/menu screens from their own state.
    r.app.render(&mut canvas, &r.fonts, &np);
    if let Some(path) = r.pending_screenshot.take() {
        match write_png(&path, &canvas) {
            Ok(()) => println!("cinder-ffi: screenshot written to {path}"),
            Err(e) => eprintln!("cinder-ffi: screenshot failed ({path}): {e}"),
        }
    }
    r.present.present(&canvas);
    r.dirty = false;
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

/// Resolve a Library shuffle band into the URIs to play, in the order to play them. `None` when
/// there is no DB or the scope is empty — the caller then emits no action rather than handing the
/// shell an empty sequence.
///
/// Each arm matches the sub-label the band draws, so what the button promises is what it does.
fn shuffle_uris(db: Option<&cinder_db::Db>, scope: cinder_ui::nav::ShuffleScope) -> Option<Vec<String>> {
    use cinder_ui::nav::ShuffleScope as S;
    let db = db?;
    let mut rng = Rng::new();

    let uris: Vec<String> = match scope {
        // "N TRACKS · RANDOM ORDER"
        S::AllSongs => {
            let mut v: Vec<String> = db
                .tracks(cinder_db::Sort::Title)
                .ok()?
                .into_iter()
                .map(|t| t.filename)
                .collect();
            rng.shuffle(&mut v);
            v
        }
        // "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE" — shuffle the albums, keep each album's
        // tracks in their disc/track order.
        S::ByAlbum => {
            let tracks = db.tracks_album_order().ok()?;
            let mut albums: Vec<Vec<String>> = Vec::new();
            let mut cur_id: Option<i64> = None;
            for t in tracks {
                if Some(t.album_id.unwrap_or(0)) != cur_id {
                    cur_id = Some(t.album_id.unwrap_or(0));
                    albums.push(Vec::new());
                }
                albums.last_mut().expect("pushed above").push(t.filename);
            }
            rng.shuffle(&mut albums);
            albums.into_iter().flatten().collect()
        }
        // "RANDOM ARTIST · SHUFFLED WITHIN ARTIST" — one artist, their tracks shuffled.
        S::ByArtist => {
            let tracks = db.tracks(cinder_db::Sort::Artist).ok()?;
            let mut by_artist: std::collections::BTreeMap<String, Vec<String>> = Default::default();
            for t in tracks {
                if !t.artist.is_empty() {
                    by_artist.entry(t.artist.clone()).or_default().push(t.filename);
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
            let mut v = playlist_uris(Some(db), pick.id)?;
            rng.shuffle(&mut v);
            v
        }
    };
    (!uris.is_empty()).then_some(uris)
}

/// Member file URIs of a playlist, in the user's saved order. `None` when there's no DB, the id
/// isn't a live playlist, or nothing in it still resolves to a playable track — the caller then
/// emits no action rather than handing the shell an empty sequence.
fn playlist_uris(db: Option<&cinder_db::Db>, playlist_id: i64) -> Option<Vec<String>> {
    let tracks = db?.playlist_tracks(playlist_id).ok()?;
    let uris: Vec<String> = tracks.into_iter().map(|t| t.filename).collect();
    (!uris.is_empty()).then_some(uris)
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
                    r.pending_play = tracks.into_iter().map(|t| t.filename).collect();
                    r.pending_play_start = idx;
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
            match playlist_uris(r.db.as_ref(), *playlist_id) {
                Some(uris) => {
                    r.pending_play = uris;
                    r.pending_play_start = 0;
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
            match shuffle_uris(r.db.as_ref(), *scope) {
                Some(uris) => {
                    r.pending_play = uris;
                    r.pending_play_start = 0;
                    8
                }
                None => {
                    eprintln!("cinder-ffi: Shuffle({scope:?}): nothing to play — ignored");
                    return None;
                }
            }
        }
        Action::ThemeChanged(_) => 16, // shell also drives the backlight (night = minimal light)
        Action::Sleep => 10,
        Action::EnterUsbMsc => 11,
        Action::ExitUsbMsc => 19,
        Action::EqChanged(_) => 12,
        Action::BtToggle(_) => return None, // UI-only (RE follow-up)
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
            r.np.repeat = (r.np.repeat + 1) % 3; // off → all → one
            return None;
        }
        Action::BtCodecChanged => 17, // shell reads cinder_get_bt_codec/quality + applies via BtTransmitter
        Action::UsbDacToggle(_) => 18, // shell reads cinder_get_usb_dac() + starts/stops the LDAC bridge
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

/// Copy pending-play URI `i` (0-based) into `buf` (NUL-terminated). Returns the byte length
/// copied, or -1 for a bad index/args.
#[no_mangle]
pub extern "C" fn cinder_pending_play_uri(i: libc::c_int, buf: *mut c_char, cap: libc::c_int) -> libc::c_int {
    if buf.is_null() || cap <= 0 {
        return -1;
    }
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return -1 };
    let Some(uri) = r.pending_play.get(i as usize) else { return -1 };
    let n = uri.len().min(cap as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(uri.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    n as libc::c_int
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
        // Only force a repaint when the visualiser is actually on screen — the audio source may
        // stream continuously, but off Now Playing the new levels are unused, so don't burn a frame.
        if r.app.viz_on() && r.app.is_now_playing() {
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
        // Only force a repaint when the visualiser is on screen (the analyzer streams continuously).
        if r.app.viz_on() && r.app.is_now_playing() {
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
        if r.np.playing && r.cur_duration_ms > 0 && r.play_pos_ms < r.cur_duration_ms {
            r.play_pos_ms = (r.play_pos_ms + dt).min(r.cur_duration_ms);
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
        if let Some(s) = r.scrob.as_mut() {
            s.tick(playing != 0);
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
                    "viz_kind" => {
                        if let Ok(n) = v.parse::<u8>() {
                            r.app.set_viz_kind(n);
                        }
                    }
                    "viz_on" => r.app.set_viz_on(v == "1"),
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
                    let native = r.db.as_ref().and_then(|db| art_load::load(db, t.object_id));
                    r.art_full = native.as_ref().map(|img| img.scaled_to(480, 480));
                    r.art_thumb = native.as_ref().map(|img| img.scaled_to(92, 92));
                    r.art_key = Some(t.object_id);
                }
                r.cur_duration_ms = t.duration_raw.unwrap_or(0).max(0);
                r.play_pos_ms = (r.cur_duration_ms as f32 * progress.clamp(0.0, 1.0)) as i64;
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
            r.art_full = None;
            r.art_thumb = None;
            r.art_key = None;
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
                album_id INTEGER, artist_id INTEGER, releaseyear_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke');
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO releaseyears VALUES (30,0,'2012','2012','2012');
            INSERT INTO releaseyears VALUES (31,0,'1992','1992','1992');
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (1,1,1,'Atlas Hands','/music/atlas.flac',1,1,1,10,20,30,5000);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (2,1,1,'Box of Stones','/music/box.flac',2,1,1,10,20,30,5001);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (3,1,1,'Harvest Moon','/music/harvest.flac',1,1,0,11,21,31,4000);
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

    /// The URIs handed to the shell are in saved (child_index) order, NOT title order — this is
    /// what makes a playlist play as the user arranged it.
    #[test]
    fn play_playlist_resolves_uris_in_saved_order() {
        let db = fixture_db();
        assert_eq!(
            playlist_uris(Some(&db), 60),
            Some(vec!["/music/harvest.flac".to_string(), "/music/atlas.flac".to_string()])
        );
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
            let uris = shuffle_uris(Some(&db), scope).unwrap_or_else(|| panic!("{scope:?} empty"));
            assert!(!uris.is_empty());
            for u in &uris {
                assert!(all.contains(u.as_str()), "{scope:?} produced a non-track: {u}");
            }
        }
        // AllSongs is the whole library; ByAlbum keeps every track too (it only reorders albums).
        assert_eq!(shuffle_uris(Some(&db), S::AllSongs).unwrap().len(), 3);
        assert_eq!(shuffle_uris(Some(&db), S::ByAlbum).unwrap().len(), 3);
    }

    /// "TRACKS IN SEQUENCE": ByAlbum may reorder albums but must never split one up or reorder
    /// the tracks inside it.
    #[test]
    fn shuffle_by_album_keeps_albums_intact_and_in_sequence() {
        use cinder_ui::nav::ShuffleScope as S;
        let db = fixture_db();
        // Album 10 = atlas then box (series_no 1,2); album 11 = harvest alone.
        for _ in 0..25 {
            let uris = shuffle_uris(Some(&db), S::ByAlbum).unwrap();
            let atlas = uris.iter().position(|u| u == "/music/atlas.flac").unwrap();
            let boxs = uris.iter().position(|u| u == "/music/box.flac").unwrap();
            assert_eq!(boxs, atlas + 1, "album 10 was split or reordered: {uris:?}");
        }
    }

    /// No DB → no action (rather than an empty queue).
    #[test]
    fn shuffle_without_db_is_ignored() {
        assert_eq!(shuffle_uris(None, cinder_ui::nav::ShuffleScope::AllSongs), None);
    }

    /// Unknown id, or no DB at all → no action, rather than handing the shell an empty sequence.
    #[test]
    fn play_playlist_unknown_is_ignored() {
        let db = fixture_db();
        assert_eq!(playlist_uris(Some(&db), 999), None);
        assert_eq!(playlist_uris(None, 60), None);
    }

    #[test]
    fn empty_title_falls_back_to_filename() {
        let t = cinder_db::Track {
            object_id: 2,
            title: String::new(),
            artist: String::new(),
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
}
