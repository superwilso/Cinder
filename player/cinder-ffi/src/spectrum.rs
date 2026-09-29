//! Spectrum extraction: turn what the audio source gives us into per-bar levels for the
//! visualiser. Two entry points, because there are two sources —
//!
//!   * `from_bands`: Sony's AudioAnalyzerService band magnitudes (the ONLY real source on device;
//!     a bank of IIR bandpasses, twelve of them, no FFT and no PCM anywhere in reach), and
//!   * `levels`: our own radix-2 FFT over a PCM window, for the host preview harness and for any
//!     future path that hands us samples directly.
//!
//! Both end in the same place: `bars` values in 0..1, dB-mapped, smoothed with real time
//! constants. The settings that steer them live in `cinder_ui::vizcfg`, next to the settings
//! screen that edits them.
//!
//! The FFT is hand-rolled (no rustfft crate — it keeps the ARM/glibc-2.23 cross-build clean) and
//! costs a 512-point transform a few times a second.

use cinder_ui::vizcfg::{Interp, Scale, VizCfg};

/// In-place iterative radix-2 Cooley-Tukey FFT. `re`/`im` length must be a power of two.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    if n <= 1 {
        return;
    }
    // bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // butterflies
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let half = len / 2;
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..half {
                let a = i + k;
                let b = a + half;
                let (tr, ti) = (cr * re[b] - ci * im[b], cr * im[b] + ci * re[b]);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Largest power of two <= n (and >= 1).
fn pow2_floor(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut p = 1;
    while p << 1 <= n {
        p <<= 1;
    }
    p
}

/// Compute `bars` normalised (0..1) levels from a PCM window (mono i16 samples). Empty input ->
/// empty output. Hann window, log-spaced bands, dB mapping over `cfg.range_db`, then the same
/// attack/decay pair as `from_bands` — one visual behaviour whichever source is feeding it.
pub fn levels(pcm: &[i16], bars: usize, prev: &[f32], cfg: &VizCfg, dt_ms: f32) -> Vec<f32> {
    if pcm.is_empty() || bars == 0 {
        return Vec::new();
    }
    let n = pow2_floor(pcm.len()).min(1024); // cap FFT size for cost
    let start = pcm.len() - n; // most recent n samples
    let mut re = vec![0.0f32; n];
    let mut im = vec![0.0f32; n];
    for i in 0..n {
        // Hann window + normalise i16 -> -1..1
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1).max(1) as f32).cos();
        re[i] = (pcm[start + i] as f32 / 32768.0) * w;
    }
    fft(&mut re, &mut im);

    let half = n / 2;
    let mut out = vec![0.0f32; bars];
    // log-spaced band edges across [1, half)
    for (b, slot) in out.iter_mut().enumerate() {
        let lo = band_edge(b, bars, half);
        let hi = band_edge(b + 1, bars, half).max(lo + 1);
        let mut mag = 0.0f32;
        for k in lo..hi {
            mag += (re[k] * re[k] + im[k] * im[k]).sqrt();
        }
        mag /= (hi - lo) as f32;
        // dB against digital full scale, over the configured window — the same curve the analyzer
        // path uses, so switching source does not change how loud the display looks.
        let v = to_frac(mag, 1.0, cfg.range_db);
        *slot = smooth_dt(v, prev, bars, b, cfg, dt_ms);
    }
    out
}

fn band_edge(b: usize, bars: usize, half: usize) -> usize {
    // log scale from bin 1 to `half`
    let t = b as f32 / bars as f32;
    let e = (1.0f32).max((half as f32).powf(t));
    (e as usize).clamp(1, half)
}

/// The magnitude that maps to the top of the display in `Scale::Fixed`.
///
/// MEASURED, not guessed: `cinder-probe --vizlab` on device (2026-09-06), with the twelve
/// log-spaced bands at Q = 1.75, reported per-band maxima of 1.8–2.1e9 on loud passages and band
/// averages of 5.6e7 (16 kHz) to 5.7e8 (mid). 2.0e9 therefore puts a loud track's peak bands at the
/// top of the display without clipping them flat, and with the default 60 dB window the floor sits
/// at 2e6 — below the quietest band the analyzer produced on music.
///
/// It only affects the FIXED scale; DYNAMIC derives its reference from the material. And it is
/// specific to our Q: a narrower filter passes less energy, so a band table with a different Q
/// would need this measured again.
pub const FIXED_REF: f32 = 2_000_000_000.0;

/// Convert one raw band magnitude to a 0..1 display fraction against `ref_mag`, over `range_db`.
#[inline]
fn to_frac(mag: f32, ref_mag: f32, range_db: f32) -> f32 {
    if mag <= 0.0 || ref_mag <= 0.0 {
        return 0.0; // log of zero is -inf, and a NaN reaching the renderer draws garbage
    }
    let db = 20.0 * (mag / ref_mag).log10();
    ((db + range_db) / range_db).clamp(0.0, 1.0)
}

/// Resample `src` (already 0..1) to `bars` values.
///
/// Down (more source than bars): average each output bucket's span. Up: interpolate between band
/// CENTRES, because block-averaging twelve bands into thirty-six columns gives three identical
/// bars per band — a staircase of twelve wide steps claiming a resolution the data does not have.
fn resample(src: &[f32], bars: usize, interp: Interp) -> Vec<f32> {
    let n = src.len();
    let mut out = vec![0.0f32; bars];
    if n == 0 || bars == 0 {
        return out;
    }
    if n >= bars {
        for (b, slot) in out.iter_mut().enumerate() {
            let lo = b * n / bars;
            let hi = (((b + 1) * n / bars).max(lo + 1)).min(n);
            let mut s = 0.0f32;
            for &x in &src[lo..hi] {
                s += x;
            }
            *slot = s / (hi - lo) as f32;
        }
        return out;
    }
    let at = |i: isize| -> f32 { src[(i.clamp(0, n as isize - 1)) as usize] };
    for (b, slot) in out.iter_mut().enumerate() {
        let t = if bars > 1 { b as f32 * (n - 1) as f32 / (bars - 1) as f32 } else { 0.0 };
        let i0 = (t.floor() as isize).clamp(0, n as isize - 1);
        let f = t - i0 as f32;
        *slot = match interp {
            Interp::Linear => at(i0) * (1.0 - f) + at(i0 + 1) * f,
            Interp::Smooth => {
                // Catmull-Rom through the four surrounding band centres, then CLAMPED to the two
                // it sits between: a spline overshoots on a steep step, and an overshoot here is a
                // bar taller than any band the analyzer actually reported.
                let (p0, p1, p2, p3) = (at(i0 - 1), at(i0), at(i0 + 1), at(i0 + 2));
                let f2 = f * f;
                let f3 = f2 * f;
                let v = 0.5
                    * ((2.0 * p1)
                        + (-p0 + p2) * f
                        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f2
                        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f3);
                v.clamp(p1.min(p2), p1.max(p2))
            }
        };
    }
    out
}

/// One-pole smoothing coefficient for a time constant `tau_ms` over an elapsed `dt_ms`.
///
/// This is the frame-rate-independent form. The old code applied a fixed 0.6/0.3 per FRAME, so the
/// display's whole character changed with the analyzer's emit rate — a setting the user can now
/// choose, which would have silently retuned the smoothing every time they touched it.
#[inline]
fn coef(tau_ms: f32, dt_ms: f32) -> f32 {
    if tau_ms <= 0.0 {
        return 1.0;
    }
    1.0 - (-dt_ms.max(0.0) / tau_ms).exp()
}

/// Attack/decay smoothing of `v` against `prev[b]` using time constants rather than per-frame
/// fractions. Rises fast, falls slow — the thing that makes a bar display readable.
#[inline]
fn smooth_dt(v: f32, prev: &[f32], bars: usize, b: usize, cfg: &VizCfg, dt_ms: f32) -> f32 {
    if prev.len() != bars {
        return v.clamp(0.0, 1.0);
    }
    let p = prev[b];
    let k = coef(if v > p { cfg.attack_ms } else { cfg.decay_ms }, dt_ms);
    (p + (v - p) * k).clamp(0.0, 1.0)
}

/// Map Sony's `AudioAnalyzerService::OnSpectrumUpdate` band magnitudes (an arbitrary-length
/// `vector<int>`) into `bars` normalised 0..1 levels for the visualiser — NO FFT on our side
/// (Sony's service already ran its filter bank, so this is both the cheapest path and the only
/// one: the device has no PCM tap).
///
/// The magnitudes are converted to DECIBELS before anything else touches them. Sony's analyzer
/// reports raw filter amplitudes spanning three decades within a single frame, so a linear
/// mapping — even with a sqrt for liveliness — leaves every band but the loudest one or two pinned
/// to the bottom of the display. The ear is logarithmic; so is the display now.
///
/// `prev` is the last frame (for the attack/decay pair), `peak` is the persistent auto-gain state
/// for `Scale::Dynamic`, and `dt_ms` is the real interval since the previous frame.
pub fn from_bands(
    bands: &[i32],
    bars: usize,
    prev: &[f32],
    peak: &mut f32,
    cfg: &VizCfg,
    dt_ms: f32,
) -> Vec<f32> {
    if bands.is_empty() || bars == 0 {
        return Vec::new();
    }
    let n = bands.len();
    let mut frac = vec![0.0f32; n];
    if bands.iter().any(|&x| x < 0) {
        // dBFS-style values: already logarithmic, so only the window has to be applied.
        for (b, slot) in frac.iter_mut().enumerate() {
            *slot = ((bands[b] as f32 + cfg.range_db) / cfg.range_db).clamp(0.0, 1.0);
        }
    } else {
        let frame_max = bands.iter().fold(0.0f32, |m, &x| m.max(x as f32));
        // Floor of 1.0 keeps silence flat and the division defined.
        *peak = frame_max.max(*peak * 0.95).max(1.0);
        let reference = match cfg.scale {
            Scale::Dynamic => *peak,
            Scale::Fixed => FIXED_REF,
        };
        for (b, slot) in frac.iter_mut().enumerate() {
            *slot = to_frac(bands[b] as f32, reference, cfg.range_db);
        }
    }
    // Interpolate in the DISPLAY domain, not the magnitude domain: halfway between a loud band and
    // a quiet one should look halfway, and in raw amplitudes it does not.
    let raw = resample(&frac, bars, cfg.interp);
    let mut out = vec![0.0f32; bars];
    for (b, slot) in out.iter_mut().enumerate() {
        *slot = smooth_dt(raw[b], prev, bars, b, cfg, dt_ms);
    }
    out
}

/// Advance the peak-hold markers against a fresh frame.
///
/// A marker jumps instantly to a bar that exceeds it, sits there for `peak_hold_ms`, then falls at
/// `peak_fall_per_s`. Markers are what let you see a transient that the bar itself has already
/// smoothed away — with a 320 ms decay the bar is a moving average, and the peak is the fact.
///
/// `held_ms` is per-bar state: how long each marker has been sitting at its current value.
pub fn hold_peaks(
    peaks: &mut Vec<f32>,
    held_ms: &mut Vec<f32>,
    levels: &[f32],
    dt_ms: f32,
    cfg: &VizCfg,
) {
    if cfg.peak_hold_ms <= 0.0 || levels.is_empty() {
        peaks.clear();
        held_ms.clear();
        return;
    }
    if peaks.len() != levels.len() {
        *peaks = levels.to_vec();
        *held_ms = vec![0.0; levels.len()];
        return;
    }
    if held_ms.len() != levels.len() {
        *held_ms = vec![0.0; levels.len()];
    }
    for b in 0..levels.len() {
        if levels[b] >= peaks[b] {
            peaks[b] = levels[b];
            held_ms[b] = 0.0;
        } else {
            held_ms[b] += dt_ms;
            if held_ms[b] > cfg.peak_hold_ms {
                peaks[b] = (peaks[b] - cfg.peak_fall_per_s * dt_ms / 1000.0).max(levels[b]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Defaults, with the smoothing turned OFF: every test below is about the MAPPING, and a time
    /// constant applied to a single frame would mean asserting on a value that is deliberately
    /// part-way to the answer. `dt` far larger than the constants makes the first frame land.
    fn cfg() -> VizCfg {
        VizCfg::default()
    }
    const DT: f32 = 5000.0;

    #[test]
    fn fft_dc() {
        // constant signal -> all energy in bin 0
        let mut re = vec![1.0f32; 8];
        let mut im = vec![0.0f32; 8];
        fft(&mut re, &mut im);
        assert!((re[0] - 8.0).abs() < 1e-3);
        for k in 1..8 {
            assert!(re[k].abs() < 1e-3 && im[k].abs() < 1e-3);
        }
    }

    #[test]
    fn pure_tone_lands_in_a_band() {
        // a 1 kHz-ish sine (relative): energy should be concentrated, not flat
        let n = 512;
        let mut pcm = vec![0i16; n];
        let freq_bin = 40.0; // cycles across the window
        for (i, s) in pcm.iter_mut().enumerate() {
            let v = (2.0 * std::f32::consts::PI * freq_bin * i as f32 / n as f32).sin();
            *s = (v * 20000.0) as i16;
        }
        let lv = levels(&pcm, 36, &[], &cfg(), DT);
        assert_eq!(lv.len(), 36);
        let max = lv.iter().cloned().fold(0.0f32, f32::max);
        let sum: f32 = lv.iter().sum();
        assert!(max > 0.0, "should have non-zero energy");
        // concentrated: the peak bar is well above the average bar
        assert!(max > sum / 36.0 * 2.0, "energy should be concentrated, not flat");
    }

    #[test]
    fn silence_is_flat_zero() {
        let lv = levels(&vec![0i16; 256], 24, &[], &cfg(), DT);
        assert_eq!(lv.len(), 24);
        assert!(lv.iter().all(|&v| v < 0.01));
    }

    #[test]
    fn empty_input_empty_output() {
        assert!(levels(&[], 36, &[], &cfg(), DT).is_empty());
    }

    #[test]
    fn from_bands_resamples_and_normalises_linear() {
        // 8 linear bands, ascending magnitude -> 4 bars, monotonic-ish, peak bar near 1.0
        let bands = [0, 100, 200, 300, 400, 500, 600, 700];
        let mut peak = 0.0;
        let lv = from_bands(&bands, 4, &[], &mut peak, &cfg(), DT);
        assert_eq!(lv.len(), 4);
        assert!(lv[3] > lv[0], "higher bands should map to higher bars");
        assert!(lv[3] <= 1.0 && lv[0] >= 0.0);
        assert!(peak >= 650.0, "peak auto-gain should track the resampled frame max");
    }

    #[test]
    fn from_bands_db_scale_maps_floor_to_zero() {
        // dBFS-style: -60 dB -> 0, 0 dB -> 1, -30 dB -> ~0.5
        let bands = [-60, -30, 0];
        let mut peak = 0.0;
        let lv = from_bands(&bands, 3, &[], &mut peak, &cfg(), DT);
        assert_eq!(lv.len(), 3);
        assert!(lv[0] < 0.05, "floor maps to ~0");
        assert!(lv[2] > 0.95, "0 dB maps to ~1");
        assert!((lv[1] - 0.5).abs() < 0.1, "-30 dB maps to ~mid");
    }

    /// Twelve source bands into 36 bars must produce a smooth curve, not 12 groups of three
    /// identical values. A staircase claims a resolution the analyzer does not have.
    #[test]
    fn upsampling_interpolates_instead_of_stepping() {
        // Monotonically rising source: the output must rise at nearly every step.
        let bands: Vec<i32> = (0..12).map(|i| 1000 + i * 1000).collect();
        let mut peak = 0.0;
        let lv = from_bands(&bands, 36, &[], &mut peak, &cfg(), DT);
        assert_eq!(lv.len(), 36);
        let mut equal_runs = 0;
        for w in lv.windows(2) {
            if (w[0] - w[1]).abs() < 1e-6 {
                equal_runs += 1;
            }
        }
        assert!(equal_runs <= 2, "output is a staircase, not a curve ({equal_runs} flat steps)");
        assert!(lv[35] > lv[0], "rising input must give a rising output");
    }

    /// A frame spanning three decades — which is what Sony's analyzer actually reports — must not
    /// leave every quiet band pinned at the bottom of the display.
    #[test]
    fn a_three_decade_frame_uses_the_whole_display() {
        let bands = [40_000, 100_000, 400_000, 1_000_000, 4_000_000, 8_000_000];
        let mut peak = 0.0;
        let lv = from_bands(&bands, 6, &[], &mut peak, &cfg(), DT);
        // The quietest band is ~-46 dB below the peak: visible, near the floor, but not zero.
        assert!(lv[0] > 0.01, "quietest band vanished: {lv:?}");
        assert!(lv[0] < 0.25, "quietest band should still read as quiet: {lv:?}");
        assert!(lv[5] > 0.95, "loudest band should be near full: {lv:?}");
        // And the middle of the range should land in the middle of the display, which is the whole
        // point of a log mapping — under the old linear/sqrt form it sat around a fifth.
        assert!(lv[3] > 0.55 && lv[3] < 0.95, "mid-range band mapped to {:.2}", lv[3]);
    }

    /// All-zero bands: log(0) is -inf, and a NaN reaching the renderer would draw garbage or panic.
    #[test]
    fn silence_maps_to_zero_not_nan() {
        let mut peak = 0.0;
        let lv = from_bands(&[0, 0, 0, 0], 4, &[], &mut peak, &cfg(), DT);
        assert!(lv.iter().all(|v| v.is_finite() && *v == 0.0), "{lv:?}");
    }

    #[test]
    fn from_bands_empty_is_empty() {
        let mut peak = 0.0;
        assert!(from_bands(&[], 36, &[], &mut peak, &cfg(), DT).is_empty());
    }
}
