//! Tiny dependency-free FFT + spectrum extraction, to turn a PCM window into per-bar levels for
//! a REAL audio-reactive visualiser. (No rustfft crate — keep the ARM/glibc-2.23 cross-build
//! clean.) The shell feeds PCM (from Sony's AudioAnalyzerService PcmReader); we window + FFT it,
//! group the magnitude bins into log-spaced bands, and normalise to 0..1 per bar. Cheap: a
//! 512-point FFT a few times a second.

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
/// empty output. Uses a Hann window, log-spaced frequency bands, and a sqrt curve for a livelier
/// display. `prev` (the last frame's levels) is used for attack/decay smoothing if same length.
pub fn levels(pcm: &[i16], bars: usize, prev: &[f32]) -> Vec<f32> {
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
        // sqrt compress + scale; clamp. (Empirical gain; calibrate against real output on device.)
        let mut v = (mag * 0.5).sqrt().clamp(0.0, 1.0);
        // attack/decay smoothing against the previous frame
        if prev.len() == bars {
            let p = prev[b];
            v = if v > p { p + (v - p) * 0.6 } else { p + (v - p) * 0.3 };
        }
        *slot = v.clamp(0.0, 1.0);
    }
    out
}

fn band_edge(b: usize, bars: usize, half: usize) -> usize {
    // log scale from bin 1 to `half`
    let t = b as f32 / bars as f32;
    let e = (1.0f32).max((half as f32).powf(t));
    (e as usize).clamp(1, half)
}

/// Attack/decay smoothing of a fresh value `v` against the previous frame `prev[b]` (matching
/// `levels`): rise fast (0.6), fall slow (0.3) for a livelier-but-stable bar.
fn smooth(v: f32, prev: &[f32], bars: usize, b: usize) -> f32 {
    if prev.len() == bars {
        let p = prev[b];
        (if v > p { p + (v - p) * 0.6 } else { p + (v - p) * 0.3 }).clamp(0.0, 1.0)
    } else {
        v.clamp(0.0, 1.0)
    }
}

/// Map Sony's `AudioAnalyzerService::OnSpectrumUpdate` band magnitudes (an arbitrary-length
/// `vector<int>`) into `bars` normalised 0..1 levels for the visualiser — NO FFT (Sony already did
/// it, so this path is the cheapest and most accurate on device). We resample the source bands to
/// our bar count and normalise. Sony's exact units are calibrated on-device (the probe dumps raw
/// values); we auto-detect the scale so any reasonable encoding works:
///   * any-negative values  -> treated as dBFS-style, mapped from [FLOOR_DB, 0] dB into [0,1];
///   * all-non-negative     -> linear magnitude, normalised against a slow-decaying peak so the
///                             display auto-scales to the material (sqrt curve for liveliness).
/// `prev` (last frame) drives the same attack/decay smoothing as `levels`; `peak` is persistent
/// auto-gain state for the linear branch (pass &mut Render.viz_peak).
pub fn from_bands(bands: &[i32], bars: usize, prev: &[f32], peak: &mut f32) -> Vec<f32> {
    if bands.is_empty() || bars == 0 {
        return Vec::new();
    }
    // resample source bands -> `bars` by averaging each output bucket's source span
    let mut raw = vec![0.0f32; bars];
    for (b, slot) in raw.iter_mut().enumerate() {
        let lo = b * bands.len() / bars;
        let hi = (((b + 1) * bands.len() / bars).max(lo + 1)).min(bands.len());
        let mut s = 0.0f32;
        for &x in &bands[lo..hi] {
            s += x as f32;
        }
        *slot = s / (hi - lo) as f32;
    }
    let mut out = vec![0.0f32; bars];
    if bands.iter().any(|&x| x < 0) {
        // dBFS-style: map [FLOOR_DB, 0] -> [0,1]. FLOOR_DB calibrated on device (start at -60).
        const FLOOR_DB: f32 = -60.0;
        for (b, slot) in out.iter_mut().enumerate() {
            let v = ((raw[b] - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
            *slot = smooth(v, prev, bars, b);
        }
    } else {
        // linear magnitude: auto-gain against a slow-decaying peak (floor 1.0 avoids div-by-0 and
        // keeps silence flat). sqrt for a livelier display, same as the FFT path.
        let frame_max = raw.iter().cloned().fold(0.0f32, f32::max);
        *peak = frame_max.max(*peak * 0.95).max(1.0);
        for (b, slot) in out.iter_mut().enumerate() {
            let v = (raw[b] / *peak).clamp(0.0, 1.0).sqrt();
            *slot = smooth(v, prev, bars, b);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let lv = levels(&pcm, 36, &[]);
        assert_eq!(lv.len(), 36);
        let max = lv.iter().cloned().fold(0.0f32, f32::max);
        let sum: f32 = lv.iter().sum();
        assert!(max > 0.0, "should have non-zero energy");
        // concentrated: the peak bar is well above the average bar
        assert!(max > sum / 36.0 * 2.0, "energy should be concentrated, not flat");
    }

    #[test]
    fn silence_is_flat_zero() {
        let lv = levels(&vec![0i16; 256], 24, &[]);
        assert_eq!(lv.len(), 24);
        assert!(lv.iter().all(|&v| v < 0.01));
    }

    #[test]
    fn empty_input_empty_output() {
        assert!(levels(&[], 36, &[]).is_empty());
    }

    #[test]
    fn from_bands_resamples_and_normalises_linear() {
        // 8 linear bands, ascending magnitude -> 4 bars, monotonic-ish, peak bar near 1.0
        let bands = [0, 100, 200, 300, 400, 500, 600, 700];
        let mut peak = 0.0;
        let lv = from_bands(&bands, 4, &[], &mut peak);
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
        let lv = from_bands(&bands, 3, &[], &mut peak);
        assert_eq!(lv.len(), 3);
        assert!(lv[0] < 0.05, "floor maps to ~0");
        assert!(lv[2] > 0.95, "0 dB maps to ~1");
        assert!((lv[1] - 0.5).abs() < 0.1, "-30 dB maps to ~mid");
    }

    #[test]
    fn from_bands_empty_is_empty() {
        let mut peak = 0.0;
        assert!(from_bands(&[], 36, &[], &mut peak).is_empty());
    }
}
