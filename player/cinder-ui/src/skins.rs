//! Display ▸ Style ▸ Winamp (or Cassette) again — the skins (or tapes) on the drive, to pick one.
//! Wampy's skin dropdown. The rows hit and scroll as `language.rs`'s do (`language::row_at`,
//! `language::max_scroll`).

use crate::canvas::H;
use crate::kit::{self, Row};
use crate::language::TOP;
use crate::text::FontSet;
use crate::theme::Theme;
use crate::widgets::{center, sty};
use crate::Canvas;

pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    tapes: bool,
    names: &[String],
    current: &str,
    scroll: i32,
) {
    c.fill(t.bg);
    let title = if tapes {
        crate::i18n::text("Tapes")
    } else {
        crate::i18n::text("Skins")
    };
    crate::chrome::header(c, t, f, title, None);
    if names.is_empty() {
        let hint = if tapes {
            crate::i18n::text("No tape: copy a tape folder to cinder_cassette/tape")
        } else {
            crate::i18n::text("No skin: copy a .wsz to the top of the drive")
        };
        let st = sty(
            crate::text::Family::Sans,
            crate::text::Weight::Regular,
            15.0,
            t.faint,
            0.0,
        );
        center(c, f, 240.0, 300.0, hint, &st);
        return;
    }
    c.set_clip_y(TOP, H as i32);
    for (i, n) in names.iter().enumerate() {
        let y = TOP + i as i32 * kit::ROW_H() - scroll;
        if y + kit::ROW_H() < TOP || y >= H as i32 {
            continue;
        }
        kit::row(c, t, f, y, kit::ROW_H(), &Row::new(n).sel(n == current));
    }
    c.clear_clip();
}
