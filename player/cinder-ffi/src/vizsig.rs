//! What the sample-based visualisers draw (`cinder_ui::viz::Signal`), made from the PCM tap's
//! window each frame: a triggered scope trace, decimated stereo pairs, L/R meters with ballistics,
//! and a spectrogram history fed from whatever feeds the bars.
//!
//! Sony's analyzer gives twelve band levels and nothing else, so none of this existed while it was
//! the only source. The tap (`pcm_tap.rs`) reads the decoded audio itself, and with the samples in
//! hand the visualiser stops being twelve numbers: a waveform, a stereo picture, true peaks.

use std::time::Instant;

/// Points in the scope trace (the Now Playing block is 432 px wide; a point every pixel or so).
pub const SCOPE_POINTS: usize = 480;
/// Source samples per scope point: 2 shows about 22 ms at 44.1 kHz, a few cycles of a bass note.
pub const SCOPE_DECIMATE: usize = 2;
/// Points in the stereo field.
pub const XY_POINTS: usize = 512;
/// Spectrogram history: 108 frames, about five seconds at the 20 Hz the visualiser runs at.
pub const HIST_ROWS: usize = 108;
/// The meters' scale floor, dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;
/// The meter bar falls this many dB per second (peak meters' usual release: 20 dB/s).
const METER_FALL_DB_S: f32 = 20.0;
/// A held peak stays this long before it falls.
const HOLD_MS: f32 = 1500.0;
/// The sample styles count as live this long after the tap's last frame.
const LIVE_MS: u128 = 400;

/// Linear amplitude (0..1) to the meters' 0..1 scale.
pub fn db01(a: f32) -> f32 {
    if a <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * a.log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

/// Where the scope trace starts: the first rising zero crossing in the first half of the window,
/// after a dip below a small threshold (so noise around zero does not trigger). A steady tone then
/// starts at the same phase every frame and stands still, which is what a scope's trigger is for.
/// 0 when there is none — the trace then free-runs, as a scope does without a trigger.
pub fn trigger(s: &[f32]) -> usize {
    let limit = s.len() / 2;
    let peak = s
        .iter()
        .take(limit.max(1))
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let arm = -(peak * 0.1).max(1e-3);
    let mut armed = false;
    for i in 1..limit {
        if s[i] < arm {
            armed = true;
        }
        if armed && s[i - 1] < 0.0 && s[i] >= 0.0 {
            return i;
        }
    }
    0
}

/// The scope trace: `points` values from `start`, each the mean of `decim` samples. Shorter when
/// the window runs out.
pub fn scope(s: &[f32], start: usize, points: usize, decim: usize) -> Vec<f32> {
    let decim = decim.max(1);
    let mut out = Vec::with_capacity(points);
    let mut i = start;
    while out.len() < points && i + decim <= s.len() {
        out.push(s[i..i + decim].iter().sum::<f32>() / decim as f32);
        i += decim;
    }
    out
}

#[derive(Default)]
pub struct SigState {
    pub wave: Vec<f32>,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// [peak L, peak R, RMS L, RMS R] on the 0..1 dB scale, with the fall applied.
    pub meter: [f32; 4],
    pub hold: [f32; 2],
    hold_ms: [f32; 2],
    pub hist: Vec<f32>,
    pub hist_cols: usize,
    pub hist_head: usize,
    hist_len: usize,
    live_at: Option<Instant>,
}

impl SigState {
    /// One tap window: the trace, the stereo pairs, the meters.
    pub fn update(&mut self, mono: &[f32], left: &[f32], right: &[f32], dt_ms: f32) {
        let start = trigger(mono);
        self.wave = scope(mono, start, SCOPE_POINTS, SCOPE_DECIMATE);
        let n = left.len().min(right.len());
        let step = (n / XY_POINTS).max(1);
        self.left = left.iter().step_by(step).take(XY_POINTS).copied().collect();
        self.right = right
            .iter()
            .step_by(step)
            .take(XY_POINTS)
            .copied()
            .collect();
        let fall = METER_FALL_DB_S / -METER_FLOOR_DB * dt_ms / 1000.0;
        for (ch, src) in [left, right].iter().enumerate() {
            let src = &src[..n];
            let peak = src.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let rms = if n > 0 {
                (src.iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt()
            } else {
                0.0
            };
            // Instant attack, steady fall: what a peak meter does.
            self.meter[ch] = db01(peak).max(self.meter[ch] - fall);
            self.meter[2 + ch] = db01(rms).max(self.meter[2 + ch] - fall);
            if self.meter[ch] >= self.hold[ch] {
                self.hold[ch] = self.meter[ch];
                self.hold_ms[ch] = 0.0;
            } else {
                self.hold_ms[ch] += dt_ms;
                if self.hold_ms[ch] > HOLD_MS {
                    self.hold[ch] = (self.hold[ch] - fall).max(self.meter[ch]);
                }
            }
        }
        self.live_at = Some(Instant::now());
    }

    /// The tap has stopped (or never started): the sample styles have nothing true to draw.
    pub fn live(&self) -> bool {
        self.live_at
            .is_some_and(|t| t.elapsed().as_millis() <= LIVE_MS)
    }

    /// One frame of the bars into the spectrogram. A change of band count starts it afresh.
    pub fn push_hist(&mut self, levels: &[f32]) {
        let cols = levels.len();
        if cols == 0 {
            return;
        }
        if cols != self.hist_cols || self.hist.len() != cols * HIST_ROWS {
            self.hist_cols = cols;
            self.hist = vec![0.0; cols * HIST_ROWS];
            self.hist_head = 0;
            self.hist_len = 0;
        }
        // Overwrite the oldest row; it becomes the newest, and the head moves past it.
        let row = self.hist_head;
        self.hist[row * cols..(row + 1) * cols].copy_from_slice(levels);
        self.hist_head = (self.hist_head + 1) % HIST_ROWS;
        self.hist_len = (self.hist_len + 1).min(HIST_ROWS);
    }

    /// The view the UI draws. Sample parts are empty when the tap is not live; the history is
    /// there whenever the bars have been.
    pub fn view(&self) -> cinder_ui::viz::Signal<'_> {
        let live = self.live();
        cinder_ui::viz::Signal {
            wave: if live { &self.wave } else { &[] },
            left: if live { &self.left } else { &[] },
            right: if live { &self.right } else { &[] },
            meter: if live { self.meter } else { [0.0; 4] },
            hold: if live { self.hold } else { [0.0; 2] },
            hist: &self.hist,
            hist_cols: if self.hist_len > 0 { self.hist_cols } else { 0 },
            hist_rows: HIST_ROWS,
            hist_head: self.hist_head,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, period: f32, phase: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| ((i as f32 / period + phase) * std::f32::consts::TAU).sin() * amp)
            .collect()
    }

    /// A steady tone starts at the same phase every frame, whatever phase the window caught it at.
    #[test]
    fn the_trigger_holds_a_tone_still() {
        let mut starts = Vec::new();
        for k in 0..8 {
            let s = sine(2048, 100.0, k as f32 * 0.13, 0.5);
            let t = trigger(&s);
            assert!(t > 0 && t < 1024, "triggered at {t}");
            assert!(s[t] >= 0.0 && s[t - 1] < 0.0, "a rising crossing");
            starts.push(scope(&s, t, 16, 1)[3]);
        }
        let spread = starts.iter().cloned().fold(f32::MIN, f32::max)
            - starts.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            spread < 0.05,
            "the trace starts at one phase (spread {spread})"
        );
        assert_eq!(trigger(&vec![0.0; 2048]), 0, "silence free-runs");
    }

    #[test]
    fn the_scope_trace_is_decimated_and_bounded() {
        let s = sine(2048, 64.0, 0.0, 0.9);
        let w = scope(&s, 10, SCOPE_POINTS, SCOPE_DECIMATE);
        assert_eq!(w.len(), SCOPE_POINTS);
        assert!(w.iter().all(|v| v.abs() <= 0.9));
        assert!(
            scope(&s, 2040, 100, 2).len() == 4,
            "stops at the end of the window"
        );
    }

    /// Full scale reads 0 dBFS (1.0), -20 dBFS reads two thirds of the way up, silence reads nothing;
    /// a peak holds, then falls; the bar falls at 20 dB/s.
    #[test]
    fn meters_read_dbfs_and_fall_like_meters() {
        assert!((db01(1.0) - 1.0).abs() < 1e-6);
        assert!((db01(0.1) - 2.0 / 3.0).abs() < 1e-3);
        assert_eq!(db01(0.0), 0.0);
        let mut st = SigState::default();
        let loud = sine(2048, 50.0, 0.0, 1.0);
        let quiet = sine(2048, 50.0, 0.0, 0.01);
        st.update(&loud, &loud, &quiet, 50.0);
        assert!(
            st.meter[0] > 0.99 && st.meter[1] < 0.4,
            "left full, right quiet"
        );
        let held = st.hold[0];
        st.update(&quiet, &quiet, &quiet, 50.0);
        assert!(
            st.meter[0] < held && st.meter[0] > 0.9,
            "falls, but by one step: {}",
            st.meter[0]
        );
        assert_eq!(st.hold[0], held, "the held peak stays");
        for _ in 0..40 {
            st.update(&quiet, &quiet, &quiet, 50.0);
        }
        assert!(st.hold[0] < held, "and falls after the hold time");
    }

    #[test]
    fn stereo_pairs_are_decimated_to_size() {
        let mut st = SigState::default();
        let l = sine(2048, 40.0, 0.0, 0.5);
        let r = sine(2048, 40.0, 0.25, 0.5);
        st.update(&l, &l, &r, 50.0);
        assert_eq!(st.left.len(), XY_POINTS);
        assert_eq!(st.right.len(), XY_POINTS);
        let v = st.view();
        assert!(v.has(cinder_ui::viz::VizKind::Stereo) && v.has(cinder_ui::viz::VizKind::Scope));
    }

    /// The history is a ring: newest last, oldest at the head, restarted on a new band count.
    #[test]
    fn the_spectrogram_rings_and_restarts() {
        let mut st = SigState::default();
        assert!(!st.view().has(cinder_ui::viz::VizKind::Spectrogram));
        for k in 0..(HIST_ROWS + 3) {
            st.push_hist(&[k as f32; 24]);
        }
        let v = st.view();
        assert!(v.has(cinder_ui::viz::VizKind::Spectrogram));
        let newest = (v.hist_head + HIST_ROWS - 1) % HIST_ROWS;
        assert_eq!(v.hist[newest * 24], (HIST_ROWS + 2) as f32);
        assert_eq!(v.hist[v.hist_head * 24], 3.0, "the oldest row is the head");
        st.push_hist(&[1.0; 48]);
        assert_eq!(st.view().hist_cols, 48, "a new band count starts again");
    }
}
