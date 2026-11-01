//! Settings ▸ Display — how the screen looks (design handoff 5k).
//!
//! Everything about colour, the volume readout and text size, on one page that fits the glass:
//!
//!   COLOUR   Palette ›   Accent (six swatches)   Night (switch)
//!   VOLUME   Full | Minimal
//!   TEXT     Size (slider)   Visualiser ›
//!   STYLE    Cinder | Nocturne | Terminal   (how Now Playing is laid out: `style.rs`)
//!            Cassette | Clock | Winamp
//!
//! These five rows were the top of the Settings list, where they shared one scrolling column with
//! Restart and Reset. The handoff's point 4 is that settings you change weekly and settings you
//! change once should not share a list; Settings now carries one "Display" row that says what is
//! behind it.
//!
//! Two deliberate departures from the mock, both keeping a control that is better than the row it
//! was drawn as: Accent keeps its six swatches (the mock draws a value and a chevron, but no picker
//! screen was designed, and six direct targets beat a list you open to choose from six), and Size
//! keeps its slider (the same reasoning — the mock's chevron leads nowhere yet).
//!
//! The layout is a fixed table: `row_top` is the single source both `render` and the hit tests
//! read, in page coordinates. The page fits the glass at every text size; Big view's taller rows
//! do not, so then it scrolls, and `nav` adds the scroll to a tap's y before asking.

use crate::canvas::{Canvas, W};
use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::{Accent, Theme};
use crate::widgets::{fill_rect, right, stroke_rect, sty};

pub const ROW_PALETTE: usize = 0;
pub const ROW_ACCENT: usize = 1;
/// Night: Off | On | Auto, as chips (`NIGHT_MODES`). Auto is a schedule, like a phone's night mode
/// (the owner, 2026-11-01): Night comes on at Starts and goes off at Ends, and switching it by hand
/// in between holds until the next of the two. Starts and Ends are shown only under Auto.
pub const ROW_NIGHT: usize = 2;
pub const ROW_NIGHT_FROM: usize = 3;
pub const ROW_NIGHT_UNTIL: usize = 4;
/// The volume HUD chips (Full | Minimal). A row for the cursor's sake; the chips are the targets.
pub const ROW_VOLUME: usize = 5;
pub const ROW_SIZE: usize = 6;
pub const ROW_VIZ: usize = 7;
/// The design styles (`style::Style`), as chips. Like Volume, the chips are the targets.
pub const ROW_STYLE: usize = 8;
pub const ROWS: usize = 9;

/// Night's chips, in order. Index = `App::night_mode`.
pub const NIGHT_MODES: [&str; 3] = ["Off", "On", "Auto"];

// Whether Starts and Ends are on the page: the navigator says, whenever Auto is picked or dropped.
// Per test thread, like `scale::large`, so tests that pick Auto cannot move each other's rows.
#[cfg(not(test))]
static SCHEDULE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
thread_local! {
    static SCHEDULE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_schedule_shown(on: bool) {
    #[cfg(not(test))]
    SCHEDULE.store(on, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    SCHEDULE.with(|s| s.set(on));
}

fn schedule_shown() -> bool {
    #[cfg(not(test))]
    return SCHEDULE.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return SCHEDULE.with(|s| s.get());
}

/// Is row `r` on the page? Starts and Ends only under Auto.
pub fn shown(r: usize) -> bool {
    !matches!(r, ROW_NIGHT_FROM | ROW_NIGHT_UNTIL) || schedule_shown()
}

/// Top of the night chips, centred in their row.
fn night_chips_top() -> i32 {
    row_top(ROW_NIGHT) + (kit::ROW_H() - kit::CHIP_H) / 2
}

/// Which night chip is under `(x, y)`, as an index into `NIGHT_MODES`.
pub fn night_chip_at(x: i32, y: i32) -> Option<usize> {
    kit::chip_at(NIGHT_MODES.len(), night_chips_top(), x, y)
}

/// The times Starts steps through, in minutes after midnight: 18:00 to 23:30, every half hour.
pub const NIGHT_STARTS: [u16; 12] = [
    1080, 1110, 1140, 1170, 1200, 1230, 1260, 1290, 1320, 1350, 1380, 1410,
];
/// The times Ends steps through: 05:00 to 09:00, every half hour.
pub const NIGHT_ENDS: [u16; 9] = [300, 330, 360, 390, 420, 450, 480, 510, 540];
/// 21:00 to 07:00 until changed.
pub const NIGHT_FROM_DEFAULT: u16 = 1260;
pub const NIGHT_UNTIL_DEFAULT: u16 = 420;

/// The preset after `now` in `list` (round to the start); a time not in the list goes to the first.
pub fn next_time(list: &[u16], now: u16) -> u16 {
    let i = list
        .iter()
        .position(|&t| t == now)
        .map_or(0, |i| (i + 1) % list.len());
    list[i]
}

/// Is `minute` (after midnight) inside the night that runs from `from` to `until`? Crosses midnight
/// when `from` is later than `until`, which is every preset pair.
pub fn in_night_window(minute: u16, from: u16, until: u16) -> bool {
    if from <= until {
        (from..until).contains(&minute)
    } else {
        minute >= from || minute < until
    }
}

pub fn hhmm(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// The volume HUD styles, in chip order. Index = `App::volume_hud`.
pub const VOLUME_HUDS: [&str; 2] = ["Full", "Minimal"];

/// Section labels and the rows under each. The ONE statement of the layout.
///
/// The Style label names what a style redraws so far, so nobody picks Terminal and wonders why the
/// Library did not change. Part of the label, not its right-hand action: that slot is drawn in the
/// accent, and the accent means "tap here".
const SECTIONS: [(&str, usize); 5] = [
    ("COLOUR", 2),
    ("NIGHT", 3),
    ("VOLUME", 1),
    ("TEXT", 2),
    ("STYLE · NOW PLAYING", 1),
];

/// Screen-y of the top of row `r`.
pub fn row_top(r: usize) -> i32 {
    let mut y = crate::chrome::HEADER_BOTTOM;
    let mut i = 0;
    for (_, n) in SECTIONS {
        y += kit::SECTION_H;
        for _ in 0..n {
            if i == r {
                return y;
            }
            if shown(i) {
                y += kit::ROW_H();
            }
            i += 1;
        }
    }
    y
}

/// The row under `y`, if any.
pub fn row_at(y: i32) -> Option<usize> {
    let h = |r| {
        if r == ROW_STYLE {
            style_row_h()
        } else {
            kit::ROW_H()
        }
    };
    (0..ROWS).find(|&r| shown(r) && (row_top(r)..row_top(r) + h(r)).contains(&y))
}

/// Top of the volume chips, centred in their row.
fn chips_top() -> i32 {
    row_top(ROW_VOLUME) + (kit::ROW_H() - kit::CHIP_H) / 2
}

/// Which volume HUD chip is under `(x, y)`.
pub fn volume_chip_at(x: i32, y: i32) -> Option<usize> {
    kit::chip_at(VOLUME_HUDS.len(), chips_top(), x, y)
}

fn style_chips_top() -> i32 {
    row_top(ROW_STYLE) + (kit::ROW_H() - kit::CHIP_H) / 2
}

/// The styles sit three to a line: six across cut every name but "Clock" at any text size.
const STYLE_COLS: usize = 3;

/// Top of the style chips' line `line`.
fn style_line_top(line: usize) -> i32 {
    style_chips_top() + line as i32 * (kit::CHIP_H + kit::CHIP_GAP)
}

/// The Style row's height: one row, plus a line of chips for every line after the first. The last
/// row on the page, so nothing under it moves.
fn style_row_h() -> i32 {
    let lines = crate::style::Style::COUNT.div_ceil(STYLE_COLS) as i32;
    kit::ROW_H() + (lines - 1) * (kit::CHIP_H + kit::CHIP_GAP)
}

/// The centre of style chip `i`, for tests that tap it.
#[cfg(test)]
pub(crate) fn style_chip_centre(i: usize) -> (i32, i32) {
    let (x, w) = kit::chip_span(i % STYLE_COLS, STYLE_COLS);
    (x + w / 2, style_line_top(i / STYLE_COLS) + kit::CHIP_H / 2)
}

/// Which style chip is under `(x, y)`, as an index into `Style::ALL`.
pub fn style_chip_at(x: i32, y: i32) -> Option<usize> {
    let n = crate::style::Style::COUNT;
    (0..n.div_ceil(STYLE_COLS)).find_map(|line| {
        kit::chip_at(STYLE_COLS, style_line_top(line), x, y)
            .map(|i| line * STYLE_COLS + i)
            .filter(|&i| i < n)
    })
}

// ── Accent swatches ────────────────────────────────────────────────────────────────────────────
const SW: i32 = 30;
const SW_GAP: i32 = 6;
fn swatch_x(i: usize) -> i32 {
    let total = Accent::COUNT as i32 * SW + (Accent::COUNT as i32 - 1) * SW_GAP;
    kit::RIGHT - total + i as i32 * (SW + SW_GAP)
}

/// Which accent swatch is under `(x, y)`. The whole row height counts vertically, and half the gap
/// on each side counts as the swatch, so a near miss still lands on the colour it was aimed at.
pub fn accent_hit(x: i32, y: i32) -> Option<usize> {
    let top = row_top(ROW_ACCENT);
    if !(top..top + kit::ROW_H()).contains(&y) {
        return None;
    }
    (0..Accent::COUNT).find(|&i| {
        let sx = swatch_x(i);
        x >= sx - SW_GAP / 2 && x < sx + SW + SW_GAP / 2
    })
}

// ── Size slider ────────────────────────────────────────────────────────────────────────────────
// Tap a stop or drag; the readout is drawn at a CONSTANT pixel size because this is the control
// that sets the scale (see the note in `render`).
const SLIDER_X0: i32 = 176;
const SLIDER_W: i32 = 196;

/// Map an x on the Size row to a `text::SCALE_STEPS` index, clamped at both ends.
pub fn size_idx_at(x: i32) -> usize {
    let n = text::SCALE_STEPS.len() as i32;
    let dx = (x - SLIDER_X0).clamp(0, SLIDER_W);
    ((dx * (n - 1) * 2 + SLIDER_W) / (SLIDER_W * 2)).clamp(0, n - 1) as usize
}

/// What the page shows. Built by `nav`.
pub struct DisplayView<'a> {
    /// The palette being drawn, by name.
    pub palette: &'a str,
    /// The palette brings its own accent, so the swatches would pick nothing.
    pub accent_locked: bool,
    pub accent: Accent,
    pub night: bool,
    /// Night's schedule: on, and its two times in minutes after midnight.
    pub night_sched: bool,
    pub night_from: u16,
    pub night_until: u16,
    /// Index into [`VOLUME_HUDS`].
    pub volume_hud: u8,
    /// "BARS · VEIL".
    pub viz: &'a str,
    pub style: crate::style::Style,
}

/// How far the page scrolls: 0 unless Big view's rows push the Style chips off the glass. The
/// 8 px margin under the last row is only for a page that scrolls.
pub fn max_scroll_px() -> i32 {
    let bottom = row_top(ROW_STYLE) + style_row_h();
    if bottom <= crate::canvas::H as i32 {
        0
    } else {
        bottom + 8 - crate::canvas::H as i32
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, v: &DisplayView, scroll: i32) {
    c.fill(t.bg);
    let head = crate::chrome::header(c, t, f, crate::i18n::text("Display"), None);
    c.set_clip_y(head, crate::canvas::H as i32);
    let mut y = head - scroll;
    let mut r = 0usize;
    for (label, n) in SECTIONS {
        y = kit::section_label(c, t, f, y, label, None);
        for _ in 0..n {
            if !shown(r) {
                r += 1;
                continue;
            }
            debug_assert_eq!(
                y + scroll,
                row_top(r),
                "display row {r} drifted from its hit test"
            );
            y = draw_row(c, t, f, y, r, sel == r, v);
            r += 1;
        }
    }
    c.clear_clip();
}

fn draw_row(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    r: usize,
    sel: bool,
    v: &DisplayView,
) -> i32 {
    match r {
        ROW_PALETTE => {
            // The palette's own colours beside its name: background, line, dim, ink, accent — the
            // five a palette file actually changes, as 10 x 18 cells.
            let cells = [t.bg, t.line, t.dim, t.ink, t.acc];
            let swatch_w = cells.len() as i32 * 10;
            // The name is fitted HERE, once, with room left for the swatch, and the swatch is
            // placed from that same string. Positioning it from the unfitted name put it under the
            // title for a wide name (16 CJK characters at 140%), because the row shortens the
            // value it is given but nobody told the swatch.
            let vst = sty(
                Family::Mono,
                Weight::Regular,
                crate::scale::CAPTION,
                t.faint,
                0.1,
            );
            let room = (kit::RIGHT - kit::LEFT) as f32 / 2.0 - (swatch_w + 14) as f32;
            let name = crate::widgets::fit(f, &v.palette.to_uppercase(), &vst, room);
            let next = kit::row(
                c,
                t,
                f,
                y,
                kit::ROW_H(),
                &Row::new(crate::i18n::text("Palette"))
                    .trail(Trail::Open(&name))
                    .sel(sel),
            );
            let vx = kit::RIGHT - 20 - text::measure(f, &name, &vst) as i32 - 14;
            let x0 = vx - swatch_w;
            let cy = y + kit::ROW_H() / 2;
            for (i, col) in cells.iter().enumerate() {
                fill_rect(c, x0 + i as i32 * 10, cy - 9, 10, 18, *col);
            }
            stroke_rect(c, x0, cy - 9, cells.len() as i32 * 10, 18, t.ctrl(), 1);
            next
        }
        ROW_ACCENT => {
            if v.accent_locked {
                return kit::row(
                    c,
                    t,
                    f,
                    y,
                    kit::ROW_H(),
                    &Row::new(crate::i18n::text("Accent"))
                        .sub(crate::i18n::text("Set by the palette"))
                        .trail(Trail::Value("—"))
                        .sel(sel),
                );
            }
            // "Amber", not the chip-case "AMBER": it is a second line of prose, not a value.
            let upper = v.accent.name();
            let mut name = upper[..1].to_string();
            name.push_str(&upper[1..].to_lowercase());
            let swatches = kit::RIGHT - swatch_x(0);
            let next = kit::row(
                c,
                t,
                f,
                y,
                kit::ROW_H(),
                &Row::new(crate::i18n::text("Accent"))
                    .sub(&name)
                    .trail(Trail::Reserve(swatches))
                    .sel(sel),
            );
            let cy = y + kit::ROW_H() / 2;
            for (i, a) in Accent::ALL.iter().enumerate() {
                let sx = swatch_x(i);
                let sy = cy - SW / 2;
                // Through the theme's dim: a swatch is a raw palette entry, not a theme colour.
                fill_rect(c, sx, sy, SW, SW, t.scale_color(a.swatch(t.night)));
                if *a == v.accent {
                    // Ink, not accent: on BONE the swatch already is near-ink.
                    stroke_rect(c, sx - 3, sy - 3, SW + 6, SW + 6, t.ink, 2);
                }
            }
            next
        }
        ROW_NIGHT_FROM | ROW_NIGHT_UNTIL => {
            let (title, at) = if r == ROW_NIGHT_FROM {
                (crate::i18n::text("Starts"), v.night_from)
            } else {
                (crate::i18n::text("Ends"), v.night_until)
            };
            let at = hhmm(at);
            kit::row(
                c,
                t,
                f,
                y,
                kit::ROW_H(),
                &Row::new(title).trail(Trail::Value(&at)).sel(sel),
            )
        }
        ROW_NIGHT => {
            if sel {
                fill_rect(c, 0, y, W as i32, kit::ROW_H(), t.row_sel);
            }
            let mode = if v.night_sched {
                2
            } else {
                usize::from(v.night)
            };
            let labels: Vec<&str> = NIGHT_MODES.iter().map(|m| crate::i18n::text(m)).collect();
            kit::chips(
                c,
                t,
                f,
                night_chips_top() - row_top(ROW_NIGHT) + y,
                &labels,
                Some(mode),
            );
            crate::widgets::hline(c, y + kit::ROW_H() - 1, t.line);
            y + kit::ROW_H()
        }
        ROW_VOLUME => {
            if sel {
                fill_rect(c, 0, y, W as i32, kit::ROW_H(), t.row_sel);
            }
            kit::chips(
                c,
                t,
                f,
                // `y` is where the row is on screen; `chips_top` is where it is on the page.
                chips_top() - row_top(ROW_VOLUME) + y,
                &VOLUME_HUDS,
                Some(v.volume_hud as usize),
            );
            crate::widgets::hline(c, y + kit::ROW_H() - 1, t.line);
            y + kit::ROW_H()
        }
        ROW_SIZE => size_row(c, t, f, y, sel),
        ROW_STYLE => {
            let h = style_row_h();
            if sel {
                fill_rect(c, 0, y, W as i32, h, t.row_sel);
            }
            let names: Vec<&str> = crate::style::Style::ALL.iter().map(|s| s.name()).collect();
            for (line, chunk) in names.chunks(STYLE_COLS).enumerate() {
                let on = v.style.index().checked_sub(line * STYLE_COLS);
                let on = on.filter(|&i| i < chunk.len());
                let top = style_line_top(line) - row_top(ROW_STYLE) + y;
                kit::chips_in(c, t, f, top, chunk, on, STYLE_COLS);
            }
            crate::widgets::hline(c, y + h - 1, t.line);
            y + h
        }
        _ => kit::row(
            c,
            t,
            f,
            y,
            kit::ROW_H(),
            &Row::new(crate::i18n::text("Visualiser"))
                .trail(Trail::Open(v.viz))
                .sel(sel),
        ),
    }
}

fn size_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, sel: bool) -> i32 {
    let next = kit::row(
        c,
        t,
        f,
        y,
        kit::ROW_H(),
        &Row::new(crate::i18n::text("Size")).sel(sel),
    );
    let cy = y + kit::ROW_H() / 2;
    let n = text::SCALE_STEPS.len() as i32;
    let idx = text::scale_idx() as i32;
    fill_rect(c, SLIDER_X0, cy - 1, SLIDER_W, 2, t.line);
    for i in 0..n {
        let x = SLIDER_X0 + i * SLIDER_W / (n - 1);
        fill_rect(
            c,
            x - 1,
            cy - 4,
            2,
            8,
            if i <= idx { t.acc } else { t.line },
        );
    }
    let kx = SLIDER_X0 + idx * SLIDER_W / (n - 1);
    fill_rect(c, SLIDER_X0, cy - 1, kx - SLIDER_X0, 2, t.acc);
    fill_rect(c, kx - 7, cy - 9, 14, 18, t.acc);
    // A CONSTANT pixel size: this is the control that sets the scale, so letting the readout grow
    // with it would crowd the knob at 140% and move the slider's own geometry.
    let unscaled = 14.0 * 100.0 / text::scale_pct() as f32;
    right(
        c,
        f,
        kit::RIGHT as f32,
        (cy + 5) as f32,
        &format!("{}%", text::scale_pct()),
        &sty(Family::Mono, Weight::Regular, unscaled, t.faint, 0.04),
    );
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page fits above the Now Playing bar without scrolling.
    #[test]
    fn the_page_fits_above_the_now_playing_bar() {
        let bottom = row_top(ROWS - 1) + kit::ROW_H();
        assert!(
            bottom <= crate::H as i32 - crate::chrome::NP_BAR_H,
            "Display runs to {bottom}"
        );
    }

    /// Every row's middle is that row, and the gaps (section labels) are nobody's.
    #[test]
    fn every_row_is_hittable_and_labels_are_not() {
        for auto in [false, true] {
            set_schedule_shown(auto);
            for r in (0..ROWS).filter(|&r| shown(r)) {
                assert_eq!(
                    row_at(row_top(r) + kit::ROW_H() / 2),
                    Some(r),
                    "auto={auto}"
                );
            }
        }
        set_schedule_shown(false);
        assert_eq!(
            row_at(crate::chrome::HEADER_BOTTOM + 5),
            None,
            "the first section label"
        );
    }

    /// Every swatch is hittable at its centre and picks itself.
    #[test]
    fn swatches_pick_themselves() {
        let cy = row_top(ROW_ACCENT) + kit::ROW_H() / 2;
        for i in 0..Accent::COUNT {
            assert_eq!(accent_hit(swatch_x(i) + SW / 2, cy), Some(i));
        }
        assert_eq!(accent_hit(40, cy), None, "the title is not a swatch");
    }

    /// The style chips sit inside their row, one per style, and each picks itself.
    #[test]
    fn the_style_chips_are_inside_their_row_and_pick_themselves() {
        let top = row_top(ROW_STYLE);
        let n = crate::style::Style::COUNT;
        let last = style_line_top((n - 1) / STYLE_COLS);
        assert!(style_chips_top() >= top && last + kit::CHIP_H <= top + style_row_h());
        assert!(
            top + style_row_h() <= crate::canvas::H as i32,
            "the page does not scroll"
        );
        assert_eq!(max_scroll_px(), 0);
        let picks: Vec<usize> = (0..n)
            .filter_map(|i| {
                let (x, y) = style_chip_centre(i);
                style_chip_at(x, y)
            })
            .collect();
        assert_eq!(picks, (0..n).collect::<Vec<_>>());
        assert_eq!(
            style_chip_at(240, top - 5),
            None,
            "the section label is not a chip"
        );
    }

    /// The chips sit inside the Volume row.
    #[test]
    fn the_volume_chips_are_inside_their_row() {
        let top = row_top(ROW_VOLUME);
        assert!(chips_top() >= top && chips_top() + kit::CHIP_H <= top + kit::ROW_H());
        assert_eq!(volume_chip_at(60, chips_top() + 10), Some(0));
        assert_eq!(volume_chip_at(420, chips_top() + 10), Some(1));
    }

    /// The slider pins to its ends and lands on every stop.
    #[test]
    fn the_size_slider_covers_every_stop() {
        let n = text::SCALE_STEPS.len();
        assert_eq!(size_idx_at(0), 0);
        assert_eq!(size_idx_at(W as i32), n - 1);
        for i in 0..n {
            let x = SLIDER_X0 + i as i32 * SLIDER_W / (n as i32 - 1);
            assert_eq!(size_idx_at(x), i);
        }
    }
}
