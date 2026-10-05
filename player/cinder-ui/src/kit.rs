//! The redesign kit — the "shared anatomy" of the 2026-09 design handoff
//! (`docs/SPEC_redesign_2026-09.md`), as draw helpers every new or restructured screen uses.
//!
//! One row anatomy, one section label, one strip, one switch, one chip set and one primary button.
//! The handoff's point is consistency: seven screens drew seven different right-hand controls, and
//! each screen spelled its own section eyebrow at 10, 11 or 12 px. Everything here is drawn from
//! `Theme` tokens (plus `Theme::ctrl`, which is derived from them), so every palette, accent and
//! night level gets the kit for free.
//!
//! GEOMETRY IS THE HIT TEST'S TOO. Anything a finger can land on here — the chips above all — is
//! laid out by a pure function of its inputs that does not measure text, so a screen's `tap` can
//! ask the same function the render used without holding a `FontSet`. Text is fitted INTO those
//! boxes; it never sizes them. That is the rule that keeps the UI-scale slider from moving targets.

use crate::canvas::{Canvas, W};
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, fit, hline, right, stroke_rect, sty};

/// Section label: 34 px, text bottom-aligned with 6 px under it.
pub const SECTION_H: i32 = 34;
/// The one-line state strip under a header.
pub const STRIP_H: i32 = 40;
/// Every list row in the kit. The same number `scale::SETTING_ROW_H` already names.
pub const ROW_H: i32 = crate::scale::SETTING_ROW_H;
/// Chip height.
pub const CHIP_H: i32 = 44;
/// The switch.
pub const SWITCH_W: i32 = 40;
pub const SWITCH_H: i32 = 22;
/// The primary button: full width less the two 20 px gutters.
pub const BUTTON_H: i32 = 56;
/// Left and right content edges.
pub const LEFT: i32 = 20;
pub const RIGHT: i32 = W as i32 - 20;
/// Gap between chips.
pub const CHIP_GAP: i32 = 8;

/// A section label ("START ON", "THIS DEVICE"), with an optional action on the right in the accent
/// ("UNDO", "PAIR NEW"). Returns the y below it.
pub fn section_label(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    label: &str,
    action: Option<&str>,
) -> i32 {
    let base = (y + SECTION_H - 8) as f32;
    let st = sty(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION,
        t.faint,
        0.16,
    );
    let aw = action.map(|a| {
        let ast = sty(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            t.acc,
            0.14,
        );
        let w = text::measure(f, a, &ast);
        text::draw(c, f, RIGHT as f32 - w, base, a, &ast);
        w + 16.0
    });
    let lw = (RIGHT - LEFT) as f32 - aw.unwrap_or(0.0);
    text::draw(c, f, LEFT as f32, base, &fit(f, label, &st, lw), &st);
    y + SECTION_H
}

/// Where a section label's action sits, for the hit test: the right-hand half of the label band.
/// Generous on purpose — the words are short and the band is only 34 px tall.
pub fn section_action_hit(label_y: i32, x: i32, y: i32) -> bool {
    y >= label_y && y < label_y + SECTION_H && x >= W as i32 / 2
}

/// The strip: one line of state on the panel tone, hairlines above and below. Returns the y below.
pub fn strip(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, line: &str) -> i32 {
    fill_rect(c, 0, y, W as i32, STRIP_H, t.panel);
    hline(c, y, t.line);
    hline(c, y + STRIP_H - 1, t.line);
    let st = sty(
        Family::Mono,
        Weight::Regular,
        crate::scale::CAPTION,
        t.dim,
        0.06,
    );
    let s = fit(f, line, &st, (RIGHT - LEFT) as f32);
    text::draw(c, f, LEFT as f32, (y + STRIP_H / 2 + 5) as f32, &s, &st);
    y + STRIP_H
}

/// The switch: 40 x 22, a 1 px border and a 16 px square knob. On is a filled accent box with the
/// knob in `acc_ink` on the right; off is a `ctrl` outline with a `dim` knob on the left.
///
/// `widgets::toggle` is the older one-off (accent outline, accent knob). New screens use this.
pub fn switch(c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
    const KNOB: i32 = 16;
    let inset = (SWITCH_H - KNOB) / 2;
    if on {
        fill_rect(c, x, y, SWITCH_W, SWITCH_H, t.acc);
        fill_rect(
            c,
            x + SWITCH_W - inset - KNOB,
            y + inset,
            KNOB,
            KNOB,
            t.acc_ink,
        );
    } else {
        stroke_rect(c, x, y, SWITCH_W, SWITCH_H, t.ctrl(), 1);
        fill_rect(c, x + inset, y + inset, KNOB, KNOB, t.dim);
    }
}

/// What sits on the right of a [`Row`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Trail<'a> {
    None,
    /// A mono value.
    Value(&'a str),
    /// A mono value in the accent — a tag the eye should find ("HOME", "EDITED") — and a chevron,
    /// because every tagged row in the handoff also opens something.
    Tag(&'a str),
    /// A mono value and a chevron: this row opens something.
    Open(&'a str),
    /// A switch.
    Switch(bool),
    /// `w` px at the right edge that the SCREEN draws into after the row (a swatch, a meter). The
    /// row draws nothing there, and fits its title and subtitle short of it — so what the screen
    /// adds can never cover the text.
    Reserve(i32),
}

/// One list row of the kit.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub title: &'a str,
    /// Second line; empty for none.
    pub sub: &'a str,
    /// A 26 px mono lead ("1", "≡"), or empty.
    pub lead: &'a str,
    pub trail: Trail<'a>,
    /// The row is the chosen one: `row_sel` wash and the title in the accent.
    pub sel: bool,
}

impl<'a> Row<'a> {
    pub const fn new(title: &'a str) -> Self {
        Row {
            title,
            sub: "",
            lead: "",
            trail: Trail::None,
            sel: false,
        }
    }
    pub const fn sub(mut self, s: &'a str) -> Self {
        self.sub = s;
        self
    }
    pub const fn lead(mut self, s: &'a str) -> Self {
        self.lead = s;
        self
    }
    pub const fn trail(mut self, t: Trail<'a>) -> Self {
        self.trail = t;
        self
    }
    pub const fn sel(mut self, on: bool) -> Self {
        self.sel = on;
        self
    }
}

/// Draw `r` at `y`, `h` tall (normally [`ROW_H`]; a list that must fit the glass may pass less).
/// Returns the y below it.
pub fn row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, h: i32, r: &Row) -> i32 {
    if r.sel {
        fill_rect(c, 0, y, W as i32, h, t.row_sel);
    }
    let cy = y + h / 2;
    let mut x = LEFT as f32;
    if !r.lead.is_empty() {
        let lst = sty(
            Family::Mono,
            Weight::Regular,
            14.0,
            if r.sel { t.acc } else { t.faint },
            0.0,
        );
        text::draw(c, f, x, (cy + 5) as f32, r.lead, &lst);
        x += 26.0 + 14.0;
    }

    // The right-hand side first: it keeps its width and the title is fitted into what is left.
    let vst = |col| {
        sty(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            col,
            0.1,
        )
    };
    let mut right_edge = RIGHT as f32;
    match r.trail {
        Trail::None => {}
        Trail::Value(v) | Trail::Tag(v) | Trail::Open(v) => {
            let chevron = matches!(r.trail, Trail::Open(_) | Trail::Tag(_));
            let vx = if chevron {
                RIGHT as f32 - 20.0
            } else {
                RIGHT as f32
            };
            if chevron {
                icons::chevron(c, RIGHT as f32 - 2.0, cy as f32, 14.0, t.faint);
            }
            let col = if matches!(r.trail, Trail::Tag(_)) {
                t.acc
            } else {
                t.faint
            };
            if !v.is_empty() {
                let st = vst(col);
                // A value never takes more than half the row: it is the answer, the title is the
                // question, and a question cut to nothing makes the answer meaningless.
                let v = fit(f, v, &st, (RIGHT - LEFT) as f32 / 2.0);
                right(c, f, vx, (cy + 5) as f32, &v, &st);
                right_edge = vx - text::measure(f, &v, &st) - 14.0;
            } else {
                right_edge = vx - 6.0;
            }
        }
        Trail::Switch(on) => {
            switch(c, t, RIGHT - SWITCH_W, cy - SWITCH_H / 2, on);
            right_edge = (RIGHT - SWITCH_W - 14) as f32;
        }
        Trail::Reserve(w) => right_edge = (RIGHT - w - 14) as f32,
    }

    let tst = sty(
        Family::Sans,
        Weight::SemiBold,
        crate::scale::ROW,
        if r.sel { t.acc } else { t.ink },
        0.0,
    );
    let avail = (right_edge - x).max(0.0);
    if r.sub.is_empty() {
        text::draw(
            c,
            f,
            x,
            (cy + 7) as f32,
            &fit(f, r.title, &tst, avail),
            &tst,
        );
    } else {
        text::draw(
            c,
            f,
            x,
            (cy - 3) as f32,
            &fit(f, r.title, &tst, avail),
            &tst,
        );
        let sst = sty(
            Family::Sans,
            Weight::Regular,
            crate::scale::SECONDARY,
            t.dim,
            0.0,
        );
        text::draw(c, f, x, (cy + 18) as f32, &fit(f, r.sub, &sst, avail), &sst);
    }
    hline(c, y + h - 1, t.line);
    y + h
}

/// A rating as five stars from `x0`, each `size` px at a `pitch` px step: `rating` filled, the
/// rest outlined. `live` draws a CONTROL — filled stars in the accent — and otherwise a readout,
/// filled in `dim`; the empty ones are `faint` either way. Returns the x after the last star.
#[allow(clippy::too_many_arguments)]
pub fn stars(
    c: &mut Canvas,
    t: &Theme,
    x0: i32,
    cy: i32,
    size: i32,
    pitch: i32,
    rating: u8,
    live: bool,
) -> i32 {
    for i in 0..5 {
        let cx = (x0 + i * pitch + pitch / 2) as f32;
        let on = (i as u8) < rating;
        let col = if on {
            if live {
                t.acc
            } else {
                t.dim
            }
        } else {
            t.faint
        };
        icons::star(c, cx, cy as f32, size as f32, col, on);
    }
    x0 + 5 * pitch
}

/// Which star (1..=5) of a row drawn by [`stars`] from `x0` at `pitch` is under `x`.
pub fn star_at(x0: i32, pitch: i32, x: i32) -> Option<u8> {
    let i = (x - x0).div_euclid(pitch.max(1));
    (0..5).contains(&i).then(|| i as u8 + 1)
}

/// The x span of chip `i` of `n` equal chips across the content width. Equal widths, not widths
/// measured from the labels: this is also the hit test, and it must not depend on the text scale.
pub fn chip_span(i: usize, n: usize) -> (i32, i32) {
    let n = n.max(1) as i32;
    let w = (RIGHT - LEFT - CHIP_GAP * (n - 1)) / n;
    (LEFT + i as i32 * (w + CHIP_GAP), w)
}

/// Which of `n` chips drawn at `y` is under `(x, py)`.
pub fn chip_at(n: usize, y: i32, x: i32, py: i32) -> Option<usize> {
    if py < y || py >= y + CHIP_H {
        return None;
    }
    (0..n).find(|&i| {
        let (cx, w) = chip_span(i, n);
        // Half the gap on each side belongs to the chip, so there is no dead strip between them.
        x >= cx - CHIP_GAP / 2 && x < cx + w + CHIP_GAP / 2
    })
}

/// A row of equal chips at `y`, `sel` filled in the accent. Returns the y below the chips.
pub fn chips(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    labels: &[&str],
    sel: Option<usize>,
) -> i32 {
    for (i, l) in labels.iter().enumerate() {
        let (x, w) = chip_span(i, labels.len());
        let on = sel == Some(i);
        if on {
            fill_rect(c, x, y, w, CHIP_H, t.acc);
        } else {
            stroke_rect(c, x, y, w, CHIP_H, t.ctrl(), 1);
        }
        let st = sty(
            Family::Sans,
            Weight::SemiBold,
            crate::scale::SECONDARY,
            if on { t.acc_ink } else { t.ink },
            0.0,
        );
        let s = fit(f, l, &st, (w - 16) as f32);
        let tw = text::measure(f, &s, &st);
        text::draw(
            c,
            f,
            x as f32 + (w as f32 - tw) / 2.0,
            (y + CHIP_H / 2 + 6) as f32,
            &s,
            &st,
        );
    }
    y + CHIP_H
}

/// The one orange thing on a screen. Returns the y below it.
pub fn primary_button(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, label: &str) -> i32 {
    fill_rect(c, LEFT, y, RIGHT - LEFT, BUTTON_H, t.acc);
    let st = sty(
        Family::Sans,
        Weight::Bold,
        crate::scale::ROW,
        t.acc_ink,
        0.0,
    );
    let s = fit(f, label, &st, (RIGHT - LEFT - 24) as f32);
    let tw = text::measure(f, &s, &st);
    text::draw(
        c,
        f,
        (W as f32 - tw) / 2.0,
        (y + BUTTON_H / 2 + 7) as f32,
        &s,
        &st,
    );
    y + BUTTON_H
}

/// Is `(x, y)` on a primary button drawn at `top`?
pub fn primary_button_hit(top: i32, x: i32, y: i32) -> bool {
    (LEFT..RIGHT).contains(&x) && (top..top + BUTTON_H).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chips tile the content width with nothing left over past the right gutter, and every x
    /// inside the band belongs to exactly one chip.
    #[test]
    fn chips_tile_the_width_and_every_point_hits_one() {
        for n in 1..=5 {
            let (x0, _) = chip_span(0, n);
            let (xl, wl) = chip_span(n - 1, n);
            assert_eq!(x0, LEFT);
            assert!(
                xl + wl <= RIGHT && xl + wl > RIGHT - n as i32,
                "{n} chips end at {}",
                xl + wl
            );
            for x in LEFT..RIGHT {
                assert!(
                    chip_at(n, 100, x, 100 + CHIP_H / 2).is_some(),
                    "{n} chips: x {x} hits nothing"
                );
            }
            assert_eq!(chip_at(n, 100, 240, 99), None, "above the band");
            assert_eq!(chip_at(n, 100, 240, 100 + CHIP_H), None, "below the band");
        }
    }

    /// The chip under the middle of each drawn chip is that chip.
    #[test]
    fn chip_centres_hit_their_own_chip() {
        for n in 1..=5 {
            for i in 0..n {
                let (x, w) = chip_span(i, n);
                assert_eq!(chip_at(n, 0, x + w / 2, CHIP_H / 2), Some(i));
            }
        }
    }

    /// The switch paints inside its box and nowhere else, on and off.
    #[test]
    fn the_switch_stays_in_its_box() {
        let t = Theme::day();
        for on in [false, true] {
            let mut c = Canvas::new();
            c.fill(t.bg);
            switch(&mut c, &t, 100, 100, on);
            let bg = crate::canvas::to_u32(t.bg);
            for y in 0..crate::canvas::H as i32 {
                for x in 0..W as i32 {
                    let inside =
                        (100..100 + SWITCH_W).contains(&x) && (100..100 + SWITCH_H).contains(&y);
                    let px = c.buf[y as usize * W + x as usize];
                    assert!(inside || px == bg, "switch({on}) painted ({x},{y})");
                }
            }
        }
    }

    /// `ctrl` sits between a hairline and faint text, for every accent, day and night — brighter
    /// than the dividers it must not be confused with, and never as loud as text.
    #[test]
    fn ctrl_sits_between_line_and_faint() {
        use embedded_graphics::pixelcolor::RgbColor;
        for a in crate::theme::Accent::ALL {
            for t in [Theme::day_with(a), Theme::night_with(a)] {
                let sum = |c: embedded_graphics::pixelcolor::Rgb888| {
                    c.r() as u32 + c.g() as u32 + c.b() as u32
                };
                assert!(sum(t.ctrl()) > sum(t.line) && sum(t.ctrl()) < sum(t.faint));
            }
        }
    }
}
