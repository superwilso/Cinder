#[allow(unused_imports)]
use super::*;

// ── Nocturne ──────────────────────────────────────────────────────────────────────────────────
//
// Dark editorial: the cover inset with room around it, the title large and light, one thin rule
// for the rail, and an outlined ring for Play. Quiet on purpose — the accent is the ring, the
// played part of the rail and the codec line, and nothing else.

pub(super) mod noc {
    pub const ART_X: i32 = 40;
    pub const ART_Y: i32 = 96;
    pub const ART: i32 = 400;
    pub const DOTS_Y: i32 = 508;
    pub const KICKER_Y: f32 = 538.0;
    pub const TITLE_Y: f32 = 576.0;
    pub const ARTIST_Y: f32 = 603.0;
    pub const TEXT_X: f32 = 40.0;
    pub const TEXT_W: f32 = 344.0;
    pub const HEART: (i32, i32, i32) = (424, 566, 26);
    pub const RAIL_Y: i32 = 636;
    pub const TIMES_Y: f32 = 661.0;
    pub const TRANSPORT_Y: i32 = 714;
    pub const TOOLBAR_TOP: i32 = 754;
}

pub fn layout(lyrics: bool) -> Layout {
    use noc::*;
    let rail = Rail {
        x0: ART_X,
        y: RAIL_Y,
        w: ART,
        h: 2,
        grab_top: 614,
        grab_bot: 668,
    };
    let mut l = Layout::new(rail, ART_Y, ART_Y + ART);
    push_lyrics(&mut l, lyrics);
    let (hx, hy, hh) = HEART;
    l.push(
        Shape::Rect {
            x: hx - hh,
            y: hy - hh,
            w: 2 * hh + 1,
            h: 2 * hh + 1,
        },
        Hit::Like,
    );
    l.push(
        Shape::Rect {
            x: 0,
            y: 516,
            w: hx - hh - 4,
            h: 94,
        },
        Hit::Info,
    );
    let ty = TRANSPORT_Y;
    l.push(
        Shape::Circle {
            cx: 240,
            cy: ty,
            r: 38,
        },
        Hit::PlayPause,
    );
    l.push(
        Shape::Circle {
            cx: 150,
            cy: ty,
            r: 30,
        },
        Hit::Prev,
    );
    l.push(
        Shape::Circle {
            cx: 330,
            cy: ty,
            r: 30,
        },
        Hit::Next,
    );
    l.push(
        Shape::Circle {
            cx: 64,
            cy: ty,
            r: 24,
        },
        Hit::Shuffle,
    );
    l.push(
        Shape::Circle {
            cx: 416,
            cy: ty,
            r: 24,
        },
        Hit::Repeat,
    );
    for i in 0..4 {
        l.push(
            Shape::Rect {
                x: i * 120,
                y: TOOLBAR_TOP + 1,
                w: 120,
                h: H - TOOLBAR_TOP - 1,
            },
            Hit::Toolbar(i as u8),
        );
    }
    push_menu(&mut l);
    l
}

/// Tracked capitals under each toolbar slot. Words, not icons: this style's one concession to
/// density is to say what each place is.
const TOOLBAR_WORDS: [&str; 4] = ["LIBRARY", "UP NEXT", "BLUETOOTH", "SETTINGS"];

/// Nocturne's Now Playing; `clock` puts the time where the cover was (the Clock style).
pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout, clock: bool) {
    use noc::*;
    c.fill(t.bg);
    let quiet = idle(np);
    let mid = (ART_X + ART / 2) as f32;

    // The page block: the cover, or the audio pages in its place. Clock puts the time there.
    if clock && (quiet || np::page_from_index(np.page) == NpPage::Cover) {
        super::clock::clock_face(c, t, f, np);
    } else if quiet {
        nothing_playing(c, t, f, mid, (ART_Y + ART / 2) as f32, false);
    } else if np::page_from_index(np.page) == NpPage::Cover {
        cover(c, t, np, ART_X, ART_Y, ART);
        cover_viz(c, t, np, ART_X + 12, ART - 24, ART_Y + ART - 8);
    } else {
        audio_page(c, t, f, np, (ART_X, ART_Y, ART, ART));
    }
    if !quiet {
        for i in 0..np::PAGES as i32 {
            let on = i == (np.page % np::PAGES) as i32;
            fill_rect(
                c,
                240 - 22 + i * 16,
                DOTS_Y,
                if on { 12 } else { 4 },
                2,
                if on { t.acc } else { t.faint },
            );
        }
    }

    // The words: codec as a kicker, the title large and light, the artist under it.
    if !quiet {
        let kick = s(Family::Mono, Weight::Regular, 12.0, t.acc, 0.18);
        text::draw(
            c,
            f,
            TEXT_X,
            KICKER_Y,
            &widgets::fit(f, np.codec, &kick, TEXT_W),
            &kick,
        );
        widgets::marquee(
            c,
            f,
            TEXT_X,
            TITLE_Y,
            np.title,
            &s(Family::Sans, Weight::Light, 34.0, t.ink, 0.0),
            TEXT_W,
        );
        widgets::marquee(
            c,
            f,
            TEXT_X,
            ARTIST_Y,
            np.artist,
            &s(Family::Sans, Weight::Regular, 17.0, t.dim, 0.0),
            TEXT_W,
        );
        let (hx, hy) = centre(l, Hit::Like);
        icons::heart(c, hx, hy, 24.0, if np.liked { t.acc } else { t.faint });
    }

    // The rail: one thin rule, the played part in the accent, a short upright at the position.
    let r = l.rail;
    fill_rect(c, r.x0, r.y, r.w, r.h, t.line);
    let fw = (r.w as f32 * np.progress.clamp(0.0, 1.0)) as i32;
    if !quiet {
        fill_rect(c, r.x0, r.y, fw, r.h, t.acc);
        let knob = if np.scrubbing { 18 } else { 10 };
        fill_rect(c, r.x0 + fw - 1, r.y + r.h / 2 - knob / 2, 2, knob, t.acc);
        let ts = s(Family::Mono, Weight::Regular, 12.0, t.dim, 0.06);
        text::draw(c, f, r.x0 as f32, TIMES_Y, np.elapsed, &ts);
        widgets::right(
            c,
            f,
            (r.x0 + r.w) as f32,
            TIMES_Y,
            np.remaining,
            &TextStyle {
                color: t.faint,
                ..ts
            },
        );
    }

    // The transport: plain glyphs either side of an outlined ring.
    let side = if quiet { t.faint } else { t.ink };
    let (px, py) = centre(l, Hit::PlayPause);
    // Big view grows the glyphs and the ring as far as the room between the rail and the toolbar
    // allows; the targets themselves are `now_playing::big_targets`' band.
    let ring = np::g(33.0).min(38.0) as i32;
    for (rad, col) in [(ring, t.acc), (ring - 2, t.bg)] {
        Circle::with_center(Point::new(px as i32, py as i32), (rad * 2) as u32)
            .into_styled(PrimitiveStyle::with_fill(col))
            .draw(c)
            .ok();
    }
    if np.playing && !quiet {
        icons::pause(c, px, py, np::g(26.0), t.acc);
    } else {
        icons::play(c, px + 2.0, py, np::g(26.0), t.acc);
    }
    let (x, y) = centre(l, Hit::Prev);
    icons::prev(c, x, y, np::g(28.0), side);
    let (x, y) = centre(l, Hit::Next);
    icons::next(c, x, y, np::g(28.0), side);
    let (x, y) = centre(l, Hit::Shuffle);
    icons::shuffle(
        c,
        x,
        y,
        np::g(20.0),
        if np.shuffle && !quiet { t.acc } else { t.faint },
    );
    let (x, y) = centre(l, Hit::Repeat);
    icons::repeat(
        c,
        x,
        y,
        np::g(20.0),
        if np.repeat > 0 && !quiet {
            t.acc
        } else {
            t.faint
        },
    );
    if !quiet && np.repeat == 1 {
        fill_rect(c, x as i32 - 1, y as i32 - 1, 3, 3, t.acc);
    } else if !quiet && np.repeat == np::REPEAT_ALBUM {
        widgets::center(
            c,
            f,
            x,
            y + 24.0,
            crate::i18n::text("ALBUM"),
            &s(Family::Mono, Weight::Bold, 11.0, t.acc, 0.1),
        );
    }

    // The toolbar: four words under a hairline.
    hline(c, TOOLBAR_TOP, t.line);
    for (i, word) in TOOLBAR_WORDS.iter().enumerate() {
        let word = if i == 2 {
            crate::now_playing::button().label()
        } else {
            crate::i18n::text(word)
        };
        let (x, _) = centre(l, Hit::Toolbar(i as u8));
        let col = if quiet && i == 0 { t.acc } else { t.dim };
        let st = s(Family::Mono, Weight::Regular, 11.0, col, 0.16);
        toolbar_word(c, f, x, 782.0, word, &st);
    }
}

/// Dark editorial: an inset cover with room around it, large light type, a thin rail and an
/// outlined play ring.
pub struct Nocturne;
impl crate::style::Look for Nocturne {
    fn name(&self) -> &'static str {
        "Nocturne"
    }
    fn token(&self) -> &'static str {
        "nocturne"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("Quiet: inset cover, thin lines")
    }
    fn np_layout(&self, _night: bool, lyrics: bool) -> Layout {
        layout(lyrics)
    }
    fn np_render(&self, c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
        let l = self.np_layout(false, np.lyrics);
        render(c, t, f, np, &l, false);
        finish(c, t, f, np);
    }

    fn section_label(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        label: &str,
        action: Option<&str>,
    ) -> i32 {
        crate::kit::section_label_in(c, t, f, y, label, action, Family::Sans, (0.24, 0.2)).0
    }
    fn switch(&self, c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
        kit_switch(c, t, x, y, on)
    }
    fn row(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y: i32,
        h: i32,
        r: &crate::kit::Row,
    ) -> i32 {
        crate::kit::row_dressed(c, t, f, y, h, r, &ROW)
    }
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
        kit_chips(c, t, f, y, labels, sel, cols)
    }
    fn primary_button(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
        kit_button(c, t, f, y, label)
    }
    fn lib_sel(&self, c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32) {
        (ROW.sel)(c, t, f, y, h)
    }
    fn lib_rule(&self, c: &mut Canvas, t: &Theme, y: i32) {
        (ROW.rule)(c, t, y)
    }
    fn lib_playing(&self, c: &mut Canvas, t: &Theme, x: i32, cy: i32) {
        ring(c, x, cy, 8, t.acc, true);
    }
    fn lib_type(&self, fam: Family, w: Weight) -> (Family, Weight) {
        (
            fam,
            match w {
                Weight::Bold | Weight::SemiBold => Weight::Regular,
                _ => Weight::Light,
            },
        )
    }
    fn lib_tabs(
        &self,
        c: &mut Canvas,
        t: &Theme,
        f: &FontSet,
        y0: i32,
        tabs: &[(f32, f32, &str, bool)],
        size: f32,
    ) -> i32 {
        lib_tabs(c, t, f, y0, tabs, size)
    }
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
        lib_band(c, t, f, rect, glyph, label, sub)
    }
    fn lib_side_button(
        &self,
        c: &mut Canvas,
        t: &Theme,
        (x, y, w, h): (i32, i32, i32, i32),
        glyph: crate::library::Glyph,
    ) {
        ring(c, x + w / 2, y + h / 2, w.min(h) - 4, t.acc, false);
        glyph(c, (x + w / 2) as f32, (y + h / 2) as f32, 18.0, t.acc);
    }
    fn header_title(&self, ink: Rgb888) -> TextStyle {
        s(Family::Sans, Weight::Light, 31.0, ink, -0.01)
    }
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
        kit_np_bar(c, t, f, title, artist, playing, progress)
    }
}

// ── Nocturne's kit: the settings screens as quiet as its Now Playing ────────────────────────
// Light type, rules inset from the glass, rings and pills, the accent only as a thin mark. The
// boxes are the kit's (`kit.rs`), so every target stays where Cinder's is.

const KL: i32 = crate::kit::LEFT;
const KR: i32 = crate::kit::RIGHT;

fn ring(c: &mut Canvas, cx: i32, cy: i32, d: i32, col: Rgb888, fill: bool) {
    let st = if fill {
        PrimitiveStyle::with_fill(col)
    } else {
        PrimitiveStyle::with_stroke(col, 1)
    };
    Circle::with_center(Point::new(cx, cy), d as u32)
        .into_styled(st)
        .draw(c)
        .ok();
}

fn pill(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    use embedded_graphics::primitives::{Rectangle, RoundedRectangle};
    RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32)),
        Size::new((h / 2) as u32, (h / 2) as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(col, 1))
    .draw(c)
    .ok();
}

/// A pill with a round knob: filled in the accent on the right when on, a hollow ring on the
/// left when off. The switch's 40 × 22 box.
fn kit_switch(c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
    let (w, h) = (crate::kit::SWITCH_W, crate::kit::SWITCH_H);
    pill(c, x, y, w, h, if on { t.acc } else { t.ctrl() });
    let cy = y + h / 2;
    if on {
        ring(c, x + w - h / 2, cy, 14, t.acc, true);
    } else {
        ring(c, x + h / 2, cy, 14, t.dim, false);
    }
}

/// Rows: regular weight, a thin accent mark at the left of the chosen one (no wash), a rule inset
/// from both edges.
const ROW: crate::kit::RowDress = crate::kit::RowDress {
    title: |t, sel| {
        s(
            Family::Sans,
            Weight::Regular,
            crate::scale::ROW,
            if sel { t.acc } else { t.ink },
            0.0,
        )
    },
    sub: |t| {
        s(
            Family::Sans,
            Weight::Light,
            crate::scale::SECONDARY,
            t.dim,
            0.0,
        )
    },
    value: |col| {
        s(
            Family::Sans,
            Weight::Regular,
            crate::scale::CAPTION,
            col,
            0.12,
        )
    },
    sel: |c, t, _, y, h| fill_rect(c, 6, y + h / 4, 2, h / 2, t.acc),
    rule: |c, t, y| fill_rect(c, KL, y, KR - KL, 1, t.line),
    open: |c, t, _, x, cy| icons::chevron(c, x - 2.0, cy, 12.0, t.faint),
};

/// Chips as words on a line: the chosen one in `ink` over a 2 px accent underline, the others in
/// `dim` over a hairline. No boxes.
#[allow(clippy::too_many_arguments)]
fn kit_chips(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    labels: &[&str],
    sel: Option<usize>,
    cols: usize,
) -> i32 {
    let h = crate::kit::CHIP_H;
    let size = crate::kit::chip_text_size(f, labels, cols, Family::Sans, Weight::Regular, 12);
    for (i, l) in labels.iter().enumerate() {
        let l = crate::i18n::text(l);
        let (x, w) = crate::kit::chip_span(i, cols);
        let on = sel == Some(i);
        let st = s(
            Family::Sans,
            Weight::Regular,
            size,
            if on { t.ink } else { t.dim },
            0.02,
        );
        let label = widgets::fit(f, l, &st, (w - 12) as f32);
        widgets::center(
            c,
            f,
            x as f32 + w as f32 / 2.0,
            (y + h / 2 + 4) as f32,
            &label,
            &st,
        );
        if on {
            fill_rect(c, x, y + h - 6, w, 2, t.acc);
        } else {
            fill_rect(c, x, y + h - 5, w, 1, t.line);
        }
    }
    y + h
}

/// An outlined pill in the accent with the label in `ink`: the screen's one action, said quietly.
fn kit_button(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    let h = crate::kit::BUTTON_H;
    pill(c, KL, y, KR - KL, h, t.acc);
    let st = s(
        Family::Sans,
        Weight::Regular,
        crate::scale::ROW,
        t.ink,
        0.02,
    );
    let l = widgets::fit(f, label, &st, (KR - KL - 40) as f32);
    widgets::center(c, f, W as f32 / 2.0, (y + h / 2 + 7) as f32, &l, &st);
    y + h
}

/// The bar: the page's own ground, a 1 px progress line, play in an outlined ring as on Now
/// Playing, light type.
#[allow(clippy::too_many_arguments)]
fn kit_np_bar(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    title: &str,
    artist: &str,
    playing: bool,
    progress: f32,
) {
    let (x, y, w, h) = crate::chrome::np_bar_rect();
    fill_rect(c, x, y, w, h, t.bg);
    fill_rect(c, x, y, w, 1, t.line);
    let fillw = (w as f32 * progress.clamp(0.0, 1.0)) as i32;
    if fillw > 0 {
        fill_rect(c, x, y, fillw, 1, t.acc);
    }
    let cy = y + h / 2;
    let col = if playing { t.acc } else { t.dim };
    ring(c, 37, cy, 40, col, false);
    if playing {
        icons::pause(c, 37.0, cy as f32, 14.0, col);
    } else {
        icons::play(c, 38.0, cy as f32, 14.0, col);
    }
    let cap = s(Family::Sans, Weight::Regular, 11.0, t.faint, 0.24);
    let cap_text = crate::i18n::text("NOW PLAYING");
    let cap_w = text::measure(f, cap_text, &cap);
    text::draw(
        c,
        f,
        W as f32 - 34.0 - cap_w,
        (y + 26) as f32,
        cap_text,
        &cap,
    );
    icons::chevron_up(c, W as f32 - 20.0, (y + 46) as f32, 10.0, t.faint);
    let avail = W as f32 - 88.0 - cap_w - 34.0;
    let ts = s(Family::Sans, Weight::Regular, 17.0, t.ink, 0.0);
    let title = if title.is_empty() {
        crate::i18n::text("Nothing playing")
    } else {
        title
    };
    text::draw(
        c,
        f,
        90.0,
        (y + 28) as f32,
        &widgets::fit(f, title, &ts, avail),
        &ts,
    );
    if !artist.is_empty() {
        let ast = s(Family::Sans, Weight::Light, 13.0, t.dim, 0.0);
        text::draw(
            c,
            f,
            90.0,
            (y + 48) as f32,
            &widgets::fit(f, artist, &ast, avail),
            &ast,
        );
    }
}

/// The Library's tabs: light sans labels, the active one in ink over a 1 px accent line its own
/// width, an inset rule under the strip.
fn lib_tabs(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y0: i32,
    tabs: &[(f32, f32, &str, bool)],
    size: f32,
) -> i32 {
    for &(x, _, label, on) in tabs {
        let st = s(
            Family::Sans,
            if on { Weight::Regular } else { Weight::Light },
            size + 1.0,
            if on { t.ink } else { t.dim },
            0.06,
        );
        text::draw(c, f, x, (y0 + 20) as f32, label, &st);
        if on {
            let w = text::measure(f, label, &st);
            fill_rect(c, x as i32, y0 + 30, w as i32, 1, t.acc);
        }
    }
    fill_rect(c, KL, y0 + 34, KR - KL, 1, t.line);
    y0 + 34
}

/// A band: the kit's outlined pill, light type, the glyph and play mark in the accent.
fn lib_band(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    (bx, top, bw, h): (i32, i32, i32, i32),
    glyph: crate::library::Glyph,
    label: &str,
    sub: &str,
) -> i32 {
    pill(c, bx, top, bw, h, t.acc);
    let cy = top + h / 2;
    glyph(c, 42.0, cy as f32, 18.0, t.acc);
    let room = (bw - 72) as f32;
    let lst = s(Family::Sans, Weight::Regular, 18.0, t.ink, 0.0);
    let sst = s(
        Family::Sans,
        Weight::Light,
        crate::scale::CAPTION,
        t.dim,
        0.0,
    );
    text::draw(
        c,
        f,
        64.0,
        (cy - 4) as f32,
        &widgets::fit(f, label, &lst, room),
        &lst,
    );
    text::draw(
        c,
        f,
        64.0,
        (cy + 14) as f32,
        &widgets::fit(f, sub, &sst, room),
        &sst,
    );
    icons::play(c, (bx + bw - 22) as f32, cy as f32, 16.0, t.acc);
    top + h
}
