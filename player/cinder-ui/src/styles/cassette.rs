#[allow(unused_imports)]
use super::*;

// ── Cassette ──────────────────────────────────────────────────────────────────────────────────
//
// A tape in a deck: the cover block is a cassette whose two reels show how far through the song
// you are (the tape winds from the left reel to the right), the title on its label, and the
// transport as a row of piano keys. Terminal's geometry exactly — `layout` hands it Terminal's —
// so every target is where the contract already proved it. ASCII labels, for the font reason
// Terminal gives.

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout) {
    use super::terminal::term::*;
    c.fill(t.bg);
    let quiet = idle(np);
    let mono = |size: f32, w: Weight, col: Rgb888| s(Family::Mono, w, size, col, 0.06);
    // Centred in the cover block, so the space above and below it is even.
    let (bx, bw, bh) = (FRAME_X, FRAME, 300);
    let by = FRAME_Y + (FRAME - bh) / 2;

    if quiet || np::page_from_index(np.page) == NpPage::Cover {
        // The shell, its label, and the window the reels show through.
        fill_rect(c, bx, by, bw, bh, t.panel);
        stroke_rect(c, bx, by, bw, bh, t.line, 2);
        let (lx, ly, lw, lh) = (bx + 20, by + 16, bw - 40, 96);
        fill_rect(c, lx, ly, lw, lh, t.bg);
        stroke_rect(c, lx, ly, lw, lh, t.line, 1);
        fill_rect(c, lx, ly + 10, lw, 3, t.acc);
        let thumb = lh - 30;
        if !quiet {
            cover(c, t, np, lx + 8, ly + 22, thumb);
        }
        // No cover with no song, so no gap for one.
        let lead = if quiet { 0 } else { thumb };
        let tx = (lx + 16 + lead) as f32;
        let tw = (lw - 24 - lead) as f32;
        let side = mono(11.0, Weight::Regular, t.faint);
        text::draw(
            c,
            f,
            tx,
            (ly + 34) as f32,
            crate::i18n::text("SIDE A"),
            &side,
        );
        if quiet {
            text::draw(
                c,
                f,
                tx,
                (ly + 62) as f32,
                crate::i18n::text("NO TAPE"),
                &mono(18.0, Weight::Bold, t.dim),
            );
        } else {
            let st = mono(18.0, Weight::Bold, t.ink);
            text::draw(
                c,
                f,
                tx,
                (ly + 62) as f32,
                &widgets::fit(f, np.title, &st, tw),
                &st,
            );
            let st = mono(12.0, Weight::Regular, t.dim);
            text::draw(
                c,
                f,
                tx,
                (ly + 82) as f32,
                &widgets::fit(f, np.artist, &st, tw),
                &st,
            );
        }
        // The window, and the tape packed on each reel: all on the left at the start of a song,
        // all on the right at its end. The spokes turn while it plays.
        let (wx, wy, ww, wh) = (bx + 70, by + 132, bw - 140, 104);
        fill_rect(c, wx, wy, ww, wh, t.bg);
        stroke_rect(c, wx, wy, ww, wh, t.line, 1);
        let p = if quiet {
            0.0
        } else {
            np.progress.clamp(0.0, 1.0)
        };
        // A full pack keeps 6 px clear of the window's edges.
        let (rmin, rmax) = (20.0f32, 44.0f32);
        let cy = wy + wh / 2;
        // The tape first, so the packs sit on it: it runs between two guide rollers along the
        // bottom of the window (drawn over the reels it sliced a full pack in half).
        let ty = wy + wh - 9;
        fill_rect(c, wx + 12, ty, ww - 24, 1, t.dim);
        for gx in [wx + 12, wx + ww - 12] {
            widgets::disc(c, gx, ty, 7, t.line);
        }
        let spin = if np.playing && !quiet {
            np.viz_seed
        } else {
            0.0
        };
        for (cx, packed) in [(wx + 62, 1.0 - p), (wx + ww - 62, p)] {
            let r = rmin + (rmax - rmin) * packed;
            widgets::disc(c, cx, cy, (2.0 * r) as u32, t.dim);
            widgets::disc(c, cx, cy, 2 * rmin as u32 - 4, t.panel);
            widgets::disc(c, cx, cy, 18, t.bg);
            for k in 0..6 {
                let a = spin * 2.0 + k as f32 * std::f32::consts::PI / 3.0;
                let (sx, sy) = (cx as f32 + 14.0 * a.cos(), cy as f32 + 14.0 * a.sin());
                widgets::disc(c, sx.round() as i32, sy.round() as i32, 5, t.faint);
            }
        }
        // The head opening.
        let (hx, hy, hw, hh) = (bx + 110, by + bh - 44, bw - 220, 44);
        stroke_rect(c, hx, hy, hw, hh, t.line, 1);
        for x in [hx + 30, hx + hw - 30] {
            widgets::disc(c, x, hy + hh / 2, 10, t.line);
        }
    } else {
        audio_page(c, t, f, np, (FRAME_X, FRAME_Y, FRAME, FRAME));
    }

    // Under the tape: the codec, the title and artist in full, the heart.
    if !quiet {
        let cap = mono(11.0, Weight::Regular, t.faint);
        text::draw(
            c,
            f,
            TEXT_X,
            READOUT_Y,
            &widgets::fit(f, np.codec, &cap, TEXT_W),
            &cap,
        );
        let mut pg = *b"1/3";
        pg[0] = b'1' + (np.page % np::PAGES);
        widgets::right(
            c,
            f,
            456.0,
            READOUT_Y,
            core::str::from_utf8(&pg).unwrap_or(""),
            &cap,
        );
        widgets::marquee(
            c,
            f,
            TEXT_X,
            TITLE_Y,
            np.title,
            &s(Family::Sans, Weight::SemiBold, 24.0, t.ink, 0.0),
            TEXT_W,
        );
        widgets::marquee(
            c,
            f,
            TEXT_X,
            ARTIST_Y,
            np.artist,
            &s(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0),
            TEXT_W,
        );
        let (hx, hy) = centre(l, Hit::Like);
        icons::heart(c, hx, hy, 24.0, if np.liked { t.acc } else { t.faint });
    }

    // The rail: a counter's strip, filled to the position.
    let r = l.rail;
    fill_rect(c, r.x0, r.y + r.h / 2 - 2, r.w, 4, t.line);
    if !quiet {
        let fw = (r.w as f32 * np.progress.clamp(0.0, 1.0)).round() as i32;
        fill_rect(c, r.x0, r.y + r.h / 2 - 2, fw, 4, t.acc);
        fill_rect(c, r.x0 + fw - 3, r.y, 6, r.h, t.ink);
    }
    if np.scrubbing {
        stroke_rect(c, r.x0 - 3, r.y - 3, r.w + 6, r.h + 6, t.acc, 1);
    }
    if !quiet {
        let ts = mono(13.0, Weight::Regular, t.dim);
        text::draw(c, f, r.x0 as f32, TIMES_Y, np.elapsed, &ts);
        widgets::right(c, f, (r.x0 + r.w) as f32, TIMES_Y, np.remaining, &ts);
    }

    // The keys. A key that is on (playing, shuffle, repeat) is pressed down: filled with the accent.
    let key = |c: &mut Canvas, h: Hit, label: &str, down: bool| {
        if let Some(Shape::Rect { x, y, w, h }) = l.at(h) {
            let (x, y, w, h) = (x + 3, y + 3, w - 6, h - 6);
            fill_rect(c, x, y, w, h, if down { t.acc } else { t.panel });
            stroke_rect(c, x, y, w, h, if down { t.acc } else { t.line }, 1);
            if !down {
                fill_rect(c, x, y + h - 4, w, 4, t.line);
            }
            let col = if down {
                t.acc_ink
            } else if quiet {
                t.faint
            } else {
                t.ink
            };
            widgets::center(
                c,
                f,
                (x + w / 2) as f32,
                (y + h / 2 + 5) as f32,
                label,
                &mono(13.0, Weight::Bold, col),
            );
        }
    };
    let playing = np.playing && !quiet;
    key(
        c,
        Hit::PlayPause,
        if playing {
            crate::i18n::text("PAUSE")
        } else {
            crate::i18n::text("PLAY")
        },
        playing,
    );
    key(c, Hit::Prev, crate::i18n::text("REW"), false);
    key(c, Hit::Next, crate::i18n::text("FF"), false);
    key(
        c,
        Hit::Shuffle,
        crate::i18n::text("SHUF"),
        np.shuffle && !quiet,
    );
    let rpt = match np.repeat {
        _ if quiet => crate::i18n::text("RPT"),
        1 => crate::i18n::text("RPT1"),
        np::REPEAT_ALBUM => crate::i18n::text("ALBM"),
        _ => crate::i18n::text("RPT"),
    };
    key(c, Hit::Repeat, rpt, np.repeat > 0 && !quiet);

    hline(c, TOOLBAR_TOP, t.line);
    for (i, label) in ["LIBRARY", "QUEUE", "BT", "SETTINGS"].iter().enumerate() {
        let label = if i == 2 {
            crate::now_playing::button().short()
        } else {
            label
        };
        let (x, y) = centre(l, Hit::Toolbar(i as u8));
        let col = if quiet && i == 0 { t.acc } else { t.dim };
        toolbar_word(
            c,
            f,
            x,
            y + 5.0,
            crate::i18n::text(label),
            &mono(12.0, Weight::Bold, col),
        );
    }
}

/// A cassette in a deck: reels that wind as the song plays, the title on the tape's label,
/// piano keys for the transport. Terminal's geometry.
pub struct Cassette;
impl crate::style::Look for Cassette {
    fn name(&self) -> &'static str {
        "Cassette"
    }
    fn token(&self) -> &'static str {
        "cassette"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("A tape: reels wind as it plays")
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
