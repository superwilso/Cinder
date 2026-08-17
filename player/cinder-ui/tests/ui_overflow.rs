//! Every screen, audited for content that falls off the panel.
//!
//! `Canvas` clips every write. That is correct at runtime — a stray glyph must never scribble
//! outside the framebuffer — but it means a layout bug has no symptom: text that runs past the
//! right margin, a row drawn below the bottom edge, a value too wide for its pill all just
//! *disappear*, and the screenshot looks slightly tight rather than wrong. `Canvas::oob()` counts
//! the pixels that were asked for and thrown away, which turns "does anything clip?" into an
//! assertion instead of an inspection.
//!
//! Clip-BAND rejections are not counted (see `Canvas::oob`): a list sets a band precisely so
//! half-scrolled rows are cut off, and that is the feature working.
//!
//! Text is exercised with long, real-world-hostile strings — the CJK/emoji-bearing tag values and
//! 90-character album titles that actually exist in a library — because a layout only overflows on
//! the content nobody had when they laid it out.

use cinder_ui::canvas::Canvas;
use cinder_ui::nav::{App, Button, Screen};
use cinder_ui::now_playing::NowPlaying;
use cinder_ui::text::FontSet;
use std::sync::{Mutex, MutexGuard};

/// The UI text scale is process-wide state (`text::set_scale_idx`), and cargo runs tests in
/// parallel threads of ONE process — so a scale set by one test leaks into another mid-render and
/// reports overflow against a size the test never asked for. This showed up immediately: the same
/// screen "overflowed" only on its night pass, purely because the scale test happened to be
/// running. Every test that renders takes this lock and sets the scale it wants.
static SCALE: Mutex<()> = Mutex::new(());

fn scale_lock(idx: usize) -> MutexGuard<'static, ()> {
    let g = SCALE.lock().unwrap_or_else(|e| e.into_inner());
    cinder_ui::text::set_scale_idx(idx);
    g
}

/// A now-playing payload with deliberately hostile strings: long, non-Latin, and full of the
/// wide glyphs that a proportional font makes longest.
fn np_hostile() -> NowPlaying<'static> {
    NowPlaying {
        title: "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto",
        artist: "Королевский филармонический оркестр / 東京都交響楽団 feat. A Very Long Guest Artist Name",
        codec: "FLAC · 24bit / 192.0 kHz · Hi-Res Audio Wireless",
        badge: "FLAC 24/192",
        clock: "23:59",
        battery: 100,
        elapsed: "1:47:22",
        remaining: "-2:45:09",
        progress: 0.39,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: true,
        playing: true,
        shuffle: true,
        repeat: 1,
        viz_seed: 2.0,
        viz_kind: 0,
        viz_size: 1,
        page: 0,
        viz_levels: None,
        scrubbing: false,
    }
}

fn np_plain() -> NowPlaying<'static> {
    NowPlaying { title: "Atlas Hands", artist: "Benjamin Francis Leftwich",
                 codec: "FLAC · 24bit / 96.0 kHz", badge: "FLAC 24/96", ..np_hostile() }
}

/// Render one screen and return how many pixels ran off the LEFT or RIGHT margin.
///
/// Horizontal only. Nothing on this device scrolls sideways, so a pixel past the margin is content
/// the user can never reach by any gesture — whereas drawing above or below the panel is what every
/// scrolling list does by construction.
///
/// Rendered twice: some screens advance animation state (a marquee on a title too long to fit) on
/// each frame, so a single frame can miss an overflow that only appears once the text has scrolled.
fn overflow_of(app: &mut App, fonts: &FontSet, np: &NowPlaying) -> u32 {
    let mut c = Canvas::new();
    app.render(&mut c, fonts, np);
    c.reset_oob();
    app.render(&mut c, fonts, np);
    c.oob_x()
}

/// Open a screen the way a user would, so the app state matches what the screen expects rather
/// than being reachable only by poking `stack` directly.
fn at(screen: Screen) -> App {
    let mut a = App::unlocked();
    if screen == Screen::NowPlaying {
        return a;
    }
    a.push_for_test(screen);
    a
}

const SCREENS: &[Screen] = &[
    Screen::Lock, Screen::NowPlaying, Screen::Menu, Screen::Library, Screen::Album,
    Screen::Artist, Screen::Playlist, Screen::UpNext, Screen::Eq, Screen::Sound,
    Screen::Bluetooth, Screen::Pairing, Screen::Settings, Screen::Fm, Screen::UsbDac,
    Screen::Receiver, Screen::Onboarding, Screen::UsbStorage, Screen::GenreFilter,
    Screen::Folders, Screen::TrackInfo, Screen::ClockSet,
];

#[test]
fn no_screen_draws_off_the_panel() {
    let _g = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
    let fonts = FontSet::load();
    let mut bad: Vec<String> = Vec::new();
    for &s in SCREENS {
        for (label, np) in [("plain", np_plain()), ("hostile", np_hostile())] {
            for night in [false, true] {
                let mut a = at(s);
                a.night = night;
                let n = overflow_of(&mut a, &fonts, &np);
                if n > 0 {
                    bad.push(format!("{s:?} [{label}, night={night}]: {n} px past the margin"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "content is being clipped away:\n  {}", bad.join("\n  "));
}

/// The same audit at every UI scale. This is where overflow actually bites: the layout was drawn
/// at 100% and the scale setting multiplies text without moving the boxes around it, so a label
/// that just fits at 100% runs off the edge at 140%.
#[test]
fn no_screen_draws_off_the_panel_at_any_ui_scale() {
    let fonts = FontSet::load();
    let mut bad: Vec<String> = Vec::new();
    for idx in 0..cinder_ui::text::SCALE_STEPS.len() {
        let _g = scale_lock(idx);
        let pct = cinder_ui::text::SCALE_STEPS[idx];
        for &s in SCREENS {
            let mut a = at(s);
            let n = overflow_of(&mut a, &fonts, &np_hostile());
            if n > 0 {
                bad.push(format!("{s:?} @ {pct}%: {n} px past the margin"));
            }
        }
    }
    // Leave the process on the default scale for whatever runs next.
    let _restore = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
    assert!(bad.is_empty(), "content is clipped at non-default UI scale:\n  {}", bad.join("\n  "));
}

/// Modals and overlays draw ON TOP of a screen, so they get their own pass — they are the most
/// likely thing to overflow (a confirm dialog sizes itself around text it did not choose).
#[test]
fn overlays_and_modals_stay_on_the_panel() {
    let _g = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
    let fonts = FontSet::load();
    let np = np_hostile();
    let mut bad: Vec<String> = Vec::new();

    // The volume HUD.
    let mut a = App::unlocked();
    a.press(Button::VolUp);
    let n = overflow_of(&mut a, &fonts, &np);
    if n > 0 {
        bad.push(format!("volume HUD: {n} px"));
    }

    // The shelf.
    let mut a = App::unlocked();
    a.open_shelf_for_test();
    let n = overflow_of(&mut a, &fonts, &np);
    if n > 0 {
        bad.push(format!("shelf: {n} px"));
    }

    // Every confirm dialog, each of which owns its own body text.
    for ask in cinder_ui::confirm::ALL {
        let mut a = App::unlocked();
        a.open_confirm_for_test(*ask);
        let n = overflow_of(&mut a, &fonts, &np);
        if n > 0 {
            bad.push(format!("confirm {ask:?}: {n} px"));
        }
    }

    assert!(bad.is_empty(), "an overlay is clipped:\n  {}", bad.join("\n  "));
}
