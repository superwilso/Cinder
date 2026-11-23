//! The styles other than Cinder, one file each (see `style.rs` for what a style is and the
//! `Look` trait each one implements). This file holds what they share: the Lyrics and Menu targets
//! every style keeps, the cover and its night dimming, the audio pages and the empty state.
//!
//! Each style's Now Playing is two functions over one table of numbers: `layout`, which says where
//! every target is, and `render`, which draws the screen and places every control FROM that
//! layout. Neither has a coordinate the other lacks, so a style cannot draw Play in one place and
//! answer for it in another. The contract they all pass is `now_playing::tests::every_style_*`.
//!
//! What every style keeps, whatever it looks like (`docs/DESIGN_GUIDE.md`, "Now Playing"):
//!
//! - the status bar, and the band under it (y 44–91) for the Menu, the Lyrics chip on the left and
//!   the sleep badge on the right. Nothing of a style's own is drawn there;
//! - the palette's colours only, with night drawing the cover at a third of its brightness;
//! - the three pages (cover, spectrum, level), turned by a swipe across the cover's block;
//! - every control Cinder has: like, title block (Track information), seek, times, shuffle, prev,
//!   play/pause, next, repeat, and a way to the Library, Up Next, Bluetooth and Settings.

#[allow(unused_imports)]
use crate::art;
use crate::canvas::Canvas;
use crate::icons;
use crate::now_playing::{self as np, Hit, Layout, NowPlaying, NpPage, Rail, Shape};
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{self, fill_rect, hline, stroke_rect, sty};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle};

const W: i32 = crate::canvas::W as i32;
const H: i32 = crate::canvas::H as i32;

/// The Lyrics chip and the Menu band: the same in every style, and first and last in its order.
fn push_lyrics(l: &mut Layout, lyrics: bool) {
    if lyrics {
        l.push(
            Shape::Rect {
                x: np::lyrics_shift(),
                y: crate::chrome::STATUS_H,
                w: np::LYRICS_HIT_W,
                h: 44,
            },
            Hit::Lyrics,
        );
    }
}

fn push_menu(l: &mut Layout) {
    l.push(
        Shape::Rect {
            x: 0,
            y: 0,
            w: W,
            h: crate::chrome::HEADER_BOTTOM,
        },
        Hit::Menu,
    );
}

fn centre(l: &Layout, h: Hit) -> (f32, f32) {
    let (x, y) = l.at(h).map_or((0, 0), Shape::centre);
    (x as f32, y as f32)
}

fn s(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    sty(fam, weight, size, color, tracking)
}

fn idle(np: &NowPlaying) -> bool {
    np.title.is_empty() && np.artist.is_empty()
}

/// The page block's contents for pages 2 and 3, inside `(x, y, w, h)`; page 1 is the style's own.
fn audio_page(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    np: &NowPlaying,
    (x, y, w, h): (i32, i32, i32, i32),
) {
    let mid = (x + w / 2) as f32;
    let caption = s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18);
    match np::page_from_index(np.page) {
        NpPage::Cover => {}
        NpPage::Spectrum => {
            widgets::center(
                c,
                f,
                mid,
                (y + 24) as f32,
                crate::viz::name_upper(np.viz_kind),
                &caption,
            );
            if np::viz_live(np) {
                crate::viz::draw_any(
                    c,
                    x + 12,
                    y + 44,
                    w - 24,
                    h - 60,
                    np.viz_seed,
                    crate::viz::from_index(np.viz_kind),
                    t.acc,
                    t.line,
                    np.viz_levels,
                    np.viz_peaks,
                    np.viz_sig,
                    255,
                    255,
                );
            } else {
                widgets::center(
                    c,
                    f,
                    mid,
                    (y + h / 2) as f32,
                    np::viz_absent_text(np).0,
                    &s(Family::Sans, Weight::Regular, 20.0, t.dim, 0.0),
                );
            }
        }
        NpPage::Level => {
            widgets::center(
                c,
                f,
                mid,
                (y + 24) as f32,
                crate::i18n::text("OUTPUT LEVEL"),
                &caption,
            );
            let (mean, peak) = np::level_stats(np);
            let (mx, mw, my, mh) = (x + 16, w - 32, y + h / 2 - 40, 48);
            fill_rect(c, mx, my, mw, mh, t.panel);
            stroke_rect(c, mx, my, mw, mh, t.line, 1);
            let fw = (mw as f32 * mean).round() as i32;
            if fw > 0 {
                fill_rect(c, mx, my, fw, mh, t.acc);
            }
            let px = mx + ((mw - 3) as f32 * peak).round() as i32;
            fill_rect(
                c,
                px,
                my - 6,
                3,
                mh + 12,
                if peak > 0.0 { t.ink } else { t.line },
            );
            let mut b = [0u8; 3];
            widgets::center(
                c,
                f,
                mid,
                (my + mh + 72) as f32,
                np::dec((mean * 100.0).round() as i32, &mut b),
                &s(Family::Mono, Weight::Bold, 48.0, t.ink, 0.0),
            );
        }
    }
}

/// The cover, or what stands in for it, in a `size` square at `(x, y)`. Night draws it at a third
/// of its brightness, as Cinder's night thumbnail does.
fn cover(c: &mut Canvas, t: &Theme, np: &NowPlaying, x: i32, y: i32, size: i32) {
    let op = if t.night { 0.32 } else { 1.0 };
    match np.art_full {
        Some(img) => art::draw_fitted(c, t, x, y, img, size as usize, op),
        None => art::block_cached(c, t, x, y, size, size, np.art, op),
    }
}

/// The small visualiser over the bottom of the cover, standing on `bottom`, when it is on.
fn cover_viz(c: &mut Canvas, t: &Theme, np: &NowPlaying, x: i32, w: i32, bottom: i32) {
    let size = crate::viz::size_from_index(np.viz_size);
    if let Some((vy, vh, at, ab)) = crate::viz::size_box(size, bottom, false) {
        crate::viz::draw_any(
            c,
            x,
            vy,
            w,
            vh,
            np.viz_seed,
            crate::viz::from_index(np.viz_kind),
            t.acc,
            t.line,
            np.viz_levels,
            np.viz_peaks,
            np.viz_sig,
            at,
            ab,
        );
    }
}

fn nothing_playing(c: &mut Canvas, t: &Theme, f: &FontSet, cx: f32, cy: f32, upper: bool) {
    let head = if upper {
        crate::i18n::text("NOTHING PLAYING")
    } else {
        crate::i18n::text("Nothing playing")
    };
    let fam = if upper { Family::Mono } else { Family::Sans };
    widgets::center(
        c,
        f,
        cx,
        cy,
        head,
        &s(fam, Weight::Regular, 22.0, t.dim, 0.0),
    );
    widgets::center(
        c,
        f,
        cx,
        cy + 26.0,
        if upper {
            crate::i18n::text("CHOOSE A TRACK FROM YOUR LIBRARY")
        } else {
            crate::i18n::text("Choose a track from your library")
        },
        &if upper {
            s(Family::Mono, Weight::Regular, 11.0, t.dim, 0.18)
        } else {
            crate::kit::hint_style(t)
        },
    );
}

/// What every style draws last: the Lyrics chip, when the song has lyrics.
fn finish(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    if np.lyrics {
        np::lyrics_chip(c, t, f);
    }
}

/// A toolbar word centred on its slot, fitted to the slot, then moved in from the glass rather than
/// cut: `widgets::center` keeps a run symmetric about its anchor, and the end slots' anchors are
/// 60 px from the edge, which cut "SETTINGS" to "SETTIN…" at 120 %.
fn toolbar_word(c: &mut Canvas, f: &FontSet, cx: f32, baseline: f32, word: &str, st: &TextStyle) {
    let word = widgets::fit(f, word, st, 112.0);
    let w = text::measure(f, &word, st);
    let x0 = (cx - w / 2.0).clamp(22.0, (crate::canvas::W as f32 - 22.0 - w).max(22.0));
    text::draw(c, f, x0, baseline, &word, st);
}

/// A dotted line from `x0` to `x1` at `y`: every other pixel. Terminal's rules.
fn dotted_h(c: &mut Canvas, x0: i32, x1: i32, y: i32, col: Rgb888) {
    for x in (x0..x1).step_by(2) {
        fill_rect(c, x, y, 1, 1, col);
    }
}

/// A dotted box outline.
fn dotted_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    dotted_h(c, x, x + w, y, col);
    dotted_h(c, x, x + w, y + h - 1, col);
    for yy in (y..y + h).step_by(2) {
        fill_rect(c, x, yy, 1, 1, col);
        fill_rect(c, x + w - 1, yy, 1, 1, col);
    }
}

/// The kit and chrome methods of `style::Look`, taken from another style: for a style that
/// redraws Now Playing and dresses everything else like `$base` (Cassette and Winamp wear
/// Terminal's kit, Clock Nocturne's). Use it inside the `impl Look`.
macro_rules! kit_from {
    ($base:expr) => {
        fn lib_sel(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            h: i32,
        ) {
            crate::style::Look::lib_sel(&$base, c, t, f, y, h)
        }
        fn lib_rule(&self, c: &mut crate::canvas::Canvas, t: &crate::theme::Theme, y: i32) {
            crate::style::Look::lib_rule(&$base, c, t, y)
        }
        fn lib_playing(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            x: i32,
            cy: i32,
        ) {
            crate::style::Look::lib_playing(&$base, c, t, x, cy)
        }
        fn lib_type(
            &self,
            fam: crate::text::Family,
            w: crate::text::Weight,
        ) -> (crate::text::Family, crate::text::Weight) {
            crate::style::Look::lib_type(&$base, fam, w)
        }
        fn lib_tabs(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y0: i32,
            tabs: &[(f32, f32, &str, bool)],
            size: f32,
        ) -> i32 {
            crate::style::Look::lib_tabs(&$base, c, t, f, y0, tabs, size)
        }
        #[allow(clippy::too_many_arguments)]
        fn lib_band(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            rect: (i32, i32, i32, i32),
            glyph: crate::library::Glyph,
            label: &str,
            sub: &str,
        ) -> i32 {
            crate::style::Look::lib_band(&$base, c, t, f, rect, glyph, label, sub)
        }
        fn lib_side_button(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            rect: (i32, i32, i32, i32),
            glyph: crate::library::Glyph,
        ) {
            crate::style::Look::lib_side_button(&$base, c, t, rect, glyph)
        }
        fn section_label(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            label: &str,
            action: Option<&str>,
        ) -> i32 {
            crate::style::Look::section_label(&$base, c, t, f, y, label, action)
        }
        fn strip(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            line: &str,
            ink: embedded_graphics::pixelcolor::Rgb888,
        ) -> i32 {
            crate::style::Look::strip(&$base, c, t, f, y, line, ink)
        }
        fn switch(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            x: i32,
            y: i32,
            on: bool,
        ) {
            crate::style::Look::switch(&$base, c, t, x, y, on)
        }
        fn row(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            h: i32,
            r: &crate::kit::Row,
        ) -> i32 {
            crate::style::Look::row(&$base, c, t, f, y, h, r)
        }
        #[allow(clippy::too_many_arguments)]
        fn chips(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            labels: &[&str],
            sel: Option<usize>,
            cols: usize,
        ) -> i32 {
            crate::style::Look::chips(&$base, c, t, f, y, labels, sel, cols)
        }
        fn primary_button(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            y: i32,
            label: &str,
        ) -> i32 {
            crate::style::Look::primary_button(&$base, c, t, f, y, label)
        }
        fn header_title(
            &self,
            ink: embedded_graphics::pixelcolor::Rgb888,
        ) -> crate::text::TextStyle {
            crate::style::Look::header_title(&$base, ink)
        }
        fn back_glyph(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            cx: f32,
            cy: f32,
            size: f32,
        ) {
            crate::style::Look::back_glyph(&$base, c, t, cx, cy, size)
        }
        #[allow(clippy::too_many_arguments)]
        fn np_bar(
            &self,
            c: &mut crate::canvas::Canvas,
            t: &crate::theme::Theme,
            f: &crate::text::FontSet,
            title: &str,
            artist: &str,
            playing: bool,
            progress: f32,
        ) {
            crate::style::Look::np_bar(&$base, c, t, f, title, artist, playing, progress)
        }
    };
}

// The styles, after `kit_from!` so they can use it.
pub mod cassette;
pub mod clock;
pub mod nocturne;
pub mod terminal;
pub mod winamp;
