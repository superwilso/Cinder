//! Headphone EQ (Sound ▸ Advanced): a parametric EQ read from a text file, in the format AutoEQ
//! publishes for thousands of headphones (`<model> ParametricEQ.txt`) and Equalizer APO reads:
//!
//! ```text
//! Preamp: -6.4 dB
//! Filter 1: ON LSC Fc 105 Hz Gain 6.6 dB Q 0.70
//! Filter 2: ON PK Fc 2734 Hz Gain 4.3 dB Q 1.94
//! ```
//!
//! The files go in `cinder_eq` on the player's drive. This module reads one and turns it into the
//! control file libcinder_mono.so plays (`cinder-home/src/hp_eq.h`): one line per sample rate, the
//! rate and then five coefficients per filter. The filters are designed here, not in the sound
//! service, because the library there links no maths library and a mistake here is a unit test,
//! not a howl.
//!
//! LEVEL. A boost is paid for by a cut in level: the file's own preamp, or more if the filters
//! would still rise above 0 dB, so the EQ never clips what it was given.

use std::f64::consts::PI;

/// The folder on the drive the profiles are read from.
pub const DIR: &str = "cinder_eq";
/// The most filters a profile may have. AutoEQ's files have ten.
pub const MAX_FILTERS: usize = 16;
/// The rates the control file carries. A stream at any other rate plays without the EQ.
pub const RATES: [u32; 9] = [
    32000, 44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub kind: Kind,
    pub hz: f64,
    pub db: f64,
    pub q: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub preamp_db: f64,
    pub filters: Vec<Filter>,
}

/// The number after `key` in `toks` (`Fc 105 Hz` → 105).
fn after(toks: &[&str], key: &str) -> Option<f64> {
    let i = toks.iter().position(|t| t.eq_ignore_ascii_case(key))?;
    toks.get(i + 1)?.parse().ok()
}

/// Read a profile. An error names the line, in words a person can act on; a file that is half
/// understood is refused whole rather than played half right.
pub fn parse(text: &str) -> Result<Profile, String> {
    let mut p = Profile::default();
    for (n, raw) in text.lines().enumerate() {
        let n = n + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((head, rest)) = line.split_once(':') else {
            return Err(format!("line {n} is not a filter"));
        };
        let toks: Vec<&str> = rest.split_whitespace().collect();
        let head = head.trim().to_ascii_lowercase();
        if head == "preamp" {
            p.preamp_db = toks
                .first()
                .and_then(|t| t.parse().ok())
                .filter(|v: &f64| v.abs() <= 30.0)
                .ok_or(format!("line {n}: preamp not understood"))?;
            continue;
        }
        if !head.starts_with("filter") {
            return Err(format!("line {n}: \"{head}\" is not a parametric EQ line"));
        }
        match toks.first().map(|t| t.to_ascii_uppercase()) {
            Some(s) if s == "OFF" => continue,
            Some(s) if s == "ON" => {}
            _ => return Err(format!("line {n}: expected ON or OFF")),
        }
        let kind = match toks.get(1).map(|t| t.to_ascii_uppercase()).as_deref() {
            Some("PK" | "PEQ") => Kind::Peak,
            Some("LS" | "LSC") => Kind::LowShelf,
            Some("HS" | "HSC") => Kind::HighShelf,
            Some(t) => return Err(format!("line {n}: filter type {t} is not supported")),
            None => return Err(format!("line {n}: no filter type")),
        };
        let hz = after(&toks, "Fc").ok_or(format!("line {n}: no Fc"))?;
        let db = after(&toks, "Gain").ok_or(format!("line {n}: no Gain"))?;
        let q = after(&toks, "Q").unwrap_or(std::f64::consts::FRAC_1_SQRT_2);
        if !(10.0..=30000.0).contains(&hz) || db.abs() > 30.0 || !(0.05..=30.0).contains(&q) {
            return Err(format!("line {n}: values out of range"));
        }
        if p.filters.len() == MAX_FILTERS {
            return Err(format!("more than {MAX_FILTERS} filters"));
        }
        p.filters.push(Filter { kind, hz, db, q });
    }
    if p.filters.is_empty() {
        return Err("no filters in the file".into());
    }
    Ok(p)
}

/// One filter as {b0 b1 b2 a1 a2}, normalised, with a1/a2 NEGATED (y = b0x + b1x1 + b2x2 + a1y1 +
/// a2y2), the convention the codec EQ uses too. RBJ Audio EQ Cookbook.
pub fn biquad(f: &Filter, fs: f64) -> [f64; 5] {
    let a = 10f64.powf(f.db / 40.0);
    let w0 = 2.0 * PI * f.hz / fs;
    let (sw, cw) = w0.sin_cos();
    let alpha = sw / (2.0 * f.q);
    let sa = 2.0 * a.sqrt() * alpha;
    let (b0, b1, b2, a0, a1, a2) = match f.kind {
        Kind::Peak => (
            1.0 + alpha * a,
            -2.0 * cw,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cw,
            1.0 - alpha / a,
        ),
        Kind::LowShelf => (
            a * ((a + 1.0) - (a - 1.0) * cw + sa),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
            a * ((a + 1.0) - (a - 1.0) * cw - sa),
            (a + 1.0) + (a - 1.0) * cw + sa,
            -2.0 * ((a - 1.0) + (a + 1.0) * cw),
            (a + 1.0) + (a - 1.0) * cw - sa,
        ),
        Kind::HighShelf => (
            a * ((a + 1.0) + (a - 1.0) * cw + sa),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
            a * ((a + 1.0) + (a - 1.0) * cw - sa),
            (a + 1.0) - (a - 1.0) * cw + sa,
            2.0 * ((a - 1.0) - (a + 1.0) * cw),
            (a + 1.0) - (a - 1.0) * cw - sa,
        ),
    };
    [b0 / a0, b1 / a0, b2 / a0, -a1 / a0, -a2 / a0]
}

/// |H| in dB of a cascade at `hz`.
pub fn response_db(c: &[[f64; 5]], fs: f64, hz: f64) -> f64 {
    let w = 2.0 * PI * hz / fs;
    let (s1, c1) = (-w.sin(), w.cos());
    let (s2, c2) = (-(2.0 * w).sin(), (2.0 * w).cos());
    c.iter()
        .map(|k| {
            let (nr, ni) = (k[0] + k[1] * c1 + k[2] * c2, k[1] * s1 + k[2] * s2);
            let (dr, di) = (1.0 - k[3] * c1 - k[4] * c2, -k[3] * s1 - k[4] * s2);
            10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
        })
        .sum()
}

/// The profile at one rate: its filters (any too close to half the rate are left out), with the
/// level folded into the first. Empty when no filter fits.
pub fn design(p: &Profile, fs: f64) -> Vec<[f64; 5]> {
    let mut c: Vec<[f64; 5]> = p
        .filters
        .iter()
        .filter(|f| f.hz < 0.45 * fs)
        .map(|f| biquad(f, fs))
        .collect();
    if c.is_empty() {
        return c;
    }
    // The loudest point, 20 Hz to 20 kHz (or just under half the rate), 1/24 octave.
    let top = (0.45 * fs).min(20000.0);
    let mut peak = f64::MIN;
    let mut hz = 20.0;
    while hz <= top {
        peak = peak.max(response_db(&c, fs, hz));
        hz *= 2f64.powf(1.0 / 24.0);
    }
    let level = p.preamp_db.min(-peak);
    let s = 10f64.powf(level / 20.0);
    for k in &mut c[0][..3] {
        *k *= s;
    }
    c
}

/// The control file libcinder_mono.so reads: per rate, `rate` and then b0 b1 b2 a1 a2 per filter.
pub fn control(p: &Profile) -> String {
    let mut out = String::new();
    for rate in RATES {
        let c = design(p, rate as f64);
        if c.is_empty() {
            continue;
        }
        out.push_str(&rate.to_string());
        for k in c.iter().flatten() {
            out.push(' ');
            out.push_str(&format!("{k:e}"));
        }
        out.push('\n');
    }
    out
}

/// The profile names in `names` (file names in the folder): the `.txt` files, without the
/// extension and AutoEQ's " ParametricEQ" suffix, sorted case-blind.
pub fn profile_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = names
        .into_iter()
        .filter_map(|n| {
            let stem = n.strip_suffix(".txt")?;
            (!stem.starts_with('.')).then(|| stem.to_string())
        })
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

/// The name shown for a profile file stem.
pub fn label(stem: &str) -> &str {
    stem.strip_suffix(" ParametricEQ").unwrap_or(stem)
}

/// The one after `cur` in `names`, then off (`None`) after the last; from off, the first.
pub fn next<'a>(names: &'a [String], cur: &str) -> Option<&'a String> {
    match names.iter().position(|n| n == cur) {
        Some(i) => names.get(i + 1),
        None => names.first(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // From AutoEQ's results for a well-known open-back (oratory1990 measurement), first lines.
    /// A profile is any file someone copied to the drive. Seeded mutation of a real one: parse
    /// never panics, and whatever it accepts designs finite filters at every rate the shim plays.
    /// `CINDER_FUZZ_N=200000 cargo test --release` for a long run.
    #[test]
    fn mangled_profiles_never_panic_or_design_a_non_finite_filter() {
        let n: usize = std::env::var("CINDER_FUZZ_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let bits = [
            "-",
            ".",
            "e",
            "9",
            "0",
            "1e308",
            "NaN",
            "inf",
            " ",
            "\n",
            "\r\n",
            "Hz",
            "dB",
            "Q",
            "Fc",
            "Gain",
            "ON",
            "OFF",
            "PK",
            "LSC",
            "HSC",
            "LS",
            "HS",
            "Filter 17:",
            "Preamp:",
            "é",
            "𝄞",
        ];
        let mut r: u64 = 0x2545_f491_4f6c_dd1d;
        let mut rnd = |m: usize| {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            (r % m.max(1) as u64) as usize
        };
        for _ in 0..n {
            let mut v: Vec<char> = SAMPLE.chars().collect();
            for _ in 0..1 + rnd(6) {
                let at = rnd(v.len() + 1);
                if rnd(3) == 0 && at < v.len() {
                    let end = (at + 1 + rnd(12)).min(v.len());
                    v.drain(at..end);
                } else {
                    let ins: Vec<char> = bits[rnd(bits.len())].chars().collect();
                    v.splice(at..at, ins);
                }
            }
            let text: String = v.into_iter().collect();
            if let Ok(p) = parse(&text) {
                for fs in [32_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 384_000.0] {
                    for c in design(&p, fs) {
                        assert!(c.iter().all(|x| x.is_finite()), "{text:?} at {fs}: {c:?}");
                        // Poles inside the unit circle (stored a1, a2 are negated): an unstable
                        // filter would run to full scale in the listener's ears.
                        let (c1, c2) = (-c[3], -c[4]);
                        assert!(
                            c2.abs() < 1.0 && c1.abs() < 1.0 + c2,
                            "unstable: {text:?} at {fs}: {c:?}"
                        );
                    }
                }
                let _ = control(&p);
            }
        }
    }

    const SAMPLE: &str = "Preamp: -6.4 dB\r\n\
        Filter 1: ON LSC Fc 105 Hz Gain 6.6 dB Q 0.70\r\n\
        Filter 2: ON PK Fc 2734 Hz Gain 4.3 dB Q 1.94\r\n\
        Filter 3: OFF PK Fc 5000 Hz Gain 9 dB Q 1\r\n\
        Filter 4: ON HSC Fc 10000 Hz Gain -2.1 dB Q 0.70\r\n";

    #[test]
    fn reads_autoeq_files() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.preamp_db, -6.4);
        assert_eq!(p.filters.len(), 3, "the OFF filter is skipped");
        assert_eq!(p.filters[0].kind, Kind::LowShelf);
        assert_eq!(
            (p.filters[1].hz, p.filters[1].db, p.filters[1].q),
            (2734.0, 4.3, 1.94)
        );
        assert_eq!(p.filters[2].kind, Kind::HighShelf);
    }

    #[test]
    fn refuses_what_it_cannot_play_and_says_where() {
        assert!(parse("GraphicEQ: 20 -1; 21 -1")
            .unwrap_err()
            .contains("not a parametric EQ line"));
        assert_eq!(
            parse("Filter 1: ON LP Fc 100 Hz").unwrap_err(),
            "line 1: filter type LP is not supported"
        );
        assert_eq!(
            parse("Preamp: -3 dB\nFilter 1: ON PK Fc 100 Hz Gain 60 dB Q 1").unwrap_err(),
            "line 2: values out of range"
        );
        assert!(parse("Preamp: -3 dB").unwrap_err().contains("no filters"));
        let many: String = (0..17)
            .map(|i| format!("Filter {i}: ON PK Fc 100 Hz Gain 1 dB Q 1\n"))
            .collect();
        assert!(parse(&many).unwrap_err().contains("more than 16"));
    }

    #[test]
    fn the_response_is_the_profile_and_never_above_0_db() {
        let p = parse(SAMPLE).unwrap();
        let fs = 44100.0;
        let c = design(&p, fs);
        assert_eq!(c.len(), 3);
        // The first filter carries the level: the shelf's +6.6 dB less the 6.4 dB preamp, or more.
        let low = response_db(&c, fs, 30.0);
        let mid = response_db(&c, fs, 1000.0);
        assert!((low - mid - 6.4).abs() < 0.5, "shelf {low} vs mid {mid}");
        let mut hz = 20.0;
        while hz < 20000.0 {
            assert!(response_db(&c, fs, hz) <= 0.01, "{hz} Hz above 0 dB");
            hz *= 1.05;
        }
        // A boost with too little preamp gets more cut, not a clip.
        let hot = parse("Preamp: 0 dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 1").unwrap();
        let c = design(&hot, fs);
        assert!(response_db(&c, fs, 1000.0).abs() < 0.05);
        assert!((response_db(&c, fs, 100.0) + 6.0).abs() < 0.3);
    }

    #[test]
    fn every_rate_matches_and_the_file_reads_back() {
        let p = parse(SAMPLE).unwrap();
        let at = |fs: f64, hz: f64| response_db(&design(&p, fs), fs, hz);
        for fs in [48000.0, 96000.0, 192000.0] {
            for hz in [50.0, 1000.0, 2734.0, 12000.0] {
                assert!((at(fs, hz) - at(44100.0, hz)).abs() < 0.3, "{fs} at {hz}");
            }
        }
        let text = control(&p);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), RATES.len());
        let nums: Vec<f64> = lines[1].split(' ').map(|t| t.parse().unwrap()).collect();
        assert_eq!(nums[0], 44100.0);
        assert_eq!(nums.len(), 1 + 3 * 5);
        let back: Vec<[f64; 5]> = nums[1..]
            .chunks(5)
            .map(|k| [k[0], k[1], k[2], k[3], k[4]])
            .collect();
        assert_eq!(back, design(&p, 44100.0));
        // A filter above 0.45 of the rate is left out at that rate only.
        let high = parse("Filter 1: ON PK Fc 16000 Hz Gain -3 dB Q 1").unwrap();
        assert!(design(&high, 32000.0).is_empty());
        assert_eq!(design(&high, 44100.0).len(), 1);
    }

    /// The file the sound service's self-test (cinder-home/tools/hpeq_selftest.cpp) plays: the two
    /// halves agree on the format because both test against these bytes. Rewrite it with
    /// `CINDER_HPEQ_FIXTURE=1 cargo test -p cinder-ui hp_eq`.
    #[test]
    fn the_sound_service_fixture_is_what_this_writes() {
        let p = parse(
            "Preamp: -6 dB\nFilter 1: ON PK Fc 1000 Hz Gain 6 dB Q 1\n\
             Filter 2: ON LSC Fc 100 Hz Gain -6 dB Q 0.7",
        )
        .unwrap();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../cinder-home/tools/hpeq_fixture.txt"
        );
        if std::env::var_os("CINDER_HPEQ_FIXTURE").is_some() {
            std::fs::write(path, control(&p)).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), control(&p));
    }

    #[test]
    fn names_and_cycling() {
        let names = profile_names(
            ["b.txt", "A ParametricEQ.txt", "notes.md", ".hidden.txt"].map(String::from),
        );
        assert_eq!(names, ["A ParametricEQ", "b"]);
        assert_eq!(label(&names[0]), "A");
        assert_eq!(next(&names, ""), Some(&names[0]));
        assert_eq!(next(&names, "A ParametricEQ"), Some(&names[1]));
        assert_eq!(next(&names, "b"), None, "after the last: off");
        assert_eq!(next(&names, "gone"), Some(&names[0]));
        assert_eq!(next(&[], ""), None);
    }
}
