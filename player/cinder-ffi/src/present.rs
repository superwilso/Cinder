//! Present thread — overlaps rasterization with presentation.
//!
//! WHY: a frame costs raster (~16.2 ms) + present (~16.6 ms software blit+flip) and the two were
//! serial, so the pump could never beat ~33 ms/frame ≈ 30 fps (measured, `cinder-probe --bench`).
//! Neither half is easy to make much cheaper — the present is vsync/ioctl-bound — but they use
//! different resources (CPU raster vs display DMA/ioctl wait), so running them on two threads makes
//! the frame cost max(raster, present) ≈ 16.6 ms ≈ 60 fps, with no change to what is drawn.
//!
//! SHAPE: a single-slot handoff, not a queue. The render thread finishes a frame and `submit`s it
//! by SWAPPING its canvas buffer with a recycled one (no copy, no allocation after the first two
//! frames — allocation churn has already caused one on-device OOM abort). The present thread takes
//! the frame, releases the slot immediately (that release is the parallelism), pushes pixels, then
//! recycles the buffer. Steady state is exactly 3 buffers: the canvas, one in flight, one spare.
//!
//! ESCAPE-LADDER CONTRACT (do not weaken):
//!  - `submit` BLOCKS while the previous frame is still queued. Normally that wait is
//!    (present − raster) ≈ sub-millisecond; but if the present thread ever wedges (driver hang —
//!    the class of failure that froze the panel on 2026-07-26), the NEXT submit blocks forever and
//!    the shell's per-frame `alarm(8)` watchdog fires → _exit → launcher bad-boot counter → stock.
//!    A dropping/replacing queue would instead let the app run "healthy" over a frozen panel,
//!    which is exactly the hole that disabled rung 1 of the ladder once already.
//!  - `FRAMES_PRESENTED` increments only AFTER a present call returns (pixels pushed, ioctl
//!    done). The shell gates its "first frame painted" health signal on that counter, so an
//!    async present can never claim health for a frame that hasn't reached the glass.
//!
//! The presenter itself is CONSTRUCTED ON the present thread (`start` takes an opener closure):
//! an EGL context is thread-affine, so the GPU path only works if `eglMakeCurrent` happens on the
//! thread that will call `eglSwapBuffers`. The software framebuffer doesn't care, but gets the
//! same treatment for one code path.
//!
//! Escape: `/contents/cinder_nothread` (or `CINDER_NOTHREAD=1`) keeps the old synchronous present
//! (see `cinder_render_init`) — strictly less machinery, per the ladder rule.

use crate::FRAMES_PRESENTED;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

/// Anything that can push a finished frame to the panel. `Presenter` (fb blit / EGL swap) on
/// device; mocks in the tests below.
pub(crate) trait PresentTarget: Send {
    fn present(&mut self, buf: &[u32]);
}

pub(crate) struct PresentThread {
    shared: Arc<Shared>,
    handle: Option<std::thread::JoinHandle<()>>,
}

struct Shared {
    slot: Mutex<Slot>,
    cv: Condvar,
}

#[derive(Default)]
struct Slot {
    /// A finished frame waiting for the present thread (depth 1 — see module doc).
    frame: Option<Vec<u32>>,
    /// A recycled buffer for the next submit's swap.
    spare: Option<Vec<u32>>,
    shutdown: bool,
}

impl PresentThread {
    /// Spawn the thread and construct the presenter ON it (EGL thread affinity). Blocks until the
    /// opener reports; a failed open joins the thread and returns its error, so the caller can
    /// fall back exactly as if it had called the opener itself.
    pub(crate) fn start<P, F>(open: F) -> Result<PresentThread, String>
    where
        P: PresentTarget + 'static,
        F: FnOnce() -> Result<P, String> + Send + 'static,
    {
        let shared = Arc::new(Shared { slot: Mutex::new(Slot::default()), cv: Condvar::new() });
        let sh = shared.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("cinder-present".into())
            .spawn(move || {
                let mut presenter = match open() {
                    Ok(p) => {
                        let _ = tx.send(Ok(()));
                        p
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                loop {
                    let buf = {
                        let mut g = sh.slot.lock().unwrap();
                        loop {
                            if let Some(b) = g.frame.take() {
                                break b;
                            }
                            if g.shutdown {
                                return;
                            }
                            g = sh.cv.wait(g).unwrap();
                        }
                    };
                    // Slot is free NOW — wake a blocked submit before the (slow) present, so the
                    // render thread rasterizes the next frame while this one goes to the panel.
                    sh.cv.notify_all();
                    presenter.present(&buf);
                    FRAMES_PRESENTED.fetch_add(1, Ordering::SeqCst);
                    sh.slot.lock().unwrap().spare = Some(buf);
                    sh.cv.notify_all(); // wakes wait_presented
                }
            })
            .map_err(|e| format!("spawn present thread: {e}"))?;
        match rx.recv() {
            Ok(Ok(())) => Ok(PresentThread { shared, handle: Some(handle) }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err("present thread died during presenter init".into()),
        }
    }

    /// Hand a finished frame over by swapping `canvas_buf` with a recycled buffer. The swapped-in
    /// buffer holds a stale frame, which is fine: every screen render begins with a full
    /// `c.fill(bg)` (the same invariant the reused-canvas optimisation already relies on).
    ///
    /// Blocks while the previous frame is still queued — that backpressure is the escape-ladder
    /// contract (module doc), not an implementation convenience.
    pub(crate) fn submit(&self, canvas_buf: &mut Vec<u32>) {
        let mut g = self.shared.slot.lock().unwrap();
        while g.frame.is_some() && !g.shutdown {
            g = self.shared.cv.wait(g).unwrap();
        }
        if g.shutdown {
            return;
        }
        let mut buf = g.spare.take().unwrap_or_else(|| vec![0u32; canvas_buf.len()]);
        if buf.len() != canvas_buf.len() {
            buf.resize(canvas_buf.len(), 0);
        }
        std::mem::swap(canvas_buf, &mut buf);
        g.frame = Some(buf);
        drop(g);
        self.shared.cv.notify_all();
    }

    /// Block until at least `target` frames have completed presentation. Used by the bench to
    /// time the true present cost through the thread, and available to anything that must know
    /// pixels reached the panel (not just that a frame was submitted).
    pub(crate) fn wait_presented(&self, target: u64) {
        let mut g = self.shared.slot.lock().unwrap();
        while FRAMES_PRESENTED.load(Ordering::SeqCst) < target && !g.shutdown {
            g = self.shared.cv.wait(g).unwrap();
        }
    }
}

impl Drop for PresentThread {
    fn drop(&mut self) {
        {
            self.shared.slot.lock().unwrap().shutdown = true;
        }
        self.shared.cv.notify_all();
        if let Some(h) = self.handle.take() {
            let _ = h.join(); // presenter drops on its own thread (EGL teardown affinity)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Records every presented frame's first pixel. `delay_ms` simulates a slow present so the
    /// backpressure path is exercised. NOTE: FRAMES_PRESENTED is process-global and other tests
    /// (or parallel test threads) bump it too, so all assertions here use this mock's own state.
    struct Mock {
        seen: Arc<Mutex<Vec<u32>>>,
        count: Arc<AtomicUsize>,
        delay_ms: u64,
    }

    impl PresentTarget for Mock {
        fn present(&mut self, buf: &[u32]) {
            if self.delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
            }
            self.seen.lock().unwrap().push(buf[0]);
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn wait_count(count: &AtomicUsize, want: usize) {
        let t0 = std::time::Instant::now();
        while count.load(Ordering::SeqCst) < want {
            assert!(t0.elapsed().as_secs() < 5, "present thread never caught up");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn frames_flow_through_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let (s2, c2) = (seen.clone(), count.clone());
        let t =
            PresentThread::start(move || Ok(Mock { seen: s2, count: c2, delay_ms: 0 })).unwrap();
        let mut canvas = vec![0u32; 64];
        for v in [11u32, 22, 33] {
            canvas[0] = v;
            t.submit(&mut canvas);
        }
        wait_count(&count, 3);
        assert_eq!(*seen.lock().unwrap(), vec![11, 22, 33]);
    }

    /// Depth-1 handoff: with a slow present, submits must block rather than drop — losing frames
    /// would be a display glitch, and NOT blocking would break the watchdog contract.
    #[test]
    fn backpressure_blocks_instead_of_dropping() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let (s2, c2) = (seen.clone(), count.clone());
        let t =
            PresentThread::start(move || Ok(Mock { seen: s2, count: c2, delay_ms: 40 })).unwrap();
        let mut canvas = vec![0u32; 64];
        let t0 = std::time::Instant::now();
        for v in 1..=4u32 {
            canvas[0] = v;
            t.submit(&mut canvas);
        }
        // 4 submits against a 40 ms present: at least two must have waited for the pipe to drain.
        assert!(t0.elapsed().as_millis() >= 60, "submits did not backpressure");
        wait_count(&count, 4);
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 3, 4], "a frame was dropped");
    }

    #[test]
    fn drop_joins_cleanly_mid_present() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let t = PresentThread::start(move || Ok(Mock { seen, count, delay_ms: 30 })).unwrap();
        let mut canvas = vec![7u32; 64];
        t.submit(&mut canvas);
        drop(t); // joins; must not hang or panic while the mock sleeps
    }

    #[test]
    fn failed_open_propagates_error() {
        let r = PresentThread::start(|| {
            Err::<Mock, String>("no display for you".into())
        });
        match r {
            Err(e) => assert!(e.contains("no display")),
            Ok(_) => panic!("open error was swallowed"),
        }
    }
}
