use super::*;

fn unlocked() -> App {
    App::unlocked()
}

/// The Menu subtitle contract: a row's literal is EMPTY exactly when render fills it from live
/// state, and non-empty only when the text is genuinely fixed. This is the guard against the
/// prototype's mock strings creeping back — those literals ("124 albums · 1,842 tracks",
/// "88.6 MHz", "WH-1000XM5 · LDAC") stated things that were not true of the device.
/// If you add a Menu row, either give it a static subtitle that is always true, or leave it
/// empty AND add an arm to the `value:` match in render.
#[test]
fn menu_rows_with_no_static_subtitle_are_the_ones_render_fills_in() {
    // Compared as sorted debug names — Screen deliberately isn't Ord.
    let mut dynamic: Vec<String> = MENU
        .iter()
        .filter(|(_, _, value)| value.is_empty())
        .map(|(screen, _, _)| format!("{screen:?}"))
        .collect();
    dynamic.sort();
    let mut expected: Vec<String> = [
        // Now Playing and Up Next left the table in the 2026-09 redesign: the strip says what
        // is playing, and the queue is one tap from Now Playing's toolbar.
        Screen::Library,
        Screen::Folders,
        Screen::SensMe, // live: how many channels, and how many tracks were analysed
        Screen::Zappin, // the standalone preview entry has no menu-body subtitle when ready
        // Screen::Fm left this list on 2026-08-18: the tuner is wired, so the row now carries
        // a real static subtitle ("Needs wired headphones as the aerial") instead of the empty
        // string it used while there was nothing behind it. Nothing fills it at render time.
        // Screen::Eq left the Menu on 2026-10-04; Sound's Equalizer row is the way in.
        Screen::Sound,
        Screen::Soundscape, // live: the sound, or Off
        Screen::Bluetooth,
        Screen::Wifi, // live: off, or the network
        Screen::UsbDac,
    ]
    .iter()
    .map(|s| format!("{s:?}"))
    .collect();
    expected.sort();
    assert_eq!(dynamic, expected);
}

/// Soundscapes: a chip plays its sound (switching it on), the playing chip again does nothing,
/// the switch remembers the sound, and the shell is told on every change and only then.
#[test]
fn soundscape_chips_play_and_the_switch_remembers_the_sound() {
    use crate::soundscape as ss;
    let mut a = unlocked();
    a.push_for_test(Screen::Soundscape);
    assert_eq!(
        a.ambient(),
        (0, ss::DEFAULT_ALONE, ss::DEFAULT_MUSIC),
        "off on a fresh install"
    );
    let beach = ss::SOUNDS.iter().position(|s| s.0 == 6).unwrap();
    let (x, y, w) = ss::chip_rect(beach);
    assert_eq!(
        a.tap(x + w / 2, y + kit_mid()),
        vec![Action::AmbientChanged]
    );
    assert_eq!(a.ambient().0, 6, "tapping Beach plays the beach");
    assert_eq!(
        a.tap(x + w / 2, y + kit_mid()),
        vec![],
        "tapping it again changes nothing"
    );
    assert_eq!(
        a.tap(240, ss::ROW_SWITCH + 20),
        vec![Action::AmbientChanged]
    );
    assert_eq!(a.ambient().0, 0, "the switch turns it off");
    assert_eq!(a.ambient_sound(), 6, "…and keeps the sound");
    assert_eq!(
        a.tap(240, ss::ROW_SWITCH + 20),
        vec![Action::AmbientChanged]
    );
    assert_eq!(a.ambient().0, 6, "on again: the beach, not the default");
    // The Menu row says what is playing.
    assert_eq!(a.menu_subtitles().soundscape, "Beach");
    a.set_ambient_on(false);
    assert_eq!(a.menu_subtitles().soundscape, "Off");
}

fn kit_mid() -> i32 {
    crate::kit::CHIP_H / 2
}

/// The level sliders: a tap on the track sets it, a drag reports each new step once, the
/// release reports once more (so the settings file gets the final value), and buttons step by
/// five. The title is not a slider.
#[test]
fn soundscape_levels_follow_the_finger_and_the_buttons() {
    use crate::soundscape as ss;
    let mut a = unlocked();
    a.push_for_test(Screen::Soundscape);
    assert_eq!(
        a.tap(40, ss::ROW_ALONE() + 30),
        vec![],
        "the title is not the slider"
    );
    assert_eq!(
        a.tap(376, ss::ROW_ALONE() + 30),
        vec![Action::AmbientChanged]
    );
    assert_eq!(a.ambient().1, 100);
    assert!(a.scrub_begin(176, ss::ROW_MUSIC() + 30));
    assert_eq!(a.ambient().2, 0);
    assert_eq!(
        a.scrub_move(177, ss::ROW_MUSIC() + 30),
        vec![],
        "the same step: nothing to say"
    );
    assert_eq!(
        a.scrub_move(276, ss::ROW_MUSIC() + 30),
        vec![Action::AmbientChanged]
    );
    assert_eq!(a.ambient().2, 50);
    assert_eq!(a.scrub_end(), vec![Action::AmbientChanged]);
    assert!(!a.scrub_is_rail(), "never a seek");
    // buttons: down to the With music row, then right
    for _ in 0..ss::SEL_MUSIC {
        a.press(Button::Down);
    }
    assert_eq!(a.press(Button::Right), vec![Action::AmbientChanged]);
    assert_eq!(a.ambient().2, 55);
    a.press(Button::Up);
    for _ in 0..30 {
        a.press(Button::Right);
    }
    assert_eq!(a.ambient().1, 100, "clamped at 100");
    assert_eq!(a.press(Button::Right), vec![], "nothing to say at the top");
}

/// The Menu has a Soundscapes row and it opens the page.
#[test]
fn the_menu_opens_soundscapes() {
    let mut a = unlocked();
    a.push_for_test(Screen::Menu);
    let row = a
        .menu_visible()
        .iter()
        .position(|m| m.0 == Screen::Soundscape)
        .expect("a Menu row");
    a.activate_menu(row);
    assert_eq!(a.current(), Screen::Soundscape);
}

/// Settings from a hand-edited file cannot reach the shell as nonsense.
#[test]
fn soundscape_setters_refuse_what_the_shell_cannot_play() {
    let mut a = unlocked();
    a.set_ambient_sound(42);
    assert_eq!(a.ambient_sound(), crate::soundscape::DEFAULT_SOUND);
    a.set_ambient_sound(0);
    assert_eq!(
        a.ambient_sound(),
        crate::soundscape::DEFAULT_SOUND,
        "0 is off, not a sound"
    );
    a.set_ambient_levels(250, 7);
    assert_eq!((a.ambient().1, a.ambient().2), (100, 7));
}

/// No static Menu subtitle may assert a count or a device name. Digits are the tell: every mock
/// string we removed had one ("124 albums", "8 tracks · 41:24", "88.6 MHz", "WH-1000XM5").
#[test]
fn static_menu_subtitles_never_assert_countable_state() {
    for (screen, label, value) in MENU.iter() {
        assert!(
            !value.chars().any(|c| c.is_ascii_digit()),
            "static subtitle for {label} ({screen:?}) asserts a number: {value:?} — \
                 make it live (empty literal + a render arm) or drop the claim"
        );
    }
}

/// The strip under the Menu's header goes to Now Playing — back DOWN the route when Now Playing
/// is under the Menu, so Back never bounces between the two.
#[test]
fn the_menu_strip_opens_now_playing() {
    let mut a = unlocked();
    a.press(Button::Up); // Now Playing → Menu, the usual way in
    assert_eq!(a.current(), Screen::Menu);
    assert!(a.tap(240, crate::menu::STRIP_TOP + 10).is_empty());
    assert_eq!(a.current(), Screen::NowPlaying);
    assert_eq!(a.stack, vec![Screen::NowPlaying], "popped, not pushed");
    // From a Menu opened over something else, it still lands on Now Playing.
    a.go(Screen::Settings);
    a.push(Screen::Menu);
    a.tap(240, crate::menu::STRIP_TOP + 10);
    assert_eq!(a.current(), Screen::NowPlaying);
}

/// START ON: the chips pick the home screen, the choice round-trips through its settings-file
/// word, and Library — the default — is tagged HOME on its row.
#[test]
fn the_start_on_chips_pick_the_home_screen() {
    let mut a = unlocked();
    assert_eq!(a.home_screen(), "library", "Library is the default");
    a.press(Button::Up);
    for (i, word) in ["library", "now_playing", "menu", "last"]
        .iter()
        .enumerate()
    {
        let (x, w) = crate::kit::chip_span(i, crate::menu::HOMES.len());
        assert!(a.tap(x + w / 2, crate::menu::CHIPS_TOP + 10).is_empty());
        assert_eq!(a.home_screen(), *word);
        assert_eq!(
            a.current(),
            Screen::Menu,
            "choosing a home does not navigate"
        );
    }
    let mut b = unlocked();
    b.set_home_screen("menu");
    assert_eq!(b.home_screen(), "menu");
    b.set_home_screen("nonsense");
    assert_eq!(b.home_screen(), "library");
}

/// `apply_home` opens the player where it was asked to, with Now Playing underneath so Back
/// always has somewhere to go — and it never overrides a route that already moved.
#[test]
fn apply_home_opens_the_chosen_screen() {
    for (word, want) in [
        ("library", vec![Screen::NowPlaying, Screen::Library]),
        ("now_playing", vec![Screen::NowPlaying]),
        ("menu", vec![Screen::NowPlaying, Screen::Menu]),
        // Last screen with nothing saved yet falls back to Library.
        ("last", vec![Screen::NowPlaying, Screen::Library]),
    ] {
        let mut a = unlocked();
        a.set_home_screen(word);
        a.apply_home();
        assert_eq!(a.stack, want, "home {word}");
    }
    let mut a = unlocked();
    a.push(Screen::Settings);
    a.apply_home();
    assert_eq!(
        a.current(),
        Screen::Settings,
        "a route that already moved is left alone"
    );
}

/// "Last screen" writes the topmost PLACE (never Display, a modal or the Menu itself), and a
/// fresh App given that line reopens it.
#[test]
fn last_screen_reopens_the_last_place() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    let mut a = unlocked();
    assert_eq!(
        a.home_last_encode(),
        "",
        "nothing is written unless Last screen is the home"
    );
    a.set_home_screen("last");
    a.go(Screen::Settings);
    a.push(Screen::Display);
    let line = a.home_last_encode();
    assert!(
        line.starts_with("settings|"),
        "Display is not a place; Settings under it is: {line}"
    );
    let mut b = unlocked();
    b.set_home_screen("last");
    b.set_home_last(&line);
    b.apply_home();
    assert_eq!(b.current(), Screen::Settings);
    // An empty library defers the restore to the first library, where album ids resolve.
    let mut c = unlocked();
    c.set_library(Library::default());
    c.set_home_screen("last");
    c.set_home_last("lib|albums|0|0|-1|0|0|0|0|0|Library|x");
    c.apply_home();
    assert_eq!(c.current(), Screen::NowPlaying, "waits for the library");
    c.set_library(Library::sample());
    assert_eq!(
        c.current(),
        Screen::Library,
        "and opens it when the library arrives"
    );
}

/// The live subtitles must report this App's real state. (These are the actual strings the Menu
/// draws; see App::menu_subtitles.) A fresh App still holds `Library::sample()` — the shell
/// replaces it in cinder_db_open, and substitutes an EMPTY library if the DB fails to load, so
/// the sample never stands in for the user's music on device. Both cases are asserted here.
#[test]
fn live_menu_subtitles_report_real_state() {
    let mut app = unlocked();
    // Library caption counts whatever library is actually loaded, not a fixed number.
    let subs = app.menu_subtitles();
    assert_eq!(
        subs.library,
        format!(
            "{} albums · {} tracks",
            app.lib.album_count(),
            app.lib.songs.len()
        )
    );
    // An empty library says so rather than showing a count of nothing.
    app.set_library(Library::default());
    let subs = app.menu_subtitles();
    assert_eq!(subs.library, "Empty");
    assert_eq!(subs.usb_dac, "Off");
    assert_eq!(subs.sound, "Off", "no effect is engaged on a fresh App");
    // The BT codec names whatever is SELECTED — a real value from the real table, indexed by
    // the App's own selection.
    assert_eq!(
        subs.bluetooth,
        crate::bluetooth::CODECS[app.bt_codec as usize].0
    );
    // And specifically: no invented headphones, anywhere.
    assert!(!subs.bluetooth.contains("WH-"));
}

/// Turning effects on changes the Sound subtitle to name them — proving it tracks state rather
/// than being a fixed feature list (it used to read "DSEE HX · VPT · Vinyl" unconditionally).
#[test]
fn sound_subtitle_names_only_the_engaged_effects() {
    let mut app = unlocked();
    app.snd_dsee = true;
    app.snd_vpt = true;
    assert_eq!(app.menu_subtitles().sound, "DSEE HX · VPT");
    // ClearAudio+ replaces the manual effects, so it is named INSTEAD of them — by its own
    // name (it used to be printed as "Clear Phase", a different effect).
    app.snd_clear = true;
    assert_eq!(app.menu_subtitles().sound, "ClearAudio+");
    app.snd_norm = true;
    assert_eq!(app.menu_subtitles().sound, "ClearAudio+ · Normalizer");
    // Source Direct bypasses everything, so it is the whole answer.
    app.adv_source_direct = true;
    assert_eq!(app.menu_subtitles().sound, "Source Direct");
    app.adv_source_direct = false;
    app.snd_clear = false;
    app.snd_norm = false;
    app.snd_dsee = false;
    app.snd_vpt = false;
    assert_eq!(app.menu_subtitles().sound, "Off");
}

/// The VPT row is ONE control with five states, cycled by tap, because this device has no
/// d-pad — a hidden mode reachable only by Left/Right would be unreachable in practice. The
/// row's own subtitle promises "Studio / Club / Concert Hall", so the pill has to be able to
/// show them; before this it only ever said On or Off.
#[test]
fn vpt_row_cycles_off_through_every_room_and_back() {
    let mut app = unlocked();
    app.sound_sel = 2;
    assert!(!app.snd_vpt, "starts off");

    let mut seen = Vec::new();
    for _ in 0..VPT_MODES.len() {
        app.sound_toggle_row();
        assert!(app.snd_vpt, "still on part-way through the rooms");
        seen.push(VPT_MODES[app.snd_vpt_mode]);
    }
    assert_eq!(
        seen,
        VPT_MODES.to_vec(),
        "visits every room, in catalogue order"
    );

    // one more tap wraps back to off, and resets the room so the next cycle starts clean
    app.sound_toggle_row();
    assert!(!app.snd_vpt, "wraps back to off");
    assert_eq!(app.snd_vpt_mode, 0);
}

/// The room travels with the A/B setup — otherwise swapping setups would keep whichever room
/// happened to be live, which is exactly the kind of silent carry-over the A/B control exists
/// to rule out.
#[test]
fn vpt_mode_is_part_of_the_setup_snapshot() {
    let mut app = unlocked();
    app.snd_vpt = true;
    app.snd_vpt_mode = 2;
    let saved = app.setup();
    assert_eq!(saved.vpt_mode, 2);

    app.snd_vpt_mode = 0;
    app.set_setup(saved);
    assert_eq!(app.snd_vpt_mode, 2, "restored from the setup");
}

/// A hand-edited settings file must not be able to hand the device an out-of-range enum.
#[test]
fn vpt_mode_is_clamped_on_the_way_in() {
    let mut app = unlocked();
    app.set_vpt_mode(99);
    assert_eq!(app.vpt_mode(), VPT_MODES.len() - 1);
}

/// DC Phase is the same one-control-many-states shape as VPT, for the same no-d-pad reason.
#[test]
fn dc_phase_row_cycles_every_filter_type_and_back() {
    let mut app = unlocked();
    app.sound_sel = 3;
    assert!(!app.snd_dc, "starts off");

    let mut seen = Vec::new();
    for _ in 0..DC_PHASE_TYPES.len() {
        app.sound_toggle_row();
        assert!(app.snd_dc);
        seen.push(DC_PHASE_TYPES[app.snd_dc_type]);
    }
    assert_eq!(
        seen,
        DC_PHASE_TYPES.to_vec(),
        "visits every filter, in catalogue order"
    );

    app.sound_toggle_row();
    assert!(!app.snd_dc, "wraps back to off");
    assert_eq!(app.snd_dc_type, 0);
}

/// The filter type travels with the A/B setup, and is clamped on the way in.
#[test]
fn dc_type_rides_the_setup_and_is_clamped() {
    let mut app = unlocked();
    app.snd_dc = true;
    app.snd_dc_type = 4;
    let saved = app.setup();
    assert_eq!(saved.dc_type, 4);
    app.snd_dc_type = 0;
    app.set_setup(saved);
    assert_eq!(app.snd_dc_type, 4);

    app.set_dc_type(99);
    assert_eq!(app.dc_type(), DC_PHASE_TYPES.len() - 1);
}

/// The Advanced row is a ROUTE, not a setting: it must push the screen and emit NO action.
/// Emitting SoundChanged here would re-apply the whole effect chain on a navigation, which is
/// how "my audio blipped when I opened a menu" starts.
#[test]
fn advanced_row_opens_the_screen_without_touching_the_sound() {
    let mut app = unlocked();
    app.push_for_test(Screen::Sound);
    app.sound_sel = crate::sound::ROW_ADVANCED;
    let acts = app.sound_toggle_row();
    assert!(acts.is_empty(), "navigation must not emit a sound action");
    assert_eq!(app.current(), Screen::Advanced);
    assert_eq!(
        app.adv_sel,
        crate::advanced::ROW_VPT,
        "focus starts at the top of the new screen"
    );
}

/// DSEE HX Custom cycles Off -> each mode -> Off, like VPT.
#[test]
fn dsee_custom_cycles_every_mode_and_back() {
    use crate::advanced::DSEE_MODES;
    let mut app = unlocked();
    app.adv_sel = crate::advanced::ROW_DSEE_CUSTOM;
    let mut seen = Vec::new();
    for _ in 0..DSEE_MODES.len() {
        app.advanced_toggle_row();
        assert!(app.adv_dsee_custom);
        seen.push(DSEE_MODES[app.adv_dsee_mode]);
    }
    assert_eq!(seen, DSEE_MODES.to_vec());
    app.advanced_toggle_row();
    assert!(!app.adv_dsee_custom, "wraps back to off");
    assert_eq!(app.adv_dsee_mode, 0);
}

/// Vinyl character has NO off state — the Vinyl Processor's own switch lives on the Sound
/// screen, and this only says which character that switch turns on. So it wraps through the
/// four and never lands on something meaning "none".
#[test]
fn vinyl_character_wraps_without_an_off_state() {
    use crate::advanced::VINYL_TYPES;
    let mut app = unlocked();
    app.adv_sel = crate::advanced::ROW_VINYL_TYPE;
    for i in 0..VINYL_TYPES.len() * 2 {
        let before = app.adv_vinyl_type;
        app.advanced_toggle_row();
        assert_eq!(
            app.adv_vinyl_type,
            (before + 1) % VINYL_TYPES.len(),
            "step {i}"
        );
    }
    assert_eq!(
        app.adv_vinyl_type, 0,
        "back where it started after two laps"
    );
}

/// The packed flags must round-trip — they are what gets persisted and handed to the shell.
#[test]
fn advanced_flags_round_trip() {
    let mut app = unlocked();
    for f in [0u8, 0b1_0101, 0b1_1111, 0b10_0000, 0b11_1111] {
        app.set_adv_flags(f);
        assert_eq!(app.adv_flags(), f, "flags {f:#08b}");
    }
    app.set_adv_flags(0b1_1111);
    assert!(
        app.adv_source_direct
            && app.adv_clear_phase
            && app.adv_dsee_ai
            && app.adv_dsee_custom
            && app.adv_tone
    );
    assert!(!app.adv_hp_linear, "bit 5 clear leaves the linear amp off");
}

/// The linear amp is opt-in: a fresh app has it off, and its row toggles exactly bit 5.
#[test]
fn linear_amp_is_off_by_default_and_owns_bit_5() {
    let mut app = unlocked();
    assert_eq!(app.adv_flags() & (1 << 5), 0, "linear amp on by default");
    app.adv_sel = crate::advanced::ROW_HP_AMP;
    app.advanced_toggle_row();
    assert_eq!(
        app.adv_flags(),
        1 << 5,
        "the row set something other than bit 5"
    );
    app.advanced_toggle_row();
    assert_eq!(app.adv_flags(), 0);
}

/// Crossfeed is opt-in, its row flips the switch with nothing for Sony's chain to re-apply,
/// and with Mono on it leaves the switch alone (there is nothing to blend).
#[test]
fn the_crossfeed_row_flips_its_switch_unless_mono_is_on() {
    let mut app = unlocked();
    assert!(!app.opt(Opt::Crossfeed), "crossfeed ships off");
    app.adv_sel = crate::advanced::ROW_CROSSFEED;
    assert!(app.advanced_toggle_row().is_empty());
    assert!(app.opt(Opt::Crossfeed));
    app.mono = true;
    app.advanced_toggle_row();
    assert!(
        app.opt(Opt::Crossfeed),
        "Mono on: the switch is not touched"
    );
    app.mono = false;
    app.advanced_toggle_row();
    assert!(!app.opt(Opt::Crossfeed));
}

/// The headphone EQ row asks the shell for the next profile; the row shows what the shell says.
#[test]
fn the_headphone_eq_row_asks_the_shell() {
    let mut app = unlocked();
    app.adv_sel = crate::advanced::ROW_HP_EQ;
    assert!(matches!(app.advanced_toggle_row()[..], [Action::NextHpEq]));
    app.set_hp_eq("HD 650");
    assert_eq!(app.hp_eq, "HD 650");
}

/// ReplayGain ships off, and its row cycles Off → Track → Album → Off.
#[test]
fn the_replaygain_row_cycles_off_track_album() {
    let mut app = unlocked();
    assert_eq!(app.replaygain_mode(), 0, "ships off");
    app.adv_sel = crate::advanced::ROW_REPLAYGAIN;
    for want in [1, 2, 0, 1] {
        assert!(app.advanced_toggle_row().is_empty());
        assert_eq!(app.replaygain_mode(), want);
    }
}

/// Brightness cycles 1..5, then 0 (backlight off), then wraps.
///
/// This test used to assert that 0 was UNREACHABLE, because a persisted 0 would blank the
/// panel and hide the Settings screen you need to undo it — across reboots. That hazard is
/// real and has not gone away; what changed is how it is answered. Level 0 is now reachable
/// but TRANSIENT: `brightness_restore` (never 0) is what gets persisted, and any input calls
/// `brightness_wake`. See `backlight_off_is_reachable_and_always_escapable`, which pins those
/// two properties — this one only pins the cycle order.
#[test]
fn brightness_cycles_one_to_five_and_wraps() {
    let mut app = unlocked();
    app.set_brightness(1);
    let mut seen = vec![app.brightness()];
    for _ in 0..5 {
        app.settings_sel = crate::settings::ROW_BRIGHTNESS;
        let acts = app.settings_activate();
        assert_eq!(acts, vec![Action::BrightnessChanged(app.brightness())]);
        seen.push(app.brightness());
    }
    // 1→2→3→4→5→1. The 0 (BACKLIGHT OFF) stop is PARKED — it was removed because zeroing this
    // panel's backlight blanks it entirely, where the point was an unlit but still READABLE
    // screen. Never offer a stop that lies about what it does.
    assert_eq!(seen, vec![1, 2, 3, 4, 5, 1]);
    assert!(
        !seen.contains(&0),
        "the parked backlight-off stop is back: {seen:?}"
    );
}

/// Out-of-range values from a corrupt settings file are clamped, not trusted.
#[test]
fn set_brightness_clamps_out_of_range_values() {
    let mut app = unlocked();
    app.set_brightness(0);
    assert_eq!(app.brightness(), 1);
    app.set_brightness(200);
    assert_eq!(app.brightness(), 5);
}

/// The idle screen-off timer must default to OFF and its cycle must START at OFF. It blanks the
/// panel, so it has to be opt-in: a default-on idle blank would land on every user at once, on
/// hardware where a failed wake looks like a dead device.
#[test]
fn screen_off_timer_defaults_to_30s_and_can_still_reach_off() {
    let app = unlocked();
    assert_eq!(
        app.screen_off_s(),
        30,
        "the backlight is the biggest draw — do not leave it on"
    );
    assert_eq!(
        SCREEN_OFF_PRESETS[0], 0,
        "OFF must still be one of the presets"
    );

    let mut app = unlocked();
    let mut seen = vec![app.screen_off_s()];
    for _ in 0..SCREEN_OFF_PRESETS.len() {
        app.settings_sel = crate::settings::ROW_SCREEN_OFF;
        let acts = app.settings_activate();
        assert_eq!(acts, vec![Action::ScreenOffTimer(app.screen_off_s())]);
        seen.push(app.screen_off_s());
    }
    // Cycles through every preset and comes back to where it started, so OFF — and the default
    // — are both always reachable again.
    assert_eq!(seen.first(), Some(&30));
    assert_eq!(seen.last(), Some(&30));
    for p in SCREEN_OFF_PRESETS {
        assert!(seen.contains(&p), "preset {p} unreachable by cycling");
    }
}

/// A hand-edited or corrupt settings value snaps to a known preset — otherwise the row would
/// display something the cycle can never return to.
#[test]
fn set_screen_off_snaps_to_a_known_preset() {
    let mut app = unlocked();
    app.set_screen_off_s(37);
    assert_eq!(app.screen_off_s(), 0, "unknown value falls back to OFF");
    app.set_screen_off_s(60);
    assert_eq!(app.screen_off_s(), 60);
}

#[test]
fn screen_off_labels_read_as_durations() {
    assert_eq!(screen_off_label(0), "OFF");
    assert_eq!(screen_off_label(15), "15 SEC");
    assert_eq!(screen_off_label(60), "1 MIN");
    assert_eq!(screen_off_label(120), "2 MIN");
    assert_eq!(screen_off_label(90), "1:30");
}

/// Boot to stock reboots the device, so one stray tap must never do it: the row arms first and
/// only acts on a second tap.
#[test]
fn boot_to_stock_needs_two_taps() {
    let mut app = unlocked();
    app.settings_sel = crate::settings::ROW_BOOT_STOCK;
    assert_eq!(app.settings_activate(), vec![], "first tap only arms");
    assert_eq!(
        app.settings_activate(),
        vec![Action::BootToStock],
        "second tap acts"
    );
    // And it disarms after firing, so a third tap arms again rather than re-firing.
    assert_eq!(app.settings_activate(), vec![]);
}

/// An armed confirmation must not survive touching something else — otherwise a tap on this row
/// minutes later, with no prompt on screen, would restart the device.
#[test]
fn boot_to_stock_disarms_when_you_touch_anything_else() {
    let mut app = unlocked();
    app.settings_sel = crate::settings::ROW_BOOT_STOCK;
    assert_eq!(app.settings_activate(), vec![]); // armed
    app.settings_sel = crate::settings::ROW_IGNORE_THE;
    let _ = app.settings_activate(); // a different row cancels it
    app.settings_sel = crate::settings::ROW_BOOT_STOCK;
    assert_eq!(app.settings_activate(), vec![], "must re-arm, not fire");

    // Leaving the screen cancels it too.
    let mut app = unlocked();
    app.settings_sel = crate::settings::ROW_BOOT_STOCK;
    assert_eq!(app.settings_activate(), vec![]); // armed
    app.push(Screen::Library);
    app.pop();
    assert_eq!(
        app.settings_activate(),
        vec![],
        "must re-arm after navigating away"
    );
}

/// Auto power-off is on (30 MIN) out of the box since 2026-10-04 — a paused player left alone
/// must not run flat — and OFF has to stay reachable: a cycle that cannot get there is how
/// someone who does not want it is stuck with it.
#[test]
fn auto_power_off_defaults_to_30_min_and_off_is_reachable() {
    let app = unlocked();
    assert_eq!(app.auto_off_min(), 30, "on by default");
    assert_eq!(AUTO_OFF_PRESETS[0], 0);

    let mut app = unlocked();
    let mut seen = vec![app.auto_off_min()];
    for _ in 0..AUTO_OFF_PRESETS.len() {
        app.settings_sel = crate::settings::ROW_AUTO_OFF;
        // The shell polls the getter from its own timer, so the row emits no action.
        assert_eq!(app.settings_activate(), vec![]);
        seen.push(app.auto_off_min());
    }
    assert_eq!(seen.last(), Some(&30), "the cycle comes back round");
    for p in AUTO_OFF_PRESETS {
        assert!(seen.contains(&p), "preset {p} unreachable by cycling");
    }
}

/// Clear Bass exists only where the firmware can run it: without the shell's report the row
/// is not a tap target and the level reads 0 whatever the settings file held.
#[test]
fn clear_bass_needs_the_firmware_and_cycles_off_to_ten() {
    use crate::advanced as adv;
    let mut app = unlocked();
    app.set_clear_bass_step(3);
    assert_eq!(
        app.clear_bass(),
        0,
        "stored, but not sent to a chain without the six-band"
    );
    assert!(!adv::order(false).contains(&adv::ROW_CLEAR_BASS));

    assert!(app.set_clear_bass_available(true));
    assert_eq!(app.clear_bass(), 6);
    assert_eq!(
        adv::row_at(adv::row_top(adv::ROW_CLEAR_BASS, true, 0) + 1, true, 0),
        Some(adv::ROW_CLEAR_BASS)
    );
    app.adv_sel = adv::ROW_CLEAR_BASS;
    let mut seen = vec![];
    for _ in 0..adv::CLEAR_BASS.len() {
        assert_eq!(app.advanced_toggle_row(), vec![Action::SoundChanged]);
        seen.push(app.clear_bass());
    }
    assert_eq!(seen, vec![8, 10, 0, 2, 4, 6], "wraps through Off");
}

/// Bluetooth auto off is on out of the box (2026-10-04) and the row switches it.
#[test]
fn bluetooth_auto_off_is_on_by_default_and_the_row_toggles_it() {
    let mut app = unlocked();
    assert!(app.bt_idle_off(), "on by default");
    app.settings_sel = crate::settings::ROW_BT_IDLE_OFF;
    assert!(
        app.settings_activate().is_empty(),
        "the shell polls it; no action"
    );
    assert!(!app.bt_idle_off());
    app.settings_activate();
    assert!(app.bt_idle_off());
}

/// A hand-edited or corrupt `auto_off=` snaps to a known preset — otherwise the row would show
/// a duration the cycle can never reach again, exactly like the screen-off timer's rule.
#[test]
fn a_bogus_auto_off_value_snaps_to_a_preset() {
    let mut app = unlocked();
    app.set_auto_off_min(37);
    assert!(AUTO_OFF_PRESETS.contains(&app.auto_off_min()));
    app.set_auto_off_min(30);
    assert_eq!(
        app.auto_off_min(),
        30,
        "a real preset survives the round trip"
    );
}

/// The ABOUT rows used to be hit-tested against hardcoded 14 and 15 while ROW_POWER_OFF was 14,
/// so selecting Power off highlighted Firmware too and Model could never be selected at all.
/// Named constants now, and they have to agree with the section table.
#[test]
fn the_about_rows_are_the_last_two_and_distinct_from_power_off() {
    use crate::settings::{ROWS, ROW_FIRMWARE, ROW_MODEL, ROW_POWER_OFF};
    assert_eq!(ROW_MODEL, ROWS - 1);
    assert_eq!(ROW_FIRMWARE, ROWS - 2);
    assert_ne!(ROW_FIRMWARE, ROW_POWER_OFF);
}

/// The genre filter is a FILTER, not a browse tab: it narrows the list that is already there.
/// Everything that draws or hit-tests the Songs list has to shrink together, or a tap lands on
/// a row the render never drew — the drift this codebase keeps getting bitten by.
#[test]
fn a_genre_filter_shrinks_the_list_and_everything_derived_from_it() {
    use crate::library::{self, Tab};
    let mut a = unlocked();
    let all = library::row_count(Tab::Songs, &a.lib);
    assert!(all > 0);
    assert_eq!(
        library::song_order(&a.lib, 0).len(),
        all,
        "no filter = everything"
    );

    a.lib.filter_genre = Some(1);
    let some = library::row_count(Tab::Songs, &a.lib);
    assert!(
        some > 0 && some < all,
        "the sample data must have more than one genre"
    );
    // row_count, song_order and visible_songs are three doors onto one answer.
    assert_eq!(library::song_order(&a.lib, 0).len(), some);
    assert_eq!(a.lib.visible_songs(), some);
    // Every surviving index really carries the genre, and the indices stay ABSOLUTE — song_at
    // resolves through lib.songs, so a relative index would silently return the wrong track.
    for i in library::song_order(&a.lib, 0) {
        assert_eq!(a.lib.songs[i].genre_id, 1);
    }
    for rank in 0..some {
        assert_eq!(library::song_at(&a.lib, 0, rank).unwrap().genre_id, 1);
    }
    assert!(
        library::song_at(&a.lib, 0, some).is_none(),
        "nothing past the filtered end"
    );

    a.lib.filter_genre = None;
    assert_eq!(
        library::row_count(Tab::Songs, &a.lib),
        all,
        "clearing restores everything"
    );
}

/// Screen-y of picker row `r` at scroll 0 — derived from the same constants the render uses,
/// so inserting a header row moves the test with the screen instead of breaking it.
fn picker_y(r: usize) -> i32 {
    crate::library::GENRE_TOP
        + r as i32 * crate::library::GENRE_RH()
        + crate::library::GENRE_RH() / 2
}

/// The picker's rows and its hit test share their arithmetic, and the "All genres" row means
/// setting and clearing are the same gesture.
#[test]
fn the_genre_picker_maps_rows_to_genres_and_row_zero_clears() {
    use crate::library::{self, GENRE_HEAD_ROWS, GENRE_ROW_ALL};
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = crate::library::Tab::Songs;
    // Tapping the strip opens the picker.
    let y = library::filter_top() + library::FILTER_H / 2;
    assert!(library::filter_hit(a.lib_tab, y));
    a.tap(240, y);
    assert_eq!(a.current(), Screen::GenreFilter);

    // The first row past the two headers is the first genre; picking it filters and leaves.
    let want = a.lib.genres[0].id;
    let y1 = picker_y(GENRE_HEAD_ROWS);
    assert_eq!(library::genre_row_at(&a.lib, y1, 0), Some(GENRE_HEAD_ROWS));
    a.tap(240, y1);
    assert_eq!(a.lib.filter_genre, Some(want));
    assert_eq!(
        a.current(),
        Screen::Library,
        "picking answers the question and leaves"
    );

    // "All genres" clears it.
    a.tap(240, y);
    let y0 = picker_y(GENRE_ROW_ALL);
    assert_eq!(library::genre_row_at(&a.lib, y0, 0), Some(GENRE_ROW_ALL));
    a.tap(240, y0);
    assert_eq!(a.lib.filter_genre, None);
}

/// Hi-Res is the OTHER axis: it ANDs with the genre, it toggles rather than choosing, and it
/// is the one row that leaves the sheet open — because combining it with a genre is the whole
/// reason the two share a screen.
#[test]
fn hi_res_is_a_second_filter_axis_that_ands_with_the_genre() {
    use crate::library::{GENRE_HEAD_ROWS, GENRE_ROW_HIRES};
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = crate::library::Tab::Songs;
    let all = a.lib.songs.len();
    let hires = a.lib.songs.iter().filter(|s| s.is_hires).count();
    assert!(
        hires > 0 && hires < all,
        "the sample library must exercise both sides"
    );

    a.push(Screen::GenreFilter);
    a.tap(240, picker_y(GENRE_ROW_HIRES));
    assert!(a.lib.filter_hires);
    assert_eq!(
        a.current(),
        Screen::GenreFilter,
        "the toggle does not close the sheet"
    );
    assert_eq!(a.lib.visible_songs(), hires);
    assert_eq!(a.lib.filter_name().as_deref(), Some("Hi-Res"));

    // Now add a genre on top: the two AND, and the strip says both.
    let g = a.lib.genres[0].id;
    a.tap(240, picker_y(GENRE_HEAD_ROWS));
    assert_eq!(a.lib.filter_genre, Some(g));
    assert!(a.lib.filter_hires, "choosing a genre does not clear Hi-Res");
    let both = a
        .lib
        .songs
        .iter()
        .filter(|s| s.is_hires && s.genre_id == g)
        .count();
    assert_eq!(a.lib.visible_songs(), both);
    assert!(a.lib.filter_name().unwrap().contains("Hi-Res"));

    // And toggling it back off leaves the genre alone.
    a.push(Screen::GenreFilter);
    a.tap(240, picker_y(GENRE_ROW_HIRES));
    assert!(!a.lib.filter_hires);
    assert_eq!(a.lib.filter_genre, Some(g));
    assert!(a.lib.filtered());
}

/// A filter changes the visible set, so the scroll offset and the A-Z map derived from the old
/// one are stale. Leaving them would scroll past the end of a now-shorter list.
#[test]
fn picking_a_genre_resets_the_scroll_and_the_az_memo() {
    use crate::library;
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = crate::library::Tab::Songs;
    let _ = a.az_present_memo();
    assert!(a.az_memo.is_some());
    a.lib_scroll_px = 400;

    a.push(Screen::GenreFilter);
    a.tap(240, picker_y(library::GENRE_HEAD_ROWS));
    assert_eq!(a.lib_scroll_px, 0);
    assert!(
        a.az_memo.is_none(),
        "the rail is indexed off the visible rows"
    );
}

/// The filter strip and the shuffle band must not share an edge. They did, and the band is the
/// one that starts playing music — so every near-miss aimed at the filter shuffled the library
/// instead ("filter is hard to press under the shuffle by album", 2026-08-16).
#[test]
fn the_filter_strip_is_a_real_target_clear_of_the_shuffle_band() {
    use crate::library::{self, Tab};
    let (_, by, _, bh) = library::library_shuffle_band();
    let band_bottom = by + bh;
    assert!(
        library::filter_top() > band_bottom,
        "the strip starts inside the band"
    );
    assert!(
        library::filter_top() - band_bottom >= 4,
        "less than 4px of dead space between shuffle and filter"
    );
    // Big enough to hit: 44 is the smallest row this UI uses anywhere.
    const {
        assert!(
            library::FILTER_H >= 44,
            "the strip is smaller than a usable row"
        )
    };
    // The band must not answer for a tap on the strip, and vice versa.
    for y in library::filter_top()..library::filter_top() + library::FILTER_H {
        assert!(
            library::filter_hit(Tab::Songs, y),
            "y={y} is not on the strip"
        );
        assert!(y >= band_bottom, "y={y} is still inside the shuffle band");
    }
    assert!(
        !library::filter_hit(Tab::Songs, band_bottom),
        "the band's last row hits the strip"
    );
    // And the list still starts below the strip, with the gap accounted for.
    assert!(
        library::list_top(Tab::Songs) >= library::filter_top() + library::FILTER_H,
        "the list draws underneath the strip"
    );
}

/// The filter strip only exists on the two tabs a genre can scope, and `list_top` moves with it
/// so the list never draws underneath it.
#[test]
fn only_track_tabs_carry_the_filter_strip() {
    use crate::library::{self, Tab};
    for tab in [Tab::Songs, Tab::Albums] {
        assert!(library::has_filter(tab));
        assert!(library::list_top(tab) - library::filter_top() >= library::FILTER_H);
        assert!(library::filter_hit(tab, library::filter_top() + 1));
    }
    for tab in [Tab::Artists, Tab::Playlists] {
        assert!(!library::has_filter(tab));
        assert!(!library::filter_hit(tab, library::filter_top() + 1));
    }
}

/// Balance cycles like the VPT and DC-Phase pills beside it, and wraps back to CENTRE — the
/// The A/B compare segments must be a real touch target and must agree with what is drawn.
/// Reported hard to press on 2026-08-17: they were 30x26 with a hit band 4px shorter than the
/// boxes and written out separately from the render, which is this codebase's recurring defect.
#[test]
fn the_ab_segments_are_a_real_target_that_matches_what_is_drawn() {
    use crate::sound;
    // Every pixel of each drawn box resolves to that box.
    for seg in 0..2usize {
        let (x, y, w, h) = sound::ab_rect(seg);
        assert!(
            w >= 44 && h >= 44,
            "segment {seg} is {w}x{h}, under a usable touch target"
        );
        for px in [x, x + w / 2, x + w - 1] {
            for py in [y, y + h / 2, y + h - 1] {
                assert_eq!(
                    sound::hit_ab(px, py),
                    Some(seg),
                    "({px},{py}) missed segment {seg}"
                );
            }
        }
    }
    // The control must live ENTIRELY between the status strip and the first list row. The
    // status strip is hit-tested before any screen, so a pixel above STATUS_H is unreachable
    // however it is drawn; a pixel at or past HEADER_BOTTOM belongs to the list.
    let (x, y, _, h) = sound::ab_rect(0);
    let (_, ay, _, ah) = sound::ab_rect(1);
    assert!(
        y >= crate::chrome::STATUS_H,
        "A/B is drawn inside the status strip"
    );
    assert!(
        ay + ah <= crate::chrome::HEADER_BOTTOM,
        "A/B is drawn over the first row"
    );
    for yy in 0..crate::chrome::STATUS_H {
        assert!(
            sound::hit_ab(x + 4, yy).is_none(),
            "the A/B band reaches the status strip"
        );
    }
    for yy in crate::chrome::HEADER_BOTTOM..crate::chrome::HEADER_BOTTOM + 20 {
        assert!(
            sound::hit_ab(x + 4, yy).is_none(),
            "the A/B band reaches into the first row"
        );
    }
    // Slack downward within the header only, and never across the title.
    assert!(
        sound::hit_ab(x + 4, y + h + 1).is_some(),
        "a tap just below the box is lost"
    );
    assert!(
        sound::hit_ab(200, y + 4).is_none(),
        "the band extends across the title"
    );

    // Tapping a segment SELECTS that SETUP, and a repeat is a no-op. A and B are two complete
    // sound configurations — the control is a comparison, not an on/off.
    let mut a = unlocked();
    a.stack = vec![Screen::Sound];
    let (bx, by, bw, bh) = sound::ab_rect(1);
    assert_eq!(a.setup_idx(), 0, "A should be live to begin with");
    let acts = a.tap(bx + bw / 2, by + bh / 2);
    assert_eq!(a.setup_idx(), 1, "tapping B did not select setup B");
    // Exactly SoundChanged: the shell receives only the first shell-visible action, so a second
    // action here would be silently dropped (the 2026-09-17 A/B defect).
    assert_eq!(
        acts,
        vec![Action::SoundChanged],
        "B must reach the shell as one SoundChanged"
    );
    assert!(
        a.tap(bx + bw / 2, by + bh / 2).is_empty(),
        "tapping B twice acted twice"
    );
    let (ax, ay2, aw, ah2) = sound::ab_rect(0);
    let acts = a.tap(ax + aw / 2, ay2 + ah2 / 2);
    assert_eq!(a.setup_idx(), 0, "tapping A did not come back");
    assert!(
        acts.iter().any(|x| matches!(x, Action::SoundChanged)),
        "A did not re-apply"
    );
}

/// The balance slider: render geometry and hit test must agree, a drag must track the finger
/// across the whole track, and the ends must be reachable. `balance_x` and `balance_at` are
/// each other's inverse — if they drift, the knob sits somewhere your finger isn't, which is
/// the render-vs-hit defect this codebase keeps producing.
#[test]
fn the_balance_slider_tracks_the_finger_end_to_end() {
    use crate::sound::{self, balance_label, BALANCE_CENTRE, BALANCE_MAX};
    let mut a = unlocked();
    assert_eq!(a.balance(), BALANCE_CENTRE, "centre is the default");

    // Round trip: every position maps to an x that maps back to itself — EXCEPT inside the
    // snap band, where landing on centre is the whole point.
    for pos in 0..=BALANCE_MAX {
        let got = sound::balance_at(sound::balance_x(pos));
        if pos.abs_diff(BALANCE_CENTRE) <= sound::BAL_SNAP {
            assert_eq!(got, BALANCE_CENTRE, "position {pos} should snap to centre");
        } else {
            assert_eq!(got, pos, "position {pos} does not survive a round trip");
        }
    }
    // The snap band is narrow enough to still reach the positions just outside it, so it buys
    // a reachable null without stealing usable travel.
    assert_eq!(
        sound::balance_at(sound::balance_x(BALANCE_CENTRE + sound::BAL_SNAP + 1)),
        BALANCE_CENTRE + sound::BAL_SNAP + 1
    );
    // Both stops are reachable, and past the ends pins rather than wrapping.
    assert_eq!(sound::balance_at(sound::BAL_X0 - 40), 0);
    assert_eq!(sound::balance_at(sound::BAL_X1 + 40), BALANCE_MAX);

    // A drag: begin inside the grab band, move, release. The value follows the finger and the
    // moves apply live (that is the point of a balance control).
    // The Balance row is below the fold since the ENHANCE / SPACE / LEVEL grouping: scroll to
    // the end, as a finger would, and take every coordinate at that scroll.
    a.stack = vec![Screen::Sound];
    let sc = sound::max_scroll();
    a.sound_scroll_px = sc;
    let ty = sound::bal_track_y(sc);
    assert!(
        a.scrub_begin(sound::balance_x(BALANCE_CENTRE), ty),
        "the grab band did not take"
    );
    let acts = a.scrub_move(sound::BAL_X0, ty);
    assert_eq!(a.balance(), 0, "the knob did not follow the finger");
    assert!(
        acts.iter().any(|x| matches!(x, Action::BalanceChanged)),
        "no live apply"
    );
    // Moving within the same step emits nothing — otherwise a slow drag floods the shell.
    assert!(
        a.scrub_move(sound::BAL_X0, ty).is_empty(),
        "a no-op move still emitted"
    );
    assert!(
        a.scrub_end()
            .iter()
            .any(|x| matches!(x, Action::SoundChanged)),
        "not persisted"
    );

    // The grab band is a real touch target, not the 3px track.
    assert!(sound::balance_grab(ty - 20, sc) && sound::balance_grab(ty + 20, sc));
    // ...and it lives inside the Balance row, so it can't steal ClearAudio+'s taps.
    assert_eq!(sound::row_at(ty, sc), Some(sound::ROW_BALANCE));
    assert_eq!(
        sound::row_at(sound::balance_top(sc) - 1, sc),
        Some(sound::ROW_NORM)
    );

    // Select recentres — the one useful thing a button can do to a continuous control.
    a.sound_sel = sound::ROW_BALANCE;
    a.sound_toggle_row();
    assert_eq!(a.balance(), BALANCE_CENTRE);

    // …and so does the CENTRE button, which is the touch path. It must sit INSIDE the Balance
    // row (so it reads as belonging to the slider) but clear of the grab band (so resetting
    // cannot be mistaken for the start of a drag).
    let (rx, ry, rw, rh) = sound::balance_reset_rect(sc);
    assert_eq!(sound::row_at(ry, sc), Some(sound::ROW_BALANCE));
    assert_eq!(sound::row_at(ry + rh - 1, sc), Some(sound::ROW_BALANCE));
    for yy in ry..ry + rh {
        assert!(
            !sound::balance_grab(yy, sc),
            "the reset button overlaps the slider grab band"
        );
    }
    a.set_balance(0);
    let acts = a.tap(rx + rw / 2, ry + rh / 2);
    assert_eq!(
        a.balance(),
        BALANCE_CENTRE,
        "the CENTRE button did not recentre"
    );
    assert!(
        acts.iter().any(|x| matches!(x, Action::SoundChanged)),
        "reset was not persisted"
    );
    // Pressing it again is a no-op, not a redundant mixer write.
    assert!(a.tap(rx + rw / 2, ry + rh / 2).is_empty());
    // Left/Right nudge instead of toggling.
    a.press(Button::Left);
    assert!(
        a.balance() < BALANCE_CENTRE,
        "Left did not nudge the slider"
    );

    // Labels say which way, and centre says neither.
    assert_eq!(balance_label(BALANCE_CENTRE), "Centre");
    assert_eq!(balance_label(0), "L 50");
    assert_eq!(balance_label(BALANCE_MAX), "R 50");

    // A hand-edited value is clamped rather than parking the knob off the end of the track.
    a.set_balance(9999);
    assert_eq!(a.balance(), BALANCE_MAX);

    // AND IT IS NOT THE PROGRESS RAIL. The FFI decides what a drag MEANS from this one bit: the
    // rail reports a seek target, everything else reports a UI action. It used to ask the
    // inverse ("is it the UI-scale slider?") and assume the rail otherwise — so the balance
    // drag ran the rail's code path, never moved the balance, and SEEKED THE TRACK on release.
    // Any future slider must answer false here or it will do the same.
    let mut a = unlocked();
    a.stack = vec![Screen::Sound];
    a.sound_scroll_px = sc;
    assert!(a.scrub_begin(sound::balance_x(20), ty));
    assert!(
        !a.scrub_is_rail(),
        "the balance drag claims to be the seek rail"
    );
    a.scrub_end();
    // The text-size slider, found by walking the Display rows rather than by a magic y.
    let mut a = unlocked();
    a.stack = vec![Screen::Display];
    for y in 0..crate::canvas::H as i32 {
        if crate::display::row_at(y) == Some(crate::display::ROW_SIZE) && a.scrub_begin(240, y) {
            assert!(
                !a.scrub_is_rail(),
                "the UI-scale drag claims to be the seek rail"
            );
            a.scrub_end();
            break;
        }
    }
    let mut a = unlocked();
    assert!(a.scrub_begin(240, crate::now_playing::RAIL_GRAB_TOP + 1));
    assert!(a.scrub_is_rail(), "the progress rail must BE the rail");
}

/// Bit 6 of the sound flags used to carry "high gain output". The row was cut (the codec
/// accepts the write and ignores it — see sound::ROWS), so the bit must now stay clear no
/// matter what the user does on the Sound screen: a stale set bit would be applied by an older
/// shell as a real mixer write.
#[test]
fn sound_flags_never_set_the_retired_high_gain_bit() {
    let mut a = unlocked();
    for row in 0..crate::sound::ROWS {
        a.sound_sel = row;
        a.sound_toggle_row();
        assert_eq!(
            a.sound_flags() & (1 << 6),
            0,
            "row {row} set the retired bit 6"
        );
    }
    // And a settings file written by the OLD build, with bit 6 set, must not resurrect it.
    let mut b = unlocked();
    b.set_sound_flags(0xFF);
    assert_eq!(b.sound_flags() & (1 << 6), 0, "bit 6 survived a restore");
}

/// EVERY Settings row must be reachable by a finger at some scroll position. This caught a real
/// bug: the content is 919px tall on an 800px panel, so before this screen scrolled the ABOUT
/// rows sat past the bottom edge — "Model" was entirely unreachable and "Firmware" was clipped,
/// and adding "Boot to stock" pushed both fully off. row_at also hardcodes the section
/// boundaries, so it silently drifts from render whenever a row is added.
#[test]
fn every_settings_row_is_reachable_at_some_scroll_position() {
    use crate::settings::{max_scroll_px, row_at, GROUPS, ROWS};
    let mut hit: Vec<usize> = Vec::new();
    for page in std::iter::once(None).chain((0..GROUPS.len()).map(Some)) {
        for scroll in 0..=max_scroll_px(page, true) {
            for y in 0..crate::canvas::H as i32 {
                if let Some(r) = row_at(y, scroll, page, true) {
                    if r < ROWS && !hit.contains(&r) {
                        hit.push(r);
                    }
                }
            }
        }
    }
    hit.sort();
    assert_eq!(
        hit,
        (0..ROWS)
            .filter(|r| crate::settings::shown(*r, true))
            .collect::<Vec<_>>(),
        "some rows can never be tapped"
    );
}

/// Left-swipe queues NEXT, right-swipe queues LATER, and both report the change so the shell
/// can schedule a flush. The two gestures are mirror images, so a mix-up would be invisible
/// without the toast naming which one happened.
#[test]
fn swiping_a_row_queues_next_or_later() {
    let mut a = with_playing(unlocked(), 3);
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    let y = crate::library::list_top(Tab::Songs) + 10;

    let acts = a.swipe(1, 240, y); // right = later
    assert_eq!(acts, vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), 3);
    assert!(
        a.toast.starts_with("Added to Up Next"),
        "toast was {:?}",
        a.toast
    );
    let later = upq(&a)[2].title.clone();

    let acts = a.swipe(-1, 240, y + crate::library::row_h(Tab::Songs)); // left = next, a different row
    assert_eq!(acts, vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), 4);
    assert!(
        a.toast.starts_with("Playing next"),
        "toast was {:?}",
        a.toast
    );
    // "Next" lands straight after the playing track, in FRONT of everything already coming;
    // "later" lands at the very end.
    assert_ne!(upq(&a)[0].title, "A1", "Play Next did not go to the front");
    assert_eq!(
        upq(&a)[3].title,
        later,
        "Add to queue did not stay at the end"
    );
}

/// Reordering and clearing both report a change, and neither may panic on a stale index — the
/// list can be edited while a gesture that started earlier is still in flight.
#[test]
fn queue_reorder_and_clear_are_safe() {
    let mut a = queued(3);
    let first = upq(&a)[0].title.clone();
    assert_eq!(a.movable_move(0, 2), vec![Action::QueueChanged]);
    assert_eq!(
        upq(&a)[2].title,
        first,
        "the moved row did not land at its target"
    );
    // Out-of-range and no-op moves change nothing and emit nothing.
    assert!(a.movable_move(9, 0).is_empty());
    assert!(a.movable_move(0, 9).is_empty());
    assert!(a.movable_move(1, 1).is_empty());
    assert_eq!(upq(&a).len(), 3);
    assert_eq!(a.queue_clear(), vec![Action::QueueChanged]);
    assert!(upq(&a).is_empty());
    assert_eq!(a.context().len(), 1, "the playing track stays");
    assert!(
        a.queue_clear().is_empty(),
        "clearing an empty list is not a change"
    );
}

/// Build an app sitting on Up Next with a track playing ("NOW") and `n` distinctly-titled
/// tracks after it. Built directly rather than by swiping rows in, so the length isn't capped
/// by the sample library (the edge-scroll test needs more tracks than fit on the panel).
fn queued(n: usize) -> App {
    let mut a = unlocked();
    let src = a
        .lib
        .songs
        .first()
        .cloned()
        .expect("sample library has songs");
    let mut rows = Vec::new();
    for i in 0..=n {
        let mut s = src.clone();
        s.title = if i == 0 {
            "NOW".into()
        } else {
            format!("Q{}", i - 1)
        };
        s.object_id = 9000 + i as i64;
        rows.push(s);
    }
    a.set_play_context(rows, 0);
    a.push(Screen::UpNext);
    a
}

/// Glue comes back after a reboot: it is written with the queue.
#[test]
fn glue_is_saved_with_the_queue() {
    let mut a = App::unlocked();
    assert!(!a.playback_encode().contains("glue="));
    a.set_glue_pairs([(5, 4), (2, 1)].into_iter().collect());
    assert!(a.playback_encode().contains("glue=2,1,5,4\n"));
}

/// Plan 1c on Up Next's shuffle: a glued run stays whole and in order, and the playing song's
/// run is not cut off from it.
#[test]
fn up_next_shuffle_keeps_glued_runs_together() {
    for seed in 0..200u64 {
        let mut a = queued(8);
        a.rng = crate::shuffle::Rng::new(seed + 1);
        // NOW(9000) -> Q0(9001) glued; Q3(9004) -> Q4(9005) -> Q5(9006) glued.
        a.set_glue_pairs(
            [(9000, 9001), (9004, 9005), (9005, 9006)]
                .into_iter()
                .collect(),
        );
        assert_eq!(a.queue_shuffle(), vec![Action::QueueChanged]);
        let ids: Vec<i64> = a.context.iter().map(|t| t.object_id).collect();
        assert_eq!(&ids[..2], &[9000, 9001], "seed {seed}: {ids:?}");
        let at = ids.iter().position(|&i| i == 9004).unwrap();
        assert_eq!(
            &ids[at..at + 3],
            &[9004, 9005, 9006],
            "seed {seed}: {ids:?}"
        );
        assert_eq!(ids.len(), 9);
    }
}

/// Everything still to come, in play order.
fn upq(a: &App) -> &[SongRow] {
    a.context.get(a.context_idx + 1..).unwrap_or(&[])
}

/// `a` with `n` rows "A0".."A{n-1}" playing from the first.
fn with_playing(mut a: App, n: usize) -> App {
    let rows = (0..n)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 7000 + i as i64,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 0);
    a
}
/// Screen-y of the centre of upcoming row `i` (0 = the next track). Reads the LAYOUT rather
/// than assuming where the list starts: history, the playing track and a section heading can
/// all sit above it.
fn qrow_y(a: &App, i: usize) -> i32 {
    let top = a
        .up_next_layout()
        .top_of(crate::up_next::Slot::Upcoming(a.context_idx + 1 + i))
        .unwrap();
    crate::chrome::HEADER_BOTTOM + top + crate::up_next::RH() / 2 - a.queue_scroll_px
}

/// The whole point: drag a row by its handle and it lands where you dropped it.
#[test]
fn dragging_a_queue_row_by_its_handle_reorders_it() {
    let mut a = queued(4);
    let titles: Vec<String> = upq(&a).iter().map(|s| s.title.clone()).collect();
    let grab = crate::up_next::GRIP_X0 + 20;
    assert!(
        a.reorder_begin(grab, qrow_y(&a, 0)),
        "the handle must pick the row up"
    );
    assert_eq!(a.reorder_state().map(|d| (d.from, d.to)), Some((0, 0)));
    // Two rows down. `to` follows the row's CENTRE, so this is unambiguous.
    a.reorder_track(2 * crate::up_next::RH());
    assert_eq!(a.reorder_state().map(|d| d.to), Some(2));
    assert_eq!(a.reorder_release(), vec![Action::QueueChanged]);
    assert!(a.reorder_state().is_none(), "the drag must end at release");
    assert_eq!(
        upq(&a)[2].title,
        titles[0],
        "the dragged row did not land at its target"
    );
    assert_eq!(
        upq(&a)[0].title,
        titles[1],
        "the rows below did not close up"
    );
    assert_eq!(
        a.context()[a.context_idx()].title,
        "NOW",
        "the playing track never moves"
    );
}

/// A drag that ends where it started is not a change — it must not spend a queue flush (which
/// costs a SetTrackSequence at the next track boundary) on nothing.
#[test]
fn a_reorder_that_moves_nothing_reports_nothing() {
    let mut a = queued(3);
    assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 1)));
    a.reorder_track(4); // less than half a row
    assert_eq!(a.reorder_state().map(|d| d.to), Some(1));
    assert!(a.reorder_release().is_empty());
}

/// Start-point ownership, the same rule the scrub rail uses. A drag that begins on the row body
/// scrolls the list; only one that begins on the handle reorders. Without this, every attempt
/// to scroll a long queue would pick a row up instead.
#[test]
fn only_the_grab_handle_starts_a_reorder() {
    let mut a = queued(4);
    assert!(
        !a.reorder_begin(40, qrow_y(&a, 0)),
        "the row body must still scroll"
    );
    assert!(!a.reorder_begin(crate::up_next::GRIP_X0 - 1, qrow_y(&a, 0)));
    assert!(
        !a.reorder_begin(crate::up_next::GRIP_X0 + 20, 20),
        "the header is not a row"
    );
    assert!(a.reorder_state().is_none());
    assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 0)));
}

/// Nothing to reorder with nothing playing, and nothing to reorder on any other screen.
#[test]
fn reorder_only_applies_to_the_user_queue() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    assert!(
        !a.reorder_begin(crate::up_next::GRIP_X0 + 20, 200),
        "empty list"
    );
    let mut a = queued(3);
    a.go(Screen::Library);
    assert!(
        !a.reorder_begin(crate::up_next::GRIP_X0 + 20, 200),
        "wrong screen"
    );
}

/// A tap on the handle must NOT play the track. A reorder that ends too short to classify as a
/// drag arrives here as a tap, and "I nudged it and it started playing" is the worst outcome.
#[test]
fn tapping_the_grab_handle_does_not_play() {
    let mut a = queued(3);
    assert!(a
        .tap(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 1))
        .is_empty());
    assert_eq!(
        a.current(),
        Screen::UpNext,
        "and it must not navigate either"
    );
    assert!(!a.modal_open(), "nor may it raise the replace prompt");
    // The row body still plays — the LIST from that row, not the tapped track's album.
    assert_eq!(a.tap(60, qrow_y(&a, 1)), vec![Action::PlayContextAt(2)]);
}

/// Holding the row at the bottom edge scrolls the list under it, so a track can be moved
/// further than one screenful. Driven from the tick, because a finger held still delivers no
/// touch events at all — which is precisely when this has to keep working.
#[test]
fn holding_a_dragged_row_at_the_edge_scrolls_the_queue() {
    let mut a = queued(40);
    assert!(
        a.up_next_layout().max_scroll_px() > 0,
        "queue must overflow"
    );
    assert!(a.reorder_begin(crate::up_next::GRIP_X0 + 20, qrow_y(&a, 0)));
    a.reorder_track(700); // park it against the bottom edge
    let before = a.queue_scroll_px;
    for _ in 0..10 {
        a.tick_dt(16);
    }
    assert!(
        a.queue_scroll_px > before,
        "the list did not scroll under the held row"
    );
    assert!(
        a.reorder_state().map(|d| d.to).unwrap() > 0,
        "the landing slot must follow"
    );
}

/// The EQ's raw band units are HALF-decibels, measured on device (`cinder-probe --eq`): the
/// ladder -20..+20 came back unclamped with dB -10.0..+10.0. So the label has to halve the raw
/// value — printing it straight claimed twice the boost the DSP applies — and the range has to
/// be the full +/-20, not the +/-6 that reached less than a third of it.
#[test]
fn eq_band_labels_are_real_decibels_over_sonys_full_range() {
    assert_eq!(crate::eq::BAND_MAX, 20);
    assert_eq!(crate::eq::band_db(crate::eq::BAND_MAX), 10.0);
    assert_eq!(crate::eq::band_db(-crate::eq::BAND_MAX), -10.0);
    assert_eq!(crate::eq::band_db(0), 0.0);
    assert_eq!(
        crate::eq::band_db(crate::eq::BAND_STEP),
        1.0,
        "one tap is one dB"
    );
    // Every shipped preset must sit inside the measured range.
    for (name, bands) in data::EQ_PRESETS {
        for b in bands {
            assert!(
                b.abs() <= crate::eq::BAND_MAX,
                "{name} band {b} is out of range"
            );
        }
    }
}

/// The bar is grabbable and the CONTENT follows the THUMB: dragging the thumb its full travel
/// must cover the whole list, however long it is. A 1:1 finger-to-content mapping would need a
/// 3400-row drag on the real library, which is the bug this replaces.
#[test]
fn dragging_the_scrollbar_scrolls_by_thumb_travel() {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    let max = a.lib_max_scroll();
    assert!(max > 0, "the sample library must overflow the panel");
    let top = library::list_top(Tab::Songs);
    let x = crate::canvas::W as i32 - 4;
    assert!(
        a.sbar_begin(x, top + 20),
        "the right-edge strip must take a vertical drag"
    );
    assert!(a.sbar_active());

    let bottom = library::list_bottom();
    let span = library::sbar_span(top, bottom, max + (bottom - top));
    assert!(span > 0);
    a.sbar_track(span / 2);
    let half = a.lib_scroll_px;
    assert!(
        (half - max / 2).abs() <= 2,
        "half the thumb travel = half the list: {half} vs {}",
        max / 2
    );
    a.sbar_track(span);
    assert_eq!(a.lib_scroll_px, max, "full travel reaches the end");
    a.sbar_track(span * 4);
    assert_eq!(a.lib_scroll_px, max, "and clamps there");
    a.sbar_track(-span * 4);
    assert_eq!(a.lib_scroll_px, 0, "dragging back up returns to the top");
    a.sbar_release();
    assert!(!a.sbar_active());
}

/// EVERY scrollable screen's thumb travel must cover ITS OWN list, not the library's window.
///
/// `sbar_span` and `sbar_begin` both assumed `library::list_bottom()`, and the two pickers stop
/// 24 px higher than that to make room for their footer note. So on those screens the thumb was
/// scaled against a window shorter than the one the rows are drawn in: a drag to the very
/// bottom of the strip stopped short of the end of the list, and the last rows were reachable
/// only by dragging the list itself. Nothing said so, because neither picker drew a scrollbar
/// at all — the drag target was invisible.
#[test]
fn the_scrollbar_covers_the_whole_list_on_every_screen_that_has_one() {
    let screens = [
        Screen::Library,
        Screen::UpNext,
        Screen::GenreFilter,
        Screen::Folders,
        Screen::SensMe,
        Screen::Books,
        Screen::TrackInfo,
        Screen::Lyrics,
        Screen::PlaylistPick,
        Screen::TrackPick,
        Screen::PlaylistEdit,
        Screen::PlaylistCover,
    ];
    let x = crate::canvas::W as i32 - 4;
    for screen in screens {
        let mut a = unlocked();
        a.push_for_test(screen);
        // The sample library has to make this screen overflow, or there is nothing to test.
        let Some((max, top, bottom)) = a.sbar_metrics() else {
            panic!("{screen:?} claims no scrollbar");
        };
        if max <= 0 {
            continue; // nothing to scroll on the sample data — not this test's business
        }
        assert!(
            a.sbar_begin(x, top + 10),
            "{screen:?}: the strip must take a drag"
        );
        let span = library::sbar_span(top, bottom, max + (bottom - top));
        assert!(span > 0, "{screen:?}: the thumb has no travel");
        // Full thumb travel reaches the END of the list. With the wrong bottom this lands short.
        a.sbar_track(span);
        let (m2, _, _) = a.sbar_metrics().expect("still scrollable");
        assert_eq!(
            a.sbar_scroll(),
            m2,
            "{screen:?}: dragging the thumb to the bottom did not reach the end of the list",
        );
        a.sbar_track(-span * 4);
        assert_eq!(
            a.sbar_scroll(),
            0,
            "{screen:?}: dragging back up returns to the top"
        );
        a.sbar_release();

        // …and the OLD arithmetic — the library's bottom for every screen — is shown to land
        // short wherever the two differ, so this test cannot pass by accident on a screen that
        // happens to end where the library does.
        if bottom != library::list_bottom() {
            let lb = library::list_bottom();
            let wrong = library::sbar_span(top, lb, max + (lb - top));
            assert!(
                wrong < span,
                "{screen:?}: the wrong bottom must give less travel, not more",
            );
            let mut b = unlocked();
            b.push_for_test(screen);
            assert!(b.sbar_begin(x, top + 10));
            b.sbar_track(wrong);
            assert!(
                b.sbar_scroll() < max,
                "{screen:?}: the old span reached the end anyway — this screen proves nothing",
            );
        }
    }
}

/// The strip is shared with the A–Z rail and split by GESTURE. A TAP there must still jump to
/// the letter — if the drag swallowed the strip, the rail would become undismissable dead space.
#[test]
fn the_scrollbar_strip_still_taps_through_to_the_az_rail() {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    a.lib_sort = 0; // TITLE — the rail is shown
    let x = crate::canvas::W as i32 - 4;
    // Find a letter the library actually has, and tap it.
    let top = library::list_top(Tab::Songs);
    let hit = (top..library::list_bottom()).find(|&y| {
        library::az_letter_at(y, Tab::Songs)
            .and_then(|l| library::az_scroll_for(Tab::Songs, &a.lib, l, 0, 0, None))
            .is_some_and(|px| px > 0)
    });
    let y = hit.expect("some letter jumps somewhere");
    a.tap(x, y);
    assert!(
        a.lib_scroll_px > 0,
        "a tap on the strip must still be an A-Z jump"
    );
    assert!(!a.sbar_active(), "and must not leave a drag armed");
}

/// Nothing to scroll = no grab, so the contact stays available to whatever is underneath
/// instead of being silently eaten by an invisible bar.
#[test]
fn the_scrollbar_declines_when_there_is_nothing_to_scroll() {
    let mut a = unlocked();
    a.push(Screen::UpNext); // nothing playing -> no px scroll on this screen
    assert!(!a.sbar_begin(crate::canvas::W as i32 - 4, 300));
    assert!(!a.sbar_active());
}

/// The queue's grab handle and the scrollbar cannot both own the same pixels.
#[test]
fn the_queue_grip_stops_short_of_the_scrollbar_strip() {
    assert!(crate::up_next::queue_grip_hit(crate::up_next::GRIP_X0));
    assert!(crate::up_next::queue_grip_hit(crate::up_next::GRIP_X1 - 1));
    assert!(!crate::up_next::queue_grip_hit(crate::up_next::GRIP_X1));
    assert!(
        library::sbar_hit_x(crate::up_next::GRIP_X1),
        "the strips must abut, not overlap"
    );
    assert!(!library::sbar_hit_x(crate::up_next::GRIP_X1 - 1));
}

/// Swiping an upcoming row removes it — either direction, because "queue this" makes no sense
/// for a track that is already in the list.
#[test]
fn swiping_a_queue_row_removes_it() {
    for dir in [-1, 1] {
        let mut a = queued(4);
        let y = qrow_y(&a, 1);
        assert!(a.swipe_track(dir * 80, y), "the row must take the gesture");
        assert_eq!(a.swipe(dir, 240, y), vec![Action::QueueChanged]);
        assert_eq!(upq(&a).len(), 3);
        assert!(
            upq(&a).iter().all(|s| s.title != "Q1"),
            "wrong row removed (dir {dir})"
        );
        assert_eq!(
            a.context()[a.context_idx()].title,
            "NOW",
            "the playing row stays"
        );
    }
}

/// The playing row does not swipe away — it is playing.
#[test]
fn the_playing_row_cannot_be_swiped_out() {
    let mut a = queued(3);
    let top = a.up_next_layout().current_top.unwrap();
    let y = crate::chrome::HEADER_BOTTOM + top + crate::up_next::RH() / 2 - a.queue_scroll_px;
    assert!(!a.swipe_track(80, y));
    assert!(a.swipe(1, 240, y).is_empty());
    assert_eq!(upq(&a).len(), 3);
}

/// Emptying Up Next cannot be undone, so it is an explicit labelled chip rather than a
/// gesture, it ASKS before it acts, and it must not also play the row it sits over.
#[test]
fn the_clear_chip_asks_then_empties_the_queue() {
    use crate::confirm::{hit, Ask, Hit};
    let mut a = queued(5);
    let (cx, cy, cw, ch) = crate::up_next::CLEAR_CHIP;
    let (tx, ty) = (cx + cw / 2, cy + ch / 2);

    // The tap raises the card and changes nothing yet.
    assert_eq!(a.tap(tx, ty), vec![]);
    assert!(a.modal_open(), "CLEAR must ask first");
    assert_eq!(
        upq(&a).len(),
        5,
        "nothing may be destroyed before the answer"
    );

    // Backing out leaves the list alone.
    let cancel = (0..crate::H as i32)
        .find(|y| hit(Ask::ClearQueue, 240, *y) == Hit::Cancel)
        .expect("the card can be dismissed");
    a.tap(240, cancel);
    assert!(!a.modal_open());
    assert_eq!(upq(&a).len(), 5, "Cancel must keep the list");

    // Confirming does the work — everything after the playing track goes; it stays.
    assert_eq!(a.tap(tx, ty), vec![]);
    let ok = (0..crate::H as i32)
        .find(|y| hit(Ask::ClearQueue, 240, *y) == Hit::Confirm)
        .expect("the card has a confirming button");
    assert_eq!(a.tap(240, ok), vec![Action::QueueChanged]);
    assert!(upq(&a).is_empty());
    assert_eq!(a.context()[a.context_idx()].title, "NOW");
    assert_eq!(a.current(), Screen::UpNext);

    // And with nothing left to come the chip is inert — it does not even ask.
    assert!(a.tap(tx, ty).is_empty());
    assert!(!a.modal_open());
}

/// Playing an album REPLACES the list, and does not arm the replace prompt — nothing in it was
/// added by hand.
#[test]
fn playback_sets_the_context_not_the_queue() {
    let mut a = unlocked();
    let rows: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("S{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows.clone(), 1);
    assert_eq!(a.context().len(), 3);
    assert_eq!(a.context_idx(), 1, "the track that started leads the list");

    // A second sequence REPLACES the first: it is what is playing, not an accumulation.
    a.set_play_context(rows.clone(), 0);
    assert_eq!(a.context().len(), 3);
    assert_eq!(a.context_idx(), 0);

    // And it does not arm the replace prompt — nothing here was hand-picked.
    assert_eq!(a.start_play(99), vec![Action::PlayIndex(99)]);
    assert!(!a.modal_open());

    // A start index past the end cannot point outside the list.
    a.set_play_context(rows, 99);
    assert_eq!(a.context_idx(), 2);
}

/// A reboot used to throw the whole sequence away. Encode → decode has to bring back the list,
/// where we were in it, and the un-shuffle order.
#[test]
fn playback_state_survives_an_encode_decode_round_trip() {
    let mut a = unlocked();
    let rows: Vec<SongRow> = (0..6)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 100 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows.clone(), 2);
    a.enqueue_at(
        SongRow {
            title: "P0".into(),
            object_id: 900,
            ..Default::default()
        },
        0,
        QueueAt::Later,
    );
    a.enqueue_at(
        SongRow {
            title: "P1".into(),
            object_id: 901,
            ..Default::default()
        },
        0,
        QueueAt::Later,
    );

    let body = a.playback_encode();
    assert!(
        body.contains("ctx=100,101,102,103,104,105,900,901"),
        "{body}"
    );
    assert!(body.contains("idx=2"), "{body}");
    assert!(
        !body.contains("q="),
        "the queue is part of the list now: {body}"
    );
    assert!(
        !body.contains("pre="),
        "nothing has been shuffled yet: {body}"
    );

    // The shell resolves the ids back to rows against the library; here, straight back.
    let mut b = unlocked();
    b.playback_restore(a.context().to_vec(), 2, vec![], None, None);
    assert_eq!(
        b.playback_encode(),
        body,
        "a round trip is not allowed to change anything"
    );
    assert_eq!(b.context_idx(), 2);
    assert!(b.has_playback_state());
}

/// A file written BEFORE the merge keeps the picks apart (`q=`, and the playing one in
/// `pick=`). The first boot on the new build puts them where they would have played — straight
/// after the row they interrupted — and resumes on the pick that was playing.
#[test]
fn an_old_format_queue_is_spliced_into_the_list_on_restore() {
    let row = |t: &str, id: i64| SongRow {
        title: t.into(),
        object_id: id,
        ..Default::default()
    };
    let ctx: Vec<SongRow> = (0..4).map(|i| row(&format!("T{i}"), 10 + i)).collect();
    let mut a = unlocked();
    a.playback_restore(
        ctx.clone(),
        1,
        vec![row("Q0", 90), row("Q1", 91)],
        None,
        Some(row("P", 80)),
    );
    let ids: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(ids, vec![10, 11, 80, 90, 91, 12, 13]);
    assert_eq!(a.context_idx(), 2, "resumes on the pick that was playing");
    assert!(
        a.hand_added,
        "the picks were built by hand, so a replace still asks"
    );

    // No pick, just queued picks: they follow the current row, which stays current.
    let mut b = unlocked();
    b.playback_restore(ctx.clone(), 1, vec![row("Q0", 90)], None, None);
    let ids: Vec<i64> = b.context().iter().map(|t| t.object_id).collect();
    assert_eq!(ids, vec![10, 11, 90, 12, 13]);
    assert_eq!(b.context_idx(), 1);

    // Nothing but picks: the list is the picks, from the top.
    let mut c = unlocked();
    c.playback_restore(vec![], 0, vec![row("Q0", 90), row("Q1", 91)], None, None);
    assert_eq!(c.context().len(), 2);
    assert_eq!(c.context_idx(), 0);
}

/// The index is stored as a number but means "this track". If a file left the library between
/// boots the number now points at a different song, so a restore must clamp rather than
/// resume something the user never chose.
#[test]
fn a_restore_clamps_an_index_past_a_shortened_context() {
    let mut a = unlocked();
    let rows: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.playback_restore(rows, 99, vec![], None, None);
    assert_eq!(a.context_idx(), 2);

    // Empty everything: no context, no index, no panic, and nothing worth persisting.
    let mut b = unlocked();
    b.playback_restore(vec![], 7, vec![], None, None);
    assert_eq!(b.context_idx(), 0);
    assert!(!b.has_playback_state());
}

/// A saved un-shuffle order describes the context it was taken from. If the two no longer
/// agree — a track was deleted between boots — keeping it would make the next shuffle-off
/// reorder against a stale list and drop every track it no longer mentions.
#[test]
fn a_stale_unshuffle_order_is_dropped_on_restore() {
    let rows: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();

    let mut good = unlocked();
    good.playback_restore(rows.clone(), 0, vec![], Some(vec![12, 10, 11]), None);
    assert!(
        good.playback_encode().contains("pre=12,10,11"),
        "a matching order is kept"
    );

    let mut stale = unlocked();
    stale.playback_restore(rows, 0, vec![], Some(vec![12, 10, 11, 13]), None);
    assert!(
        !stale.playback_encode().contains("pre="),
        "a mismatched order is dropped"
    );
}

/// Build a 6-track album context with track `at` playing.
fn ctx6(at: usize) -> App {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..6)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, at);
    a
}

/// Q1, the old regression, in the one-list model. Play Next a track FROM THE ALBUM ALREADY
/// PLAYING and the index must step onto the inserted copy, not search the album and jump to
/// the original — which dropped every track in between.
#[test]
fn a_pick_taken_from_the_playing_album_does_not_skip_the_tracks_between() {
    let mut a = ctx6(1); // T1 playing
    let pick = a.context()[5].clone(); // T5
    a.enqueue_at(pick, 0, QueueAt::Next);

    assert!(
        !a.track_started(15, false),
        "ordinary progression onto the inserted copy"
    );
    assert_eq!(a.context_idx(), 2);
    let rest: Vec<i64> = a.context()[a.context_idx() + 1..]
        .iter()
        .map(|t| t.object_id)
        .collect();
    assert_eq!(
        rest,
        vec![12, 13, 14, 15],
        "T2..T5 all survive, the album's own T5 included"
    );
}

/// A track in the list twice: a jump lands on the NEXT copy after the playing row, not the
/// first copy in the list.
#[test]
fn a_pick_that_was_also_the_next_track_advances_the_context_by_one() {
    let mut a = ctx6(4); // T4 playing
    let again = a.context()[2].clone(); // T2, already played
    a.enqueue_at(again, 0, QueueAt::Later); // …queued again at the end
    assert!(!a.track_started(12, false));
    assert_eq!(a.context_idx(), 6, "the queued copy, not the one behind");
}

/// Ordinary progression and deliberate jumps both follow the list. The jump case is
/// load-bearing: ◁ steps BACKWARDS, so the search cannot be replaced with "advance by one".
#[test]
fn a_track_that_is_not_a_pick_follows_the_context_either_way() {
    let mut a = ctx6(1);
    assert!(!a.track_started(12, false));
    assert_eq!(a.context_idx(), 2, "ordinary progression");

    assert!(!a.track_started(14, false)); // Up Next row tap, forward
    assert_eq!(a.context_idx(), 4);

    assert!(!a.track_started(11, false)); // ◁ rewind, backwards
    assert_eq!(a.context_idx(), 1, "a jump behind the index still lands");

    // A track that is not in the list leaves the index alone rather than guessing.
    assert!(!a.track_started(9999, false));
    assert_eq!(a.context_idx(), 1);
    // And the same track again (repeat-one, a re-report) moves nothing.
    assert!(!a.track_started(11, true));
    assert_eq!(a.context_idx(), 1);
}

/// An album that is one file with a CUE sheet lists the sheet's tracks; a tap plays the file
/// from that track, a swipe queues the file, and Select does what the tap does.
#[test]
fn a_cue_album_page_plays_its_tracks() {
    use crate::model::{ArtistGroup, CueRow, Library};
    let cue = |t: &str, ms: i64| CueRow {
        file: 0,
        title: t.into(),
        start_ms: ms,
        dur: "5:00".into(),
    };
    let album = AlbumRow {
        name: "Kind of Blue".into(),
        track_list: vec![SongRow {
            title: "Kind of Blue".into(),
            object_id: 70,
            ..Default::default()
        }],
        cue: vec![cue("So What", 0), cue("Freddie Freeloader", 562_000)],
        ..Default::default()
    };
    let mut a = unlocked();
    a.set_library(Library {
        album_groups: vec![ArtistGroup {
            artist: "Miles Davis".into(),
            albums: vec![album],
        }],
        ..Default::default()
    });
    assert!(a.open_album_named("Kind of Blue"));
    let row =
        |i: i32| crate::library::album_tracks_top() + i * crate::library::ALBUM_TRACK_RH() + 10;
    assert_eq!(a.tap(240, row(1)), vec![Action::PlayIndexAt(70, 562_000)]);
    assert_eq!(
        a.tap(240, row(0)),
        vec![Action::PlayIndex(70)],
        "the first starts at 0"
    );
    assert!(a.tap(240, row(2)).is_empty(), "two CUE tracks, two rows");
    a.album_track_idx = 1;
    assert_eq!(
        a.press(Button::Select),
        vec![Action::PlayIndexAt(70, 562_000)]
    );
}

/// The album page marks the row that is playing, and none on an album that is not playing.
#[test]
fn the_album_page_marks_the_track_that_is_playing() {
    let song = |id: i64| SongRow {
        object_id: id,
        ..Default::default()
    };
    let mut a = unlocked();
    let album = AlbumRow {
        track_list: (1..=4).map(song).collect(),
        ..Default::default()
    };
    assert_eq!(a.album_playing_row(&album), None, "nothing playing");
    a.context = vec![song(9), song(3)];
    a.context_idx = 1;
    assert_eq!(a.album_playing_row(&album), Some(2));
    a.context_idx = 0;
    assert_eq!(
        a.album_playing_row(&album),
        None,
        "another album's song is playing"
    );
}

/// An album queued with Add to queue plays AFTER the album that is playing — the request this
/// list was merged for. Play next puts a track in front of the rest.
#[test]
fn an_album_queued_later_plays_after_the_one_playing() {
    let mut a = ctx6(1);
    let second = AlbumRow {
        name: "Second".into(),
        track_list: (0..3)
            .map(|i| SongRow {
                title: format!("S{i}"),
                object_id: 50 + i,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(
        a.enqueue_album_at(&second, 0, QueueAt::Later),
        vec![Action::QueueChanged]
    );
    let order: Vec<i64> = a.context()[a.context_idx() + 1..]
        .iter()
        .map(|t| t.object_id)
        .collect();
    assert_eq!(
        order,
        vec![12, 13, 14, 15, 50, 51, 52],
        "the rest of the first album, then the second"
    );
    assert!(a.toast.contains("Second"), "{}", a.toast);

    a.enqueue_at(
        SongRow {
            title: "N".into(),
            object_id: 77,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );
    assert_eq!(
        a.context()[a.context_idx() + 1].object_id,
        77,
        "Play next goes to the front"
    );
    // The list plays top to bottom: the next track to start is the next row.
    for want in [77, 12, 13, 14, 15, 50, 51, 52] {
        let cur = a.context()[a.context_idx()].object_id;
        let _ = cur;
        assert!(!a.track_started(want, false));
        assert_eq!(a.context()[a.context_idx()].object_id, want);
    }
}

/// The 2026-09-11 queue report, on the navigator's side. Over Bluetooth an edit waits for the
/// boundary, so PlayerService stepped from T1 onto T2 from a sequence that never held the Play
/// next. `track_started` with `stale` steps the LIST onto its own next row instead and says so,
/// and T2 is still in the list behind it: nothing skipped, nothing twice.
#[test]
fn a_context_track_the_service_stepped_onto_goes_back_behind_the_picks() {
    let mut a = ctx6(1); // T1 playing
    a.enqueue_at(
        SongRow {
            title: "Queued".into(),
            object_id: 777,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );

    assert!(
        a.track_started(12, true),
        "the service went ahead of the edit"
    );
    assert_eq!(
        a.context()[a.context_idx()].object_id,
        777,
        "the list's next row plays"
    );
    let rest: Vec<i64> = a.context()[a.context_idx() + 1..]
        .iter()
        .map(|t| t.object_id)
        .collect();
    assert_eq!(rest, vec![12, 13, 14, 15], "T2 leads what is left");
    // History got the track that was playing, not the one that started early.
    assert_eq!(a.history().last().map(|h| h.object_id), Some(11));
}

/// `stale` does not override a deliberate step BACKWARDS, and there is nothing to put first at
/// the end of the list.
#[test]
fn deferring_refuses_anything_but_the_current_track_with_a_queue_behind_it() {
    let mut a = ctx6(3);
    assert!(!a.track_started(11, true), "◁ went back on purpose");
    assert_eq!(a.context_idx(), 1);

    let mut b = ctx6(5);
    assert!(
        !b.track_started(10, true),
        "the last row has no next row to prefer"
    );
    assert_eq!(b.context_idx(), 0, "…so it is an ordinary jump");
}

/// A Songs list long enough to scroll far past the band's slide.
fn long_songs() -> App {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    let base = a.lib.songs.clone();
    for n in 1..40 {
        a.lib.songs.extend(base.iter().map(|s| SongRow {
            object_id: s.object_id + n * 100_000,
            ..s.clone()
        }));
    }
    assert!(
        a.lib_max_scroll() > 1000,
        "the fixture must scroll well past the band"
    );
    a
}

/// The header's view button steps the tab through its views, says which one it is now, keeps
/// the item at the top of the screen at the top, and survives the settings file.
/// The Library's tab and sort orders survive a restart (`lib_place=`), and a bad value is
/// ignored rather than trusted.
#[test]
fn library_tab_and_sorts_round_trip() {
    let mut a = App::unlocked();
    a.lib_tab = Tab::Albums;
    a.lib_sort = library::SORTS.len() - 1;
    a.album_sort = library::ALBUM_SORTS.len() - 1;
    let saved = a.lib_place_str();
    let mut b = App::unlocked();
    b.set_lib_place_str(&saved);
    assert_eq!(
        (b.lib_tab, b.lib_sort, b.album_sort),
        (a.lib_tab, a.lib_sort, a.album_sort)
    );
    b.set_lib_place_str("songs|x|0");
    b.set_lib_place_str("songs");
    assert_eq!(b.lib_tab, Tab::Albums, "garbage changes nothing");
    b.set_lib_place_str("artists|999|999");
    assert_eq!(b.lib_tab, Tab::Artists);
    assert_eq!(b.lib_sort, library::SORTS.len() - 1, "clamped");
    assert_eq!(b.album_sort, library::ALBUM_SORTS.len() - 1, "clamped");
}

#[test]
fn the_view_button_switches_the_layout_and_remembers_it() {
    let mut a = long_songs();
    let (x0, x1) = library::view_button_x(a.search_enabled);
    let (vx, vy) = ((x0 + x1) / 2, 62);
    // Scroll to the 20th song, then switch: the 20th song is still the first one on screen.
    a.lib_scroll_px = 20 * library::row_h(Tab::Songs);
    a.tap(vx, vy);
    assert_eq!(
        library::view_of(&a.lib, Tab::Songs),
        library::LibView::Compact
    );
    assert_eq!(a.toast, "Compact view");
    assert_eq!(a.lib_scroll_px, 20 * library::COMPACT_ROW_H);
    assert_eq!(a.lib_views_str(), "compact,list,list,list");
    a.tap(vx, vy);
    assert_eq!(library::view_of(&a.lib, Tab::Songs), library::LibView::List);

    // Per tab, and read back from the settings file's words.
    let mut b = unlocked();
    b.set_lib_views_str("list,grid,compact,nonsense");
    assert_eq!(b.lib_views_str(), "list,grid,compact,list");
    // A new library (a rescan) keeps them.
    let lib = b.lib.clone();
    b.set_library(lib);
    assert_eq!(
        library::view_of(&b.lib, Tab::Albums),
        library::LibView::Grid
    );
    // Settings ▸ Reset puts every tab back in the list.
    b.reset_settings();
    assert_eq!(
        library::view_of(&b.lib, Tab::Albums),
        library::LibView::List
    );
}

/// In a grid a tile is one target: the right edge of a tile opens the artist, not the row
/// shortcut the list keeps there, and an album tile never slides for swipe-to-queue.
#[test]
fn a_grid_tile_opens_and_never_swipes() {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Artists;
    a.set_lib_views_str("list,grid,grid,grid");
    let y = library::list_top(Tab::Artists) + 50;
    let x = library::grid_x(3) + library::GRID_TILE - 4; // right of 404: the list's shuffle button
    assert!(x >= 404);
    let acts = a.tap(x, y);
    assert!(
        !acts.iter().any(|t| matches!(t, Action::ShuffleArtist(_))),
        "{acts:?}"
    );
    assert_eq!(a.current(), Screen::Artist);
    assert_eq!(a.artist_view, 3);

    let mut b = unlocked();
    b.stack = vec![Screen::Library];
    b.lib_tab = Tab::Albums;
    b.set_lib_views_str("list,grid,list,list");
    let y = library::list_top(Tab::Albums) + library::ALBUM_HDR_H + 50;
    assert!(
        !b.swipe_track(80, y),
        "a grid line must not slide under the finger"
    );
    assert!(b.grid_cover_wants().iter().all(|id| *id > 0));
}

/// The Library's shuffle band slides up under the tab strip as the list scrolls down, tracking
/// the finger, and comes back the moment the list scrolls up — mid-list, not only at the top.
/// Requested 2026-09-11 for Songs, Albums, Artists and Playlists, which share this one offset.
#[test]
fn the_shuffle_band_slides_away_on_the_way_down_and_back_on_the_way_up() {
    let mut a = long_songs();
    let slide = library::band_slide(Tab::Songs);
    assert_eq!(a.lib_band(), 0, "at the top it is fully shown");

    a.scroll_px(30);
    assert_eq!(a.lib_band(), 30, "it tracks the finger");
    a.scroll_px(400);
    assert_eq!(
        a.lib_band(),
        slide,
        "and stops once it is fully under the tabs"
    );

    a.scroll_px(-20);
    assert_eq!(
        a.lib_band(),
        slide - 20,
        "scrolling up brings it straight back"
    );
    a.scroll_px(-60);
    assert_eq!(a.lib_band(), 0, "all the way");
    assert!(
        a.lib_scroll_px > 0,
        "without having to go back to the top of the list"
    );
}

/// A slide bigger than the scroll would leave a strip of bare background between the band and
/// the first row. No sequence of gestures may produce one.
#[test]
fn the_band_never_opens_a_gap_above_the_first_row() {
    let mut a = long_songs();
    for d in [5, 40, -30, 200, -190, -40, 7, -3, 90, -95, 300, -299, -50] {
        a.scroll_px(d);
        let (band, scroll) = (a.lib_band(), a.lib_scroll_px);
        assert!(
            band <= scroll,
            "slide {band} > scroll {scroll} after a {d} px scroll"
        );
        assert!(
            (0..=library::band_slide(Tab::Songs)).contains(&band),
            "slide {band} out of range"
        );
    }
}

/// Once the band has gone, its old place on the glass is list: a tap there opens a song instead
/// of starting a shuffle. That is the single-source rule — the tap reads the offset the renderer
/// drew with, so the two cannot disagree about where the band is.
#[test]
fn a_hidden_band_takes_no_taps_and_the_rows_that_replaced_it_do() {
    let mut a = long_songs();
    let (bx, by, bw, bh) = library::library_shuffle_band();
    let (x, y) = (bx + bw / 2, by + bh / 2);
    assert!(
        a.tap(x, y).iter().any(|t| matches!(t, Action::Shuffle(_))),
        "shown: the band shuffles"
    );

    // On Songs the FILTER STRIP rides up into the band's old place — the band is the only thing
    // that leaves, so the filter stays reachable mid-list. That spot is the filter now.
    let mut b = long_songs();
    b.scroll_px(600);
    assert_eq!(b.lib_band(), library::band_slide(Tab::Songs));
    assert!(
        !library::hit_shuffle_band_at(x, y, b.lib_band()),
        "hidden: the band is not there"
    );
    assert!(
        library::filter_hit_at(Tab::Songs, y, b.lib_band()),
        "hidden: the filter moved up under it"
    );
    let acts = b.tap(x, y);
    assert!(
        !acts.iter().any(|t| matches!(t, Action::Shuffle(_))),
        "hidden: no shuffle — {acts:?}"
    );

    // Artists has no filter strip, so there the ROWS take the band's place.
    let mut ar = long_songs();
    ar.lib_tab = Tab::Artists;
    let base = ar.lib.artists.clone();
    for _ in 0..40 {
        ar.lib.artists.extend(base.iter().cloned());
    }
    ar.lib_scroll_px = 0;
    ar.scroll_px(600);
    assert_eq!(ar.lib_band(), library::band_slide(Tab::Artists));
    assert!(
        library::hit_row_at(Tab::Artists, &ar.lib, ar.lib_scroll_px, y, ar.lib_band()).is_some(),
        "hidden: on a tab with no filter that y is an artist row"
    );
}

/// Changing tab zeroes the scroll, and the clamp in `band_offset` brings the band back on the same
/// frame — nothing has to remember to reset it.
#[test]
fn a_fresh_list_always_shows_its_band() {
    let mut a = long_songs();
    a.scroll_px(600);
    assert_eq!(a.lib_band(), library::band_slide(Tab::Songs));
    a.lib_tab = Tab::Albums;
    a.lib_scroll_px = 0;
    assert_eq!(a.lib_band(), 0);
}

/// NEW PLAYLIST leaves with the band and comes back with it — asked for 2026-09-11, once the band
/// had passed on the device. While it is away its old place is list, and a tap there is a playlist.
#[test]
fn new_playlist_slides_away_with_the_band_and_comes_back() {
    let many = || {
        let mut a = own_and_sony();
        let base = a.lib.playlists.clone();
        for _ in 0..20 {
            a.lib.playlists.extend(base.iter().cloned());
        }
        a
    };
    let mut a = many();
    let slide = library::band_slide(Tab::Playlists);
    assert!(
        slide > library::band_slide(Tab::Songs),
        "the row goes as well as the band"
    );
    assert!(
        a.lib_max_scroll() > slide + 200,
        "the fixture must scroll well past the row"
    );
    let (x, y, w, h) = library::new_playlist_rect();
    let (px, py) = (x + w / 2, y + h / 2);

    a.scroll_px(600);
    assert_eq!(a.lib_band(), slide);
    assert_eq!(
        y + h - a.lib_band(),
        library::TABS_BOTTOM,
        "fully under the tab strip, and no further"
    );
    assert!(
        !library::hit_new_playlist_at(Tab::Playlists, px, py, a.lib_band()),
        "hidden: it is not there"
    );
    assert!(
        library::hit_row_at(Tab::Playlists, &a.lib, a.lib_scroll_px, py, a.lib_band()).is_some(),
        "hidden: its old place is a playlist row"
    );
    let mut b = many();
    b.scroll_px(600);
    b.tap(px, py);
    assert_ne!(
        b.current(),
        Screen::Keyboard,
        "hidden: a tap where it was must not open the keyboard"
    );

    a.scroll_px(-slide);
    assert_eq!(a.lib_band(), 0, "scrolling up brings it straight back");
    assert!(a.lib_scroll_px > 0, "mid-list, not only at the top");
    assert!(a.tap(px, py).is_empty());
    assert_eq!(
        a.current(),
        Screen::Keyboard,
        "and it works the moment it is back"
    );
}

/// Q2, the regression. Shuffle used to be a one-way door — ON permuted the remainder, OFF did
/// nothing at all, and the list stayed shuffled for the rest of the session.
#[test]
fn turning_shuffle_off_puts_the_context_back_in_order() {
    let mut a = ctx6(1);
    let before: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();

    assert!(
        !a.queue_shuffle().is_empty(),
        "there are five other tracks to shuffle"
    );
    let shuffled: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_ne!(shuffled, before, "the list actually moved");
    assert_eq!(
        a.context()[a.context_idx()].object_id,
        11,
        "the playing track keeps playing"
    );

    assert_eq!(a.unshuffle_context(), vec![Action::QueueChanged]);
    let after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(after, before, "restored to the album's order");
    assert_eq!(
        a.context_idx(),
        1,
        "and the same track is still the current one"
    );

    // Nothing left to undo, so a second off is inert rather than emitting a pointless re-issue.
    assert!(a.unshuffle_context().is_empty());
}

/// THE 2026-09-23 REPORT: start an album part-way through, press shuffle, and it "cuts out half
/// of the songs". Shuffle took only the tracks AFTER the playing one, so the tracks before the
/// start point never played. Every track of the album has to be in the shuffled list.
#[test]
fn shuffle_after_starting_mid_album_keeps_every_track() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..12)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album.clone(), 6); // started at track 7
    a.queue_shuffle();
    assert_eq!(
        a.context_idx(),
        0,
        "the playing track leads the shuffled list"
    );
    assert_eq!(
        a.context()[0].object_id,
        16,
        "…and it is still the track that was playing"
    );
    let mut coming: Vec<i64> = a.context()[1..].iter().map(|t| t.object_id).collect();
    coming.sort_unstable();
    let mut want: Vec<i64> = album
        .iter()
        .map(|t| t.object_id)
        .filter(|id| *id != 16)
        .collect();
    want.sort_unstable();
    assert_eq!(
        coming, want,
        "all eleven other tracks are still to come — none cut out"
    );
    assert_eq!(a.upcoming_len(), 11);

    // Off again: the album's own order, carrying on from the track that is playing.
    a.unshuffle_context();
    assert_eq!(a.context().len(), 12);
    assert_eq!(a.context()[a.context_idx()].object_id, 16);
    assert_eq!(a.context_idx(), 6);
}

/// A track in the list twice (an album queued while it plays) survives a shuffle round trip as
/// two separate rows in their own places — ranking by id alone stacked the copies together.
#[test]
fn unshuffle_keeps_both_copies_of_a_track_in_their_own_places() {
    let mut a = ctx6(0);
    let again = a.context()[2].clone();
    a.enqueue_at(again, 0, QueueAt::Later);
    let before: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    a.queue_shuffle();
    a.unshuffle_context();
    let after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(after, before);
}

/// Rows added while shuffled land where they were put when shuffle goes off: Play next stays
/// straight after the playing track, Add to queue stays at the end.
#[test]
fn rows_added_while_shuffled_keep_their_place_when_shuffle_goes_off() {
    let mut a = ctx6(2);
    a.queue_shuffle();
    a.enqueue_at(
        SongRow {
            title: "N".into(),
            object_id: 70,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );
    a.enqueue_at(
        SongRow {
            title: "L".into(),
            object_id: 80,
            ..Default::default()
        },
        0,
        QueueAt::Later,
    );
    a.unshuffle_context();
    let after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(after, vec![10, 11, 12, 70, 13, 14, 15, 80]);
    assert_eq!(a.context()[a.context_idx()].object_id, 12);
}

/// The restored index follows the AUDIBLE track, not the old slot number. After a shuffle the
/// playing song is rarely where it started, and reusing the index would make Up Next draw the
/// wrong part of the list as coming next.
#[test]
fn unshuffle_follows_the_track_that_is_playing_not_the_index() {
    let mut a = ctx6(0);
    let original: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    a.queue_shuffle();
    // Walk forward into the shuffled list so the current track is somewhere unpredictable.
    a.track_started(a.context()[3].object_id, false);
    let playing = a.context()[a.context_idx()].object_id;

    a.unshuffle_context();
    assert_eq!(
        a.context()[a.context_idx()].object_id,
        playing,
        "the same song is still current after the reorder"
    );
    let after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(after, original);
}

/// The path the toggle actually breaks on: shuffle ON **first**, then tap an album. The shell
/// shuffles the rows before handing them over, so `set_play_context` receives an already
/// permuted context and clears the undo — and shuffle-off then had nothing to restore and did
/// nothing at all. Reported 2026-08-19 as "some edge cases for shuffle that don't work".
#[test]
fn shuffle_on_before_play_can_still_be_turned_off() {
    let mut a = ctx6(0);
    // What the shell hands in: the album's real order, permuted, plus the order it replaced.
    let original: Vec<i64> = vec![10, 11, 12, 13, 14, 15];
    let played: Vec<i64> = vec![13, 15, 10, 12, 14, 11];
    let rows: Vec<SongRow> = played
        .iter()
        .map(|id| SongRow {
            title: format!("T{id}"),
            object_id: *id,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 0);
    a.note_pre_shuffle(original.clone());

    assert_eq!(
        a.unshuffle_context(),
        vec![Action::QueueChanged],
        "shuffle-off did nothing"
    );
    let after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(
        after, original,
        "the album did not come back in its running order"
    );
    assert_eq!(
        a.context()[a.context_idx()].object_id,
        13,
        "the song that was playing must still be the current one"
    );
}

/// The guard: an order that does not describe this list is refused rather than half-applied.
#[test]
fn a_pre_shuffle_order_of_the_wrong_length_is_refused() {
    let mut a = ctx6(0);
    let n = a.context().len();
    a.note_pre_shuffle(vec![1, 2, 3]);
    assert!(
        a.unshuffle_context().is_empty(),
        "an order of other tracks was accepted"
    );
    a.note_pre_shuffle((0..9).collect());
    assert!(
        a.unshuffle_context().is_empty(),
        "a longer order was accepted"
    );
    assert_eq!(a.context().len(), n, "and the list was left alone");
    // A PREFIX that is an order of the list's first rows is fine: "keep Up Next" appends the
    // old list after the new sequence, and the kept rows keep the order they have.
    a.note_pre_shuffle(vec![11, 10]);
    assert_eq!(a.unshuffle_context(), vec![Action::QueueChanged]);
    let ids: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(ids, vec![11, 10, 12, 13, 14, 15]);
}

/// Playing something new drops the undo. Restoring THIS context into a previous album's order
/// would reorder by a ranking that mostly does not apply to it.
#[test]
fn a_new_context_forgets_the_shuffle_undo() {
    let mut a = ctx6(0);
    a.queue_shuffle();
    let rows: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("N{i}"),
            object_id: 90 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 0);
    assert!(
        a.unshuffle_context().is_empty(),
        "nothing to undo on a fresh sequence"
    );
    let ids: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(ids, vec![90, 91, 92], "and the new context is untouched");
}

/// Up Next is ONE list below NOW PLAYING — no separate queue section — in the order the shell
/// builds the sequence from.
#[test]
fn a_queued_song_plays_before_the_rest_of_the_album() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, 1); // A1 playing
    a.enqueue_at(
        SongRow {
            title: "PICK".into(),
            object_id: 99,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );
    a.history_restore(vec![SongRow {
        object_id: 10,
        ..Default::default()
    }]);
    let l = a.up_next_layout();
    let order: Vec<crate::up_next::Slot> = l.slots.iter().map(|(s, _)| *s).collect();
    use crate::up_next::{Section, Slot};
    assert_eq!(
        order,
        vec![
            Slot::Head(Section::History),
            Slot::History(0),
            Slot::Head(Section::Now),
            Slot::Current(1),
            Slot::Head(Section::Next),
            Slot::Upcoming(2),
            Slot::Upcoming(3),
            Slot::Upcoming(4),
        ]
    );
    assert_eq!(
        a.context()[2].object_id,
        99,
        "Play next sits straight after the playing track"
    );
}

/// MIX with shuffle already on deals the list again: a permutation of the same rows, the
/// playing one first.
#[test]
fn queue_shuffle_only_moves_what_is_still_to_come() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..12)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album.clone(), 2);
    let acts = a.queue_shuffle();
    assert_eq!(
        acts,
        vec![Action::QueueChanged],
        "the sequence must be re-issued"
    );
    assert_eq!(a.context()[0].object_id, 12);
    let mut before: Vec<i64> = album.iter().map(|t| t.object_id).collect();
    let mut after: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_ne!(
        after[1..],
        before[..],
        "12 tracks should not shuffle back into their order"
    );
    before.sort_unstable();
    after.sort_unstable();
    assert_eq!(
        before, after,
        "a permutation, not a truncation or a duplication"
    );
    // Nothing to shuffle => no action, no crash.
    a.set_play_context(vec![SongRow::default(), SongRow::default()], 0);
    assert!(a.queue_shuffle().is_empty());
}

/// The playing song tapped again in another list asks first, and "Play the rest … next" queues
/// what follows it there without a play action; the list it is already playing does not ask.
#[test]
fn the_playing_song_tapped_again_offers_the_rest_of_the_list() {
    use crate::confirm::{hit, Ask, Hit};
    let row = |id: i64| SongRow {
        object_id: id,
        title: format!("t{id}"),
        ..Default::default()
    };
    let mut a = unlocked();
    a.lib.songs = (1..=5).map(row).collect();
    a.set_play_context(vec![row(9), row(3), row(8)], 1); // song 3 plays, from another list
    let press = |a: &mut App, ask: Ask, want: Hit| {
        let y = (0..crate::canvas::H as i32)
            .find(|y| hit(ask, 240, *y) == want)
            .unwrap();
        a.tap(240, y)
    };
    // Songs list 1..5, song 3 tapped: asked, nothing played.
    assert!(a.start_play_list(vec![1, 2, 3, 4, 5], 2).is_empty());
    assert_eq!(a.confirm, Some(Ask::SongPlayingList));
    assert_eq!(
        press(&mut a, Ask::SongPlayingList, Hit::RestNext),
        vec![Action::QueueChanged]
    );
    let ids: Vec<i64> = a.context().iter().map(|r| r.object_id).collect();
    assert_eq!(
        ids,
        [9, 3, 4, 5, 8],
        "the rest of the list, straight after the playing song"
    );
    // Tapped again, the rest is already what follows: no "playing" question, only the usual
    // one about Up Next, which now holds songs added by hand.
    assert!(a.start_play_list(vec![1, 2, 3, 4, 5], 2).is_empty());
    assert_eq!(a.confirm, Some(Ask::QueueOnPlay));
    a.confirm = None;
    // Start over from the list instead.
    a.set_play_context(vec![row(9), row(3), row(8)], 1);
    a.hand_added = false;
    a.start_play_list(vec![1, 2, 3, 4, 5], 2);
    assert_eq!(
        press(&mut a, Ask::SongPlayingList, Hit::RestRestart),
        vec![Action::PlayListAt(2)]
    );
    // A different song never asks.
    a.set_play_context(vec![row(9), row(3), row(8)], 1);
    a.hand_added = false;
    assert_eq!(
        a.start_play_list(vec![1, 2, 3, 4, 5], 0),
        vec![Action::PlayListAt(0)]
    );
}

/// Owner, 11-23: add one song, let it play, then start something new — no "Play this now?",
/// because what is left in Up Next is only the album's own rest. While the added song is still
/// to come, it asks.
#[test]
fn the_up_next_question_ends_once_the_added_songs_have_played() {
    use crate::confirm::Ask;
    let row = |id: i64| SongRow {
        object_id: id,
        ..Default::default()
    };
    let mut a = unlocked();
    a.set_play_context(vec![row(1), row(2), row(3), row(4)], 0);
    a.insert_rows(vec![row(9)], QueueAt::Next); // 1, 9, 2, 3, 4
    assert!(a.start_play_now(Action::PlayIndex(50)).is_empty());
    assert_eq!(a.confirm, Some(Ask::QueueOnPlay), "9 is still to come");
    a.confirm = None;
    a.pending_play = None;
    a.context_idx = 2; // 9 played; 2 is playing, 3 and 4 are the album's own rest
    assert_eq!(
        a.start_play_now(Action::PlayIndex(50)),
        vec![Action::PlayIndex(50)]
    );
    assert_eq!(a.confirm, None);
    // Added at the end: still asks all the way until it plays.
    a.insert_rows(vec![row(7)], QueueAt::Later);
    a.context_idx = 3;
    assert!(a.start_play_now(Action::PlayIndex(50)).is_empty());
}

/// Removing the SECOND copy of a song from Up Next forgets that copy's place in the un-shuffle
/// order, not the first copy's: shuffle-off then puts the first copy back where it was.
#[test]
fn removing_a_repeated_song_keeps_the_other_copy_in_its_place() {
    let row = |id: i64, title: &str| SongRow {
        object_id: id,
        title: title.into(),
        ..Default::default()
    };
    let mut a = unlocked();
    a.set_play_context(
        vec![
            row(2, "two"),
            row(1, "first"),
            row(3, "three"),
            row(1, "second"),
        ],
        0,
    );
    a.note_pre_shuffle(vec![1, 2, 3, 1]);
    a.remove_upcoming(3);
    a.unshuffle_context();
    let titles: Vec<&str> = a.context().iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, ["first", "two", "three"]);
}

/// Shuffling mid-list: the songs still to come are dealt first and the ones already played
/// after them, so a song just heard does not come straight back.
#[test]
fn queue_shuffle_puts_what_has_played_last() {
    let mut a = unlocked();
    let rows: Vec<SongRow> = (0..12)
        .map(|i| SongRow {
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 5);
    a.queue_shuffle();
    let ids: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(ids[0], 15, "the playing song leads");
    assert!(ids[1..7].iter().all(|&i| i > 15), "unheard next: {ids:?}");
    assert!(ids[7..].iter().all(|&i| i < 15), "heard last: {ids:?}");
}

/// MIX turns shuffle ON when it is off — the same as the transport button, so the icon on Now
/// Playing tells the truth — and deals again when it is already on.
#[test]
fn the_mix_chip_turns_shuffle_on_before_it_reshuffles() {
    let mut a = queued(5);
    let (sx, sy, sw, sh) = crate::up_next::SHUFFLE_CHIP;
    assert_eq!(a.tap(sx + sw / 2, sy + sh / 2), vec![Action::ShuffleToggle]);
    a.shuffle_on = true;
    assert_eq!(a.tap(sx + sw / 2, sy + sh / 2), vec![Action::QueueChanged]);
}

/// The context index follows playback, so Up Next's history/up-next split stays true.
#[test]
fn the_context_index_follows_the_track_that_starts() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, 0);
    assert!(a.set_context_playing(12));
    assert_eq!(a.context_idx(), 2);
    assert!(!a.set_context_playing(12), "no move, no repaint");
    // A track that is not in the context (a user-queue pick) leaves the index alone rather
    // than guessing where we are.
    assert!(!a.set_context_playing(9999));
    assert_eq!(a.context_idx(), 2);
}

/// "Play now, then Up Next" has to keep it: the new sequence plays, and what was still to come
/// follows it.
#[test]
fn keeping_the_queue_appends_the_new_sequence() {
    use crate::confirm::{hit, Ask, Hit};
    let pick = |want: Hit| {
        (0..crate::canvas::H as i32)
            .find(|y| hit(Ask::QueueOnPlay, 240, *y) == want)
            .expect("row has no tappable pixel")
    };
    let mut a = with_playing(unlocked(), 2);
    a.queue_push_for_test(); // one hand-swiped pick
    assert!(a.start_play(77).is_empty(), "hand-built picks still prompt");
    assert_eq!(
        a.tap(240, pick(Hit::KeepQueue)),
        vec![Action::PlayIndex(77)]
    );

    let rows: Vec<SongRow> = (0..2)
        .map(|i| SongRow {
            title: format!("S{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 0);
    let ids: Vec<i64> = a.context().iter().map(|t| t.object_id).collect();
    assert_eq!(
        ids,
        vec![10, 11, 7001, 0],
        "the new sequence, then what was still to come"
    );
    assert!(
        a.hand_added,
        "the kept rows are still hand-built, so the next replace asks too"
    );
    // The flag is one-shot: the next play replaces the list.
    a.set_play_context(vec![SongRow::default()], 0);
    assert_eq!(a.context().len(), 1);
    assert!(!a.hand_added);
}

/// A long list scrolls, and the hit test follows the scroll.
#[test]
fn the_user_queue_scrolls() {
    let mut a = queued(40);
    let max = a.up_next_layout().max_scroll_px();
    assert!(max > 0);
    a.scroll_px(10_000);
    assert_eq!(a.queue_scroll_px, max, "must scroll, and clamp to the end");
    let y_unscrolled = crate::chrome::HEADER_BOTTOM
        + a.up_next_layout()
            .top_of(crate::up_next::Slot::Upcoming(1))
            .unwrap()
        + crate::up_next::RH() / 2;
    assert_ne!(
        a.up_next_layout().at(y_unscrolled, a.queue_scroll_px),
        Some(crate::up_next::Slot::Upcoming(1))
    );
}

/// Repeat is two states, both of which do something. It used to cycle off → all → one and tell
/// PlayerService nothing at all; "all" has no known primitive, so a third position would still
/// be decorative. Every press must also emit an action, or the shell never applies it.
#[test]
fn repeat_is_two_real_states_and_always_reaches_the_shell() {
    let mut a = unlocked();
    assert_eq!(a.current(), Screen::NowPlaying);
    // The repeat icon lives at the far right of the transport row.
    let acts = a.tap(436, 692);
    assert_eq!(
        acts,
        vec![Action::RepeatCycle],
        "tapping repeat must emit an action"
    );
    let acts = a.tap(436, 692);
    assert_eq!(
        acts,
        vec![Action::RepeatCycle],
        "and again on the way back off"
    );
}

/// Shuffle likewise: the icon is at the far LEFT of the same row, and must not be swallowed by
/// the prev-track target beside it.
#[test]
fn shuffle_and_repeat_do_not_steal_each_others_taps() {
    let mut a = unlocked();
    assert_eq!(a.tap(44, 692), vec![Action::ShuffleToggle]);
    assert_eq!(a.tap(436, 692), vec![Action::RepeatCycle]);
    assert_eq!(a.tap(130, 692), vec![Action::Prev]);
    assert_eq!(a.tap(350, 692), vec![Action::Next]);
}

/// Tapping a swatch must select THAT accent, not advance the cycle by one. This is the whole
/// reason the swatches are hit-tested separately from the row. (Settings ▸ Display since the
/// 2026-09 redesign; the page does not scroll.)
#[test]
fn tapping_an_accent_swatch_picks_that_colour() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    use crate::display::{accent_hit, row_top, ROW_ACCENT};
    let mut a = unlocked();
    a.go(Screen::Display);
    let y = row_top(ROW_ACCENT) + crate::kit::ROW_H() / 2;
    for want in 0..Accent::COUNT {
        // Sweep x to find a pixel that really is inside swatch `want`, then tap exactly there.
        let x = (0..crate::canvas::W as i32)
            .find(|x| accent_hit(*x, y) == Some(want))
            .unwrap_or_else(|| panic!("swatch {want} has no tappable pixel"));
        let acts = a.tap(x, y);
        assert!(
            acts.is_empty(),
            "accent is render-only; it must not emit a shell action"
        );
        assert_eq!(
            a.accent(),
            want as u8,
            "tapping swatch {want} selected something else"
        );
        assert_eq!(
            a.display_sel, ROW_ACCENT,
            "the tap should focus the Accent row"
        );
    }
}

/// The physical Select button still cycles the row — the swatches are the shortcut, not the
/// only way in.
#[test]
fn select_on_the_accent_row_cycles() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    let mut a = unlocked();
    a.go(Screen::Display);
    a.display_sel = crate::display::ROW_ACCENT;
    for i in 0..Accent::COUNT {
        assert_eq!(a.accent() as usize, i);
        a.press(Button::Select);
    }
    assert_eq!(a.accent(), 0, "the cycle must wrap back to the default");
}

/// Opening Display counts as leaving the Boot to stock row, so a pending restart confirmation
/// does not survive a trip to pick a colour — otherwise coming back could leave the device one
/// stray tap from rebooting.
#[test]
fn visiting_display_disarms_boot_to_stock() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    let mut a = unlocked();
    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_BOOT_STOCK;
    assert!(a.settings_activate().is_empty(), "first tap only arms");
    assert!(a.boot_stock_armed);
    let y = crate::settings::LIST_TOP + crate::kit::ROW_H() / 2;
    assert_eq!(
        crate::settings::row_at(y, 0, None, false),
        Some(crate::settings::ROW_DISPLAY)
    );
    a.settings_scroll_px = 0;
    a.tap(240, y);
    assert_eq!(a.current(), Screen::Display);
    let sy = crate::display::row_top(crate::display::ROW_ACCENT) + 20;
    let x = (0..crate::canvas::W as i32)
        .find(|x| crate::display::accent_hit(*x, sy).is_some())
        .unwrap();
    a.tap(x, sy);
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Settings);
    assert!(
        !a.boot_stock_armed,
        "picking a colour left the restart armed"
    );
}

/// The swatch band must sit inside the Accent row and nowhere else — a swatch hit-test that
/// leaked into the neighbouring rows would swallow taps on Palette or Night.
#[test]
fn accent_swatches_do_not_leak_into_other_rows() {
    use crate::display::{accent_hit, row_at, ROW_ACCENT};
    for y in 0..crate::canvas::H as i32 {
        for x in (0..crate::canvas::W as i32).step_by(3) {
            if accent_hit(x, y).is_some() {
                assert_eq!(
                    row_at(y),
                    Some(ROW_ACCENT),
                    "swatch hit at ({x},{y}) is outside the Accent row"
                );
            }
        }
    }
}

/// Choosing Minimal on Display switches the volume readout, shows it at once so the choice is
/// visible, and round-trips through the settings-file word.
#[test]
fn the_volume_readout_is_chosen_on_display() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    let mut a = unlocked();
    a.go(Screen::Display);
    assert_eq!(a.volume_hud(), "full", "Full is the default");
    let top = crate::display::row_top(crate::display::ROW_VOLUME);
    let y = top + crate::kit::ROW_H() / 2;
    let x = (0..crate::canvas::W as i32)
        .rev()
        .find(|x| crate::display::volume_chip_at(*x, y) == Some(1))
        .unwrap();
    assert!(a.tap(x, y).is_empty());
    assert_eq!(a.volume_hud(), "minimal");
    assert!(a.vol_overlay > 0, "the choice is shown, not just stored");
    let mut b = unlocked();
    b.set_volume_hud(a.volume_hud());
    assert_eq!(b.volume_hud(), "minimal");
    b.set_volume_hud("garbage");
    assert_eq!(b.volume_hud(), "full", "anything unknown is the default");
}

/// Scrolling to the bottom must actually bring the last row fully on screen — a max_scroll that
/// is too small leaves it half-clipped and awkward to hit.
#[test]
fn settings_scrolls_far_enough_to_reach_the_last_row() {
    use crate::settings::{content_height, max_scroll_px, GROUPS};
    let _scale = lock_scale();
    // Big view's taller rows are what make a page taller than the panel.
    crate::scale::set_large(true);
    assert!(
        max_scroll_px(None, true) > 0,
        "the home page is taller than the panel in Big view; it must scroll"
    );
    for page in std::iter::once(None).chain((0..GROUPS.len()).map(Some)) {
        assert!(
            content_height(page, true) - max_scroll_px(page, true) <= crate::canvas::H as i32,
            "bottom of page {page:?} is still off-screen at full scroll"
        );
    }
    crate::scale::set_large(false);
}

/// The A–Z rail must land on a row that really is in that bucket, on every tab. A jump that
/// lands near-but-not-on the letter is worse than no jump: you can't tell it worked.
#[test]
fn az_jump_lands_on_a_row_in_that_bucket() {
    let app = unlocked();
    for &letter in library::AZ_LETTERS {
        for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
            let Some(px) = library::az_scroll_for(
                tab,
                &app.lib,
                letter,
                app.lib_sort,
                app.album_sort,
                app.album_expanded,
            ) else {
                continue; // no rows in this bucket — rail greys it and the tap is a no-op
            };
            let max = library::max_scroll_px(tab, &app.lib, app.album_sort, app.album_expanded);
            assert!(
                (0..=max).contains(&px),
                "{tab:?}/{}: scroll {px} out of 0..={max}",
                letter as char
            );
        }
    }
}

/// The rail files a name under its first letter with case and accents folded, and anything not
/// a Latin letter under '#'. A title keeps its "The"; an ARTIST loses it only with Settings ▸
/// Ignore "The" in artists — the rail follows the list's order either way, or a jump lands
/// nowhere near the eye.
#[test]
fn az_buckets_fold_case_and_accents_and_artists_follow_the_setting() {
    assert_eq!(library::az_bucket("aphex twin"), b'A');
    assert_eq!(library::az_bucket("bôa"), b'B');
    assert_eq!(library::az_bucket("Édith Piaf"), b'E');
    assert_eq!(library::az_bucket("65daysofstatic"), b'#');
    assert_eq!(library::az_bucket("...And Justice For All"), b'#');
    assert_eq!(library::az_bucket(""), b'#');
    assert_eq!(
        library::az_bucket("アルク"),
        b'#',
        "other scripts file under #"
    );
    assert_eq!(
        library::az_bucket("The Beatles"),
        b'T',
        "a title keeps its The"
    );

    let artist =
        |lib: &crate::model::Library, s: &str| library::az_letter(lib, library::AzKey::Artist, s);
    let mut lib = crate::model::Library::default();
    assert_eq!(
        artist(&lib, "The Beatles"),
        b'T',
        "off by default: artists as written"
    );
    lib.ignore_the = true;
    assert_eq!(artist(&lib, "The Beatles"), b'B');
    assert_eq!(artist(&lib, "THE NORTH"), b'N', "a leading The in any case");
    assert_eq!(artist(&lib, "the xx"), b'X');
    assert_eq!(
        artist(&lib, "Theatre of Tragedy"),
        b'T',
        "only a whole leading word"
    );
}

/// Settings ▸ Ignore "The" in artists: flipping it re-sorts the STORED artist lists at once, an
/// open artist page and album keep what they point at rather than their index, a rescan's
/// reload keeps the setting, and Reset settings turns it back off.
#[test]
fn ignore_the_resorts_artists_keeps_open_pages_and_survives_a_reload() {
    use crate::model::{AlbumRow, ArtistGroup, ArtistRow, Library};
    let artist = |name: &str| ArtistRow {
        name: name.into(),
        albums: 1,
        tracks: 1,
        arts: Vec::new(),
        album_ids: Vec::new(),
    };
    let group = |name: &str, id: i64| ArtistGroup {
        artist: name.into(),
        albums: vec![AlbumRow {
            name: format!("{name} LP"),
            artist: name.into(),
            album_id: id,
            ..Default::default()
        }],
    };
    // Built in no particular order, the way a shell hands a library over.
    let lib = || Library {
        artists: vec![
            artist("Tycho"),
            artist("The Beatles"),
            artist("Muse"),
            artist("Arcade Fire"),
        ],
        album_groups: vec![
            group("Tycho", 4),
            group("The Beatles", 3),
            group("Muse", 2),
            group("Arcade Fire", 1),
        ],
        ..Default::default()
    };
    let names = |a: &App| {
        a.lib
            .artists
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<_>>()
    };
    let groups = |a: &App| {
        a.lib
            .album_groups
            .iter()
            .map(|g| g.artist.clone())
            .collect::<Vec<_>>()
    };

    let mut a = App::unlocked();
    a.set_library(lib());
    assert_eq!(
        names(&a),
        ["Arcade Fire", "Muse", "The Beatles", "Tycho"],
        "off by default: as written"
    );
    assert_eq!(groups(&a), names(&a));
    a.artist_view = 1; // Muse's page is open underneath Settings
    a.album_view = 1; // and so is Muse LP (one album per group, so flat index = group index)

    a.set_ignore_the(true);
    assert_eq!(names(&a), ["Arcade Fire", "The Beatles", "Muse", "Tycho"]);
    assert_eq!(
        groups(&a),
        names(&a),
        "the Albums tab's groups follow the same order"
    );
    assert_eq!(
        a.lib.artists[a.artist_view].name, "Muse",
        "the open artist page keeps its artist"
    );
    assert_eq!(
        a.lib.albums_flat()[a.album_view].album_id,
        2,
        "the open album keeps its album"
    );

    a.set_library(lib());
    assert!(a.lib.ignore_the);
    assert_eq!(
        names(&a),
        ["Arcade Fire", "The Beatles", "Muse", "Tycho"],
        "a reload keeps the setting"
    );

    let _ = a.reset_settings();
    assert!(!a.ignore_the() && !a.lib.ignore_the);
    assert_eq!(
        names(&a),
        ["Arcade Fire", "Muse", "The Beatles", "Tycho"],
        "reset puts artists back as written"
    );
}

/// The rail's hit test must cover the whole list height and map monotonically onto the letters,
/// with the first and last letters actually reachable.
#[test]
fn az_rail_hit_test_covers_every_letter() {
    for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
        let mut seen: Vec<u8> = Vec::new();
        for y in 0..crate::canvas::H as i32 {
            if let Some(l) = library::az_letter_at(y, tab) {
                if !seen.contains(&l) {
                    seen.push(l);
                }
            }
        }
        assert_eq!(
            seen,
            library::AZ_LETTERS.to_vec(),
            "{tab:?}: rail letters unreachable"
        );
    }
    // And the rail only claims the right edge, so it can't swallow row taps.
    assert!(library::az_hit_x(crate::canvas::W as i32 - 1));
    assert!(!library::az_hit_x(
        crate::canvas::W as i32 - library::AZ_W - 1
    ));
}

/// The status strip's three zones must be distinct, and the Shelf target must be big enough for
/// a thumb — it is the only part of the strip that doesn't open the Menu, so a miss there lands
/// you on the wrong screen rather than doing nothing.
#[test]
fn status_strip_zones_are_distinct_and_the_shelf_target_is_thumb_sized() {
    use crate::chrome::{status_hit, StatusTap, STATUS_H};
    let mut shelf_w = 0;
    let mut np_w = 0;
    let mut menu_w = 0;
    let mut nothing_w = 0;
    for x in 0..crate::canvas::W as i32 {
        match status_hit(x, STATUS_H / 2) {
            Some(StatusTap::Shelf) => shelf_w += 1,
            Some(StatusTap::NowPlaying) => np_w += 1,
            Some(StatusTap::Menu) => menu_w += 1,
            Some(StatusTap::Nothing) => nothing_w += 1,
            None => panic!("x={x} inside the strip resolved to nothing"),
        }
    }
    assert!(
        shelf_w >= 56,
        "Shelf target only {shelf_w}px wide — too small for a thumb"
    );
    assert!(np_w >= 120, "Now Playing zone only {np_w}px wide");
    assert!(menu_w >= 44, "Menu target only {menu_w}px wide");
    assert!(
        nothing_w > 0,
        "#51: a miss along the strip must not open the Menu"
    );
    assert_eq!(shelf_w + np_w + menu_w + nothing_w, crate::canvas::W as i32);
    // Below the strip the whole thing stops claiming taps.
    assert_eq!(status_hit(10, STATUS_H), None);
}

/// One-tap return to Now Playing from anywhere, and it must COLLAPSE the stack rather than
/// burying Now Playing under whatever was being browsed.
#[test]
fn status_bar_returns_to_now_playing_from_any_screen() {
    let mut app = unlocked();
    app.push(Screen::Library);
    app.push(Screen::Settings);
    assert_eq!(app.tap(20, 20), vec![]);
    assert_eq!(app.current(), Screen::NowPlaying);
    assert_eq!(app.stack.len(), 1, "stack should have collapsed");
    // Back returns to the screen Now Playing was opened from (#51, 2026-11-03); it used to do
    // nothing there.
    let _ = app.press(Button::Back);
    assert_eq!(app.current(), Screen::Settings);
}

/// The Now Playing bar's left zone is a real play/pause button; the rest still opens the screen.
#[test]
fn np_bar_left_zone_is_play_pause_and_the_rest_navigates() {
    use crate::chrome::{np_bar_rect, NP_BAR_PLAY_W};
    let (_, by, w, h) = np_bar_rect();
    let midy = by + h / 2;

    let mut app = unlocked();
    app.push(Screen::Library);
    assert_eq!(app.tap(NP_BAR_PLAY_W / 2, midy), vec![Action::PlayPause]);
    assert_eq!(
        app.current(),
        Screen::Library,
        "play/pause must not navigate"
    );

    let mut app = unlocked();
    app.push(Screen::Library);
    assert_eq!(app.tap(w - 40, midy), vec![]);
    assert_eq!(app.current(), Screen::NowPlaying);
}

/// A fling must cover the same distance in the same wall-clock time whatever the frame rate.
/// It used to step `v/60` per call and decay a flat 0.92 per call, so at the ~32 fps a scrolling
/// frame actually costs on device a flick travelled roughly half as far AND died twice as fast
/// — while feeling right on the host, which runs far quicker.
#[test]
fn fling_distance_is_frame_rate_independent() {
    // Needs a list long enough to scroll: the sample library is 8 songs, so the fling would hit
    // the clamp on its first step and the test would measure nothing.
    let big = {
        let mut lib = crate::model::Library::sample();
        let base = lib.songs.clone();
        while lib.songs.len() < 400 {
            lib.songs.extend(base.iter().cloned());
        }
        lib
    };
    let travel = |dt: u32, steps: usize| {
        let mut app = unlocked();
        app.push(Screen::Library);
        app.set_library(big.clone());
        app.lib_tab = crate::library::Tab::Songs;
        app.fling(3000.0);
        for _ in 0..steps {
            app.tick_dt(dt);
        }
        app.lib_scroll_px
    };
    // Same 500 ms of wall clock, delivered as 30x17ms and as 16x31ms (the device's real
    // scrolling frame time). The two must land within a few percent of each other.
    let fast = travel(17, 30);
    let slow = travel(31, 16);
    assert!(fast > 100, "fling should actually travel ({fast}px)");
    let diff = (fast - slow).abs() as f32 / fast as f32;
    assert!(
        diff < 0.15,
        "frame-rate dependent: 17ms->{fast}px vs 31ms->{slow}px"
    );
}

/// HUD countdowns are written in 60 fps frames; they must still last the same wall-clock time
/// when frames are slower, or a toast outstays its welcome exactly when the device is busy.
#[test]
fn hud_countdown_duration_is_frame_rate_independent() {
    let mut fast = unlocked();
    let mut slow = unlocked();
    fast.toast_frames = TOAST_FRAMES;
    slow.toast_frames = TOAST_FRAMES;
    // ~500 ms each way.
    for _ in 0..30 {
        fast.tick_dt(17);
    }
    for _ in 0..16 {
        slow.tick_dt(31);
    }
    assert_eq!(
        fast.toast_frames == 0,
        slow.toast_frames == 0,
        "toast outlived its wall-clock duration at the slower frame rate"
    );
}

/// The visualiser must never animate without real spectrum data behind it. The synthetic
/// fallback moved identically for silence, a ballad and a drum solo — it looked like a
/// representation of the audio and wasn't one.
#[test]
fn visualiser_is_hidden_unless_real_spectrum_data_is_arriving() {
    let app = unlocked();
    assert!(app.viz_on(), "precondition: the user preference is on");

    // No levels => the NowPlaying view-model must come out with viz_on false.
    let live_flag = |levels: Option<&[f32]>| {
        // Mirrors the expression in render's NowPlaying arm.
        app.viz_on() && levels.is_some()
    };
    assert!(!live_flag(None), "no data must mean no visualiser");
    let bars = [0.5f32; 36];
    assert!(live_flag(Some(&bars)), "real data must show it");
}

#[test]
fn hold_switch_is_the_only_unlock() {
    // The Hold SWITCH locks/unlocks; the touchscreen and nav buttons never do.
    let mut a = App::unlocked();
    a.set_hold(true);
    assert!(a.locked);
    assert_eq!(a.current(), Screen::Lock);
    // While locked: a tap is ignored (touchscreen disabled) and stays on Lock.
    assert!(a.tap(200, 400).is_empty());
    assert!(a.locked);
    // Transport buttons still control playback WITHOUT unlocking.
    assert_eq!(a.press(Button::Right), vec![Action::Next]);
    assert!(a.locked);
    assert_eq!(a.press(Button::Play), vec![Action::PlayPause]);
    assert!(a.locked);
    // Power toggles the screen (emits Sleep) but does NOT unlock.
    assert_eq!(a.press(Button::Power), vec![Action::Sleep]);
    assert!(a.locked);
    // Only the switch going off unlocks.
    a.set_hold(false);
    assert!(!a.locked);
    assert_eq!(a.current(), Screen::NowPlaying);
}

#[test]
fn power_toggles_screen_without_locking() {
    let mut a = unlocked();
    let acts = a.press(Button::Power);
    assert_eq!(acts, vec![Action::Sleep]); // shell blanks/wakes the backlight
    assert!(!a.locked); // Power never locks — that's the Hold switch's job
    assert_ne!(a.current(), Screen::Lock);
}

#[test]
fn big_view_grows_rows_and_text_and_gives_back_a_size_it_set() {
    let _g = crate::text::scale_guard();
    let mut a = unlocked();
    a.push_for_test(Screen::Settings);
    let row = crate::settings::row_top_px(crate::settings::ROW_BIG) + crate::settings::LIST_TOP;
    assert_eq!(a.tap(200, row + 20), vec![Action::UiScaleChanged]);
    assert!(a.big_view());
    assert_eq!(crate::text::scale_pct(), BIG_TEXT_PCT);
    assert_eq!(crate::scale::TRACK_ROW_H(), 78);
    assert!(crate::text::touch_scale() >= 1.25);
    // A size picked on the slider while Big view is on is kept when it goes off.
    crate::text::set_scale_pct(140);
    a.set_big_view(false);
    assert_eq!(crate::text::scale_pct(), 140);
    assert_eq!(crate::scale::TRACK_ROW_H(), 62);
    // The one Big view set is given back.
    a.set_big_view(true);
    crate::text::set_scale_pct(BIG_TEXT_PCT);
    a.set_big_view(false);
    assert_eq!(crate::text::scale_pct(), 100);
}

/// Big view pushes Display's Style chips off the glass, so the page scrolls, and a chip tapped
/// after scrolling picks itself. Under the header nothing is tappable.
#[test]
fn big_view_display_scrolls_to_its_style_chips() {
    let _g = crate::text::scale_guard();
    let mut a = unlocked();
    a.push_for_test(Screen::Display);
    a.set_big_view(true);
    let max = crate::display::max_scroll_px();
    assert!(max > 0, "Display fits in Big view; drop its scroll");
    a.scroll_px(10_000);
    assert_eq!(a.display_scroll(), max);
    let (x, y) = crate::display::style_chip_centre(1);
    a.tap(x, y - max);
    assert_eq!(a.style, crate::style::Style::Nocturne);
    let before = a.display_sel;
    assert_eq!(a.tap(400, crate::chrome::HEADER_BOTTOM - 4), vec![]);
    assert_eq!(a.display_sel, before);
    a.set_big_view(false);
    assert_eq!(a.display_scroll(), 0, "nothing to scroll with Big view off");
}

/// In Big view the Settings home page is taller than the screen. Back from a lower topic's page,
/// by the Back key (the edge swipe) or the header arrow, lands on the home page scrolled to show
/// that topic's link. Seen on the player 11-03: Back showed the top with the cursor off the
/// bottom, and the arrow left Settings altogether.
#[test]
fn big_view_back_from_a_settings_page_shows_its_link() {
    use crate::settings::{group_row, page_rows, GROUPS, LIST_TOP, RH};
    let _g = crate::text::scale_guard();
    let mut a = open_from_menu(Screen::Settings);
    a.set_big_view(true);
    let lang = a.settings_lang();
    assert!(
        crate::settings::max_scroll_px(None, lang) > 0,
        "the home page fits in Big view; this test has nothing to check"
    );
    let last = GROUPS.len() - 1;
    let link_on_screen = |a: &App| {
        let rows = page_rows(None, lang);
        let i = rows.iter().position(|r| *r == a.settings_sel).unwrap() as i32;
        let top = LIST_TOP + i * RH() - a.settings_scroll_px;
        top >= LIST_TOP && top + RH() <= crate::canvas::H as i32
    };
    for by_arrow in [false, true] {
        a.settings_sel = group_row(last);
        a.press(Button::Select);
        assert_eq!(a.settings_page, Some(last));
        if by_arrow {
            a.tap(30, 60);
        } else {
            a.press(Button::Back);
        }
        assert_eq!(
            a.current(),
            Screen::Settings,
            "arrow={by_arrow}: left Settings"
        );
        assert_eq!(a.settings_page, None, "arrow={by_arrow}: still on the page");
        assert_eq!(a.settings_sel, group_row(last));
        assert!(
            link_on_screen(&a),
            "arrow={by_arrow}: the link is off the screen"
        );
    }
    // At normal size the home page fits: Back leaves it where it starts.
    a.set_big_view(false);
    a.settings_sel = group_row(last);
    a.press(Button::Select);
    a.press(Button::Back);
    assert_eq!(a.settings_scroll_px, 0);
    assert!(link_on_screen(&a));
}

/// Big view pushes Sound quality's last rows off the glass, so the page scrolls, and Fine volume
/// answers where it is drawn after scrolling. Nothing to scroll with Big view off.
#[test]
fn big_view_sound_quality_scrolls_to_fine_volume() {
    let _g = crate::text::scale_guard();
    let mut a = unlocked();
    a.set_bt_on(true);
    a.bt_codec = crate::bluetooth::LDAC;
    a.push_for_test(Screen::BtCodec);
    a.set_big_view(true);
    let max = crate::bluetooth::codec_max_scroll(true, false, false);
    assert!(max > 0, "Sound quality fits in Big view; drop its scroll");
    a.scroll_px(10_000);
    assert_eq!(a.bt_codec_scroll(), max);
    let span = a.bt_fine_span();
    a.tap(240, crate::bluetooth::fine_row_y(true) - max);
    assert_ne!(
        a.bt_fine_span(),
        span,
        "Fine volume did not answer after scrolling"
    );
    assert_eq!(a.tap(240, crate::chrome::HEADER_BOTTOM - 4), vec![]);
    a.set_big_view(false);
    // A row per codec scrolls with Big view off too, but not as far.
    assert_eq!(
        a.bt_codec_scroll(),
        crate::bluetooth::codec_max_scroll(true, false, false)
    );
    assert!(a.bt_codec_scroll() < max);
}

/// Big view: the Menu keeps full rows and scrolls to START ON; a chip tapped there picks
/// itself, and the strip stays the strip whatever the scroll.
#[test]
fn big_view_menu_scrolls_to_start_on() {
    let _g = crate::text::scale_guard();
    let mut a = unlocked();
    a.push_for_test(Screen::Menu);
    a.set_big_view(true);
    let rows = a.menu_len();
    let max = crate::menu::max_scroll_px(rows);
    assert!(max > 0);
    a.scroll_px(10_000);
    assert_eq!(a.menu_scroll(), max);
    let (x, w) = crate::kit::chip_span(1, 2);
    let y = crate::menu::label_top(rows) + crate::kit::SECTION_H + 5 - max;
    assert!(a.tap(x + w / 2, y).is_empty());
    assert_eq!(a.home_screen, 1, "Now Playing");
    a.tap(240, crate::menu::STRIP_TOP + 5);
    assert_eq!(a.current(), Screen::NowPlaying);
    a.set_big_view(false);
}

#[test]
fn power_wakes_the_screen_over_a_dialog_usb_storage_and_the_intro() {
    let mut a = unlocked();
    a.confirm = Some(crate::confirm::Ask::Restart);
    assert_eq!(a.press(Button::Power), vec![Action::Sleep]);
    assert!(a.confirm.is_some(), "Power must not dismiss the dialog");
    a.confirm = None;
    a.push(Screen::UsbStorage);
    assert_eq!(a.press(Button::Power), vec![Action::Sleep]);
    assert_eq!(a.current(), Screen::UsbStorage);
    a.push(Screen::Onboarding);
    assert_eq!(a.press(Button::Power), vec![Action::Sleep]);
}

#[test]
fn shelf_opens_pins_and_jumps_back() {
    let mut a = unlocked(); // Now Playing
                            // open the Shelf from the status-bar bookmark → overlay over the current place (Now Playing)
    a.tap(392, 16);
    assert!(a.shelf_is_open());
    assert_eq!(a.current(), Screen::NowPlaying);
    // Pin "this place" (the Pin button), then close and navigate away.
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby); // header Pin
    assert!(a.pins[0].is_some());
    a.tap(240, 200); // dim backdrop → Close
    assert!(!a.shelf_is_open());
    // Go to the Library, reopen the Shelf, and tap the pin's "GO ›" → back to Now Playing.
    a.tap(344, 22); // status bar, on the ☰ glyph (not the bookmark) → Menu
    a.tap(200, 91 + 63 + 8); // Library row
    assert_eq!(a.current(), Screen::Library);
    a.open_shelf();
    a.tap(380, crate::shelf::slot_center_y(0)); // slot 0 "GO ›" hit
    assert!(!a.shelf_is_open());
    assert_eq!(a.current(), Screen::NowPlaying);
}

/// Bluetooth ▸ THIS DEVICE ▸ Debug log: one action per switch, off at every boot, and the
/// shell can switch it off itself at its size limit.
#[test]
fn the_bluetooth_debug_log_is_a_switch_that_starts_off() {
    let mut a = App::unlocked();
    assert!(!a.bt_debug_log(), "never on at boot");
    a.set_bt_on(true);
    a.push(Screen::Bluetooth);
    let y = crate::bluetooth::debug_row_y();
    assert_eq!(a.tap(240, y), vec![Action::BtDebugLog]);
    assert!(a.bt_debug_log());
    assert_eq!(a.tap(240, y), vec![Action::BtDebugLog]);
    assert!(!a.bt_debug_log());
    a.tap(240, y);
    a.bt_debug_log_stopped();
    assert!(
        !a.bt_debug_log(),
        "the shell's size limit turns the switch off"
    );
}

// ── The pull-down panel (quick.rs, community request B1) ─────────────────────────────────
// OFF path first: with the setting off nothing may change — no claim on any contact.

/// With the panel switched off (the default) a drag from the status bar is not claimed, so the
/// shell hands it to the list exactly as before, and nothing opens.
#[test]
fn the_pull_down_panel_is_off_by_default_and_claims_nothing() {
    let mut a = App::unlocked();
    assert!(!a.opt(Opt::Quick), "off unless switched on");
    for y in [0, 10, 30, crate::chrome::STATUS_H - 1] {
        assert!(
            !a.quick_pull_begin(240, y),
            "y={y} claimed with the panel off"
        );
    }
    assert!(!a.quick_pull_open());
    assert!(!a.quick_is_open());
    // The list still scrolls: the guard that stops a scroll under an overlay is not tripped.
    let _scale = lock_scale();
    crate::scale::set_large(true); // the home page scrolls in Big view
    a.go(Screen::Settings);
    a.scroll_px(120);
    assert!(
        a.settings_scroll_px > 0,
        "a drag still scrolls Settings with the panel off"
    );
    crate::scale::set_large(false);
}

/// Switched on: a pull from the status bar opens it, a drag anywhere else is still the list's,
/// and while it is open the list underneath stays put.
#[test]
fn the_pull_down_panel_opens_from_the_status_bar_only() {
    let mut a = App::unlocked();
    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_QUICK;
    a.settings_activate();
    assert!(a.opt(Opt::Quick), "the Settings row switches it on");
    assert!(a.quick_pull_begin(240, 20));
    assert!(
        !a.quick_pull_begin(240, crate::chrome::STATUS_H),
        "below the status bar is the list's"
    );
    assert!(!a.quick_pull_begin(240, 400));
    assert!(a.quick_pull_open());
    assert!(a.quick_is_open());
    let before = a.settings_scroll_px;
    a.scroll_px(200);
    assert_eq!(
        a.settings_scroll_px, before,
        "the panel owns the gesture, like the Shelf"
    );
    assert!(
        !a.shelf_swipe_open(),
        "the Shelf does not open over the panel"
    );
    assert!(
        !a.quick_pull_begin(240, 20),
        "a second pull does not reopen an open panel"
    );
}

/// #51: the Bluetooth row's name opens the Bluetooth page, so Now Playing's Bluetooth button
/// is not the only way there.
#[test]
fn the_pull_down_bluetooth_name_opens_the_page() {
    let mut a = App::unlocked();
    a.set_opt(Opt::Quick, true);
    assert!(a.quick_pull_begin(240, 20));
    assert!(a.quick_pull_open());
    assert_eq!(a.tap(120, crate::quick::ROW_BT + 30), vec![]);
    assert!(!a.quick_is_open(), "the panel closes behind it");
    assert_eq!(a.current(), Screen::Bluetooth);
}

/// #51: the third toolbar button is Bluetooth unless Settings says otherwise, opens what it
/// shows, and every style draws the choice rather than the Bluetooth glyph.
#[test]
fn the_third_toolbar_button_is_the_one_settings_picked() {
    use crate::now_playing::NpButton;
    let _scale = lock_scale();
    let fonts = crate::text::FontSet::load();
    let slot = |a: &mut App| {
        a.go(Screen::NowPlaying);
        let (x, y) = crate::now_playing::layout(a.style, a.night, false)
            .at(crate::now_playing::Hit::Toolbar(2))
            .expect("a third toolbar slot")
            .centre();
        a.tap(x, y);
        a.current()
    };
    let mut a = unlocked();
    assert_eq!(a.np_button(), NpButton::Bluetooth, "Bluetooth by default");
    assert_eq!(slot(&mut a), Screen::Bluetooth);
    for (b, screen) in [
        (NpButton::Sound, Screen::Sound),
        (NpButton::Eq, Screen::Eq),
        (NpButton::Menu, Screen::Menu),
    ] {
        a.set_np_button(b);
        assert_eq!(slot(&mut a), screen, "{b:?}");
        assert_eq!(NpButton::from_token(b.token()), b);
    }
    assert_eq!(
        NpButton::Menu.next(),
        NpButton::Bluetooth,
        "the row comes round"
    );
    // What each style draws in the slot changes with the choice.
    for style in crate::style::Style::ALL {
        let toolbar = |b: NpButton| {
            let mut a = unlocked();
            a.set_style(style);
            a.set_np_button(b);
            a.go(Screen::NowPlaying);
            let mut c = Canvas::new();
            a.render(&mut c, &fonts, &bare_np());
            let rgb = c.to_rgb_bytes();
            (crate::now_playing::TOOLBAR_TOP as usize..800)
                .flat_map(|y| rgb[(y * 480 + 240) * 3..(y * 480 + 360) * 3].to_vec())
                .collect::<Vec<u8>>()
        };
        assert_ne!(
            toolbar(NpButton::Bluetooth),
            toolbar(NpButton::Sound),
            "{style:?}: the slot still says Bluetooth"
        );
    }
}

/// #51: a swipe on Now Playing opens what Settings picked, and nothing while it is Off — the
/// default, so no player changes unasked.
#[test]
fn now_playing_swipes_open_what_settings_picked() {
    use crate::settings::SwipeTo;
    let mut a = App::unlocked();
    a.go(Screen::NowPlaying);
    assert!(!a.np_swipe(-200) && !a.np_swipe(200), "off by default");
    assert_eq!(a.current(), Screen::NowPlaying);
    a.set_np_swipes(SwipeTo::Sound, SwipeTo::Menu);
    assert!(!a.np_swipe(-40), "too short to be a swipe");
    assert!(a.np_swipe(-200));
    assert_eq!(a.current(), Screen::Sound, "up");
    a.go(Screen::NowPlaying);
    assert!(a.np_swipe(200));
    assert_eq!(a.current(), Screen::Menu, "down");
    assert!(!a.np_swipe(200), "only on Now Playing");
    // The Settings rows step through every target and back to Off, and the words round-trip.
    for t in SwipeTo::ALL {
        assert_eq!(SwipeTo::from_token(t.token()), t);
    }
    let mut t = SwipeTo::Off;
    for _ in 0..SwipeTo::ALL.len() {
        t = t.next();
    }
    assert_eq!(t, SwipeTo::Off);
    assert_eq!(SwipeTo::from_token("nonsense"), SwipeTo::Off);
}

/// Every control is an action that already exists, one per tap.
#[test]
fn the_pull_down_panel_controls_are_existing_actions() {
    use crate::quick;
    let mut a = App::unlocked();
    a.set_opt(Opt::Quick, true);
    assert!(a.quick_pull_open());
    let chip = |i: usize, n: usize| {
        let (x, w) = crate::kit::chip_span(i, n);
        x + w / 2
    };
    let bright_y = quick::TOP + crate::kit::SECTION_H + crate::kit::CHIP_H / 2;
    assert_eq!(
        a.tap(chip(1, 5), bright_y),
        vec![Action::BrightnessChanged(2)]
    );
    assert_eq!(a.brightness(), 2);
    let bt = a.bt_on;
    let sw = crate::kit::RIGHT - crate::kit::SWITCH_W / 2;
    assert_eq!(a.tap(sw, quick::ROW_BT + 30), vec![Action::BtToggle(!bt)]);
    let night = a.night;
    assert_eq!(
        a.tap(240, quick::ROW_NIGHT() + 30),
        vec![Action::ThemeChanged(!night)]
    );
    let sleep_y = quick::BOTTOM() - 26 - crate::kit::CHIP_H / 2;
    assert_eq!(a.tap(chip(2, 6), sleep_y), vec![Action::SleepTimer(30)]);
    // The sixth chip is "Song": stop after this one. It cancels any countdown.
    assert_eq!(a.tap(chip(5, 6), sleep_y), vec![Action::SleepTimer(0)]);
    assert!(a.stop_after());
    assert!(a.quick_is_open(), "controls leave the panel open");
    assert_eq!(a.tap(240, quick::BOTTOM() + 100), Vec::<Action>::new());
    assert!(!a.quick_is_open(), "a tap on the screen behind closes it");
}

/// Back closes it, the transport keys still work over it, and it never opens over the lock,
/// the Shelf or a confirmation.
#[test]
fn the_pull_down_panel_is_modal_like_the_shelf() {
    let mut a = App::unlocked();
    a.set_opt(Opt::Quick, true);
    a.quick_pull_open();
    assert_eq!(a.press(Button::Next), vec![Action::Next]);
    assert!(a.quick_is_open());
    a.press(Button::Back);
    assert!(!a.quick_is_open());

    a.open_shelf();
    assert!(
        !a.quick_pull_begin(240, 20) && !a.quick_pull_open(),
        "not over the Shelf"
    );
    a.shelf_open = false;
    a.locked = true;
    assert!(!a.quick_pull_begin(240, 20), "not on the lock screen");
    a.locked = false;
    a.set_opt(Opt::Quick, false);
    assert!(
        !a.quick_pull_begin(240, 20),
        "switching it off takes the gesture away again"
    );
}

#[test]
fn status_bar_bookmark_opens_shelf() {
    let mut a = unlocked(); // Now Playing
    a.tap(392, 16); // the bookmark glyph (top-right) → Shelf overlay
    assert!(a.shelf_is_open());
    assert_eq!(a.current(), Screen::NowPlaying); // overlays the current place
                                                 // tapping elsewhere on the bar still opens the Menu (after closing the shelf)
    a.tap(240, 200); // backdrop → close
    assert!(!a.shelf_is_open());
    // Mid-strip is a miss now (#51), and only the ☰ opens the Menu.
    a.tap(240, 16);
    assert_eq!(a.current(), Screen::NowPlaying);
    a.tap(338, 16);
    assert_eq!(a.current(), Screen::Menu);
}

/// The status strip's two zones must land where the glyphs are DRAWN. Both come from
/// `chrome::status_hit`, so this pins the agreement rather than a pair of literals.
#[test]
fn status_strip_targets_match_the_drawn_glyphs() {
    use crate::chrome::{status_hit, StatusTap, STATUS_DEAD_H, STATUS_H};
    // The bookmark is drawn at x=388, centre of the strip.
    assert_eq!(status_hit(388, STATUS_H / 2), Some(StatusTap::Shelf));
    // The ☰ is drawn at x=344 — outside the Shelf zone, so tapping it means Menu.
    assert_eq!(status_hit(344, STATUS_H / 2), Some(StatusTap::Menu));
    // Live down to the dead band, and NOT through it. The strip is hit-tested before any
    // screen, so its bottom edge used to steal taps aimed at the back chevron and the
    // Bluetooth switch directly beneath it — see STATUS_DEAD_H.
    assert_eq!(
        status_hit(240, STATUS_H - STATUS_DEAD_H - 1),
        Some(StatusTap::Nothing)
    );
    for y in STATUS_H - STATUS_DEAD_H..STATUS_H {
        assert_eq!(status_hit(240, y), None, "the strip still claims y={y}");
    }
    assert_eq!(status_hit(240, STATUS_H), None); // below the strip: not ours
                                                 // Every x along the strip resolves to something — no dead columns.
    for x in 0..crate::W as i32 {
        assert!(status_hit(x, 10).is_some(), "dead column at x={x}");
    }
}

/// One tap from anywhere in the library back to Now Playing, however deep the drill-in went.
#[test]
fn np_bar_returns_to_now_playing() {
    let mut a = unlocked();
    a.tap(344, 22); // Menu
    a.tap(200, 91 + 63 + 8); // Library
    assert_eq!(a.current(), Screen::Library);
    let (_, by, _, _) = crate::chrome::np_bar_rect();
    a.tap(240, by + 20);
    assert_eq!(a.current(), Screen::NowPlaying);
    // …and it collapses the stack rather than popping one screen, so Back from Now Playing
    // doesn't walk back into the library.
    assert_eq!(a.stack.len(), 1);
}

/// The bar is pinned over the bottom of the list area, so no list row may resolve under it.
#[test]
fn np_bar_does_not_overlap_the_library_list() {
    let (_, by, _, _) = crate::chrome::np_bar_rect();
    let m = crate::model::Library::sample();
    for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
        assert_eq!(
            library::hit_row(tab, &m, 0, by),
            None,
            "{tab:?} row under the bar"
        );
        assert_eq!(
            library::hit_row(tab, &m, 0, by + 30),
            None,
            "{tab:?} row under the bar"
        );
    }
}

/// THE RULE, not the instance: a screen whose list stops at `library::list_bottom()` has
/// reserved the Now Playing bar's 64 px, so it must draw the bar.
///
/// Up Next was reported on 2026-09-05 as having "a black bar under the queue" and was fixed by
/// adding it to `shows_np_bar`. Folders — which reserves exactly the same space, through the
/// same constant, one file away — was not, and still had the black bar a month later. Fixing
/// the reported screen instead of the rule is how that happens, so the rule is the test now.
#[test]
fn every_screen_that_reserves_the_np_bar_draws_it() {
    // Screens whose list is clipped at `library::LIST_BOTTOM`, i.e. that give up the bottom
    // 64 px on the assumption the bar is there. Adding a screen to this list without adding it
    // to `shows_np_bar` is the defect.
    let reserves = [
        Screen::Library,
        Screen::Album,
        Screen::Artist,
        Screen::Playlist,
        Screen::UpNext,
        Screen::Folders,
        Screen::SensMe,
        Screen::Books,
    ];
    for s in reserves {
        assert!(
            App::shows_np_bar(s),
            "{s:?} reserves the Now Playing bar's space and must draw the bar",
        );
    }
    // And the bar is live there: play/pause on the left, return on the right, from each one.
    let (_, by, _, _) = crate::chrome::np_bar_rect();
    let midy = by + crate::chrome::NP_BAR_H / 2;
    for s in reserves {
        let mut a = unlocked();
        a.go_for_preview(s);
        assert_eq!(
            a.tap(crate::chrome::NP_BAR_PLAY_W / 2, midy),
            vec![Action::PlayPause],
            "{s:?}: the bar's left zone must be play/pause"
        );
        assert_eq!(a.current(), s, "{s:?}: play/pause must not navigate away");
        a.tap(300, midy);
        assert_eq!(
            a.current(),
            Screen::NowPlaying,
            "{s:?}: the bar must return home"
        );
    }
}

/// A drawn control must answer a finger. `chrome::header` draws a back chevron unconditionally,
/// so every screen that calls it has one on the glass — and the Menu was excluded from the tap,
/// leaving a chevron that did nothing on a device whose only input is a finger.
#[test]
fn every_screen_that_draws_a_back_chevron_answers_a_tap_on_it() {
    // The chevron's own band: below the status strip's dead edge, above the header rule.
    let y = (crate::chrome::STATUS_H + crate::chrome::HEADER_BOTTOM) / 2;
    // Every screen that reaches `chrome::header` — i.e. everything but the two roots and the
    // two full-panel screens that draw their own world.
    let with_header = [
        Screen::Menu,
        Screen::Library,
        Screen::UpNext,
        Screen::Eq,
        Screen::Sound,
        Screen::Soundscape,
        Screen::Advanced,
        Screen::Tone,
        Screen::DacEq,
        Screen::VolCurve,
        Screen::Presets,
        Screen::Nc,
        Screen::Skins,
        Screen::Bluetooth,
        Screen::BtCodec,
        Screen::Pairing,
        Screen::Settings,
        Screen::Device,
        Screen::Fm,
        Screen::UsbDac,
        Screen::Receiver,
        Screen::VizSet,
        Screen::ClockSet,
        Screen::Wifi,
        Screen::WifiNetworks,
        Screen::NetRadio,
        Screen::WifiSpeakers,
        Screen::MediaServers,
        Screen::RadioFind,
        Screen::WifiRemote,
        Screen::WifiOptions,
        Screen::GenreFilter,
        Screen::Folders,
        Screen::SensMe,
        Screen::Books,
        Screen::TrackInfo,
        Screen::Lyrics,
        Screen::Search,
        Screen::PlaylistPick,
        Screen::TrackPick,
        Screen::PlaylistEdit,
        Screen::PlaylistCover,
        Screen::ViewEdit,
    ];
    for s in with_header {
        let mut a = unlocked();
        a.push_for_test(s);
        assert_eq!(a.current(), s);
        a.tap(30, y);
        assert_ne!(
            a.current(),
            s,
            "{s:?}: the back chevron it draws did nothing"
        );
    }
    // Now Playing and Lock draw no header, so no invisible Back is claimed there. Now Playing
    // gives the same band to the Menu — the same thing the status strip above it means — which
    // is a deliberate use of the space rather than a stray target, and is pinned here so it
    // stays deliberate.
    let mut np = unlocked();
    assert_eq!(np.current(), Screen::NowPlaying);
    np.tap(30, y);
    assert_eq!(
        np.current(),
        Screen::Menu,
        "the header band on Now Playing opens the Menu"
    );
    // Locked, nothing in that band does anything at all.
    let mut lock = App::new();
    assert_eq!(lock.current(), Screen::Lock);
    lock.tap(30, y);
    assert_eq!(
        lock.current(),
        Screen::Lock,
        "a locked device answers no taps"
    );
}

#[test]
fn np_bar_is_live_on_up_next() {
    let (_, by, _, _) = crate::chrome::np_bar_rect();
    let midy = by + crate::chrome::NP_BAR_H / 2;

    let mut a = unlocked();
    a.go_for_preview(Screen::UpNext);
    assert!(
        App::shows_np_bar(Screen::UpNext),
        "Up Next must draw the bar it reserves space for"
    );

    // Left zone toggles playback without leaving the queue…
    assert_eq!(
        a.tap(crate::chrome::NP_BAR_PLAY_W / 2, midy),
        vec![Action::PlayPause]
    );
    assert_eq!(
        a.current(),
        Screen::UpNext,
        "play/pause must not navigate away"
    );

    // …and the rest returns to Now Playing, as it does from every other browsing screen.
    a.tap(240, midy);
    assert_eq!(a.current(), Screen::NowPlaying);
}

#[test]
fn now_playing_transport_emits_actions() {
    let mut a = unlocked();
    assert_eq!(a.press(Button::Right), vec![Action::Next]);
    assert_eq!(a.press(Button::Left), vec![Action::Prev]);
    let before = a.playing;
    assert_eq!(a.press(Button::Play), vec![Action::PlayPause]);
    assert_eq!(a.playing, !before);
}

#[test]
fn menu_navigation_and_routing() {
    let mut a = unlocked();
    a.press(Button::Up); // NowPlaying -> Menu
    assert_eq!(a.current(), Screen::Menu);
    // "Library" is the first row (index 0) since Now Playing became the strip; select it.
    assert_eq!(a.menu_index(), 0);
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::Library);
    // Back returns to Menu, Back again to NowPlaying
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Menu);
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::NowPlaying);
}

// ── Audiobooks ─────────────────────────────────────────────────────────────────────────────

fn books_lib(n: usize) -> Library {
    Library {
        books: (0..n)
            .map(|i| crate::model::BookRow {
                title: format!("Book {i}"),
                dir: format!("/contents/LEARNING/Author/Book {i}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn bare_np() -> NowPlaying<'static> {
    NowPlaying {
        skin: None,
        tape: None,
        title: "t",
        artist: "a",
        codec: "",
        badge: "",
        clock: "21:07",
        battery: 50,
        elapsed: "",
        remaining: "",
        progress: 0.0,
        art: "x",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: false,
        shuffle: false,
        repeat: 0,
        viz_seed: 0.0,
        viz_kind: 0,
        viz_size: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        page: 0,
        scrubbing: false,
        lyrics: false,
    }
}

/// With audiobooks the Library header carries a switch spelling "Music"; a tap opens the Books
/// list, where it spells "Books" and a tap goes back to the Library, on the tab it was left on.
#[test]
fn the_music_books_switch_moves_between_the_library_and_the_audiobooks() {
    for search in [false, true] {
        let mut a = unlocked();
        a.search_enabled = search;
        a.set_library(books_lib(3));
        a.push_for_test(Screen::Library);
        a.lib_tab = Tab::Artists;
        let (x0, x1) = crate::chrome::mode_zone();
        assert_eq!(
            a.tap((x0 + x1) / 2, 62),
            vec![],
            "opening a list plays nothing"
        );
        assert_eq!(a.current(), Screen::Books, "search {search}");
        a.tap((x0 + x1) / 2, 62);
        assert_eq!(a.current(), Screen::Library);
        assert_eq!(a.lib_tab, Tab::Artists, "the tab is where it was");
        a.press(Button::Back);
        assert_eq!(
            a.current(),
            Screen::NowPlaying,
            "the switch left no history"
        );
    }
}

/// The Library button on Now Playing returns to whichever of Music and Books you last browsed.
#[test]
fn the_now_playing_library_button_opens_the_last_browsed_side() {
    let tap_library_button = |a: &mut App| {
        a.tap(
            crate::now_playing::TOOLBAR_CX[0],
            crate::now_playing::TOOLBAR_TOP + 20,
        );
    };
    let mut a = unlocked();
    a.set_library(books_lib(3));
    tap_library_button(&mut a);
    assert_eq!(a.current(), Screen::Library, "music is the default");
    let (x0, x1) = crate::chrome::mode_zone();
    a.tap((x0 + x1) / 2, 62);
    assert_eq!(a.current(), Screen::Books);
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::NowPlaying);
    tap_library_button(&mut a);
    assert_eq!(a.current(), Screen::Books, "back where the books were");
    a.tap((x0 + x1) / 2, 62);
    assert_eq!(a.current(), Screen::Library);
    a.press(Button::Back);
    tap_library_button(&mut a);
    assert_eq!(a.current(), Screen::Library, "and back to music again");
}

/// Without audiobooks there is no switch: those taps are the title's and must open nothing.
#[test]
fn without_audiobooks_the_library_header_has_no_switch() {
    let mut a = unlocked();
    a.set_library(books_lib(0));
    a.push_for_test(Screen::Library);
    let (x0, x1) = crate::chrome::mode_zone();
    a.tap((x0 + x1) / 2, 62);
    assert_eq!(a.current(), Screen::Library);
}

/// A pinned Audiobooks list has Library under it no longer, so Music on it still lands on the
/// Library.
#[test]
fn music_on_a_restored_audiobooks_pin_lands_on_the_library() {
    let mut a = unlocked();
    a.set_library(books_lib(2));
    a.push_for_test(Screen::Books);
    let (m0, m1) = crate::chrome::mode_zone();
    a.tap((m0 + m1) / 2, 62);
    assert_eq!(a.current(), Screen::Library);
}

/// #51: each of Big view's extras grows what it names and nothing else, and neither does
/// anything without Big view.
#[test]
fn big_views_extras_grow_what_they_name() {
    let _scale = lock_scale();
    let fonts = crate::text::FontSet::load();
    // Pixels that are not background in a box of `screen`.
    let ink =
        |screen: Screen, large: bool, bar: bool, icons: bool, b: (usize, usize, usize, usize)| {
            crate::scale::set_large(large);
            crate::scale::set_big_bar(bar);
            crate::scale::set_big_icons(icons);
            let mut a = unlocked();
            a.go_for_preview(screen);
            a.settings_sel = crate::settings::ROW_SLEEP;
            let mut c = Canvas::new();
            a.render(&mut c, &fonts, &bare_np());
            let rgb = c.to_rgb_bytes();
            let bg = &rgb[(60 * 480 + 300) * 3..(60 * 480 + 300) * 3 + 3].to_vec();
            let n = (b.2..b.3)
                .flat_map(|y| (b.0..b.1).map(move |x| (x, y)))
                .filter(|&(x, y)| rgb[(y * 480 + x) * 3..(y * 480 + x) * 3 + 3] != bg[..])
                .count();
            crate::scale::set_large(false);
            crate::scale::set_big_bar(false);
            crate::scale::set_big_icons(false);
            n
        };
    let battery = (420, 480, 0, 38);
    // The Display row's arrow; the cursor is moved off it so its highlight does not count.
    let display_chevron = (440, 480, 91, 91 + 64);
    let toolbar = (240, 360, crate::now_playing::TOOLBAR_TOP as usize + 1, 800);
    let s = Screen::Settings;
    assert!(ink(s, true, true, false, battery) > ink(s, true, false, false, battery));
    assert_eq!(
        ink(s, false, true, true, battery),
        ink(s, false, false, false, battery),
        "nothing without Big view"
    );
    assert!(
        ink(s, true, false, true, display_chevron) > ink(s, true, false, false, display_chevron),
        "the row arrow grows"
    );
    assert_eq!(
        ink(s, true, false, true, battery),
        ink(s, true, false, false, battery),
        "Bigger icons leaves the top bar to its own switch"
    );
    let np = Screen::NowPlaying;
    assert_eq!(
        ink(np, true, false, true, toolbar),
        ink(np, true, false, false, toolbar),
        "Now Playing sizes its own icons"
    );
}

/// Only the ☰ opens the Menu now (#51), so it has to be on the glass wherever the top bar is:
/// every screen but onboarding and the sideways Winamp and cassette views.
#[test]
fn every_screen_but_onboarding_draws_the_menu_glyph() {
    let _scale = lock_scale();
    let fonts = crate::text::FontSet::load();
    let glyph_ink = |screen: Screen| {
        let mut a = unlocked();
        a.go_for_preview(screen);
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &bare_np());
        let rgb = c.to_rgb_bytes();
        let px = |x: usize, y: usize| {
            let i = (y * 480 + x) * 3;
            [rgb[i] as i32, rgb[i + 1] as i32, rgb[i + 2] as i32]
        };
        let bg = px(310, 22);
        (12..33)
            .flat_map(|y| (328..349).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let p = px(x, y);
                (0..3).map(|i| (p[i] - bg[i]).abs()).sum::<i32>() > 60
            })
            .count()
    };
    use Screen::*;
    for s in [
        Lock,
        NowPlaying,
        Menu,
        Library,
        Album,
        Artist,
        Playlist,
        UpNext,
        Zappin,
        Eq,
        Sound,
        Soundscape,
        Advanced,
        Tone,
        DacEq,
        VolCurve,
        Presets,
        Nc,
        Bluetooth,
        Pairing,
        BtCodec,
        Settings,
        Display,
        Palette,
        Language,
        Skins,
        Device,
        Storage,
        Fm,
        UsbDac,
        Receiver,
        Help,
        UsbStorage,
        VizSet,
        ClockSet,
        Alarm,
        Wifi,
        WifiNetworks,
        NetRadio,
        WifiSpeakers,
        MediaServers,
        RadioFind,
        WifiRemote,
        WifiOptions,
        GenreFilter,
        Folders,
        SensMe,
        Books,
        TrackInfo,
        Lyrics,
        Search,
        Keyboard,
        PlaylistPick,
        TrackPick,
    ] {
        assert!(glyph_ink(s) >= 20, "{s:?}: no ☰ on the top bar");
    }
    assert!(
        glyph_ink(Onboarding) < 20,
        "onboarding owns the whole panel"
    );
}

/// Both screens draw the switch, each spelling its own word.
#[test]
fn both_screens_draw_the_switch_with_their_own_word() {
    let _scale = lock_scale();
    let fonts = crate::text::FontSet::load();
    let band = |screen: Screen, books: usize| {
        let mut a = unlocked();
        a.set_library(books_lib(books));
        a.go_for_preview(screen);
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &bare_np());
        let rgb = c.to_rgb_bytes();
        let (x0, x1) = crate::chrome::mode_zone();
        (46..90usize)
            .flat_map(|y| rgb[(y * 480 + x0 as usize) * 3..(y * 480 + x1 as usize) * 3].to_vec())
            .collect::<Vec<u8>>()
    };
    let lib = band(Screen::Library, 3);
    let books = band(Screen::Books, 3);
    assert_ne!(lib, band(Screen::Library, 0), "the Library gains a switch");
    assert_ne!(lib, books, "the word differs");
}

/// A tap on a row asks the shell to play THAT book.
#[test]
fn tapping_a_book_plays_that_book() {
    let mut a = unlocked();
    a.set_library(books_lib(4));
    a.push_for_test(Screen::Books);
    for i in 0..4 {
        let y = crate::audiobooks::row_top(&a.lib, i, 0) + crate::audiobooks::ROW_H() / 2;
        assert_eq!(a.tap(240, y), vec![Action::PlayBook(i)], "row {i}");
    }
    assert_eq!(
        a.tap(
            240,
            crate::audiobooks::row_top(&a.lib, 3, 0) + crate::audiobooks::ROW_H() + 5
        ),
        vec![]
    );
}

/// A scrolled list plays the row that is under the finger, not the one at the same place
/// unscrolled.
#[test]
fn a_scrolled_audiobook_list_plays_the_row_under_the_finger() {
    let mut a = unlocked();
    a.set_library(books_lib(30));
    a.push_for_test(Screen::Books);
    a.scroll_px(crate::audiobooks::ROW_H() * 5);
    let y = crate::audiobooks::TOP + crate::audiobooks::ROW_H() / 2;
    assert_eq!(a.tap(240, y), vec![Action::PlayBook(5)]);
}

// ── SensMe channels ────────────────────────────────────────────────────────────────────────

/// A library with two channels over three songs, plus one unanalysed song.
fn sensme_lib() -> Library {
    let song = |t: &str, id: i64, bits: u16| crate::model::SongRow {
        title: t.into(),
        artist: "Someone".into(),
        dur: "3:00".into(),
        object_id: id,
        sensme: bits,
        ..Default::default()
    };
    let mut lib = Library {
        songs: vec![
            song("one", 101, 1 | 1 << 1),            // channel 0
            song("two", 102, 1 | 1 << 9),            // channel 8
            song("three", 103, 1 | 1 << 1 | 1 << 9), // both
            song("untagged", 104, 0),
        ],
        ..Default::default()
    };
    lib.build_channels(&[
        "Active",
        "Emotional",
        "Lounge",
        "Dance",
        "Extreme",
        "Upbeat",
        "Relax",
        "Mellow",
        "Morning",
    ]);
    lib
}

fn at_sensme() -> App {
    let mut a = unlocked();
    a.set_library(sensme_lib());
    a.set_sensme_enabled(true);
    a.open_sensme();
    a
}

/// The channels are built from the per-track bitmasks, empty ones are dropped, and a track in
/// two channels is counted once in "analysed".
#[test]
fn sensme_channels_are_built_from_the_bitmasks() {
    let lib = sensme_lib();
    assert_eq!(lib.channels.len(), 2, "only the two channels with members");
    assert_eq!((lib.channels[0].id, lib.channels[0].name), (0, "Active"));
    assert_eq!(lib.channels[0].tracks, vec![0, 2]);
    assert_eq!((lib.channels[1].id, lib.channels[1].name), (8, "Morning"));
    assert_eq!(lib.channels[1].tracks, vec![1, 2]);
    assert_eq!(
        lib.sensme_tracks, 3,
        "the track in two channels is one analysed track"
    );
    assert_eq!(lib.sensme_all(), vec![0, 1, 2]);
}

/// The Menu row is the component's switch. Without it installed the row is not drawn at all —
/// and the rows below it must not shift under the cursor, which is why everything indexes
/// `menu_visible` rather than `MENU`.
#[test]
fn the_sensme_menu_row_appears_only_when_the_component_is_installed() {
    let mut a = unlocked();
    a.set_preview_enabled(true); // the other opt-in row, so the counts below are SensMe's alone
                                 // (Wi-Fi's row is the third, and tests run without the Wi-Fi option: hence the extra 1.)
    assert!(!a.menu_visible().iter().any(|m| m.0 == Screen::SensMe));
    assert_eq!(a.menu_len(), MENU.len() - 2);
    a.set_sensme_enabled(true);
    assert!(a.menu_visible().iter().any(|m| m.0 == Screen::SensMe));
    assert_eq!(a.menu_len(), MENU.len() - 1);
    // …and it opens the screen, from the Menu, the way every other row does.
    a.press(Button::Up);
    let idx = a
        .menu_visible()
        .iter()
        .position(|m| m.0 == Screen::SensMe)
        .unwrap();
    for _ in 0..idx {
        a.press(Button::Down);
    }
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::SensMe);
    assert_eq!(a.sensme_channel, None, "it opens on the channel LIST");
}

/// Tapping a channel opens it; Back comes out to the list, not out of SensMe, and puts the
/// list back where it was.
#[test]
fn a_channel_opens_and_back_returns_to_the_list() {
    let mut a = at_sensme();
    a.sensme_scroll_px = 0;
    // The list is a grid (handoff 5d): the first tile is the first channel.
    let (x, y, w, h) = crate::sensme::tile_rect(0, 0);
    assert_eq!(a.tap(x + w / 2, y + h / 2), vec![]);
    assert_eq!(a.sensme_channel, Some(0));
    assert_eq!(a.current(), Screen::SensMe);
    a.press(Button::Back);
    assert_eq!(
        a.sensme_channel, None,
        "Back is up one level, not out of the screen"
    );
    assert_eq!(a.current(), Screen::SensMe);
    a.press(Button::Back);
    assert_ne!(
        a.current(),
        Screen::SensMe,
        "and from the list, Back leaves"
    );
}

/// A track inside a channel plays THE CHANNEL from that row. `PlayIndex` would resolve the
/// track to its album and lose the channel after one song — the defect `PlayPlaylistAt` was
/// added for, and the reason this action carries the context.
#[test]
fn a_track_in_a_channel_plays_the_channel_from_there() {
    let mut a = at_sensme();
    a.sensme_enter(1); // Morning: tracks 1 and 2
    let y = crate::sensme::row_top(1, a.sensme_channel, 0) + crate::sensme::ROW_H() / 2;
    assert_eq!(
        a.tap(240, y),
        vec![Action::PlaySensMe {
            chan: 8,
            from: 1,
            shuffle: false
        }],
    );
    assert!(!a
        .tap(240, y)
        .iter()
        .any(|x| matches!(x, Action::PlayIndex(_))));
}

/// The band's two halves are two verbs on one channel.
#[test]
fn the_channel_band_plays_and_shuffles_that_channel() {
    let mut a = at_sensme();
    a.sensme_enter(0); // Active, channel id 0
    let (px, py, pw, ph) = crate::sensme::play_band();
    let (sx, _, sw, _) = crate::sensme::shuffle_band();
    assert_eq!(
        a.tap(px + pw / 2, py + ph / 2),
        vec![Action::PlaySensMe {
            chan: 0,
            from: 0,
            shuffle: false
        }],
    );
    assert_eq!(
        a.tap(sx + sw / 2, py + ph / 2),
        vec![Action::PlaySensMe {
            chan: 0,
            from: 0,
            shuffle: true
        }],
    );
}

/// The grid's button shuffles every analysed track — until "Follow the time of day" is on, when
/// it plays the channel for the hour on the clock.
#[test]
fn the_grid_button_shuffles_all_or_follows_the_clock() {
    let mut a = at_sensme(); // channels: Active (0), Morning (8)
    let button = crate::sensme::BUTTON_Y() + crate::kit::BUTTON_H / 2;
    assert_eq!(
        a.tap(240, button),
        vec![Action::PlaySensMe {
            chan: crate::sensme::ALL,
            from: 0,
            shuffle: true
        }],
    );
    a.tap(240, crate::sensme::FOLLOW_Y() + crate::kit::ROW_H() / 2);
    assert!(a.sensme_follow(), "the switch row turns following on");
    a.clock_hour = Some(8);
    assert_eq!(
        a.tap(240, button),
        vec![Action::PlaySensMe {
            chan: 8,
            from: 0,
            shuffle: false
        }],
        "08:00 plays Morning"
    );
    a.clock_hour = Some(21);
    assert_eq!(
        a.tap(240, button),
        vec![Action::PlaySensMe {
            chan: crate::sensme::ALL,
            from: 0,
            shuffle: true
        }],
        "Night has no tracks here, so it falls back to everything",
    );
}

/// The clock the status bar draws is where the hour comes from.
#[test]
fn the_hour_comes_from_the_status_bar_clock() {
    let mut a = App::unlocked();
    let mut c = Canvas::new();
    let fonts = crate::text::FontSet::load();
    let np = NowPlaying {
        skin: None,
        tape: None,
        title: "t",
        artist: "a",
        codec: "",
        badge: "",
        clock: "21:07",
        battery: 50,
        elapsed: "",
        remaining: "",
        progress: 0.0,
        art: "x",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: false,
        shuffle: false,
        repeat: 0,
        viz_seed: 0.0,
        viz_kind: 0,
        viz_size: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        page: 0,
        scrubbing: false,
        lyrics: false,
    };
    a.render(&mut c, &fonts, &np);
    assert_eq!(a.clock_hour, Some(21));
    let np = NowPlaying { clock: "", ..np };
    a.render(&mut c, &fonts, &np);
    assert_eq!(
        a.clock_hour,
        Some(21),
        "a frame without a clock keeps the last hour"
    );
}

/// An untagged library: the row says so, the screen has no rows, and a tap on it does nothing
/// rather than playing something arbitrary.
#[test]
fn a_library_with_no_analysis_says_so_and_plays_nothing() {
    let mut a = unlocked();
    a.set_library(Library::default());
    a.set_sensme_enabled(true);
    assert_eq!(a.menu_subtitles().sensme, "Not analysed");
    a.open_sensme();
    assert!(crate::sensme::rows(&a.lib, None).is_empty());
    assert_eq!(a.tap(240, crate::sensme::TOP + 20), vec![]);
    assert_eq!(a.tap(240, crate::sensme::TOP + 200), vec![]);
}

/// The subtitle counts what is there. Two channels over three analysed tracks, with a fourth
/// track untagged.
#[test]
fn the_sensme_menu_subtitle_counts_channels_and_analysed_tracks() {
    let mut a = unlocked();
    a.set_library(sensme_lib());
    assert_eq!(a.menu_subtitles().sensme, "2 channels · 3 tracks");
}

/// A library reload while a channel is open closes it: the open channel is an INDEX into the
/// library that was just replaced.
#[test]
fn reloading_the_library_closes_the_open_channel() {
    let mut a = at_sensme();
    a.sensme_enter(1);
    a.sensme_scroll_px = 120;
    assert_eq!(a.sensme_channel, Some(1));
    a.set_library(sensme_lib());
    assert_eq!(a.sensme_channel, None);
    assert_eq!(a.sensme_scroll_px, 0);
}

#[test]
fn menu_cursor_clamps() {
    let mut a = unlocked();
    a.press(Button::Up); // -> Menu
    for _ in 0..3 {
        a.press(Button::Up);
    }
    assert_eq!(a.menu_index(), 0); // never below 0
    for _ in 0..50 {
        a.press(Button::Down);
    }
    // The VISIBLE rows, not `MENU`: SensMe is in the table but only drawn when its component
    // is installed, and the cursor indexes what is drawn.
    assert_eq!(a.menu_index(), a.menu_len() - 1); // never past end
}

#[test]
fn library_tabs_and_cursor() {
    let mut a = unlocked();
    a.press(Button::Up); // Menu — the cursor starts on Library, the first row
    a.press(Button::Select); // enter Library
    assert_eq!(a.current(), Screen::Library);
    let start = a.lib_tab();
    // Left from the default Albums tab lands on Songs (only Songs rows emit PlayIndex for a
    // single track; Artists navigate, Playlists emit PlayPlaylist).
    a.press(Button::Left);
    assert_ne!(a.lib_tab(), start); // tab changed
    assert_eq!(a.lib_tab(), Tab::Songs);
    assert_eq!(a.lib_index(), 0); // cursor reset on tab change
    a.press(Button::Down);
    assert!(a.lib_index() <= 1);
    // Select on a Songs row plays the SONGS LIST from that row, in the order drawn.
    let row = a.lib_index();
    let acts = a.press(Button::Select);
    assert_eq!(acts, vec![Action::PlayListAt(row)]);
    assert_eq!(a.take_play_list(), a.songs_list_ids());
}

/// Tapping a preset pill selects THAT preset. Regression: the pills are drawn per-pill but
/// were hit-tested as five uniform 86px slots starting at x=22, so tapping "A2" applied
/// "JAZZ", and taps on the blank space past the last pill still changed the sound.
#[test]
fn eq_preset_tap_selects_the_pill_under_the_finger() {
    for i in 0..data::EQ_PRESETS.len() {
        let mut a = unlocked();
        a.push(Screen::Eq);
        let (px, py, pw, ph) = crate::eq::preset_rect(i);
        let acts = a.tap(px + pw / 2, py + ph / 2);
        assert_eq!(
            a.eq_preset, i,
            "pill {i} centre selected preset {}",
            a.eq_preset
        );
        assert_eq!(acts, vec![Action::EqChanged(data::EQ_PRESETS[i].1)]);
    }
}

/// A tap in the gap between pills, or past the last one, must do nothing rather than change
/// the sound.
#[test]
fn eq_preset_gaps_are_inert() {
    let mut a = unlocked();
    a.push(Screen::Eq);
    let (px0, py, pw0, ph) = crate::eq::preset_rect(0);
    let (px1, _, _, _) = crate::eq::preset_rect(1);
    let gap_x = (px0 + pw0 + px1) / 2; // between pill 0 and pill 1
    assert!(
        a.tap(gap_x, py + ph / 2).is_empty(),
        "gap between pills changed the EQ"
    );
    let (lx, _, lw, _) = crate::eq::preset_rect(data::EQ_PRESETS.len() - 1);
    assert!(
        a.tap(lx + lw + 6, py + ph / 2).is_empty(),
        "blank space past the pills changed the EQ"
    );
}

/// Above the zero line raises, below lowers — the split must be the drawn zero line, not an
/// unrelated constant (it was 375 while the line is drawn at the field midpoint).
#[test]
fn eq_band_tap_raises_above_the_zero_line_and_lowers_below() {
    use crate::eq::{band_center_x, FIELD_BOTTOM, FIELD_MID, FIELD_TOP};
    let mut a = unlocked();
    a.push(Screen::Eq);
    a.eq_bands = [0; 10];
    for band in [0usize, 5, 9] {
        let x = band_center_x(band);
        a.tap(x, (FIELD_TOP + FIELD_MID) / 2); // upper half
        assert_eq!(
            a.eq_bands[band],
            crate::eq::BAND_STEP,
            "band {band} above the line should raise"
        );
        assert_eq!(
            a.eq_sel, band,
            "tap should select the band under the finger"
        );
        a.tap(x, (FIELD_MID + FIELD_BOTTOM) / 2); // lower half
        assert_eq!(
            a.eq_bands[band], 0,
            "band {band} below the line should lower"
        );
    }
    // Just below the zero line lowers (this pixel used to raise).
    a.tap(band_center_x(3), FIELD_MID + 1);
    assert_eq!(a.eq_bands[3], -crate::eq::BAND_STEP);
}

/// The band hit test must agree with where the knobs are drawn, for every band.
#[test]
fn eq_band_centres_resolve_to_their_own_band() {
    for i in 0..10 {
        assert_eq!(
            crate::eq::band_at(crate::eq::band_center_x(i)),
            Some(i),
            "band {i}"
        );
    }
}

/// The EQ footer "Reset" was drawn but hit-tested nowhere.
#[test]
fn eq_footer_reset_flattens_the_bands() {
    let mut a = unlocked();
    a.push(Screen::Eq);
    a.eq_bands = [5, -3, 2, 0, 1, -1, 4, 2, -2, 6];
    let acts = a.tap(60, crate::eq::FOOTER_TOP + 20);
    assert_eq!(a.eq_bands, [0; 10]);
    assert_eq!(acts, vec![Action::EqChanged([0; 10])]);
}

/// The accent band on each Library tab shuffles in that tab's scope. It is the largest touch
/// target on the screen and was previously drawn but hit-tested nowhere.
#[test]
fn shuffle_band_is_tappable_on_every_tab() {
    let (bx, by, bw, bh) = library::library_shuffle_band();
    let (cx, cy) = (bx + bw / 2, by + bh / 2);
    for (tab, want) in [
        (Tab::Songs, ShuffleScope::AllSongs),
        (Tab::Albums, ShuffleScope::ByAlbum),
        (Tab::Artists, ShuffleScope::ByArtist),
        (Tab::Playlists, ShuffleScope::Playlist),
    ] {
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = tab;
        assert_eq!(a.tap(cx, cy), vec![Action::Shuffle(want)], "tab {tab:?}");
    }
}

/// EVERY Shuffle band asks before it destroys the user's picks.
///
/// The four bands used to return their action directly instead of going through
/// `start_play_action`, so they reached `set_play_context` — and its `queue.clear()` — with no
/// prompt. The shuffle bands are the largest targets in the app, which made them the easiest
/// way to lose a hand-built queue and the only play path that did it silently. On the
/// Playlists tab the two behaviours sat side by side: the PLAY band asked, the SHUFFLE band
/// beside it did not.
///
/// Note what `shuffle_band_is_tappable_on_every_tab` above does NOT catch: with an EMPTY queue
/// the funnel is transparent and returns `vec![act]` unchanged, so that test passes either
/// way. The queue has to be non-empty for the difference to exist at all.
#[test]
fn every_shuffle_band_asks_before_it_clears_the_queue() {
    use crate::confirm::{hit, Ask, Hit};
    let (bx, by, bw, bh) = library::library_shuffle_band();
    let (cx, cy) = (bx + bw / 2, by + bh / 2);

    // The Library band, on each of its four tabs.
    for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
        let mut a = unlocked();
        a.push(Screen::Library);
        a.lib_tab = tab;
        a.queue_push_for_test();
        assert_eq!(
            a.tap(cx, cy),
            vec![],
            "tab {tab:?}: the band must not play yet"
        );
        assert!(
            a.modal_open(),
            "tab {tab:?}: it must ask about the queue first"
        );
    }

    // The artist PAGE band. Located the way the screen locates it, by asking the hit test.
    let ay = (0..crate::H as i32)
        .find(|y| library::hit_artist_shuffle_band(240, *y, 2))
        .expect("the artist page draws a shuffle band");
    let mut a = unlocked();
    a.push(Screen::Artist);
    a.queue_push_for_test();
    assert_eq!(a.tap(240, ay), vec![], "artist page: must not play yet");
    assert!(
        a.modal_open(),
        "artist page: it must ask about the queue first"
    );

    // And answering KEEP still starts the shuffle — the prompt delays the action, it never
    // swallows it.
    let keep = (0..crate::H as i32)
        .find(|y| hit(Ask::QueueOnPlay, 240, *y) == Hit::KeepQueue)
        .expect("the card offers a keep-queue row");
    let acts = a.tap(240, keep);
    assert!(
        acts.iter().any(|t| matches!(t, Action::ShuffleArtist(_))),
        "keeping the queue must still shuffle — {acts:?}"
    );
    assert_eq!(upq(&a).len(), 1, "and the pick must survive it");
}

/// The band must not swallow taps meant for the list, and the list must not start inside it —
/// this is the render/hit-test drift that made row taps land on the wrong song before.
#[test]
fn shuffle_band_and_list_do_not_overlap() {
    let (_, by, _, bh) = library::library_shuffle_band();
    for tab in [Tab::Songs, Tab::Albums, Tab::Artists, Tab::Playlists] {
        assert!(
            library::list_top(tab) >= by + bh,
            "{tab:?} list starts inside the shuffle band"
        );
    }
    // A tap one pixel below the band is a list tap, not a shuffle.
    let mut a = unlocked();
    a.push(Screen::Library);
    a.lib_tab = Tab::Songs;
    assert!(!library::hit_shuffle_band(240, by + bh + 1));
    // ...and one inside it is not a row.
    assert_eq!(library::hit_row(Tab::Songs, &a.lib, 0, by + bh / 2), None);
}

/// A playlist row has no single track under the cursor, so both input routes (button Select
/// and a finger tap) play the whole list from the top.
#[test]
fn playlist_row_plays_the_whole_list() {
    let mut a = two_playlists();
    assert_eq!(a.press(Button::Select), vec![Action::PlayPlaylist(77)]);

    // Same for a tap on the ROW BUTTON (x >= 404). Derive the y from the render's own geometry
    // (list_top + row_h) rather than a magic number, so this can't drift from the layout.
    let y = library::list_top(Tab::Playlists) + library::row_h(Tab::Playlists) + 4;
    assert_eq!(a.tap(434, y), vec![Action::PlayPlaylist(78)]);
    assert_eq!(a.lib_index(), 1);
}

/// Two playlists with real members, sitting on the Playlists tab.
fn two_playlists() -> App {
    let mut a = unlocked();
    a.push(Screen::Library);
    a.lib_tab = Tab::Playlists;
    a.lib_idx = 0;
    let songs: Vec<SongRow> = (0..5)
        .map(|i| SongRow {
            title: format!("P{i}"),
            artist: "Someone".into(),
            dur: "3:00".into(),
            object_id: 500 + i,
            ..Default::default()
        })
        .collect();
    a.lib = crate::model::Library {
        playlists: vec![
            crate::model::PlaylistRow {
                id: 77,
                name: "Night Bus".into(),
                tracks: 3,
                art: "Night Bus".into(),
                track_list: songs[..3].to_vec(),
                user: false,
                ..Default::default()
            },
            crate::model::PlaylistRow {
                id: 78,
                name: "Morning".into(),
                tracks: 2,
                art: "Morning".into(),
                track_list: songs[3..].to_vec(),
                user: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    a
}

/// A library with one of CINDER'S OWN playlists (editable) and one of Sony's (not).
fn own_and_sony() -> App {
    let mut a = App::unlocked();
    a.push_for_test(Screen::Library);
    a.lib_tab = Tab::Playlists;
    let songs: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("T{i}"),
            artist: "Someone".into(),
            dur: "3:00".into(),
            object_id: 900 + i,
            ..Default::default()
        })
        .collect();
    a.lib = crate::model::Library {
        songs: songs.clone(),
        playlists: vec![
            crate::model::PlaylistRow {
                id: -42,
                name: "Mine".into(),
                tracks: 2,
                art: "Mine".into(),
                track_list: songs[..2].to_vec(),
                user: true,
                ..Default::default()
            },
            crate::model::PlaylistRow {
                id: 7,
                name: "Sony's".into(),
                tracks: 2,
                art: "Sony's".into(),
                track_list: songs[2..].to_vec(),
                user: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    a
}

fn kb_key(a: &mut App, key: crate::keyboard::Key) -> Vec<Action> {
    // Tap the key where it is drawn, so the test goes through the same hit test the finger does.
    let page = a.kb_page;
    let (row, col) = (0..crate::keyboard::ROWS)
        .flat_map(|r| (0..12).map(move |c| (r, c)))
        .find(|&(r, c)| crate::keyboard::key_at(page, r, c) == Some(key))
        .expect("key is on this page");
    let (x, y, w, h) = crate::keyboard::key_rect(page, row, col).unwrap();
    a.tap(x + w / 2, y + h / 2)
}

/// The only way to make a playlist on the device: the row above the list, then the keyboard.
#[test]
fn the_new_playlist_row_opens_the_keyboard_and_names_one() {
    let mut a = own_and_sony();
    let (x, y, _, h) = library::new_playlist_rect();
    assert!(
        a.tap(x + 100, y + h / 2).is_empty(),
        "opening the keyboard emits no action"
    );
    assert_eq!(a.current(), Screen::Keyboard);

    for ch in "night bus".chars() {
        let key = if ch == ' ' {
            crate::keyboard::Key::Space
        } else {
            crate::keyboard::Key::Char(ch)
        };
        assert!(kb_key(&mut a, key).is_empty());
    }
    assert_eq!(a.text_input(), "night bus");
    assert_eq!(
        kb_key(&mut a, crate::keyboard::Key::Done),
        vec![Action::PlaylistCreate]
    );
    assert_eq!(
        a.current(),
        Screen::Library,
        "Done leaves the keyboard behind"
    );
}

/// An empty name is not a playlist: Done must do nothing rather than create "Playlist".
#[test]
fn done_with_an_empty_field_does_nothing() {
    let mut a = own_and_sony();
    let (x, y, _, h) = library::new_playlist_rect();
    a.tap(x + 100, y + h / 2);
    assert!(kb_key(&mut a, crate::keyboard::Key::Done).is_empty());
    assert_eq!(
        a.current(),
        Screen::Keyboard,
        "and it stays on the keyboard"
    );
}

#[test]
fn renaming_starts_from_the_current_name() {
    let mut a = own_and_sony();
    a.open_playlist(0);
    let (bx, by, bw, bh) = library::playlist_action_rect(1); // RENAME
    assert!(a.tap(bx + bw / 2, by + bh / 2).is_empty());
    assert_eq!(a.current(), Screen::Keyboard);
    assert_eq!(a.text_input(), "Mine", "the field is seeded, not blank");
    assert_eq!(
        kb_key(&mut a, crate::keyboard::Key::Done),
        vec![Action::PlaylistRename(-42)]
    );
}

/// The page has no × any more: removing is the editor's job, so a tap at the row's right end
/// plays it like the rest of the row.
#[test]
fn the_playlist_page_plays_where_the_remove_column_was() {
    let mut a = own_and_sony();
    a.open_playlist(0);
    let row_y =
        library::playlist_content_top(a.playlist_row().unwrap()) + library::PLAYLIST_TRACK_RH() / 2;
    assert_eq!(a.tap(420, row_y), vec![Action::PlayPlaylistAt(-42, 0)]);
}

/// Settings ▸ Delete music: off, the header slot does nothing; on, DELETE asks, and yes hands
/// the shell exactly the files the question was about, skips the song if it was playing, and
/// takes the rest out of Up Next.
#[test]
fn menu_icons_are_off_by_default_and_their_row_switches_them() {
    let mut a = unlocked();
    assert!(!a.opt(Opt::MenuIcons), "off by default");
    assert_eq!(
        crate::settings::page_of(crate::settings::ROW_MENU_ICONS),
        crate::settings::GROUPS
            .iter()
            .position(|g| g.0 == "Buttons & gestures"),
        "beside the pull-down panel, which #53 asked for with it"
    );
    a.settings_sel = crate::settings::ROW_MENU_ICONS;
    assert!(
        a.settings_activate().is_empty(),
        "render-only: no shell work"
    );
    assert!(a.opt(Opt::MenuIcons));
    assert!(a.settings_activate().is_empty());
    assert!(!a.opt(Opt::MenuIcons));
}

#[test]
fn deleting_music_is_off_by_default_then_asks_and_hands_over_the_files() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 900 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, 1);
    a.set_track_info(vec![("Title".into(), "T1".into())]);
    a.push(Screen::TrackInfo);
    let (hx, hy) = (
        crate::chrome::HEADER_ACTION_X + 40,
        crate::chrome::HEADER_BOTTOM - 20,
    );
    assert!(!a.opt(Opt::DeleteMusic), "off by default");
    assert!(a.tap(hx, hy).is_empty());
    assert!(!a.modal_open());

    a.set_opt(Opt::DeleteMusic, true);
    assert!(a.tap(hx, hy).is_empty());
    assert!(a.modal_open(), "a delete is always asked first");
    let (cx, cy) = crate::confirm::confirm_button_centre(crate::confirm::Ask::DeleteTrack);
    assert_eq!(a.tap(cx, cy), vec![Action::DeleteMusic, Action::Next]);
    assert_eq!(a.take_delete(), vec![901]);
    assert!(a.take_delete().is_empty(), "taken once");
    assert_ne!(a.current(), Screen::TrackInfo, "the page about it closes");

    // An album: every song of it, and the ones still to come leave Up Next.
    let ids: Vec<i64> = a.lib.albums_flat()[0]
        .track_list
        .iter()
        .map(|t| t.object_id)
        .collect();
    let rows: Vec<SongRow> = a.lib.albums_flat()[0].track_list.clone();
    a.set_play_context(rows, 0);
    a.album_view = 0;
    a.push(Screen::Album);
    assert!(a.tap(hx, hy).is_empty());
    assert!(a.modal_open());
    let (cx, cy) = crate::confirm::confirm_button_centre(crate::confirm::Ask::DeleteAlbum);
    assert_eq!(
        a.tap(cx, cy),
        vec![Action::DeleteMusic, Action::QueueChanged, Action::Next]
    );
    assert_eq!(a.take_delete(), ids);
    assert_eq!(a.context.len(), 1, "only the playing row is left");
}

#[test]
fn deleting_a_playlist_asks_first() {
    let mut a = own_and_sony();
    a.open_playlist(0);
    let (bx, by, bw, bh) = library::playlist_action_rect(2); // DELETE
    assert!(a.tap(bx + bw / 2, by + bh / 2).is_empty());
    assert!(a.modal_open(), "delete must be confirmed, not immediate");
    let (cx, cy) = crate::confirm::confirm_button_centre(crate::confirm::Ask::DeletePlaylist);
    assert_eq!(a.tap(cx, cy), vec![Action::PlaylistDelete(-42)]);
    assert_ne!(
        a.current(),
        Screen::Playlist,
        "the page it deleted must not stay open"
    );
}

/// Reported 2026-08-23: "selecting a single song just plays that song not the rest of the
/// playlist". A member row must carry the PLAYLIST and the row's index, because an object id
/// on its own resolves to the track's album and nothing wider.
#[test]
fn a_playlist_member_plays_the_whole_playlist_from_that_row() {
    let mut a = two_playlists();
    a.open_playlist(0); // "Night Bus", 3 members
    let top = library::playlist_content_top(a.playlist_row().unwrap());
    for row in 0..3u32 {
        let y = top + row as i32 * library::PLAYLIST_TRACK_RH() + library::PLAYLIST_TRACK_RH() / 2;
        assert_eq!(
            a.tap(200, y),
            vec![Action::PlayPlaylistAt(77, row)],
            "row {row} must play playlist 77 from index {row}"
        );
    }
}

/// Reported 2026-08-23: "no way to press normal non shuffled play on a playlist". The band's
/// two halves must tile it exactly — no dead strip between them, and neither may answer for
/// the other.
#[test]
fn the_playlist_band_offers_play_as_well_as_shuffle() {
    let (bx, by, bw, bh) = library::shuffle_band_rect(library::PLAYLIST_BAND_Y);
    let (px, py, pw, ph) = library::playlist_play_band();
    let (sx, sy, sw, sh) = library::playlist_shuffle_band();
    assert_eq!(
        (px, py, ph),
        (bx, by, bh),
        "the play half keeps the band's origin"
    );
    assert_eq!((sy, sh), (by, bh));
    assert_eq!(px + pw, sx, "the halves must be adjacent");
    assert_eq!(pw + sw, bw, "the halves must cover the whole band");
    for x in [px, px + pw / 2, px + pw - 1] {
        assert!(library::hit_playlist_play_band(x, by + bh / 2));
        assert!(!library::hit_playlist_shuffle_band(x, by + bh / 2));
    }
    for x in [sx, sx + sw / 2, sx + sw - 1] {
        assert!(library::hit_playlist_shuffle_band(x, by + bh / 2));
        assert!(!library::hit_playlist_play_band(x, by + bh / 2));
    }
    assert!(!library::hit_playlist_play_band(px + pw / 2, by - 1));
    assert!(!library::hit_playlist_shuffle_band(sx + sw / 2, by + bh));
}

/// The queue-replace prompt sits in the middle of every play funnel. It must replay the
/// action it was holding — the whole point of the fix above is that the context travels with
/// the tap, and a prompt that downgraded it back to a bare object id would undo that.
#[test]
fn the_queue_prompt_replays_the_playlist_context() {
    let mut a = two_playlists();
    a.open_playlist(0);
    a.queue_push_for_test();
    let y = library::playlist_content_top(a.playlist_row().unwrap())
        + library::PLAYLIST_TRACK_RH()
        + library::PLAYLIST_TRACK_RH() / 2;
    assert!(a.tap(200, y).is_empty(), "with a queue, the tap asks first");
    assert!(a.modal_open());
    let keep = (0..crate::canvas::H as i32)
        .find(|y| {
            crate::confirm::hit(crate::confirm::Ask::QueueOnPlay, 240, *y)
                == crate::confirm::Hit::KeepQueue
        })
        .expect("row has no tappable pixel");
    assert_eq!(a.tap(240, keep), vec![Action::PlayPlaylistAt(77, 1)]);
}

/// Sony's playlists are read-only here, and the page must not pretend otherwise.
#[test]
fn a_sony_playlist_has_no_edit_controls() {
    let mut a = own_and_sony();
    a.open_playlist(1); // Sony's
    let pl = a.playlist_row().unwrap();
    let (bx, by, bw, bh) = library::playlist_action_rect(2);
    assert!(library::hit_playlist_action(pl, bx + bw / 2, by + bh / 2).is_none());
    // The row's right end plays it, as on a list Cinder owns.
    let row_y =
        library::playlist_content_top(a.playlist_row().unwrap()) + library::PLAYLIST_TRACK_RH() / 2;
    // …and it plays the PLAYLIST from that row, not the track's album.
    assert_eq!(a.tap(420, row_y), vec![Action::PlayPlaylistAt(7, 0)]);
}

/// The metadata block is tappable IN BOTH THEMES, at the place it is actually drawn.
///
/// The two layouts are separate code paths: day puts title/artist/codec under the cover at
/// `INFO_TOP`, night draws a compact header at the top of the screen. The hit test was
/// theme-blind, so at night the only way in was a patch of empty space where the day text
/// would have been.
#[test]
fn track_info_opens_from_the_metadata_block_in_either_theme() {
    use crate::now_playing as np;
    let day_y = (np::INFO_TOP + np::INFO_BOT) / 2;
    let night_y = (np::INFO_NIGHT_TOP + np::INFO_NIGHT_BOT) / 2;

    // Day: the block under the cover opens it; the night header band does not.
    assert!(np::hit_info(200, day_y, false));
    assert!(
        !np::hit_info(200, night_y, false),
        "the night band is not live in day mode"
    );
    // ...and the heart still wins its own corner.
    assert!(!np::hit_info(np::HEART_CX, day_y, false));

    // Night: the header opens it, and the empty space the day layout used does NOT — a button
    // in blank space is how this got reported in the first place.
    assert!(np::hit_info(200, night_y, true));
    assert!(
        np::hit_info(30, night_y, true),
        "the thumb is part of the same block"
    );
    assert!(
        !np::hit_info(200, day_y, true),
        "no hidden control in the night layout's gap"
    );

    // End to end through the navigator, in night mode.
    let mut a = own_and_sony();
    while a.current() != Screen::NowPlaying {
        a.press(Button::Back);
    }
    a.set_night_for_test(true);
    assert_eq!(a.tap(200, night_y), Vec::<Action>::new());
    assert_eq!(
        a.current(),
        Screen::TrackInfo,
        "night mode must reach Track information"
    );
}

/// The Artist and Album rows are LINKS: they go to the library rather than sitting there.
#[test]
fn track_info_links_jump_to_the_artist_and_album() {
    assert!(crate::track_info::is_link("Artist"));
    assert!(crate::track_info::is_link("Album"));
    assert!(
        !crate::track_info::is_link("Format"),
        "a fact about the file is not a place"
    );

    // `App::unlocked()` carries `Library::sample()`, which has real artists and albums;
    // `own_and_sony()` replaces the library with playlists only and has neither.
    let mut a = App::unlocked();
    let artist = a.lib.artists[0].name.clone();
    let album = a.lib.albums_flat()[0].name.clone();
    let album_artist = a.lib.albums_flat()[0].artist.clone();

    // Rows as the shell sends them, and the heights the renderer would measure.
    let rows = vec![
        ("Title".to_string(), "Whatever".to_string()),
        ("Artist".to_string(), artist.clone()),
        ("Album".to_string(), album.clone()),
        ("Format".to_string(), "FLAC".to_string()),
    ];
    let row_y = |i: usize| crate::track_info::TOP + i as i32 * 44 + 4;

    // Artist row -> the artist page.
    a.push_for_test(Screen::TrackInfo);
    a.set_track_info(rows.clone());
    a.set_track_info_rows_for_test(vec![44, 44, 44, 44]);
    assert_eq!(a.tap(240, row_y(1)), Vec::<Action>::new());
    assert_eq!(a.current(), Screen::Artist);

    // Album row -> the album page, and the RIGHT album: prefer the one by this track's artist.
    let mut b = App::unlocked();
    let mut rows2 = rows.clone();
    rows2[1].1 = album_artist;
    b.push_for_test(Screen::TrackInfo);
    b.set_track_info(rows2);
    b.set_track_info_rows_for_test(vec![44, 44, 44, 44]);
    assert_eq!(b.tap(240, row_y(2)), Vec::<Action>::new());
    assert_eq!(b.current(), Screen::Album);
    assert_eq!(b.lib.albums_flat()[b.album_view_for_test()].name, album);

    // A tag naming something absent says so instead of doing nothing silently.
    let mut d = App::unlocked();
    d.push_for_test(Screen::TrackInfo);
    d.set_track_info(vec![("Album".to_string(), "No Such Record".to_string())]);
    d.set_track_info_rows_for_test(vec![44]);
    assert_eq!(d.tap(240, row_y(0)), Vec::<Action>::new());
    assert_eq!(
        d.current(),
        Screen::TrackInfo,
        "a dead tag must not navigate"
    );

    // A non-link row is inert.
    let mut e = App::unlocked();
    e.push_for_test(Screen::TrackInfo);
    e.set_track_info(rows);
    e.set_track_info_rows_for_test(vec![44, 44, 44, 44]);
    e.tap(240, row_y(3));
    assert_eq!(e.current(), Screen::TrackInfo);
}

/// Lyrics open from their Track information row, follow the song until the user scrolls, ask
/// for a repaint only when the sung line changes, and start over for the next song.
#[test]
fn lyrics_open_from_track_info_and_follow_until_scrolled() {
    use crate::lyrics::{Line, Lyrics};
    let mut a = App::unlocked();
    let lines = (0..40u32)
        .map(|i| Line {
            at_ms: Some(i * 1000),
            text: format!("line {i}"),
        })
        .collect();
    a.set_lyrics(Some(Lyrics { lines }));
    a.push_for_test(Screen::TrackInfo);
    a.set_track_info(vec![
        ("Title".to_string(), "Whatever".to_string()),
        ("Lyrics".to_string(), "Synced, 40 lines".to_string()),
    ]);
    a.set_track_info_rows_for_test(vec![44, 44]);
    assert!(
        !a.set_lyrics_position(3_000),
        "a new line off the Lyrics page is not a repaint"
    );

    assert_eq!(
        a.tap(240, crate::track_info::TOP + 44 + 4),
        Vec::<Action>::new()
    );
    assert_eq!(a.current(), Screen::Lyrics);
    assert!(a.lyrics_follow, "opening the page follows the song");
    assert!(a.set_lyrics_position(5_000), "line 3 -> line 5 on the page");
    assert!(!a.set_lyrics_position(5_400), "still line 5");

    a.lyrics_heights = vec![38; 40]; // what the render measures
    a.scroll_px(30);
    assert!(
        !a.lyrics_follow,
        "the user's own scroll stops the song dragging the page"
    );
    assert_eq!(a.lyrics_scroll_px, 30);

    a.set_lyrics(None);
    assert!(
        a.lyrics_follow && a.lyrics_scroll_px == 0,
        "the next song starts over"
    );
    assert!(
        !a.set_lyrics_position(9_000),
        "no lyrics, nothing to repaint"
    );
}

/// Now Playing's Lyrics chip is one tap to the words — and with no lyrics there is no chip, so
/// the same tap does what it always did (the band under the status bar opens the Menu).
#[test]
fn the_lyrics_chip_opens_the_words_only_when_there_are_some() {
    use crate::lyrics::{Line, Lyrics};
    use crate::now_playing as np;
    let (x, y) = (np::LYRICS_X0 + 20, np::LYRICS_Y0 + np::LYRICS_H / 2);

    let mut a = App::unlocked();
    a.set_opt(Opt::NpLyrics, true);
    assert_eq!(a.current(), Screen::NowPlaying);
    a.tap(x, y);
    assert_eq!(
        a.current(),
        Screen::Menu,
        "no lyrics: no chip, the band is the Menu's"
    );

    let mut a = App::unlocked();
    a.set_opt(Opt::NpLyrics, true);
    a.set_lyrics(Some(Lyrics { lines: vec![] }));
    a.tap(x, y);
    assert_eq!(a.current(), Screen::Menu, "an empty file is no lyrics");

    let mut a = App::unlocked();
    a.set_opt(Opt::NpLyrics, true);
    a.set_lyrics(Some(Lyrics {
        lines: vec![Line {
            at_ms: None,
            text: "words".into(),
        }],
    }));
    a.tap(x, y);
    assert_eq!(a.current(), Screen::Lyrics);
    a.pop();
    assert_eq!(
        a.current(),
        Screen::NowPlaying,
        "Back returns to Now Playing"
    );

    // The chip's target clears the heart, the transport and the paging swipe's taps.
    assert!(!np::hit_lyrics(np::HEART_CX, np::HEART_CY, true));
    assert!(!np::hit_lyrics(240, 300, true));
    const { assert!(np::LYRICS_Y0 + np::LYRICS_H <= crate::chrome::STATUS_H + 44) };

    let mut a = App::unlocked();
    a.set_lyrics(Some(Lyrics {
        lines: vec![Line {
            at_ms: None,
            text: "words".into(),
        }],
    }));
    assert!(!a.opt(Opt::NpLyrics), "the chip ships off");
    a.tap(x, y);
    assert_eq!(
        a.current(),
        Screen::Menu,
        "switched off: no chip, even with words"
    );
}

/// Library search is opt-in: no button without the component. With it, the header button opens
/// search with the keyboard up, typing filters as you go, an empty query finds nothing, and a
/// result plays.
#[test]
fn library_search_is_opt_in_and_plays_what_it_finds() {
    let (bx, by) = ((library::SEARCH_X0 + library::SEARCH_X1) / 2, 62);
    let mut off = App::unlocked();
    off.push_for_test(Screen::Library);
    assert_eq!(off.tap(bx, by), Vec::<Action>::new());
    assert_eq!(
        off.current(),
        Screen::Library,
        "not installed: the header has no button"
    );

    let mut a = App::unlocked();
    a.set_search_enabled(true);
    a.push_for_test(Screen::Library);
    assert_eq!(a.tap(bx, by), Vec::<Action>::new());
    assert_eq!(a.current(), Screen::Keyboard, "the keyboard is already up");
    assert!(a.search_order.is_empty(), "nothing typed, nothing found");

    let want = a.lib.songs[0].clone();
    a.kb_text = want.title.clone();
    a.kb_after_key();
    let song_row = |a: &App| {
        a.search_order.iter().position(|h| {
                matches!(*h, crate::search::Hit::Song(i) if a.lib.songs[i].object_id == want.object_id)
            })
    };
    assert!(song_row(&a).is_some(), "found while typing");
    a.pop();
    assert_eq!(a.current(), Screen::Search);

    let row = song_row(&a).unwrap();
    let y = crate::playlist_pick::LIST_TOP + row as i32 * crate::playlist_pick::ROW_H + 10;
    assert_eq!(a.tap(240, y), vec![Action::PlayIndex(want.object_id)]);
}

/// Search finds artists and albums by name, ahead of the songs, and their rows open the page.
#[test]
fn search_finds_artists_and_albums_and_opens_them() {
    use crate::search::Hit;
    let row_y =
        |r: usize| crate::playlist_pick::LIST_TOP + r as i32 * crate::playlist_pick::ROW_H + 10;
    let mut a = App::unlocked();
    a.set_search_enabled(true);
    a.push_for_test(Screen::Library);

    let artist = a.lib.artists[0].name.clone();
    a.search_query = artist.clone();
    a.search_rebuild();
    a.push_for_test(Screen::Search);
    let r = a
        .search_order
        .iter()
        .position(|h| *h == Hit::Artist(0))
        .expect("artist found");
    assert!(
        a.search_order[..r]
            .iter()
            .all(|h| matches!(h, Hit::Artist(_))),
        "artists come first"
    );
    assert_eq!(a.tap(240, row_y(r)), Vec::<Action>::new());
    assert_eq!(a.current(), Screen::Artist);
    assert_eq!(a.artist_view, 0);

    let album = a.lib.albums_flat()[0].name.clone();
    a.pop();
    a.search_query = album;
    a.search_rebuild();
    let r = a
        .search_order
        .iter()
        .position(|h| *h == Hit::Album(0))
        .expect("album found");
    if let Some(first_song) = a
        .search_order
        .iter()
        .position(|h| matches!(h, Hit::Song(_)))
    {
        assert!(r < first_song, "albums come before songs");
    }
    assert_eq!(a.tap(240, row_y(r)), Vec::<Action>::new());
    assert_eq!(a.current(), Screen::Album);
    assert_eq!(a.album_view, 0);
    // An album row has nothing to queue on its own: it neither slides nor plays next.
    a.pop();
    assert!(!a.swipe_track(-80, row_y(r)));

    // A book is found by its title or its author, and plays from its place.
    a.lib.books.push(crate::model::BookRow {
        title: "Zqx Chronicles".into(),
        author: "Wren Abbot".into(),
        chapters: vec![a.lib.songs[0].clone()],
        ..Default::default()
    });
    let b = a.lib.books.len() - 1;
    for q in ["zqx", "wren abbot"] {
        a.search_query = q.into();
        a.search_rebuild();
        assert_eq!(a.search_order.first(), Some(&Hit::Book(b)), "{q}");
    }
    assert_eq!(a.tap(240, row_y(0)), vec![Action::PlayBook(b)]);
}

/// The "Add to playlist" picker is REACHABLE. It was built complete — renderer, hit tests,
/// actions, persistence — and then shipped with nothing that opened it, so the only way to add
/// a song to a playlist was to open the playlist and go and find the song. Reported 2026-09-09
/// as "no way to edit an existing playlist or add songs to it".
#[test]
fn track_info_opens_the_add_to_playlist_picker() {
    let mut a = own_and_sony(); // the picker only ever lists YOUR OWN playlists
    let album: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("T{i}"),
            artist: "Someone".into(),
            dur: "3:00".into(),
            object_id: 900 + i,
            ..Default::default()
        })
        .collect();
    let want = album[1].object_id;
    a.set_play_context(album, 1);
    a.push(Screen::TrackInfo);

    // The band is under the list, and the list cannot be scrolled over it.
    let by = crate::track_info::BOTTOM + crate::track_info::ACTION_H / 2;
    assert!(crate::track_info::hit_add_to_playlist(by));
    assert!(
        !crate::track_info::hit_add_to_playlist(crate::track_info::BOTTOM - 1),
        "the row above the band is still a row"
    );

    assert_eq!(
        a.tap(240, by),
        Vec::<Action>::new(),
        "opening a picker is navigation"
    );
    assert_eq!(a.current(), Screen::PlaylistPick, "the picker is on screen");

    // ...and it is aimed at the PLAYING track, not at whatever library row was last touched.
    let acts = a.tap(
        240,
        crate::playlist_pick::TOP + crate::playlist_pick::ROW_H + 4,
    );
    assert!(
        acts.iter()
            .any(|x| matches!(x, Action::PlaylistAddTrack(_, t) if *t == want)),
        "the track added must be the one that was playing, got {acts:?}"
    );
}

/// The picker itself, driven directly: row 0 makes a new playlist, the rest add to one.
#[test]
fn add_to_playlist_picker() {
    let mut a = own_and_sony();
    while a.current() != Screen::NowPlaying {
        a.press(Button::Back);
    }
    let songs = a.lib.songs.clone();
    a.set_play_context(songs, 1);
    a.open_playlist_pick(901);
    assert_eq!(a.current(), Screen::PlaylistPick);

    // Row 0 makes a new playlist AND remembers the track; row 1 is "Mine".
    let row1 = crate::playlist_pick::TOP + crate::playlist_pick::ROW_H + 4;
    assert_eq!(a.tap(240, row1), vec![Action::PlaylistAddTrack(-42, 901)]);
    assert_ne!(
        a.current(),
        Screen::PlaylistPick,
        "picking a playlist closes the picker"
    );
}

/// #56: holding a FOLDER row opens "Add to playlist" for every song in it and below, in browse
/// order; picking a playlist hands the whole list over once. Holding a song row still queues.
#[test]
fn holding_a_folder_adds_the_whole_tree_to_a_playlist() {
    let _g = lock_scale();
    let mut a = own_and_sony();
    let song = |id: i64| SongRow {
        title: format!("S{id}"),
        object_id: id,
        ..Default::default()
    };
    let folder = |name: &str, tracks: Vec<SongRow>, subdirs: Vec<usize>| crate::model::FolderRow {
        name: name.into(),
        tracks,
        subdirs,
        ..Default::default()
    };
    a.lib.folders = vec![
        folder("Music", vec![], vec![1, 3, 4]),
        folder("Artist", vec![song(10), song(11)], vec![2]),
        folder("Album", vec![song(12)], vec![]),
        folder("Empty", vec![], vec![]),
        folder("Other", vec![song(20), song(10)], vec![]),
    ];
    a.push(Screen::Folders);
    a.folder_stack = vec![0];
    let row = |r: usize| crate::folders::row_top(r, 0) + 10;
    let header = crate::chrome::STATUS_H + 10;
    let row1 = crate::playlist_pick::TOP + crate::playlist_pick::ROW_H + 4;

    // A hold starts a selection — answered with `hold_play_next` OFF, the default: on the
    // player it was never asked until the shell stopped gating every hold on that switch.
    assert!(!a.hold_play_next);
    assert_eq!(a.hold(240, row(0)), Some(vec![]));
    assert_eq!(a.current(), Screen::Folders, "selecting, not picking yet");
    // A tap ticks another folder (not entering it); a second tap unticks; Other is ticked.
    assert!(a.tap(240, row(1)).is_empty());
    assert!(a.tap(240, row(1)).is_empty());
    assert!(a.tap(240, row(2)).is_empty());
    assert_eq!(a.folder_sel, vec![1, 4]);
    // ADD: both trees, ticked order, a song in two folders once.
    assert!(a.tap(400, header).is_empty());
    assert_eq!(a.current(), Screen::PlaylistPick);
    assert!(a.folder_sel.is_empty());
    assert_eq!(a.tap(240, row1), vec![Action::PlaylistAddPicked(-42)]);
    assert_eq!(a.take_pick_many(), vec![10, 11, 12, 20]);
    assert_eq!(a.current(), Screen::Folders, "back where the selection was");

    // Back cancels a selection without leaving the level.
    a.hold(240, row(0));
    a.press(Button::Back);
    assert!(a.folder_sel.is_empty());
    assert_eq!(a.current(), Screen::Folders);

    // One folder: a new playlist from the picker is named after it.
    a.hold(240, row(0));
    a.tap(400, header);
    assert_eq!(a.tap(240, crate::playlist_pick::TOP + 4), vec![]);
    assert_eq!(a.text_input(), "Artist");
}

#[test]
fn menu_soundscape_localizes_the_builtin_name_without_changing_the_id() {
    let mut app = unlocked();
    app.ambient_on = true;
    app.ambient_sound = 4;
    assert_eq!(app.menu_subtitles().soundscape, "Rain");
    {
        let _language = crate::i18n::enter(crate::i18n::Language::SimplifiedChinese);
        assert_eq!(app.menu_subtitles().soundscape, "雨声");
        assert_eq!(app.ambient_sound, 4);
    }
    assert_eq!(app.menu_subtitles().soundscape, "Rain");
}

#[test]
fn cached_playlist_and_queue_notices_keep_identity_across_language_changes() {
    let mut playlist = own_and_sony();
    playlist.lib.playlists[0].name = "Up Next — Blue".into();
    playlist.set_play_context(playlist.lib.songs.clone(), 1);
    playlist.open_playlist_pick(901);
    let row = crate::playlist_pick::TOP + crate::playlist_pick::ROW_H + 4;
    assert_eq!(
        playlist.tap(240, row),
        vec![Action::PlaylistAddTrack(-42, 901)]
    );
    assert_eq!(playlist.toast, "Added to Up Next — Blue");
    assert!(
        playlist.toast_frames > 0,
        "language change must not clear the notice"
    );
    {
        let _language = crate::i18n::enter(crate::i18n::Language::SimplifiedChinese);
        assert_eq!(playlist.toast_for_render(), "已加入 Up Next — Blue");
    }
    assert_eq!(playlist.toast_for_render(), "Added to Up Next — Blue");

    let mut queue = unlocked();
    let row = SongRow {
        title: "Blue".into(),
        ..Default::default()
    };
    queue.enqueue_at(row, 0, QueueAt::Later);
    assert_eq!(queue.toast, "Added to Up Next — Blue");
    {
        let _language = crate::i18n::enter(crate::i18n::Language::SimplifiedChinese);
        assert_eq!(queue.toast_for_render(), "已加入待播队列：Blue");
    }
    assert_eq!(queue.toast_for_render(), "Added to Up Next — Blue");
}

#[test]
fn a_new_playlist_from_the_picker_keeps_the_track() {
    let mut a = own_and_sony();
    while a.current() != Screen::NowPlaying {
        a.press(Button::Back);
    }
    let songs = a.lib.songs.clone();
    a.set_play_context(songs, 0);
    a.open_playlist_pick(900);
    assert!(
        a.tap(240, crate::playlist_pick::TOP + 4).is_empty(),
        "row 0 opens the keyboard"
    );
    assert_eq!(a.current(), Screen::Keyboard);
    kb_key(&mut a, crate::keyboard::Key::Char('x'));
    assert_eq!(
        kb_key(&mut a, crate::keyboard::Key::Done),
        vec![Action::PlaylistCreateWith(900)]
    );
    assert_ne!(
        a.current(),
        Screen::PlaylistPick,
        "the picker's question is answered, so it goes with the keyboard"
    );
}

#[test]
fn toolbar_navigation_four_slots() {
    let mut a = own_and_sony();
    while a.current() != Screen::NowPlaying {
        a.press(Button::Back);
    }
    // Slot 0 -> Library
    a.tap(
        crate::now_playing::TOOLBAR_CX[0],
        crate::now_playing::TOOLBAR_TOP + 20,
    );
    assert_eq!(a.current(), Screen::Library);
    a.press(Button::Back);

    // Slot 1 -> UpNext
    a.tap(
        crate::now_playing::TOOLBAR_CX[1],
        crate::now_playing::TOOLBAR_TOP + 20,
    );
    assert_eq!(a.current(), Screen::UpNext);
    a.press(Button::Back);

    // Slot 2 -> Bluetooth
    a.tap(
        crate::now_playing::TOOLBAR_CX[2],
        crate::now_playing::TOOLBAR_TOP + 20,
    );
    assert_eq!(a.current(), Screen::Bluetooth);
    a.press(Button::Back);

    // Slot 3 -> Settings
    a.tap(
        crate::now_playing::TOOLBAR_CX[3],
        crate::now_playing::TOOLBAR_TOP + 20,
    );
    assert_eq!(a.current(), Screen::Settings);
}

/// The track picker stays open across adds — building a playlist is a run of taps.
#[test]
fn the_track_picker_adds_without_leaving() {
    let mut a = own_and_sony();
    a.open_playlist(0);
    let (bx, by, bw, bh) = library::playlist_action_rect(0); // + TRACKS
    assert!(a.tap(bx + bw / 2, by + bh / 2).is_empty());
    assert_eq!(a.current(), Screen::TrackPick);

    // The picker lists the library in title order: T0..T3. T0 and T1 are already members.
    // Rows start below the fixed SEARCH BAND, not at TOP.
    let row = |i: i32| crate::playlist_pick::LIST_TOP + i * crate::playlist_pick::ROW_H + 4;
    assert!(
        a.tap(240, row(0)).is_empty(),
        "a track already in the playlist is not re-added"
    );
    assert_eq!(a.tap(240, row(2)), vec![Action::PlaylistAddTrack(-42, 902)]);
    assert_eq!(a.current(), Screen::TrackPick, "the picker stays open");
}

/// Searching narrows the "Add tracks" list, and the rows still resolve to the right songs.
///
/// 3,462 tracks at 64 px a row is ~220,000 px of scrolling, so an unfiltered flat list is a
/// way of browsing, not of finding. Reported 2026-09-10: "rework the +tracks to make it easier
/// to add and find songs".
#[test]
fn the_track_picker_can_be_searched() {
    let mut a = own_and_sony();
    a.open_playlist(0);
    let (bx, by, bw, bh) = library::playlist_action_rect(0);
    a.tap(bx + bw / 2, by + bh / 2);
    assert_eq!(a.current(), Screen::TrackPick);
    let all = a.track_pick_len_for_test();
    assert!(all > 1, "precondition: more than one track to filter");

    // The band is FIXED under the header, so it is reachable however far the list is scrolled.
    assert!(crate::playlist_pick::hit_search(
        crate::playlist_pick::TOP + 4
    ));
    assert!(!crate::playlist_pick::hit_search(
        crate::playlist_pick::LIST_TOP + 4
    ));
    assert!(a.tap(240, crate::playlist_pick::TOP + 4).is_empty());
    assert_eq!(a.current(), Screen::Keyboard, "the band opens the keyboard");

    // THE SCREEN SAYS WHAT IT IS. It said "New playlist" — the catch-all arm — which is why
    // the search looked broken even though the filter worked.
    assert_eq!(a.kb_title_for_test(), "Find a song");

    // Type a title that exists, and commit.
    // `type_for_test` fills the buffer directly — the fixture's titles ("T2") need both a
    // shift and a page flip, which is the keyboard's business and not this test's.
    let want = a.lib.songs[2].title.clone();
    a.type_for_test(&want);
    // FILTERED ALREADY, before DONE — the list narrows as you type, and the header counts.
    assert!(
        a.track_pick_len_for_test() < all,
        "typing must filter, without waiting for DONE"
    );
    assert_eq!(
        a.kb_title_for_test(),
        format!("{} matching", a.track_pick_len_for_test())
    );

    assert!(
        kb_key(&mut a, crate::keyboard::Key::Done).is_empty(),
        "a filter is not an Action"
    );
    assert_eq!(a.current(), Screen::TrackPick, "back to the list, filtered");
    assert!(
        a.track_pick_len_for_test() < all,
        "the search must actually narrow the list"
    );

    // The first row is now the match, and tapping it adds THAT track.
    let acts = a.tap(240, crate::playlist_pick::LIST_TOP + 4);
    assert!(
        acts.iter().any(
            |x| matches!(x, Action::PlaylistAddTrack(_, id) if *id == a.lib.songs[2].object_id)
        ),
        "a filtered row must resolve to the song it shows, got {acts:?}"
    );

    // Re-entering starts clean — the last search belonged to the last errand.
    a.press(Button::Back);
    a.tap(bx + bw / 2, by + bh / 2);
    assert_eq!(
        a.track_pick_len_for_test(),
        all,
        "a fresh visit is unfiltered"
    );
}

/// The open page follows its playlist when the list is re-sorted under it (a rename does that).
#[test]
fn the_open_playlist_page_follows_a_rename() {
    let mut a = own_and_sony();
    a.open_playlist(0); // "Mine", id -42, first of two
    let mut rows = a.lib.playlists.clone();
    rows[0].name = "Zzz last".into();
    rows.sort_by_key(|x| x.name.to_lowercase());
    a.set_playlists(rows);
    assert_eq!(
        a.playlist_row().map(|p| p.id),
        Some(-42),
        "the page must not change playlist"
    );
}

/// Tapping the row body opens that playlist's own page — the same shape the Artists tab has.
/// The row used to be a shortcut that played the whole list, which left no way to see what was
/// in one before committing to it.
#[test]
fn tapping_a_playlist_row_opens_its_page() {
    let mut a = two_playlists();
    let y = library::list_top(Tab::Playlists) + library::row_h(Tab::Playlists) + 4;
    assert!(a.tap(120, y).is_empty(), "opening a page emits no action");
    assert_eq!(a.current(), Screen::Playlist);
    assert_eq!(a.playlist_row().map(|p| p.id), Some(78));

    // A track row plays THE PLAYLIST from that row; the band's two halves play and shuffle it.
    let ty =
        library::playlist_content_top(a.playlist_row().unwrap()) + library::PLAYLIST_TRACK_RH() / 2;
    assert_eq!(a.tap(200, ty), vec![Action::PlayPlaylistAt(78, 0)]);
    assert_eq!(
        a.current(),
        Screen::Playlist,
        "playing must not leave the page"
    );
    let (px, py, pw, ph) = library::playlist_play_band();
    assert_eq!(
        a.tap(px + pw / 2, py + ph / 2),
        vec![Action::PlayPlaylist(78)]
    );
    let (sx, sy, sw, sh) = library::playlist_shuffle_band();
    assert_eq!(
        a.tap(sx + sw / 2, sy + sh / 2),
        vec![Action::ShufflePlaylist(78)]
    );

    // And Back returns to the tab.
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Library);
}

/// The page scrolls, and a right-swipe on one of its rows queues that track like every other
/// track list in the app.
#[test]
fn the_playlist_page_scrolls_and_queues() {
    let mut a = two_playlists();
    a.open_playlist(0);
    let ty =
        library::playlist_content_top(a.playlist_row().unwrap()) + library::PLAYLIST_TRACK_RH() / 2;
    assert_eq!(a.swipe(1, 240, ty), vec![Action::QueueChanged]);
    // Nothing was playing, so the queued track starts the list.
    assert_eq!(a.context().len(), 1);
    assert_eq!(a.context()[0].title, "P0");
    // Three rows do not overflow the panel, so there is nothing to scroll — and the scrollbar
    // must decline rather than eat the contact.
    assert_eq!(
        library::playlist_max_scroll_px(a.playlist_row().unwrap()),
        0
    );
    assert!(!a.sbar_begin(crate::canvas::W as i32 - 4, ty));
}

/// An empty Playlists tab must not emit an action for a tap on nothing.
#[test]
fn empty_playlists_tab_is_inert() {
    let mut a = unlocked();
    a.push(Screen::Library);
    a.lib_tab = Tab::Playlists;
    a.lib = crate::model::Library::default();
    assert!(a.press(Button::Select).is_empty());
}

#[test]
fn home_resets_to_now_playing() {
    let mut a = unlocked();
    a.press(Button::Up); // Menu
    a.press(Button::Down);
    a.press(Button::Select); // Library
    a.press(Button::Home);
    assert_eq!(a.current(), Screen::NowPlaying);
}

/// Settings ▸ Power ▸ Alarm: the big button turns it on and off, the ± buttons move the time,
/// and the shell only sees a time while it is on.
#[test]
fn the_alarm_screen_sets_a_time_and_turns_it_on() {
    let mut a = unlocked();
    a.push(Screen::Alarm);
    assert_eq!(a.alarm(), None, "off until asked");
    let (sx, sy, _, _) = crate::clockset::set_rect();
    a.tap(sx + 5, sy + 5);
    assert_eq!(a.alarm(), Some(crate::alarm::DEFAULT_MIN));
    let (px, py, _, _) = crate::clockset::btn_rect(0, 1); // hour +
    a.tap(px + 5, py + 5);
    let (mx, my, _, _) = crate::clockset::btn_rect(1, 0); // minute −
    a.tap(mx + 5, my + 5);
    assert_eq!(a.alarm(), Some(8 * 60 - 5));
    a.tap(sx + 5, sy + 5);
    assert_eq!(a.alarm(), None);
    assert_eq!(
        a.alarm_saved(),
        (false, 8 * 60 - 5),
        "the time is kept while off"
    );
}

/// Line out holds the headphone level, ends at the jack's next edge with the level brought
/// down, and is never read back from the settings file.
#[test]
fn line_out_fixes_the_volume_until_the_plug_changes() {
    let mut a = unlocked();
    a.volume = 100;
    a.set_opt(Opt::LineOut, true);
    a.press(Button::VolUp);
    a.press(Button::VolDown);
    assert_eq!(a.volume, 100, "the keys do not move it");
    assert!(a.wa_volume_to(0).is_empty(), "nor the skin's slider");
    assert!(a.line_out_jack_change(), "the level is lowered");
    assert!(!a.opt(Opt::LineOut));
    assert_eq!(a.volume, App::LINE_OUT_SAFE);
    assert!(!a.line_out_jack_change(), "off: an edge does nothing");
    a.press(Button::VolUp);
    assert_eq!(a.volume, App::LINE_OUT_SAFE + 1);
    // Already quiet: off, and left where it was.
    a.volume = 10;
    a.set_opt(Opt::LineOut, true);
    assert!(!a.line_out_jack_change());
    assert_eq!(a.volume, 10);
    assert!(!a.opt(Opt::LineOut));
    assert_eq!(Opt::from_key("line_out"), None, "never loaded at start");
}

#[test]
fn volume_keys_adjust_and_pop_hud() {
    let mut a = unlocked();
    let v0 = a.volume;
    assert_eq!(a.press(Button::VolUp), vec![Action::VolUp]);
    assert_eq!(a.volume, v0 + 1);
    assert!(a.vol_overlay > 0); // HUD showing
                                // HUD counts down and eventually hides
    for _ in 0..crate::overlay::VOL_FRAMES {
        a.tick();
    }
    assert_eq!(a.vol_overlay, 0);
    // never below 0
    a.set_volume(0);
    a.press(Button::VolDown);
    assert_eq!(a.volume, 0);
}

#[test]
fn visualiser_cycles_and_settings_control_it() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    let mut a = unlocked();
    // Option on Now Playing cycles the visualiser type
    let k0 = a.viz_kind();
    a.press(Button::Option);
    assert_eq!(a.viz_kind(), (k0 + 1) % crate::viz::COUNT);
    // route to Settings — by MENU lookup, not a press count — then into Display.
    a = open_from_menu(Screen::Settings);
    assert_eq!(a.current(), Screen::Settings);
    assert_eq!(a.settings_sel, crate::settings::ROW_DISPLAY);
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::Display);
    // cursor down to the Visualiser row. Walk to the constant rather than pressing a fixed
    // number of times, so a row inserted above it can't silently retarget this test.
    while a.display_sel < crate::display::ROW_VIZ {
        a.press(Button::Down);
    }
    assert_eq!(a.display_sel, crate::display::ROW_VIZ);
    // The row is a CHEVRON now: the style and cover-size cycles moved onto the Visualiser
    // screen, along with seven settings that never existed. Select opens it.
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::VizSet);
    let k1 = a.viz_kind();
    a.press(Button::Select); // row 0 is Style
    assert_eq!(a.viz_kind(), (k1 + 1) % crate::viz::COUNT);
    // Down to the cover-size row. OFF is index 0, so a full cycle must pass through
    // viz_on() == false exactly once and come home.
    a.press(Button::Down);
    assert_eq!(a.viz_sel, crate::vizset::ROW_COVER);
    let start = a.viz_size();
    let mut seen_off = false;
    for _ in 0..crate::viz::SIZE_COUNT {
        a.press(Button::Select);
        if a.viz_size() == 0 {
            seen_off = true;
            assert!(!a.viz_on(), "size 0 must read as off");
        } else {
            assert!(a.viz_on(), "any non-zero size must read as on");
        }
    }
    assert!(seen_off, "the cycle never offered OFF");
    assert_eq!(
        a.viz_size(),
        start,
        "the cycle did not return to where it started"
    );
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Display);
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Settings);
    // The cursor clamps at the last topic link on the home page…
    for _ in 0..30 {
        a.press(Button::Down);
    }
    let last = crate::settings::group_row(crate::settings::GROUPS.len() - 1);
    assert_eq!(a.settings_sel, last);
    // …which opens System, cursor on its first row, and its last row is the clamp there.
    a.press(Button::Select);
    assert_eq!(a.settings_page, Some(crate::settings::GROUPS.len() - 1));
    // (One language installed here, so no Language row: System opens on Boot to stock.)
    assert_eq!(a.settings_sel, crate::settings::ROW_BOOT_STOCK);
    for _ in 0..30 {
        a.press(Button::Down);
    }
    assert_eq!(a.settings_sel, crate::settings::ROW_MODEL);
    // Back is the home page, on System's link; Back again leaves Settings.
    a.press(Button::Back);
    assert_eq!(a.settings_page, None);
    assert_eq!(a.settings_sel, last);
    assert_eq!(a.current(), Screen::Settings);
    a.press(Button::Back);
    assert_ne!(a.current(), Screen::Settings);
}

/// Walk the Menu cursor to the row that leads to `want` and open it. Derived from MENU rather
/// than a fixed number of Down presses: a row inserted above the target used to silently send
/// every one of these tests to the wrong screen.
fn open_from_menu(want: Screen) -> App {
    let mut a = unlocked();
    a.press(Button::Up); // Menu
    let idx = a
        .menu_visible()
        .iter()
        .position(|m| m.0 == want)
        .expect("no menu row for {want:?}");
    for _ in 0..idx {
        a.press(Button::Down);
    }
    assert_eq!(a.menu_index(), idx);
    a.press(Button::Select);
    a
}

/// The Equalizer is opened from Sound's row since 2026-10-04 (it left the Menu).
fn enter_eq() -> App {
    let mut a = open_from_menu(Screen::Sound);
    a.push(Screen::Eq);
    assert_eq!(a.current(), Screen::Eq);
    a
}

#[test]
fn eq_band_select_and_adjust() {
    let mut a = enter_eq();
    a.press(Button::Right); // band 1
    let before = a.eq_bands[1];
    let acts = a.press(Button::Up);
    assert_eq!(
        a.eq_bands[1],
        (before + crate::eq::BAND_STEP).min(crate::eq::BAND_MAX)
    );
    assert!(matches!(acts.as_slice(), [Action::EqChanged(_)]));
    // Clamps symmetrically at the limit the slider field is DRAWN to — a knob that stops at
    // 60% of its column is the bug this replaces.
    for _ in 0..40 {
        a.press(Button::Up);
    }
    assert_eq!(a.eq_bands[1], crate::eq::BAND_MAX);
    for _ in 0..40 {
        a.press(Button::Down);
    }
    assert_eq!(a.eq_bands[1], -crate::eq::BAND_MAX);
}

#[test]
fn bluetooth_select_toggles() {
    let mut a = open_from_menu(Screen::Bluetooth);
    assert_eq!(a.current(), Screen::Bluetooth);
    let was = a.bt_on;
    let acts = a.press(Button::Select);
    assert_eq!(acts, vec![Action::BtToggle(!was)]);
    assert_eq!(a.bt_on, !was);
}

fn enter_bluetooth() -> App {
    let a = open_from_menu(Screen::Bluetooth);
    assert_eq!(a.current(), Screen::Bluetooth);
    assert!(a.bt_on);
    a
}

/// The codec controls now live one level down, behind "Audio quality" — the Bluetooth screen
/// itself is the paired-device list. Reaching them through the real tap keeps this honest: if
/// the row moves, these tests fail rather than testing a page nothing can open.
fn enter_bt_codec() -> App {
    let mut a = enter_bluetooth();
    let (_, ay, _, ah) = crate::bluetooth::advanced_row();
    assert!(
        a.tap(240, ay + ah / 2).is_empty(),
        "opening a page emits no action"
    );
    assert_eq!(
        a.current(),
        Screen::BtCodec,
        "Audio quality did not open the codec page"
    );
    a
}

/// The ON/OFF switch used to share an edge with the status strip, and `chrome::status_hit`
/// claims every pixel above that line — with "anywhere else → Menu" as its fallback. So a tap
/// slightly high did not miss the switch, it left the screen entirely. Reported 2026-08-19.
/// The same edge, on the control that matters most: the back chevron, which every screen with
/// a header draws. A tap a pixel high used to jump to Now Playing (the strip sends x < 80 to
/// `StatusTap::NowPlaying`) instead of going back.
#[test]
fn back_chevron_is_not_stolen_by_the_status_strip() {
    let mut a = open_from_menu(Screen::Bluetooth);
    assert_eq!(a.current(), Screen::Bluetooth);

    // In the dead band: nothing happens, and crucially we do not leave the screen.
    assert!(a.tap(30, crate::chrome::STATUS_H - 2).is_empty());
    assert_eq!(
        a.current(),
        Screen::Bluetooth,
        "a high back tap navigated away"
    );

    // On the chevron itself: back, as asked.
    a.tap(30, crate::chrome::STATUS_H + 4);
    assert_ne!(
        a.current(),
        Screen::Bluetooth,
        "the back chevron did not pop"
    );
}

#[test]
fn bluetooth_toggle_has_a_dead_band_under_the_status_strip() {
    let mut a = enter_bluetooth();
    let was = a.bt_on;

    // A near miss in the dead band is INERT — it must not toggle, and above all it must not
    // navigate. The old geometry opened the Menu from here.
    let acts = a.tap(430, crate::chrome::STATUS_H - 2);
    assert!(acts.is_empty(), "a near miss emitted {acts:?}");
    assert_eq!(a.bt_on, was, "a near miss flipped the radio");
    assert_eq!(a.current(), Screen::Bluetooth, "a near miss navigated away");

    // And the target itself is genuinely tall — the old one stopped at HEADER_BOTTOM (91).
    for y in [crate::chrome::STATUS_H, 70, 100, 120] {
        let before = a.bt_on;
        assert_eq!(
            a.tap(430, y),
            vec![Action::BtToggle(!before)],
            "y={y} missed the switch"
        );
        assert_eq!(a.bt_on, !before);
    }
}

#[test]
fn bluetooth_codec_and_quality_select() {
    let mut a = enter_bt_codec();
    // default codec = LDAC (0): the LDAC rows are visible. Pick 660 (quality index 2).
    assert_eq!(a.bt_codec, 0);
    assert_eq!(
        a.tap(240, crate::bluetooth::quality_row_y(2)),
        vec![Action::BtCodecChanged]
    );
    assert_eq!(a.bt_ldac_quality, 2);
    // Auto is drawn last but keeps its persisted index, 0.
    assert_eq!(
        a.tap(240, crate::bluetooth::quality_row_y(0)),
        vec![Action::BtCodecChanged]
    );
    assert_eq!(a.bt_ldac_quality, 0);
    assert_eq!(
        a.tap(240, crate::bluetooth::quality_row_y(2)),
        vec![Action::BtCodecChanged]
    );
    // select SBC (codec chip 3) → device-wide codec changes; the LDAC rows hide
    let (x, y) = (240, crate::bluetooth::codec_row_y(3, false, false));
    assert_eq!(a.tap(x, y), vec![Action::BtCodecChanged]);
    assert_eq!(a.bt_codec, 3);
    // with SBC active there are no LDAC rows: a tap where one was never picks a quality
    a.tap(240, crate::bluetooth::quality_row_y(1));
    assert_eq!(a.bt_ldac_quality, 2, "a hidden LDAC row answered");
}

/// Sony's "Use Enhanced Mode" (firmware message 230077) is the AVRCP absolute-volume switch.
/// It defaults ON because the OFF path sends VOLUME_UP/VOLUME_DOWN key events, which sinks
/// like the CMF Buds answer with their own feedback beep — and because Sony's own
/// SetCurrentVolume refuses to transmit while the preference is clear.
/// Pinning from the Albums accordion names the OPEN ALBUM, and restoring jumps to it —
/// recomputing the scroll from the album's current position rather than trusting a pixel
/// offset that a library rebuild would have invalidated.
#[test]
fn albums_pin_names_the_open_album_and_restores_as_a_jump_to() {
    let mut a = unlocked();
    a.go_for_preview(Screen::Library);
    a.lib_tab = Tab::Albums;
    a.album_expanded = None;
    // With nothing expanded the pin is just "the Albums tab".
    let (t, _) = a.place_label();
    assert_eq!(t, "Library");
    // Expand one, and the pin takes its name.
    let flat = 1usize;
    a.album_expanded = Some(flat);
    let want = a
        .lib
        .albums_flat()
        .get(flat)
        .map(|al| al.name.clone())
        .unwrap();
    let (t, sub) = a.place_label();
    assert_eq!(t, want);
    assert_eq!(sub, a.lib.albums_flat()[flat].artist.clone());
    // Pin it, scroll somewhere else, restore: the album comes back expanded and in view.
    let pin = a.capture_pin();
    a.lib_scroll_px = 4000;
    a.album_expanded = None;
    a.restore_pin(&pin);
    assert_eq!(a.album_expanded, Some(flat));
    assert_eq!(a.lib_tab, Tab::Albums);
    let rank = a.album_rank_of(flat).unwrap();
    let expect =
        crate::library::row_top_px(Tab::Albums, &a.lib, rank, a.album_sort, a.album_expanded);
    assert_eq!(a.lib_scroll_px, expect.clamp(0, a.lib_max_scroll()));
}

#[test]
fn bluetooth_enhanced_mode_toggles_and_is_inert_while_off() {
    let mut a = enter_bt_codec();
    let ey = crate::bluetooth::enhanced_row_y(true);
    assert!(a.bt_enhanced(), "enhanced mode defaults on");
    assert_eq!(a.tap(240, ey), vec![Action::BtEnhancedChanged]);
    assert!(!a.bt_enhanced());
    assert_eq!(a.tap(240, ey), vec![Action::BtEnhancedChanged]);
    assert!(a.bt_enhanced());
    // Radio off ⇒ the whole panel is inert, this row included. The switch is on the Bluetooth
    // screen, so go back for it and return.
    a.press(Button::Back);
    a.tap(430, 64); // header toggle
    assert!(!a.bt_on);
    a.push(Screen::BtCodec);
    assert!(a.tap(240, ey).is_empty());
    assert!(a.bt_enhanced(), "an inert tap must not flip the preference");
}

/// Support is a fact from the radio, not a preference: it only repaints when it CHANGES, so
/// the shell's per-connect push can't dirty a frame on every poll.
#[test]
fn bluetooth_enhanced_supported_reports_change_only() {
    let mut a = enter_bluetooth();
    assert!(!a.set_bt_enhanced_supported(true), "already true");
    assert!(a.set_bt_enhanced_supported(false));
    assert!(!a.set_bt_enhanced_supported(false));
}

#[test]
fn usb_dac_toggles_ldac_routing() {
    let mut a = open_from_menu(Screen::UsbDac);
    assert_eq!(a.current(), Screen::UsbDac);
    assert!(!a.usb_dac_on());
    // A stray tap in the body must NOT engage it: this switches the USB gadget mode and
    // starts the LDAC bridge, so it may only happen on the switch itself.
    assert!(a.tap(240, 300).is_empty(), "a body tap engaged USB-DAC");
    assert!(!a.usb_dac_on());
    // The switch engages USB-DAC → routes to 3.5mm + BT/LDAC (no BT disconnect).
    let sw = (441, 65); // centre of the drawn toggle
    assert_eq!(a.tap(sw.0, sw.1), vec![Action::UsbDacToggle(true)]);
    assert!(a.usb_dac_on());
    // tapping again disengages
    assert_eq!(a.tap(sw.0, sw.1), vec![Action::UsbDacToggle(false)]);
    assert!(!a.usb_dac_on());
    // The target is padded to ≥44px around the small drawn switch.
    assert!(crate::usbdac::hit_toggle(sw.0 - 20, sw.1 - 18));
    assert!(crate::usbdac::hit_toggle(sw.0 + 15, sw.1 + 18));
}

#[test]
fn low_latency_is_chosen_before_dac_mode_and_turns_off_after_it() {
    let row = (240, 128);
    let mut a = open_from_menu(Screen::UsbDac);
    a.tap(row.0, row.1);
    assert!(
        !a.usb_dac_low_latency(),
        "no module installed: the row is not there"
    );
    a.set_usb_dac_low_latency(true, false);
    a.tap(row.0, row.1);
    assert!(a.usb_dac_low_latency());
    a.tap(441, 65); // DAC on
    a.tap(row.0, row.1);
    assert!(
        a.usb_dac_low_latency(),
        "the row does nothing while DAC mode is on"
    );
    a.tap(441, 65); // DAC off: the shell unloads the module and says so
    a.set_usb_dac_low_latency(true, false);
    a.tap(row.0, row.1);
    assert!(
        !a.usb_dac_low_latency(),
        "outside DAC mode the switch turns off, no restart"
    );
}

#[test]
fn settings_usb_mode_enters_mass_storage() {
    let mut a = open_from_menu(Screen::Settings);
    assert_eq!(a.current(), Screen::Settings);
    walk_to_setting(&mut a, crate::settings::ROW_USB_MODE);
    assert_eq!(a.settings_sel, crate::settings::ROW_USB_MODE);
    assert_eq!(a.press(Button::Select), vec![Action::EnterUsbMsc]);
}

#[test]
fn bluetooth_codec_persists_round_trip() {
    let mut a = unlocked();
    a.set_bt_codec(2); // aptX
    a.set_bt_ldac_quality(3); // 330
    assert_eq!(a.bt_codec(), 2);
    assert_eq!(a.bt_ldac_quality(), 3);
    a.set_bt_codec(99); // wraps into range, never panics
    assert!((a.bt_codec() as usize) < crate::bluetooth::CODECS.len());
}

/// Settings ▸ Database asks for a rescan, says so while one is out, and STOPS saying so.
///
/// The stuck-label case is the one worth pinning: a scan that finds nothing changed writes
/// nothing, so no library ever comes back, and a label cleared only by `set_library` would read
/// "Rescanning…" for the rest of the session exactly when the answer was "already up to date".
#[test]
fn the_database_row_asks_for_a_rescan_and_the_label_always_recovers() {
    let mut a = open_from_menu(Screen::Settings);
    walk_to_setting(&mut a, crate::settings::ROW_DATABASE);
    assert_eq!(a.settings_sel, crate::settings::ROW_DATABASE);
    assert!(!a.rescanning(), "nothing has been asked for yet");

    assert_eq!(a.press(Button::Select), vec![Action::RescanLibrary]);
    assert!(a.rescanning());
    assert_eq!(a.database_label(), "Rescanning…");

    // Exit 1: a reloaded library arrives (the scan found something).
    a.set_library(Library::default());
    assert!(!a.rescanning(), "a reloaded library ends the rescan state");
    assert_eq!(
        a.database_label(),
        "Empty",
        "no tracks -> Empty, never a bare number"
    );

    // Exit 2: the scan changed nothing, so no library ever comes back. cinder-ffi's deadline
    // clears it; the label must not be able to stick on its own.
    a.press(Button::Select);
    assert!(a.rescanning());
    a.set_rescanning(false);
    assert_ne!(
        a.database_label(),
        "Rescanning…",
        "the label outlived the scan"
    );
}

#[test]
fn settings_select_toggles_theme() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    // Night lives on Settings ▸ Display now: the first Settings row opens it.
    let mut a = open_from_menu(Screen::Settings);
    assert_eq!(a.current(), Screen::Settings);
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::Display);
    while a.display_sel < crate::display::ROW_NIGHT {
        a.press(Button::Down);
    }
    let was = a.night;
    let acts = a.press(Button::Select);
    assert_eq!(acts, vec![Action::ThemeChanged(!was)]);
    assert_eq!(a.night, !was);
}

#[test]
fn the_settings_battery_row_opens_the_battery_screen_instead_of_toggling() {
    let mut a = open_from_menu(Screen::Settings);
    assert_eq!(a.current(), Screen::Settings);
    // move cursor to the Battery row
    walk_to_setting(&mut a, crate::settings::ROW_BATTERY);
    assert_eq!(a.settings_sel, crate::settings::ROW_BATTERY);
    let was = a.battery_care();
    let acts = a.press(Button::Select);
    // The row is a chevron now. It must NOT flip the setting on the way in — that was the old
    // behaviour, and a drill-in row that also changes something is how you turn battery care
    // off by trying to look at it.
    assert!(
        acts.is_empty(),
        "opening the screen must not change anything"
    );
    assert_eq!(a.battery_care(), was);
    assert_eq!(a.current(), Screen::Device);
}

/// Night on a schedule acts at its two edges only, like a phone's: on at Starts, off at
/// Ends, a hand on the switch in between holds, and nothing happens with it off.
#[test]
fn night_follows_its_schedule_at_the_edges_only() {
    use crate::display::{in_night_window, next_time, NIGHT_ENDS, NIGHT_STARTS};
    let mut a = unlocked();
    let at = |h: u16, m: u16| h * 60 + m;
    assert!(a.night_schedule_tick(at(23, 0)).is_empty(), "off: nothing");
    assert!(!a.night);
    a.set_night_schedule(true, at(21, 0), at(7, 0));
    assert_eq!(
        a.night_schedule_tick(at(20, 59)),
        vec![],
        "before Starts, already day"
    );
    assert_eq!(
        a.night_schedule_tick(at(21, 0)),
        vec![Action::ThemeChanged(true)]
    );
    assert!(a.night);
    a.night = false; // switched off by hand at 23:00
    assert_eq!(
        a.night_schedule_tick(at(23, 0)),
        vec![],
        "a hand holds till the next edge"
    );
    assert!(!a.night);
    a.night = true; // and back on by hand
    assert_eq!(
        a.night_schedule_tick(at(7, 0)),
        vec![Action::ThemeChanged(false)]
    );
    // A boot inside the window wakes into Night.
    let mut b = unlocked();
    b.set_night_schedule(true, at(21, 0), at(7, 0));
    assert_eq!(
        b.night_schedule_tick(at(2, 30)),
        vec![Action::ThemeChanged(true)]
    );
    // The window crosses midnight; the rows step through their presets and come round.
    assert!(in_night_window(at(0, 0), at(21, 0), at(7, 0)));
    assert!(!in_night_window(at(12, 0), at(21, 0), at(7, 0)));
    assert_eq!(next_time(&NIGHT_STARTS, at(23, 30)), at(18, 0));
    assert_eq!(next_time(&NIGHT_ENDS, at(7, 0)), at(7, 30));
    assert_eq!(
        next_time(&NIGHT_ENDS, 1),
        at(5, 0),
        "an odd time goes to the first"
    );
}

/// Display ▸ Night is three chips. Auto brings Starts and Ends onto the page; Off and On take
/// them away again and set Night there and then.
#[test]
fn the_night_chips_pick_off_on_or_the_schedule() {
    use crate::display::{night_chip_at, row_at, row_top, shown, ROW_NIGHT, ROW_NIGHT_FROM};
    let mut a = open_from_menu(Screen::Settings);
    a.go_for_preview(Screen::Display);
    let chip = |i: usize| {
        let (x, w) = crate::kit::chip_span(i, 3);
        let y = row_top(ROW_NIGHT) + crate::kit::ROW_H() / 2;
        assert_eq!(night_chip_at(x + w / 2, y), Some(i));
        (x + w / 2, y)
    };
    assert!(!shown(ROW_NIGHT_FROM), "no times until Auto");
    let (x, y) = chip(1);
    assert_eq!(a.tap(x, y), vec![Action::ThemeChanged(true)]);
    assert!(a.night);
    let (x, y) = chip(2);
    assert_eq!(a.tap(x, y), vec![], "Auto waits for the clock");
    assert!(shown(ROW_NIGHT_FROM));
    let from = row_top(ROW_NIGHT_FROM) + 10;
    assert_eq!(row_at(from), Some(ROW_NIGHT_FROM));
    a.tap(240, from);
    assert_eq!(
        a.night_schedule().1,
        21 * 60 + 30,
        "Starts steps on half an hour"
    );
    let (x, y) = chip(0);
    assert_eq!(a.tap(x, y), vec![Action::ThemeChanged(false)]);
    assert!(!shown(ROW_NIGHT_FROM) && !a.night_schedule().0);
}

/// #51 follow-up: "Make it yours" in the introduction flips the same switches Settings does.
#[test]
fn the_introduction_lets_you_pick_the_settings_people_change_most() {
    use crate::onboarding::{choice_rows, Choice, CHOICES_PAGE};
    let _scale = lock_scale();
    let mut a = unlocked();
    a.start_onboarding();
    a.press(Button::Select);
    assert_eq!(a.onboarding_page, CHOICES_PAGE);
    let row_y = |c: Choice| {
        let i = choice_rows()
            .iter()
            .position(|r| *r == c)
            .expect("row shown");
        150 + i as i32 * crate::kit::ROW_H() + 10
    };
    assert!(!crate::scale::large());
    assert!(
        !choice_rows().contains(&Choice::Bar),
        "the extras wait for Big view"
    );
    assert_eq!(a.tap(400, row_y(Choice::Big)), vec![Action::UiScaleChanged]);
    assert!(crate::scale::large(), "Big view is on");
    assert_eq!(
        a.onboarding_page, CHOICES_PAGE,
        "a row does not turn the page"
    );
    assert_eq!(a.tap(400, row_y(Choice::Bar)), vec![Action::UiScaleChanged]);
    assert!(crate::scale::big_bar());
    a.tap(400, row_y(Choice::Quick));
    assert!(a.opt(Opt::Quick));
    a.tap(400, row_y(Choice::Folders));
    assert!(a.opt(Opt::FoldersTab));
    // Off the rows the page turns as every other page does.
    a.tap(400, 760);
    assert_eq!(a.onboarding_page, CHOICES_PAGE + 1);
    a.set_big_view(false);
    crate::scale::set_big_bar(false);
}

/// Walk the Settings cursor to `row` the way a user does, from the top of the home page: down
/// to the row, or to its topic's link, Select, and down the page.
fn walk_to_setting(a: &mut App, row: usize) {
    let lang = a.settings_lang();
    let press_to = |a: &mut App, target: usize, page: Option<usize>| {
        let at = crate::settings::page_rows(page, lang)
            .iter()
            .position(|r| *r == target)
            .expect("the row is on this page");
        for _ in 0..at {
            a.press(Button::Down);
        }
    };
    match crate::settings::page_of(row) {
        None => press_to(a, row, None),
        Some(g) => {
            press_to(a, crate::settings::group_row(g), None);
            a.press(Button::Select);
            press_to(a, row, Some(g));
        }
    }
    assert_eq!(a.settings_sel, row);
}

/// Open Settings ▸ Device the way a user does.
fn open_device(a: &mut App) {
    walk_to_setting(a, crate::settings::ROW_BATTERY);
    a.press(Button::Select);
    assert_eq!(a.current(), Screen::Device);
}

#[test]
fn battery_care_toggles_from_its_row_on_the_device_screen() {
    let mut a = open_from_menu(Screen::Settings);
    open_device(&mut a);

    let was = a.battery_care();
    let y = crate::device::care_row_y(&a.device_view(), a.device_scroll_px).unwrap()
        + crate::device::ROW_H() / 2;
    let acts = a.tap(240, y);
    assert_eq!(acts, vec![Action::BatteryCareChanged(!was)]);
    assert_eq!(a.battery_care(), !was);

    // shell can push the device's real state back in
    a.set_battery_care(false);
    assert!(!a.battery_care());
}

#[test]
fn every_row_but_the_care_switch_is_inert() {
    let mut a = open_from_menu(Screen::Settings);
    open_device(&mut a);
    let was = a.battery_care();
    let care_top = crate::device::care_row_y(&a.device_view(), 0).unwrap();
    // Walk the whole visible column. Everything except the care row is a reading, and a tap on
    // one must do nothing — in particular it must not fall through to the toggle.
    let mut y = crate::chrome::HEADER_BOTTOM + 4;
    while y < crate::canvas::H as i32 {
        if !(care_top..care_top + crate::device::ROW_H()).contains(&y) {
            assert!(a.tap(240, y).is_empty(), "y={y} should be inert");
        }
        y += 9;
    }
    assert_eq!(a.battery_care(), was);
}

#[test]
fn the_care_switch_follows_the_screen_when_it_is_scrolled() {
    // The bug this guards: a hit test that uses the unscrolled layout while the render uses the
    // scrolled one. After scrolling, the OLD position must be dead and the new one live.
    let mut a = open_from_menu(Screen::Settings);
    open_device(&mut a);
    let before = crate::device::care_row_y(&a.device_view(), 0).unwrap();
    a.scroll_px(140);
    assert!(
        a.device_scroll_px > 0,
        "the device screen must actually scroll"
    );
    let shifted = before - a.device_scroll_px;
    let was = a.battery_care();
    assert!(
        a.tap(240, before + crate::device::ROW_H() / 2).is_empty()
            || shifted + crate::device::ROW_H() > before,
        "the pre-scroll position must no longer be the switch"
    );
    a.set_battery_care(was);
    let acts = a.tap(240, shifted + crate::device::ROW_H() / 2);
    assert_eq!(
        acts,
        vec![Action::BatteryCareChanged(!was)],
        "the switch moved with the render"
    );
}

#[test]
fn the_shell_can_push_every_device_reading_through() {
    let mut a = unlocked();
    a.set_battery_detail(83, "Charging", "Good", 4091, 1, 0, "10 AC 78");
    a.set_device_temps(34400, 39365, 34400);
    a.set_device_cpu(1300000, 1300000, 1, 2, "hotplug");
    a.set_device_storage(467512, 159772, 56320, 1024, 13);
    a.set_device_system(711, "3.10.26");
    // The formatting itself is tested in `device`; this asserts the PLUMBING, which is the part
    // that can silently not exist.
    let v = a.device_view();
    assert_eq!(v.percent, 83);
    assert_eq!(v.temp_pmic, 39365);
    assert_eq!(v.cpu_khz, 1300000);
    assert_eq!(v.governor, "hotplug");
    assert_eq!(v.music_free_mb, 1024);
    assert_eq!(v.kernel, "3.10.26");
    assert_eq!(a.device_summary(), "83% · 34.4 °C");

    // The card mounts after the library is built: its bar appears when the bridge reads it.
    a.set_sd_space(0, 0);
    assert_eq!(a.storage_view().sum.sd_total_mb, 0);
    assert!(a.set_sd_space(243_000, 101_000));
    assert!(
        !a.set_sd_space(243_000, 101_000),
        "the same reading is no change"
    );
    assert_eq!(
        (
            a.storage_view().sum.sd_total_mb,
            a.storage_view().sum.sd_free_mb
        ),
        (243_000, 101_000)
    );

    // A gauge reporting over 100 must not be stored as-is; the bar would draw past its box.
    a.set_battery_detail(255, "Full", "Good", 4100, 2, 0, "");
    assert_eq!(a.device_view().percent, 100);
}

/// The Bluetooth vernier: with fine volume on, most presses must move the trim and NOT the
/// sink, the trim must never sit outside (-sub, 0], and a full cycle of presses must land back
/// where it started having moved the sink exactly one step.
#[test]
fn bt_fine_volume_subdivides_one_avrcp_step() {
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_volume(40);
    a.set_bt_fine_span(2); // ±2 dB span -> 4 half-dB sub-steps per AVRCP step
    let start_level = a.bt_volume_level();
    let mut sink_moves = 0;
    for _ in 0..4 {
        if a.vol_step(false) {
            sink_moves += 1;
        }
        assert!(a.bt_trim_half_db() <= 0, "the trim only ever attenuates");
        assert!(a.bt_trim_half_db() >= -4, "the trim never exceeds its span");
    }
    assert_eq!(
        sink_moves, 1,
        "four fine presses must cost exactly one AVRCP step"
    );
    assert_eq!(a.bt_volume_level(), start_level - 1);
    // And back up: the same four presses return to the starting level with no trim left.
    for _ in 0..4 {
        a.vol_step(true);
    }
    assert_eq!(a.bt_volume_level(), start_level);
    assert_eq!(a.bt_trim_half_db(), 0);
}

/// Every fine press must move something the user can SEE, or the rocker reads as dropping
/// presses. The BAR cannot show it — one fine step is a quarter of a unit on a 0..120 bar — so
/// the trim readout is what has to change, and it must change on every single press.
#[test]
fn bt_fine_presses_move_the_hud() {
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_volume(60);
    a.set_bt_fine_span(3);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..6 {
        a.vol_step(false);
        // (bar, trim) together are what is drawn; the pair must be different every press.
        assert!(
            seen.insert((a.display_volume(), a.bt_trim_half_db())),
            "a fine press produced no visible change"
        );
    }
}

/// The trim rides on the EQ, so a curve with no headroom left must fall back to whole steps
/// rather than clamping bands and tilting the curve as the volume moves.
#[test]
fn bt_fine_backs_off_when_the_eq_has_no_headroom() {
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_volume(40);
    a.set_bt_fine_span(3);
    a.set_eq_bands([0, 0, 0, -crate::eq::BAND_MAX, 0, 0, 0, 0, 0, 0]); // one band at the floor
    assert!(
        a.vol_step(false),
        "with no headroom the sink must take the press"
    );
    assert_eq!(
        a.bt_trim_half_db(),
        0,
        "no trim may be applied without headroom"
    );
}

/// A trim is an invisible attenuation applied through the global EQ; it must not survive the
/// route going back to the jack.
#[test]
fn leaving_bluetooth_clears_the_trim() {
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_fine_span(2);
    a.vol_step(false);
    assert!(a.bt_trim_half_db() < 0);
    a.set_bt_route(false);
    assert_eq!(a.bt_trim_half_db(), 0, "the jack would have played quiet");
}

/// Fine volume OFF is exactly the old behaviour: every press is one AVRCP step.
#[test]
fn bt_fine_off_is_one_step_per_press() {
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_volume(40);
    a.set_bt_fine_span(0);
    assert!(a.vol_step(false));
    assert_eq!(a.bt_volume_level(), 39);
    assert_eq!(a.bt_trim_half_db(), 0);
}

#[test]
fn restore_eq_and_sound_round_trip() {
    let mut a = unlocked();
    // 99/-99 clamp to the range the EQ screen can reach and the DSP honours: ±BAND_MAX, i.e.
    // ±20 half-dB units = ±10 dB. This asserted ±6 — three dB — which is where the loader's
    // clamp used to be, so a band the user had genuinely set to 8 dB came back at 3 on the next
    // boot and the test agreed with the bug.
    a.set_eq_bands([3, -2, 6, -6, 0, 1, 99, -99, 4, 5]);
    assert_eq!(a.eq_bands(), [3, -2, 6, -6, 0, 1, 20, -20, 4, 5]);
    // And a value the EDITOR can reach must survive the round trip untouched.
    a.set_eq_bands([20, -20, 12, -12, 0, 0, 0, 0, 0, 0]);
    assert_eq!(a.eq_bands(), [20, -20, 12, -12, 0, 0, 0, 0, 0, 0]);
    a.set_sound_flags(0b101101); // dsee, vpt, dc, clear
    let f = a.sound_flags();
    assert_eq!(f, 0b101101);
    assert_eq!(f & 1, 1); // dsee
    assert_eq!(f & (1 << 4), 0); // normalizer off
}

#[test]
fn settings_sleep_timer_cycles_presets() {
    let mut a = open_from_menu(Screen::Settings);
    walk_to_setting(&mut a, crate::settings::ROW_SLEEP);
    assert_eq!(a.settings_sel, crate::settings::ROW_SLEEP);
    assert_eq!(a.sleep_min(), 0);
    assert_eq!(a.sleep_label(), "OFF");
    // Off -> 15 -> 30
    assert_eq!(a.press(Button::Select), vec![Action::SleepTimer(15)]);
    assert_eq!(a.sleep_min(), 15);
    assert_eq!(a.sleep_label(), "15 MIN");
    assert_eq!(a.press(Button::Select), vec![Action::SleepTimer(30)]);
    assert_eq!(a.sleep_min(), 30);
    // ffi pushes the live countdown back
    a.set_sleep_min(29);
    assert_eq!(a.sleep_label(), "29 MIN");
    a.set_sleep_min(0); // expired
    assert_eq!(a.sleep_label(), "OFF");
}

/// Sleep ▸ End of song ("stop after current", SPEC_queue_v2 §4): the last preset in the cycle
/// arms `stop_after` instead of a countdown, survives the FFI's "0 minutes left" push, and the
/// shell's `stop_after_done` puts the row back to Off.
#[test]
fn end_of_song_is_the_last_sleep_preset() {
    let mut a = unlocked();
    let last = crate::quick::SLEEP_PRESETS.len() - 1;
    let mut acts = Vec::new();
    for _ in 0..last {
        acts = a.set_sleep_choice((a.sleep_idx + 1) % crate::quick::SLEEP_PRESETS.len());
    }
    assert_eq!(
        acts,
        vec![Action::SleepTimer(0)],
        "any running countdown is cancelled"
    );
    assert!(a.stop_after());
    assert_eq!(a.sleep_label(), "END OF SONG");
    a.set_sleep_min(0); // the FFI's countdown reporting nothing left must not disarm it
    assert!(a.stop_after());
    assert_eq!(a.sleep_idx, last);
    a.stop_after_done();
    assert!(!a.stop_after());
    assert_eq!(a.sleep_label(), "OFF");
    // …and the next press starts the cycle again at 15.
    assert_eq!(
        a.set_sleep_choice((a.sleep_idx + 1) % crate::quick::SLEEP_PRESETS.len()),
        vec![Action::SleepTimer(15)]
    );
    // Picking a length disarms it.
    a.set_sleep_choice(last);
    a.set_sleep_choice(1);
    assert!(!a.stop_after());
}

#[test]
fn touch_tap_navigates() {
    let mut a = App::unlocked();
    assert_eq!(a.current(), Screen::NowPlaying);
    // The far-left clock/badge zone is the ONE-TAP return to Now Playing (the badge is the
    // now-playing indicator); the ☰ opens the Menu; the rest of the strip does nothing (#51).
    a.tap(20, 16); // over the clock → Now Playing (already there, so it stays)
    assert_eq!(a.current(), Screen::NowPlaying);
    a.tap(240, 16); // mid strip → nothing
    assert_eq!(a.current(), Screen::NowPlaying);
    a.tap(338, 16); // the ☰ → Menu
    assert_eq!(a.current(), Screen::Menu);
    a.tap(200, 91 + 63 + 8); // leave Menu (into Library) so the next assert is meaningful
    a.tap(338, 22); // the ☰ icon → Menu too
    assert_eq!(a.current(), Screen::Menu);
    // tap the Library row (row 1: y0=91 + 1*63 + a bit)
    a.tap(200, 91 + 63 + 8);
    assert_eq!(a.current(), Screen::Library);
    // tap the Albums tab, then the header-back chevron → back to Menu
    a.tap(180, 105);
    assert_eq!(a.lib_tab, Tab::Albums);
    a.tap(20, 60);
    assert_eq!(a.current(), Screen::Menu);
}

/// Settings ▸ Visualiser ▸ Bands: a tap on the row steps 36 → 48 → 64 → 12 → 24 → 36, the
/// signal settings carry it, and the settings file brings it back.
#[test]
fn the_bands_row_steps_through_the_counts_and_is_kept() {
    let mut a = unlocked();
    a.stack = vec![Screen::NowPlaying, Screen::VizSet];
    assert_eq!(a.viz_cfg().bands, 36, "an upgrade draws what it always did");
    let y = crate::vizset::TOP + crate::vizset::ROW_BANDS as i32 * crate::vizset::ROW_H + 20;
    let mut seen = Vec::new();
    for _ in 0..5 {
        a.tap(240, y);
        seen.push(a.viz_cfg().bands);
    }
    assert_eq!(seen, [48, 64, 12, 24, 36]);
    a.tap(240, y);
    let mut b = unlocked();
    b.set_viz_bands(a.viz_bands_idx());
    assert_eq!(b.viz_cfg().bands, 48);
}

/// A style is answered where it draws: every control, the seek rail and the page swipe follow
/// its layout, and the literal-coordinate test above keeps holding for Cinder.
#[test]
fn each_style_answers_taps_where_it_draws_its_controls() {
    use crate::now_playing::{layout, Hit};
    use crate::style::Style;
    for style in Style::ALL {
        let l = layout(style, false, false);
        let mid = |h: Hit| {
            l.at(h)
                .map(|s| s.centre())
                .unwrap_or_else(|| panic!("{style:?}: no {h:?}"))
        };
        let mut a = unlocked();
        a.set_style(style);
        for (h, want) in [
            (Hit::PlayPause, Action::PlayPause),
            (Hit::Prev, Action::Prev),
            (Hit::Next, Action::Next),
            (Hit::Shuffle, Action::ShuffleToggle),
            (Hit::Repeat, Action::RepeatCycle),
            (Hit::Like, Action::ToggleLiked),
        ] {
            let (x, y) = mid(h);
            assert_eq!(a.tap(x, y), vec![want], "{style:?}: {h:?} at ({x}, {y})");
        }
        let (x, y) = mid(Hit::Info);
        a.tap(x, y);
        assert_eq!(a.current(), Screen::TrackInfo, "{style:?}: the title block");
        for (slot, screen) in [
            (0, Screen::Library),
            (1, Screen::UpNext),
            (3, Screen::Settings),
        ] {
            let mut b = unlocked();
            b.set_style(style);
            let (x, y) = mid(Hit::Toolbar(slot));
            b.tap(x, y);
            assert_eq!(b.current(), screen, "{style:?}: toolbar slot {slot}");
        }
        // The rail: grabbed in its band, and the far end is the end of the song.
        let mut b = unlocked();
        b.set_style(style);
        let r = l.rail;
        assert!(
            b.scrub_begin(r.x0, (r.grab_top + r.grab_bot) / 2),
            "{style:?}: the rail"
        );
        b.scrub_move(r.x0 + r.w + 30, r.grab_top);
        assert_eq!(
            b.scrub_end(),
            vec![Action::Seek(1000)],
            "{style:?}: past the end is the end"
        );
        // The page block turns the page; below it, a swipe skips.
        let mut b = unlocked();
        b.set_style(style);
        assert!(
            b.swipe(-1, 240, (l.page_top + l.page_bot) / 2).is_empty(),
            "{style:?}: page swipe"
        );
        assert_eq!(b.np_page, 1);
        assert_eq!(
            b.swipe(-1, 240, l.page_bot + 10),
            vec![Action::Next],
            "{style:?}: skip swipe"
        );
    }
}

/// Display ▸ Style: a chip picks the style, Select steps through them, and `style=` in the
/// settings file brings it back — while an unknown word keeps what was there.
#[test]
fn a_cassette_tape_turns_now_playing_on_its_side() {
    let mut a = unlocked();
    a.set_style(crate::style::Style::Cassette);
    assert!(!a.tape_view(), "no tape: Cinder's drawn cassette");
    a.set_tape_loaded(true);
    assert!(a.tape_view());
    // Landscape (lx, ly) on the panel.
    let at = |lx: i32, ly: i32| (479 - ly, lx);
    let (x, y) = at(100, 300);
    assert_eq!(a.tap(x, y), vec![Action::Prev]);
    let (x, y) = at(700, 300);
    assert_eq!(a.tap(x, y), vec![Action::Next]);
    let (x, y) = at(400, 300);
    assert_eq!(a.tap(x, y), vec![Action::PlayPause]);
    let (x, y) = at(400, 300);
    assert!(!a.scrub_begin(x, y), "nothing to drag on a tape");
    assert_eq!(a.swipe(-1, x, y), vec![]);
    let (x, y) = at(790, 20);
    a.tap(x, y);
    assert_eq!(
        a.current(),
        Screen::Menu,
        "the corner Wampy keeps its settings in"
    );
    a.go(Screen::NowPlaying);
    // The status strip (the panel's top, the view's left edge) is the view's: a tap at the
    // left middle is ◁, not the strip's Menu.
    let (x, y) = at(10, 240);
    assert_eq!(a.tap(x, y), vec![Action::Prev]);
    assert!(!a.shelf_swipe_open(), "no Shelf on its side");
}

#[test]
fn the_winamp_skin_view_answers_every_control() {
    use crate::winamp::{self, Ctl};
    let mut a = unlocked();
    a.set_style(crate::style::Style::Winamp);
    // No skin: Terminal's targets, so the transport is where Terminal draws it.
    assert!(!a.winamp_view());
    a.set_winamp_skin(true);
    assert!(a.winamp_view());
    // A point of the layout on the panel: the inverse of `winamp::to_land`.
    let at = |lx: i32, ly: i32| (lx, ly);
    // The middle of everywhere that answers as `ctl`.
    let centre = |ctl: Ctl, shade: bool| {
        let pts: Vec<(i32, i32)> = (0..winamp::LW)
            .step_by(2)
            .flat_map(|x| (0..winamp::LH).step_by(2).map(move |y| (x, y)))
            .filter(|&(x, y)| winamp::ctl_at(x, y, shade) == Some(ctl))
            .collect();
        let n = pts.len().max(1) as i32;
        let sum = pts.iter().fold((0, 0), |s, p| (s.0 + p.0, s.1 + p.1));
        at(sum.0 / n, sum.1 / n)
    };
    let tap = |a: &mut App, ctl: Ctl| {
        let (x, y) = centre(ctl, a.wa_playlist);
        a.tap(x, y)
    };
    a.playing = true;
    assert_eq!(
        tap(&mut a, Ctl::Play),
        vec![],
        "▶ while playing does nothing"
    );
    assert_eq!(tap(&mut a, Ctl::Pause), vec![Action::PlayPause]);
    assert_eq!(tap(&mut a, Ctl::Play), vec![Action::PlayPause]);
    assert_eq!(tap(&mut a, Ctl::Stop), vec![Action::Stop]);
    assert!(!a.playing);
    assert_eq!(tap(&mut a, Ctl::Next), vec![Action::Next]);
    assert_eq!(tap(&mut a, Ctl::Prev), vec![Action::Prev]);
    assert_eq!(tap(&mut a, Ctl::Shuffle), vec![Action::ShuffleToggle]);
    assert_eq!(tap(&mut a, Ctl::Repeat), vec![Action::RepeatCycle]);
    assert_eq!(tap(&mut a, Ctl::Double), vec![Action::NextSkin]);
    tap(&mut a, Ctl::Time);
    assert!(a.wa_remaining);
    for want in [1, 2, 0] {
        tap(&mut a, Ctl::Vis);
        assert_eq!(a.wa_vis, want, "spectrum, oscilloscope, off");
    }
    tap(&mut a, Ctl::VisMenu);
    assert_eq!(a.wa_style, 1, "V steps the analyzer's look");
    let saved = a.winamp_prefs();
    let mut b = unlocked();
    b.set_winamp_prefs(&saved);
    assert_eq!(
        (b.wa_remaining, b.wa_vis, b.wa_style),
        (true, 0, 1),
        "and they survive a restart"
    );
    b.set_winamp_prefs("junk");
    assert_eq!(b.wa_style, 1, "an unreadable line changes nothing");
    for (ctl, screen) in [
        (Ctl::Eject, Screen::Library),
        (Ctl::Menu, Screen::Menu),
        (Ctl::Close, Screen::Library),
        (Ctl::Options, Screen::Settings),
        (Ctl::OnTop, Screen::Display),
        (Ctl::FileInfo, Screen::TrackInfo),
        (Ctl::Title, Screen::TrackInfo),
        (Ctl::Eq, Screen::Eq),
    ] {
        tap(&mut a, ctl);
        assert_eq!(a.current(), screen, "{ctl:?}");
        a.go(Screen::NowPlaying);
    }
    // The volume slider sets the level outright on the jack; the shell reads it back.
    let (x0, y0) = centre(Ctl::Volume, false);
    assert!(a.scrub_begin(x0, y0));
    assert_eq!(a.scrub_move(x0 + 40, y0), vec![Action::VolUp]);
    let set = a.volume_level();
    assert_eq!(a.scrub_end(), vec![Action::VolUp]);
    assert!(set > 0);
    // The seek bar: a drop is a seek, as the rail's is.
    let (sx, sy) = centre(Ctl::Seek, false);
    assert!(a.scrub_begin(sx, sy));
    assert!(a.scrub_is_rail());
    assert!(matches!(a.scrub_end().as_slice(), [Action::Seek(_)]));
    // The title drags by hand, and a press that never moves is still the tap: Track
    // information.
    let (tx, ty) = centre(Ctl::Title, false);
    assert!(a.scrub_begin(tx, ty));
    a.scrub_move(tx - 4 * crate::winamp::CELL, ty);
    assert_eq!(a.wa_title_shift, 4, "four cells left = four characters on");
    assert_eq!(a.scrub_end(), vec![]);
    assert_eq!(a.current(), Screen::NowPlaying, "a drag opens nothing");
    assert!(a.scrub_begin(tx, ty));
    a.scrub_end();
    assert_eq!(a.current(), Screen::TrackInfo);
    a.go(Screen::NowPlaying);
    // The balance is live, like Sound's.
    let (bx, by) = centre(Ctl::Balance, false);
    assert!(a.scrub_begin(bx, by));
    a.scrub_move(bx + 200, by);
    assert_eq!(a.balance(), 100);
    assert_eq!(a.scrub_end(), vec![Action::SoundChanged]);
    // PL and windowshade swap the window for the playlist, full screen; its title bar swaps
    // back, and a row below the playing track jumps to it.
    for ctl in [Ctl::Pl, Ctl::Shade] {
        tap(&mut a, ctl);
        assert!(a.wa_playlist, "{ctl:?}");
        tap(&mut a, Ctl::Pl);
        assert!(!a.wa_playlist);
    }
    a.context = (1..=5)
        .map(|i| SongRow {
            object_id: i,
            ..Default::default()
        })
        .collect();
    a.context_idx = 1;
    tap(&mut a, Ctl::Pl);
    assert_eq!(tap(&mut a, Ctl::PlRow(2)), vec![Action::PlayContextAt(3)]);
    assert_eq!(tap(&mut a, Ctl::PlRow(0)), vec![], "the playing track");
    assert!(!a.scrub_begin(240, 400), "no sliders on the playlist");
    // A swipe pages through the queue; a row then plays that track and follows it again.
    assert_eq!(a.swipe(-1, 240, 400), vec![]);
    let first = a.wa_pl_first();
    assert!(first > 1, "paged on");
    a.swipe(1, 240, 400);
    assert_eq!(a.wa_pl_first(), 0, "and back, no further than the top");
    tap(&mut a, Ctl::Pl);
    assert!(!a.wa_playlist);
    assert_eq!(
        tap(&mut a, Ctl::PlRow(1)),
        vec![Action::PlayContextAt(2)],
        "under the window too"
    );
    // A swipe on the window means nothing.
    let before = a.wa_pl_first();
    assert_eq!(a.swipe(-1, 240, 100), vec![]);
    assert_eq!(a.wa_pl_first(), before);
}

#[test]
fn the_style_is_picked_on_display_and_survives_a_restart() {
    // Taps Display's rows, whose positions follow the global text scale: hold the lock
    // so a concurrent 140% test cannot move a tap onto the Size row (see text::scale_guard).
    let _scale = lock_scale();
    use crate::style::Style;
    let mut a = unlocked();
    a.stack = vec![Screen::NowPlaying, Screen::Display];
    let chip = crate::display::style_chip_centre;
    let tap = |a: &mut App, i: usize| {
        let (x, y) = chip(i);
        a.tap(x, y)
    };
    tap(&mut a, Style::COUNT - 1);
    assert_eq!(a.style(), Style::ALL[Style::COUNT - 1]);
    assert_eq!(
        tap(&mut a, Style::COUNT - 1),
        vec![Action::ListSkins],
        "Winamp again lists the skins"
    );
    assert_eq!(a.current(), Screen::Skins);
    a.set_skin_list(vec!["a.wsz".into(), "b.wsz".into()], "a.wsz");
    let row1 = crate::language::TOP + crate::kit::ROW_H() * 3 / 2;
    assert_eq!(a.tap(240, row1), vec![Action::PickSkin(1)]);
    assert_eq!(a.skin_name(1), Some("b.wsz"));
    assert_eq!(a.current(), Screen::Display, "picking goes back");
    assert_eq!(a.style(), Style::Winamp);
    assert_eq!(tap(&mut a, 1), vec![], "any other chip only picks");
    tap(&mut a, Style::COUNT - 1);
    a.display_sel = crate::display::ROW_STYLE;
    a.display_activate();
    assert_eq!(a.style(), Style::Cinder, "Select wraps round");
    tap(&mut a, 1);
    assert_eq!(a.style(), Style::Nocturne);
    let saved = a.style_token();
    let mut b = unlocked();
    b.set_style_token(saved);
    assert_eq!(b.style(), Style::Nocturne);
    b.set_style_token("vaporwave");
    assert_eq!(
        b.style(),
        Style::Nocturne,
        "a newer build's style is not a reset"
    );
}

#[test]
fn touch_tap_nowplaying_transport() {
    let mut a = unlocked(); // NowPlaying
    let was = a.playing;
    let acts = a.tap(240, 692); // the play circle
    assert_eq!(acts, vec![Action::PlayPause]);
    assert_eq!(a.playing, !was);
    assert_eq!(a.tap(350, 692), vec![Action::Next]); // next button
    assert_eq!(a.tap(130, 692), vec![Action::Prev]); // prev button
                                                     // shuffle / repeat icons are now tappable (previously dead on a touch-only device)
    assert_eq!(a.tap(44, 692), vec![Action::ShuffleToggle]);
    assert_eq!(a.tap(436, 692), vec![Action::RepeatCycle]);
}

#[test]
fn swipe_pages_onboarding_and_skips_track() {
    let mut a = unlocked();
    a.start_onboarding();
    assert_eq!(a.current(), Screen::Onboarding);
    let p0 = a.onboarding_page;
    assert!(a.swipe(-1, 240, 400).is_empty()); // leftward = next page
    assert_eq!(a.onboarding_page, p0 + 1);
    assert!(a.swipe(1, 240, 400).is_empty()); // rightward = back
    assert_eq!(a.onboarding_page, p0);
    // swipe left through every page finishes onboarding
    for _ in 0..crate::onboarding::PAGES + 1 {
        a.swipe(-1, 240, 400);
    }
    assert!(a.current() != Screen::Onboarding);
    // Now Playing is ZONED: a swipe BELOW the paging block still skips tracks, exactly as it
    // always did. y=400 is on the artwork, so that one now turns the page instead.
    let mut b = unlocked();
    assert_eq!(b.current(), Screen::NowPlaying);
    let below = crate::now_playing::PAGE_SWIPE_BOT + 40;
    assert_eq!(b.swipe(-1, 240, below), vec![Action::Next]);
    assert_eq!(b.swipe(1, 240, below), vec![Action::Prev]);
    // locked → swipes dead
    b.set_hold(true);
    assert!(b.swipe(-1, 240, below).is_empty());
}

/// A swipe on the artwork turns the Now Playing page, wraps both ways, and emits NO action —
/// it is a pure view change, so it must never reach the shell or the audio path.
#[test]
fn swiping_the_artwork_turns_the_page_and_never_skips() {
    use crate::now_playing::{PAGES, PAGE_SWIPE_BOT};
    let mut a = unlocked();
    assert_eq!(a.current(), Screen::NowPlaying);
    let on_art = PAGE_SWIPE_BOT - 100;
    assert_eq!(a.np_page(), 0);
    for want in 1..PAGES {
        assert!(
            a.swipe(-1, 240, on_art).is_empty(),
            "paging must emit no action"
        );
        assert_eq!(a.np_page(), want);
    }
    // wraps forward…
    assert!(a.swipe(-1, 240, on_art).is_empty());
    assert_eq!(a.np_page(), 0);
    // …and backward
    assert!(a.swipe(1, 240, on_art).is_empty());
    assert_eq!(a.np_page(), PAGES - 1);
}

/// The analyzer must run for the audio pages even when the cover overlay is OFF — otherwise
/// swiping to the spectrum page would show a permanently empty graph.
#[test]
fn the_audio_pages_ask_for_the_analyzer_regardless_of_the_cover_overlay() {
    let mut a = unlocked();
    a.set_viz_size(0); // cover overlay OFF
    a.set_np_page(0);
    assert!(
        !a.wants_spectrum(),
        "cover page with the overlay off needs no spectrum"
    );
    for page in 1..crate::now_playing::PAGES {
        a.set_np_page(page);
        assert!(
            a.wants_spectrum(),
            "page {page} draws audio and must ask for the analyzer"
        );
    }
    // And the cover page alone is enough when its overlay is on.
    a.set_np_page(0);
    a.set_viz_size(2);
    assert!(a.wants_spectrum());
}

#[test]
fn right_swipe_on_song_row_queues_it() {
    let mut a = with_playing(unlocked(), 1);
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    // Rightward swipe starting on the first Songs row. The y is DERIVED from list_top rather
    // than written as a literal: the genre filter strip shifted the list down by 32px and a
    // hardcoded 220 silently started landing on a different row.
    let row_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) / 2;
    assert_eq!(a.swipe(1, 240, row_y), vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), 1);
    let expected = library::song_at(&a.lib, a.lib_sort, 0)
        .unwrap()
        .title
        .clone();
    assert_eq!(upq(&a)[0].title, expected);
    // Feedback started: toast + row chip animation, and tick() reports animation frames.
    assert!(a.toast.starts_with("Added to Up Next"));
    assert_eq!(a.queue_anim_frames, QUEUE_ANIM_FRAMES);
    assert_eq!(a.queue_anim_y, row_y);
    assert!(a.tick());
    // A LEFTWARD swipe now queues too — as PLAY NEXT, straight after the playing track.
    // It used to do nothing at all; the gesture was free and is now the mirror of the right.
    assert_eq!(a.swipe(-1, 240, row_y), vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), 2);
    assert!(
        a.toast.starts_with("Playing next"),
        "toast was {:?}",
        a.toast
    );
    // A rightward swipe on chrome (above the rows) queues nothing.
    assert!(a.swipe(1, 240, 100).is_empty());
    assert_eq!(upq(&a).len(), 2);
}

/// A long press that lifted nothing is Play next on a song row, nothing anywhere else, and
/// nothing on Up Next, where a hold moves the row instead.
#[test]
fn a_hold_on_a_song_row_plays_it_next() {
    let mut a = with_playing(unlocked(), 1);
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    let row_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) / 2;
    assert_eq!(
        a.hold(240, row_y),
        None,
        "opt-in: off, a song hold does nothing"
    );
    a.set_hold_play_next(true);
    assert_eq!(a.hold(240, row_y), Some(vec![Action::QueueChanged]));
    assert_eq!(upq(&a).len(), 1);
    assert!(
        a.toast.starts_with("Playing next"),
        "toast was {:?}",
        a.toast
    );
    assert_eq!(a.hold(240, 100), None, "chrome above the rows");
    assert_eq!(upq(&a).len(), 1);
    a.stack = vec![Screen::UpNext];
    assert_eq!(a.hold(240, row_y), None);
}

/// Swipe-to-queue also works on an expanded album's inline track rows. Those rows are
/// tappable-to-play, but the swipe used to handle the Songs tab only, so the same row queued
/// on one tab and did nothing on another.
#[test]
fn right_swipe_on_an_expanded_album_track_queues_it() {
    let mut a = with_playing(unlocked(), 1);
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Albums;
    // Expand the first album in display order.
    let flat = library::album_display_order(&a.lib, a.album_sort)[0];
    a.album_expanded = Some(flat);
    // Find the y of its first inline track row through the accordion's own layout.
    let want = a.lib.albums_flat()[flat].track_list[0].clone();
    let y = (0..800)
        .find(|&y| a.albums_track_at(y).map(|s| s.object_id) == Some(want.object_id))
        .expect("expanded album should expose a track row");
    // Queuing now reports the change so the shell can schedule a boundary flush.
    assert_eq!(a.swipe(1, 240, y), vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), 1);
    assert_eq!(upq(&a)[0].object_id, want.object_id);
    // Collapsed, that same y is no longer a track row.
    a.album_expanded = None;
    assert!(a.albums_track_at(y).is_none());
    assert!(
        a.swipe(1, 240, 20).is_empty(),
        "a non-row swipe must queue nothing"
    );
    assert_eq!(upq(&a).len(), 1);
}

/// Tapping a row plays that track. The layout is what `tap` resolves against, so a test drives
/// it exactly as `render` does — build the layout, publish it and the album ids, then tap.
#[test]
fn up_next_row_tap_plays_that_track() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    // Three album tracks, the middle one playing: HISTORY hdr, 101, NOW hdr, 202, NEXT hdr, 303.
    a.set_play_context(
        [101i64, 202, 303]
            .iter()
            .map(|&object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
        1,
    );
    // 101 played before 202 started. The history is its own list now, so it is stated.
    a.history_restore(vec![SongRow {
        object_id: 101,
        ..Default::default()
    }]);
    a.queue_scroll_px = 0;
    let l = a.up_next_layout();
    let top = crate::chrome::HEADER_BOTTOM;
    let y_of = |slot| top + l.top_of(slot).unwrap() + crate::up_next::RH() / 2;
    // A history row jumps WITHIN the context and re-arms the follow. PlayContextAt, not
    // PlayIndex: the row is a position in the sequence on screen, and resolving it to the
    // track's album instead is what threw away a "Shuffle all songs" order.
    a.queue_follow = false;
    assert_eq!(
        a.tap(240, y_of(crate::up_next::Slot::History(0))),
        vec![Action::PlayContextAt(0)]
    );
    assert!(
        a.queue_follow,
        "playing from the list re-arms the auto-follow"
    );
    assert_eq!(
        a.current(),
        Screen::UpNext,
        "playing a row should not leave the screen"
    );
    // An upcoming row does the same.
    assert_eq!(
        a.tap(240, y_of(crate::up_next::Slot::Upcoming(2))),
        vec![Action::PlayContextAt(2)]
    );
    // The NOW PLAYING row is where you already are — it just opens Now Playing.
    assert!(a
        .tap(240, y_of(crate::up_next::Slot::Current(1)))
        .is_empty());
    assert_eq!(a.current(), Screen::NowPlaying);
}

/// Jumping to a row in Up Next must keep the sequence that is playing.
///
/// Reported 2026-09-02: "if i shuffle all songs and then try to jump to a specific song in the
/// queue it deletes everything and starts playing that album." The tap emitted `PlayIndex`,
/// and `PlayIndex` resolves an object id to its ALBUM — the only context an object id carries
/// — so a shuffled whole-library context was replaced by the tapped track's album. Exactly the
/// defect `PlayPlaylistAt` was added for, reached from the other direction.
///
/// The index must be the row's position in the CONTEXT, not the track's id, and it must not
/// depend on the tracks belonging to one album — which is what a shuffle-all context is.
#[test]
fn up_next_jump_keeps_a_shuffled_context() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    // A "Shuffle all songs" context: tracks from several different albums, in a scrambled
    // order that belongs to no album at all. Playing at index 1.
    a.set_play_context(
        [501i64, 502, 503, 504, 505]
            .iter()
            .map(|&object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
        1,
    );
    a.queue_scroll_px = 0;
    let l = a.up_next_layout();
    let top = crate::chrome::HEADER_BOTTOM;
    let y_of = |slot| top + l.top_of(slot).unwrap() + crate::up_next::RH() / 2;

    // Four rows down, well past the album any one of these tracks came from.
    let acts = a.tap(240, y_of(crate::up_next::Slot::Upcoming(4)));
    assert_eq!(
        acts,
        vec![Action::PlayContextAt(4)],
        "a jump must carry its position in the context, not an object id"
    );
    assert!(
        !acts.iter().any(|x| matches!(x, Action::PlayIndex(_))),
        "PlayIndex would re-derive the album and discard the shuffle"
    );
    // The context itself is untouched by the tap — the shell reports back what is playing.
    assert_eq!(
        a.context().len(),
        5,
        "jumping must not rebuild or truncate the sequence"
    );
    assert_eq!(
        a.context().iter().map(|t| t.object_id).collect::<Vec<_>>(),
        vec![501, 502, 503, 504, 505],
        "and must not reorder it"
    );
}

/// A row index past the end of the context is dropped rather than played.
///
/// `layout()` is built from `context.len()`, so the two normally agree — but the hit test and
/// the context are read at different moments, and a context that shrank between them would
/// otherwise index off the end.
#[test]
fn up_next_jump_ignores_a_row_past_the_context() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    a.set_play_context(
        [601i64, 602, 603]
            .iter()
            .map(|&object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
        1,
    );
    let l = a.up_next_layout();
    let top = crate::chrome::HEADER_BOTTOM;
    let y = top + l.top_of(crate::up_next::Slot::Upcoming(2)).unwrap() + crate::up_next::RH() / 2;
    // Shrink the context behind the layout's back, then tap the row that no longer exists.
    a.set_play_context(
        vec![SongRow {
            object_id: 601,
            ..Default::default()
        }],
        0,
    );
    assert!(
        a.tap(240, y).is_empty(),
        "a row past the context plays nothing"
    );
}

#[test]
fn up_next_layout_is_history_then_now_then_queue_then_album() {
    use crate::up_next::{layout, Section, Slot};
    let l = layout(1, 4, Some(1));
    let kinds: Vec<Slot> = l.slots.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        kinds,
        vec![
            Slot::Head(Section::History),
            Slot::History(0),
            Slot::Head(Section::Now),
            Slot::Current(1),
            Slot::Head(Section::Next),
            Slot::Upcoming(2),
            Slot::Upcoming(3),
        ]
    );
    // Nothing played yet => no history section at all, header included.
    let l = layout(0, 2, Some(0));
    assert_eq!(
        l.slots.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![
            Slot::Head(Section::Now),
            Slot::Current(0),
            Slot::Head(Section::Next),
            Slot::Upcoming(1)
        ]
    );
    // Nothing at all => the empty state, and no scroll.
    let l = layout(0, 0, None);
    assert!(l.slots.is_empty());
    assert_eq!(l.max_scroll_px(), 0);
}

/// The queue follows playback until the user scrolls, and re-entering the screen re-arms it.
#[test]
fn up_next_follows_playback_until_the_user_scrolls() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    assert!(
        a.queue_follow,
        "arriving on the screen always shows the current track"
    );
    // A long album so there is somewhere to scroll to, and the 20 tracks before the playing
    // one in the history — which is what puts NOW PLAYING far enough down to need scrolling to.
    a.set_play_context(
        (0..40)
            .map(|object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
        20,
    );
    a.history_restore(
        (0..20)
            .map(|object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
    );
    a.queue_scroll_px = a.up_next_layout().follow_scroll();
    assert!(a.queue_scroll_px > 0, "row 20 of 40 must be scrolled to");
    a.scroll_px(-200); // drag the list
    assert!(!a.queue_follow, "a deliberate scroll takes the list over");
    // Leaving and coming back resets it.
    a.pop();
    a.push(Screen::UpNext);
    assert!(a.queue_follow);
    assert_eq!(a.up_next_cur, None);
}

/// Playing something from ANYWHERE re-arms the follow. Found in the queue bug sweep: the
/// shell calls `set_play_queue` for every play that does not start on this screen, and it
/// reset the scroll without re-arming — so one scroll of Up Next left it pinned to the top for
/// the rest of the session, showing the wrong album's first track.
#[test]
fn playing_from_elsewhere_re_arms_the_queue_follow() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    // Something long enough to actually scroll — a scroll that cannot move the list is not a
    // takeover, and must NOT disarm the follow.
    a.set_play_context(
        (0..40)
            .map(|object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
        20,
    );
    a.history_restore(
        (0..20)
            .map(|object_id| SongRow {
                object_id,
                ..Default::default()
            })
            .collect(),
    );
    a.queue_scroll_px = a.up_next_layout().follow_scroll();
    a.scroll_px(-200);
    assert!(
        !a.queue_follow,
        "a scroll that moves the list hands it to the user"
    );
    a.set_play_context(vec![SongRow::default(), SongRow::default()], 0);
    assert!(
        a.queue_follow,
        "a new play must make the queue follow again"
    );
    assert_eq!(
        a.up_next_cur, None,
        "and the next render must treat it as a track change"
    );
}

/// The BT scale went 30 -> 64 (2026-08-11) -> 127 (2026-08-18). 127 is AVRCP's own scale, so a
/// press now moves the sink by exactly ONE unit and no finer control exists over Bluetooth —
/// absolute volume is a 7-bit field. The HUD stretches whatever the scale is back over
/// 0..VOL_MAX, so the bar must look the same at both ends however many steps there are.
#[test]
fn bt_volume_scale_is_finer_and_still_maps_onto_the_shared_hud() {
    use crate::overlay::{BT_VOL_MAX, BT_VOL_MAX_LEGACY, BT_VOL_MAX_LEGACY_64, VOL_MAX};
    const {
        assert!(
            BT_VOL_MAX > BT_VOL_MAX_LEGACY_64,
            "the whole point is finer steps"
        );
        assert!(BT_VOL_MAX_LEGACY_64 > BT_VOL_MAX_LEGACY);
    }
    // 1:1 with the wire. Anything coarser throws away levels the sink can resolve; anything
    // finer is not representable in AVRCP's 7 bits and would be a lie on the bar.
    assert_eq!(BT_VOL_MAX, 127, "AVRCP absolute volume is 0..127");
    let mut a = unlocked();
    a.set_bt_route(true);
    a.set_bt_volume(0);
    assert_eq!(a.display_volume(), 0, "empty bar at the bottom");
    a.set_bt_volume(BT_VOL_MAX);
    assert_eq!(a.display_volume(), VOL_MAX, "full bar at the top");
    // And the level itself cannot exceed the scale.
    a.set_bt_volume(255);
    assert_eq!(a.bt_volume_level(), BT_VOL_MAX);
}

/// The bottom of the Bluetooth scale is the SINK'S FLOOR, not silence, and the HUD must not
/// round a live level down onto it.
///
/// Two halves of one 2026-09-07 report ("the mute on bt is not actually muted"). The label is
/// handled in `overlay::volume_trimmed`, which takes the route and says MIN instead of MUTE —
/// Sony cannot mute an A2DP sink either (analysis/RE_volume_service.md). The half that lives
/// here is arithmetic: the scale is 127 -> 120, so AVRCP 1 truncates to 0 and the bar would
/// claim the floor while the sink is a step above it.
#[test]
fn a_live_bluetooth_level_never_displays_as_the_floor() {
    let mut a = unlocked();
    a.set_bt_route(true);
    // Only a genuine 0 reads as 0.
    a.set_bt_volume(0);
    assert_eq!(
        a.display_volume(),
        0,
        "AVRCP 0 is the floor and must read as it"
    );
    // Every level the sink can actually be at reads as something above the floor.
    for lvl in 1..=crate::overlay::BT_VOL_MAX {
        a.set_bt_volume(lvl);
        assert!(
            a.display_volume() > 0,
            "sink level {lvl} rounded onto the floor readout"
        );
    }
}

/// The scrollbar on this screen is `library::scrollbar`, whose drag maths measure against the
/// LIBRARY's list bottom — so the two bottoms must be the same number, not two literals that
/// happen to agree.
#[test]
fn up_next_list_bottom_matches_the_library_it_shares_a_scrollbar_with() {
    assert_eq!(
        crate::up_next::queue_view_h(),
        library::list_bottom() - crate::chrome::HEADER_BOTTOM
    );
}

#[test]
fn up_next_follow_parks_the_current_row_a_third_down() {
    let l = crate::up_next::layout(20, 40, Some(20));
    let view = crate::up_next::queue_view_h();
    let top = l.current_top.unwrap();
    assert_eq!(
        l.follow_scroll(),
        (top - view / 3).clamp(0, l.max_scroll_px())
    );
    // Early tracks cannot scroll above the start.
    assert_eq!(crate::up_next::layout(0, 40, Some(0)).follow_scroll(), 0);
}

/// An empty queue keeps the old behaviour: a tap just leaves.
#[test]
fn up_next_tap_with_nothing_queued_returns_to_now_playing() {
    let mut a = unlocked();
    a.push(Screen::UpNext);
    a.set_play_context(Vec::new(), 0);
    assert!(a.tap(240, crate::chrome::HEADER_BOTTOM + 10).is_empty());
    assert_eq!(a.current(), Screen::NowPlaying);
}

/// The "Play album" band on the drill-in is hit-tested through its own rect. The old literal
/// range started 16px above where the band is actually drawn.
#[test]
fn album_play_band_hit_matches_where_it_is_drawn() {
    let (bx, by, bw, bh) = library::album_play_band();
    assert!(library::hit_album_play_band(bx + bw / 2, by + bh / 2));
    assert!(
        !library::hit_album_play_band(bx + bw / 2, by - 1),
        "band hit extends above the band"
    );
    assert!(
        !library::hit_album_play_band(bx - 1, by + bh / 2),
        "band hit extends left of the band"
    );
    assert!(
        !library::hit_album_play_band(bx + bw / 2, by + bh),
        "band hit extends below the band"
    );
}

#[test]
fn album_screen_taps_play_band_and_track_rows() {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Albums;
    a.album_view = 0;
    a.push(Screen::Album);
    let album = a.lib.albums_flat()[0].clone();
    let first_id = album.track_list[0].object_id;
    // Coordinates come from the layout functions, never literals: this test used to hardcode
    // 260/320/160, which pinned it to one band position and said nothing about whether the
    // renderer and the hit test agreed.
    let (bx, by, bw, bh) = library::album_play_band();
    // Tap the "Play album" band → plays from the first track (shell expands the album).
    assert_eq!(
        a.tap(bx + bw / 2, by + bh / 2),
        vec![Action::PlayIndex(first_id)]
    );
    // Tap the first track row → same first track by object id.
    let row0 = library::album_tracks_top() + library::ALBUM_TRACK_RH() / 2;
    assert_eq!(a.tap(240, row0), vec![Action::PlayIndex(first_id)]);
    // A tap in the header art/title area, above the band, plays nothing.
    assert!(a.tap(240, by - 20).is_empty());
    // The square beside the band shuffles this album: its tracks, dealt as songs.
    let (sx, sy, sw, sh) = library::album_shuffle_button();
    assert!(sx >= bx + bw, "the button sits beside the band, not on it");
    assert_eq!(a.tap(sx + sw / 2, sy + sh / 2), vec![Action::ShuffleList]);
    let ids: Vec<i64> = album.track_list.iter().map(|s| s.object_id).collect();
    assert_eq!(a.list_shuffle_by(), crate::shuffle::ShuffleBy::Songs);
    assert_eq!(a.take_play_list(), ids);
}

/// The artist page's Random album plays one of the artist's albums from its first track, and
/// over a few presses reaches more than one of them.
#[test]
fn random_album_plays_one_of_the_artists_albums() {
    let mut a = library_on(Tab::Artists);
    let idx = (0..a.lib.artists.len())
        .find(|&i| {
            let name = a.lib.artists[i].name.clone();
            library::artist_page(&a.lib, &name, false).albums.len() > 1
        })
        .expect("the sample has an artist with two albums");
    a.tap(200, artist_row_y(idx));
    let names: Vec<String> = a
        .artist_page()
        .unwrap()
        .albums
        .iter()
        .map(|(_, al)| al.name.clone())
        .collect();
    let (x, y, w, h) = library::artist_band(names.len())
        .1
        .expect("two albums, so a button");
    let mut seen = std::collections::HashSet::new();
    for _ in 0..12 {
        let acts = a.tap(x + w / 2, y + h / 2);
        assert!(
            matches!(acts.as_slice(), [Action::PlayIndex(_)]),
            "{acts:?}"
        );
        let name = a
            .toast
            .strip_prefix("Random album · ")
            .expect("named")
            .to_string();
        assert!(names.contains(&name), "{name} is not this artist's");
        seen.insert(name);
    }
    assert!(
        seen.len() > 1,
        "always the same album: {seen:?} of {names:?}"
    );
    // One album: no button, and the band runs the whole line instead of leaving its gap.
    let (one, button, _) = library::artist_band(1);
    assert!(button.is_none());
    assert!(one.2 > library::artist_band(2).0 .2);
    assert!(!library::hit_artist_random_album(x + w / 2, y + h / 2, 1));
}

/// BOTH swipe directions on the album drill-in reach the shell. The right-swipe (queue later)
/// used to call a wrapper that THREW THE ACTION AWAY and then return an empty vec: the track
/// landed in the UI's queue and popped its "Added to queue" toast, and nothing ever re-issued
/// the sequence — so it played wherever the old sequence said, not where Up Next did. Its
/// mirror one arm up (left = Play Next) always returned the action, which is what made the
/// difference invisible.
#[test]
fn both_album_page_swipes_tell_the_shell_the_queue_changed() {
    let open_album = || {
        let mut a = with_playing(unlocked(), 1);
        a.stack = vec![Screen::Library];
        a.lib_tab = Tab::Albums;
        a.album_view = 0;
        a.push(Screen::Album);
        a
    };
    let row0 = library::album_tracks_top() + library::ALBUM_TRACK_RH() / 2;

    let mut later = open_album();
    let want = later.lib.albums_flat()[0].track_list[0].object_id;
    assert_eq!(
        later.swipe(1, 240, row0),
        vec![Action::QueueChanged],
        "right-swipe queued the track but never told the shell"
    );
    assert_eq!(upq(&later).len(), 1);
    assert_eq!(upq(&later)[0].object_id, want);

    let mut next = open_album();
    assert_eq!(next.swipe(-1, 240, row0), vec![Action::QueueChanged]);
    assert_eq!(upq(&next).len(), 1);
    assert_eq!(upq(&next)[0].object_id, want);

    // And a swipe on the header, where there is no track, still queues nothing and says so.
    let mut miss = open_album();
    let (_, by, _, _) = library::album_play_band();
    assert!(miss.swipe(1, 240, by - 20).is_empty());
    assert!(upq(&miss).is_empty());
}

/// Tapping a row in Up Next jumps WITHIN the list — it never asks "replace Up Next?", because
/// nothing is replaced — and a list built by hand stays one, so starting something else from
/// the library still asks.
#[test]
fn tapping_a_queue_row_keeps_the_queue_a_queue() {
    let mut a = queued(3);
    a.queue_push_for_test();
    assert!(a.hand_added);
    assert_eq!(a.tap(60, qrow_y(&a, 1)), vec![Action::PlayContextAt(2)]);
    assert!(!a.modal_open(), "a jump within the list is not a replace");
    // The list's hand-built state survives the jump (the shell hands it back after re-issuing
    // the same list — see `list_state`).
    let kept = a.list_state();
    a.set_play_context(a.context().to_vec(), 2);
    a.restore_list_state(kept);
    assert!(a.hand_added);
    assert_eq!(
        a.tap_for_test_play(10),
        Vec::<Action>::new(),
        "a replace still asks first"
    );
    assert!(a.modal_open());
}

/// A file tapped in a folder plays THE FOLDER from that file — not the file's album, which is
/// what `PlayIndex` resolves to and what this used to emit.
#[test]
fn a_folder_track_plays_the_folder_from_there() {
    let mut a = unlocked();
    let tracks: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("F{i}"),
            object_id: 40 + i,
            ..Default::default()
        })
        .collect();
    a.lib.folders = vec![crate::model::FolderRow {
        path: "/contents/Music/Mix".into(),
        name: "Mix".into(),
        tracks,
        ..Default::default()
    }];
    a.folder_stack = vec![0];
    assert_eq!(a.folder_play(2), vec![Action::PlayListAt(2)]);
    assert_eq!(a.take_play_list(), vec![40, 41, 42, 43]);
    assert!(
        a.folder_play(9).is_empty(),
        "a row that is not there plays nothing"
    );
}

/// A folder's files swipe to Up Next like every other track row: left = Play next, right =
/// Add to Up Next. The folder rows above them do not.
#[test]
fn a_folder_track_swipes_to_up_next() {
    let mut a = with_playing(unlocked(), 2);
    let tracks: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("F{i}"),
            object_id: 40 + i,
            ..Default::default()
        })
        .collect();
    a.lib.folders = vec![crate::model::FolderRow {
        name: "Mix".into(),
        tracks,
        ..Default::default()
    }];
    a.push(Screen::Folders);
    a.folder_stack = vec![0];
    let y = crate::folders::row_top(1, 0) + 10;
    assert!(a.swipe_track(80, y));
    assert_eq!(a.swipe(1, 240, y), vec![Action::QueueChanged]);
    assert_eq!(
        upq(&a).last().map(|t| t.object_id),
        Some(41),
        "right = the end of Up Next"
    );
    assert_eq!(a.swipe(-1, 240, y), vec![Action::QueueChanged]);
    assert_eq!(
        upq(&a).first().map(|t| t.object_id),
        Some(41),
        "left = next"
    );
}

/// The Songs tab plays the Songs LIST, in the sort that is showing, from the tapped row.
#[test]
fn a_songs_row_plays_the_songs_list_in_the_order_drawn() {
    let mut a = unlocked();
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Songs;
    let y =
        library::list_top(Tab::Songs) + library::row_h(Tab::Songs) + library::row_h(Tab::Songs) / 2;
    assert_eq!(a.tap(200, y), vec![Action::PlayListAt(1)]);
    let list = a.take_play_list();
    let drawn: Vec<i64> = library::song_order(&a.lib, a.lib_sort)
        .into_iter()
        .map(|i| a.lib.songs[i].object_id)
        .collect();
    assert_eq!(list, drawn);
}

#[test]
fn a_hold_lifts_a_queue_row_from_anywhere_the_handle_only_from_its_column() {
    let seed = || queued(3);
    // A y that lands on the first upcoming row, whatever the layout puts above it.
    let mut probe = seed();
    let row_y = qrow_y(&probe, 0) - crate::up_next::RH() / 2 + 4;
    let off_handle = crate::up_next::GRIP_X0 - 40;
    let on_handle = crate::up_next::GRIP_X0 + 4;

    // Off the handle: the handle entry point declines, the hold entry point takes it.
    assert!(!probe.reorder_begin(off_handle, row_y), "no handle there");
    let mut a = seed();
    assert!(
        a.reorder_begin_hold(off_handle, row_y),
        "a hold lifts it from anywhere"
    );

    // On the handle: still immediate, no hold needed.
    let mut b = seed();
    assert!(
        b.reorder_begin(on_handle, row_y),
        "the handle is still immediate"
    );

    // Neither entry point works with nothing playing, or off the Up Next screen.
    let mut c = unlocked();
    c.go(Screen::UpNext);
    assert!(
        !c.reorder_begin_hold(off_handle, row_y),
        "an empty list has no rows to lift"
    );
    let mut d = seed();
    d.go(Screen::NowPlaying);
    assert!(
        !d.reorder_begin_hold(off_handle, row_y),
        "only Up Next reorders"
    );
}

/// NEXT FROM reorders too, and only the rows that have not played yet.
///
/// Being able to see the rest of the sequence but not arrange it is the difference between a
/// queue you can read and a queue you can use. Everything BELOW the playing row moves, in one
/// index space; history has already played and the playing row is not a position in a list.
/// Because everything that can move is strictly after `context_idx`, a reorder cannot disturb
/// what is playing — which is what makes it safe to do to a live sequence.
#[test]
fn the_next_from_section_reorders_and_never_moves_the_playing_track() {
    let seed = || {
        let mut a = unlocked();
        a.go(Screen::UpNext);
        let album: Vec<SongRow> = (0..6)
            .map(|i| SongRow {
                title: format!("A{i}"),
                object_id: 10 + i,
                ..Default::default()
            })
            .collect();
        a.set_play_context(album, 1); // A1 is playing; A2..A5 are upcoming
                                      // A real play history, since it is no longer implied by `context_idx`.
        a.history_restore(vec![SongRow {
            object_id: 10,
            ..Default::default()
        }]);
        a
    };
    let ids = |a: &App| a.context().iter().map(|s| s.object_id).collect::<Vec<_>>();

    // Lift the first upcoming row (A2) and drop it two down.
    let mut a = seed();
    let lay = a.up_next_layout();
    assert_eq!(
        lay.upcoming_first(),
        2,
        "the section starts one past the playing track"
    );
    assert_eq!(lay.upcoming_len(), 4);
    let row_y = crate::chrome::HEADER_BOTTOM
        + lay
            .top_of(crate::up_next::Slot::Upcoming(2))
            .expect("A2 is drawn")
        + 4;
    assert!(
        a.reorder_begin_hold(20, row_y),
        "a hold lifts an upcoming row from anywhere on it"
    );
    let d = a.reorder_state().expect("a row is in hand");
    // One history row and no queue, so the first upcoming row is span index 1.
    assert_eq!(d.from, 1);
    a.reorder_track(2 * crate::up_next::RH() + 4);
    let acts = a.reorder_release();
    assert_eq!(
        acts,
        vec![Action::QueueChanged],
        "the sequence has to be re-issued"
    );
    assert_eq!(
        ids(&a),
        vec![10, 11, 13, 14, 12, 15],
        "A2 moved down two, nothing else moved"
    );
    // The playing track kept its place, so playback is undisturbed.
    assert_eq!(a.context_idx(), 1);
    assert_eq!(a.context()[1].object_id, 11);

    // The grab handle lifts an upcoming row too, because it is now DRAWN on one. This pairs
    // with `up_next::album_row`'s `grippy` flag: if that stops drawing the bars, this must
    // stop lifting, or the column becomes a control with no mark on it.
    let mut b = seed();
    assert!(
        b.reorder_begin(crate::up_next::GRIP_X0 + 4, row_y),
        "the handle lifts an upcoming row"
    );
    let bd = b.reorder_state().expect("the handle put a row in hand");
    assert_eq!(bd.from, 1);

    // A PLAYED row carries a handle too, and lifts: reaching back into the history and pulling
    // a track into what is coming is the reason it is draggable at all.
    let mut b2 = seed();
    let hgy = crate::chrome::HEADER_BOTTOM
        + b2.up_next_layout()
            .top_of(crate::up_next::Slot::History(0))
            .expect("A0 is history")
        + 4;
    assert!(
        b2.reorder_begin(crate::up_next::GRIP_X0 + 4, hgy),
        "a played row lifts — it can be played again"
    );
    assert_eq!(b2.reorder_state().expect("in hand").from, 0);
    let mut b2 = seed();
    let cgy = crate::chrome::HEADER_BOTTOM
        + b2.up_next_layout()
            .top_of(crate::up_next::Slot::Current(1))
            .expect("A1 is playing")
        + 4;
    assert!(
        !b2.reorder_begin(crate::up_next::GRIP_X0 + 4, cgy),
        "no handle on the playing row"
    );
    assert!(b2.reorder_state().is_none());

    // A drop that goes nowhere changes nothing and asks for no re-issue.
    let mut d2 = seed();
    assert!(d2.reorder_begin_hold(20, row_y));
    assert_eq!(d2.reorder_release(), Vec::<Action>::new());
    assert_eq!(ids(&d2), vec![10, 11, 12, 13, 14, 15]);

    // A HOLD anywhere on a played row lifts it too, the same as the other two sections.
    let mut h = seed();
    let hy = crate::chrome::HEADER_BOTTOM
        + h.up_next_layout()
            .top_of(crate::up_next::Slot::History(0))
            .expect("A0 is history")
        + 4;
    assert!(
        h.reorder_begin_hold(20, hy),
        "a played row can be pulled back into the queue"
    );
}

/// Any upcoming row moves anywhere in what is to come; the playing row never moves.
#[test]
fn a_row_can_be_dragged_between_the_queue_and_the_album() {
    use crate::up_next::Slot;
    let seed = || {
        let mut a = unlocked();
        a.go(Screen::UpNext);
        let album: Vec<SongRow> = (0..5)
            .map(|i| SongRow {
                title: format!("A{i}"),
                object_id: 10 + i,
                ..Default::default()
            })
            .collect();
        a.set_play_context(album, 1); // A1 playing; A2..A4 upcoming
        for id in [90, 91] {
            a.enqueue_at(
                SongRow {
                    title: format!("P{id}"),
                    object_id: id,
                    ..Default::default()
                },
                0,
                QueueAt::Later,
            );
        }
        a
    };
    let ctx = |a: &App| a.context().iter().map(|s| s.object_id).collect::<Vec<_>>();

    // The span is the upcoming rows in play order: [A2, A3, A4, 90, 91].
    let lay = seed().up_next_layout();
    assert_eq!(lay.movable_len(), 5);
    assert_eq!(lay.movable_slot(0), Some(Slot::Upcoming(2)));
    assert_eq!(lay.movable_slot(4), Some(Slot::Upcoming(6)));

    // A queued track dragged to the front plays next.
    let mut a = seed();
    assert_eq!(a.movable_move(4, 0), vec![Action::QueueChanged]);
    assert_eq!(ctx(&a), vec![10, 11, 91, 12, 13, 14, 90]);
    assert_eq!(a.context_idx(), 1, "the playing track never moves");

    // An album track dragged to the end plays last.
    let mut b = seed();
    assert_eq!(b.movable_move(0, 4), vec![Action::QueueChanged]);
    assert_eq!(ctx(&b), vec![10, 11, 13, 14, 90, 91, 12]);

    // A move to its own slot is a no-op, and out-of-range indices are refused.
    let mut e = seed();
    assert_eq!(e.movable_move(2, 2), Vec::<Action>::new());
    assert_eq!(e.movable_move(0, 5), Vec::<Action>::new());
    assert_eq!(e.movable_move(9, 0), Vec::<Action>::new());
    assert_eq!(ctx(&e), vec![10, 11, 12, 13, 14, 90, 91]);
}

#[test]
fn a_played_track_can_be_pulled_back_but_nothing_can_be_pushed_into_the_past() {
    let song = |id: i64| SongRow {
        object_id: id,
        ..Default::default()
    };
    let seed = || {
        let mut a = unlocked();
        a.go(Screen::UpNext);
        a.set_play_context((0..5).map(|i| song(10 + i)).collect(), 1); // A1 playing
        a.history_restore(vec![song(700), song(701)]);
        a
    };
    let hist = |a: &App| a.history().iter().map(|h| h.object_id).collect::<Vec<_>>();
    let ctx = |a: &App| a.context().iter().map(|s| s.object_id).collect::<Vec<_>>();

    // The span is history ++ upcoming = [700, 701, A2, A3, A4].
    let l = seed().up_next_layout();
    assert_eq!(l.movable_len(), 5);
    assert_eq!(
        l.drop_first(),
        2,
        "the first two indices are history and cannot be landed on"
    );
    assert_eq!(l.movable_slot(0), Some(crate::up_next::Slot::History(0)));
    assert_eq!(l.movable_slot(2), Some(crate::up_next::Slot::Upcoming(2)));

    // HISTORY → PLAY NEXT. Dragging a played row up lands it at the first droppable slot —
    // index 1, because lifting it out of the history made the history one shorter.
    let mut a = seed();
    assert_eq!(a.movable_move(0, 1), vec![Action::QueueChanged]);
    assert_eq!(hist(&a), vec![701], "it left the history");
    assert_eq!(
        ctx(&a),
        vec![10, 11, 700, 12, 13, 14],
        "…and is the next thing that plays"
    );
    assert_eq!(a.context_idx(), 1, "the playing track never moves");

    // HISTORY → the end. Span index 4 is the last slot.
    let mut b = seed();
    assert_eq!(b.movable_move(1, 4), vec![Action::QueueChanged]);
    assert_eq!(hist(&b), vec![700]);
    assert_eq!(
        ctx(&b),
        vec![10, 11, 12, 13, 14, 701],
        "it is the last thing that plays"
    );

    // NOTHING GOES INTO THE HISTORY. Every upcoming row, aimed at every history index, lands
    // at the front of what is coming instead — and the history is left exactly as it was.
    for from in 2..5 {
        for to in 0..2 {
            let mut c = seed();
            let before = hist(&c);
            c.movable_move(from, to);
            assert_eq!(hist(&c), before, "{from}->{to} wrote into the history");
            let want = [700, 701, 12, 13, 14][from];
            assert_eq!(
                c.context()[2].object_id,
                want,
                "{from}->{to} did not land next"
            );
        }
    }

    // A played row aimed back INTO the history clamps to the first droppable slot.
    let mut d = seed();
    assert_eq!(d.movable_move(0, 0), vec![Action::QueueChanged]);
    assert_eq!(hist(&d), vec![701]);
    assert_eq!(ctx(&d), vec![10, 11, 700, 12, 13, 14]);
    // The one genuine no-op: a row put back in the slot it came from.
    let mut e = seed();
    assert_eq!(e.movable_move(3, 3), Vec::<Action>::new());
    assert_eq!(hist(&e), vec![700, 701]);
    assert_eq!(ctx(&e), vec![10, 11, 12, 13, 14]);
}

/// The storage move lands every row exactly where the drag preview showed it, for every
/// from/to pair over history + upcoming.
#[test]
fn the_storage_move_matches_the_order_the_drag_previewed() {
    for (hlen, ulen) in [(0usize, 3usize), (2, 4), (1, 0), (2, 1), (3, 6), (0, 1)] {
        for from in 0..hlen + ulen {
            for to in 0..hlen + ulen {
                let mut a = unlocked();
                // One playing row, then `ulen` upcoming ones.
                let ctx: Vec<SongRow> = (0..=ulen)
                    .map(|i| SongRow {
                        object_id: 100 + i as i64,
                        ..Default::default()
                    })
                    .collect();
                a.set_play_context(ctx, 0);
                a.history_restore(
                    (0..hlen)
                        .map(|i| SongRow {
                            object_id: 700 + i as i64,
                            ..Default::default()
                        })
                        .collect(),
                );
                let span = |a: &App| -> Vec<i64> {
                    a.history()
                        .iter()
                        .map(|r| r.object_id)
                        .chain(a.context()[1..].iter().map(|r| r.object_id))
                        .collect()
                };
                // The order the renderer would have drawn — with `to` held out of the history,
                // which is the one thing the preview is not free to promise.
                let before = span(&a);
                let floor = hlen - usize::from(from < hlen);
                let landed = to.max(floor);
                let mut want = before.clone();
                if landed != from {
                    let it = want.remove(from);
                    want.insert(landed, it);
                }

                a.movable_move(from, to);
                let after = span(&a);
                assert_eq!(
                    after, want,
                    "span disagrees for h={hlen} u={ulen} {from}->{to}"
                );
                // Nothing was duplicated or lost, and the playing row is still the playing row.
                assert_eq!(after.len(), before.len());
                assert_eq!(a.context_idx(), 0);
                assert_eq!(a.context()[0].object_id, 100);
            }
        }
    }
}

/// The playlist page's cover and the editor's Cover row both open the picker; a choice saves at
/// once and comes back. Sony's playlists have nowhere to store one, so theirs is just a picture.
#[test]
fn the_cover_picker_offers_auto_albums_and_pictures() {
    use crate::playlist_cover as pc;
    let (cx, cy, cw, ch) = library::PLAYLIST_COVER;
    let (tap_x, tap_y) = (cx + cw / 2, cy + ch / 2);
    let row = |i: usize| pc::TOP + i as i32 * pc::ROW_H + pc::ROW_H / 2;

    let mut b = App::unlocked();
    b.push_for_test(Screen::Library);
    b.lib_tab = Tab::Playlists;
    let songs: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("T{i}"),
            art: format!("Album {}", i / 2),
            object_id: 900 + i,
            album_id: 1 + i / 2,
            ..Default::default()
        })
        .collect();
    b.lib = crate::model::Library {
        playlists: vec![crate::model::PlaylistRow {
            id: -9,
            name: "Mine".into(),
            tracks: 4,
            art: "Mine".into(),
            track_list: songs,
            user: true,
            cover_album_id: 1,
            ..Default::default()
        }],
        cover_pictures: vec![crate::model::CoverPicture {
            path: "/contents/cinder_playlists/beach.jpg".into(),
            name: "beach".into(),
            thumb: crate::art::Image {
                w: 0,
                h: 0,
                rgb: Vec::new(),
            },
        }],
        ..Default::default()
    };
    b.open_playlist(0);
    assert!(b.tap(tap_x, tap_y).is_empty());
    assert_eq!(b.current(), Screen::PlaylistCover);
    // Automatic, Album 0, Album 1 (named by its first track), the picture.
    assert_eq!(b.tap(240, row(2)), vec![Action::PlaylistSetCover(-9, 902)]);
    assert_eq!(b.current(), Screen::Playlist, "a choice goes back");
    assert_eq!(b.toast, "Cover: Album 1");
    b.tap(tap_x, tap_y);
    assert_eq!(b.tap(240, row(3)), vec![Action::PlaylistSetPicture(-9, 0)]);
    assert_eq!(b.toast, "Cover: beach");
    b.tap(tap_x, tap_y);
    assert_eq!(b.tap(240, row(0)), vec![Action::PlaylistSetCover(-9, 0)]);
    assert_eq!(b.toast, "Cover: automatic");

    // From the editor's Cover row too, and its Name row opens the keyboard on the name.
    b.tap(458, 66); // EDIT
    assert_eq!(b.current(), Screen::PlaylistEdit);
    b.tap(240, crate::playlist_edit::COVER_Y + 10);
    assert_eq!(b.current(), Screen::PlaylistCover);
    b.pop();
    b.tap(240, crate::playlist_edit::NAME_Y + 10);
    assert_eq!(b.current(), Screen::Keyboard);
    assert_eq!(b.text_input(), "Mine");

    let mut c = own_and_sony();
    c.open_playlist(1);
    assert!(!library::hit_playlist_cover(
        c.playlist_row().unwrap(),
        tap_x,
        tap_y
    ));
}

/// MONO is an ACCESSIBILITY setting, not a tuning — so it does not travel with A/B, it turns
/// the balance control off rather than fighting it, and it persists like a preference.
#[test]
fn mono_is_a_preference_not_part_of_the_ab_setup() {
    let mut a = unlocked();
    a.go(Screen::Sound);
    let sc = crate::sound::max_scroll();
    a.sound_scroll_px = sc;
    let (mx, my, mw, mh) = crate::sound::balance_mono_rect(sc);
    let (tap_x, tap_y) = (mx + mw / 2, my + mh / 2);

    assert!(
        !a.mono(),
        "off by default — it is an accommodation, not a default"
    );
    assert_eq!(a.tap(tap_x, tap_y), vec![Action::MonoChanged]);
    assert!(a.mono());
    assert_eq!(
        a.sound_sel,
        crate::sound::ROW_BALANCE,
        "the tap selects its own row"
    );
    assert_eq!(a.tap(tap_x, tap_y), vec![Action::MonoChanged]);
    assert!(!a.mono(), "it latches both ways");

    // IT SURVIVES THE A/B SWITCH. A and B are two tunings to compare; someone who needs mono
    // needs it on both sides, and having it vanish on one would be a defect, not a feature.
    a.set_mono(true);
    let before = a.setup_idx();
    let ab = |seg: usize| {
        let (x, y, w, h) = crate::sound::ab_rect(seg);
        (x + w / 2, y + h / 2)
    };
    let (bx, by) = ab(1);
    a.tap(bx, by);
    assert_ne!(a.setup_idx(), before, "the A/B control moved");
    assert!(
        a.mono(),
        "mono must not be part of the setup being compared"
    );
    let (ax, ay) = ab(0);
    a.tap(ax, ay);
    assert!(a.mono());

    // CENTRE is dead while mono is on: with one signal in both channels there is no image to
    // re-centre, and the row draws it greyed to say so.
    let mut b = unlocked();
    b.go(Screen::Sound);
    b.set_balance(20);
    b.set_mono(true);
    b.sound_scroll_px = sc;
    let (rx, ry, rw, rh) = crate::sound::balance_reset_rect(sc);
    assert!(
        b.tap(rx + rw / 2, ry + rh / 2).is_empty(),
        "CENTRE acted under mono"
    );
    assert_eq!(
        b.balance(),
        20,
        "…and the balance the user tuned is still there"
    );
    // Switching mono off hands the slider back exactly as it was, rather than resetting it.
    b.set_mono(false);
    assert_eq!(b.tap(rx + rw / 2, ry + rh / 2), vec![Action::SoundChanged]);
    assert_eq!(b.balance(), crate::sound::BALANCE_CENTRE);
}

/// A saved pin naming a screen that is not a "place" is dropped, not restored.
///
/// The pin GESTURE has always refused those, but the decoder did not re-check, and the two are
/// not the same thing: the token table is an on-disk format that outlives a build, it covers
/// screens `pinnable` excludes, and the settings file is plain text a user can edit.
#[test]
fn a_pin_naming_a_non_place_screen_is_refused_on_load() {
    let mut a = unlocked();
    // A well-formed record in every respect except the screen it names.
    let rec = |tok: &str| format!("{tok}|albums|0|0|-1|0|0|0|0|0|Title|Sub|-1|-1||-1");
    a.shelf_pin_decode(0, &rec("tone"));
    assert!(
        a.pins[0].is_none(),
        "Tone is not a place a pin may point at"
    );
    a.shelf_pin_decode(0, &rec("btcodec"));
    assert!(a.pins[0].is_none(), "nor is the BT codec picker");
    // …and a real place still decodes, so the guard is a filter and not a wall.
    a.shelf_pin_decode(0, &rec("album"));
    assert!(a.pins[0].is_some(), "Album is a place");
    assert_eq!(a.pins[0].as_ref().unwrap().screen, Screen::Album);
}

/// A Play next track that starts IS the NOW PLAYING row — it is a row of the list like any
/// other, so there is no second "playing pick" state to fall out of step with the list.
#[test]
fn a_playing_pick_is_the_now_playing_row_not_the_track_it_interrupted() {
    use crate::up_next::{Section, Slot};
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, 1); // A1 playing
    a.enqueue_at(
        SongRow {
            title: "PICK2".into(),
            object_id: 98,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );
    a.enqueue_at(
        SongRow {
            title: "PICK".into(),
            object_id: 99,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );

    // A1 ends, the first pick starts.
    assert!(!a.track_started(99, false));
    assert_eq!(a.context()[a.context_idx()].object_id, 99);
    // A1 has been played, so it is history. ONE row: this list started at A1, and A0 never
    // played.
    assert_eq!(
        a.history().iter().map(|h| h.object_id).collect::<Vec<_>>(),
        vec![11]
    );
    let order: Vec<Slot> = a.up_next_layout().slots.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        order,
        vec![
            Slot::Head(Section::History),
            Slot::History(0),
            Slot::Head(Section::Now),
            Slot::Current(2),
            Slot::Head(Section::Next),
            Slot::Upcoming(3),
            Slot::Upcoming(4),
            Slot::Upcoming(5),
        ]
    );
    let next: Vec<i64> = upq(&a).iter().map(|t| t.object_id).collect();
    assert_eq!(
        next,
        vec![98, 12, 13],
        "the other pick, then the rest of the album"
    );
}

/// THE RETENTION POLICY, which is the part of a history people actually notice. Every clause
/// of `App::history`'s doc comment has an assertion here, because "when is it wiped" was left
/// to this side to decide and a decision nobody can see is not a decision.
#[test]
fn the_play_history_keeps_what_was_played_and_drops_what_it_said_it_would() {
    let song = |id: i64| SongRow {
        object_id: id,
        ..Default::default()
    };
    let ids = |a: &App| a.history().iter().map(|h| h.object_id).collect::<Vec<_>>();

    // 1. IT RECORDS WHAT WAS PLAYED, in the order it was played, oldest first.
    let mut a = unlocked();
    a.set_play_context((0..4).map(|i| song(10 + i)).collect(), 0);
    for id in [11, 12, 13] {
        a.track_started(id, false);
    }
    assert_eq!(
        ids(&a),
        vec![10, 11, 12],
        "the outgoing track is what lands in the history"
    );

    // 2. IT SURVIVES A CONTEXT CHANGE. This is the whole reason it exists: the old derived
    //    history was a slice of the context, so starting a different album erased it.
    a.set_play_context(vec![song(50), song(51)], 0);
    assert_eq!(
        ids(&a),
        vec![10, 11, 12],
        "a new album does not erase what came before it"
    );
    a.track_started(51, false);
    assert_eq!(
        ids(&a),
        vec![10, 11, 12, 50],
        "…and the new album records into the same list"
    );

    // 3. A QUEUED TRACK IS RECORDED TOO.
    let mut b = unlocked();
    b.set_play_context(vec![song(10), song(11)], 0);
    b.enqueue_at(song(99), 0, QueueAt::Next);
    b.track_started(99, false); // the pick starts; 10 was playing
    b.track_started(11, false); // the album takes over; the PICK was playing
    assert_eq!(
        ids(&b),
        vec![10, 99],
        "a swipe-queued song is something you played"
    );

    // 4. CONSECUTIVE REPEATS COLLAPSE. Repeat-one is a supported mode, and without this an
    //    hour of it is a hundred rows of one song and nothing else.
    let mut c = unlocked();
    c.set_play_context(vec![song(10), song(11)], 0);
    for _ in 0..8 {
        c.track_started(10, false); // the same track restarting
    }
    assert!(
        ids(&c).is_empty(),
        "a track does not follow itself into the history"
    );
    c.track_started(11, false);
    c.track_started(10, false);
    c.track_started(11, false);
    assert_eq!(
        ids(&c),
        vec![10, 11, 10],
        "…but alternating tracks are separate listens"
    );

    // 5. IT IS CAPPED, and the OLDEST go. A device left playing for a week must not grow an
    //    unbounded list inside a file that is rewritten every second.
    let mut d = unlocked();
    d.set_play_context((0..(HISTORY_MAX as i64 + 40)).map(song).collect(), 0);
    for id in 1..(HISTORY_MAX as i64 + 40) {
        d.track_started(id, false);
    }
    assert_eq!(d.history().len(), HISTORY_MAX, "the cap holds");
    assert_eq!(
        d.history().first().map(|h| h.object_id),
        Some(39),
        "the oldest are the ones lost"
    );
    assert_eq!(
        d.history().last().map(|h| h.object_id),
        Some(HISTORY_MAX as i64 + 38)
    );

    // 6. ONLY AN EXPLICIT CLEAR EMPTIES IT — and it is not the queue's CLEAR chip, which
    //    destroys what you asked to hear NEXT. Two buttons because they are two things.
    let mut e = unlocked();
    e.set_play_context(vec![song(10), song(11)], 0);
    e.enqueue_at(song(99), 0, QueueAt::Later);
    e.track_started(11, false);
    assert_eq!(ids(&e), vec![10]);
    assert_eq!(e.queue_clear(), vec![Action::QueueChanged]);
    assert_eq!(
        ids(&e),
        vec![10],
        "clearing the queue leaves the history alone"
    );
    // …and the reverse: clearing the history leaves the picks alone, and asks the shell for
    // nothing, because nothing about the transport has changed.
    e.enqueue_at(song(98), 0, QueueAt::Later);
    assert_eq!(e.history_clear(), Vec::<Action>::new());
    assert!(e.history().is_empty());
    assert_eq!(
        upq(&e).len(),
        1,
        "clearing the history leaves Up Next alone"
    );
    assert_eq!(
        e.history_clear(),
        Vec::<Action>::new(),
        "clearing an empty history is a no-op"
    );

    // 7. A RESTORE IS TRIMMED TOO, so a hand-edited resume file cannot seed an unbounded list.
    let mut f = unlocked();
    f.history_restore((0..(HISTORY_MAX as i64 * 3)).map(song).collect());
    assert_eq!(f.history().len(), HISTORY_MAX);
    assert_eq!(
        f.history().last().map(|h| h.object_id),
        Some(HISTORY_MAX as i64 * 3 - 1)
    );
}

/// The CLEAR that empties it lives on the PREVIOUSLY PLAYED heading — a header this screen
/// otherwise treats as never tappable — so it must claim its own pixels and no others.
#[test]
fn repeat_album_laps_the_run_the_playing_song_is_in() {
    let mut a = unlocked();
    let row = |id: i64, album: i64, name: &str| SongRow {
        object_id: id,
        album_id: album,
        art: name.into(),
        ..Default::default()
    };
    // Album 7 (three songs), then a queued song from album 9, then album 7 again.
    let list = vec![
        row(1, 7, "Isles"),
        row(2, 7, "Isles"),
        row(3, 7, "Isles"),
        row(4, 9, "Untrue"),
        row(5, 7, "Isles"),
    ];
    a.set_play_context(list.clone(), 1);
    assert_eq!(
        a.album_run(),
        Some((0, 3)),
        "the run around the playing song, not every row of the album"
    );
    a.set_play_context(list.clone(), 4);
    assert_eq!(
        a.album_run(),
        Some((4, 5)),
        "the album's second place in the list is its own run"
    );
    a.set_play_context(list, 3);
    assert_eq!(a.album_run(), Some((3, 4)));
    // No album id (host/sample data): the album name decides.
    let named = vec![row(1, 0, "Isles"), row(2, 0, "Isles"), row(3, 0, "Other")];
    a.set_play_context(named, 0);
    assert_eq!(a.album_run(), Some((0, 2)));
    a.set_play_context(Vec::new(), 0);
    assert_eq!(a.album_run(), None, "nothing playing, nothing to lap");
}

#[test]
fn up_next_saves_as_a_playlist_from_the_now_playing_heading() {
    use crate::up_next::{Section, Slot};
    let mut a = unlocked();
    a.go(Screen::UpNext);
    let row = |i: i64, album: &str| SongRow {
        object_id: 10 + i,
        art: album.into(),
        ..Default::default()
    };
    a.set_play_context(
        vec![
            row(0, "Isles"),
            row(1, "Isles"),
            row(2, "Isles"),
            row(3, "Isles"),
        ],
        1,
    );
    a.queue_scroll_px = 0;
    let l = a.up_next_layout();
    let now_y = crate::chrome::HEADER_BOTTOM + l.top_of(Slot::Head(Section::Now)).unwrap() + 8;
    let x = crate::up_next::HIST_CLEAR_X0 + 4;
    assert!(crate::up_next::hit_now_save(&l, x, now_y, 0));
    assert!(
        !crate::up_next::hit_now_save(&l, 40, now_y, 0),
        "the label is not the button"
    );
    let row_y = crate::chrome::HEADER_BOTTOM + l.top_of(Slot::Current(1)).unwrap() + 8;
    assert!(
        !crate::up_next::hit_now_save(&l, x, row_y, 0),
        "the playing row keeps its own taps"
    );

    assert!(a.tap(x, now_y).is_empty());
    assert_eq!(a.current(), Screen::Keyboard);
    assert_eq!(a.text_input(), "Isles", "one album: its name is offered");
    assert_eq!(
        a.kb_title_for_test(),
        "Save Up Next",
        "the keyboard says what it is for"
    );
    assert_eq!(
        kb_key(&mut a, crate::keyboard::Key::Done),
        vec![Action::PlaylistCreateFromQueue]
    );
    assert_eq!(a.current(), Screen::UpNext, "Done goes back to Up Next");
    assert_eq!(
        a.queue_to_save(),
        vec![11, 12, 13],
        "the playing track and what follows, not what played"
    );

    // A list from more than one album is offered as "Up Next".
    a.set_play_context(vec![row(0, "Isles"), row(1, "Untrue")], 0);
    a.open_keyboard(KbPurpose::SaveQueue);
    assert_eq!(a.text_input(), "Up Next");
}

#[test]
fn the_history_clear_is_on_its_own_heading_and_nowhere_else() {
    use crate::up_next::Slot;
    let mut a = unlocked();
    a.go(Screen::UpNext);
    a.set_play_context(
        (0..6)
            .map(|i| SongRow {
                object_id: 10 + i,
                ..Default::default()
            })
            .collect(),
        2,
    );
    a.history_restore(
        (0..2)
            .map(|i| SongRow {
                object_id: 10 + i,
                ..Default::default()
            })
            .collect(),
    );
    a.queue_scroll_px = 0;
    let l = a.up_next_layout();
    let y_of = |slot| crate::chrome::HEADER_BOTTOM + l.top_of(slot).unwrap() + 8;
    let hdr_y = y_of(Slot::Head(crate::up_next::Section::History));

    // On the heading, in the control's column: it clears.
    assert!(crate::up_next::hit_history_clear(
        &l,
        crate::up_next::HIST_CLEAR_X0 + 4,
        hdr_y,
        0
    ));
    // Same row, left of the control — that is the label, and it is not a button.
    assert!(!crate::up_next::hit_history_clear(&l, 40, hdr_y, 0));
    // Same column, but on a ROW rather than the heading: the rows own their own taps.
    let row_y = y_of(Slot::History(0));
    assert!(!crate::up_next::hit_history_clear(
        &l,
        crate::up_next::HIST_CLEAR_X0 + 4,
        row_y,
        0
    ));
    // …and on another section's heading, which has no such control at all.
    let qhdr = y_of(Slot::Head(crate::up_next::Section::Now));
    assert!(!crate::up_next::hit_history_clear(
        &l,
        crate::up_next::HIST_CLEAR_X0 + 4,
        qhdr,
        0
    ));

    // The tap really goes through, and really empties it.
    assert!(a.tap(crate::up_next::HIST_CLEAR_X0 + 4, hdr_y).is_empty());
    assert!(
        a.history().is_empty(),
        "the heading's CLEAR empties the history"
    );
}

/// A reboot mid-pick comes back on the pick: it is the list's current row, and the list and
/// its index are what is saved.
#[test]
fn a_playing_pick_survives_a_reboot() {
    let mut a = unlocked();
    let album: Vec<SongRow> = (0..3)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(album, 0);
    a.enqueue_at(
        SongRow {
            title: "PICK".into(),
            object_id: 99,
            ..Default::default()
        },
        0,
        QueueAt::Next,
    );
    a.track_started(99, false);
    let body = a.playback_encode();
    assert!(
        body.contains("ctx=10,99,11,12") && body.contains("idx=1"),
        "{body}"
    );
    assert!(
        body.contains("hist=10"),
        "the history is not persisted: {body}"
    );

    let mut b = unlocked();
    b.playback_restore(a.context().to_vec(), 1, vec![], None, None);
    b.history_restore(vec![SongRow {
        object_id: 10,
        ..Default::default()
    }]);
    assert_eq!(b.context()[b.context_idx()].object_id, 99);
    assert_eq!(b.playback_encode(), body, "a restore must round-trip");
}

/// MIX must not deal the same hand after every boot. The seed starts at a constant so these
/// tests are reproducible; the shell replaces it once at start-up, and without that the Nth
/// press of MIX always produced the Nth permutation of the same generator.
#[test]
fn the_shuffle_seed_is_the_shells_to_set() {
    let ctx: Vec<SongRow> = (0..24)
        .map(|i| SongRow {
            title: format!("A{i}"),
            object_id: 10 + i,
            ..Default::default()
        })
        .collect();
    let mix = |seed: u64| {
        let mut a = unlocked();
        a.seed_shuffle(seed);
        a.set_play_context(ctx.clone(), 0);
        a.queue_shuffle();
        a.context().iter().map(|t| t.object_id).collect::<Vec<_>>()
    };
    // Two sessions, two seeds, two orders — the whole point.
    assert_ne!(mix(0x1234_5678_9abc_def0), mix(0x0fed_cba9_8765_4321));
    // The same seed still replays exactly, which is what keeps every other test here honest.
    assert_eq!(mix(0x1234_5678_9abc_def0), mix(0x1234_5678_9abc_def0));
    // Zero is refused rather than jamming the xorshift on its fixed point.
    let mut a = unlocked();
    a.seed_shuffle(0);
    a.set_play_context(ctx.clone(), 0);
    a.queue_shuffle();
    assert_ne!(
        a.context().iter().map(|t| t.object_id).collect::<Vec<_>>(),
        ctx.iter().map(|t| t.object_id).collect::<Vec<_>>(),
    );
}

#[test]
fn album_row_swipe_queues_album() {
    let mut a = with_playing(unlocked(), 1);
    a.stack = vec![Screen::Library];
    a.lib_tab = Tab::Albums;
    let album = a.lib.albums_flat()[0].clone();
    let y = (0..800)
        .find(|&y| a.albums_album_at(y).map(|al| al.name) == Some(album.name.clone()))
        .expect("albums list should contain the first album row");
    assert_eq!(a.swipe(1, 240, y), vec![Action::QueueChanged]);
    assert_eq!(upq(&a).len(), album.track_list.len());
}

#[test]
fn usb_storage_modal_back_exits() {
    let mut a = unlocked();
    a.push(Screen::UsbStorage);
    // Modal: taps dead, Back pops AND emits the exit action for the shell.
    assert!(a.tap(240, 400).is_empty());
    assert_eq!(a.current(), Screen::UsbStorage);
    assert_eq!(a.press(Button::Back), vec![Action::ExitUsbMsc]);
    assert!(a.current() != Screen::UsbStorage);
}

#[test]
fn auto_show_usb_storage_is_idempotent() {
    // The shell auto-raises the modal when it detects a PC host; the ~1 Hz poll may call this
    // every tick, so it must be idempotent (no stacked screens) and exit must still pop cleanly.
    let mut a = unlocked();
    assert!(!a.is_usb_storage());
    a.show_usb_storage();
    assert!(a.is_usb_storage());
    let depth = a.stack.len();
    a.show_usb_storage(); // repeat poll → no-op, no second screen pushed
    assert!(a.is_usb_storage());
    assert_eq!(a.stack.len(), depth);
    // Same single exit path as a manual entry: Back pops the modal + emits ExitUsbMsc.
    assert_eq!(a.press(Button::Back), vec![Action::ExitUsbMsc]);
    assert!(!a.is_usb_storage());
}

#[test]
fn pixel_scroll_and_fling_move_library_window() {
    let mut a = unlocked();
    a.tap(344, 22); // Menu
    a.tap(200, 91 + 63 + 8); // Library
    let max = library::max_scroll_px(a.lib_tab, &a.lib, 0, None);
    a.scroll_px(120); // live drag → later rows (clamped to content)
    assert_eq!(a.lib_scroll_px, 120.min(max));
    a.scroll_px(-10_000); // clamp at 0
    assert_eq!(a.lib_scroll_px, 0);
    // Fling: velocity integrates over ticks, decays, and eventually stops.
    a.fling(1200.0);
    let mut ticks = 0;
    while a.tick() && ticks < 600 {
        ticks += 1;
    }
    assert!(ticks < 600, "fling must decay to a stop");
    assert!(a.lib_scroll_px >= 0 && a.lib_scroll_px <= max);
    assert_eq!(a.fling_v, 0.0);
    // Touching down again kills momentum instantly.
    a.fling(1200.0);
    a.stop_fling();
    assert_eq!(a.fling_v, 0.0);
}

#[test]
fn up_next_empty_when_nothing_playing() {
    let a = unlocked();
    assert!(a.now_playing_queue("", "").is_none());
    assert!(a.now_playing_queue("No Such Track", "Nobody").is_none());
}

#[test]
fn now_playing_queue_finds_current_album() {
    let a = unlocked();
    // a real track from the sample library's first populated album
    let (album, title, artist) = a
        .lib
        .album_groups
        .iter()
        .flat_map(|g| &g.albums)
        .find(|al| !al.track_list.is_empty())
        .map(|al| {
            (
                al.name.clone(),
                al.track_list[0].title.clone(),
                al.track_list[0].artist.clone(),
            )
        })
        .expect("sample lib has an album with tracks");
    let (qa, tracks, idx) = a
        .now_playing_queue(&title, &artist)
        .expect("resolves to the album");
    assert_eq!(qa, album);
    assert_eq!(idx, 0);
    assert!(!tracks.is_empty());
}

#[test]
fn sound_screen_toggles_effects_and_emits_action() {
    let mut a = open_from_menu(Screen::Sound);
    assert_eq!(a.current(), Screen::Sound);
    assert_eq!(a.sound_flags(), 0); // all effects off initially
                                    // toggle DSEE (row 0)
    let acts = a.press(Button::Select);
    assert_eq!(acts, vec![Action::SoundChanged]);
    assert_eq!(a.sound_flags() & 1, 1);
    // The Dynamic Normalizer is the next row DOWN (the cursor walks `sound::ORDER`, the order
    // the rows are drawn in, not the order of their ids) -> bit4 set
    a.press(Button::Down);
    assert_eq!(a.sound_sel, crate::sound::ROW_NORM);
    a.press(Button::Select);
    assert_eq!(a.sound_flags() & (1 << 4), 1 << 4);
    // ClearAudio+ is a segment of the mode control now -> bit5 set, and Direct clears it.
    let (x, y, w, h) = crate::sound::mode_rect(crate::sound::MODE_CLEARAUDIO, 0, 3);
    assert_eq!(a.tap(x + w / 2, y + h / 2), vec![Action::SoundChanged]);
    assert_eq!(a.sound_flags() & (1 << 5), 1 << 5);
    assert!(
        a.tap(x + w / 2, y + h / 2).is_empty(),
        "the live mode is a no-op"
    );
    let (x, y, w, h) = crate::sound::mode_rect(crate::sound::MODE_DIRECT, 0, 3);
    a.tap(x + w / 2, y + h / 2);
    assert_eq!(a.sound_flags() & (1 << 5), 0);
    assert_eq!(a.adv_flags() & 1, 1, "Direct is Source Direct");
    // The cursor stops at both ends of the drawn order.
    for _ in 0..crate::sound::ROWS + 2 {
        a.press(Button::Up);
    }
    assert_eq!(a.sound_sel, crate::sound::ROW_PROFILE);
    for _ in 0..crate::sound::ROWS + 2 {
        a.press(Button::Down);
    }
    assert_eq!(a.sound_sel, crate::sound::ROW_ADVANCED);
}

// ── sound profiles per output (R5) ────────────────────────────────────────────────────────

/// A profile is EVERY DSP value: the Advanced rows travel with A/B now, and the two things that
/// are not tunings — mono and the jack's own amplifier — do not.
#[test]
fn a_profile_carries_the_advanced_rows_but_not_the_hardware() {
    let mut a = unlocked();
    a.set_adv_flags(0b11_1111); // every Advanced switch on, the linear amp included
    a.set_dsee_mode(3);
    a.set_vinyl_type(2);
    a.set_tone_bands([4, -6, 8]);
    a.set_mono(true);
    assert_eq!(a.select_setup(1), vec![Action::SoundChanged]);
    // B is a fresh profile: none of A's Advanced values came with it…
    assert_eq!(
        a.adv_flags() & 0b1_1111,
        0,
        "B inherited A's Advanced switches"
    );
    assert_eq!(
        (a.dsee_mode(), a.vinyl_type(), a.tone_bands()),
        (0, 0, [0, 0, 0])
    );
    // …but the amp and mono are not the profile's to change.
    assert_eq!(
        a.adv_flags() & (1 << 5),
        1 << 5,
        "the linear amp is hardware, not a profile value"
    );
    assert!(a.mono());
    // And A comes back whole.
    a.select_setup(0);
    assert_eq!(a.adv_flags(), 0b11_1111);
    assert_eq!(
        (a.dsee_mode(), a.vinyl_type(), a.tone_bands()),
        (3, 2, [4, -6, 8])
    );
}

/// Mono, the linear amp and the DAC EQ are one value for both sides of the compare until
/// switched in on the Presets screen; switching one in changes nothing heard.
#[test]
fn mono_amp_and_dac_eq_follow_the_sound_only_when_asked() {
    use crate::presets::{centre, Hit};
    let mut a = unlocked();
    assert_eq!(a.profile_follow(), 0, "off by default");
    a.set_mono(true);
    a.set_dac_eq([2, 0, -2, 0, 4]);
    a.select_setup(1);
    assert!(a.mono() && a.dac_eq() == [2, 0, -2, 0, 4]);
    a.select_setup(0);
    a.stack = vec![Screen::Presets];
    let before = a.setup();
    for bit in [FOLLOW_MONO, FOLLOW_DAC_EQ] {
        assert!(a.tap(240, centre(Hit::Follow(bit), 0)).is_empty());
    }
    assert_eq!(a.profile_follow(), FOLLOW_MONO | FOLLOW_DAC_EQ);
    assert_eq!(a.setup(), before);
    a.select_setup(1);
    a.set_mono(false);
    a.select_setup(0);
    assert!(a.mono(), "A's mono came back");
}

/// A/B is a compare: a route change never switches sides any more.
#[test]
fn a_route_change_leaves_the_compare_alone() {
    let mut a = unlocked();
    a.select_setup(1);
    a.set_bt_route(true);
    assert_eq!(a.setup_idx(), 1);
    a.set_bt_route(false);
    assert_eq!(a.setup_idx(), 1);
}

/// Save keeps the live sound; Load puts it back on the live side; Delete needs two taps.
#[test]
fn presets_save_load_and_delete() {
    use crate::presets::{centre, Hit, MAX};
    let mut a = unlocked();
    a.go(Screen::Presets);
    a.snd_dsee = true;
    // Save names it first: the keyboard opens on "Preset 1", Back saves nothing, DONE saves.
    assert!(a.tap(100, centre(Hit::Save, 0)).is_empty());
    assert_eq!(a.current(), Screen::Keyboard);
    assert_eq!(a.kb_text, "Preset 1");
    a.pop();
    assert!(
        a.presets().is_empty(),
        "Back from the keyboard saves nothing"
    );
    a.tap(100, centre(Hit::Save, 0));
    a.kb_text = "Late night".into();
    assert!(kb_key(&mut a, crate::keyboard::Key::Done).is_empty());
    assert_eq!(a.current(), Screen::Presets);
    assert_eq!(a.presets().len(), 1);
    assert_eq!(a.preset_names(), &["Late night".to_string()]);
    assert!(describe_setup(&a.presets()[0]).ends_with("DSEE HX"));
    a.snd_dsee = false;
    assert_eq!(
        a.tap(100, centre(Hit::Load(0), 1)),
        vec![Action::SoundChanged]
    );
    assert!(a.snd_dsee, "the preset came back");
    // RENAME, beside DELETE: the keyboard opens on the name; "Preset N" is kept as no name.
    let rename_x = crate::canvas::W as i32 - 150;
    a.tap(rename_x, centre(Hit::Load(0), 1));
    assert_eq!(
        (a.current(), a.kb_text.as_str()),
        (Screen::Keyboard, "Late night")
    );
    a.kb_text = "Preset 1".into();
    kb_key(&mut a, crate::keyboard::Key::Done);
    assert_eq!(a.preset_names(), &[String::new()]);
    for _ in 1..MAX + 2 {
        a.tap(100, centre(Hit::Save, a.presets().len()));
        if a.current() == Screen::Keyboard {
            kb_key(&mut a, crate::keyboard::Key::Done);
        }
    }
    assert_eq!(a.presets().len(), MAX, "never more than MAX");
    assert_eq!(
        a.current(),
        Screen::Presets,
        "a full list does not open the keyboard"
    );
    let x = crate::canvas::W as i32 - 10;
    a.tap(x, centre(Hit::Load(0), MAX));
    assert_eq!(a.presets().len(), MAX, "the first tap only arms");
    a.tap(x, centre(Hit::Load(0), MAX));
    assert_eq!(a.presets().len(), MAX - 1);
}

/// Track info's ALBUM SOUND: a tap steps Off → Preset 1 → … → Off; a track from another album
/// puts back the sound from before, and coming back to the album loads its preset again.
#[test]
fn album_sound_follows_the_playing_album() {
    use crate::model::SongRow;
    let song = |id: i64, album: &str| SongRow {
        title: format!("t{id}"),
        art: album.into(),
        object_id: id,
        ..Default::default()
    };
    let mut a = unlocked();
    a.set_play_context(
        vec![song(1, "A"), song(2, "A"), song(3, "B"), song(4, "A")],
        0,
    );
    a.go(Screen::TrackInfo);
    let (x, y) = (240, crate::track_info::BOTTOM + 10);
    a.tap(x, y);
    assert_eq!(
        a.current(),
        Screen::PlaylistPick,
        "no presets: the band is Add to playlist"
    );
    a.go(Screen::TrackInfo);
    a.snd_dsee = true;
    a.presets.push(a.setup());
    a.snd_dsee = false;
    assert_eq!(a.tap(x, y), vec![Action::SoundChanged]);
    assert!(a.snd_dsee, "album A plays with Preset 1");
    assert_eq!(a.album_sounds().len(), 1);
    // The next track is the same album: nothing to do.
    a.track_started(2, false);
    assert!(!a.album_sound_follow() && a.snd_dsee);
    // Another album: the sound from before comes back, and the shell is asked to apply it.
    a.track_started(3, false);
    assert!(a.album_sound_follow());
    assert!(!a.snd_dsee && a.take_profile_apply());
    // Back on album A: its preset again.
    a.track_started(4, false);
    assert!(a.album_sound_follow() && a.snd_dsee);
    // Stepping past the last preset is Off, which forgets the album and restores.
    assert_eq!(a.tap(x, y), vec![Action::SoundChanged]);
    assert!(a.album_sounds().is_empty() && !a.snd_dsee);
}

/// Track info's SONG control: a song's preset wins over its album's, only for that song.
#[test]
fn a_song_preset_wins_over_its_album() {
    use crate::model::SongRow;
    let song = |id: i64| SongRow {
        title: format!("t{id}"),
        art: "A".into(),
        object_id: id,
        ..Default::default()
    };
    let mut a = unlocked();
    a.set_play_context(vec![song(1), song(2), song(3)], 0);
    a.snd_dsee = true;
    a.presets.push(a.setup()); // Preset 1: DSEE on
    a.snd_dsee = false;
    a.snd_vinyl = true;
    a.presets.push(a.setup()); // Preset 2: vinyl on, DSEE off
    a.snd_vinyl = false;
    a.go(Screen::TrackInfo);
    let y = crate::track_info::BOTTOM + 10;
    let (album, song_x) = (240, 400);
    a.tap(album, y);
    assert!(a.snd_dsee, "album A: Preset 1");
    // Song 2 gets Preset 2 (two taps); it is not the playing song, so nothing changes yet.
    a.track_started(2, false);
    a.album_sound_follow();
    a.go(Screen::TrackInfo);
    a.tap(song_x, y);
    assert_eq!(a.tap(song_x, y), vec![Action::SoundChanged]);
    assert!(
        a.snd_vinyl && !a.snd_dsee,
        "song 2: Preset 2 over the album's"
    );
    // Song 3: back to the album's preset; song 2 again: its own.
    a.track_started(3, false);
    assert!(a.album_sound_follow() && a.snd_dsee && !a.snd_vinyl);
    a.track_started(2, false);
    assert!(a.album_sound_follow() && a.snd_vinyl);
    assert_eq!(a.album_sounds().len(), 2, "one album entry, one song entry");
}

/// Deleting a preset drops the albums that used it and renumbers the ones after it.
#[test]
fn deleting_a_preset_renumbers_album_sounds() {
    use crate::presets::{centre, Hit};
    let mut a = unlocked();
    a.presets = vec![a.setup(); 3];
    a.restore_album_sounds(vec![
        ("x".into(), 1),
        ("y".into(), 2),
        ("z".into(), 3),
        ("w".into(), 9),
    ]);
    assert_eq!(
        a.album_sounds().len(),
        3,
        "a preset that is not saved is dropped"
    );
    a.go(Screen::Presets);
    let x = crate::canvas::W as i32 - 10;
    a.tap(x, centre(Hit::Load(1), 3));
    a.tap(x, centre(Hit::Load(1), 3));
    assert_eq!(
        a.album_sounds(),
        &[("x".to_string(), 1), ("z".to_string(), 2)]
    );
}

/// "Copy A to B" makes the other side the same as the live one, and changes nothing heard.
#[test]
fn copy_makes_the_other_side_match_the_live_one() {
    let mut a = unlocked();
    a.snd_vpt = true;
    a.snd_vpt_mode = 2;
    a.go(Screen::Presets);
    assert!(a
        .tap(240, crate::presets::centre(crate::presets::Hit::Copy, 0))
        .is_empty());
    assert_eq!(a.setup_inactive(), a.setup());
    a.select_setup(1);
    assert!(a.snd_vpt && a.snd_vpt_mode == 2);
}

/// Volume curve: nothing to edit without the helper; with it, a tap or a drag sets a point,
/// switches the curve on and asks the shell to apply it once; a point never passes its
/// neighbours; the switch puts the installer's curve back.
#[test]
fn the_volume_curve_screen() {
    use crate::vol_curve as vc;
    let mut a = unlocked();
    a.go(Screen::Advanced);
    a.adv_scroll_px = crate::advanced::max_scroll(false);
    let y = crate::advanced::row_top(crate::advanced::ROW_VOL_CURVE, false, a.adv_scroll_px);
    assert!(a.tap(240, y + 20).is_empty());
    assert_eq!(a.current(), Screen::VolCurve, "Advanced ▸ Volume curve");
    let x = vc::point_x(3);
    assert!(
        a.tap(x, vc::FIELD_TOP + 5).is_empty(),
        "no helper: nothing changes"
    );
    assert_eq!(a.vol_curve(), None);
    a.set_vol_curve_available(true);
    assert_eq!(a.tap(x, vc::FIELD_TOP + 5), vec![Action::VolCurveChanged]);
    assert_eq!(a.vol_curve().map(|p| p[3]), Some(61));
    assert!(a.scrub_begin(x, vc::knob_y(5)));
    assert!(a.scrub_move(x, vc::knob_y(10)).is_empty());
    assert_eq!(a.scrub_end(), vec![Action::VolCurveChanged]);
    assert_eq!(
        a.vol_curve().map(|p| p[3]),
        Some(40),
        "held at the point before"
    );
    assert_eq!(
        a.tap(400, vc::FOOTER_TOP + 10),
        vec![Action::VolCurveChanged]
    );
    assert_eq!(a.vol_curve(), Some(vc::EVERY_STEP));
    assert_eq!(
        a.tap(240, vc::SWITCH_TOP + 10),
        vec![Action::VolCurveChanged]
    );
    assert_eq!(a.vol_curve(), None, "off: the installer's curve");
    assert_eq!(a.vol_curve_points().0, vc::EVERY_STEP, "the edit is kept");
}

/// On the jack the curve is the DAC's: Sony's 10-band gets flat bands, the Equalizer row opens
/// the DAC EQ, and moving to Bluetooth hands the 10-band back (with a re-apply).
#[test]
fn the_jack_eq_lives_in_the_dac_by_default() {
    let mut a = unlocked();
    a.set_eq_bands([4; 10]);
    assert!(a.eq_on_dac());
    assert_eq!(a.eq_bands_for_dsp(), [0; 10]);
    a.go(Screen::Sound);
    let y = crate::sound::row_top(crate::sound::ROW_EQ, 0) + crate::sound::ROW_H() / 2;
    a.tap(240, y);
    assert_eq!(a.current(), Screen::DacEq);
    a.set_bt_route(true);
    assert!(a.take_profile_apply());
    assert_eq!(a.eq_bands_for_dsp(), [4; 10]);
    a.set_jack_eq_dac(false);
    a.set_bt_route(false);
    assert!(!a.take_profile_apply());
    assert_eq!(a.eq_bands_for_dsp(), [4; 10]);
}

/// A preset chip loads its curve once; the same chip again changes nothing; a slider moved off
/// it leaves the curve as edited.
#[test]
fn a_dac_eq_preset_loads_its_curve() {
    let mut a = unlocked();
    a.go(Screen::DacEq);
    let (x, w) = crate::kit::chip_span(0, crate::dac_eq::PRESETS.len());
    let y = crate::dac_eq::PRESET_TOP + 10;
    assert_eq!(a.tap(x + w / 2, y), vec![Action::DacEqChanged]);
    assert_eq!(a.dac_eq(), crate::dac_eq::PRESETS[0].1);
    assert!(a.tap(x + w / 2, y).is_empty(), "already loaded");
    a.tap(
        crate::dac_eq::band_center_x(4),
        crate::dac_eq::FIELD_TOP + 20,
    );
    assert_ne!(a.dac_eq(), crate::dac_eq::PRESETS[0].1);
    assert_eq!(crate::dac_eq::preset_of(&a.dac_eq()), None);
}

/// The Presets row is a route and does not re-apply the chain.
#[test]
fn the_presets_row_opens_presets() {
    let mut a = unlocked();
    a.go(Screen::Sound);
    let y = crate::sound::row_top(crate::sound::ROW_PROFILE, 0) + crate::sound::ROW_H() / 2;
    assert!(a.tap(240, y).is_empty());
    assert_eq!(a.current(), Screen::Presets);
}

/// Noise cancelling exists only while Sony's earphones are in: the row pushes the list down one
/// row without moving any other target, the choice is kept, and pulling the plug closes it.
#[test]
fn noise_cancelling_follows_the_plug() {
    let mut a = unlocked();
    a.go(Screen::Sound);
    let presets = crate::sound::row_top(crate::sound::ROW_PROFILE, 0) + 10;
    assert_eq!(a.nc_items(), None);
    assert!(a.set_nc_plug(true, false));
    // The old Presets position is now the NC row; Presets moved down by exactly one row.
    a.tap(240, crate::sound::TOP + 10);
    assert_eq!(a.current(), Screen::Nc);
    assert_eq!(a.nc_items(), Some(("off", "nw500n")));
    let auto = crate::nc::centre(crate::nc::Hit::Mode(1), 0, false);
    let nc750 = crate::nc::centre(crate::nc::Hit::Model(1), 0, false);
    a.tap(240, auto);
    a.tap(240, nc750);
    assert_eq!(a.nc_items(), Some(("ainc", "nw750n")));
    a.pop();
    a.tap(240, presets + crate::sound::NC_H());
    assert_eq!(a.current(), Screen::Presets);
    a.pop();
    // Ambient is written as off where it cannot run.
    a.set_nc(crate::nc::MODE_AMBIENT, crate::nc::MODEL_NW500N);
    assert_eq!(a.nc_items(), Some(("off", "nw500n")));
    a.set_nc_plug(true, true);
    assert_eq!(a.nc_items(), Some(("ambient1", "nw500n")));
    a.tap(240, crate::sound::TOP + 10);
    assert_eq!(a.current(), Screen::Nc);
    assert!(a.set_nc_plug(false, true));
    assert_ne!(a.current(), Screen::Nc);
    assert_eq!(a.nc_items(), None);
    assert_eq!(
        a.nc_mode(),
        crate::nc::MODE_AMBIENT,
        "the choice survives the unplug"
    );
}

/// Advanced holds the effects that used to fold out of Sound: VPT there is the Sound
/// screen's VPT, the list scrolls, and a tap lands on the row under the finger at that scroll.
#[test]
fn advanced_scrolls_and_carries_the_moved_effects() {
    use crate::advanced as adv;
    let mut a = unlocked();
    a.go(Screen::Sound);
    a.tap(
        240,
        crate::sound::row_top(crate::sound::ROW_ADVANCED, 0) + 20,
    );
    assert_eq!(a.current(), Screen::Advanced);
    assert!(!a.snd_vpt);
    let y = adv::row_top(adv::ROW_VPT, false, 0) + 20;
    assert_eq!(a.tap(240, y), vec![Action::SoundChanged]);
    assert!(a.snd_vpt, "the Sound screen's own VPT");
    a.scroll_px(10_000);
    assert_eq!(a.adv_scroll_px, adv::max_scroll(false));
    assert!(a.adv_scroll_px > 0);
    a.tap(
        240,
        adv::row_top(adv::ROW_DAC_EQ, false, a.adv_scroll_px) + 20,
    );
    assert_eq!(a.current(), Screen::DacEq);
}

// ── Power button hold -> Power menu ───────────────────────────────────────────────────────

/// Holding Power opens the menu, and each row emits its own action. The whole gesture exists
/// because acting on the PRESS meant a hold could only ever blank the screen.
#[test]
fn holding_power_opens_a_menu_whose_rows_act() {
    use crate::confirm::{hit, Ask, Hit};
    for (want, act) in [
        (Hit::PowerOff, Action::PowerOff),
        (Hit::Restart, Action::Restart),
    ] {
        let mut a = unlocked();
        assert!(a.power_held(), "the hold should open the menu");
        assert!(a.modal_open());
        // Find a pixel that really is on the row we mean, then tap exactly there.
        let (x, y) = (0..crate::canvas::H as i32)
            .find_map(|y| (hit(Ask::PowerMenu, 240, y) == want).then_some((240, y)))
            .expect("row has no tappable pixel");
        assert_eq!(a.tap(x, y), vec![act]);
        assert!(!a.modal_open(), "answering must close the modal");
    }
}

/// Cancel — and the dimmed backdrop — must do nothing at all. A power menu that can be
/// dismissed into a power-off is the one failure this whole widget exists to prevent.
#[test]
fn cancelling_the_power_menu_does_nothing() {
    use crate::confirm::{hit, Ask, Hit};
    let cancel_y = (0..crate::canvas::H as i32)
        .find(|y| hit(Ask::PowerMenu, 240, *y) == Hit::Cancel && *y > 400)
        .expect("no Cancel row");
    for (x, y) in [(240, cancel_y), (5, 5), (240, 60)] {
        let mut a = unlocked();
        assert!(a.power_held());
        assert!(a.tap(x, y).is_empty(), "({x},{y}) must not act");
        assert!(!a.modal_open(), "the tap must still dismiss");
    }
}

/// Hold engaged = the device is in a pocket. Nothing may open a power menu in there, and a
/// refused hold must not leave a modal behind.
#[test]
fn a_locked_device_refuses_the_power_menu() {
    let mut a = unlocked();
    a.set_hold(true);
    assert!(!a.power_held());
    assert!(!a.modal_open());
}

/// A second hold must not stack or silently replace the question already on screen.
#[test]
fn a_second_hold_does_not_restack_the_menu() {
    let mut a = unlocked();
    assert!(a.power_held());
    assert!(!a.power_held(), "the menu is already up");
    assert!(a.modal_open());
}

/// Back is the second escape from a modal, and every other button is swallowed — a transport
/// press must not reach the music underneath a "Power off?" prompt.
#[test]
fn back_dismisses_a_modal_and_other_buttons_are_swallowed() {
    let mut a = unlocked();
    assert!(a.power_held());
    for b in [
        Button::Play,
        Button::Next,
        Button::Prev,
        Button::Select,
        Button::Up,
    ] {
        assert!(a.press(b).is_empty(), "{b:?} leaked through the modal");
        assert!(a.modal_open(), "{b:?} should not have closed it");
    }
    assert!(a.press(Button::Back).is_empty());
    assert!(!a.modal_open(), "Back must dismiss");
}

/// The metadata block opens Track information, and it must not have stolen a target from the
/// controls it sits between — the heart above-right of it and the progress rail below.
#[test]
fn the_metadata_block_opens_track_information() {
    use crate::now_playing as np;
    let mut a = unlocked();
    a.go(Screen::NowPlaying);
    a.set_track_info(vec![
        ("Title".into(), "Atlas Hands".into()),
        ("File".into(), "/contents/Music/x.flac".into()),
    ]);

    // The band itself, well left of the heart.
    let y = (np::INFO_TOP + np::INFO_BOT) / 2;
    assert!(np::hit_info(120, y, false));
    assert!(a.tap(120, y).is_empty());
    assert_eq!(a.current(), Screen::TrackInfo);
    assert_eq!(a.track_info_rows().len(), 2);
    assert!(a.press(Button::Back).is_empty());
    assert_eq!(a.current(), Screen::NowPlaying, "Back returns to the track");

    // The heart's square keeps its own taps.
    assert!(
        !np::hit_info(np::HEART_CX, np::HEART_CY, false),
        "the block must not swallow the heart"
    );
    assert_eq!(a.tap(np::HEART_CX, np::HEART_CY), vec![Action::ToggleLiked]);
    assert_eq!(a.current(), Screen::NowPlaying);

    // And it stops above the rail's grab band, which owns drag-to-seek.
    assert!(!np::hit_info(120, np::RAIL_GRAB_TOP, false));
    assert!(!np::hit_info(120, np::RAIL_Y, false));

    // A track change replaces the rows and rewinds the scroll — a shorter list must not open
    // part-way down because the last one was long.
    a.push(Screen::TrackInfo);
    a.scroll_px(400);
    a.set_track_info(vec![("Title".into(), "Something else".into())]);
    assert_eq!(a.track_info_rows().len(), 1);
    assert_eq!(a.track_info_scroll_px, 0);
}

#[test]
fn reset_switches_every_opt_in_off_with_what_it_drives() {
    let mut a = unlocked();
    for o in Opt::ALL {
        a.set_opt(o, true);
    }
    assert!(a.lib.folders_tab);
    a.reset_settings();
    assert!(Opt::ALL.into_iter().all(|o| !a.opt(o)));
    assert!(
        !a.lib.folders_tab,
        "the library kept across Reset loses its FOLDERS link too"
    );
}

/// A reset must put the PREFERENCES back and leave everything else alone. The two halves are
/// asserted together because the bug in either direction is silent: a reset that keeps a
/// preference looks like the button did nothing, and a reset that clears the library or stops
/// the music looks like a factory wipe.
#[test]
fn reset_settings_restores_defaults_without_touching_playback_or_the_library() {
    // SETS the global text scale (150% below), so it must hold the crate-wide lock or it
    // corrupts any concurrent test that measures text. This one was the source of the
    // intermittent `track_info::long_values_wrap_inside_the_value_column` failure.
    let _scale = lock_scale();
    use crate::confirm::{hit, Ask, Hit};
    let mut a = unlocked();
    // Move a spread of preferences off their defaults, one from each family.
    a.night = !App::default().night;
    a.set_ui_scale_pct(150);
    a.set_eq_bands([9; 10]);
    a.set_sound_flags(0b0111_1111);
    a.set_balance(0);
    a.set_brightness(1);
    a.lib.filter_hires = true;
    a.lib.filter_genre = Some(a.lib.genres[0].id);
    let scale_default = App::default().ui_scale_pct();

    // …and set up state a reset must NOT disturb.
    let rows: Vec<SongRow> = (0..4)
        .map(|i| SongRow {
            title: format!("T{i}"),
            object_id: 50 + i,
            ..Default::default()
        })
        .collect();
    a.set_play_context(rows, 2);
    a.enqueue_at(
        SongRow {
            title: "P".into(),
            object_id: 900,
            ..Default::default()
        },
        0,
        QueueAt::Later,
    );
    a.playing = true;
    let songs = a.lib.songs.len();

    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_RESET;
    assert!(
        a.press(Button::Select).is_empty(),
        "the row itself must not act"
    );
    assert!(a.modal_open(), "reset is destructive — it asks first");
    // Engage Hold with the card up: the modal is answerable while locked (its branch runs
    // ahead of the lock check, deliberately), and this is the state that proves a reset
    // cannot desync the UI from the physical switch.
    a.set_hold(true);
    let y = (0..crate::canvas::H as i32)
        .find(|&y| hit(Ask::ResetSettings, 360, y) == Hit::Confirm)
        .expect("no confirm pixel");
    assert_eq!(a.tap(360, y), vec![Action::SettingsReset]);

    // Preferences: back to default.
    assert_eq!(a.night, App::default().night);
    assert_eq!(a.ui_scale_pct(), scale_default);
    assert_eq!(a.eq_bands(), App::default().eq_bands());
    assert_eq!(a.sound_flags(), App::default().sound_flags());
    assert_eq!(a.balance(), App::default().balance());
    assert_eq!(a.brightness(), App::default().brightness());
    assert!(
        !a.lib.filtered(),
        "an invisible filter is exactly what a reset is for"
    );

    // Everything else: untouched.
    assert_eq!(
        a.lib.songs.len(),
        songs,
        "a preference reset is not a library wipe"
    );
    assert_eq!(
        a.context().len(),
        5,
        "the music keeps playing, the hand-queued track included"
    );
    assert_eq!(a.context_idx(), 2);
    assert!(
        a.hand_added,
        "the hand-built list is the user's, not a setting"
    );
    assert!(a.playing);
    assert!(
        a.locked,
        "the Hold switch is hardware truth, not a preference"
    );
    assert!(
        a.onboarding_seen(),
        "a settings reset is not a factory reset"
    );
    assert_eq!(
        a.current(),
        Screen::Settings,
        "it lands back where it was asked for"
    );
}

/// The Settings rows still raise their own two-button confirms — the menu is an addition, not
/// a replacement, and each card must answer for its own question.
#[test]
fn settings_rows_still_confirm_their_own_action() {
    use crate::confirm::{hit, Ask, Hit};
    for (row, ask, act) in [
        (crate::settings::ROW_RESTART, Ask::Restart, Action::Restart),
        (
            crate::settings::ROW_POWER_OFF,
            Ask::PowerOff,
            Action::PowerOff,
        ),
    ] {
        let mut a = unlocked();
        a.go(Screen::Settings);
        a.settings_sel = row;
        assert!(
            a.press(Button::Select).is_empty(),
            "the row itself must not act"
        );
        assert!(a.modal_open());
        let (x, y) = (0..crate::canvas::H as i32)
            .find_map(|y| (hit(ask, 240 + 120, y) == Hit::Confirm).then_some((360, y)))
            .expect("no confirm pixel");
        assert_eq!(a.tap(x, y), vec![act]);
    }
}

/// Receiver ▸ Use Sony's receiver restarts the device, so the row only ASKS; the confirm acts.
#[test]
fn sony_receiver_row_asks_before_restarting() {
    use crate::confirm::{hit, Ask, Hit};
    let mut a = unlocked();
    a.push(Screen::Receiver);
    assert!(
        a.tap(240, crate::receiver::SONY_Y + 10).is_empty(),
        "the row itself must not act"
    );
    assert!(a.modal_open());
    let (x, y) = (0..crate::canvas::H as i32)
        .find_map(|y| (hit(Ask::SonyReceiver, 360, y) == Hit::Confirm).then_some((360, y)))
        .expect("no confirm pixel");
    assert_eq!(a.tap(x, y), vec![Action::BootToSonyReceiver]);
}

// ── Playing a song while a hand-built queue exists ────────────────────────────────────────

/// With an empty queue, playing is immediate — the prompt must not appear for the common case.
#[test]
fn an_empty_queue_plays_without_asking() {
    let mut a = unlocked();
    assert_eq!(a.start_play(77), vec![Action::PlayIndex(77)]);
    assert!(!a.modal_open());
}

/// With tracks queued it asks first, and each answer does exactly one thing.
#[test]
fn a_queued_song_asks_before_replacing_the_queue() {
    use crate::confirm::{hit, Ask, Hit};
    let pick = |want: Hit| {
        (0..crate::canvas::H as i32)
            .find(|y| hit(Ask::QueueOnPlay, 240, *y) == want)
            .expect("row has no tappable pixel")
    };

    // REPLACE: the play action replaces the list — no separate QueueChanged, which would only
    // be a stale flush against a list about to be replaced.
    let mut a = unlocked();
    a.queue_push_for_test();
    assert!(
        a.start_play(77).is_empty(),
        "the tap must not play until answered"
    );
    assert!(a.modal_open());
    let acts = a.tap(240, pick(Hit::ClearQueue));
    assert_eq!(acts, vec![Action::PlayIndex(77)]);

    // KEEP: plays, list untouched until the new sequence arrives, and no QueueChanged.
    let mut a = unlocked();
    a.queue_push_for_test();
    let before = upq(&a).len();
    assert!(a.start_play(77).is_empty());
    assert_eq!(
        a.tap(240, pick(Hit::KeepQueue)),
        vec![Action::PlayIndex(77)]
    );
    assert_eq!(upq(&a).len(), before, "keeping must not touch the list");
    assert!(a.queue_keep, "and the new sequence will keep it");

    // CANCEL: nothing plays, nothing changes, and the pending song is dropped so it cannot
    // leak into the NEXT prompt.
    let mut a = unlocked();
    a.queue_push_for_test();
    assert!(a.start_play(77).is_empty());
    assert!(a.tap(5, 5).is_empty());
    assert!(!a.modal_open());
    assert_eq!(upq(&a).len(), 1);
    assert!(
        a.pending_play.is_none(),
        "a cancelled song must not survive the dialog"
    );
}

/// Open the Library on a given tab, the way a user does.
fn library_on(tab: Tab) -> App {
    let mut a = unlocked();
    a.go(Screen::Library);
    a.lib_tab = tab;
    a
}

/// The y of Artists row `i`, from the same geometry the renderer uses.
fn artist_row_y(i: usize) -> i32 {
    library::list_top(Tab::Artists) + library::row_h(Tab::Artists) * i as i32 + 20
}

#[test]
fn an_artist_row_opens_that_artists_page_and_back_returns_to_the_list() {
    let mut a = library_on(Tab::Artists);
    assert!(
        a.lib.artists.len() > 1,
        "sample library must have artists to open"
    );
    // Tap the SECOND row — row 0 would also be the default index, so it could pass by accident.
    assert!(
        a.tap(200, artist_row_y(1)).is_empty(),
        "opening a page plays nothing"
    );
    assert_eq!(a.current(), Screen::Artist);
    assert_eq!(a.artist_view, 1);
    // The page is the one that row named.
    let page = a.artist_page().expect("page resolves");
    assert_eq!(page.name, a.lib.artists[1].name);
    // Back returns to the list, still on Artists.
    assert_eq!(a.press(Button::Back), vec![]);
    assert_eq!(a.current(), Screen::Library);
    assert_eq!(a.lib_tab, Tab::Artists);
}

#[test]
fn the_row_shuffle_button_shuffles_that_artist_without_opening_the_page() {
    let mut a = library_on(Tab::Artists);
    // The button is drawn at x 414..454; anywhere in that block is it.
    assert_eq!(a.tap(434, artist_row_y(1)), vec![Action::ShuffleArtist(1)]);
    assert_eq!(
        a.current(),
        Screen::Library,
        "the button must not also navigate"
    );
    // And the band on the page itself shuffles the artist whose page it is.
    a.tap(200, artist_row_y(1));
    let (bx, by, _, bh) = library::shuffle_band_rect(library::ARTIST_BAND_Y);
    assert_eq!(a.tap(bx + 20, by + bh / 2), vec![Action::ShuffleArtist(1)]);
}

/// The y of the open artist page's "SONGS · n" header, through the page layout.
fn artist_songs_header_y(a: &App) -> i32 {
    let p = a.artist_page().expect("page");
    let vy = p
        .rows
        .iter()
        .find_map(|(vy, r)| matches!(*r, library::ArtistRowKind::SongsSection).then_some(*vy))
        .expect("the artist has tracks");
    library::artist_content_top() + vy + library::ARTIST_SONGS_H / 2 - a.artist_scroll_px
}

fn open_artist_songs(a: &mut App) {
    let y = artist_songs_header_y(a);
    assert!(a.tap(400, y).is_empty(), "the header plays nothing");
    assert!(a.artist_page().expect("page").songs_open);
}

/// 2026-10-04 (owner's call): the full SONGS list is folded away when an artist page opens,
/// its header shows and hides it, and a page is never left scrolled past its own end.
#[test]
fn an_artist_pages_songs_are_folded_until_the_header_is_tapped() {
    let mut a = library_on(Tab::Artists);
    a.tap(200, artist_row_y(0));
    let song_rows = |a: &App| {
        let p = a.artist_page().expect("page");
        p.rows
            .iter()
            .filter(|(_, r)| matches!(r, library::ArtistRowKind::Song(_)))
            .count()
    };
    let total = a.artist_page().expect("page").tracks.len();
    assert!(total > 0);
    assert_eq!(song_rows(&a), 0, "closed when the page opens");
    // Shuffle and the stats line still count every track.
    assert_eq!(a.artist_list_ids().len(), total);
    open_artist_songs(&mut a);
    assert_eq!(song_rows(&a), total);
    // Scroll to the end, fold the list: the offset comes back inside the shorter page.
    a.artist_scroll_px = a
        .artist_page()
        .map(|p| library::artist_max_scroll_px(&p))
        .unwrap();
    let y = artist_songs_header_y(&a);
    if (library::artist_content_top()..library::LIST_BOTTOM).contains(&y) {
        a.tap(400, y);
    } else {
        a.artist_scroll_px = 0;
        a.tap(400, artist_songs_header_y(&a));
    }
    assert_eq!(song_rows(&a), 0);
    let max = a
        .artist_page()
        .map(|p| library::artist_max_scroll_px(&p))
        .unwrap();
    assert!(a.artist_scroll_px <= max);
    // Leaving and coming back starts folded again.
    open_artist_songs(&mut a);
    a.press(Button::Back);
    a.tap(200, artist_row_y(0));
    assert_eq!(song_rows(&a), 0);
}

#[test]
fn an_artist_page_track_row_plays_and_swipes_to_the_queue() {
    let mut a = library_on(Tab::Artists);
    a.tap(200, artist_row_y(0));
    assert_eq!(a.current(), Screen::Artist);
    open_artist_songs(&mut a);
    // The first track row, located through the page layout rather than a literal.
    let (first_track_y, want) = {
        let p = a.artist_page().expect("page");
        let vy = p
            .rows
            .iter()
            .find_map(|(vy, r)| matches!(*r, library::ArtistRowKind::Song(0)).then_some(*vy))
            .expect("the artist has tracks");
        (
            library::artist_content_top() + vy + 4,
            p.tracks[0].song.object_id,
        )
    };
    // The ARTIST's list plays, from the tapped track — not that track's album.
    assert_eq!(a.tap(200, first_track_y), vec![Action::PlayListAt(0)]);
    let list = a.take_play_list();
    assert_eq!(list.first(), Some(&want));
    assert_eq!(list, a.artist_list_ids());
    // Right-swiping the same row queues it instead (empty queue → no prompt involved).
    let mut a = with_playing(library_on(Tab::Artists), 1);
    a.tap(200, artist_row_y(0));
    open_artist_songs(&mut a);
    assert!(upq(&a).is_empty());
    a.swipe(1, 200, first_track_y);
    assert_eq!(
        upq(&a).len(),
        1,
        "a swipe on an artist-page track must queue it"
    );
}

#[test]
fn only_rows_with_a_track_under_the_finger_move_with_the_swipe() {
    // A song row takes the gesture …
    let mut a = library_on(Tab::Songs);
    let song_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) + 20;
    assert!(a.swipe_track(40, song_y));
    assert_eq!(a.swipe_state().map(|s| s.dx), Some(40));
    // … an ARTIST row does not: there is nothing to queue, so a row that slid would be
    // promising an action release cannot perform.
    let mut a = library_on(Tab::Artists);
    assert!(!a.swipe_track(40, artist_row_y(1)));
    assert_eq!(a.swipe_state(), None);
    // Neither does the shuffle band above the list.
    let mut a = library_on(Tab::Songs);
    let (_, by, _, bh) = library::library_shuffle_band();
    assert!(!a.swipe_track(40, by + bh / 2));
    assert_eq!(a.swipe_state(), None);
}

#[test]
fn a_released_row_animates_back_to_rest_and_stops() {
    let mut a = library_on(Tab::Songs);
    let song_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) + 20;
    a.swipe_track(200, song_y);
    let held = a.swipe_state().expect("row is travelling").dx;
    assert!(held > 0);
    // While the finger is down the row holds its offset, however many frames pass.
    for _ in 0..60 {
        a.tick();
    }
    assert_eq!(
        a.swipe_state().map(|s| s.dx),
        Some(held),
        "a held row must not drift home"
    );
    // After release it decays to rest — and then STOPS asking for frames, or the device would
    // repaint forever for an animation that has finished.
    a.swipe_release();
    let mut frames = 0;
    while a.swipe_state().is_some() {
        assert!(
            a.tick(),
            "an animating row must report that it needs a repaint"
        );
        frames += 1;
        assert!(frames < 240, "the snap-back never settled");
    }
    assert!(frames > 1, "the row must animate home, not teleport");
}

// ── Shelf: the 2026-08-05 audit fixes ───────────────────────────────────────────────────

#[test]
fn shelf_go_leaves_a_working_back_button() {
    // Regression: `Go` called go(), which REPLACED the route stack with a single screen, so
    // Back (and the left-edge swipe) did nothing and the user was stranded on the pin.
    let mut a = unlocked();
    a.go(Screen::Library);
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby); // header Pin → slot 0
    a.tap(240, 200); // backdrop closes
    a.go(Screen::NowPlaying);
    a.open_shelf();
    a.tap(200, crate::shelf::slot_center_y(0)); // slot 0 row body → GO
    assert_eq!(a.current(), Screen::Library);
    assert!(!a.shelf_is_open());
    assert_eq!(a.press(Button::Back), vec![]);
    assert_eq!(
        a.current(),
        Screen::NowPlaying,
        "Back must work after a pin jump"
    );
}

#[test]
fn shelf_empty_slot_pins_and_filled_slot_goes() {
    // Regression: tapping a bookmark's row BODY used to pin the current place into a
    // different slot instead of jumping to the bookmark; only a ~60px "GO" column worked.
    // And the same column on an EMPTY slot returned Go(i) — which did nothing but still
    // dismissed the sheet.
    let mut a = unlocked();
    a.go(Screen::Library);
    a.open_shelf();
    a.tap(200, crate::shelf::slot_center_y(2)); // body of empty slot 2
    assert!(a.pins[2].is_some());
    assert!(
        a.pins[0].is_none(),
        "it must pin where the finger was, not slot 0"
    );
    assert!(a.toast.starts_with("Pinned to slot 3"), "{}", a.toast);
    assert!(a.shelf_is_open(), "pinning must not dismiss the sheet");
    a.tap(440, crate::shelf::slot_center_y(2)); // the × column forgets it
    assert!(a.pins[2].is_none());
    assert!(a.toast.starts_with("Slot 3 cleared"), "{}", a.toast);
}

#[test]
fn shelf_restores_the_whole_place_not_just_the_screen() {
    let mut a = unlocked();
    let mut lib = Library::sample();
    lib.songs = (0..120)
        .map(|i| SongRow {
            title: format!("Track {i:03}"),
            object_id: i,
            ..Default::default()
        })
        .collect();
    a.set_library(lib);
    a.go(Screen::Library);
    a.lib_tab = Tab::Songs;
    a.lib_sort = 2;
    a.scroll_px(140);
    let scroll = a.lib_scroll_px;
    assert!(scroll > 0);
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby);
    a.tap(240, 200);
    // Wander off: different tab, different sort, scrolled back to the top.
    a.lib_tab = Tab::Albums;
    a.lib_sort = 0;
    a.lib_scroll_px = 0;
    a.go(Screen::NowPlaying);
    a.open_shelf();
    a.tap(200, crate::shelf::slot_center_y(0)); // GO
    assert_eq!(a.current(), Screen::Library);
    assert_eq!(a.lib_tab, Tab::Songs);
    assert_eq!(a.lib_sort, 2);
    assert_eq!(
        a.lib_scroll_px, scroll,
        "the list position is part of 'the place'"
    );
}

/// The reported failure, 2026-08-19: a pin saved on "anymore (Deluxe)" opened "burn" instead.
/// A pin stored the album's ROW NUMBER, and the library is rebuilt from the DB at every boot —
/// so anything that shifts the list (a track added, a rescan) silently re-aims every pin.
/// Opening an album must not move the row you pressed. Only one is open at a time, so opening
/// B closes A — and when A is above B, closing it used to drag B upward off the screen.
/// The mini-player's play/pause used to be a local guess that nothing ever corrected: five
/// places flipped it, and the player's real state was never read back. A track ending, or a
/// pause the app did not initiate, left the bar showing the opposite of the truth.
#[test]
fn library_play_pause_follows_the_player_after_its_optimistic_hold() {
    let _scale = crate::text::scale_guard();
    let mut a = unlocked();
    a.go_for_preview(Screen::Library);

    // The player says PLAYING and the UI agrees.
    let mut np = NowPlaying {
        skin: None,
        tape: None,
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        codec: "FLAC",
        badge: "FLAC 24/96",
        clock: "14:32",
        battery: 78,
        elapsed: "1:47",
        remaining: "-2:45",
        progress: 0.39,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: true,
        shuffle: false,
        repeat: 0,
        viz_seed: 2.0,
        viz_kind: 0,
        viz_size: 1,
        page: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        scrubbing: false,
        lyrics: false,
    };
    let mut c = Canvas::new();
    let fonts = FontSet::load();
    a.render(&mut c, &fonts, &np);
    assert!(a.playing);

    // Tap the bar's play/pause: the button answers IMMEDIATELY, before any round trip, and
    // holds that answer even though the player still reports the old state.
    let (_, by, _, bh) = crate::chrome::np_bar_rect();
    assert_eq!(a.tap(30, by + bh / 2), vec![Action::PlayPause]);
    assert!(!a.playing, "the button did not respond under the finger");
    a.render(&mut c, &fonts, &np);
    assert!(
        !a.playing,
        "the guess was overwritten before the player could act on it"
    );

    // Once the hold runs out, the player is the authority again.
    for _ in 0..20 {
        a.tick_dt(100);
    }
    a.render(&mut c, &fonts, &np);
    assert!(a.playing, "the bar never adopted the player's actual state");

    // And a change the UI never initiated — a track ending — is picked up on its own.
    np.playing = false;
    a.render(&mut c, &fonts, &np);
    assert!(
        !a.playing,
        "playback stopped and the bar still said playing"
    );
}

#[test]
fn expanding_an_album_leaves_the_pressed_row_where_it_is() {
    let mut a = unlocked();
    a.go_for_preview(Screen::Library);
    a.lib_tab = Tab::Albums;
    let order = crate::library::album_display_order(&a.lib, a.album_sort);
    assert!(order.len() >= 3, "need a few albums");
    let (first, later) = (order[0], order[2]);

    // Open the FIRST album, so there is something above to collapse.
    a.album_expanded = Some(first);
    a.clamp_lib_scroll();

    // Where the third album sits on screen right now.
    let rank = a.album_rank_of(later).unwrap();
    // Screen y of the row, matching `albums_hit`'s own mapping (content y - scroll + list top).
    let screen_y = |app: &App| {
        crate::library::row_top_px(
            Tab::Albums,
            &app.lib,
            rank,
            app.album_sort,
            app.album_expanded,
        ) - app.lib_scroll_px
            + crate::library::list_top(Tab::Albums)
    };
    // Scroll well down, so closing the album above has scroll to give back. At the very top
    // of the list it cannot: there is no content left above the row to remove, and then the row
    // must come up with it — a boundary, not a bug.
    a.lib_scroll_px = a.lib_max_scroll();

    let before = screen_y(&a);
    assert!(
        (crate::library::list_top(Tab::Albums)..crate::library::list_bottom() - 40)
            .contains(&before),
        "the third album is not on screen to begin with (y={before})"
    );

    // Tap its BODY (x clear of the art, which drills in instead of toggling).
    let _ = a.tap_albums(240, before + 8);
    assert_eq!(
        a.album_expanded,
        Some(later),
        "the tapped album did not open"
    );
    let after = screen_y(&a);
    assert_eq!(
        after, before,
        "the pressed row moved on screen ({before} -> {after}); the rest of the library \
             should move around it instead"
    );
    assert!(
        (crate::library::list_top(Tab::Albums)..crate::library::list_bottom()).contains(&after),
        "the pressed row is not even on screen ({after})"
    );
}

#[test]
fn a_pin_follows_the_album_not_its_row_number() {
    let mut a = unlocked();
    // The fixture leaves album_id at 0, which is deliberately not trusted. Give them real ids.
    let mut lib = a.lib.clone();
    assert!(lib.albums_flat().len() >= 3, "need a few albums to shift");
    let mut n = 0i64;
    for g in lib.album_groups.iter_mut() {
        for al in g.albums.iter_mut() {
            al.album_id = 100 + n;
            n += 1;
        }
    }
    a.set_library(lib.clone());
    a.lib_tab = Tab::Albums;

    // Pin the album sitting at row 2.
    let target = a.lib.albums_flat()[2].album_id;
    let target_name = a.lib.albums_flat()[2].name.clone();
    a.album_view = 2;
    a.album_expanded = Some(2);
    let pin = a.capture_pin();

    // Now the library gains an album ahead of it, exactly as a rescan would. Every row after
    // the insertion shifts by one, and the OLD code would have restored the wrong album.
    let mut grown = lib.clone();
    let mut newcomer = grown.album_groups[0].albums[0].clone();
    newcomer.album_id = 999;
    newcomer.name = "Brand New".into();
    grown.album_groups[0].albums.insert(0, newcomer);
    a.set_library(grown);

    a.restore_pin(&pin);
    assert_eq!(
        a.lib.albums_flat()[a.album_view].album_id,
        target,
        "the pin landed on '{}' instead of '{}'",
        a.lib.albums_flat()[a.album_view].name,
        target_name
    );
    assert_eq!(
        a.album_expanded.map(|e| a.lib.albums_flat()[e].album_id),
        Some(target)
    );
    assert_ne!(
        a.album_view, 2,
        "the row number should have moved; the album should not"
    );
}

/// The identities have to survive the config file too, and an OLD record (12 fields, no
/// identities) must still decode rather than clearing the slot.
#[test]
fn pin_identities_round_trip_and_old_records_still_load() {
    let mut a = unlocked();
    let mut lib = a.lib.clone();
    let mut n = 0i64;
    for g in lib.album_groups.iter_mut() {
        for al in g.albums.iter_mut() {
            al.album_id = 100 + n;
            n += 1;
        }
    }
    a.set_library(lib);
    a.lib_tab = Tab::Albums;
    a.album_view = 1;
    a.pins[0] = Some(a.capture_pin());
    let enc = a.shelf_pin_encode(0);
    assert!(
        enc.contains("|101|"),
        "the album id is not in the record: {enc}"
    );

    let mut b = unlocked();
    b.shelf_pin_decode(0, &enc);
    assert!(b.pins[0].is_some());

    // A pre-identity record: the first 12 fields only.
    let old: String = enc.split('|').take(12).collect::<Vec<_>>().join("|");
    b.shelf_pin_decode(1, &old);
    assert!(b.pins[1].is_some(), "an older pin record was thrown away");
}

#[test]
fn shelf_pins_survive_a_reboot() {
    // Regression: pins were session-scoped, so every reboot silently wiped the bookmarks.
    let mut a = unlocked();
    a.go(Screen::Library);
    a.lib_tab = Tab::Artists;
    a.lib_sort = 1;
    a.lib_scroll_px = 96;
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby);
    let encoded = a.shelf_pin_encode(0);
    assert!(!encoded.is_empty());
    let mut b = unlocked();
    b.shelf_pin_decode(0, &encoded);
    assert_eq!(b.pins[0], a.pins[0]);
    // Garbage in a hand-edited config clears the slot rather than panicking or half-loading.
    b.shelf_pin_decode(0, "not|a|pin");
    assert!(b.pins[0].is_none());
    b.shelf_pin_decode(1, "");
    assert!(b.pins[1].is_none());
}

#[test]
fn shelf_swallows_drags_meant_for_the_sheet() {
    // Regression: a drag on the modal sheet scrolled (and flung) the list behind it, so the
    // Shelf appeared frozen while the screen moved underneath.
    let mut a = unlocked();
    a.go(Screen::Library);
    a.scroll_px(200);
    let before = a.lib_scroll_px;
    a.open_shelf();
    a.scroll_px(300);
    a.fling(2000.0);
    a.tick();
    assert_eq!(a.lib_scroll_px, before, "the sheet must own the gesture");
}

#[test]
fn the_audiobooks_screen_can_be_pinned_and_a_pin_goes_back_to_it() {
    let mut a = unlocked();
    a.set_library(Library {
        books: (0..3)
            .map(|i| crate::model::BookRow {
                title: format!("Book {i}"),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    a.go(Screen::Library);
    a.open_books();
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby);
    let pin = a.pins[0]
        .as_ref()
        .expect("the Audiobooks screen is a place");
    assert_eq!(pin.screen, Screen::Books);
    assert_eq!(
        (pin.title.as_str(), pin.sub.as_str()),
        ("Audiobooks", "3 BOOKS")
    );
    // It survives a reboot, and from anywhere the pin goes back to it.
    let mut b = unlocked();
    b.shelf_pin_decode(0, &a.shelf_pin_encode(0));
    assert_eq!(b.pins[0], a.pins[0]);
    a.go(Screen::Settings);
    a.open_shelf();
    a.tap(100, crate::shelf::slot_center_y(0));
    assert_eq!(a.current(), Screen::Books);
}

#[test]
fn internet_radio_can_be_pinned_and_its_pin_reloads_the_stations() {
    let mut a = unlocked();
    a.push_for_test(Screen::NetRadio);
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby);
    let pin = a.pins[0].as_ref().expect("the radio is a place");
    assert_eq!(pin.title, "Internet radio");
    let mut b = unlocked();
    b.shelf_pin_decode(0, &a.shelf_pin_encode(0));
    assert_eq!(b.pins[0], a.pins[0], "it survives a reboot");
    b.go(Screen::Library);
    b.open_shelf();
    assert_eq!(
        b.tap(100, crate::shelf::slot_center_y(0)),
        vec![Action::RadioOpen]
    );
    assert_eq!(b.current(), Screen::NetRadio);
}

#[test]
fn a_modal_screen_is_not_a_place_you_can_pin() {
    let mut a = unlocked();
    a.go(Screen::UsbStorage);
    a.open_shelf();
    let (pbx, pby) = crate::shelf::pin_button_center();
    a.tap(pbx, pby);
    assert!(a.pins[0].is_none());
    assert_eq!(a.toast, "Nothing to pin here");
}

// ── UI scale ────────────────────────────────────────────────────────────────────────────

/// Serialise every scale-sensitive test against the crate-wide lock in `text` — see
/// `text::scale_guard` for why one shared lock and not a per-module one.
fn lock_scale() -> crate::text::ScaleGuard {
    crate::text::scale_guard()
}

/// The EQ band field is DRAGGABLE, and the knob has to end up where the finger is. Before
/// 2026-08-17 every slider on this device was tap-only — one tap, one step, ten taps to cross
/// a band — which is what the user reported as "they can't be dragged".
#[test]
fn eq_band_drag_follows_the_finger() {
    let mut a = App::unlocked();
    a.stack = vec![Screen::Eq];
    let x = crate::eq::band_center_x(3);
    // Grab: the value is set at finger-DOWN, not only on the first move.
    assert!(
        a.scrub_begin(x, crate::eq::FIELD_MID),
        "the band field did not take the grab"
    );
    assert_eq!(
        a.eq_bands[3], 0,
        "grabbing the zero line should read as 0 dB"
    );
    // Drag to the top of the field → full boost.
    let acts = a.scrub_move(x, crate::eq::FIELD_TOP);
    assert_eq!(
        a.eq_bands[3],
        crate::eq::BAND_MAX,
        "the knob did not follow the finger up"
    );
    assert!(
        acts.iter().any(|q| matches!(q, Action::EqChanged(_))),
        "no live apply"
    );
    // A move that lands on the same step emits NOTHING. Effect writes cost ~10 ms each
    // (cinder-probe --fxtime), so a drag that queued one per motion event stuttered the audio.
    assert!(
        a.scrub_move(x, crate::eq::FIELD_TOP - 5).is_empty(),
        "a no-op move still emitted"
    );
    // …and the bottom of the field is full cut.
    a.scrub_move(x, crate::eq::FIELD_BOTTOM);
    assert_eq!(
        a.eq_bands[3],
        -crate::eq::BAND_MAX,
        "the knob did not follow the finger down"
    );
    // The release re-emits, which is what makes the shell's 60 ms throttle safe to drop
    // intermediate writes: the FINAL value always gets through.
    assert!(
        a.scrub_end()
            .iter()
            .any(|q| matches!(q, Action::EqChanged(_))),
        "the release did not re-emit the final curve"
    );
}

/// The band is captured at finger-down and never re-picked. A diagonal sweep must not rewrite
/// the bands it passes over — there is no undo on this screen.
#[test]
fn an_eq_drag_never_changes_band_mid_sweep() {
    let mut a = App::unlocked();
    a.stack = vec![Screen::Eq];
    // Snapshot first: the default curve is a preset, not flat, so "unchanged" is not "zero".
    let before = a.eq_bands;
    assert!(a.scrub_begin(crate::eq::band_center_x(0), crate::eq::FIELD_MID));
    // Finger wanders all the way across the field while moving up.
    a.scrub_move(crate::eq::band_center_x(9), crate::eq::FIELD_TOP);
    assert_eq!(
        a.eq_bands[0],
        crate::eq::BAND_MAX,
        "the grabbed band did not move"
    );
    assert_eq!(
        a.eq_bands[1..10],
        before[1..10],
        "a band was rewritten by a sweep that only passed over it"
    );
}

/// Hand-editing a band drops the preset pill: it would otherwise keep claiming a name for a
/// curve that is no longer that preset.
#[test]
fn dragging_a_band_clears_the_preset_name() {
    let mut a = App::unlocked();
    a.stack = vec![Screen::Eq];
    a.eq_preset = 2;
    let x = crate::eq::band_center_x(5);
    assert!(a.scrub_begin(x, crate::eq::FIELD_MID));
    a.scrub_move(x, crate::eq::FIELD_TOP);
    assert_eq!(
        a.eq_preset, 0,
        "the preset pill still names a curve that was edited by hand"
    );
}

/// The Tone Control field drags the same way, over its own geometry.
#[test]
fn tone_band_drag_follows_the_finger() {
    let mut a = App::unlocked();
    a.stack = vec![Screen::Tone];
    let x = crate::tone::band_center_x(1);
    assert!(
        a.scrub_begin(x, crate::tone::FIELD_MID),
        "the tone field did not take the grab"
    );
    let acts = a.scrub_move(x, crate::tone::FIELD_TOP);
    assert_eq!(a.tone_bands()[1], crate::tone::BAND_MAX);
    assert!(
        acts.iter().any(|q| matches!(q, Action::SoundChanged)),
        "no live apply"
    );
    assert!(
        a.scrub_move(x, crate::tone::FIELD_TOP).is_empty(),
        "a no-op move still emitted"
    );
    a.scrub_move(x, crate::tone::FIELD_BOTTOM);
    assert_eq!(a.tone_bands()[1], -crate::tone::BAND_MAX);
    assert!(a
        .scrub_end()
        .iter()
        .any(|q| matches!(q, Action::SoundChanged)));
}

/// A drag started OFF the field must not be claimed as a band scrub — otherwise a tap on the
/// preset pills or the footer would grab the whole gesture and the screen would feel stuck.
#[test]
fn only_the_band_field_claims_a_scrub() {
    let mut a = App::unlocked();
    a.stack = vec![Screen::Eq];
    assert!(
        !a.scrub_begin(crate::eq::band_center_x(0), crate::eq::FIELD_TOP - 1),
        "a point above the field was claimed"
    );
    assert!(
        !a.scrub_begin(crate::eq::band_center_x(0), crate::eq::FIELD_BOTTOM),
        "a point below the field was claimed"
    );
    assert!(
        !a.scrub_begin(5, crate::eq::FIELD_MID),
        "the left gutter was claimed"
    );
}

#[test]
fn ui_scale_slider_scrubs_taps_and_steps() {
    let _scale = lock_scale();
    let mut a = unlocked();
    a.go(Screen::Display);
    let row_y = crate::display::row_top(crate::display::ROW_SIZE) + 10;
    // A tap on the track jumps straight to that stop (SeekBar idiom, not tap-to-cycle).
    assert_eq!(a.tap(460, row_y), vec![Action::UiScaleChanged]);
    assert_eq!(
        crate::text::scale_pct(),
        *crate::text::SCALE_STEPS.last().unwrap()
    );
    // Dragging it scrubs live.
    assert!(a.scrub_begin(100, row_y));
    assert!(a.scrub_is_ui_scale());
    a.scrub_move(100, row_y);
    assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[0]);
    assert_eq!(a.scrub_end(), vec![Action::UiScaleChanged]);
    // Buttons step one stop and clamp at both ends.
    a.display_sel = crate::display::ROW_SIZE;
    a.press(Button::Right);
    assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[1]);
    for _ in 0..20 {
        a.press(Button::Left);
    }
    assert_eq!(crate::text::scale_pct(), crate::text::SCALE_STEPS[0]);
    for _ in 0..20 {
        a.press(Button::Right);
    }
    assert_eq!(
        crate::text::scale_pct(),
        *crate::text::SCALE_STEPS.last().unwrap()
    );
}

/// The 2026-09-30 report: Back and Sort were small, and aiming at Sort opened the Menu or the
/// Shelf. Back's target grows with the UI scale; the Sort chip owns the strip's bottom edge on
/// the two tabs that draw it, and nowhere else.
#[test]
fn scale_grows_back_and_sort_owns_the_strip_edge_above_it() {
    let _scale = lock_scale();
    use crate::chrome::{back_hit, status_hit, StatusTap};
    assert!(
        back_hit(79, 60) && !back_hit(80, 60),
        "100% keeps the old x < 80"
    );
    crate::text::set_scale_pct(140);
    assert!(back_hit(105, 60), "140% widens the back target");
    crate::text::set_scale_pct(80);
    assert!(
        back_hit(79, 60) && !back_hit(80, 60),
        "below 100% never shrinks it"
    );
    crate::text::set_scale_pct(100);

    let mut a = unlocked();
    a.go(Screen::Library);
    a.lib_tab = Tab::Songs;
    let y = library::SORT_HIT_TOP + 2;
    assert_eq!(
        status_hit(344, y),
        Some(StatusTap::Menu),
        "the strip alone would take it"
    );
    let sort0 = a.lib_sort;
    a.tap(344, y);
    assert_eq!(a.current(), Screen::Library, "no Menu");
    assert_ne!(a.lib_sort, sort0, "the tap cycled SORT");
    a.tap(390, y);
    assert_eq!(a.current(), Screen::Library, "no Shelf");
    // A count caption is not a control: the strip keeps its full height there.
    a.lib_tab = Tab::Artists;
    a.tap(344, y);
    assert_eq!(a.current(), Screen::Menu);
    // The view button sits under the strip too, on every tab.
    a.go(Screen::Library);
    let view0 = library::view_of(&a.lib, a.lib_tab);
    let (vx0, vx1) = library::view_button_x(a.search_enabled);
    a.tap((vx0 + vx1) / 2, y);
    assert_eq!(
        a.current(),
        Screen::Library,
        "the view button, not the Menu"
    );
    assert_ne!(
        library::view_of(&a.lib, a.lib_tab),
        view0,
        "the tap cycled the view"
    );
}

/// The tab strip draws 35 px; with the band in place its target runs down to the band.
#[test]
fn library_tabs_take_the_gap_above_the_band() {
    let _scale = lock_scale();
    let mut a = unlocked();
    a.go(Screen::Library);
    a.lib_tab = Tab::Songs;
    let bottom = library::tab_hit_bottom(0);
    assert!(bottom > library::TAB_BOT && bottom < library::library_shuffle_band().1);
    let zones = library::tab_layout(&FontSet::load(), false);
    let x = zones
        .iter()
        .find(|z| z.0 == Some(Tab::Albums))
        .map(|z| (z.1 + z.2 / 2.0) as i32)
        .unwrap();
    *a.lib_tab_zones.borrow_mut() = zones;
    a.tap(x, bottom - 1);
    assert_eq!(a.lib_tab, Tab::Albums);
    assert_eq!(
        library::tab_hit_bottom(10),
        library::TAB_BOT,
        "slid band: the rows own the gap"
    );
}

#[test]
fn text_scale_keeps_measure_and_draw_in_step() {
    let _scale = lock_scale();
    // The safety property behind the slider: `fit`/`center`/`right` all resolve through
    // measure(), so a scale that measure() ignored would silently break every truncation.
    let fonts = FontSet::load();
    let st = crate::widgets::sty(
        crate::text::Family::Sans,
        crate::text::Weight::Bold,
        20.0,
        Theme::day().ink,
        0.0,
    );
    let base = crate::text::measure(&fonts, "Atlas Hands", &st);
    crate::text::set_scale_pct(140);
    let big = crate::text::measure(&fonts, "Atlas Hands", &st);
    assert!(
        big > base * 1.3,
        "measure() must follow the scale ({base} -> {big})"
    );
    let mut c = Canvas::new();
    let pen = crate::text::draw(&mut c, &fonts, 0.0, 40.0, "Atlas Hands", &st);
    assert!(
        (pen - big).abs() < 0.01,
        "draw() pen {pen} != measure() {big}"
    );
}

/// #51 (plaquette): Settings ▸ Back arrow puts a back arrow top-left on Now Playing that does
/// what the left-edge swipe does, and the Lyrics chip moves right of it. Off by default, and
/// with it off the corner is what it always was.
#[test]
fn the_now_playing_back_arrow_does_what_back_does() {
    let fresh = || {
        let mut a = unlocked();
        a.push_for_test(Screen::Library);
        a.push_for_test(Screen::NowPlaying);
        a
    };
    let (x, y) = (
        crate::now_playing::LYRICS_X0 + 10,
        crate::now_playing::LYRICS_Y0 + 10,
    );
    let mut a = fresh();
    assert!(
        !a.opt(Opt::NpBack) && !a.opt(Opt::NpCount) && !a.opt(Opt::HoldButtons),
        "all off by default"
    );
    a.set_opt(Opt::NpBack, false);
    assert!(!crate::now_playing::hit_back(x, y), "no arrow, no target");

    let mut want = fresh();
    want.press(Button::Back);
    a.set_opt(Opt::NpBack, true);
    a.tap(x, y);
    assert_ne!(
        a.current(),
        Screen::NowPlaying,
        "the arrow left Now Playing"
    );
    assert_eq!(a.current(), want.current(), "the arrow is Back");

    // The Lyrics chip sits right of the arrow while it is shown, and where it was without.
    let lx = crate::now_playing::LYRICS_X0 + 20;
    let ly = crate::now_playing::LYRICS_Y0 + 10;
    assert!(!crate::now_playing::hit_lyrics(lx, ly, true));
    assert!(crate::now_playing::hit_lyrics(
        lx + crate::now_playing::BACK_SHIFT,
        ly,
        true
    ));
    a.set_opt(Opt::NpBack, false);
    assert!(crate::now_playing::hit_lyrics(lx, ly, true));

    // With nothing to go back to (Now Playing at boot), Back opens the Library.
    let mut a = unlocked();
    a.press(Button::Back);
    assert_eq!(a.current(), Screen::Library);
    // Hold on and off lands on Now Playing; Back from there never returns to the Lock screen.
    let mut a = fresh();
    a.set_hold(true);
    a.set_hold(false);
    assert_eq!(a.current(), Screen::NowPlaying);
    a.press(Button::Back);
    assert!(!matches!(a.current(), Screen::Lock | Screen::NowPlaying));
}

#[test]
fn library_tab_taps_land_on_the_labels_that_are_drawn() {
    let _scale = lock_scale();
    // Regression: the strip was LAID OUT from measured label widths but HIT-TESTED against
    // hardcoded thresholds (x<120/220/330). At the default size "ALBUMS" is drawn at
    // x≈94..154, so tapping its left half selected SONGS. Checked at three UI scales, since
    // the labels move with the scale and fixed thresholds could not have followed.
    let fonts = FontSet::load();
    for (pct, folders) in [80u32, 100, 140]
        .iter()
        .flat_map(|&p| [(p, false), (p, true)])
    {
        crate::text::set_scale_pct(pct);
        let zones = library::tab_layout(&fonts, folders);
        assert_eq!(zones.len(), if folders { 5 } else { 4 });
        // The strip fits between the 22 px margins, FOLDERS and all.
        let (_, lx, lw) = zones[zones.len() - 1];
        assert!(
            lx + lw <= crate::canvas::W as f32 - 21.0,
            "scale {pct}%, folders {folders}: strip runs to {}",
            lx + lw
        );
        for (tab, x, w) in &zones {
            let mid = (x + w / 2.0) as i32;
            assert_eq!(
                tab_zone_at(&zones, mid),
                Some(*tab),
                "scale {pct}%: tap at x={mid} picked the wrong tab"
            );
        }
        // Every pixel of the strip belongs to some tab — no dead gaps between labels.
        for x in 0..crate::canvas::W as i32 {
            assert!(
                tab_zone_at(&zones, x).is_some(),
                "scale {pct}%: dead strip at x={x}"
            );
        }
    }
}

#[test]
fn the_folders_link_is_off_until_switched_on_and_opens_the_folder_browser() {
    let _scale = lock_scale();
    let mut a = unlocked();
    assert!(!a.opt(Opt::FoldersTab), "off by default");
    a.go(Screen::Library);
    let fonts = FontSet::load();
    a.render(&mut Canvas::new(), &fonts, &bare_np());
    assert_eq!(
        a.lib_tab_zones.borrow().len(),
        4,
        "off: the four tabs, nothing else"
    );

    a.set_opt(Opt::FoldersTab, true);
    a.render(&mut Canvas::new(), &fonts, &bare_np());
    let (slot, x, w) = *a.lib_tab_zones.borrow().last().unwrap();
    assert_eq!(slot, None, "FOLDERS is the last slot");
    let tab = a.lib_tab;
    a.tap((x + w / 2.0) as i32, library::TAB_TOP + 10);
    assert_eq!(a.current(), Screen::Folders);
    assert_eq!(a.lib_tab, tab, "the list behind it keeps its tab");
    a.folder_stack.clear(); // at the top of the tree, Back leaves it
    a.pop();
    assert_eq!(a.current(), Screen::Library);

    // A reloaded library keeps the switch.
    a.set_library(Library::sample());
    assert!(a.lib.folders_tab);
}

// ── Brightness: level 0 is transient, never a trap ──────────────────────────────────────

#[test]
fn backlight_off_is_unreachable_from_the_row_but_still_escapable() {
    let mut a = unlocked();
    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_BRIGHTNESS;
    // THE ROW NO LONGER OFFERS 0. The backlight-off stop is parked (see `brightness`): it
    // blanked the panel where the point was an unlit but still readable screen. Cycling must
    // never land on it, however many presses.
    for _ in 0..12 {
        a.press(Button::Select);
        assert_ne!(
            a.brightness(),
            0,
            "the parked backlight-off stop is reachable again"
        );
    }

    // The ESCAPE machinery stays, because the feature is parked and not deleted — if the panel
    // turns out to support a true unlit-but-readable mode, this is what makes it safe.
    a.set_brightness(5);
    a.brightness = 0; // as the shell's transient level-0 state would be
    assert_eq!(a.brightness_restore(), 5); // what gets PERSISTED is never 0
    assert!(a.brightness_wake());
    assert_eq!(a.brightness(), 5); // back to the last VISIBLE level
    assert!(
        !a.brightness_wake(),
        "already awake → nothing to do, no needless backlight write"
    );
}

#[test]
fn a_corrupt_config_can_never_restore_a_black_panel() {
    let mut a = unlocked();
    a.set_brightness(0);
    assert_eq!(
        a.brightness(),
        1,
        "0 in the config file means 'visible', not 'dark'"
    );
    a.set_brightness(99);
    assert_eq!(a.brightness(), 5);
}

// ── Bluetooth: we say when we don't know ────────────────────────────────────────────────

#[test]
fn bluetooth_link_is_unknown_until_the_shell_reports() {
    let mut a = unlocked();
    assert!(a.bt_on);
    assert!(
        !a.bt_link_known(),
        "before the first poll, 'no device' would be a guess"
    );
    assert_eq!(a.bt_connected(), None);
    assert!(a.set_bt_connected(Some("WH-1000XM4")));
    assert_eq!(a.bt_connected(), Some("WH-1000XM4"));
    assert!(a.bt_link_known());
    assert!(
        !a.set_bt_connected(Some("WH-1000XM4")),
        "no change → no repaint"
    );
    // A reported disconnect is an observation, not a fallback to ignorance.
    assert!(a.set_bt_connected(None));
    assert_eq!(a.bt_connected(), None);
    assert!(a.bt_link_known());
}

/// 2026-09-15: a connect's spinner died within a frame, because the paired-list refresh every
/// connect triggers cleared it, and the headphones' row was tapped 24 times in 21 s. The spinner
/// must outlive a refresh of the same list, the same row must not ask again while it spins, and
/// the spinner must stop when the device links.
#[test]
fn a_paired_row_connect_spins_through_a_refresh_and_asks_once() {
    let mut a = unlocked();
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", false);
    a.bt_paired_add("WONDERBOOM", "Speaker", false);
    assert_eq!(a.bt_connect_row(1), vec![Action::BtConnectDevice(1)]);
    // The shell answers every connect with a list refresh.
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", false);
    a.bt_paired_add("WONDERBOOM", "Speaker", false);
    assert_eq!(a.bt_connecting, Some(1), "the spinner survived the refresh");
    assert!(
        a.bt_connect_row(1).is_empty(),
        "a second tap while it spins asks for nothing"
    );
    assert_eq!(
        a.bt_connect_row(0),
        vec![Action::BtConnectDevice(0)],
        "another row is a new request"
    );
    // The link comes up: the refresh marks the row connected and the spinner stops.
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", true);
    a.bt_paired_add("WONDERBOOM", "Speaker", false);
    assert_eq!(a.bt_connecting, None);
}

/// The spinner asks for a repaint when its head moves (eight times a second), on every screen:
/// the status bar carries the ring everywhere.
#[test]
fn the_spinner_repaints_per_step_not_per_frame() {
    let mut a = unlocked();
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", false);
    a.push_for_test(Screen::Pairing);
    a.bt_connect_row(0);
    let painted = (0..60).filter(|_| a.tick_dt(16)).count(); // 0.96 s of 60 Hz ticks
    assert!((6..=9).contains(&painted), "{painted} repaints in a second");
    a.push_for_test(Screen::NowPlaying);
    let painted = (0..60).filter(|_| a.tick_dt(16)).count();
    assert!(
        (6..=9).contains(&painted),
        "{painted} repaints on Now Playing"
    );
}

/// …and a connect that never lands must not lock its row forever.
#[test]
fn a_connect_that_goes_nowhere_frees_its_row() {
    let mut a = unlocked();
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", false);
    assert_eq!(a.bt_connect_row(0), vec![Action::BtConnectDevice(0)]);
    for _ in 0..(App::BT_CONNECT_SPIN_MS / 200 + 1) {
        a.tick_dt(200);
    }
    assert_eq!(a.bt_connecting, None, "the spinner gave up");
    assert_eq!(
        a.bt_connect_row(0),
        vec![Action::BtConnectDevice(0)],
        "and the row can be tapped again"
    );
    a.set_bt_on(false);
    assert_eq!(
        a.bt_connecting, None,
        "switching the radio off stops it too"
    );
}

/// 2026-09-16 on the device: after a disconnect the headphones' row still said connected, so
/// every tap meant as "connect" sent another hang-up. The rows must follow the live link.
#[test]
fn a_dropped_link_turns_its_row_back_into_a_connect() {
    let mut a = unlocked();
    a.bt_paired_clear();
    a.bt_paired_add("WH-1000XM4", "Headphones", true);
    a.bt_paired_add("WONDERBOOM", "Speaker", false);
    a.set_bt_connected(Some("WH-1000XM4"));
    assert!(a.bt_paired[0].connected);
    a.set_bt_connected(None); // the link went away; nobody re-read the list
    assert!(
        !a.bt_paired[0].connected,
        "the row stopped saying connected"
    );
    assert_eq!(
        a.bt_connect_row(0),
        vec![Action::BtConnectDevice(0)],
        "so a tap connects"
    );
    // The link comes back before the list is read again: the row says so.
    a.set_bt_connected(Some("WH-1000XM4"));
    assert!(a.bt_paired[0].connected && !a.bt_paired[1].connected);
}

/// Two paired devices with one name: the one the list marked keeps the flag.
#[test]
fn a_shared_name_keeps_the_row_the_list_marked() {
    let mut a = unlocked();
    a.bt_paired_clear();
    a.bt_paired_add("Speaker", "Speaker", false);
    a.bt_paired_add("Speaker", "Speaker", true);
    a.set_bt_connected(Some("Speaker"));
    assert!(!a.bt_paired[0].connected && a.bt_paired[1].connected);
}
