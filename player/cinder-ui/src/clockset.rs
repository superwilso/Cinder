//! Settings ▸ Date & time — the clock editor.
//!
//! Cinder had no way to set the time at all: the status-bar clock was read-only, and a flat battery
//! or a drifting RTC left no route back to a correct time short of booting stock. Sony's own player
//! can do it (`HgrmMediaPlayer/src/model/date_time/DateTime.cpp`), but nothing in `vendor/sony/lib`
//! exposes a clock setter, so the shell goes to the kernel through the setuid `cinder-clock`
//! helper. This module is only the editor and the arithmetic.
//!
//! FIVE ±ROWS, not a row of spinners. The NW-A55 has no d-pad and a 480px-wide panel; five little
//! up/down arrows side by side would each be ~20px of target. A vertical list with one big minus
//! and one big plus per field reuses the row idiom the rest of the UI already uses and gives every
//! control a 64px-tall hit box.

use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{center, fill_rect, right, stroke_rect, sty};
use crate::Canvas;

/// Year bounds. The lower end rejects a flat-RTC 1970; the upper end stops well clear of the
/// 32-bit `time_t` wrap at 2038-01-19 03:14:07 UTC — past that the clock does not go far into the
/// future, it goes NEGATIVE, to 1901. `cinder-clock` enforces the same bound independently, so a
/// UI bug cannot hand the helper something it would accept.
pub const YEAR_MIN: i32 = 2001;
pub const YEAR_MAX: i32 = 2037;

pub const FIELDS: usize = 5;
pub const F_YEAR: usize = 0;
pub const F_MONTH: usize = 1;
pub const F_DAY: usize = 2;
pub const F_HOUR: usize = 3;
pub const F_MIN: usize = 4;

pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;
/// The live preview sits between the header and the fields.
pub const PREVIEW_H: i32 = 60;
pub const ROW_H: i32 = 64;
pub fn fields_top() -> i32 {
    TOP + PREVIEW_H
}
pub fn fields_bottom() -> i32 {
    fields_top() + ROW_H * FIELDS as i32
}

/// − and + buttons. Wide and full-height within the row: this is the whole interaction.
pub const BTN_W: i32 = 64;
pub const BTN_H: i32 = 48;
pub const MINUS_X: i32 = 306;
pub const PLUS_X: i32 = 382;

/// The Set button.
pub const SET_Y: i32 = 520;
pub const SET_H: i32 = 68;
pub const SET_X: i32 = 22;
pub const SET_W: i32 = 436;

pub fn set_rect() -> (i32, i32, i32, i32) {
    (SET_X, SET_Y, SET_W, SET_H)
}
pub fn hit_set(x: i32, y: i32) -> bool {
    (SET_X..SET_X + SET_W).contains(&x) && (SET_Y..SET_Y + SET_H).contains(&y)
}

/// Which field row is under `y`.
pub fn field_at(y: i32) -> Option<usize> {
    let t = fields_top();
    if y < t {
        return None;
    }
    let r = ((y - t) / ROW_H) as usize;
    (r < FIELDS).then_some(r)
}

/// Button rect for row `f`: `which` 0 = minus, 1 = plus.
pub fn btn_rect(f: usize, which: usize) -> (i32, i32, i32, i32) {
    let y = fields_top() + f as i32 * ROW_H + (ROW_H - BTN_H) / 2;
    let x = if which == 0 { MINUS_X } else { PLUS_X };
    (x, y, BTN_W, BTN_H)
}

/// Which button a tap lands on: `(field, delta)`. Returns `None` for a tap that is on a row but
/// not on either button, so reading the screen never changes the time.
pub fn hit_btn(x: i32, y: i32) -> Option<(usize, i32)> {
    let f = field_at(y)?;
    for (which, delta) in [(0usize, -1i32), (1, 1)] {
        let (bx, by, bw, bh) = btn_rect(f, which);
        if (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y) {
            return Some((f, delta));
        }
    }
    None
}

// ── civil ↔ epoch ───────────────────────────────────────────────────────────────────────────────
// Howard Hinnant's days_from_civil / civil_from_days. Proleptic Gregorian, era-based, no lookup
// tables and no leap-year special cases beyond the era arithmetic — and no `chrono`, which would be
// a dependency (and a much larger binary) for two functions.

/// Days since 1970-01-01 for a civil date. `m` is 1..=12, `d` is 1..=31.
pub fn days_from_civil(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // 0..=399
    let mp = ((m + 9) % 12) as i64; // Mar=0 … Feb=11
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // 0..=365
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // 0..=146096
    era * 146097 + doe - 719468
}

/// Inverse of `days_from_civil`.
pub fn civil_from_days(z: i64) -> (i32, i32, i32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // 0..=399
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365
    let mp = (5 * doy + 2) / 153; // 0..=11
    let d = doy - (153 * mp + 2) / 5 + 1; // 1..=31
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // 1..=12
    ((if m <= 2 { y + 1 } else { y }) as i32, m as i32, d as i32)
}

/// Days in `m` of year `y`.
pub fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// The editor's five fields: `[year, month, day, hour, minute]`.
pub type Fields = [i32; FIELDS];

/// Seed the editor from a UTC epoch.
pub fn fields_from_epoch(epoch: i64) -> Fields {
    let days = epoch.div_euclid(86400);
    let secs = epoch.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    [y.clamp(YEAR_MIN, YEAR_MAX), m, d, (secs / 3600) as i32, ((secs / 60) % 60) as i32]
}

/// UTC epoch for the editor's fields. Seconds are dropped to zero: the editor has no seconds field,
/// and carrying the old ones over would make "set" land at an arbitrary point in the minute.
pub fn epoch_from_fields(f: &Fields) -> i64 {
    let y = f[F_YEAR].clamp(YEAR_MIN, YEAR_MAX);
    let m = f[F_MONTH].clamp(1, 12);
    let d = f[F_DAY].clamp(1, days_in_month(y, m));
    days_from_civil(y, m, d) * 86400 + f[F_HOUR].clamp(0, 23) as i64 * 3600
        + f[F_MIN].clamp(0, 59) as i64 * 60
}

/// Step one field, wrapping where wrapping is what a user means (hours and minutes) and clamping
/// where it is not (the year). The day is re-clamped after a month or year change so 31 January
/// stepping to February lands on the 28th/29th rather than an impossible date.
pub fn step(f: &mut Fields, field: usize, delta: i32) {
    match field {
        F_YEAR => f[F_YEAR] = (f[F_YEAR] + delta).clamp(YEAR_MIN, YEAR_MAX),
        F_MONTH => f[F_MONTH] = wrap(f[F_MONTH] - 1 + delta, 12) + 1,
        F_DAY => {
            let n = days_in_month(f[F_YEAR], f[F_MONTH]);
            f[F_DAY] = wrap(f[F_DAY] - 1 + delta, n) + 1;
        }
        F_HOUR => f[F_HOUR] = wrap(f[F_HOUR] + delta, 24),
        F_MIN => f[F_MIN] = wrap(f[F_MIN] + delta, 60),
        _ => {}
    }
    let n = days_in_month(f[F_YEAR], f[F_MONTH]);
    if f[F_DAY] > n {
        f[F_DAY] = n;
    }
}

fn wrap(v: i32, n: i32) -> i32 {
    ((v % n) + n) % n
}

pub const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June",
     "July", "August", "September", "October", "November", "December"];
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday",
                             "Saturday", "Sunday"];

/// Weekday name for a civil date. 1970-01-01 was a Thursday, index 3 in a Monday-first table.
pub fn weekday(y: i32, m: i32, d: i32) -> &'static str {
    let idx = (days_from_civil(y, m, d) + 3).rem_euclid(7) as usize;
    WEEKDAYS[idx]
}

fn label_of(f: usize) -> &'static str {
    match f {
        F_YEAR => "Year",
        F_MONTH => "Month",
        F_DAY => "Day",
        F_HOUR => "Hour",
        _ => "Minute",
    }
}

fn value_of(fields: &Fields, f: usize) -> String {
    match f {
        F_YEAR => format!("{}", fields[F_YEAR]),
        F_MONTH => MONTHS[(fields[F_MONTH].clamp(1, 12) - 1) as usize].to_string(),
        F_DAY => format!("{}", fields[F_DAY]),
        F_HOUR => format!("{:02}", fields[F_HOUR]),
        _ => format!("{:02}", fields[F_MIN]),
    }
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, fields: &Fields, sel: usize) {
    c.fill(t.bg);
    let y0 = crate::chrome::header(c, t, f, "Date & time", None);
    debug_assert_eq!(y0, TOP, "clock editor top drifted from the hit test");

    // Live preview of exactly what Set will write, spelled out — "2026-08-17" is easy to misread
    // by a month when you are changing it a step at a time.
    {
        let y = fields[F_YEAR];
        let m = fields[F_MONTH].clamp(1, 12);
        let d = fields[F_DAY];
        let s = format!("{} {} {} {} · {:02}:{:02}",
                        weekday(y, m, d), d, MONTHS[(m - 1) as usize], y,
                        fields[F_HOUR], fields[F_MIN]);
        center(c, f, 240.0, (TOP + 36) as f32, &s,
               &sty(Family::Sans, Weight::SemiBold, 17.0, t.acc, 0.0));
        center(c, f, 240.0, (TOP + 54) as f32, "UTC — THE DEVICE KEEPS NO TIME ZONE",
               &sty(Family::Mono, Weight::Regular, 10.0, t.faint, 0.16));
    }

    let ft = fields_top();
    crate::widgets::hline(c, ft, t.line);
    for i in 0..FIELDS {
        let ry = ft + i as i32 * ROW_H;
        let cy = ry + ROW_H / 2;
        if i == sel {
            fill_rect(c, 0, ry, crate::canvas::W as i32, ROW_H, t.row_sel);
        }
        let lc = if i == sel { t.acc } else { t.ink };
        text::draw(c, f, 22.0, (cy + 6) as f32, label_of(i),
                   &sty(Family::Sans, Weight::SemiBold, 18.0, lc, 0.0));
        // The value sits left of the buttons, right-aligned, so the digits line up down the column.
        right(c, f, (MINUS_X - 16) as f32, (cy + 7) as f32, &value_of(fields, i),
              &sty(Family::Mono, Weight::Bold, 20.0, t.ink, 0.04));
        for (which, glyph) in [(0usize, "−"), (1, "+")] {
            let (bx, by, bw, bh) = btn_rect(i, which);
            stroke_rect(c, bx, by, bw, bh, t.line, 1);
            center(c, f, (bx + bw / 2) as f32, (by + bh / 2 + 8) as f32, glyph,
                   &sty(Family::Sans, Weight::Bold, 24.0, t.acc, 0.0));
        }
        crate::widgets::hline(c, ry + ROW_H, t.line);
    }

    let (sx, sy, sw, sh) = set_rect();
    fill_rect(c, sx, sy, sw, sh, t.acc);
    center(c, f, (sx + sw / 2) as f32, (sy + sh / 2 + 7) as f32, "SET CLOCK",
           &sty(Family::Sans, Weight::ExtraBold, 19.0, t.acc_ink, 0.06));
    center(c, f, 240.0, (sy + sh + 26) as f32,
           "Written to the hardware clock, so it survives a power cycle.",
           &sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The civil↔days pair must be exact inverses across the whole supported range, including
    /// every leap-year edge. This is arithmetic with no second implementation to check it against,
    /// so the round trip IS the test.
    #[test]
    fn civil_and_days_round_trip_over_the_whole_range() {
        for y in YEAR_MIN..=YEAR_MAX {
            for m in 1..=12 {
                for d in 1..=days_in_month(y, m) {
                    let z = days_from_civil(y, m, d);
                    assert_eq!(civil_from_days(z), (y, m, d), "{y}-{m}-{d} did not round trip");
                }
            }
        }
        // A known anchor, so a consistent-but-wrong pair cannot pass: the epoch itself.
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(weekday(1970, 1, 1), "Thursday");
        // And a leap day that only exists in a 400-divisible century.
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
    }

    #[test]
    fn epoch_and_fields_round_trip() {
        // Minute resolution: seconds are deliberately dropped, so compare on a whole minute.
        for &e in &[978307200i64, 1786957260, 1234567800, 2145916800 - 60] {
            let f = fields_from_epoch(e);
            assert_eq!(epoch_from_fields(&f), e, "epoch {e} did not survive the editor");
        }
    }

    /// Stepping must never produce a date that does not exist, and must wrap where a user expects
    /// it to. 31 January stepping a month is the classic way to get 31 February.
    #[test]
    fn stepping_never_produces_an_impossible_date() {
        let mut f: Fields = [2024, 1, 31, 23, 59];
        step(&mut f, F_MONTH, 1);
        assert_eq!(f[F_DAY], 29, "31 Jan -> Feb should land on the 29th in a leap year");
        let mut f: Fields = [2023, 1, 31, 0, 0];
        step(&mut f, F_MONTH, 1);
        assert_eq!(f[F_DAY], 28, "and the 28th in a common year");
        // Year change re-clamps too: 29 Feb 2024 stepping a year has nowhere to land.
        let mut f: Fields = [2024, 2, 29, 0, 0];
        step(&mut f, F_YEAR, 1);
        assert_eq!((f[0], f[1], f[2]), (2025, 2, 28));

        // Hours and minutes wrap; the year clamps.
        let mut f: Fields = [YEAR_MAX, 6, 15, 23, 59];
        step(&mut f, F_HOUR, 1);
        assert_eq!(f[F_HOUR], 0, "hour did not wrap");
        step(&mut f, F_MIN, 1);
        assert_eq!(f[F_MIN], 0, "minute did not wrap");
        step(&mut f, F_YEAR, 1);
        assert_eq!(f[F_YEAR], YEAR_MAX, "year must clamp, not wrap past the 2038 bound");
        let mut f: Fields = [YEAR_MIN, 1, 1, 0, 0];
        step(&mut f, F_YEAR, -1);
        assert_eq!(f[F_YEAR], YEAR_MIN);
        step(&mut f, F_MONTH, -1);
        assert_eq!(f[F_MONTH], 12, "month did not wrap backwards");
    }

    /// Every epoch the editor can produce must sit inside the range `cinder-clock` accepts, or the
    /// UI would offer a value the helper silently refuses.
    #[test]
    fn the_editor_cannot_produce_a_time_the_helper_rejects() {
        const HELPER_MIN: i64 = 978307200;
        const HELPER_MAX: i64 = 2145916800;
        for &f in &[[YEAR_MIN, 1, 1, 0, 0], [YEAR_MAX, 12, 31, 23, 59]] {
            let e = epoch_from_fields(&f);
            assert!((HELPER_MIN..=HELPER_MAX).contains(&e), "{f:?} -> {e} is outside the helper's range");
        }
    }

    /// The ± buttons must be real touch targets, inside their own row, and must not overlap.
    #[test]
    fn the_step_buttons_are_reachable_and_distinct() {
        for i in 0..FIELDS {
            let (mx, my, mw, mh) = btn_rect(i, 0);
            let (px, py, pw, ph) = btn_rect(i, 1);
            assert!(mw >= 44 && mh >= 44, "minus is {mw}x{mh}");
            assert!(pw >= 44 && ph >= 44, "plus is {pw}x{ph}");
            assert!(mx + mw < px, "the buttons overlap");
            assert!(px + pw <= 458, "plus runs past the right margin");
            assert_eq!(hit_btn(mx + mw / 2, my + mh / 2), Some((i, -1)));
            assert_eq!(hit_btn(px + pw / 2, py + ph / 2), Some((i, 1)));
            assert_eq!(field_at(my + mh / 2), Some(i));
            // A tap on the label half changes nothing.
            assert_eq!(hit_btn(30, my + mh / 2), None);
        }
        // Set is clear of the last field row.
        assert!(SET_Y >= fields_bottom(), "the Set button overlaps the fields");
        assert!(hit_set(240, SET_Y + SET_H / 2));
        assert!(!hit_set(240, fields_bottom() - 1));
        assert!(SET_Y + SET_H <= crate::canvas::H as i32, "Set runs off the bottom");
    }
}
