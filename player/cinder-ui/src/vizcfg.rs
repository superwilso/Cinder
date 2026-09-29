//! Visualiser signal settings — the knobs a desktop spectrum analyser exposes, applied to the one
//! data source this device actually has.
//!
//! Sony's AudioAnalyzerService is a bank of 2nd-order IIR bandpass filters (one per passband) each
//! feeding a level detector; it is NOT an FFT, and the service allocates its detectors once, from
//! a hardcoded twelve-entry list, in its constructor. Twelve bands is therefore a ceiling we do not
//! get to raise from a client, and there is no PCM tap on the device to run our own transform over
//! (the effect shim sets parameters; it never sees audio). What we *do* own is every band's centre
//! frequency and Q, the detector window, the emit rate, and all of the display mapping — which is
//! where the rest of this module lives.
//!
//! The defaults here are the ones that make twelve filters behave like a twelve-band analyser
//! rather than twelve tone detectors: see `analyzer_shim.cpp` for the Q derivation.

/// How raw band magnitudes are mapped onto the 0..1 the bars are drawn from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    /// Auto-gain: the top of the display tracks a slow-decaying peak, so a quiet passage still
    /// fills the bars. What the visualiser has always done, and the right default for "is it
    /// moving with the music" — but it lies about absolute level, and between tracks the whole
    /// display breathes.
    Dynamic,
    /// Fixed reference: the top of the display is a constant magnitude, so a quiet track LOOKS
    /// quiet and two tracks are comparable. Needs `FULL_SCALE` to be right for Sony's units.
    Fixed,
}

impl Scale {
    pub const COUNT: u8 = 2;
    pub fn from_index(i: u8) -> Scale {
        if i % Self::COUNT == 0 {
            Scale::Dynamic
        } else {
            Scale::Fixed
        }
    }
    pub fn index(self) -> u8 {
        match self {
            Scale::Dynamic => 0,
            Scale::Fixed => 1,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Scale::Dynamic => "DYNAMIC",
            Scale::Fixed => "FIXED",
        }
    }
}

/// How the twelve source bands are stretched across the display's columns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Interp {
    /// Straight lines between band centres. Honest and cheap; visibly faceted on the contour
    /// styles, because twelve points across 432 px is one corner every 36 px.
    Linear,
    /// Catmull-Rom through the same points: same data, continuous first derivative, so Ribbon and
    /// Line read as curves instead of a polyline. Clamped to the source range so the spline cannot
    /// overshoot into a level no band reported.
    Smooth,
}

impl Interp {
    pub const COUNT: u8 = 2;
    pub fn from_index(i: u8) -> Interp {
        if i % Self::COUNT == 0 {
            Interp::Smooth
        } else {
            Interp::Linear
        }
    }
    pub fn index(self) -> u8 {
        match self {
            Interp::Smooth => 0,
            Interp::Linear => 1,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Interp::Smooth => "SMOOTH",
            Interp::Linear => "LINEAR",
        }
    }
}

/// The full signal-side configuration. Display-only concerns (which style, how tall) stay in
/// `viz.rs`; this is everything that changes the NUMBERS.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VizCfg {
    pub scale: Scale,
    /// Display window in dB: the level that maps to 0. 60 dB is a good compromise between "the
    /// quiet bands are visible" and "the noise floor does not dance".
    pub range_db: f32,
    /// Rise time constant (ms). Small = twitchy transients, large = smeared.
    pub attack_ms: f32,
    /// Fall time constant (ms). Traditionally much slower than the attack, which is what makes a
    /// bar display readable rather than a flicker.
    pub decay_ms: f32,
    /// Peak-hold markers: how long a peak is held before it starts to fall, in ms. 0 = no markers.
    pub peak_hold_ms: f32,
    /// How fast a held peak falls once it lets go, in display-fractions per second.
    pub peak_fall_per_s: f32,
    pub interp: Interp,
}

impl Default for VizCfg {
    fn default() -> Self {
        VizCfg {
            scale: Scale::Dynamic,
            range_db: 60.0,
            // 40/320 ms is the classic bar-meter pairing: fast enough to catch a kick drum, slow
            // enough that the bar does not strobe. Both are TIME constants, applied against the
            // real frame interval — so changing the analyzer's emit rate no longer changes the
            // feel of the display, which it silently did when the smoothing was per-frame.
            attack_ms: 40.0,
            decay_ms: 320.0,
            peak_hold_ms: 0.0,
            peak_fall_per_s: 0.9,
            interp: Interp::Smooth,
        }
    }
}

/// Presets for the response pair, since two independent millisecond fields is not a thing you can
/// offer on a 4-inch touch screen. Named for what they feel like.
pub const RESPONSE_COUNT: u8 = 4;

pub fn response_from_index(i: u8) -> (f32, f32, &'static str) {
    match i % RESPONSE_COUNT {
        0 => (40.0, 320.0, "NORMAL"),
        1 => (10.0, 120.0, "FAST"),
        2 => (90.0, 700.0, "SMOOTH"),
        _ => (5.0, 60.0, "RAW"),
    }
}

/// dB-range presets, in the order the settings row cycles them.
pub const RANGE_COUNT: u8 = 4;

pub fn range_from_index(i: u8) -> f32 {
    match i % RANGE_COUNT {
        0 => 60.0,
        1 => 48.0,
        2 => 36.0,
        _ => 72.0,
    }
}

pub fn range_index_of(db: f32) -> u8 {
    for i in 0..RANGE_COUNT {
        if (range_from_index(i) - db).abs() < 0.5 {
            return i;
        }
    }
    0
}

/// Detector-window presets (SetCalcSamples), in milliseconds — the analyzer's averaging time, the
/// same knob a desktop analyser calls "time window". Converted to samples by the shell, which is
/// the only side that knows the stream's sample rate.
pub const WINDOW_COUNT: u8 = 4;

pub fn window_ms_from_index(i: u8) -> u16 {
    match i % WINDOW_COUNT {
        0 => 0, // 0 = leave the service's own default alone
        1 => 25,
        2 => 60,
        _ => 125,
    }
}

pub fn window_name(i: u8) -> &'static str {
    match i % WINDOW_COUNT {
        0 => "AUTO",
        1 => "25 MS",
        2 => "60 MS",
        _ => "125 MS",
    }
}

/// Emit-rate presets (SetUpdateRate), in Hz. The panel is repainted per frame, so this is also the
/// visualiser's share of the render budget — 20 Hz is what the shell asked for before any of this
/// was configurable.
pub const RATE_COUNT: u8 = 3;

pub fn rate_from_index(i: u8) -> u8 {
    match i % RATE_COUNT {
        0 => 20,
        1 => 30,
        _ => 45,
    }
}
