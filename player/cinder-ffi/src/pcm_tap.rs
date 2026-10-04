//! The decoded audio, read from the queue PlayerService fills for SoundServiceFw — the visualiser's
//! own PCM tap (`docs/PLAN_pcm_visualiser.md`).
//!
//! Sony's AudioAnalyzerService is twelve IIR bandpasses and nothing more (`spectrum.rs`). But the
//! decoded samples pass between two of Sony's processes through two files in `/dev/shm`, owned by
//! `system` — the user cinder-home runs as — so we can read them and run our own transform:
//!
//! ```text
//! MappedShmHolderTK_MUSIC_PID_<pid>_PKT_131072_QUE_5_<n>          216 B   ring state
//! MappedShmHolderTK_MUSIC_PID_<pid>_PKT_131072_QUE_5_<n>_packet   5 slots of SLOT_BYTES
//! ```
//!
//! Each slot is a 368-byte header and room for 131,072 bytes of audio. Read off the player on
//! 2026-09-29 while a 16-bit, 44.1 kHz FLAC played (every value below is from those dumps):
//!
//! | Offset | Seen | Meaning |
//! |---|---|---|
//! | +4 | 44100 | sample rate |
//! | +8 | 16 | bits |
//! | +12 | 2 | channels |
//! | +240 | 130127526, 130173967, … | the slot's first frame, in µs of the track; +46441 per slot = 2048 / 44100 s |
//! | +364 | 8192 | bytes of audio in the slot |
//! | +368 | | interleaved little-endian samples |
//!
//! The five slots hold the newest ~650 ms of timestamps; a slot SoundServiceFw has taken keeps its
//! old packet until it is refilled. So the slot to draw is the one whose span covers the playback
//! position — and when none does, there is nothing true to draw, and the analyzer takes over.
//!
//! SAFETY RULES, all load-bearing:
//! * `pread`, never `mmap`. If PlayerService shrank or recreated the file under a mapping, the next
//!   read would be SIGBUS, and cinder-home dying leaves the player with no Home app.
//! * Never write, never lock. A read torn by a concurrent refill is one odd frame of bars.
//! * The file carries PlayerService's PID and a counter in its name: found by prefix, and found
//!   again whenever a read fails.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

pub const SHM_DIR: &str = "/dev/shm";
const PREFIX: &str = "MappedShmHolderTK_MUSIC_";
const SUFFIX: &str = "_packet";
pub const SLOTS: usize = 5;
pub const HEADER_BYTES: usize = 368;
pub const PAYLOAD_CAP: usize = 131_072;
pub const SLOT_BYTES: usize = HEADER_BYTES + PAYLOAD_CAP;

/// One slot's header, as far as the tap needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub rate: u32,
    pub bits: u32,
    pub channels: u32,
    /// The slot's first frame, in µs from the start of the track.
    pub pts_us: i64,
    pub bytes: usize,
}

impl Slot {
    /// Only what has been seen on the device: 16-bit PCM, one or two channels, a sane rate and a
    /// payload that is whole frames. Anything else — 24-bit, DSD, a header this does not
    /// understand — is refused, and the analyzer draws instead.
    pub fn parse(h: &[u8]) -> Option<Slot> {
        if h.len() < HEADER_BYTES {
            return None;
        }
        let i32_at = |o: usize| i32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
        let s = Slot {
            rate: i32_at(4) as u32,
            bits: i32_at(8) as u32,
            channels: i32_at(12) as u32,
            pts_us: i32_at(240) as u32 as i64,
            bytes: i32_at(364) as u32 as usize,
        };
        let frame = s.frame_bytes();
        let ok = s.bits == 16
            && (1..=2).contains(&s.channels)
            && (8_000..=384_000).contains(&s.rate)
            && s.bytes > 0
            && s.bytes <= PAYLOAD_CAP
            && s.bytes % frame == 0;
        ok.then_some(s)
    }

    pub fn frame_bytes(&self) -> usize {
        (self.bits as usize / 8) * self.channels as usize
    }

    pub fn frames(&self) -> usize {
        self.bytes / self.frame_bytes().max(1)
    }

    pub fn span_us(&self) -> i64 {
        self.frames() as i64 * 1_000_000 / self.rate.max(1) as i64
    }

    /// Does this slot hold the frame at `t_us`?
    pub fn covers(&self, t_us: i64) -> bool {
        t_us >= self.pts_us && t_us < self.pts_us + self.span_us()
    }
}

/// The slot holding the frame at `t_us`, among the ones that parse.
pub fn choose(slots: &[Option<Slot>], t_us: i64) -> Option<usize> {
    slots.iter().position(|s| s.is_some_and(|s| s.covers(t_us)))
}

/// How much audio the tap keeps, in µs. THE QUEUE IS AHEAD OF THE EAR: it holds what PlayerService
/// has decoded and not yet handed on, and on the player that was 630 ms after the reported position
/// (2026-10-04: `pos 17084 ms, slot 2 holds 17718..17764 ms`). By the time a moment is heard its
/// packet has been overwritten, so the queue alone can never cover the position — which is why the
/// first build drew for a split second and stopped. The tap therefore copies each packet as it
/// appears and draws from its own copy. 1.5 s is the lead seen, twice over; at 44.1 kHz that is
/// about 265 kB.
pub const HIST_US: i64 = 1_500_000;

/// How far a slot may be from the position and still be drawn. The queue holds about 190 ms (four
/// 46 ms packets and one being refilled) and the position is the service's once-a-second report
/// carried forward by the clock, so "no slot covers it" was the NORMAL case on the player, not
/// the exception: 2026-10-04, a whole session with no tap frame and Scope, Stereo field and
/// Meters empty. Within this distance the nearest packet is the music of this moment to the eye;
/// beyond it (a seek, a pause, another track's leftovers) nothing is drawn.
pub const NEAR_US: i64 = 750_000;

/// How far `t_us` is outside a slot's span: 0 inside, else the gap in µs.
fn gap_us(s: &Slot, t_us: i64) -> i64 {
    if t_us < s.pts_us {
        s.pts_us - t_us
    } else {
        (t_us - (s.pts_us + s.span_us()) + 1).max(0)
    }
}

/// [`choose`], and when nothing covers `t_us`, the nearest slot within [`NEAR_US`]. Returns the
/// slot and how far off it is (0 = it covers the position).
pub fn choose_near(slots: &[Option<Slot>], t_us: i64) -> Option<(usize, i64)> {
    if let Some(k) = choose(slots, t_us) {
        return Some((k, 0));
    }
    slots
        .iter()
        .enumerate()
        .filter_map(|(k, s)| s.map(|s| (k, gap_us(&s, t_us))))
        .filter(|(_, g)| *g <= NEAR_US)
        .min_by_key(|(_, g)| *g)
}

/// Up to `n` mono samples (-1..1) from `payload`, starting `offset` frames in: the channels are
/// averaged. Fewer when the slot ends first. The tap reads [`stereo`] and averages that; this stays
/// as the reference the tests hold it to.
#[cfg(test)]
pub fn mono(slot: &Slot, payload: &[u8], offset: usize, n: usize) -> Vec<f32> {
    let fb = slot.frame_bytes();
    let ch = slot.channels as usize;
    let frames = (payload.len() / fb.max(1)).min(slot.frames());
    let from = offset.min(frames);
    let to = (from + n).min(frames);
    let mut out = Vec::with_capacity(to - from);
    for f in from..to {
        let base = f * fb;
        let mut acc = 0i32;
        for c in 0..ch {
            let o = base + c * 2;
            acc += i16::from_le_bytes([payload[o], payload[o + 1]]) as i32;
        }
        out.push(acc as f32 / (32768.0 * ch as f32));
    }
    out
}

/// Up to `n` frames as two channels (-1..1), for the visualisers that need the stereo picture (the
/// stereo field, the L/R meters). A one-channel slot gives the same samples on both sides.
pub fn stereo(slot: &Slot, payload: &[u8], offset: usize, n: usize) -> (Vec<f32>, Vec<f32>) {
    let fb = slot.frame_bytes();
    let ch = slot.channels as usize;
    let frames = (payload.len() / fb.max(1)).min(slot.frames());
    let from = offset.min(frames);
    let to = (from + n).min(frames);
    let (mut l, mut r) = (Vec::with_capacity(to - from), Vec::with_capacity(to - from));
    for f in from..to {
        let base = f * fb;
        let a = i16::from_le_bytes([payload[base], payload[base + 1]]) as f32 / 32768.0;
        let b = if ch > 1 { i16::from_le_bytes([payload[base + 2], payload[base + 3]]) as f32 / 32768.0 } else { a };
        l.push(a);
        r.push(b);
    }
    (l, r)
}

/// The queue file, if PlayerService has one: the first `…_packet` with the prefix.
pub fn find(dir: &Path) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(SUFFIX))
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// A window of the audio at the playback position.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub rate: u32,
    /// Mono, (L + R) / 2: what the spectrum and the scope use.
    pub samples: Vec<f32>,
    /// The two channels, frame for frame with `samples`.
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// Which slot it came from, and the covered span, for the tuning log.
    pub slot: usize,
    pub first_us: i64,
    pub last_us: i64,
    /// How far the slot was from the position asked for, in µs: 0 when it covered it.
    pub off_us: i64,
}

/// The open queue file, found again when it goes away.
pub struct Tap {
    dir: PathBuf,
    /// The packets seen lately, oldest first — see [`HIST_US`].
    hist: std::collections::VecDeque<(Slot, Vec<u8>)>,
    /// Each slot's timestamp when it was last copied, so a slot is copied once per refill.
    seen: [i64; SLOTS],
    /// When a refilled slot was last copied.
    fresh_at: Option<std::time::Instant>,
    file: Option<(PathBuf, File)>,
    /// When the directory was last searched. A missing file (FM, USB-DAC, nothing ever played) is
    /// searched for at most once a second, not twenty times.
    searched: Option<std::time::Instant>,
}

impl Tap {
    pub fn new(dir: impl Into<PathBuf>) -> Tap {
        Tap { dir: dir.into(), hist: Default::default(), seen: [-1; SLOTS], fresh_at: None, file: None, searched: None }
    }

    fn open(&mut self) -> Option<&File> {
        if self.file.is_none() {
            if self.searched.is_some_and(|t| t.elapsed().as_millis() < 1000) {
                return None;
            }
            self.searched = Some(std::time::Instant::now());
            let path = find(&self.dir)?;
            let f = File::open(&path).ok()?;
            self.file = Some((path, f));
        }
        self.file.as_ref().map(|(_, f)| f)
    }

    /// `n` samples of the audio at `t_us` into the track, or `None` when the queue does not hold
    /// that moment in a format this reads. The window starts at `t_us`; when the slot ends first, it
    /// carries on into the next slot if that one follows straight on.
    pub fn window(&mut self, t_us: i64, n: usize) -> Option<Window> {
        if self.ingest().is_none() {
            // The file could not be read: look for it again (at most once a second).
            self.file = None;
            return None;
        }
        // A file that has stopped changing may have been replaced by one with another name
        // (PlayerService restarted; the old one stays readable through the open handle). Look
        // again, without throwing away what was kept. A MISS is not a reason to: until 2026-10-04
        // every miss closed the file, and the second it then took to reopen was a hole.
        if self.fresh_at.is_some_and(|t| t.elapsed().as_millis() > 2000) {
            self.file = None;
            self.fresh_at = None;
        }
        self.pick(t_us, n)
    }

    /// Copy every slot that has been refilled since the last call into the history. `None` when
    /// there is no queue file to read.
    fn ingest(&mut self) -> Option<()> {
        let mut fresh: Vec<(usize, Slot, Vec<u8>)> = Vec::new();
        {
            let seen = self.seen;
            let f = self.open()?;
            let mut hdr = vec![0u8; HEADER_BYTES];
            for (k, was) in seen.iter().enumerate() {
                f.read_exact_at(&mut hdr, (k * SLOT_BYTES) as u64).ok()?;
                let Some(s) = Slot::parse(&hdr) else { continue };
                if s.pts_us == *was {
                    continue;
                }
                let mut payload = vec![0u8; s.bytes];
                f.read_exact_at(&mut payload, (k * SLOT_BYTES + HEADER_BYTES) as u64).ok()?;
                // A slot refilled while it was being copied is half one packet and half the next:
                // leave it for the next call, when its header has settled.
                f.read_exact_at(&mut hdr, (k * SLOT_BYTES) as u64).ok()?;
                if Slot::parse(&hdr) == Some(s) {
                    fresh.push((k, s, payload));
                }
            }
        }
        let Some(newest) = fresh.iter().map(|(_, s, _)| s.pts_us).max() else { return Some(()) };
        self.fresh_at = Some(std::time::Instant::now());
        for (k, s, payload) in fresh {
            self.seen[k] = s.pts_us;
            if !self.hist.iter().any(|(h, _)| h.pts_us == s.pts_us) {
                let at = self.hist.partition_point(|(h, _)| h.pts_us < s.pts_us);
                self.hist.insert(at, (s, payload));
            }
        }
        // Keep what leads up to the newest packet. A seek or a new track lands its packets outside
        // that span on one side or the other, and the old ones go.
        self.hist.retain(|(h, _)| h.pts_us > newest - HIST_US && h.pts_us <= newest + HIST_US);
        Some(())
    }

    fn pick(&self, t_us: i64, n: usize) -> Option<Window> {
        let slots: Vec<Option<Slot>> = self.hist.iter().map(|(s, _)| Some(*s)).collect();
        let (k, off_us) = choose_near(&slots, t_us)?;
        let (s, payload) = &self.hist[k];
        let s = *s;
        // A packet that does not cover the position is read from its start.
        let offset = if off_us == 0 { ((t_us - s.pts_us) * s.rate as i64 / 1_000_000).max(0) as usize } else { 0 };
        let (mut left, mut right) = stereo(&s, payload, offset, n);
        let mut last = s.pts_us + s.span_us();
        // Carry on into the packets that follow without a gap, until the window is full.
        for (o, p2) in self.hist.iter().skip(k + 1) {
            if left.len() >= n || o.rate != s.rate || o.channels != s.channels || (o.pts_us - last).abs() > 1_000 {
                break;
            }
            let (l2, r2) = stereo(o, p2, 0, n - left.len());
            left.extend(l2);
            right.extend(r2);
            last = o.pts_us + o.span_us();
        }
        let samples: Vec<f32> = left.iter().zip(&right).map(|(a, b)| (a + b) * 0.5).collect();
        (!samples.is_empty()).then_some(Window { rate: s.rate, samples, left, right, slot: k, first_us: s.pts_us, last_us: last, off_us })
    }

    /// What the tap holds right now, as `(earliest start, latest end)` in µs — for the log line
    /// that says why a frame was not drawn. `None` when there is no queue or nothing in it parses.
    pub fn held(&mut self) -> Option<(i64, i64)> {
        self.ingest()?;
        let a = self.hist.front().map(|(s, _)| s.pts_us)?;
        let b = self.hist.back().map(|(s, _)| s.pts_us + s.span_us())?;
        Some((a, b))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A slot header laid out exactly as the device's (see the module comment).
    fn header(rate: u32, bits: u32, ch: u32, pts_us: u32, bytes: u32) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_BYTES];
        let mut put = |o: usize, v: u32| h[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(4, rate);
        put(8, bits);
        put(12, ch);
        put(64 + 4, rate);
        put(64 + 8, bits);
        put(240, pts_us);
        put(248, 1.0f32.to_bits());
        put(364, bytes);
        h
    }

    /// A queue file of five slots, each 2048 stereo frames of a constant `L`/`R` pair.
    /// THE QUEUE IS AHEAD OF THE EAR (the player, 2026-10-04: the reported position 630 ms behind
    /// every slot). A moment the tap saw go by is still drawn once the queue has moved on, exactly;
    /// a seek away drops what was kept.
    #[test]
    fn a_moment_the_queue_has_moved_past_is_drawn_from_the_taps_own_copy() {
        let d = tmp("history");
        let name = "MappedShmHolderTK_MUSIC_PID_478_PKT_131072_QUE_5_2_packet";
        let step = 46_439u32;
        let at = |first: u32| [first, first + step, first + 2 * step, first + 3 * step, first + 4 * step];
        queue(&d, name, at(10_000_000), [(8192, 8192); 5]);
        let mut tap = Tap::new(&d);
        // What is heard now is 600 ms before anything in the queue: nothing exact to draw yet, so
        // the nearest packet stands in and says how far off it is.
        let w = tap.window(9_400_000, 2048).expect("the nearest packet");
        assert_eq!(w.off_us, 600_000);
        // The queue moves on by more than its own length; the ear reaches the first packet.
        queue(&d, name, at(10_600_000), [(-8192, -8192); 5]);
        let w = tap.window(10_000_100, 2048).expect("the copy the tap kept");
        assert_eq!(w.off_us, 0, "drawn exactly, from history");
        assert_eq!(w.samples[0], 0.25, "…and it is the OLD packet's audio");
        assert_eq!(tap.held().map(|(a, _)| a), Some(10_000_000));
        // A seek back to the start: the kept packets are from another place and go.
        queue(&d, name, at(1_000_000), [(0, 0); 5]);
        assert!(tap.window(10_000_100, 2048).is_none(), "the old place is gone after a seek");
        assert_eq!(tap.held().map(|(a, _)| a), Some(1_000_000));
        let _ = std::fs::remove_dir_all(&d);
    }

    fn queue(dir: &Path, name: &str, pts: [u32; 5], lr: [(i16, i16); 5]) -> PathBuf {
        let mut body = Vec::new();
        for k in 0..SLOTS {
            body.extend(header(44100, 16, 2, pts[k], 8192));
            let mut payload = Vec::new();
            for _ in 0..2048 {
                payload.extend(lr[k].0.to_le_bytes());
                payload.extend(lr[k].1.to_le_bytes());
            }
            payload.resize(PAYLOAD_CAP, 0);
            body.extend(payload);
        }
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cinder-pcmtap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The header values read off the player parse, and the spans come out as measured.
    #[test]
    fn the_devices_header_parses() {
        let s = Slot::parse(&header(44100, 16, 2, 130_127_526, 8192)).unwrap();
        assert_eq!((s.rate, s.channels, s.frames()), (44100, 2, 2048));
        assert_eq!(s.span_us(), 46_439, "2048 / 44100 s, as the timestamps step");
        assert!(s.covers(130_127_526) && s.covers(130_127_526 + 46_000) && !s.covers(130_127_526 + 46_439));
    }

    #[test]
    fn anything_not_seen_on_the_device_is_refused() {
        for (rate, bits, ch, bytes) in [
            (44100, 24, 2, 8192),   // 24-bit: layout not seen
            (44100, 1, 2, 8192),    // DSD-like
            (44100, 16, 6, 8196),   // six channels
            (0, 16, 2, 8192),       // no rate
            (44100, 16, 2, 0),      // empty
            (44100, 16, 2, 200_000), // past the slot
            (44100, 16, 2, 8190),   // not whole frames
        ] {
            assert_eq!(Slot::parse(&header(rate, bits, ch, 0, bytes)), None, "{rate} {bits} {ch} {bytes}");
        }
        assert_eq!(Slot::parse(&[0u8; 20]), None, "a short read");
    }

    /// The slot covering the position is chosen whatever order the ring left them in, and a
    /// position no slot holds gives nothing — the analyzer's turn, not a stale frame.
    #[test]
    fn the_slot_covering_the_position_is_chosen() {
        let at = |pts: u32| Slot::parse(&header(44100, 16, 2, pts, 8192));
        let slots = [at(1_000_000), at(1_139_319), at(1_046_439), at(1_092_879), None];
        assert_eq!(choose(&slots, 1_050_000), Some(2));
        assert_eq!(choose(&slots, 1_000_000), Some(0));
        assert_eq!(choose(&slots, 1_185_757), Some(1));
        assert_eq!(choose(&slots, 999_999), None, "already played and overwritten");
        assert_eq!(choose(&slots, 2_000_000), None, "not decoded yet");
        // The nearest slot stands in when nothing covers the position, inside NEAR_US only.
        assert_eq!(choose_near(&slots, 1_050_000), Some((2, 0)), "a covering slot is not 'near'");
        assert_eq!(choose_near(&slots, 999_999).map(|(k, _)| k), Some(0), "just behind the queue");
        assert_eq!(choose_near(&slots, 999_999).map(|(_, g)| g), Some(1));
        let far = slots.iter().flatten().map(|s| s.pts_us + s.span_us()).max().unwrap() + NEAR_US + 1;
        assert_eq!(choose_near(&slots, far), None, "a seek away: nothing true to draw");
        assert_eq!(choose_near(&slots, 1_000_000 - NEAR_US - 1), None);
    }

    #[test]
    fn stereo_is_averaged_to_mono_from_the_offset() {
        let s = Slot::parse(&header(44100, 16, 2, 0, 16)).unwrap(); // four frames
        let mut p = Vec::new();
        for (l, r) in [(16384i16, 0i16), (-32768, -32768), (100, 300), (0, 0)] {
            p.extend(l.to_le_bytes());
            p.extend(r.to_le_bytes());
        }
        let m = mono(&s, &p, 0, 10);
        assert_eq!(m.len(), 4, "only what the slot holds");
        assert_eq!(m[0], 0.25);
        assert_eq!(m[1], -1.0);
        assert_eq!(mono(&s, &p, 2, 1), vec![200.0 / 32768.0]);
        assert!(mono(&s, &p, 9, 4).is_empty(), "an offset past the end");
    }

    /// End to end against a file laid out like the device's: found by prefix, the right slot, and
    /// a window that runs on into the next slot when it follows straight on.
    #[test]
    fn a_window_is_read_from_the_queue_file_and_runs_on_into_the_next_slot() {
        let d = tmp("window");
        std::fs::write(d.join("MappedShmHolderTK_MUSIC_PID_478_PKT_131072_QUE_5_2"), [0u8; 216]).unwrap();
        std::fs::write(d.join("bt.oppc.shm"), [0u8; 16]).unwrap();
        let pts = [2_000_000, 2_046_439, 1_860_000, 1_906_000, 1_953_000];
        queue(&d, "MappedShmHolderTK_MUSIC_PID_478_PKT_131072_QUE_5_2_packet", pts, [
            (8192, 8192),
            (-8192, -8192),
            (0, 0),
            (0, 0),
            (0, 0),
        ]);
        let mut tap = Tap::new(&d);
        // 1000 frames before slot 0 ends: 1000 of slot 0, then 1048 of slot 1.
        let t = 2_000_000 + (1048i64 * 1_000_000 / 44100) + 1;
        let w = tap.window(t, 2048).expect("a window");
        // `slot` is the packet's place in the tap's history, which is in time order: three older
        // packets come before this one.
        assert_eq!((w.rate, w.slot, w.samples.len(), w.off_us), (44100, 3, 2048, 0));
        assert_eq!(w.samples[0], 0.25);
        assert_eq!(w.samples[2047], -0.25, "the tail came from the next slot");
        assert!(tap.window(5_000_000, 2048).is_none(), "a moment the queue does not hold");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// No queue file (FM, USB-DAC, nothing played yet): nothing, and no error.
    #[test]
    fn no_queue_file_is_simply_no_window() {
        let d = tmp("none");
        let mut tap = Tap::new(&d);
        assert!(tap.window(0, 2048).is_none());
        let mut gone = Tap::new(d.join("does-not-exist"));
        assert!(gone.window(0, 2048).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The stereo read is the mono read, channel by channel: their mean is exactly `mono`.
    #[test]
    fn stereo_channels_average_to_mono() {
        let s = Slot { rate: 44100, bits: 16, channels: 2, pts_us: 0, bytes: 40 };
        let mut p = Vec::new();
        for i in 0..10i16 {
            p.extend_from_slice(&(i * 300).to_le_bytes());
            p.extend_from_slice(&(-i * 100).to_le_bytes());
        }
        let (l, r) = stereo(&s, &p, 0, 10);
        let m = mono(&s, &p, 0, 10);
        for i in 0..10 {
            assert!(((l[i] + r[i]) * 0.5 - m[i]).abs() < 1e-6);
        }
        assert_eq!(l[2], 600.0 / 32768.0);
        assert_eq!(r[2], -200.0 / 32768.0);
        let one = Slot { channels: 1, bytes: 20, ..s };
        let (a, b) = stereo(&one, &p[..20], 0, 10);
        assert_eq!(a, b, "one channel is both sides");
    }
}
