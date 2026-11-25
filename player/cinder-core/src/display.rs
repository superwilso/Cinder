//! The panel: /dev/graphics/fb0 mapped and flipped, and how a finished frame leaves the render
//! thread (inline, or through the present thread in `present.rs`). Shared by both Cinder apps.
//! Moved out of cinder-ffi/src/lib.rs (2026-11-24) unchanged.

use crate::present;
use cinder_ui::{Canvas, H, W};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;

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
pub struct Framebuffer {
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
    /// The last blit rewrote most of the screen (a fling, a transition): the next one writes page 0
    /// only. See `blit`, "WHILE A LIST MOVES".
    moving: bool,
    /// Pages 1.. are behind page 0 and are brought up to date from the shadow when the screen goes
    /// still.
    others_stale: bool,
    /// One-shot efficiency sample, so the win is a measured number in the log rather than a claim.
    stat_frames: u32,
    stat_rows: u64,
    stat_done: bool,
    /// D1, opt-in: flip by queueing the overlay instead of `flip`'s blocking ioctl (`ovl.rs`).
    queued: Option<crate::ovl::Queued>,
}

/// How often to write every row whether it changed or not. Insurance, not correctness: nothing
/// else is known to write fb0 during a session, but the cost of being wrong about that is a
/// permanently stale region of screen, and the cost of the insurance is one full blit a minute.
const FULL_BLIT_EVERY_S: u64 = 60;

/// How long after opening fb0 to assume something else may also be drawing into it. Covers
/// icx_bootanimation, which cinder-home kills repeatedly over roughly the first five seconds.
const UNCONTESTED_AFTER_S: u64 = 15;

impl Framebuffer {
    pub fn open() -> Result<Self, String> {
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
            moving: false,
            others_stale: false,
            stat_frames: 0,
            stat_rows: 0,
            stat_done: false,
            queued: crate::ovl::Queued::new(fd, fix.smem_start as u32, stride, pages),
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
    pub fn blit(&mut self, buf: &[u32]) {
        let base = self.base as *mut u8;
        if self.opened.elapsed().as_secs() >= UNCONTESTED_AFTER_S {
            if let Some(q) = self.queued.as_mut() {
                q.present(base, self.map_len, buf);
                return;
            }
        }
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
        // WHILE A LIST MOVES, PAGE 0 ONLY. `flip` pins the pan to page 0, so after the opening
        // window pages 1 and 2 are insurance for whoever else pans (the boot animation), not what
        // the panel shows. Writing all three costs ~5 ms of a ~16 ms fling frame; page 0 alone is
        // a third of it. The other pages are caught up from the shadow on the first frame that is
        // not moving, so they are never behind for longer than the fling itself.
        let pages_now = if self.moving && !force_full {
            1
        } else {
            self.pages
        };
        #[cfg(feature = "dev")]
        let t_copy = std::time::Instant::now();

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
            for page in 0..pages_now {
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
        // A frame that wrote every page wrote only its CHANGED rows there, so only a forced full
        // write (every row, every page) or the catch-up below makes the other pages current.
        if pages_now < self.pages {
            self.others_stale = true;
        } else if force_full {
            self.others_stale = false;
        }
        self.moving = wrote * 2 > H;
        // Still again, with pages behind: copy the shadow (what page 0 holds) into them. They are
        // not on the panel, so this needs no flip.
        if self.others_stale && !self.moving {
            self.others_stale = false;
            for page in 1..self.pages {
                for y in 0..H {
                    let dst_row = (page * H + y) * self.stride;
                    if dst_row + copy_bytes > self.map_len {
                        break;
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            self.shadow.as_ptr().add(y * W) as *const u8,
                            base.add(dst_row),
                            copy_bytes,
                        );
                    }
                }
            }
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
        #[cfg(feature = "dev")]
        let t_flip = std::time::Instant::now();
        self.flip();
        #[cfg(feature = "dev")]
        if wrote * 2 > H {
            scroll_timing::blit(
                t_flip.duration_since(t_copy).as_micros() as u64,
                t_flip.elapsed().as_micros() as u64,
                pages_now,
            );
        }
    }

    /// Push the frame to the glass. mtkfb does NOT scan the framebuffer continuously — the panel
    /// only updates on this trigger ioctl (icx_bootanimation's flip, replicated exactly).
    /// Occasionally the driver blocks >33 ms here (the anim logs it as "heavy ioctl") — harmless
    /// at our frame rate, and skipped entirely now when no row changed.
    pub fn flip(&mut self) {
        self.var.xoffset = 0;
        self.var.yoffset = 0;
        self.var.activate |= FB_ACTIVATE_FORCE;
        let rc = unsafe { libc::ioctl(self.fd, FBIOPUT_VSCREENINFO, &mut self.var as *mut _) };
        if rc != 0 {
            // One-time diagnostic: a failing flip means an invisible UI, which is otherwise
            // indistinguishable from the old frozen-boot-image symptom on device.
            static FLIP_ERR: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !FLIP_ERR.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "cinder-ffi: fb flip ioctl FAILED (errno {}) — UI will not reach the panel",
                    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                );
            }
        }
    }
}

/// DEV BUILDS ONLY: where a scrolling frame's time goes, on the player. A drag dirties nearly every
/// row, so these are the frames that decide how closely the list follows the finger. Every 60
/// such frames: the raster (frames drawn within 100 ms of a drag move), the copy into the
/// framebuffer pages, and the flip ioctl. Written 2026-11-03 for "scrolls feel laggy".
#[cfg(feature = "dev")]
pub mod scroll_timing {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static LAST_MOVE_MS: AtomicU64 = AtomicU64::new(0);
    static RASTER: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    static COVERS: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
    static BLIT: [AtomicU64; 4] = [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ];
    pub fn ms() -> u64 {
        let mut t = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
        t.tv_sec as u64 * 1000 + t.tv_nsec as u64 / 1_000_000
    }
    /// [n, sum, max] in microseconds.
    pub fn add(a: &[AtomicU64], us: u64) {
        a[0].fetch_add(1, Relaxed);
        a[1].fetch_add(us, Relaxed);
        a[2].fetch_max(us, Relaxed);
    }
    pub fn take(a: &[AtomicU64]) -> (u64, f64, f64) {
        let n = a[0].swap(0, Relaxed).max(1);
        let sum = a[1].swap(0, Relaxed);
        let max = a[2].swap(0, Relaxed);
        (n, sum as f64 / n as f64 / 1000.0, max as f64 / 1000.0)
    }
    pub fn moved() {
        LAST_MOVE_MS.store(ms(), Relaxed);
    }
    pub fn covers(us: u64) {
        if us >= 100 {
            add(&COVERS, us);
        }
    }
    pub fn raster(us: u64) {
        if ms().saturating_sub(LAST_MOVE_MS.load(Relaxed)) <= 100 {
            add(&RASTER, us);
        }
    }
    pub fn blit(copy_us: u64, flip_us: u64, pages: usize) {
        add(&BLIT[..3], copy_us);
        BLIT[3].fetch_add(flip_us, Relaxed);
        if BLIT[0].load(Relaxed) >= 60 {
            let flip = BLIT[3].swap(0, Relaxed) as f64 / BLIT[0].load(Relaxed) as f64 / 1000.0;
            let (n, copy, copy_max) = take(&BLIT[..3]);
            let (rn, raster, raster_max) = take(&RASTER);
            let (cn, covers, covers_max) = take(&COVERS);
            println!(
                "cinder-ffi: scroll timing — {n} full frames: copy {copy:.1} ms (max {copy_max:.1}, {pages} pages), flip {flip:.1} ms; raster {raster:.1} ms (max {raster_max:.1}) over {rn} drag frames; cover loads {covers:.1} ms (max {covers_max:.1}) in {cn} frames"
            );
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

impl present::PresentTarget for Framebuffer {
    fn present(&mut self, buf: &[u32]) {
        self.blit(buf)
    }
}

/// Frames whose presentation has COMPLETED (blit + flip ioctl returned) —
/// i.e. pixels were pushed toward the glass, not merely queued. The shell reads this via
/// `cinder_frames_presented` to gate its "first frame painted" bad-boot health signal; with the
/// present running on its own thread, "cinder_render_tick returned" no longer implies that.
pub static FRAMES_PRESENTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How frames leave the render thread: inline (the original serial path, kept as the flagged
/// escape because it is strictly less machinery) or through the present thread (see present.rs).
// One Sink per process, so the size of the larger variant costs nothing.
#[allow(clippy::large_enum_variant)]
pub enum Sink {
    Sync(Framebuffer),
    Threaded(present::PresentThread),
}

impl Sink {
    pub fn present(&mut self, canvas: &mut Canvas) {
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
    pub fn wait_presented(&self, target: u64) {
        if let Sink::Threaded(t) = self {
            t.wait_presented(target);
        }
    }
}
