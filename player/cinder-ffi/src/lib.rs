//! cinder-ffi — C-ABI surface over the Rust Cinder UI, for the C++ easel shell
//! (`cinder-home`). One glibc process: the C++ shell does the appmgr/easel lifecycle
//! and the Sony IPC, then calls these `extern "C"` entry points to paint the panel.
//!
//! Frame model: the C++ pump calls `cinder_render_tick()` once per frame; the shell
//! pushes state via the setters. All state lives behind a Mutex; panics abort (the
//! workspace profile sets panic="abort"), so nothing unwinds across the FFI boundary.

use cinder_ui::now_playing::{self, NowPlaying};
use cinder_ui::{Canvas, FontSet, Theme, H, W};
use std::ffi::c_char;
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::sync::{Mutex, OnceLock};

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
    *cell().lock().unwrap() = Some(Render { fb, fonts: FontSet::load(), night: false, np });
    0
}

/// Render the current state to the panel (call once per frame from the pump).
#[no_mangle]
pub extern "C" fn cinder_render_tick() {
    let guard = cell().lock().unwrap();
    let Some(r) = guard.as_ref() else { return };
    let mut canvas = Canvas::new();
    let theme = if r.night { Theme::night() } else { Theme::day() };
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
    now_playing::render(&mut canvas, &theme, &r.fonts, &np);
    r.fb.blit(&canvas);
}

#[no_mangle]
pub extern "C" fn cinder_render_shutdown() {
    *cell().lock().unwrap() = None; // Framebuffer::drop unmaps
}

#[no_mangle]
pub extern "C" fn cinder_set_theme_night(night: libc::c_int) {
    if let Some(r) = cell().lock().unwrap().as_mut() {
        r.night = night != 0;
    }
}

/// Push the currently-playing track. Strings are copied; NULL = empty.
/// `progress` is 0..1; `playing`/`battery` as shown in the status/transport.
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
