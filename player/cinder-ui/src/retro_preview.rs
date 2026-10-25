//! Standalone retro portable-player page for finite music previews.

use crate::canvas::Canvas;
use crate::text::{Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{self, fill_rect, stroke_rect};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle, Rectangle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Short,
    Long,
    Prev,
    PlayPause,
    Next,
    Full,
    End,
}

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Rect {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

// The cartridge, then the controls under it. Before a pass starts, the only thing to do is pick a
// length and start, so the transport row is one wide START and Full and End are not drawn: four
// disabled buttons said nothing a first-time reader could use.
const CART: Rect = Rect {
    x: 20,
    y: 104,
    w: 440,
    h: 276,
};
const SHORT_RECT: Rect = Rect {
    x: 24,
    y: 396,
    w: 208,
    h: 56,
};
const LONG_RECT: Rect = Rect {
    x: 248,
    y: 396,
    w: 208,
    h: 56,
};
const PREV_RECT: Rect = Rect {
    x: 24,
    y: 468,
    w: 128,
    h: 80,
};
const PLAY_RECT: Rect = Rect {
    x: 160,
    y: 468,
    w: 160,
    h: 80,
};
const NEXT_RECT: Rect = Rect {
    x: 328,
    y: 468,
    w: 128,
    h: 80,
};
/// START, before a pass: the whole transport row.
const START_RECT: Rect = Rect {
    x: 24,
    y: 468,
    w: 432,
    h: 80,
};
const FULL_RECT: Rect = Rect {
    x: 24,
    y: 564,
    w: 432,
    h: 64,
};
const END_RECT: Rect = Rect {
    x: 24,
    y: 640,
    w: 432,
    h: 64,
};

pub fn hit(x: i32, y: i32, active: bool, has_queue: bool) -> Option<Hit> {
    // Drawing and hit testing share the exact same strict rectangles. This rejects margins,
    // inter-button gaps, and controls that are not drawn.
    if SHORT_RECT.contains(x, y) {
        return Some(Hit::Short);
    }
    if LONG_RECT.contains(x, y) {
        return Some(Hit::Long);
    }
    if active && PREV_RECT.contains(x, y) {
        return Some(Hit::Prev);
    }
    let play = if active { PLAY_RECT } else { START_RECT };
    if (active || has_queue) && play.contains(x, y) {
        return Some(Hit::PlayPause);
    }
    if active && NEXT_RECT.contains(x, y) {
        return Some(Hit::Next);
    }
    if active && FULL_RECT.contains(x, y) {
        return Some(Hit::Full);
    }
    if active && END_RECT.contains(x, y) {
        return Some(Hit::End);
    }
    None
}

fn time(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn button(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    rect: Rect,
    label: &str,
    selected: bool,
    enabled: bool,
) {
    let fill = if selected {
        t.acc
    } else if enabled {
        t.panel
    } else {
        t.bg
    };
    let ink = if selected {
        t.acc_ink
    } else if enabled {
        t.ink
    } else {
        t.dim
    };
    fill_rect(c, rect.x, rect.y, rect.w, rect.h, fill);
    stroke_rect(
        c,
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if selected {
            t.acc
        } else if enabled {
            t.line
        } else {
            t.dim
        },
        1,
    );
    let st = widgets::sty(Family::Mono, Weight::Bold, 17.0, ink, 0.01);
    widgets::center(
        c,
        f,
        rect.x as f32 + rect.w as f32 / 2.0,
        rect.y as f32 + rect.h as f32 / 2.0 + 6.0,
        label,
        &st,
    );
}

/// `s` fitted to `w` and centred on `cx`.
fn center_in(c: &mut Canvas, f: &FontSet, cx: f32, baseline: f32, s: &str, st: &TextStyle, w: f32) {
    let s = widgets::fit(f, s, st, w);
    let sw = crate::text::measure(f, &s, st);
    crate::text::draw(c, f, cx - sw / 2.0, baseline, &s, st);
}

/// Render on the real 480×800 canvas. Copy is fitted at the selected 80–140% UI scale.
#[allow(clippy::too_many_arguments)]
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    title: &str,
    artist: &str,
    preview_ms: u32,
    active: bool,
    completed: bool,
    playing: bool,
    source: Option<crate::zappin::StartSource>,
    start_ms: Option<u64>,
    has_queue: bool,
    elapsed_ms: u32,
    length_ms: u32,
    current: u32,
    total: u32,
) {
    c.fill(t.bg);
    crate::chrome::header(c, t, f, crate::i18n::text("Preview"), None);

    // Silver MiniDisc cartridge: squared metal shell, inset rim, shutter groove and a static disc
    // window. The silhouette stays recognizable at every UI scale; only its lettering scales.
    // At night the metal is dimmed well below the page's own night palette, or the cartridge is
    // the brightest thing on a dark screen; its lettering then takes the theme's light inks.
    let metal = |r: u8, g: u8, b: u8| {
        // 22 %, not 40: at 40 the night theme's light lettering all but vanished into the metal.
        let k: u16 = if t.night { 22 } else { 100 };
        let m = |v: u8| (u16::from(v) * k / 100) as u8;
        t.scale_color(Rgb888::new(m(r), m(g), m(b)))
    };
    let silver = metal(178, 184, 188);
    let bright = metal(222, 226, 228);
    let shadow = metal(92, 101, 107);
    let cartridge_ink = if t.night {
        t.ink
    } else {
        t.scale_color(Rgb888::new(36, 42, 45))
    };
    // At night the dim ink is too close to the metal; size alone sets the artist apart.
    let cartridge_dim = if t.night {
        t.ink
    } else {
        t.scale_color(Rgb888::new(66, 73, 76))
    };
    let Rect {
        x: cx0,
        y: y0,
        w: cw,
        h: ch,
    } = CART;
    fill_rect(c, cx0, y0, cw, ch, silver);
    stroke_rect(c, cx0, y0, cw, ch, shadow, 2);
    stroke_rect(c, cx0 + 7, y0 + 7, cw - 14, ch - 14, bright, 1);
    fill_rect(c, cx0 + 13, y0 + 13, 7, ch - 26, shadow);
    fill_rect(c, cx0 + cw - 20, y0 + 13, 7, ch - 26, bright);
    fill_rect(c, cx0 + 10, y0 + 13, 112, 5, bright);
    fill_rect(c, cx0 + 10, y0 + ch - 18, 112, 5, shadow);
    let disc = metal(116, 125, 93);
    let hub = Point::new(105, y0 + ch / 2);
    Circle::with_center(hub, 132)
        .into_styled(PrimitiveStyle::with_fill(disc))
        .draw(c)
        .ok();
    Circle::with_center(hub, 48)
        .into_styled(PrimitiveStyle::with_fill(t.panel))
        .draw(c)
        .ok();
    Circle::with_center(hub, 132)
        .into_styled(PrimitiveStyle::with_stroke(t.line, 2))
        .draw(c)
        .ok();
    Rectangle::new(hub - Point::new(7, 7), Size::new(14, 14))
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();

    // The lettering column, right of the disc.
    let (lx, right) = (186.0, 438.0);
    let y = |dy: i32| (y0 + dy) as f32;
    let state = if active {
        crate::i18n::text("PREVIEWING")
    } else if completed {
        crate::i18n::text("ROUND COMPLETE")
    } else if !has_queue {
        crate::i18n::text("UP NEXT IS EMPTY")
    } else {
        crate::i18n::text("READY TO PREVIEW")
    };
    // Where in the pass, while there is one.
    let label = if (active || completed) && total > 0 {
        format!("{state} \u{b7} {current}/{total}")
    } else {
        state.to_string()
    };
    let cap = widgets::sty(Family::Mono, Weight::Bold, 13.0, cartridge_ink, 0.08);
    widgets::draw_fit(c, f, lx, y(42), &label, &cap, right);
    let title_style = widgets::sty(Family::Sans, Weight::Bold, 19.0, cartridge_ink, 0.0);
    let sub = widgets::sty(Family::Sans, Weight::Regular, 14.0, cartridge_dim, 0.0);
    if title.is_empty() {
        // An instruction, not a title: the smaller type, wrapped (in title type it was cut).
        let hint = crate::i18n::text("Start an album or playlist");
        let wrapped = crate::track_info::wrap(f, hint, &sub, right - lx);
        for (i, line) in wrapped.iter().take(2).enumerate() {
            let base = y(72) + i as f32 * crate::text::scaled(20.0);
            widgets::draw_fit(c, f, lx, base, line.trim_end(), &sub, right);
        }
    } else {
        widgets::draw_fit(c, f, lx, y(72), title, &title_style, right);
        widgets::draw_fit(c, f, lx, y(96), artist, &sub, right);
    }

    // What this sample is, while there is one; before a pass, what a pass does.
    let info = widgets::sty(Family::Mono, Weight::Regular, 13.0, cartridge_ink, 0.0);
    let line_h = crate::text::scaled(21.0);
    let mut lines: Vec<String> = Vec::new();
    if active {
        lines.push(
            match source {
                Some(crate::zappin::StartSource::Sabi) => crate::i18n::text("Chorus start"),
                Some(crate::zappin::StartSource::FixedFallback) => {
                    crate::i18n::text("Fixed position")
                }
                Some(crate::zappin::StartSource::UnknownDurationFromStart) => {
                    crate::i18n::text("Unknown duration")
                }
                None => crate::i18n::text("Loading the next song"),
            }
            .to_string(),
        );
        // Its own line: at 140 % a combined one ellipsized the timestamp, and the start position
        // is required playback context, not decoration.
        if let Some(ms) = start_ms {
            lines.push(format!(
                "{} {}",
                crate::i18n::text("Start position"),
                time(ms)
            ));
        }
        if length_ms != preview_ms {
            lines.push(format!(
                "{} {}s \u{b7} {} {}s",
                crate::i18n::text("NOW"),
                length_ms / 1_000,
                crate::i18n::text("NEXT PREVIEW"),
                preview_ms / 1_000,
            ));
        }
    } else if !completed && has_queue {
        // With no queue, the state line, the hint and the LCD already say it.
        let why = crate::i18n::text("Preview will not change your queue");
        lines = crate::track_info::wrap(f, why, &info, right - lx)
            .into_iter()
            .take(2)
            .map(|l| l.trim_end().to_string())
            .collect();
    }
    for (i, line) in lines.iter().take(3).enumerate() {
        let base = y(126) + i as f32 * line_h;
        widgets::draw_fit(c, f, lx, base, line, &info, right);
    }

    // The LCD carries the one live number: the sample's time. Before and after a pass, the step.
    let (lcd_x, lcd_y, lcd_w, lcd_h) = (186, y0 + 192, 252, 40);
    let lcd = metal(194, 201, 158);
    fill_rect(c, lcd_x, lcd_y, lcd_w, lcd_h, lcd);
    stroke_rect(c, lcd_x, lcd_y, lcd_w, lcd_h, t.line, 1);
    let lcd_ink = if t.night {
        t.ink
    } else {
        t.scale_color(Rgb888::new(42, 50, 36))
    };
    let lcd_text = if active {
        format!(
            "{} / {}",
            time(u64::from(elapsed_ms.min(length_ms))),
            time(u64::from(length_ms))
        )
    } else if completed {
        crate::i18n::text("PASS FINISHED").to_string()
    } else if has_queue {
        crate::i18n::text("PRESS START").to_string()
    } else {
        crate::i18n::text("ADD SONGS TO UP NEXT").to_string()
    };
    let size = if active { 20.0 } else { 14.0 };
    let lcd_style = widgets::sty(Family::Mono, Weight::Bold, size, lcd_ink, 0.0);
    let lcd_mid = (lcd_x + lcd_w / 2) as f32;
    let lcd_base = (lcd_y + lcd_h / 2) as f32 + crate::text::scaled(size) * 0.36;
    center_in(
        c,
        f,
        lcd_mid,
        lcd_base,
        &lcd_text,
        &lcd_style,
        (lcd_w - 16) as f32,
    );
    if active && length_ms > 0 {
        let bar_y = lcd_y + lcd_h + 10;
        fill_rect(c, lcd_x, bar_y, lcd_w, 4, t.line);
        let width = ((elapsed_ms.min(length_ms) as u64 * lcd_w as u64) / length_ms as u64) as i32;
        if width > 0 {
            fill_rect(c, lcd_x, bar_y, width, 4, t.acc);
        }
    }

    let short = preview_ms == 4_000;
    button(
        c,
        t,
        f,
        SHORT_RECT,
        crate::i18n::text("4 SECONDS"),
        short,
        true,
    );
    button(
        c,
        t,
        f,
        LONG_RECT,
        crate::i18n::text("15 SECONDS"),
        !short,
        true,
    );
    if !active {
        button(
            c,
            t,
            f,
            START_RECT,
            crate::i18n::text("START"),
            has_queue,
            has_queue,
        );
        return;
    }
    button(
        c,
        t,
        f,
        PREV_RECT,
        crate::i18n::text("PREVIOUS"),
        false,
        true,
    );
    button(
        c,
        t,
        f,
        PLAY_RECT,
        crate::i18n::text(if playing { "PAUSE" } else { "RESUME" }),
        true,
        true,
    );
    button(c, t, f, NEXT_RECT, crate::i18n::text("NEXT"), false, true);
    button(
        c,
        t,
        f,
        FULL_RECT,
        crate::i18n::text("PLAY THIS SONG IN FULL"),
        false,
        true,
    );
    button(
        c,
        t,
        f,
        END_RECT,
        crate::i18n::text("END PREVIEW"),
        false,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{enter, Language};

    #[test]
    fn every_control_has_a_touch_target_and_gaps_do_nothing() {
        for (x, y, expected) in [
            (80, 420, Hit::Short),
            (400, 420, Hit::Long),
            (80, 500, Hit::Prev),
            (240, 500, Hit::PlayPause),
            (400, 500, Hit::Next),
            (240, 590, Hit::Full),
            (240, 670, Hit::End),
        ] {
            assert_eq!(hit(x, y, true, true), Some(expected));
        }
        assert_eq!(hit(-1, 500, true, true), None);
        assert_eq!(hit(480, 500, true, true), None);
        assert_eq!(hit(240, 460, true, true), None);
        assert_eq!(hit(240, 634, true, true), None);
        for (x, y) in [
            (10, 420),
            (240, 420),
            (470, 420),
            (10, 500),
            (156, 500),
            (324, 500),
            (470, 500),
            (10, 590),
            (470, 590),
            (10, 670),
            (470, 670),
        ] {
            assert_eq!(
                hit(x, y, true, true),
                None,
                "margin/gap tap at {x},{y} must be rejected"
            );
        }
        for (x, y) in [(240, 590), (240, 670)] {
            assert_eq!(hit(x, y, false, true), None, "undrawn control at {x},{y}");
        }
        // Before a pass, START is the whole transport row.
        for x in [40, 240, 440] {
            assert_eq!(
                hit(x, 500, false, true),
                Some(Hit::PlayPause),
                "START at {x}"
            );
        }
        assert_eq!(
            hit(240, 500, false, false),
            None,
            "Start is disabled without a queue"
        );
        assert_eq!(hit(80, 420, false, false), Some(Hit::Short));
    }

    #[test]
    fn long_title_and_all_sources_fit_both_languages_themes_and_scale_limits() {
        let _scale = crate::text::scale_guard();
        let fonts = FontSet::load();
        let long = "A Very Long Song Title With More Words Than Fit on One Line — 长标题测试";
        for language in [Language::English, Language::SimplifiedChinese] {
            let _language = enter(language);
            for pct in [100, 140] {
                crate::text::set_scale_pct(pct);
                for night in [false, true] {
                    for source in [
                        Some(crate::zappin::StartSource::Sabi),
                        Some(crate::zappin::StartSource::FixedFallback),
                        Some(crate::zappin::StartSource::UnknownDurationFromStart),
                        None,
                    ] {
                        let theme = if night { Theme::night() } else { Theme::day() };
                        let mut canvas = Canvas::new();
                        canvas.track_text();
                        render(
                            &mut canvas,
                            &theme,
                            &fonts,
                            long,
                            "An Artist With a Long Name",
                            15_000,
                            source.is_some(),
                            false,
                            true,
                            source,
                            Some(62_000),
                            true,
                            1_500,
                            4_000,
                            2,
                            9,
                        );
                        assert_eq!(canvas.oob_x(), 0, "{language:?} {pct}% night={night}");
                        assert!(
                            canvas.text_collisions().is_empty(),
                            "{language:?} {pct}% night={night}: {:?}",
                            canvas.text_collisions()
                        );
                    }
                }
            }
        }
    }
}
