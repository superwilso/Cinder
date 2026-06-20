//! Library — ported from cinder-proto-screens2.jsx `CLibrary` + `CArtist`.
//! Tabs (Songs / Albums / Artists / Playlists), each with a scope-aware accent
//! shuffle row, then the list. Plus the drill-in Artist page.

use crate::art;
use crate::canvas::W;
use crate::data::{self, ALBUM_GROUPS, ARTISTS, PLAYLISTS, SONGS};
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, stroke_rect, sty};
use crate::Canvas;
use embedded_graphics::pixelcolor::Rgb888;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Songs,
    Albums,
    Artists,
    Playlists,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Songs, "SONGS"),
    (Tab::Albums, "ALBUMS"),
    (Tab::Artists, "ARTISTS"),
    (Tab::Playlists, "PLAYLISTS"),
];

fn count_caption(tab: Tab) -> &'static str {
    match tab {
        Tab::Songs => "1,842 TRACKS",
        Tab::Albums => "124 ALBUMS",
        Tab::Artists => "96 ARTISTS",
        Tab::Playlists => "12 PLAYLISTS",
    }
}

fn artdim(t: &Theme) -> f32 {
    if t.night { 0.30 } else { 1.0 }
}

/// The 4 now-playing indicator bars (FBars n=4).
fn tiny_bars(c: &mut Canvas, x: i32, cy: i32, acc: Rgb888) {
    let hs = [10, 14, 7, 12];
    for (i, h) in hs.iter().enumerate() {
        fill_rect(c, x + i as i32 * 5, cy + 7 - h, 3, *h, acc);
    }
}

/// Overlapping album-art stack (artist identity): one or two swatches.
fn art_stack(c: &mut Canvas, t: &Theme, x: i32, cy: i32, arts: &[&str]) {
    let op = artdim(t);
    if arts.len() == 1 {
        art::block(c, t, x, cy - 22, 44, 44, arts[0], op);
    } else {
        art::block(c, t, x + 18, cy - 24, 36, 36, arts[1], 0.55 * op);
        art::block(c, t, x, cy - 4, 40, 40, arts[0], op);
    }
}

/// Tab bar; returns the y where the shuffle row begins.
fn tabs(c: &mut Canvas, t: &Theme, f: &FontSet, y0: i32, active: Tab) -> i32 {
    let mut x = 22.0;
    for (tab, label) in TABS {
        let on = tab == active;
        let st = sty(Family::Mono, Weight::Regular, 11.0, if on { t.acc } else { t.faint }, 0.12);
        let w = text::measure(f, label, &st);
        text::draw(c, f, x, (y0 + 20) as f32, label, &st);
        if on {
            fill_rect(c, x as i32, y0 + 32, w as i32, 2, t.acc);
        }
        x += w + 22.0;
    }
    hline(c, y0 + 34, t.line);
    y0 + 34
}

/// Accent shuffle row (scope-aware). Returns the y below it.
fn shuffle_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str, sub: &str) -> i32 {
    let top = y + 16;
    let h = 56;
    fill_rect(c, 22, top, (W as i32) - 44, h, t.acc);
    let cy = top + h / 2;
    icons::shuffle(c, 42.0, cy as f32, 20.0, t.acc_ink);
    text::draw(c, f, 64.0, (cy - 4) as f32, label, &sty(Family::Sans, Weight::Bold, 15.0, t.acc_ink, 0.0));
    text::draw(c, f, 64.0, (cy + 14) as f32, sub, &sty(Family::Mono, Weight::Regular, 9.0, t.acc_ink, 0.06));
    icons::play(c, 438.0, cy as f32, 18.0, t.acc_ink);
    top + h
}

fn body_label(fam: Family, w: Weight, size: f32, col: Rgb888) -> TextStyle {
    sty(fam, w, size, col, 0.0)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, tab: Tab, current: usize) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    let y0 = crate::chrome::header(c, t, f, "Library", Some(count_caption(tab)));
    let yt = tabs(c, t, f, y0, tab);

    match tab {
        Tab::Songs => {
            let mut y = shuffle_row(c, t, f, yt, "Shuffle all songs", "1,842 TRACKS · RANDOM ORDER") + 8;
            let rh = 62;
            for (i, sgn) in SONGS.iter().enumerate() {
                let cy = y + rh / 2;
                let now = i == current;
                art::block(c, t, 22, y + (rh - 42) / 2, 42, 42, sgn.art, artdim(t));
                let tcol = if now { t.acc } else { t.ink };
                text::draw(c, f, 78.0, (cy - 2) as f32, sgn.t, &body_label(Family::Sans, Weight::SemiBold, 15.0, tcol));
                text::draw(c, f, 78.0, (cy + 16) as f32, sgn.a, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                if now {
                    tiny_bars(c, 408, cy, t.acc);
                }
                right(c, f, 458.0, (cy + 4) as f32, sgn.d, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
                hline(c, y + rh, t.line);
                y += rh;
            }
        }
        Tab::Albums => {
            let mut y = shuffle_row(c, t, f, yt, "Shuffle by album", "RANDOM ALBUM ORDER · TRACKS IN SEQUENCE") + 4;
            for g in ALBUM_GROUPS {
                // section header
                let label = g.artist.to_uppercase();
                text::draw(c, f, 22.0, (y + 20) as f32, &label, &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.16));
                let cap = format!("{} ALBUM{}", g.albums.len(), if g.albums.len() > 1 { "S" } else { "" });
                right(c, f, 458.0, (y + 20) as f32, &cap, &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.08));
                y += 30;
                let rh = 60;
                for al in g.albums {
                    let cy = y + rh / 2;
                    art::block(c, t, 22, y + (rh - 44) / 2, 44, 44, al.art, artdim(t));
                    text::draw(c, f, 80.0, (cy - 2) as f32, al.n, &body_label(Family::Sans, Weight::SemiBold, 15.0, t.ink));
                    let sub = format!("{} · {} tracks", al.y, al.k);
                    text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                    stroke_rect(c, 420, cy - 19, 38, 38, t.line, 1);
                    icons::shuffle(c, 439.0, cy as f32, 14.0, t.dim);
                    hline(c, y + rh, t.line);
                    y += rh;
                }
            }
        }
        Tab::Artists => {
            let mut y = shuffle_row(c, t, f, yt, "Shuffle by artist", "RANDOM ARTIST · SHUFFLED WITHIN ARTIST") + 8;
            let rh = 64;
            for ar in ARTISTS {
                let cy = y + rh / 2;
                art_stack(c, t, 22, cy, ar.arts);
                text::draw(c, f, 90.0, (cy - 2) as f32, ar.n, &body_label(Family::Sans, Weight::SemiBold, 15.0, t.ink));
                let sub = format!("{} albums · {} tracks", ar.al, ar.tr);
                text::draw(c, f, 90.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                stroke_rect(c, 418, cy - 20, 40, 40, t.line, 1);
                icons::shuffle(c, 438.0, cy as f32, 15.0, t.dim);
                hline(c, y + rh, t.line);
                y += rh;
            }
        }
        Tab::Playlists => {
            let mut y = shuffle_row(c, t, f, yt, "Shuffle a playlist", "RANDOM PLAYLIST · SHUFFLED") + 8;
            let rh = 64;
            for pl in PLAYLISTS {
                let cy = y + rh / 2;
                art::block(c, t, 22, y + (rh - 44) / 2, 44, 44, pl.art, artdim(t));
                text::draw(c, f, 80.0, (cy - 2) as f32, pl.n, &body_label(Family::Sans, Weight::SemiBold, 15.0, t.ink));
                let sub = format!("{} tracks", pl.k);
                text::draw(c, f, 80.0, (cy + 16) as f32, &sub, &body_label(Family::Sans, Weight::Regular, 11.0, t.dim));
                icons::chevron(c, 456.0, cy as f32, 14.0, t.faint);
                hline(c, y + rh, t.line);
                y += rh;
            }
        }
    }
}

/// Artist drill-in page (`CArtist`).
pub fn artist(c: &mut Canvas, t: &Theme, f: &FontSet) {
    c.fill(t.bg);
    crate::chrome::status_bar(c, t, f, "14:32", "FLAC 24/96", 78);
    // back + ARTIST eyebrow
    icons::back(c, 30.0, 110.0, 20.0, t.dim);
    text::draw(c, f, 50.0, 114.0, "ARTIST", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.2));
    // name + stats
    text::draw(c, f, 22.0, 150.0, data::ARTIST_NAME, &sty(Family::Sans, Weight::ExtraBold, 28.0, t.ink, -0.01));
    text::draw(c, f, 22.0, 173.0, data::ARTIST_STATS, &sty(Family::Mono, Weight::Regular, 10.0, t.dim, 0.1));
    let y = shuffle_row(c, t, f, 180, "Shuffle artist", "ALL 34 TRACKS · RANDOM ORDER");

    // ALBUMS · 3
    text::draw(c, f, 22.0, (y + 28) as f32, "ALBUMS · 3", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    let ay = y + 40;
    let cw = (W as i32 - 44 - 24) / 3; // 3 across with 12px gaps
    for (i, al) in data::ARTIST_ALBUMS.iter().enumerate() {
        let ax = 22 + i as i32 * (cw + 12);
        art::block(c, t, ax, ay, cw, cw, al.art, artdim(t));
        let nst = body_label(Family::Sans, Weight::SemiBold, 12.0, t.ink);
        let name = crate::widgets::fit(f, al.n, &nst, cw as f32);
        text::draw(c, f, ax as f32, (ay + cw + 16) as f32, &name, &nst);
        text::draw(c, f, ax as f32, (ay + cw + 32) as f32, al.y, &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.0));
    }

    // TOP SONGS
    let mut sy = ay + cw + 48;
    text::draw(c, f, 22.0, sy as f32, "TOP SONGS", &sty(Family::Mono, Weight::Regular, 9.0, t.faint, 0.18));
    sy += 12;
    let rh = 54;
    for (i, sgn) in data::ARTIST_TOP.iter().enumerate() {
        let cy = sy + rh / 2;
        let now = i == 0;
        let col = if now { t.acc } else { t.faint };
        text::draw(c, f, 22.0, (cy + 4) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 11.0, col, 0.0));
        let tcol = if now { t.acc } else { t.ink };
        text::draw(c, f, 48.0, (cy - 2) as f32, sgn.t, &body_label(Family::Sans, Weight::SemiBold, 14.0, tcol));
        text::draw(c, f, 48.0, (cy + 15) as f32, sgn.al, &body_label(Family::Sans, Weight::Regular, 10.0, t.dim));
        if now {
            tiny_bars(c, 410, cy, t.acc);
        }
        right(c, f, 458.0, (cy + 4) as f32, sgn.d, &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.0));
        hline(c, sy + rh, t.line);
        sy += rh;
    }
}
