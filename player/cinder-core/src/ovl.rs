//! D1 (`docs/PLAN_v0.5.md`, "GPU and display hardware"): flip without sleeping to the next frame.
//!
//! `FBIOPUT_VSCREENINFO` (`Framebuffer::flip`) queues the overlay compose and then blocks until it
//! has run, up to a frame: most of the measured 9–10 ms "flip". `MTKFB_QUEUE_OVERLAY_CONFIG` queues
//! the same layer-0 description (the one mtkfb's pan builds, `mtkfb.c:826`) to a kernel worker
//! and returns a fence fd at once.
//!
//! Because the compose then reads the framebuffer AFTER the call returns, the next frame must not
//! write the page being composed. So frames alternate between pages 1 and 2, each with its own
//! shadow, and a page is written only after its last fence has signalled (normally long since).
//!
//! Opt-in while unproven on the player: `/contents/cinder_fb_queue` (dev builds), and never in
//! the first seconds of a boot, while the boot animation may still own the panel.

use cinder_ui::{H, W};

/// `_IOW('O', 137, struct fb_overlay_config)`, 472 bytes on the player's 32-bit kernel.
const MTKFB_QUEUE_OVERLAY_CONFIG: libc::Ioctl = 0x41D8_4F89u32 as libc::Ioctl;
/// `MAKE_MTK_FB_FORMAT_ID(4, 4)`: what pan uses at 32 bpp.
const FMT_ARGB8888: u32 = 0x404;

/// `struct fb_overlay_layer` (`include/linux/mtkfb.h`), pointers as the kernel's 32-bit words.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Layer {
    layer_id: u32,
    layer_enable: u32,
    src_base_addr: u32,
    src_phy_addr: u32,
    src_direct_link: u32,
    src_fmt: u32,
    src_use_color_key: u32,
    src_color_key: u32,
    src_pitch: u32,
    src_offset_x: u32,
    src_offset_y: u32,
    src_width: u32,
    src_height: u32,
    tgt_offset_x: u32,
    tgt_offset_y: u32,
    tgt_width: u32,
    tgt_height: u32,
    layer_rotation: u32,
    layer_type: u32,
    video_rotation: u32,
    is_tdshp: u32,
    next_buff_idx: i32,
    identity: i32,
    connected_type: i32,
    security: u32,
    fence_fd: i32,
    ion_fd: i32,
    alpha_enable: u32,
    alpha: u32,
}

/// `struct fb_overlay_config`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Config {
    fence: i32,
    time: i32,
    layers: [Layer; 4],
}

const _: () = assert!(std::mem::size_of::<Config>() == 472);

/// The two-page queued flip, over a framebuffer mapping it does not own.
pub struct Queued {
    fd: libc::c_int,
    /// Physical (= device) address of page 0.
    pa: u32,
    stride: usize,
    /// Per page (1 and 2): what it holds, and the fence of its last compose (-1 = none).
    shadow: [Vec<u32>; 2],
    fence: [i32; 2],
    /// The page last queued (0 or 1, meaning fb pages 1 or 2).
    cur: usize,
}

impl Queued {
    /// `pa` is `FBIOGET_FSCREENINFO`'s `smem_start`. `None` unless the opt-in file exists and the
    /// mapping has the three pages this needs.
    pub fn new(fd: libc::c_int, pa: u32, stride: usize, pages: usize) -> Option<Queued> {
        if pages < 3 || !std::path::Path::new("/contents/cinder_fb_queue").exists() {
            return None;
        }
        println!("cinder: fb queued flip ON (pages 1+2, fence per page)");
        Some(Queued {
            fd,
            pa,
            stride,
            // Nothing a canvas can hold, so the first frame on each page writes every row.
            shadow: [vec![u32::MAX; W * H], vec![u32::MAX; W * H]],
            fence: [-1, -1],
            cur: 0,
        })
    }

    /// Present `buf` through the mapping at `base` (`map_len` bytes). Returns the rows written, 0
    /// when the frame matched what is showing and nothing was queued.
    pub(crate) fn present(&mut self, base: *mut u8, map_len: usize, buf: &[u32]) -> usize {
        if buf.len() < W * H || self.shadow[self.cur] == buf[..W * H] {
            return 0;
        }
        let p = 1 - self.cur;
        self.wait(p);
        let copy = (W * 4).min(self.stride);
        let mut rows = 0;
        for y in 0..H {
            let row = y * W..(y + 1) * W;
            if self.shadow[p][row.clone()] == buf[row.clone()] {
                continue;
            }
            let dst = ((p + 1) * H + y) * self.stride;
            if dst + copy > map_len {
                break;
            }
            self.shadow[p][row.clone()].copy_from_slice(&buf[row.clone()]);
            // SAFETY: bounded against the mapping above.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    buf.as_ptr().add(y * W) as *const u8,
                    base.add(dst),
                    copy,
                )
            };
            rows += 1;
        }
        self.queue(p);
        self.cur = p;
        rows.max(1)
    }

    fn queue(&mut self, p: usize) {
        let off = ((p + 1) * H * self.stride) as u32;
        let mut c = Config {
            fence: -1,
            ..Default::default()
        };
        for (i, l) in c.layers.iter_mut().enumerate() {
            *l = Layer {
                layer_id: i as u32,
                fence_fd: -1,
                ion_fd: -1,
                next_buff_idx: -1,
                ..Default::default()
            };
        }
        c.layers[0] = Layer {
            layer_enable: 1,
            src_phy_addr: self.pa + off,
            src_fmt: FMT_ARGB8888,
            src_color_key: 0xFF,
            src_pitch: (self.stride / 4) as u32,
            src_width: W as u32,
            src_height: H as u32,
            tgt_width: W as u32,
            tgt_height: H as u32,
            is_tdshp: 1,
            ..c.layers[0]
        };
        let rc = unsafe { libc::ioctl(self.fd, MTKFB_QUEUE_OVERLAY_CONFIG, &mut c as *mut Config) };
        if rc != 0 {
            static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "cinder: queued flip FAILED (errno {})",
                    std::io::Error::last_os_error()
                );
            }
            return;
        }
        self.fence[p] = c.fence;
    }

    /// Wait (at most 50 ms: the kernel's own wait is a second) for page `p`'s last compose.
    fn wait(&mut self, p: usize) {
        let fd = std::mem::replace(&mut self.fence[p], -1);
        if fd < 0 {
            return;
        }
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut pfd, 1, 50);
            libc::close(fd);
        }
    }
}

impl Drop for Queued {
    fn drop(&mut self) {
        for f in self.fence {
            if f >= 0 {
                unsafe { libc::close(f) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_alternate_pages_and_write_only_what_each_page_lacks() {
        let stride = W * 4;
        let mut map = vec![0u8; 3 * H * stride];
        let mut q = Queued {
            fd: -1,
            pa: 0,
            stride,
            shadow: [vec![u32::MAX; W * H], vec![u32::MAX; W * H]],
            fence: [-1, -1],
            cur: 0,
        };
        let a = vec![1u32; W * H];
        let mut b = a.clone();
        b[5 * W] = 2;
        let len = map.len();
        assert_eq!(
            q.present(map.as_mut_ptr(), len, &a),
            H,
            "first frame: every row"
        );
        assert_eq!(q.cur, 1, "into fb page 2");
        assert_eq!(
            q.present(map.as_mut_ptr(), len, &a),
            0,
            "same frame: nothing queued"
        );
        assert_eq!(
            q.present(map.as_mut_ptr(), len, &b),
            H,
            "fb page 1 has never been written"
        );
        assert_eq!(q.cur, 0);
        assert_eq!(
            q.present(map.as_mut_ptr(), len, &a),
            1,
            "fb page 2 lacks only row 5's undo"
        );
        let px = |page: usize, y: usize| {
            u32::from_ne_bytes(map[(page * H + y) * stride..][..4].try_into().unwrap())
        };
        assert_eq!(
            (px(1, 5), px(2, 5), px(0, 5)),
            (2, 1, 0),
            "page 0 is never touched"
        );
    }
}
