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
    let n = bands.len();
    let mut raw = vec![0.0f32; bars];
    if n >= bars {
        // Downsampling: average each output bucket's source span.
        for (b, slot) in raw.iter_mut().enumerate() {
            let lo = b * n / bars;
            let hi = (((b + 1) * n / bars).max(lo + 1)).min(n);
            let mut s = 0.0f32;
            for &x in &bands[lo..hi] {
                s += x as f32;
            }
            *slot = s / (hi - lo) as f32;
        }
    } else {
        // UPSAMPLING — interpolate, don't block-average. Sony's analyzer caps at TWELVE passbands
        // (wampy's MAKING_OF_VIS, confirmed against Sony's own client library), and the display asks
        // for 36 columns. Bucket-averaging 12 into 36 gives three IDENTICAL bars per band: a
        // staircase of 12 wide steps pretending to be 36 bars, which claims a frequency resolution
        // the data does not have. Interpolating between band centres is honest about the same data
        // and reads correctly in every style — especially Ribbon and Line, which are contours.
        for (b, slot) in raw.iter_mut().enumerate() {
            let t = if bars > 1 { b as f32 * (n - 1) as f32 / (bars - 1) as f32 } else { 0.0 };
            let i0 = (t.floor() as usize).min(n - 1);
            let i1 = (i0 + 1).min(n - 1);
            let f = t - i0 as f32;
            *slot = bands[i0] as f32 * (1.0 - f) + bands[i1] as f32 * f;
        }
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
        // Linear amplitudes -> DECIBELS against a slow-decaying peak.
        //
        // This was `raw / peak` with a `sqrt` for liveliness, and that is wrong for this data.
        // Sony's analyzer reports raw amplitudes "ranging from 40k to millions" (wampy, from
        // intercepting the stock player), i.e. a dynamic range of 100:1 to 1000:1 within one frame.
        // Divided linearly by the peak, everything but the loudest band or two sits in the bottom
        // tenth of the display — a `sqrt` softens that but does not fix it, because the ear is
        // logarithmic and the data spans three decades. Hence a real dB mapping: the quiet bands
        // get the room they actually occupy perceptually. Sony's own player converts these to sound
        // pressure levels for the same reason.
        const FLOOR_DB: f32 = -48.0;
        let frame_max = raw.iter().cloned().fold(0.0f32, f32::max);
        // Floor of 1.0 keeps silence flat and the division defined.
        *peak = frame_max.max(*peak * 0.95).max(1.0);
        for (b, slot) in out.iter_mut().enumerate() {
            let v = if raw[b] <= 0.0 {
                0.0 // log of zero is -inf; silence is the floor, not a NaN
            } else {
                let db = 20.0 * (raw[b] / *peak).log10();
                ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
            };
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

    /// Twelve source bands into 36 bars must produce a smooth curve, not 12 groups of three
    /// identical values. A staircase claims a resolution the analyzer does not have.
    #[test]
    fn upsampling_interpolates_instead_of_stepping() {
        // Monotonically rising source: the output must rise at nearly every step.
        let bands: Vec<i32> = (0..12).map(|i| 1000 + i * 1000).collect();
        let mut peak = 0.0;
        let lv = from_bands(&bands, 36, &[], &mut peak);
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
        let lv = from_bands(&bands, 6, &[], &mut peak);
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
        let lv = from_bands(&[0, 0, 0, 0], 4, &[], &mut peak);
        assert!(lv.iter().all(|v| v.is_finite() && *v == 0.0), "{lv:?}");
    }

    #[test]
    fn from_bands_empty_is_empty() {
        let mut peak = 0.0;
        assert!(from_bands(&[], 36, &[], &mut peak).is_empty());
    }
}
