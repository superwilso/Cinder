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
//! A style implements what it redesigns and inherits the rest from Cinder: the [`Look`] trait's
//! methods default to Cinder's own drawing. Each style other than Cinder is one file in
//! `styles/` (`styles/terminal.rs`, say) holding its unit struct, its `Look` and its drawing;
//! [`Style::look`] is the one place that names them all. To add a style: a variant here, its line
//! in `ALL` and `look`, and its file. `docs/STYLES.md` walks through it, and the contract tests
//! (`now_playing::tests::every_style_*`, `ui_overflow`) run over every style in `ALL` without being
//! told about it.

use crate::canvas::Canvas;
use crate::kit::Row;
use crate::now_playing::{Layout, NowPlaying};
use crate::text::{Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;

/// Everything a style can change. Every method but the three names has Cinder's drawing as its
/// default, so a style overrides only what it redesigns.
///
/// What no method may do (`docs/DESIGN_GUIDE.md`): bring colours (draw with the theme's tokens),
/// draw under the status bar, move a target the tap test does not also read, use a character the
/// bundled fonts lack, or allocate or resample per frame.
pub trait Look: Sync {
    /// As Settings shows it.
    fn name(&self) -> &'static str;
    /// As `cinder_settings.conf` stores it (`style=`): lower case, never changed once shipped.
    fn token(&self) -> &'static str;
    /// One line for the Display page, under the chips. Write it as `crate::i18n::text("…")`: the
    /// locale tools find translatable text by that call.
    fn about(&self) -> &'static str;

    /// Now Playing's targets. Both the render and the tap test read this, so they cannot disagree.
    fn np_layout(&self, night: bool, lyrics: bool) -> Layout {
        crate::now_playing::cinder_layout(night, lyrics)
    }
    /// Now Playing, every control placed from [`Look::np_layout`].
    fn np_render(&self, c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
        crate::now_playing::cinder_render(c, t, f, np)
    }

    // ── The kit (`kit.rs`): every settings screen is built from these. The boxes are fixed and
    // are the tap test's too (`kit::ROW_H`, `CHIP_H`, `chip_span`, `BUTTON_H`), so a style draws
    // inside them and returns the same y below. Fit text to the box; never size a box to text.

    /// A section label at `y`, `kit::SECTION_H` tall, with an optional action on the right.
    fn section_label(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        label: &str,
        action: Option<&str>,
    ) -> i32 {
        crate::kit::cinder_section_label(c, t, f, y, label, action)
    }
    /// The one-line state strip, `kit::STRIP_H` tall, its line in `ink`.
    fn strip(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        line: &str,
        ink: Rgb888,
    ) -> i32 {
        crate::kit::cinder_strip(c, t, f, y, line, ink)
    }
    /// The switch, `kit::SWITCH_W` × `kit::SWITCH_H` at `(x, y)`.
    fn switch(&self, c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
        crate::kit::cinder_switch(c, t, x, y, on)
    }
    /// A list row, `h` tall at `y`.
    fn row(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32, r: &Row) -> i32 {
        crate::kit::cinder_row(c, t, f, y, h, r)
    }
    /// A row of chips at `y`, each in its `kit::chip_span(i, cols)`, `kit::CHIP_H` tall.
    #[allow(clippy::too_many_arguments)]
    fn chips(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        labels: &[&str],
        sel: Option<usize>,
        cols: usize,
    ) -> i32 {
        crate::kit::cinder_chips(c, t, f, y, labels, sel, cols)
    }
    /// The primary button: `kit::LEFT..kit::RIGHT` across, `kit::BUTTON_H` tall at `y`.
    fn primary_button(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
        crate::kit::cinder_primary_button(c, t, f, y, label)
    }

    // ── The Library (`library.rs`). Its geometry (rows, tabs, bands, the grid) is shared with
    // `nav`'s hit tests; a style changes what is drawn inside it.

    /// The type of a Library list's text: Cinder's family and weight, re-cut for the style. The
    /// size stays, so rows keep their height. Per row, so it must be a lookup, nothing more.
    fn lib_type(&self, fam: Family, w: Weight) -> (Family, Weight) {
        (fam, w)
    }
    /// The chosen (playing or cursor) row's mark across `y..y + h`. Cinder: a wash.
    fn lib_sel(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32) {
        let _ = f;
        crate::widgets::fill_rect(c, 0, y, crate::canvas::W as i32, h, t.row_sel)
    }
    /// The rule under a list row, at `y`. Drawn once per row while a list flings: keep it to a few
    /// fills (a dotted line is 240).
    fn lib_rule(&self, c: &mut Canvas, t: &Theme, y: i32) {
        crate::widgets::hline(c, y, t.line)
    }
    /// The playing row's mark, centred on `(x, cy)`. Cinder: three small bars.
    fn lib_playing(&self, c: &mut Canvas, t: &Theme, x: i32, cy: i32) {
        crate::library::tiny_bars(c, x, cy, t.acc)
    }
    /// The tab strip at `y0`: each tab's `(x, measured width, label, active)`, labels `size` px.
    /// Returns the y of its bottom rule, `y0 + 34`.
    fn lib_tabs(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y0: i32,
        tabs: &[(f32, f32, &str, bool)],
        size: f32,
    ) -> i32 {
        crate::library::cinder_tabs(c, t, f, y0, tabs, size)
    }
    /// A band in `rect` (the Library's shuffle band, an album's Play band): `glyph` on the left,
    /// `label` over `sub`, a play mark on the right. Returns its bottom.
    #[allow(clippy::too_many_arguments)]
    fn lib_band(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        rect: (i32, i32, i32, i32),
        glyph: crate::library::Glyph,
        label: &str,
        sub: &str,
    ) -> i32 {
        crate::library::cinder_band(c, t, f, rect, glyph, label, sub)
    }
    /// The square button beside a band, `glyph` in its middle.
    fn lib_side_button(
        &self,
        c: &mut Canvas,
        t: &Theme,
        rect: (i32, i32, i32, i32),
        glyph: crate::library::Glyph,
    ) {
        crate::library::cinder_side_button(c, t, rect, glyph)
    }

    // ── The chrome (`chrome.rs`). The status bar is the same in every style.

    /// The header title's type, in `ink`. Measured as well as drawn (`chrome::header_title_end`).
    fn header_title(&self, ink: Rgb888) -> TextStyle {
        crate::chrome::cinder_title_style(ink)
    }
    /// The header's back mark, centred on `(cx, cy)`, `size` px.
    fn back_glyph(&self, c: &mut Canvas, t: &Theme, cx: f32, cy: f32, size: f32) {
        crate::icons::back(c, cx, cy, size, t.dim)
    }
    /// The Now Playing bar at the foot of every browsing screen, in `chrome::np_bar_rect()`, its
    /// play button the left `chrome::NP_BAR_PLAY_W` px.
    #[allow(clippy::too_many_arguments)]
    fn np_bar(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        title: &str,
        artist: &str,
        playing: bool,
        progress: f32,
    ) {
        crate::chrome::cinder_np_bar(c, t, f, title, artist, playing, progress)
    }
}

/// Cinder: the default, and every other style's fallback.
pub struct Cinder;
impl Look for Cinder {
    fn name(&self) -> &'static str {
        "Cinder"
    }
    fn token(&self) -> &'static str {
        "cinder"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("Full-bleed cover, big controls")
    }
}

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
    /// A Themes v2 folder (`look.rs`), the one `look::set_active` loaded. v0.5 only: not in `ALL`,
    /// so v0.4's Settings never offers it and no settings file names it.
    Data,
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

    /// The style's drawing. The one place every style is named.
    pub fn look(self) -> &'static dyn Look {
        use crate::styles::*;
        match self {
            Style::Cinder => &Cinder,
            Style::Nocturne => &nocturne::Nocturne,
            Style::Terminal => &terminal::Terminal,
            Style::Cassette => &cassette::Cassette,
            Style::Clock => &clock::Clock,
            Style::Winamp => &winamp::Winamp,
            Style::Data => crate::look::active(),
        }
    }

    /// As Settings shows it.
    pub fn name(self) -> &'static str {
        self.look().name()
    }

    /// As `cinder_settings.conf` stores it (`style=`).
    pub fn token(self) -> &'static str {
        self.look().token()
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
        self.look().about()
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
