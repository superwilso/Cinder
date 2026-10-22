//! Pure ZAPPIN preview timing and finite-session state.
//!
//! Times are milliseconds. This module only plans short previews; it does not claim that an
//! upstream SABI value is a detected chorus, nor does it own playback or persistence.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartSource {
    Sabi,
    FixedFallback,
    UnknownDurationFromStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewPlan {
    pub start_ms: u64,
    pub length_ms: Option<u64>,
    pub source: StartSource,
}

pub const FALLBACK_START_MS: u64 = 45_000;
pub const MIN_TAIL_MS: u64 = 1_000;

/// Make a safe, bounded preview plan. `None` duration permits a seek-free attempt from zero.
pub fn plan(duration_ms: Option<u64>, sabi_ms: Option<i64>, preview_ms: u64) -> PreviewPlan {
    let Some(duration) = duration_ms.filter(|d| *d > 0) else {
        return PreviewPlan {
            start_ms: 0,
            length_ms: Some(preview_ms),
            source: StartSource::UnknownDurationFromStart,
        };
    };
    let valid_sabi = sabi_ms
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0 && *value < duration);
    let (start_ms, source) = if let Some(sabi) = valid_sabi {
        (
            sabi.min(duration.saturating_sub(MIN_TAIL_MS)),
            StartSource::Sabi,
        )
    } else {
        (
            FALLBACK_START_MS.min(duration.saturating_sub(preview_ms)),
            StartSource::FixedFallback,
        )
    };
    PreviewPlan {
        start_ms,
        length_ms: Some(preview_ms.min(duration.saturating_sub(start_ms))),
        source,
    }
}

/// An explicit finite pass through the current queue, starting at its current index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    generation: u64,
    len: usize,
    index: usize,
    preview_ms: u64,
    active: bool,
}

impl Session {
    pub fn start(generation: u64, len: usize, current: usize, preview_ms: u64) -> Option<Self> {
        (len > 0 && current < len && preview_ms > 0).then_some(Self {
            generation,
            len,
            index: current,
            preview_ms,
            active: true,
        })
    }

    pub fn generation(self) -> u64 {
        self.generation
    }
    pub fn index(self) -> usize {
        self.index
    }
    pub fn preview_ms(self) -> u64 {
        self.preview_ms
    }
    pub fn set_preview_ms(&mut self, preview_ms: u64) {
        if preview_ms > 0 {
            self.preview_ms = preview_ms;
        }
    }
    pub fn active(self) -> bool {
        self.active
    }

    /// Move one candidate in the finite sequence. No wrap; false means the endpoint was reached.
    /// Keep the session active there so the caller can distinguish natural completion from a
    /// manual Previous at the first item and explicitly finish or keep the current preview.
    pub fn advance(&mut self, forward: bool) -> bool {
        if !self.active {
            return false;
        }
        let next = if forward {
            self.index.checked_add(1).filter(|i| *i < self.len)
        } else {
            self.index.checked_sub(1)
        };
        match next {
            Some(index) => {
                self.index = index;
                true
            }
            None => false,
        }
    }

    /// A callback may mutate playback only for the active session generation.
    pub fn accepts(self, generation: u64, index: usize) -> bool {
        self.active && self.generation == generation && self.index == index
    }

    pub fn cancel(&mut self) {
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sabi_is_preferred_and_late_sabi_keeps_one_second_tail() {
        assert_eq!(
            plan(Some(180_000), Some(62_000), 4_000),
            PreviewPlan {
                start_ms: 62_000,
                length_ms: Some(4_000),
                source: StartSource::Sabi,
            }
        );
        assert_eq!(
            plan(Some(10_000), Some(9_800), 4_000),
            PreviewPlan {
                start_ms: 9_000,
                length_ms: Some(1_000),
                source: StartSource::Sabi,
            }
        );
    }

    #[test]
    fn missing_invalid_and_boundary_sabi_use_honest_fixed_fallback() {
        for sabi in [None, Some(0), Some(-1), Some(120_000), Some(i64::MAX)] {
            let p = plan(Some(120_000), sabi, 15_000);
            assert_eq!(p.start_ms, FALLBACK_START_MS);
            assert_eq!(p.source, StartSource::FixedFallback);
            assert_eq!(p.length_ms, Some(15_000));
        }
    }

    #[test]
    fn short_tracks_clamp_start_and_preview_to_remaining_duration() {
        for d in [1, 999, 1_000, 4_000, 20_000, 45_000] {
            let p = plan(Some(d), None, 15_000);
            assert!(p.start_ms <= d);
            assert!(p.length_ms.unwrap() <= d - p.start_ms);
        }
        assert_eq!(plan(Some(5_000), None, 15_000).start_ms, 0);
        assert_eq!(plan(Some(5_000), None, 15_000).length_ms, Some(5_000));
    }

    #[test]
    fn unknown_duration_never_invents_a_seek_or_detection() {
        for d in [None, Some(0)] {
            assert_eq!(
                plan(d, Some(20_000), 4_000),
                PreviewPlan {
                    start_ms: 0,
                    length_ms: Some(4_000),
                    source: StartSource::UnknownDurationFromStart,
                }
            );
        }
    }

    #[test]
    fn pass_is_finite_and_generation_rejects_stale_callbacks() {
        let mut s = Session::start(7, 3, 1, 4_000).unwrap();
        assert!(s.accepts(7, 1));
        assert!(!s.accepts(6, 1));
        assert!(s.advance(true) && s.index() == 2);
        assert!(!s.advance(true) && s.active() && s.index() == 2);
        assert!(s.advance(false) && s.index() == 1);
    }

    #[test]
    fn empty_queue_and_invalid_current_index_cannot_start() {
        assert!(Session::start(1, 0, 0, 4_000).is_none());
        assert!(Session::start(1, 2, 2, 4_000).is_none());
        assert!(Session::start(1, 2, 0, 0).is_none());
    }
}
