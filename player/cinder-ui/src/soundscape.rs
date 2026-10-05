//! Menu ▸ Soundscapes — rain, a beach, a stream, wind, a fire, a night, and three colours of noise,
//! over the music or on their own (`docs/SPEC_soundscapes.md`).
//!
//! ```text
//!   ‹ Soundscapes
//!   RAIN · ON ITS OWN · HEADPHONES             the strip: what is playing, and where
//!   Soundscape                         (●)     on / off
//!   SOUND
//!   [ Rain        ][ Storm       ]             two columns of chips: the sound
//!   [ Beach       ][ Stream      ]
//!   [ Wind        ][ Fire        ]
//!   [ Night       ][ White noise ]
//!   [ Pink noise  ][ Dark noise  ]
//!   LEVEL
//!   On its own     ───────●──────   60%      the level with nothing else playing
//!   With music     ───●──────────   35%      the level over the music
//!   Over music it reaches the jack and Bluetooth.   (or what stops it)
//! ```
//!
//! Two levels because they answer two different questions: on its own a soundscape IS the thing
//! you are listening to; over a song it is a bed under it and wants to be well below it. The shell
//! picks between them from the play state and hands the generator one number.
//!
//! None of the sounds is a recording. They are made as they play (`cinder-home/src/soundscape.h`),
//! from noise and short events at random times, so nothing loops.
//!
//! The page does not scroll; `row_top` and the chip grid are the single statement of the layout
//! that both `render` and the hit tests read, as on every kit screen.

use crate::canvas::{Canvas, W};
use crate::kit::{self, Row, Trail};
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, fit, hline, right, sty};

/// The sounds, in chip order: `(id, label)`. The id is `soundscape.h`'s `SS_*` number — a FILE
/// FORMAT (the settings file and `/tmp/cinder_ambient` both carry it), so it is never renumbered;
/// the chip order is free to change.
pub const SOUNDS: [(u8, &str); 10] = [
    (4, "Rain"),
    (5, "Storm"),
    (6, "Beach"),
    (7, "Stream"),
    (8, "Wind"),
    (9, "Fire"),
    (10, "Night"),
    (1, "White noise"),
    (2, "Pink noise"),
    (3, "Dark noise"),
];

/// The highest sound id `soundscape.h` knows (`SS_COUNT - 1`).
pub const MAX_ID: u8 = 10;
/// What a fresh install picks when the switch is first turned on.
pub const DEFAULT_SOUND: u8 = 4;
/// Defaults for the two levels, in percent.
pub const DEFAULT_ALONE: u8 = 60;
pub const DEFAULT_MUSIC: u8 = 30;
/// Levels move in steps of this many percent, by finger or by button.
pub const LEVEL_STEP: u8 = 5;

/// The label for a sound id ("Rain"), or "Off".
pub fn label(id: u8) -> &'static str {
    SOUNDS
        .iter()
        .find(|s| s.0 == id)
        .map(|s| s.1)
        .unwrap_or("Off")
}

/// The level curve: percent on the slider to a linear gain in thousandths, which is what the shell
/// writes into `/tmp/cinder_ambient`. 0 is silence; 1..100 spans -36 dB..0 dB evenly in decibels,
/// because loudness is heard in ratios — a straight line in amplitude would put every useful
/// setting under the bottom fifth of the slider.
pub fn gain_milli(pct: u8) -> u16 {
    if pct == 0 {
        return 0;
    }
    let pct = pct.min(100) as f32;
    let db = -36.0 * (100.0 - pct) / 99.0;
    let g = 10f32.powf(db / 20.0);
    ((g * 1000.0).round() as u16).clamp(1, 1000)
}

/// Where a soundscape is going right now, as the shell reports it (`cinder_set_ambient_route`).
/// The strip says it in words, because "on" and "audible" are different claims and this project's
/// rule is that a screen makes only the second.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Route {
    /// Switched off, or nothing reported yet.
    #[default]
    Off,
    /// Mixed into the music on its way to the jack and Bluetooth (libcinder_mono.so).
    OverMusic,
    /// Playing on its own through the headphone jack.
    AloneJack,
    /// Playing on its own over Bluetooth.
    AloneBt,
    /// Carried by Cinder's own stream: USB-DAC to LDAC, or FM to Bluetooth.
    OverStream,
    /// Music is playing and the service it plays through has no soundscape hook: Wampy's preload
    /// is missing, so there is nowhere to mix it in. Silent until the music stops.
    NoHook,
    /// Waiting for the output: Sony's sound service still holds it after a pause.
    Waiting,
    /// The output could not be opened at all (libasound missing, or a mode that owns the output:
    /// USB storage, the Bluetooth receiver).
    Unavailable,
}

impl Route {
    pub fn from_code(c: u8) -> Route {
        match c {
            1 => Route::OverMusic,
            2 => Route::AloneJack,
            3 => Route::AloneBt,
            4 => Route::OverStream,
            5 => Route::NoHook,
            6 => Route::Waiting,
            7 => Route::Unavailable,
            _ => Route::Off,
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Route::Off => 0,
            Route::OverMusic => 1,
            Route::AloneJack => 2,
            Route::AloneBt => 3,
            Route::OverStream => 4,
            Route::NoHook => 5,
            Route::Waiting => 6,
            Route::Unavailable => 7,
        }
    }

    fn words(self) -> &'static str {
        match self {
            Route::Off => "",
            Route::OverMusic => "OVER THE MUSIC",
            Route::AloneJack => "ON ITS OWN · HEADPHONES",
            Route::AloneBt => "ON ITS OWN · BLUETOOTH",
            Route::OverStream => "OVER USB-DAC / RADIO",
            Route::NoHook => "SILENT WHILE MUSIC PLAYS",
            Route::Waiting => "WAITING FOR THE OUTPUT",
            Route::Unavailable => "NO OUTPUT RIGHT NOW",
        }
    }
}

/// What the page shows. Built by `nav`.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub on: bool,
    /// The chosen sound's id (kept while switched off, so on brings it back).
    pub sound: u8,
    pub alone: u8,
    pub music: u8,
    pub route: Route,
    /// libcinder_mono.so is loaded in Sony's sound service, so over-music mixing can happen.
    pub hook: bool,
}

/// The one line of state under the header.
pub fn strip_text(v: &View) -> String {
    if !v.on {
        return String::from("OFF · TAP A SOUND TO START");
    }
    let name = label(v.sound).to_uppercase();
    match v.route {
        Route::Off => name,
        r => format!("{name} · {}", r.words()),
    }
}

/// The note at the bottom: where over-music mixing reaches, or why it cannot.
pub fn reach_text(hook: bool) -> &'static str {
    if hook {
        "Over music it reaches the jack and Bluetooth, after the EQ and before the volume."
    } else {
        "Over music needs Wampy installed (install.md, \"mono\"). On its own, and over USB-DAC or the radio, it always plays."
    }
}

// ── layout ─────────────────────────────────────────────────────────────────────────────────────
pub const STRIP_TOP: i32 = crate::chrome::HEADER_BOTTOM;
pub const ROW_SWITCH: i32 = STRIP_TOP + kit::STRIP_H;
const SOUND_LABEL: i32 = ROW_SWITCH + kit::ROW_H;
pub const GRID_TOP: i32 = SOUND_LABEL + kit::SECTION_H;
pub const COLS: usize = 2;
pub const GRID_ROWS: usize = SOUNDS.len().div_ceil(COLS);
const GRID_PITCH: i32 = kit::CHIP_H + kit::CHIP_GAP;
const GRID_BOTTOM: i32 = GRID_TOP + GRID_ROWS as i32 * GRID_PITCH - kit::CHIP_GAP;
const LEVEL_LABEL: i32 = GRID_BOTTOM + 8;
pub const ROW_ALONE: i32 = LEVEL_LABEL + kit::SECTION_H;
pub const ROW_MUSIC: i32 = ROW_ALONE + kit::ROW_H;
pub const NOTE_TOP: i32 = ROW_MUSIC + kit::ROW_H;

/// The cursor's stops for button navigation, top to bottom: the switch, each chip, the two levels.
pub const SEL_SWITCH: usize = 0;
pub const SEL_FIRST_CHIP: usize = 1;
pub const SEL_ALONE: usize = SEL_FIRST_CHIP + SOUNDS.len();
pub const SEL_MUSIC: usize = SEL_ALONE + 1;
pub const SEL_COUNT: usize = SEL_MUSIC + 1;

/// Top-left of chip `i` (in [`SOUNDS`] order) and its width.
pub fn chip_rect(i: usize) -> (i32, i32, i32) {
    let (x, w) = kit::chip_span(i % COLS, COLS);
    (x, GRID_TOP + (i / COLS) as i32 * GRID_PITCH, w)
}

/// Which chip is under `(x, y)`. Half the gap around each chip belongs to it, both ways.
pub fn chip_at(x: i32, y: i32) -> Option<usize> {
    if !(GRID_TOP - kit::CHIP_GAP / 2..GRID_BOTTOM + kit::CHIP_GAP / 2).contains(&y) {
        return None;
    }
    let row =
        ((y - GRID_TOP + kit::CHIP_GAP / 2) / GRID_PITCH).clamp(0, GRID_ROWS as i32 - 1) as usize;
    let chip_y = GRID_TOP + row as i32 * GRID_PITCH;
    let col = kit::chip_at(COLS, chip_y, x, chip_y + kit::CHIP_H / 2)?;
    let i = row * COLS + col;
    (i < SOUNDS.len()).then_some(i)
}

pub fn switch_hit(y: i32) -> bool {
    (ROW_SWITCH..ROW_SWITCH + kit::ROW_H).contains(&y)
}

/// Which level row is under `y`: 0 = on its own, 1 = with music.
pub fn level_row_at(y: i32) -> Option<usize> {
    if (ROW_ALONE..ROW_ALONE + kit::ROW_H).contains(&y) {
        Some(0)
    } else if (ROW_MUSIC..ROW_MUSIC + kit::ROW_H).contains(&y) {
        Some(1)
    } else {
        None
    }
}

const SLIDER_X0: i32 = 176;
const SLIDER_W: i32 = 200;

/// Is `x` on (or near) a level slider's track? Tapping the title does nothing; the track is the
/// target, with 20 px of grace either side so the ends are reachable.
pub fn slider_grab(x: i32) -> bool {
    (SLIDER_X0 - 20..SLIDER_X0 + SLIDER_W + 20).contains(&x)
}

/// A level for an x on the slider, clamped, in [`LEVEL_STEP`]s.
pub fn level_at(x: i32) -> u8 {
    let dx = (x - SLIDER_X0).clamp(0, SLIDER_W);
    let steps = 100 / LEVEL_STEP as i32;
    let k = (dx * steps * 2 + SLIDER_W) / (SLIDER_W * 2);
    (k * LEVEL_STEP as i32).clamp(0, 100) as u8
}

fn level_x(pct: u8) -> i32 {
    SLIDER_X0 + pct.min(100) as i32 * SLIDER_W / 100
}

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, sel: usize, v: &View) {
    c.fill(t.bg);
    let y = crate::chrome::header(c, t, f, "Soundscapes", None);
    debug_assert_eq!(y, STRIP_TOP);
    kit::strip(c, t, f, STRIP_TOP, &strip_text(v));
    let sub = if v.on { label(v.sound) } else { "Off" };
    kit::row(
        c,
        t,
        f,
        ROW_SWITCH,
        kit::ROW_H,
        &Row::new("Soundscape")
            .sub(sub)
            .trail(Trail::Switch(v.on))
            .sel(sel == SEL_SWITCH),
    );
    kit::section_label(c, t, f, SOUND_LABEL, "SOUND", None);
    for (i, (id, name)) in SOUNDS.iter().enumerate() {
        let (x, y, w) = chip_rect(i);
        let chosen = *id == v.sound;
        // The chosen sound stays marked while switched off, outlined rather than filled: it is
        // what the switch will bring back, not what is playing.
        if chosen && v.on {
            fill_rect(c, x, y, w, kit::CHIP_H, t.acc);
        } else if chosen {
            crate::widgets::stroke_rect(c, x, y, w, kit::CHIP_H, t.acc, 2);
        } else {
            crate::widgets::stroke_rect(c, x, y, w, kit::CHIP_H, t.ctrl(), 1);
        }
        if sel == SEL_FIRST_CHIP + i {
            crate::widgets::stroke_rect(c, x - 3, y - 3, w + 6, kit::CHIP_H + 6, t.ink, 1);
        }
        let ink = if chosen && v.on { t.acc_ink } else { t.ink };
        let st = sty(
            Family::Sans,
            Weight::SemiBold,
            crate::scale::SECONDARY,
            ink,
            0.0,
        );
        let s = fit(f, name, &st, (w - 16) as f32);
        let tw = text::measure(f, &s, &st);
        text::draw(
            c,
            f,
            x as f32 + (w as f32 - tw) / 2.0,
            (y + kit::CHIP_H / 2 + 6) as f32,
            &s,
            &st,
        );
    }
    kit::section_label(c, t, f, LEVEL_LABEL, "LEVEL", None);
    level_row(
        c,
        t,
        f,
        ROW_ALONE,
        "On its own",
        v.alone,
        sel == SEL_ALONE,
        v.on,
    );
    level_row(
        c,
        t,
        f,
        ROW_MUSIC,
        "With music",
        v.music,
        sel == SEL_MUSIC,
        v.on,
    );
    // The note: two lines at most, dim, wrapped by words.
    let st = sty(
        Family::Sans,
        Weight::Regular,
        crate::scale::SECONDARY,
        t.dim,
        0.0,
    );
    let mut y = NOTE_TOP + 26;
    for line in crate::track_info::wrap(f, reach_text(v.hook), &st, (kit::RIGHT - kit::LEFT) as f32)
        .iter()
        .take(3)
    {
        text::draw(c, f, kit::LEFT as f32, y as f32, line, &st);
        y += 21;
    }
}

fn level_row(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    y: i32,
    title: &str,
    pct: u8,
    sel: bool,
    on: bool,
) {
    kit::row(
        c,
        t,
        f,
        y,
        kit::ROW_H,
        &Row::new(title)
            .trail(Trail::Reserve(W as i32 - SLIDER_X0 + 6))
            .sel(sel),
    );
    let cy = y + kit::ROW_H / 2;
    // Dimmed, not hidden, while switched off: the levels can be set before turning it on.
    let fg = if on { t.acc } else { t.faint };
    fill_rect(c, SLIDER_X0, cy - 1, SLIDER_W, 2, t.line);
    let kx = level_x(pct);
    fill_rect(c, SLIDER_X0, cy - 1, kx - SLIDER_X0, 2, fg);
    fill_rect(c, kx - 7, cy - 9, 14, 18, fg);
    right(
        c,
        f,
        kit::RIGHT as f32,
        (cy + 5) as f32,
        &format!("{pct}%"),
        &sty(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            t.faint,
            0.04,
        ),
    );
    hline(c, y + kit::ROW_H - 1, t.line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_fits_above_the_now_playing_bar() {
        let note_bottom = NOTE_TOP + 26 + 2 * 21 + 6;
        assert!(
            note_bottom <= crate::H as i32 - crate::chrome::NP_BAR_H,
            "Soundscapes runs to {note_bottom}"
        );
    }

    #[test]
    fn every_sound_id_is_one_soundscape_h_knows_and_none_repeats() {
        let mut seen = [false; MAX_ID as usize + 1];
        for (id, name) in SOUNDS {
            assert!((1..=MAX_ID).contains(&id), "{name}: id {id}");
            assert!(!seen[id as usize], "{name}: id {id} twice");
            seen[id as usize] = true;
        }
        assert!(
            seen[1..].iter().all(|s| *s),
            "every id 1..={MAX_ID} has a chip"
        );
        assert_eq!(label(DEFAULT_SOUND), "Rain");
        assert_eq!(label(0), "Off");
        assert_eq!(label(99), "Off");
    }

    /// The centre of every chip hits that chip, gaps between rows belong to a neighbour, and the
    /// labels and the switch are not chips.
    #[test]
    fn chips_hit_what_they_draw() {
        for i in 0..SOUNDS.len() {
            let (x, y, w) = chip_rect(i);
            assert_eq!(chip_at(x + w / 2, y + kit::CHIP_H / 2), Some(i));
            assert_eq!(chip_at(x + 2, y + 1), Some(i));
            assert_eq!(chip_at(x + w - 2, y + kit::CHIP_H - 2), Some(i));
        }
        for y in GRID_TOP..GRID_BOTTOM {
            assert!(chip_at(100, y).is_some(), "a dead strip at y {y}");
        }
        assert_eq!(
            chip_at(100, GRID_TOP - kit::CHIP_GAP),
            None,
            "the SOUND label is not a chip"
        );
        assert!(!switch_hit(GRID_TOP));
        assert_eq!(
            level_row_at(GRID_BOTTOM + 2),
            None,
            "the LEVEL label is not a slider"
        );
    }

    #[test]
    fn the_slider_spans_zero_to_one_hundred_in_steps() {
        assert_eq!(level_at(0), 0);
        assert_eq!(level_at(SLIDER_X0), 0);
        assert_eq!(level_at(SLIDER_X0 + SLIDER_W), 100);
        assert_eq!(level_at(479), 100);
        assert_eq!(level_at(level_x(60)), 60);
        for x in SLIDER_X0..SLIDER_X0 + SLIDER_W {
            assert_eq!(level_at(x) % LEVEL_STEP, 0);
        }
        assert!(slider_grab(SLIDER_X0 - 10) && slider_grab(SLIDER_X0 + SLIDER_W + 10));
        assert!(!slider_grab(40), "the title is not the slider");
    }

    /// Off is silence; full is unity; the curve is even in decibels and never goes backwards.
    #[test]
    fn the_level_curve_is_even_in_decibels() {
        assert_eq!(gain_milli(0), 0);
        assert_eq!(gain_milli(100), 1000);
        assert!(
            (14..=18).contains(&gain_milli(1)),
            "1% is about -36 dB: {}",
            gain_milli(1)
        );
        let half = gain_milli(50) as f32 / 1000.0;
        assert!(
            (20.0 * half.log10() + 18.2).abs() < 0.5,
            "50% is about -18 dB"
        );
        let mut last = 0;
        for p in 0..=100 {
            let g = gain_milli(p);
            assert!(g >= last, "{p}%");
            last = g;
        }
    }

    #[test]
    fn the_strip_says_what_and_where() {
        let v = View {
            on: true,
            sound: 6,
            alone: 60,
            music: 30,
            route: Route::AloneJack,
            hook: false,
        };
        assert_eq!(strip_text(&v), "BEACH · ON ITS OWN · HEADPHONES");
        assert_eq!(
            strip_text(&View {
                route: Route::NoHook,
                ..v
            }),
            "BEACH · SILENT WHILE MUSIC PLAYS"
        );
        assert_eq!(
            strip_text(&View { on: false, ..v }),
            "OFF · TAP A SOUND TO START"
        );
        for c in 0..=7 {
            assert_eq!(Route::from_code(c).code(), c);
        }
        assert_eq!(Route::from_code(200), Route::Off);
    }
}
