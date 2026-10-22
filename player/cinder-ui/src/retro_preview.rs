//! Standalone retro portable-player page for finite music previews.

use crate::canvas::Canvas;
use crate::text::{Family, FontSet, Weight};
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

const SHORT_RECT: Rect = Rect {
    x: 24,
    y: 430,
    w: 208,
    h: 54,
};
const LONG_RECT: Rect = Rect {
    x: 248,
    y: 430,
    w: 208,
    h: 54,
};
const PREV_RECT: Rect = Rect {
    x: 24,
    y: 500,
    w: 128,
    h: 64,
};
const PLAY_RECT: Rect = Rect {
    x: 160,
    y: 500,
    w: 160,
    h: 64,
};
const NEXT_RECT: Rect = Rect {
    x: 328,
    y: 500,
    w: 128,
    h: 64,
};
const FULL_RECT: Rect = Rect {
    x: 24,
    y: 590,
    w: 432,
    h: 64,
};
const END_RECT: Rect = Rect {
    x: 24,
    y: 668,
    w: 432,
    h: 62,
};

pub fn hit(x: i32, y: i32, active: bool, has_queue: bool) -> Option<Hit> {
    // Drawing and hit testing share the exact same strict rectangles. This rejects margins,
    // inter-button gaps, and disabled controls at the geometry boundary.
    if SHORT_RECT.contains(x, y) {
        return Some(Hit::Short);
    }
    if LONG_RECT.contains(x, y) {
        return Some(Hit::Long);
    }
    if active && PREV_RECT.contains(x, y) {
        return Some(Hit::Prev);
    }
    if (active || has_queue) && PLAY_RECT.contains(x, y) {
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

/// Render on the real 480×800 canvas. Copy is fitted at the selected 80–140% UI scale.
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
        let k: u16 = if t.night { 40 } else { 100 };
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
    let cartridge_dim = if t.night {
        t.dim
    } else {
        t.scale_color(Rgb888::new(66, 73, 76))
    };
    fill_rect(c, 20, 138, 440, 276, silver);
    stroke_rect(c, 20, 138, 440, 276, shadow, 2);
    stroke_rect(c, 27, 145, 426, 262, bright, 1);
    fill_rect(c, 33, 151, 7, 250, shadow);
    fill_rect(c, 440, 151, 7, 250, bright);
    fill_rect(c, 30, 151, 112, 5, bright);
    fill_rect(c, 30, 396, 112, 5, shadow);
    let disc = metal(116, 125, 93);
    Circle::with_center(Point::new(105, 244), 132)
        .into_styled(PrimitiveStyle::with_fill(disc))
        .draw(c)
        .ok();
    Circle::with_center(Point::new(105, 244), 48)
        .into_styled(PrimitiveStyle::with_fill(t.panel))
        .draw(c)
        .ok();
    Circle::with_center(Point::new(105, 244), 132)
        .into_styled(PrimitiveStyle::with_stroke(t.line, 2))
        .draw(c)
        .ok();
    Rectangle::new(Point::new(98, 237), Size::new(14, 14))
        .into_styled(PrimitiveStyle::with_fill(t.acc))
        .draw(c)
        .ok();
    let label = if active {
        crate::i18n::text("PREVIEWING")
    } else if completed {
        crate::i18n::text("ROUND COMPLETE")
    } else if !has_queue {
        crate::i18n::text("UP NEXT IS EMPTY")
    } else {
        crate::i18n::text("READY TO PREVIEW")
    };
    let cap = widgets::sty(Family::Mono, Weight::Bold, 13.0, cartridge_ink, 0.08);
    widgets::draw_fit(c, f, 186.0, 180.0, label, &cap, 438.0);
    let title_style = widgets::sty(Family::Sans, Weight::Bold, 16.0, cartridge_ink, 0.0);
    widgets::draw_fit(
        c,
        f,
        186.0,
        208.0,
        if title.is_empty() {
            crate::i18n::text("Start an album or playlist first")
        } else {
            title
        },
        &title_style,
        438.0,
    );
    let sub = widgets::sty(Family::Sans, Weight::Regular, 12.0, cartridge_dim, 0.0);
    widgets::draw_fit(c, f, 186.0, 232.0, artist, &sub, 438.0);
    let source_copy = match source {
        Some(crate::zappin::StartSource::Sabi) => crate::i18n::text("Chorus start"),
        Some(crate::zappin::StartSource::FixedFallback) => crate::i18n::text("Fixed position"),
        Some(crate::zappin::StartSource::UnknownDurationFromStart) => {
            crate::i18n::text("Unknown duration")
        }
        None if active => crate::i18n::text("Loading the next song"),
        None => crate::i18n::text("Choose a preview length"),
    };
    let source_style = widgets::sty(Family::Mono, Weight::Regular, 13.0, cartridge_ink, 0.0);
    // Keep source and exact start on separate rows. At 140% English, a combined line silently
    // ellipsized the timestamp; the start position is required playback context, not decoration.
    widgets::draw_fit(c, f, 186.0, 262.0, source_copy, &source_style, 438.0);
    let start_copy = start_ms
        .map(|ms| format!("{} {}", crate::i18n::text("Start position"), time(ms)))
        .unwrap_or_else(|| format!("{} · …", crate::i18n::text("Start position")));
    widgets::draw_fit(c, f, 186.0, 284.0, &start_copy, &source_style, 438.0);
    let detail = if active && length_ms != preview_ms {
        format!(
            "{} {}s · {} {}s",
            crate::i18n::text("NOW"),
            length_ms / 1_000,
            crate::i18n::text("NEXT PREVIEW"),
            preview_ms / 1_000,
        )
    } else if active {
        crate::i18n::text(if length_ms <= 4_000 {
            "4 SEC SAMPLE"
        } else {
            "15 SEC SAMPLE"
        })
        .to_string()
    } else if !has_queue {
        crate::i18n::text("No playable music queue").to_string()
    } else {
        crate::i18n::text("Up Next stays as it is").to_string()
    };
    if has_queue {
        widgets::draw_fit(c, f, 186.0, 306.0, &detail, &source_style, 438.0);
    }
    let lcd = metal(194, 201, 158);
    fill_rect(c, 186, 322, 252, 30, lcd);
    stroke_rect(c, 186, 322, 252, 30, t.line, 1);
    let lcd_style = widgets::sty(
        Family::Mono,
        Weight::Bold,
        14.0,
        if t.night {
            t.ink
        } else {
            t.scale_color(Rgb888::new(42, 50, 36))
        },
        0.0,
    );
    let lcd_text = if active {
        crate::i18n::text("UP NEXT IS SAFE")
    } else if completed {
        crate::i18n::text("PASS FINISHED")
    } else if has_queue {
        crate::i18n::text("PRESS START")
    } else {
        crate::i18n::text("ADD SONGS TO UP NEXT")
    };
    widgets::center(c, f, 312.0, 341.0, lcd_text, &lcd_style);
    fill_rect(c, 186, 360, 252, 4, t.line);
    if active && length_ms > 0 {
        let width = ((elapsed_ms.min(length_ms) as u64 * 252) / length_ms as u64) as i32;
        if width > 0 {
            fill_rect(c, 186, 360, width, 4, t.acc);
        }
    }
    let elapsed = time(u64::from(elapsed_ms));
    let remaining = time(u64::from(length_ms.saturating_sub(elapsed_ms)));
    let timer_copy = format!("{} / {} · {}/{}", elapsed, remaining, current, total);
    widgets::draw_fit(c, f, 186.0, 389.0, &timer_copy, &source_style, 438.0);

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
    let enabled = active || has_queue;
    button(
        c,
        t,
        f,
        PREV_RECT,
        crate::i18n::text("PREVIOUS"),
        false,
        active,
    );
    button(
        c,
        t,
        f,
        PLAY_RECT,
        crate::i18n::text(if active {
            if playing {
                "PAUSE"
            } else {
                "RESUME"
            }
        } else {
            "START"
        }),
        enabled,
        enabled,
    );
    button(c, t, f, NEXT_RECT, crate::i18n::text("NEXT"), false, active);
    button(
        c,
        t,
        f,
        FULL_RECT,
        crate::i18n::text("PLAY THIS SONG IN FULL"),
        false,
        enabled && active,
    );
    button(
        c,
        t,
        f,
        END_RECT,
        crate::i18n::text("END PREVIEW"),
        false,
        active,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{enter, Language};

    #[test]
    fn every_control_has_a_touch_target_and_gaps_do_nothing() {
        for (x, y, expected) in [
            (80, 450, Hit::Short),
            (400, 450, Hit::Long),
            (80, 530, Hit::Prev),
            (240, 530, Hit::PlayPause),
            (400, 530, Hit::Next),
            (240, 620, Hit::Full),
            (240, 700, Hit::End),
        ] {
            assert_eq!(hit(x, y, true, true), Some(expected));
        }
        assert_eq!(hit(-1, 530, true, true), None);
        assert_eq!(hit(480, 530, true, true), None);
        assert_eq!(hit(240, 490, true, true), None);
        assert_eq!(hit(240, 660, true, true), None);
        for (x, y) in [
            (10, 450),
            (240, 450),
            (470, 450),
            (10, 530),
            (156, 530),
            (324, 530),
            (470, 530),
            (10, 620),
            (470, 620),
            (10, 700),
            (470, 700),
        ] {
            assert_eq!(
                hit(x, y, true, true),
                None,
                "margin/gap tap at {x},{y} must be rejected"
            );
        }
        for (x, y) in [(80, 530), (400, 530), (240, 620), (240, 700)] {
            assert_eq!(hit(x, y, false, true), None, "disabled control at {x},{y}");
        }
        assert_eq!(
            hit(240, 530, false, false),
            None,
            "Start is disabled without a queue"
        );
        assert_eq!(hit(80, 450, false, false), Some(Hit::Short));
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
