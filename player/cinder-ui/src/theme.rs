//! Cinder design tokens (RUST-HANDOFF.md §1.1/§1.2). Two themes: Day + Night, times six accents.
//!
//! The neutrals (bg / panel / line / ink / dim / faint) are the Cinder identity — warm near-black,
//! flat fills, hairline rules — and they do not change. What the user picks is the **accent**: the
//! one saturated colour, used for the selected row, the progress fill, the active segment, the
//! focused label. Everything else stays put, so a colour choice can't wreck the design or make
//! anything unreadable.
//!
//! Each accent carries its own `row_sel` (the wash behind a highlighted row) and `acc_ink` (what is
//! drawn ON an accent fill) rather than deriving them, because both are contrast decisions, not
//! arithmetic: a blend that looks right under amber goes muddy under mint, and near-black ink that
//! reads on a bright accent reads differently on a dark one. Six explicit rows of data beat a
//! clever formula that has to be re-tuned every time an accent is added.

use embedded_graphics::pixelcolor::Rgb888;

#[inline]
fn rgb(hex: u32) -> Rgb888 {
    Rgb888::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The user-selectable accent colour. `Amber` is Cinder's own and is the default — picking it
/// reproduces the original palette byte for byte.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Accent {
    #[default]
    Amber,
    Crimson,
    Violet,
    Azure,
    Mint,
    Bone,
}

/// One accent's colours: `(acc, acc_ink, row_sel)` for day, then the same three for night.
/// Night values are the day ones taken down to roughly 55% luminance — the same relationship the
/// original amber pair had, kept by hand so each accent can be nudged where the eye needs it.
struct Palette {
    name: &'static str,
    acc_d: u32,
    ink_d: u32,
    sel_d: u32,
    acc_n: u32,
    ink_n: u32,
    sel_n: u32,
}

// Order is the cycle order and the swatch order in Settings. Amber first: it is the default and
// the one the design was drawn against.
const PALETTES: [Palette; 6] = [
    // Cinder amber — the original. These six values are unchanged from the pre-accent theme.
    Palette { name: "AMBER",   acc_d: 0xf4651f, ink_d: 0x1a0a02, sel_d: 0x1c1713,
                               acc_n: 0x863810, ink_n: 0x000000, sel_n: 0x0f0c0a },
    Palette { name: "CRIMSON", acc_d: 0xe0392f, ink_d: 0x1a0403, sel_d: 0x1c1214,
                               acc_n: 0x7a1f1a, ink_n: 0x000000, sel_n: 0x0f0a0b },
    Palette { name: "VIOLET",  acc_d: 0x9a6ff0, ink_d: 0x0b0618, sel_d: 0x15141f,
                               acc_n: 0x553d84, ink_n: 0x000000, sel_n: 0x0b0a10 },
    Palette { name: "AZURE",   acc_d: 0x2f8fe0, ink_d: 0x020a16, sel_d: 0x12161f,
                               acc_n: 0x1a4e7a, ink_n: 0x000000, sel_n: 0x0a0b10 },
    Palette { name: "MINT",    acc_d: 0x2fc98a, ink_d: 0x02120c, sel_d: 0x121a17,
                               acc_n: 0x1a6e4c, ink_n: 0x000000, sel_n: 0x0a0e0c },
    // Bone is the "no colour" option: the accent is the ink itself. Nothing on screen is tinted,
    // which is the point — it is the closest Cinder gets to a monochrome instrument panel.
    Palette { name: "BONE",    acc_d: 0xd8d2c8, ink_d: 0x0d0c0b, sel_d: 0x1a1917,
                               acc_n: 0x77736e, ink_n: 0x000000, sel_n: 0x0e0d0c },
];

impl Accent {
    /// How many accents there are — the cycle length, and the number of swatches Settings draws.
    pub const COUNT: usize = PALETTES.len();

    /// All accents in cycle/swatch order.
    pub const ALL: [Accent; 6] = [
        Accent::Amber,
        Accent::Crimson,
        Accent::Violet,
        Accent::Azure,
        Accent::Mint,
        Accent::Bone,
    ];

    /// Index into the cycle. Also what gets persisted.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }

    /// From a persisted/hand-edited index. Out of range falls back to the default rather than
    /// failing — a corrupt settings file must never leave the UI in a state it can't cycle out of.
    pub fn from_index(i: usize) -> Accent {
        *Self::ALL.get(i).unwrap_or(&Accent::Amber)
    }

    /// Short display name for the Settings row ("AMBER", "MINT", …).
    pub fn name(self) -> &'static str {
        PALETTES[self.index()].name
    }

    /// The accent colour itself, in the given theme mode. Used by Settings to paint the swatches,
    /// which have to show every accent at once — not just the selected one.
    pub fn swatch(self, night: bool) -> Rgb888 {
        let p = &PALETTES[self.index()];
        rgb(if night { p.acc_n } else { p.acc_d })
    }

    /// Next accent in the cycle (wraps).
    pub fn next(self) -> Accent {
        Self::from_index((self.index() + 1) % Self::COUNT)
    }
}

pub struct Theme {
    pub bg: Rgb888,
    pub panel: Rgb888,
    pub line: Rgb888,
    pub ink: Rgb888,
    pub dim: Rgb888,
    pub faint: Rgb888,
    pub acc: Rgb888,
    pub acc_ink: Rgb888,
    /// Fill behind the highlighted list/menu row (subtle wash, theme- and accent-aware).
    pub row_sel: Rgb888,
    pub night: bool,
}

/// Scale an `0xRRGGBB` toward black by `pct`/100, per channel. Const so the palettes stay
/// compile-time constants.
const fn dim_rgb(c: u32, pct: u32) -> u32 {
    let r = ((c >> 16) & 0xff) * pct / 100;
    let g = ((c >> 8) & 0xff) * pct / 100;
    let b = (c & 0xff) * pct / 100;
    (r << 16) | (g << 8) | b
}

impl Theme {
    /// Day palette with Cinder's own amber. Equivalent to `day_with(Accent::Amber)`.
    pub fn day() -> Self {
        Self::day_with(Accent::Amber)
    }

    /// Night palette with Cinder's own amber. Equivalent to `night_with(Accent::Amber)`.
    pub fn night() -> Self {
        Self::night_with(Accent::Amber)
    }

    pub fn day_with(a: Accent) -> Self {
        let p = &PALETTES[a.index()];
        Theme {
            bg: rgb(0x0d0c0b),
            panel: rgb(0x13110f),
            line: rgb(0x221f1b),
            ink: rgb(0xece7df),
            dim: rgb(0x95908a),
            faint: rgb(0x5f5a52),
            acc: rgb(p.acc_d),
            acc_ink: rgb(p.ink_d),
            row_sel: rgb(p.sel_d),
            night: false,
        }
    }

    /// How much of the night palette's light to keep. The BACKLIGHT cannot go below its floor —
    /// the panel is a TFT LCD (Himax hx8379c) lit by an MTK BLS PWM, and raw 0 and 1 are visibly
    /// identical because the driver clamps at a nonzero duty. So the remaining lever is how much
    /// white the UI paints: at the backlight floor, what makes a screen uncomfortable in a dark
    /// room is bright chrome, not the lamp behind it.
    ///
    /// 55% keeps every relationship in the palette (ink over dim over faint, accent still the
    /// brightest thing) while roughly halving the light the panel actually emits.
    const NIGHT_DIM_PCT: u32 = 55;

    pub fn night_with(a: Accent) -> Self {
        let p = &PALETTES[a.index()];
        let d = |c: u32| rgb(dim_rgb(c, Self::NIGHT_DIM_PCT));
        Theme {
            // Already true black; scaling it would change nothing.
            bg: rgb(0x000000),
            panel: d(0x0a0908),
            line: d(0x161310),
            ink: d(0x8d8170),
            dim: d(0x5b5347),
            faint: d(0x3b362d),
            acc: d(p.acc_n),
            // NOT dimmed: this is the ink drawn ON the accent band, and it is already the dark
            // half of that pair. Dimming both sides would collapse the contrast between them.
            acc_ink: rgb(p.ink_n),
            row_sel: d(p.sel_n),
            night: true,
        }
    }

    /// The theme for a (mode, accent) pair — what the navigator actually calls.
    pub fn for_mode(night: bool, a: Accent) -> Self {
        if night {
            Self::night_with(a)
        } else {
            Self::day_with(a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::RgbColor;

    /// The default accent must reproduce the original palette exactly. If this ever fails, adding
    /// accents changed the look of a device that never asked for one.
    ///
    /// NIGHT is the deliberate exception: its whole palette is scaled by `NIGHT_DIM_PCT` so the
    /// screen emits less light at a backlight that cannot go below its own floor. The originals
    /// are still written out here, scaled by the same constant, so this stays a statement about
    /// the palette rather than a row of magic numbers.
    #[test]
    fn amber_is_byte_identical_to_the_original_palette() {
        let d = Theme::day();
        assert_eq!(d.acc, rgb(0xf4651f));
        assert_eq!(d.acc_ink, rgb(0x1a0a02));
        assert_eq!(d.row_sel, rgb(0x1c1713));
        let n = Theme::night();
        assert_eq!(n.acc, rgb(dim_rgb(0x863810, Theme::NIGHT_DIM_PCT)));
        assert_eq!(n.acc_ink, rgb(0x000000), "ink ON the accent must not be dimmed too");
        assert_eq!(n.row_sel, rgb(dim_rgb(0x0f0c0a, Theme::NIGHT_DIM_PCT)));
    }

    /// The point of the change: night must actually emit less light than it used to, and still
    /// keep its own ordering (ink brighter than dim, dim brighter than faint).
    #[test]
    fn night_is_dimmer_than_it_was_and_still_ordered() {
        let lum = |c: Rgb888| c.r() as u32 * 2 + c.g() as u32 * 3 + c.b() as u32;
        let n = Theme::night();
        // The pre-change values, for the comparison this test exists to make.
        assert!(lum(n.ink) < lum(rgb(0x8d8170)), "night ink is no dimmer than before");
        assert!(lum(n.acc) < lum(rgb(0x863810)), "night accent is no dimmer than before");
        assert!(lum(n.ink) > lum(n.dim), "ink must stay above dim");
        assert!(lum(n.dim) > lum(n.faint), "dim must stay above faint");
        assert!(lum(n.ink) > lum(n.bg), "ink must stay readable against the background");
        assert_eq!(n.bg, rgb(0x000000), "night background must stay true black");
    }

    /// Neutrals are the identity — they must not vary with the accent.
    #[test]
    fn accents_change_only_the_accent_tokens() {
        let base = Theme::day_with(Accent::Amber);
        for a in Accent::ALL {
            let t = Theme::day_with(a);
            assert_eq!(t.bg, base.bg, "{a:?} moved bg");
            assert_eq!(t.panel, base.panel, "{a:?} moved panel");
            assert_eq!(t.line, base.line, "{a:?} moved line");
            assert_eq!(t.ink, base.ink, "{a:?} moved ink");
            assert_eq!(t.dim, base.dim, "{a:?} moved dim");
            assert_eq!(t.faint, base.faint, "{a:?} moved faint");
        }
    }

    /// Every accent must be visually distinct, or the picker offers a choice that isn't one.
    #[test]
    fn every_accent_is_distinct() {
        for (i, a) in Accent::ALL.iter().enumerate() {
            for b in &Accent::ALL[i + 1..] {
                assert_ne!(a.swatch(false), b.swatch(false), "{a:?} and {b:?} look the same");
                assert_ne!(a.swatch(true), b.swatch(true), "{a:?}/{b:?} collide at night");
            }
        }
    }

    /// The night accent has to be dimmer than its day twin — that is the whole point of night mode,
    /// and an accent that skipped the dimming would be the brightest thing on a dark screen.
    #[test]
    fn night_accents_are_dimmer_than_day() {
        let lum = |c: Rgb888| {
            use embedded_graphics::pixelcolor::RgbColor;
            c.r() as u32 * 30 + c.g() as u32 * 59 + c.b() as u32 * 11
        };
        for a in Accent::ALL {
            assert!(
                lum(a.swatch(true)) < lum(a.swatch(false)),
                "{a:?} is not dimmer at night"
            );
        }
    }

    /// Cycling must visit every accent and come home.
    #[test]
    fn the_cycle_is_a_cycle() {
        let mut a = Accent::Amber;
        let mut seen = vec![a];
        for _ in 1..Accent::COUNT {
            a = a.next();
            assert!(!seen.contains(&a), "cycle repeated at {a:?}");
            seen.push(a);
        }
        assert_eq!(a.next(), Accent::Amber, "cycle did not wrap to the default");
    }

    /// A corrupt or hand-edited settings file must land on the default, not panic or index-wrap.
    #[test]
    fn out_of_range_index_falls_back_to_the_default() {
        assert_eq!(Accent::from_index(99), Accent::Amber);
        assert_eq!(Accent::from_index(Accent::COUNT), Accent::Amber);
        for (i, a) in Accent::ALL.iter().enumerate() {
            assert_eq!(Accent::from_index(i), *a);
            assert_eq!(a.index(), i);
        }
    }
}
