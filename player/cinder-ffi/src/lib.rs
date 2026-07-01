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
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;

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
        Ok(Framebuffer { _file: file, base: ptr as usize, stride, pages, map_len })
    }

    /// Blit one canvas to every page (the panel is triple-buffered).
    ///
    /// Bullet-proofing: we NEVER write past the mapped region. On the confirmed panel
    /// (480x800, virtual 2400 = 3x800) every row fits exactly, but if a unit/firmware ever reports
    /// a geometry where `pages*H` overruns `yres_virtual` (e.g. yres_virtual not a multiple of H, a
    /// rotated panel, or H > yres), an unchecked `(page*H+y)*stride` would write off the end of the
    /// mmap → SIGSEGV/corruption. So each row is bounded against `map_len`; an out-of-range row is
    /// skipped rather than written. Worst case is a cosmetically clipped frame, never a crash.
    fn blit(&self, canvas: &Canvas) {
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
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.base as *mut libc::c_void, self.map_len);
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
    fb: Framebuffer,
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
    let fb = match Framebuffer::open() {
        Ok(fb) => fb,
        Err(e) => {
            eprintln!("cinder-ffi: {e}");
            return -1;
        }
    };
    let mut np = Np::default();
    np.codec = "—".into();
    np.battery = 100;
    *cell().lock().unwrap() = Some(Render {
        fb,
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
        sleep_remaining_ms: 0,
        sleep_fire: false,
        settings_path: None,
        last_saved_body: String::new(),
        dirty: true, // paint the first frame
        viz_phase: 2.0,
        last_viz: std::time::Instant::now(),
        viz_levels: Vec::new(),
        viz_peak: 0.0,
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
        "night={}\nviz_kind={}\nviz_on={}\neq={}\nsound={}\nonboarding={}\nbt_codec={}\nbt_ldac_quality={}\n",
        r.app.night as u8,
        r.app.viz_kind(),
        r.app.viz_on() as u8,
        eq.join(","),
        r.app.sound_flags(),
        r.app.onboarding_seen() as u8,
        r.app.bt_codec(),
        r.app.bt_ldac_quality(),
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

    let tracks = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
    let mut album_artist: BTreeMap<String, String> = BTreeMap::new();
    let mut artist_albums: BTreeMap<String, (BTreeSet<String>, u32)> = BTreeMap::new();
    let mut album_tracks: BTreeMap<String, Vec<SongRow>> = BTreeMap::new();
    let mut songs = Vec::with_capacity(tracks.len());
    for t in &tracks {
        let title = if t.title.is_empty() {
            t.filename.rsplit('/').next().unwrap_or("").to_string()
        } else {
            t.title.clone()
        };
        let art = if t.album.is_empty() { title.clone() } else { t.album.clone() };
        let row = SongRow {
            title,
            artist: t.artist.clone(),
            dur: t.duration_raw.map(fmt_time).unwrap_or_default(),
            art,
            object_id: t.object_id,
        };
        if !t.album.is_empty() {
            album_artist.entry(t.album.clone()).or_insert_with(|| t.artist.clone());
            album_tracks.entry(t.album.clone()).or_default().push(row.clone());
        }
        songs.push(row);
        let e = artist_albums.entry(t.artist.clone()).or_default();
        if !t.album.is_empty() {
            e.0.insert(t.album.clone());
        }
        e.1 += 1;
    }

    // Album list (ordered, with track counts) → rows, grouped by artist.
    let mut album_rows: Vec<AlbumRow> = db
        .albums()
        .unwrap_or_default()
        .into_iter()
        .map(|a| AlbumRow {
            artist: album_artist.get(&a.name).cloned().unwrap_or_default(),
            year: String::new(),
            tracks: a.track_count.max(0) as u32,
            art: a.name.clone(),
            track_list: album_tracks.remove(&a.name).unwrap_or_default(),
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

    cinder_ui::Library { songs, album_groups, artists, playlists: Vec::new() }
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
        liked: r.np.liked,
        playing: r.np.playing,
        shuffle: r.np.shuffle,
        repeat: r.np.repeat,
        viz_seed: if animate { r.viz_phase } else { 2.0 },
        viz_kind: 0, // nav injects the real viz type on the NowPlaying render
        // real FFT spectrum if the shell is feeding PCM AND we're animating; else None (synthetic)
        viz_levels: if animate && !r.viz_levels.is_empty() { Some(&r.viz_levels) } else { None },
    };
    // The navigator decides which screen is showing; it draws Now Playing from `np` and
    // the list/menu screens from their own state.
    r.app.render(&mut canvas, &r.fonts, &np);
    r.fb.blit(&canvas);
    r.dirty = false;
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
        Action::PlayIndex(_) => 8,
        Action::ThemeChanged(_) => 16, // shell also drives the backlight (night = minimal light)
        Action::Sleep => 10,
        Action::EnterUsbMsc => 11,
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

/// Drag-to-scroll the current list by `dy_rows` rows (the shell converts the touch drag distance).
#[no_mangle]
pub extern "C" fn cinder_touch_scroll(dy_rows: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.app.touch_scroll(dy_rows as i32);
        r.dirty = true;
    }
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

/// Read the current UI volume as a 0..100 percentage. The shell scales it to the device mixer range
/// (configured from the discovery report) to set the hardware volume after a VOLUP/VOLDOWN action.
#[no_mangle]
pub extern "C" fn cinder_get_volume() -> libc::c_int {
    match cell().lock().unwrap().as_ref() {
        Some(r) => r.app.volume_pct() as libc::c_int,
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

/// Load persisted UI preferences (theme + visualiser + EQ + sound effects) from `path`, apply them,
/// and remember the path so later changes auto-save. Call once at boot after cinder_render_init.
/// Returns 1 if a settings file was actually read (so the shell can re-apply EQ/sound to the DSP),
/// else 0. Best-effort: a missing/garbage file is ignored (defaults stand); robust line parser.
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
            // Unknown track → unknown duration: no estimated bar.
            r.cur_duration_ms = 0;
            r.play_pos_ms = 0;
            set_progress(&mut r.np, 0, 0);
            r.last_track = None;
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
            othumb_id: Some(100),
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
            CREATE TABLE object_body (
                object_id INTEGER PRIMARY KEY AUTOINCREMENT, object_type INTEGER NOT NULL,
                child_index INTEGER, media_type INTEGER DEFAULT 0, format INTEGER DEFAULT 0,
                initial INTEGER, sort_str TEXT, search_str TEXT, title TEXT DEFAULT "",
                addedtime INTEGER DEFAULT 0, filename TEXT, filesize INTEGER,
                series_no INTEGER, disc_no INTEGER, is_high_resolution INTEGER,
                album_id INTEGER, artist_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke');
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO object_body (object_id,object_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,addedtime)
              VALUES (1,1,'Atlas Hands','/music/atlas.flac',1,1,1,10,20,5000);
            INSERT INTO object_body (object_id,object_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,addedtime)
              VALUES (2,1,'Box of Stones','/music/box.flac',2,1,1,10,20,5001);
            INSERT INTO object_body (object_id,object_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,addedtime)
              VALUES (3,1,'Harvest Moon','/music/harvest.flac',1,1,0,11,21,4000);
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
