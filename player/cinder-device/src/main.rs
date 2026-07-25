//! Device backend: render a Cinder screen onto /dev/graphics/fb0 (mtkfb,
//! 480x800, 32bpp XRGB8888, triple-buffered). Writes the Canvas to ALL
//! framebuffer pages so it's visible regardless of the active scanout page.
//! (fb-open/ioctl logic carried over from artifacts/build/cinder_fb_spike.)

use cinder_ui::now_playing::{self, NowPlaying};
use cinder_ui::{Canvas, FontSet, Theme, H, W};
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;

const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
const FBIOPUT_VSCREENINFO: libc::Ioctl = 0x4601;
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;
// mtkfb only pushes the framebuffer to the panel on FBIOPUT_VSCREENINFO with this activate flag
// set (icx_bootanimation's per-frame flip); writing the mmap alone never reaches the glass.
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

/// True when a USB *data host* (a PC) is connected — not merely a dumb charger.
/// A PC enumerates the gadget, so `android_usb/state` reaches CONFIGURED; a charger
/// stays at CONNECTED. Falls back to the power-supply flags if that node is absent
/// (those can't tell a charger from a PC — recalibrate on device if the primary is gone).
fn usb_host_present() -> bool {
    if let Ok(s) = std::fs::read_to_string("/sys/class/android_usb/android0/state") {
        return s.trim() == "CONFIGURED";
    }
    for p in [
        "/sys/class/power_supply/usb/online",
        "/sys/class/power_supply/usb/present",
    ] {
        if let Ok(s) = std::fs::read_to_string(p) {
            return s.trim() == "1";
        }
    }
    false
}

/// Replace fds 1 and 2 with `path` opened using `flags`. Used to MOVE our log off
/// /contents before a mass-storage handoff: an open write fd on /contents would make
/// stock's `umount /contents` fail (EBUSY) and silently break mass storage; and once
/// /contents is unmounted, writes here would hit a stale mountpoint.
fn redirect_fds(path: &str, flags: libc::c_int) {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if let Ok(c) = std::ffi::CString::new(path) {
        unsafe {
            let fd = libc::open(c.as_ptr(), flags, 0o644);
            if fd >= 0 {
                libc::dup2(fd, 1);
                libc::dup2(fd, 2);
                if fd > 2 {
                    libc::close(fd);
                }
            }
        }
    }
}

fn main() {
    // Render the screen into our software canvas.
    let fonts = FontSet::load();
    let mut canvas = Canvas::new();
    let np = NowPlaying {
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        codec: "FLAC · 24bit / 96.0 kHz",
        badge: "FLAC 24/96",
        clock: "14:32",
        battery: 78,
        elapsed: "1:47",
        remaining: "-2:45",
        progress: 0.39,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: true,
        playing: true,
        shuffle: false,
        repeat: 1,
        viz_seed: 2.0,
        viz_kind: 0,
        viz_on: true,
        viz_levels: None,
    };
    now_playing::render(&mut canvas, &Theme::day(), &fonts, &np);

    // Open the framebuffer and read its geometry.
    let fb = match OpenOptions::new().read(true).write(true).open("/dev/graphics/fb0") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("open /dev/graphics/fb0: {e}");
            std::process::exit(1);
        }
    };
    let fd = fb.as_raw_fd();
    let mut var = VarInfo::default();
    let mut fix = FixInfo::default();
    unsafe {
        libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var as *mut _);
        libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix as *mut _);
    }
    let stride = fix.line_length as usize; // bytes per row
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
        eprintln!("mmap failed");
        std::process::exit(1);
    }
    let base = ptr as *mut u8;

    // Continuously blit the canvas to every page of the (triple-buffered)
    // framebuffer so Cinder stays on-screen in normal boot even as the stock
    // compositor redraws its own pages. Exit cleanly if the escape flag
    // /contents/cinder_off appears, so the stock UI can always be recovered
    // without a reflash. (Static screen for now — no input wired yet.)
    let pages = (var.yres_virtual / var.yres).max(1) as usize;
    let copy_bytes = (W * 4).min(stride); // one row
    println!(
        "cinder-device: {}x{} {}bpp, {} pages, stride {} — entering render loop",
        var.xres, var.yres, var.bits_per_pixel, pages, stride
    );
    // Diagnostic: record the idle USB-detect signal so the handoff trigger can be
    // calibrated from a log read (we expect android0/state to exist and read e.g.
    // "DISCONNECTED" when idle, "CONFIGURED" when a PC is attached).
    println!(
        "cinder-device: usb-detect: android0/state={:?} ps/online={:?} (host_present={})",
        std::fs::read_to_string("/sys/class/android_usb/android0/state").map(|s| s.trim().to_string()),
        std::fs::read_to_string("/sys/class/power_supply/usb/online").map(|s| s.trim().to_string()),
        usb_host_present()
    );
    let escape = std::path::Path::new("/contents/cinder_off");
    // Flag (on devtmpfs, which is never unmounted) that tells the launch wrapper to
    // THAW the frozen stock app for a USB mass-storage handoff. See the handoff note below.
    let usb_flag = "/dev/cinder_usb";
    let log_path = "/contents/cinder_device.log";
    let mut tick: u32 = 0;
    let mut handoff = false; // true while we've yielded the screen to stock for USB-MSC
    let mut usb_hi: u8 = 0; // debounce: consecutive "host present" samples
    loop {
        if handoff {
            // Stock owns the screen and is performing its own (clean) mass-storage mount;
            // we must NOT draw to the framebuffer or touch /contents (it's being
            // unmounted). Just watch for the cable to come out, then resume Cinder.
            if !usb_host_present() {
                let _ = std::fs::remove_file(usb_flag); // wrapper re-freezes stock
                redirect_fds(log_path, libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND);
                println!("cinder-device: USB host gone — resuming Cinder");
                handoff = false;
                usb_hi = 0;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        // --- normal: paint Cinder over every framebuffer page ---
        for page in 0..pages {
            for y in 0..H {
                let dst_row = (page * H + y) * stride;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        canvas.buf.as_ptr().add(y * W) as *const u8,
                        base.add(dst_row),
                        copy_bytes,
                    );
                }
            }
        }
        // Trigger the panel update (see FB_ACTIVATE_FORCE above) — without this the write
        // above never becomes visible.
        var.xoffset = 0;
        var.yoffset = 0;
        var.activate |= FB_ACTIVATE_FORCE;
        unsafe {
            libc::ioctl(fd, FBIOPUT_VSCREENINFO, &mut var as *mut _);
        }
        tick = tick.wrapping_add(1);

        // Escape hatch (checked a few times a second, not every frame).
        if tick % 8 == 0 && escape.exists() {
            println!("cinder-device: /contents/cinder_off present — exiting");
            break;
        }

        // USB mass-storage handoff (~2x/s, debounced over ~1s so enumeration flicker
        // doesn't bounce us in and out). When a PC is connected we hand the screen back
        // to stock: only stock can cleanly release /contents for mass storage — a frozen
        // app can't, and forcing the unmount would corrupt the vfat volume. We drop our
        // own /contents log fd first so stock's `umount /contents` doesn't hit EBUSY.
        if tick % 12 == 0 {
            if usb_host_present() {
                usb_hi = usb_hi.saturating_add(1);
            } else {
                usb_hi = 0;
            }
            if usb_hi >= 2 {
                println!("cinder-device: USB host detected — yielding to stock for mass storage");
                redirect_fds("/dev/null", libc::O_WRONLY);
                let _ = std::fs::File::create(usb_flag); // wrapper thaws stock (SIGCONT)
                handoff = true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}
