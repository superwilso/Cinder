#[allow(unused_imports)]
use super::*;

// ── Terminal ──────────────────────────────────────────────────────────────────────────────────
//
// Retro and blunt: a framed cover, monospace capitals, controls as bracketed words, and a rail of
// cells. ASCII only — a glyph the bundled fonts lack costs the font chain tens of megabytes, and one
// nothing covers kills the app (`reference_font_chain_oom`), so no ► or ░ here.

pub(super) mod term {
    pub const FRAME_X: i32 = 36;
    pub const FRAME_Y: i32 = 96;
    pub const FRAME: i32 = 408;
    pub const ART_INSET: i32 = 6;
    pub const READOUT_Y: f32 = 528.0;
    pub const TITLE_Y: f32 = 562.0;
    pub const ARTIST_Y: f32 = 588.0;
    pub const TEXT_X: f32 = 24.0;
    pub const TEXT_W: f32 = 376.0;
    pub const HEART: (i32, i32, i32) = (436, 554, 26);
    pub const RAIL_X0: i32 = 24;
    pub const RAIL_Y: i32 = 620;
    pub const RAIL_W: i32 = 432;
    pub const CELLS: i32 = 27;
    /// Under the cells, not beside them: beside, "-2:45:09" at 140% ran off the panel.
    pub const TIMES_Y: f32 = 654.0;
    pub const TRANSPORT_Y: i32 = 700;
    pub const TOOLBAR_TOP: i32 = 744;
}

pub fn layout(lyrics: bool) -> Layout {
    use term::*;
    let rail = Rail {
        x0: RAIL_X0,
        y: RAIL_Y,
        w: RAIL_W,
        h: 14,
        grab_top: 608,
        grab_bot: 660,
    };
    let mut l = Layout::new(rail, FRAME_Y, FRAME_Y + FRAME);
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
            y: 508,
            w: hx - hh - 6,
            h: 96,
        },
        Hit::Info,
    );
    let ty = TRANSPORT_Y;
    l.push(
        Shape::Rect {
            x: 172,
            y: ty - 26,
            w: 136,
            h: 52,
        },
        Hit::PlayPause,
    );
    l.push(
        Shape::Rect {
            x: 104,
            y: ty - 22,
            w: 64,
            h: 44,
        },
        Hit::Prev,
    );
    l.push(
        Shape::Rect {
            x: 312,
            y: ty - 22,
            w: 64,
            h: 44,
        },
        Hit::Next,
    );
    l.push(
        Shape::Rect {
            x: 20,
            y: ty - 22,
            w: 76,
            h: 44,
        },
        Hit::Shuffle,
    );
    l.push(
        Shape::Rect {
            x: 384,
            y: ty - 22,
            w: 76,
            h: 44,
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

const TERMINAL_TOOLBAR: [&str; 4] = ["[LIB]", "[QUEUE]", "[BT]", "[SET]"];

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying, l: &Layout) {
    use term::*;
    c.fill(t.bg);
    let quiet = idle(np);
    let mono = |size: f32, w: Weight, col: Rgb888| s(Family::Mono, w, size, col, 0.04);

    // The framed page block.
    stroke_rect(
        c,
        FRAME_X,
        FRAME_Y,
        FRAME,
        FRAME,
        if quiet { t.line } else { t.dim },
        1,
    );
    let (ax, ay, asz) = (
        FRAME_X + ART_INSET,
        FRAME_Y + ART_INSET,
        FRAME - ART_INSET * 2,
    );
    if quiet {
        nothing_playing(c, t, f, 240.0, (FRAME_Y + FRAME / 2) as f32, true);
    } else if np::page_from_index(np.page) == NpPage::Cover {
        cover(c, t, np, ax, ay, asz);
        cover_viz(c, t, np, ax + 8, asz - 16, ay + asz - 6);
    } else {
        audio_page(c, t, f, np, (ax, ay, asz, asz));
    }

    // The readout: where you are, which page, what is playing.
    let cap = mono(11.0, Weight::Regular, t.faint);
    text::draw(
        c,
        f,
        TEXT_X,
        READOUT_Y,
        crate::i18n::text("// NOW PLAYING"),
        &cap,
    );
    if !quiet {
        let mut pg = *b"[1/3]";
        pg[1] = b'1' + (np.page % np::PAGES);
        widgets::center(
            c,
            f,
            240.0,
            READOUT_Y,
            core::str::from_utf8(&pg).unwrap_or(""),
            &cap,
        );
        if !np.badge.is_empty() {
            let st = mono(11.0, Weight::Regular, t.acc);
            let badge = widgets::fit(f, np.badge, &st, 140.0);
            widgets::right(c, f, 456.0, READOUT_Y, &format!("[{badge}]"), &st);
        }
        widgets::marquee(
            c,
            f,
            TEXT_X,
            TITLE_Y,
            &np.title.to_uppercase(),
            &mono(24.0, Weight::Bold, t.ink),
            TEXT_W,
        );
        widgets::marquee(
            c,
            f,
            TEXT_X,
            ARTIST_Y,
            &crate::ui_format!("BY {}", np.artist.to_uppercase()),
            &mono(13.0, Weight::Regular, t.dim),
            TEXT_W,
        );
        let (hx, hy) = centre(l, Hit::Like);
        icons::heart(c, hx, hy, 24.0, if np.liked { t.acc } else { t.faint });
    }

    // The rail: a row of cells, filled to the position, with the times under its two ends.
    let r = l.rail;
    let cell = r.w / CELLS;
    let filled = if quiet {
        0
    } else {
        (np.progress.clamp(0.0, 1.0) * CELLS as f32).round() as i32
    };
    for i in 0..CELLS {
        let col = if i < filled { t.acc } else { t.line };
        fill_rect(c, r.x0 + i * cell, r.y, cell - 2, r.h, col);
    }
    if np.scrubbing {
        stroke_rect(c, r.x0 - 3, r.y - 3, r.w + 4, r.h + 6, t.acc, 1);
    }
    if !quiet {
        let ts = mono(13.0, Weight::Regular, t.dim);
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

    // The transport: bracketed words, Play boxed in the accent.
    let word = |c: &mut Canvas, h: Hit, label: &str, col: Rgb888| {
        let (x, y) = centre(l, h);
        widgets::center(c, f, x, y + 5.0, label, &mono(14.0, Weight::Bold, col));
    };
    let side = if quiet { t.faint } else { t.ink };
    if let Some(Shape::Rect { x, y, w, h }) = l.at(Hit::PlayPause) {
        stroke_rect(c, x + 4, y + 4, w - 8, h - 8, t.acc, 2);
    }
    word(
        c,
        Hit::PlayPause,
        if np.playing && !quiet {
            crate::i18n::text("|| PAUSE")
        } else {
            crate::i18n::text("> PLAY")
        },
        t.acc,
    );
    word(c, Hit::Prev, "[<<]", side);
    word(c, Hit::Next, "[>>]", side);
    word(
        c,
        Hit::Shuffle,
        crate::i18n::text("[SHUF]"),
        if np.shuffle && !quiet { t.acc } else { t.faint },
    );
    let rpt = match np.repeat {
        _ if quiet => crate::i18n::text("[RPT]"),
        1 => crate::i18n::text("[RPT1]"),
        np::REPEAT_ALBUM => crate::i18n::text("[ALBM]"),
        _ => crate::i18n::text("[RPT]"),
    };
    word(
        c,
        Hit::Repeat,
        rpt,
        if np.repeat > 0 && !quiet {
            t.acc
        } else {
            t.faint
        },
    );

    // The toolbar.
    hline(c, TOOLBAR_TOP, t.line);
    for (i, label) in TERMINAL_TOOLBAR.iter().enumerate() {
        let custom;
        let label = if i == 2 {
            custom = format!("[{}]", crate::now_playing::button().short());
            &custom
        } else {
            crate::i18n::text(label)
        };
        let col = if quiet && i == 0 { t.acc } else { t.dim };
        word(c, Hit::Toolbar(i as u8), label, col);
    }
}

/// Retro and blunt: a framed cover, monospace capitals, bracketed text buttons and a rail of
/// cells. Pairs well with an amber palette.
pub struct Terminal;
impl crate::style::Look for Terminal {
    fn name(&self) -> &'static str {
        "Terminal"
    }
    fn token(&self) -> &'static str {
        "terminal"
    }
    fn about(&self) -> &'static str {
        crate::i18n::text("Framed cover, text buttons")
    }
    fn np_layout(&self, _night: bool, lyrics: bool) -> Layout {
        layout(lyrics)
    }
    fn np_render(&self, c: &mut Canvas, t: &Theme, f: &FontSet, np: &NowPlaying) {
        let l = self.np_layout(false, np.lyrics);
        render(c, t, f, np, &l);
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
        kit_section_label(c, t, f, y, label, action)
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
        // Dashes, not the kit's dots: this runs once per row while a list flings (3 px in 6, 64
        // fills against the dotted rule's 240).
        for x in (KL..KR).step_by(6) {
            fill_rect(c, x, y, 3, 1, t.line);
        }
    }
    fn lib_playing(&self, c: &mut Canvas, t: &Theme, x: i32, cy: i32) {
        // A block cursor.
        fill_rect(c, x - 4, cy - 7, 8, 14, t.acc);
    }
    fn lib_type(&self, _fam: Family, _w: Weight) -> (Family, Weight) {
        (Family::Mono, Weight::Regular)
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
        // A key cap, as the header's back mark.
        stroke_rect(c, x, y, w, h, t.acc, 1);
        glyph(c, (x + w / 2) as f32, (y + h / 2) as f32, 20.0, t.acc);
    }
    fn header_title(&self, ink: Rgb888) -> TextStyle {
        s(Family::Mono, Weight::SemiBold, 26.0, ink, 0.0)
    }
    fn back_glyph(&self, c: &mut Canvas, t: &Theme, cx: f32, cy: f32, size: f32) {
        // A key cap: the back mark in a box, like the transport's bracketed buttons.
        let half = (size * 0.7).round() as i32;
        stroke_rect(
            c,
            cx as i32 - half,
            cy as i32 - half,
            2 * half,
            2 * half,
            t.line,
            1,
        );
        icons::back(c, cx, cy, size * 0.8, t.dim);
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

// ── Terminal's kit: the settings screens in the same voice as its Now Playing ─────────────────
// Mono type, dotted rules, brackets for what is chosen. The boxes are the kit's (`kit.rs`), so
// every target stays where Cinder's is.

const KL: i32 = crate::kit::LEFT;
const KR: i32 = crate::kit::RIGHT;

/// `── START ON ──────── UNDO`: the label between two rules.
fn kit_section_label(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    label: &str,
    action: Option<&str>,
) -> i32 {
    // The label moves right by the lead rule's width, so the generic label draws after it.
    let (below, end, rule_end) =
        crate::kit::section_label_in(c, t, f, y, label, action, Family::Mono, (0.16, 0.14));
    let mid = y + crate::kit::SECTION_H - 12;
    fill_rect(c, 2, mid, KL - 6, 1, t.line);
    let x0 = end.round() as i32 + 8;
    let x1 = rule_end.round() as i32 - 8;
    if x1 > x0 {
        fill_rect(c, x0, mid, x1 - x0, 1, t.line);
    }
    below
}

/// Brackets with a block inside: the block on the right in the accent is on, on the left in `dim`
/// is off. The switch's 40 × 22 box.
fn kit_switch(c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
    let (w, h) = (crate::kit::SWITCH_W, crate::kit::SWITCH_H);
    let col = if on { t.acc } else { t.ctrl() };
    for bx in [x, x + w - 2] {
        fill_rect(c, bx, y, 2, h, col);
    }
    fill_rect(c, x, y, 6, 2, col);
    fill_rect(c, x, y + h - 2, 6, 2, col);
    fill_rect(c, x + w - 6, y, 6, 2, col);
    fill_rect(c, x + w - 6, y + h - 2, 6, 2, col);
    let k = 14;
    let ky = y + (h - k) / 2;
    if on {
        fill_rect(c, x + w - 6 - k, ky, k, k, t.acc);
    } else {
        fill_rect(c, x + 6, ky, k, k, t.dim);
    }
}

/// Rows: mono type, a `>` cursor on the chosen one (no wash), dotted rules, `>` where a row opens.
const ROW: crate::kit::RowDress = crate::kit::RowDress {
    title: |t, sel| {
        s(
            Family::Mono,
            Weight::Regular,
            crate::scale::ROW - 1.0,
            if sel { t.acc } else { t.ink },
            0.0,
        )
    },
    sub: |t| {
        s(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            t.dim,
            0.0,
        )
    },
    value: |col| {
        s(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            col,
            0.06,
        )
    },
    sel: |c, t, f, y, h| {
        let st = s(Family::Mono, Weight::Bold, crate::scale::ROW, t.acc, 0.0);
        text::draw(c, f, 4.0, (y + h / 2 + 7) as f32, ">", &st);
    },
    rule: |c, t, y| dotted_h(c, KL, KR, y, t.line),
    open: |c, t, f, x, cy| {
        let st = s(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            t.faint,
            0.0,
        );
        widgets::right(c, f, x, cy + 5.0, ">", &st);
    },
};

/// Chips as a menu: the chosen one `[LIKE THIS]` on the wash, the others plain in a dotted box.
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
    // Two brackets' worth more room than Cinder's padding, so "[" and "]" never cut a label.
    let size = crate::kit::chip_text_size(f, labels, cols, Family::Mono, Weight::Regular, 36);
    for (i, l) in labels.iter().enumerate() {
        let l = crate::i18n::text(l);
        let (x, w) = crate::kit::chip_span(i, cols);
        let on = sel == Some(i);
        let st = s(
            Family::Mono,
            if on {
                Weight::SemiBold
            } else {
                Weight::Regular
            },
            size,
            if on { t.ink } else { t.dim },
            0.0,
        );
        let label = widgets::fit(f, l, &st, (w - 36) as f32);
        let shown = if on { format!("[{label}]") } else { label };
        if on {
            fill_rect(c, x, y, w, h, t.row_sel);
        } else {
            dotted_rect(c, x, y, w, h, t.ctrl());
        }
        widgets::center(
            c,
            f,
            x as f32 + w as f32 / 2.0,
            (y + h / 2 + 5) as f32,
            &shown,
            &st,
        );
    }
    y + h
}

/// `[ PAIR NEW DEVICE ]`: square, filled in the accent, the label bracketed.
fn kit_button(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    let h = crate::kit::BUTTON_H;
    fill_rect(c, KL, y, KR - KL, h, t.acc);
    let st = s(
        Family::Mono,
        Weight::Bold,
        crate::scale::ROW - 1.0,
        t.acc_ink,
        0.04,
    );
    let l = widgets::fit(f, label, &st, (KR - KL - 60) as f32);
    widgets::center(
        c,
        f,
        W as f32 / 2.0,
        (y + h / 2 + 7) as f32,
        &format!("[ {l} ]"),
        &st,
    );
    y + h
}

/// The bar: progress as a row of cells (the seek rail's), `[>]` / `[||]` for play, mono text.
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
    fill_rect(c, x, y, w, h, t.panel);
    dotted_h(c, x, x + w, y, t.line);
    const CELLS: i32 = 48;
    let pitch = w / CELLS;
    let lit = (progress.clamp(0.0, 1.0) * CELLS as f32).round() as i32;
    for i in 0..lit {
        fill_rect(c, x + i * pitch + 1, y + 2, pitch - 2, 3, t.acc);
    }
    let play_w = crate::chrome::NP_BAR_PLAY_W;
    let gy = (y + h / 2 + 6) as f32;
    let pst = s(
        Family::Mono,
        Weight::Bold,
        16.0,
        if playing { t.acc } else { t.dim },
        0.0,
    );
    // Drawn into the play zone, not through `widgets::center`, whose glass margin would leave a
    // 37 px anchor only 34 px and cut "[||]".
    let mark = if playing { "[||]" } else { "[>]" };
    let mw = text::measure(f, mark, &pst);
    text::draw(c, f, (play_w as f32 - mw) / 2.0, gy, mark, &pst);
    for yy in (y + 14..y + h - 14).step_by(2) {
        fill_rect(c, play_w, yy, 1, 1, t.line);
    }
    let cap = s(Family::Mono, Weight::Regular, 11.0, t.faint, 0.16);
    let cap_text = crate::i18n::text("NOW PLAYING");
    let cap_w = text::measure(f, cap_text, &cap);
    text::draw(
        c,
        f,
        W as f32 - 20.0 - cap_w,
        (y + 26) as f32,
        cap_text,
        &cap,
    );
    widgets::right(c, f, W as f32 - 20.0, (y + 48) as f32, "^", &cap);
    let avail = W as f32 - 88.0 - cap_w - 34.0;
    let ts = s(Family::Mono, Weight::SemiBold, 15.0, t.ink, 0.0);
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
        let ast = s(Family::Mono, Weight::Regular, 12.0, t.dim, 0.0);
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

/// The Library's tabs: the active one bracketed in the accent, the strip ruled with dots. The
/// labels stay where Cinder measured them (the hit test reads those); the brackets sit in the
/// gaps between them.
fn lib_tabs(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y0: i32,
    tabs: &[(f32, f32, &str, bool)],
    size: f32,
) -> i32 {
    for &(x, w, label, on) in tabs {
        let st = s(
            Family::Mono,
            if on { Weight::Bold } else { Weight::Regular },
            size,
            if on { t.acc } else { t.faint },
            0.12,
        );
        text::draw(c, f, x, (y0 + 20) as f32, label, &st);
        if on {
            let b = s(Family::Mono, Weight::Regular, size, t.acc, 0.0);
            text::draw(c, f, x - 9.0, (y0 + 20) as f32, "[", &b);
            text::draw(c, f, x + w + 2.0, (y0 + 20) as f32, "]", &b);
        }
    }
    dotted_h(c, 0, W, y0 + 34, t.line);
    y0 + 34
}

/// A band: the accent block of the kit's button, mono type, `>` for play.
fn lib_band(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    (bx, top, bw, h): (i32, i32, i32, i32),
    glyph: crate::library::Glyph,
    label: &str,
    sub: &str,
) -> i32 {
    fill_rect(c, bx, top, bw, h, t.acc);
    let cy = top + h / 2;
    glyph(c, 42.0, cy as f32, 18.0, t.acc_ink);
    let room = (bw - 72) as f32;
    let lst = s(Family::Mono, Weight::Bold, 16.0, t.acc_ink, 0.02);
    let sst = s(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION - 1.0,
        t.acc_ink,
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
    widgets::right(c, f, (bx + bw - 14) as f32, (cy + 6) as f32, ">", &lst);
    top + h
}
