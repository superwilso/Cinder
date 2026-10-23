//! Design styles — how the screens are laid out and drawn, chosen in Settings ▸ Display ▸ Style.
//!
//! Three layers make up how Cinder looks (see `docs/DESIGN_GUIDE.md`):
//!
//! | Layer | What it changes | Made in | Chosen in |
//! |---|---|---|---|
//! | **Palette** | Colours only | A text file (`docs/PALETTES.md`) | Display ▸ Palette |
//! | **Style** | Layout, type, shapes, how a control looks | Rust, compiled in (this file) | Display ▸ Style |
//! | **Accent** | The one colour that means "act here" | Built in | Display ▸ Accent |
//!
//! They are independent: every style draws with the active palette's colours, and a palette is
//! checked against its readability rules whatever style draws it. A style never brings colours of
//! its own, which is what keeps night mode, the accent and every palette working under it.
//!
//! A style implements the screens it redesigns and inherits the rest from `Cinder`. Today that is
//! Now Playing: its geometry is one pure function per style (`now_playing::layout`), which both the
//! draw and the tap read, so a style cannot draw a control in one place and answer taps for it in
//! another. The contract every style must pass is `now_playing::tests::every_style_*`.

/// A design style. `Cinder` is the default and the fallback for any screen a style leaves alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    #[default]
    Cinder,
    /// Dark editorial: an inset cover with room around it, large light type, a thin rail and an
    /// outlined play ring. From the design exploration's "Nocturne" direction.
    Nocturne,
    /// Retro and blunt: a framed cover, monospace capitals, bracketed text buttons and a rail of
    /// cells. From the design exploration's "Terminal" direction. Pairs well with an amber palette.
    Terminal,
    /// A cassette in a deck: reels that wind as the song plays, the title on the tape's label,
    /// piano keys for the transport. Terminal's geometry.
    Cassette,
    /// A large digital clock where the cover was, Nocturne around it.
    Clock,
    /// A classic Winamp skin (`.wsz`) from the top of the drive. Terminal's geometry.
    Winamp,
}

impl Style {
    pub const ALL: [Style; 6] = [
        Style::Cinder,
        Style::Nocturne,
        Style::Terminal,
        Style::Cassette,
        Style::Clock,
        Style::Winamp,
    ];
    pub const COUNT: usize = Self::ALL.len();

    /// As Settings shows it.
    pub fn name(self) -> &'static str {
        match self {
            Style::Cinder => "Cinder",
            Style::Nocturne => "Nocturne",
            Style::Terminal => "Terminal",
            Style::Cassette => "Cassette",
            Style::Clock => "Clock",
            Style::Winamp => "Winamp",
        }
    }

    /// As `cinder_settings.conf` stores it (`style=`).
    pub fn token(self) -> &'static str {
        match self {
            Style::Cinder => "cinder",
            Style::Nocturne => "nocturne",
            Style::Terminal => "terminal",
            Style::Cassette => "cassette",
            Style::Clock => "clock",
            Style::Winamp => "winamp",
        }
    }

    /// An unknown word is `None`, and the caller keeps what it had: a settings file written by a
    /// newer build must not reset the style to something the user did not pick.
    pub fn from_token(s: &str) -> Option<Style> {
        Self::ALL.iter().copied().find(|st| st.token() == s.trim())
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Style {
        Self::ALL.get(i).copied().unwrap_or_default()
    }

    /// One line for the Display page, under the chips.
    pub fn about(self) -> &'static str {
        match self {
            Style::Cinder => crate::i18n::text("Full-bleed cover, big controls"),
            Style::Nocturne => crate::i18n::text("Quiet: inset cover, thin lines"),
            Style::Terminal => crate::i18n::text("Framed cover, text buttons"),
            Style::Cassette => crate::i18n::text("A tape: reels wind as it plays"),
            Style::Clock => crate::i18n::text("A large clock in place of the cover"),
            Style::Winamp => crate::i18n::text("A Winamp skin (.wsz); tap again for the next"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_round_trips_through_its_token_and_index() {
        for s in Style::ALL {
            assert_eq!(Style::from_token(s.token()), Some(s));
            assert_eq!(Style::from_index(s.index()), s);
        }
        assert_eq!(
            Style::from_token("vaporwave"),
            None,
            "an unknown style is not a reset"
        );
        assert_eq!(Style::from_index(99), Style::Cinder);
    }
}
