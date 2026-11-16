//! Settings ▸ Power ▸ Alarm: wake and play at a time of day. Two ± rows (hour, minute) laid out
//! exactly as the clock editor's first two (`clockset`, whose geometry and hit tests this reuses),
//! and its big button to turn the alarm on or off.
//!
//! The shell does the rest: it arms the clock chip's wake alarm through `cinder-clock alarm` (so it
//! fires from suspend to RAM too), and at the time wakes the screen and plays what was playing.

use crate::clockset::{btn_rect, fields_top, set_rect, ROW_H};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, right, stroke_rect, sty};
use crate::Canvas;

/// Minutes after midnight when there is none saved: 07:00.
pub const DEFAULT_MIN: u16 = 7 * 60;
/// One press of the minute row: alarms are set to the five minutes, and 12 presses is an hour.
pub const MIN_STEP: i32 = 5;

/// The row a tap's ± landed on: 0 hour, 1 minute, with the step.
pub fn hit_btn(x: i32, y: i32) -> Option<(usize, i32)> {
    crate::clockset::hit_btn(x, y).filter(|(f, _)| *f < 2)
}

pub fn hit_button(x: i32, y: i32) -> bool {
    crate::clockset::hit_set(x, y)
}

/// `min` moved by one press of row `row` (0 hour, 1 minute), wrapping round the day.
pub fn step(min: u16, row: usize, delta: i32) -> u16 {
    let by = if row == 0 {
        60 * delta
    } else {
        MIN_STEP * delta
    };
    (min as i32 + by).rem_euclid(24 * 60) as u16
}

pub fn hhmm(min: u16) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

/// "07:30" from the settings file, or `None`.
pub fn parse_hhmm(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, min: u16, on: bool, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, crate::i18n::text("Alarm"), None);
    center(
        c,
        f,
        240.0,
        (y0 + 36) as f32,
        &if on {
            crate::ui_format!("ON · PLAYS AT {time}", time = hhmm(min))
        } else {
            crate::i18n::text("OFF").to_string()
        },
        &sty(
            Family::Sans,
            Weight::SemiBold,
            crate::scale::ROW,
            t.acc,
            0.0,
        ),
    );
    center(
        c,
        f,
        240.0,
        (y0 + 54) as f32,
        crate::i18n::text("By the clock in the top bar"),
        &crate::kit::hint_style(t),
    );
    let ft = fields_top();
    crate::widgets::hline(c, ft, t.line);
    for (i, (label, value)) in [
        (crate::i18n::text("Hour"), format!("{:02}", min / 60)),
        (crate::i18n::text("Minute"), format!("{:02}", min % 60)),
    ]
    .into_iter()
    .enumerate()
    {
        let ry = ft + i as i32 * ROW_H;
        let cy = ry + ROW_H / 2;
        if i == sel {
            fill_rect(c, 0, ry, crate::canvas::W as i32, ROW_H, t.row_sel);
        }
        let lc = if i == sel { t.acc } else { t.ink };
        text::draw(
            c,
            f,
            22.0,
            (cy + 6) as f32,
            label,
            &sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, lc, 0.0),
        );
        right(
            c,
            f,
            (crate::clockset::MINUS_X - 16) as f32,
            (cy + 7) as f32,
            &value,
            &sty(Family::Mono, Weight::Bold, 20.0, t.ink, 0.04),
        );
        for (which, glyph) in [(0usize, "−"), (1, "+")] {
            let (bx, by, bw, bh) = btn_rect(i, which);
            stroke_rect(c, bx, by, bw, bh, t.line, 1);
            center(
                c,
                f,
                (bx + bw / 2) as f32,
                (by + bh / 2 + 8) as f32,
                glyph,
                &sty(Family::Sans, Weight::Bold, 24.0, t.acc, 0.0),
            );
        }
        crate::widgets::hline(c, ry + ROW_H, t.line);
    }
    let (sx, sy, sw, sh) = set_rect();
    fill_rect(c, sx, sy, sw, sh, if on { t.panel } else { t.acc });
    center(
        c,
        f,
        (sx + sw / 2) as f32,
        (sy + sh / 2 + 7) as f32,
        crate::i18n::text(if on {
            "Turn the alarm off"
        } else {
            "Turn the alarm on"
        }),
        &sty(
            Family::Sans,
            Weight::Bold,
            crate::scale::ROW,
            if on { t.ink } else { t.bg },
            0.0,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_step_round_the_day_and_the_file_form_round_trips() {
        assert_eq!(step(DEFAULT_MIN, 0, 1), 8 * 60);
        assert_eq!(step(23 * 60, 0, 1), 0);
        assert_eq!(step(0, 1, -1), 23 * 60 + 55);
        assert_eq!(step(7 * 60 + 55, 1, 1), 8 * 60);
        assert_eq!(hhmm(7 * 60 + 5), "07:05");
        assert_eq!(parse_hhmm("07:05"), Some(7 * 60 + 5));
        for bad in ["24:00", "7", "07:60", "x:1", ""] {
            assert_eq!(parse_hhmm(bad), None, "{bad}");
        }
        assert_eq!(hit_btn(0, 0), None);
    }
}
