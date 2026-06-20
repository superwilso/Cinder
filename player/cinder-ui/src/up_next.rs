//! Up Next — ported from cinder-proto-screens1.jsx `CUpNext`. The play queue:
//! index/▶, 40px thumb, title/artist, duration, drag handle; now-playing row
//! gets the panel fill + accent text. Footer: Clear queue / Save as playlist.

use crate::art;
use crate::canvas::W;
use crate::data::SONGS;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use crate::Canvas;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, current: usize) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Up Next", Some(&format!("{} TRACKS · 41:24", SONGS.len())));

    let rh = 62;
    for (i, song) in SONGS.iter().enumerate() {
        let yt = y0 + i as i32 * rh;
        let cy = (yt + rh / 2) as f32;
        let now = i == current;
        if now {
            fill_rect(c, 0, yt, W as i32, rh, t.panel);
        }
        // index / ▶
        let idx_col = if now { t.acc } else { t.faint };
        let idx = if now { "▶".to_string() } else { format!("{:02}", i + 1) };
        text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 10.0, idx_col, 0.0));
        // thumb
        art::block(c, t, 46, yt + (rh - 40) / 2, 40, 40, song.art, if t.night { 0.30 } else { 1.0 });
        // title / artist
        let title_col = if now { t.acc } else { t.ink };
        text::draw(c, f, 100.0, cy - 2.0, song.t, &sty(Family::Sans, Weight::SemiBold, 15.0, title_col, 0.0));
        text::draw(c, f, 100.0, cy + 16.0, song.a, &sty(Family::Sans, Weight::Regular, 11.0, t.dim, 0.0));
        // duration + drag handle (3 short rules)
        right(c, f, 432.0, cy + 4.0, song.d, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        for dy in [-4, 0, 4] {
            fill_rect(c, 446, cy as i32 + dy, 14, 1, t.faint);
        }
        hline(c, yt + rh, t.line);
    }

    // footer
    let fy = 744;
    hline(c, fy, t.line);
    let fcy = (fy + 56 / 2) as f32;
    text::draw(c, f, 22.0, fcy + 4.0, "Clear queue", &sty(Family::Sans, Weight::SemiBold, 13.0, t.dim, 0.0));
    right(c, f, 458.0, fcy + 4.0, "Save as playlist", &sty(Family::Sans, Weight::Bold, 13.0, t.acc, 0.0));
}
