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
    if screen == Screen::Keyboard {
        // A full-length name is the state that can overflow the field; an empty one never could.
        a.type_for_test(&"Late Night On The Bus Mix ".repeat(3));
    }
    a
}

const SCREENS: &[Screen] = &[
    Screen::Lock, Screen::NowPlaying, Screen::Menu, Screen::Library, Screen::Album,
    Screen::Artist, Screen::Playlist, Screen::UpNext, Screen::Eq, Screen::Sound,
    Screen::Bluetooth, Screen::Pairing, Screen::Settings, Screen::Fm, Screen::UsbDac,
    Screen::Receiver, Screen::Onboarding, Screen::UsbStorage, Screen::GenreFilter,
    Screen::Folders, Screen::TrackInfo, Screen::ClockSet, Screen::Advanced, Screen::Tone,
    Screen::Keyboard, Screen::PlaylistPick, Screen::TrackPick,
];

/// The keyboard's word keys (SHIFT / SPACE / DONE / 123) are drawn centred with no `fit`, so a
/// larger UI scale could push a label past its own key without ever leaving the panel — which the
/// off-panel audit would not see. Measure each label against the key it sits in, at every scale.
#[test]
fn keyboard_labels_fit_their_keys_at_every_scale() {
    use cinder_ui::keyboard::{key_at, key_rect, Key, ROWS};
    let mut bad: Vec<String> = Vec::new();
    for idx in 0..cinder_ui::text::SCALE_STEPS.len() {
        let _g = scale_lock(idx);
        let fonts = FontSet::load();
        for page in [0u8, 1] {
            for row in 0..ROWS {
                for col in 0.. {
                    let Some((_, _, w, _)) = key_rect(page, row, col) else { break };
                    let Some(key) = key_at(page, row, col) else { break };
                    let (label, size) = match key {
                        Key::Char(ch) => (ch.to_string(), 24.0),
                        Key::Shift => ("CAPS".to_string(), 15.0),
                        Key::Page => ("123".to_string(), 15.0),
                        Key::Space => ("SPACE".to_string(), 15.0),
                        Key::Backspace => ("DEL".to_string(), 15.0),
                        Key::Done => ("DONE".to_string(), 15.0),
                    };
                    let st = cinder_ui::widgets::sty(
                        cinder_ui::text::Family::Sans, cinder_ui::text::Weight::SemiBold, size,
                        cinder_ui::theme::Theme::day().ink, 0.0);
                    let measured = cinder_ui::text::measure(&fonts, &label, &st);
                    if measured > (w - 6) as f32 {
                        bad.push(format!("scale {idx}: {label:?} is {measured:.0}px in a {w}px key"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "keyboard labels overflow their keys:\n{}", bad.join("\n"));
}

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


/// EVERY onboarding page, at every scale, day and night.
///
/// `SCREENS` only ever renders `Screen::Onboarding` at page 0, so pages 1-3 — Controls, Features
/// and Done — had no coverage at all until a device report of a crash "on the 3rd page" (2026-08-19)
/// went looking for it. A paged screen needs its pages walked, not just its entry state.
#[test]
fn every_onboarding_page_stays_on_the_panel_at_every_scale() {
    let fonts = FontSet::load();
    let np = np_hostile();
    let mut bad: Vec<String> = Vec::new();
    for idx in 0..cinder_ui::text::SCALE_STEPS.len() {
        let _g = scale_lock(idx);
        let pct = cinder_ui::text::SCALE_STEPS[idx];
        for night in [false, true] {
            let mut a = at(Screen::Onboarding);
            a.night = night;
            for page in 0..cinder_ui::onboarding::PAGES {
                // Rendering is the test as much as the count is: a panic in a glyph, an icon or a
                // layout helper takes the whole app down on device (Rust panics abort into
                // appmgr's SIGCHLD reboot), and only these pages draw these strings.
                let n = overflow_of(&mut a, &fonts, &np);
                if n > 0 {
                    bad.push(format!("onboarding page {page} @ {pct}% night={night}: {n} px past the margin"));
                }
                a.press(Button::Select);   // next page (finishes on the last, which is fine here)
            }
        }
    }
    let _restore = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
    assert!(bad.is_empty(), "onboarding content is clipped:\n  {}", bad.join("\n  "));
}


/// NO PIECE OF UI CHROME MAY NEED SONY'S FONTS.
///
/// The device font chain is five files, ~18 MB on disk and ~250 MB once fontdue has parsed them.
/// On 2026-08-19 a single character in the onboarding Features page — `▸` in "Settings ▸ Theme",
/// which Hanken Grotesk does not carry — walked that whole chain looking for a glyph none of the
/// five has, and the kernel killed the app:
///
///     Out of memory: Kill process 1700 (cinder-probe) ... anon-rss:251472kB
///
/// For cinder-home that is a device REBOOT, because appmgr reboots when its foreground app dies.
/// `resolve` now tries the other bundled family first and gates the chain by script, so `▸` is
/// found in JetBrains Mono for free — and this test is the rule that keeps it that way: with a
/// Latin-only library and Latin-only track metadata, rendering every screen must never once reach
/// the chain. Non-Latin *tags* legitimately do; the UI's own strings never may.
#[test]
fn ui_chrome_never_reaches_the_device_font_chain() {
    let _g = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
    let fonts = FontSet::load();
    let np = np_plain();
    for &s in SCREENS {
        let mut a = at(s);
        a.set_library(cinder_ui::model::Library::default()); // no sample rows: chrome only
        for night in [false, true] {
            a.night = night;
            let mut c = Canvas::new();
            a.render(&mut c, &fonts, &np);
        }
    }
    // Every onboarding page, since that is where this bug lived and only page 0 is in SCREENS.
    let mut a = at(Screen::Onboarding);
    a.set_library(cinder_ui::model::Library::default());
    for _ in 0..cinder_ui::onboarding::PAGES {
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &np);
        a.press(Button::Select);
    }
    // ASSERT ON WHAT NOTHING COULD DRAW, not on what reached the chain.
    //
    // This test used to filter `chain_char_list()` down to symbols — and could therefore never
    // fail, because `fallback_covers_script` gates every symbol out BEFORE that set is recorded.
    // It watched a set the code cannot put a symbol into, which is worse than no test: it read as
    // a guard on the bug that OOM-killed the app.
    //
    // The honest signal is `unresolved_chars`: characters the requested font lacked, that no
    // script-eligible fallback had, and that no other bundled face had either. Those draw a
    // `.notdef` box. Non-Latin TAGS legitimately borrow Sony's fonts and are fine — but chrome is
    // ours, and a box in it is a bug whether or not the chain was involved.
    // Characters in a script the DEVICE chain provides are content, and unresolvable here only
    // because this host has no Sony font directory. What must never appear is a character no font
    // anywhere could draw — that is a box on the device too.
    let missing: Vec<char> = fonts
        .unresolved_chars()
        .into_iter()
        .filter(|c| !cinder_ui::text::device_chain_covers(*c))
        .collect();
    assert!(
        missing.is_empty(),
        "UI chrome used glyphs no font on the device can draw, so they render as \u{25a1} boxes: \
         {}\n\
         Pick a character Hanken Grotesk or JetBrains Mono actually has. (Before the script gate \
         existed, one of these also walked the whole Sony font chain — ~250 MB parsed — and the \
         kernel OOM-killed the app, which for cinder-home means a REBOOT.)",
        missing
            .iter()
            .map(|c| format!("U+{:04X} {c:?}", *c as u32))
            .collect::<Vec<_>>()
            .join(", ")
    );
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

/// Every VPT room and DC Phase filter label must fit its pill, at every UI scale.
///
/// The rest of this file drives screens through the navigator, which leaves VPT OFF — so the pill
/// only ever renders "OFF" and the audit never sees a room name. "Concert Hall" is four times as
/// long as "Off" and is right-aligned against the same margin, so it is exactly the case that
/// would slip through. Added with the VPT rooms (2026-08-17).
#[test]
fn effect_enum_labels_fit_at_every_scale() {
    let fonts = FontSet::load();
    let np = np_plain();
    for idx in 0..cinder_ui::text::SCALE_STEPS.len() {
        let _g = scale_lock(idx);
        let pct = cinder_ui::text::SCALE_STEPS[idx];
        for room in 0..cinder_ui::nav::VPT_MODES.len() {
            let mut a = at(Screen::Sound);
            a.set_sound_flags(1 << 2); // VPT on — the pill shows a room, not "Off"
            a.set_vpt_mode(room);
            let oob = overflow_of(&mut a, &fonts, &np);
            assert_eq!(
                oob, 0,
                "VPT room {:?} overflows at {}% UI scale ({} px past the margin)",
                cinder_ui::nav::VPT_MODES[room], pct, oob
            );
        }
        // Sound ▸ Advanced: its two pills carry the longest labels on the screen, and the
        // override banner only appears when something upstream is bypassing the chain — so the
        // sweep above, which never turns those on, would not draw either of them.
        for f in [0u8, 0b0000_1001, 0b0001_1111] {
            for mode in 0..cinder_ui::advanced::DSEE_MODES.len() {
                for vt in 0..cinder_ui::advanced::VINYL_TYPES.len() {
                    let mut a = at(Screen::Advanced);
                    a.set_adv_flags(f);
                    a.set_dsee_mode(mode);
                    a.set_vinyl_type(vt);
                    let oob = overflow_of(&mut a, &fonts, &np);
                    assert_eq!(
                        oob, 0,
                        "Advanced overflows at {pct}% (flags {f:#07b}, dsee {mode}, vinyl {vt}): {oob} px"
                    );
                }
            }
        }
        // Tone Control: its state line is the variable-width part, and the longest wording only
        // appears when something upstream is bypassing the chain — "<name> is on — nothing here is
        // in the path", where <name> is a control's name and so not a width this file may assume.
        // The default state never draws it, so the sweep above would miss it entirely; same reason
        // the Advanced banner gets its own pass.
        for f in [0u8, 0b0001_0000, 0b0001_0001] {
            let mut a = at(Screen::Tone);
            a.set_adv_flags(f);
            a.set_tone_bands([cinder_ui::tone::BAND_MAX; cinder_ui::tone::BANDS]);
            let oob = overflow_of(&mut a, &fonts, &np);
            assert_eq!(oob, 0, "Tone Control overflows at {pct}% (flags {f:#07b}): {oob} px");
            // …and with ClearAudio+ upstream, which names a different control in the same line.
            let mut a = at(Screen::Tone);
            a.set_adv_flags(f);
            a.set_sound_flags(1 << 5);
            a.set_tone_bands([-cinder_ui::tone::BAND_MAX; cinder_ui::tone::BANDS]);
            let oob = overflow_of(&mut a, &fonts, &np);
            assert_eq!(oob, 0, "Tone Control overflows under ClearAudio+ at {pct}%: {oob} px");
        }
        for ty in 0..cinder_ui::nav::DC_PHASE_TYPES.len() {
            let mut a = at(Screen::Sound);
            a.set_sound_flags(1 << 3); // DC Phase on — the pill shows a filter type, not "Off"
            a.set_dc_type(ty);
            let oob = overflow_of(&mut a, &fonts, &np);
            assert_eq!(
                oob, 0,
                "DC Phase type {:?} overflows at {}% UI scale ({} px past the margin)",
                cinder_ui::nav::DC_PHASE_TYPES[ty], pct, oob
            );
        }
    }
    let _restore = scale_lock(cinder_ui::text::SCALE_DEFAULT_IDX);
}

/// The previous version of the chrome test could not fail, and nobody noticed for weeks. So prove
/// the detector fires: draw a character no bundled font has and that the device chain is not for,
/// and require it to be reported. If this ever goes quiet, the chrome assertion above is decorative
/// again.
#[test]
fn the_unresolved_detector_actually_fires() {
    use cinder_ui::text::{self, Family, Weight};
    let fonts = FontSet::load();
    let mut c = Canvas::new();
    assert!(
        fonts.unresolved_chars().is_empty(),
        "nothing has been drawn yet, so nothing can be unresolved"
    );
    // U+2603 SNOWMAN: not in Hanken Grotesk or JetBrains Mono, and outside every script the Sony
    // fallbacks exist to provide — so no font anywhere on the device can draw it.
    assert!(!text::device_chain_covers('\u{2603}'), "pick a character the chain is NOT for");
    let st = cinder_ui::widgets::sty(
        Family::Sans, Weight::Regular, 18.0, cinder_ui::theme::Theme::day().ink, 0.0);
    text::draw(&mut c, &fonts, 10.0, 40.0, "\u{2603}", &st);
    assert_eq!(
        fonts.unresolved_chars(),
        vec!['\u{2603}'],
        "the detector did not notice a glyph nothing can draw"
    );
}
