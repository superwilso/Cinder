//! Lock screen — ported from cinder-proto-screens1.jsx `CLock`.
//! Big mono clock, track title/artist, a thin centred progress bar, and a lock-glyph hint. The
//! prototype woke on a double-tap, but the NW-A55 lock is the physical Hold switch (touch is fully
//! disabled while locked), so the hint reads "HOLD LOCK · SIDE KEYS ACTIVE · SLIDE HOLD OFF".

use crate::canvas::Canvas;
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub struct Lock<'a> {
    pub clock: &'a str,      // status-bar clock
    pub big_clock: &'a str,  // large centred clock
    pub title: &'a str,
    pub artist: &'a str,
    pub badge: &'a str,
    pub battery: u8,
    pub progress: f32,
}

fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

/// Screen margin the centred text may not cross. The lock screen has no controls to hide, so this
/// is not the status bar's problem — it is the CENTRING that makes an unclamped string harmful:
/// `240 - w/2` goes negative on a long one, so the overflow is split across BOTH edges and the
/// title loses its beginning as well as its end. A trailing ellipsis at least starts at the start.
const SIDE_MARGIN: f32 = 22.0;

fn centered(c: &mut Canvas, f: &FontSet, baseline: f32, s: &str, st: &TextStyle) {
    // Clamp HERE rather than at each call: the two unbounded strings on this screen are the track
    // title and the artist, and putting the budget in the shared helper is what stops the next one
    // added from arriving unclamped.
    let s = crate::widgets::fit(f, s, st, 480.0 - SIDE_MARGIN * 2.0);
    let w = text::measure(f, &s, st);
    text::draw(c, f, 240.0 - w / 2.0, baseline, &s, st);
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, l: &Lock) {
    c.fill(t.bg);

    // big clock, centred in the body
    centered(c, f, 366.0, l.big_clock, &sty(Family::Mono, Weight::Light, 88.0, t.ink, -0.02));
    centered(c, f, 408.0, l.title, &sty(Family::Sans, Weight::Regular, 17.0, t.ink, 0.0));
    centered(c, f, 428.0, l.artist, &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0));

    // thin progress bar, 240px wide, centred
    let (px, pw, py) = (120, 240, 454);
    fill_rect(c, px, py, pw, 2, t.line);
    fill_rect(c, px, py, (pw as f32 * l.progress.clamp(0.0, 1.0)) as i32, 2, t.dim);

    // bottom hint: lock glyph + caption. The Hold switch unlocks (touch is disabled); the side
    // transport keys stay live; Power just wakes the screen.
    let hint = "HOLD LOCK · SIDE KEYS ACTIVE · SLIDE HOLD OFF";
    let hs = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16);
    let hw = text::measure(f, hint, &hs);
    let total = 12.0 + 9.0 + hw;
    let startx = 240.0 - total / 2.0;
    icons::lock(c, startx + 6.0, 771.0, 13.0, t.faint);
    text::draw(c, f, startx + 21.0, 775.0, hint, &hs);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long title must not run off the edges. It is CENTRED, so an unclamped one loses its
    /// start as well as its end — the reader gets the middle of a name and no way to tell.
    #[test]
    fn a_long_title_stays_on_screen() {
        let f = FontSet::load();
        let t = Theme::day();
        let mut c = Canvas::new();
        let long = "\u{041a}\u{043e}\u{0440}\u{043e}\u{043b}\u{0435}\u{0432}\u{0441}\u{043a}\u{0438}\u{0439} \
                    Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364";
        let l = Lock {
            clock: "23:41",
            big_clock: "23:41",
            title: long,
            artist: long,
            badge: "FLAC 24/96",
            battery: 78,
            progress: 0.5,
        };
        render(&mut c, &t, &f, &l);

        // Nothing painted in the side margins on the title/artist baselines. The clock and the
        // status bar are drawn elsewhere on the screen, so these rows see only the two strings
        // this test is about.
        let bg = t.bg;
        for y in [396i32, 400, 416, 420] {
            for x in [0i32, 4, 475, 479] {
                assert_eq!(
                    c.buf[y as usize * crate::canvas::W as usize + x as usize],
                    crate::canvas::to_u32(bg),
                    "text reached the screen edge at ({x},{y}) — the centred clamp is not holding"
                );
            }
        }
    }
}
