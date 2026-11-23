#[allow(unused_imports)]
use super::*;

// ── Winamp ────────────────────────────────────────────────────────────────────────────────────
//
// A classic Winamp skin from the drive takes the whole panel, turned on its side (`winamp.rs`);
// `nav` draws that with the queue and the levels it holds, and this is the same view without them,
// for a render that does not come through `nav`. Without a skin it is Terminal with a line that
// says how to add one, and Terminal's targets.

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout) {
    use super::terminal::term::*;
    if let Some(skin) = np.skin.filter(|s| s.main.is_some()) {
        crate::winamp::render(c, t, f, np, skin, &crate::winamp::View::default());
        return;
    }
    super::terminal::render(c, t, f, np, l);
    fill_rect(c, FRAME_X, FRAME_Y, FRAME, 40, t.panel);
    widgets::center(
        c,
        f,
        240.0,
        (FRAME_Y + 25) as f32,
        crate::i18n::text("NO SKIN: COPY A .WSZ TO THE TOP OF THE DRIVE"),
        &s(Family::Mono, Weight::Regular, 11.0, t.acc, 0.06),
    );
}

/// A classic Winamp skin (`.wsz`) from the top of the drive. Terminal's geometry.
pub struct Winamp;
impl crate::style::Look for Winamp {
    fn name(&self) -> &'static str {
        "Winamp"
    }
    fn token(&self) -> &'static str {
        "winamp"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("A Winamp skin (.wsz); tap again for the next")
    }
    fn np_layout(&self, _night: bool, lyrics: bool) -> Layout {
        super::terminal::layout(lyrics)
    }
    fn np_render(&self, c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
        let l = self.np_layout(false, np.lyrics);
        render(c, t, f, np, &l);
        finish(c, t, f, np);
    }
    kit_from!(super::terminal::Terminal);
}
