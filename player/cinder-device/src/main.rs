//! Device backend: render a Cinder screen onto /dev/graphics/fb0 (mtkfb,
//! 480x800, 32bpp XRGB8888, triple-buffered). Writes the Canvas to ALL
//! framebuffer pages so it's visible regardless of the active scanout page.
//! (fb-open/ioctl logic carried over from artifacts/build/cinder_fb_spike.)

use cinder_ui::now_playing::{self, NowPlaying};
use cinder_ui::{Canvas, FontSet, Theme, H, W};
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;

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
        liked: true,
        playing: true,
        shuffle: false,
        repeat: 1,
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
    let escape = std::path::Path::new("/contents/cinder_off");
    let mut tick: u32 = 0;
    loop {
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
        // Check the escape hatch a few times a second (not every frame).
        tick = tick.wrapping_add(1);
        if tick % 8 == 0 && escape.exists() {
            println!("cinder-device: /contents/cinder_off present — exiting");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}
