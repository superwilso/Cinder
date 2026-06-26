//! cinder-ffi — C-ABI surface over the Rust Cinder UI, for the C++ easel shell
//! (`cinder-home`). One glibc process: the C++ shell does the appmgr/easel lifecycle
//! and the Sony IPC, then calls these `extern "C"` entry points to paint the panel.
//!
//! Frame model: the C++ pump calls `cinder_render_tick()` once per frame; the shell
//! pushes state via the setters. All state lives behind a Mutex; panics abort (the
//! workspace profile sets panic="abort"), so nothing unwinds across the FFI boundary.

mod scrobble;

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
    fn blit(&self, canvas: &Canvas) {
        let base = self.base as *mut u8;
        let copy_bytes = (W * 4).min(self.stride);
        for page in 0..self.pages {
            for y in 0..H {
                let dst_row = (page * H + y) * self.stride;
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
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
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
fn apply_track(np: &mut Np, t: &cinder_db::Track, progress: f32) {
    np.title = if t.title.is_empty() {
        t.filename.rsplit('/').next().unwrap_or("").to_string()
    } else {
        t.title.clone()
    };
    np.artist = t.artist.clone();
    let (codec, badge) = codec_label(&t.filename, t.is_hires);
    np.codec = codec;
    np.badge = badge;
    let dur = t.duration_raw.unwrap_or(0).max(0);
    let p = progress.clamp(0.0, 1.0);
    let elapsed_ms = (dur as f32 * p) as i64;
    np.elapsed = fmt_time(elapsed_ms);
    np.remaining = format!("-{}", fmt_time(dur - elapsed_ms));
    np.progress = p;
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

/// Render the current state to the panel (call once per frame from the pump).
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let mut guard = cell().lock().unwrap();
    let Some(r) = guard.as_mut() else { return };
    r.app.tick(); // advance overlay/HUD countdowns
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
    };
    // The navigator decides which screen is showing; it draws Now Playing from `np` and
    // the list/menu screens from their own state.
    r.app.render(&mut canvas, &r.fonts, &np);
    r.fb.blit(&canvas);
}

/// Deliver a logical button press to the navigator. `button` is a `cinder_button_t`
/// (see cinder.h). Theme changes are applied internally; the return value is a
/// `cinder_action_t` the shell should carry out via cinder-audio (0 = nothing).
#[no_mangle]
pub extern "C" fn cinder_input(button: libc::c_int) -> libc::c_int {
    use cinder_ui::nav::{Action, Button};
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
    // Keep the renderer's theme in sync with navigator-driven theme changes.
    r.night = r.app.night;
    // Report the first actionable result to the shell (audio/USB are the shell's job).
    for a in &actions {
        let code = match a {
            Action::PlayPause => 1,
            Action::Next => 2,
            Action::Prev => 3,
            Action::NextAlbum => 4,
            Action::PrevAlbum => 5,
            Action::VolUp => 6,
            Action::VolDown => 7,
            Action::PlayIndex(_) => 8,
            Action::ThemeChanged(_) => continue, // applied internally above
            // EQ band gains + BT on/off are reflected in the UI now; driving the device DSP
            // (SoundService) / BtTransmitterService is a device-RE follow-up, so swallow them
            // here rather than surface an action the shell can't yet carry out.
            Action::EqChanged(_) | Action::BtToggle(_) => continue,
            Action::Sleep => 10,
            Action::EnterUsbMsc => 11,
        };
        return code;
    }
    0
}

#[no_mangle]
pub extern "C" fn cinder_render_shutdown() {
    *cell().lock().unwrap() = None; // Framebuffer::drop unmaps
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
    }
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
    let track = r.db.as_ref().and_then(|db| db.track_by_filename(&u).ok().flatten());
    match track {
        Some(t) => {
            apply_track(&mut r.np, &t, progress);
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
            r.np.progress = progress.clamp(0.0, 1.0);
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
        apply_track(&mut np, &t, 0.5);
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
        apply_track(&mut np, &t, 0.0);
        assert_eq!(np.title, "box.flac");
        assert_eq!(np.elapsed, "0:00");
        assert_eq!(np.remaining, "-0:00");
    }
}
