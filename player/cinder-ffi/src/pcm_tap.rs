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

/// Up to `n` mono samples (-1..1) from `payload`, starting `offset` frames in: the channels are
/// averaged. Fewer when the slot ends first.
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
    pub samples: Vec<f32>,
    /// Which slot it came from, and the covered span, for the tuning log.
    pub slot: usize,
    pub first_us: i64,
    pub last_us: i64,
}

/// The open queue file, found again when it goes away.
pub struct Tap {
    dir: PathBuf,
    file: Option<(PathBuf, File)>,
    /// When the directory was last searched. A missing file (FM, USB-DAC, nothing ever played) is
    /// searched for at most once a second, not twenty times.
    searched: Option<std::time::Instant>,
}

impl Tap {
    pub fn new(dir: impl Into<PathBuf>) -> Tap {
        Tap { dir: dir.into(), file: None, searched: None }
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
        let got = self.read(t_us, n);
        if got.is_none() {
            // A failed read may be a renamed file: look again (at most once a second).
            self.file = None;
        }
        got
    }

    fn read(&mut self, t_us: i64, n: usize) -> Option<Window> {
        let f = self.open()?;
        let mut hdr = vec![0u8; HEADER_BYTES];
        let mut slots = [None; SLOTS];
        for (k, slot) in slots.iter_mut().enumerate() {
            f.read_exact_at(&mut hdr, (k * SLOT_BYTES) as u64).ok()?;
            *slot = Slot::parse(&hdr);
        }
        let k = choose(&slots, t_us)?;
        let s = slots[k]?;
        let offset = ((t_us - s.pts_us) * s.rate as i64 / 1_000_000).max(0) as usize;
        let mut payload = vec![0u8; s.bytes];
        f.read_exact_at(&mut payload, (k * SLOT_BYTES + HEADER_BYTES) as u64).ok()?;
        let mut samples = mono(&s, &payload, offset, n);
        let mut last = s.pts_us + s.span_us();
        if samples.len() < n {
            // The next packet in time, if the queue has it and it follows without a gap.
            let next = slots.iter().enumerate().find(|(_, o)| {
                o.is_some_and(|o| o.rate == s.rate && o.channels == s.channels && (o.pts_us - last).abs() <= 1_000)
            });
            if let Some((j, Some(o))) = next {
                let mut p2 = vec![0u8; o.bytes];
                if f.read_exact_at(&mut p2, (j * SLOT_BYTES + HEADER_BYTES) as u64).is_ok() {
                    samples.extend(mono(o, &p2, 0, n - samples.len()));
                    last = o.pts_us + o.span_us();
                }
            }
        }
        (!samples.is_empty()).then_some(Window { rate: s.rate, samples, slot: k, first_us: s.pts_us, last_us: last })
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
        assert_eq!((w.rate, w.slot, w.samples.len()), (44100, 0, 2048));
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
}
