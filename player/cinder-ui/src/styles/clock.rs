#[allow(unused_imports)]
use super::*;

/// Clock: Nocturne, with the time large where the cover was. The cover's page is the clock; the
/// other two pages are the usual ones. Shown when nothing plays too, since a clock is useful then.
pub(super) fn clock_face(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
    use super::nocturne::noc::*;
    let mid = (ART_X + ART / 2) as f32;
    let cy = (ART_Y + ART / 2) as f32;
    // Display type, not body text: it already fills the block, so the text size must not grow it
    // (at 130 % it was cut to "14:…").
    let size = 112.0 * 100.0 / crate::text::scale_pct().max(100) as f32;
    widgets::center(
        c,
        f,
        mid,
        cy + 30.0,
        np.clock,
        &s(Family::Mono, Weight::Light, size, t.ink, 0.0),
    );
    fill_rect(c, ART_X, ART_Y + ART / 2 + 70, ART, 1, t.line);
    if !np.title.is_empty() {
        let st = s(Family::Mono, Weight::Regular, 12.0, t.faint, 0.18);
        widgets::center(c, f, mid, cy + 100.0, crate::i18n::text("NOW PLAYING"), &st);
    }
}

/// A large digital clock where the cover was, Nocturne around it.
pub struct Clock;
impl crate::style::Look for Clock {
    fn name(&self) -> &'static str {
        "Clock"
    }
    fn token(&self) -> &'static str {
        "clock"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("A large clock in place of the cover")
    }
    fn np_layout(&self, _night: bool, lyrics: bool) -> Layout {
        super::nocturne::layout(lyrics)
    }
    fn np_render(&self, c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
        let l = self.np_layout(false, np.lyrics);
        super::nocturne::render(c, t, f, np, &l, true);
        finish(c, t, f, np);
    }
    kit_from!(super::nocturne::Nocturne);
}
