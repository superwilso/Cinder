use super::*;
use crate::display::{ROW_ACCENT, ROW_PALETTE};

fn slate() -> Palette {
    Palette::parse("slate", include_str!("../../palettes/slate.palette")).expect("slate loads")
}

fn paper() -> Palette {
    Palette::parse("paper", include_str!("../../palettes/paper.palette")).expect("paper loads")
}

/// Put the cursor on a Settings ▸ Display row. `settings_activate` in these tests is the
/// Display page's `display_activate` — the palette and accent rows moved there.
fn on_settings_row(a: &mut App, row: usize) {
    a.go(Screen::Display);
    a.display_sel = row;
}

/// Content-to-screen y of the picker's `i`th palette row, at scroll 0.
fn palette_row_y(i: usize) -> i32 {
    crate::chrome::HEADER_BOTTOM
        + 2 * crate::kit::SECTION_H
        + crate::kit::CHIP_H
        + 8
        + i as i32 * crate::kit::ROW_H()
        + crate::kit::ROW_H() / 2
}

/// Display ▸ Palette opens the picker; a tap on a row applies that palette at once and stays
/// Help & controls (5i) is one list opened from the Menu at the top. Its rows are facts and do
/// nothing; the last one replays the first-run intro, and finishing the intro comes back here.
#[test]
fn help_is_a_list_whose_last_row_replays_the_intro() {
    let open_from_menu = |want: Screen| {
        let mut a = App::unlocked();
        a.press(Button::Up); // Menu
        let idx = a
            .menu_visible()
            .iter()
            .position(|m| m.0 == want)
            .expect("a Menu row");
        a.activate_menu(idx);
        a
    };
    let mut a = open_from_menu(Screen::Help);
    assert_eq!(a.current(), Screen::Help);
    a.scroll_px(10_000);
    let max = crate::help::max_scroll(false);
    assert_eq!(
        a.help_scroll_px, max,
        "the scroll stops at the end of the list"
    );
    // Opening it again starts at the top.
    a.press(Button::Back);
    let mut a = open_from_menu(Screen::Help);
    assert_eq!(a.help_scroll_px, 0);
    assert!(a.tap(240, crate::chrome::HEADER_BOTTOM + 60).is_empty());
    assert_eq!(a.current(), Screen::Help, "a fact row is not a target");
    a.scroll_px(10_000);
    a.tap(240, crate::canvas::H as i32 - 30);
    assert_eq!(
        a.current(),
        Screen::Onboarding,
        "the last row replays the intro"
    );
    assert_eq!(a.onboarding_page, 0);
    for _ in 0..crate::onboarding::PAGES + 1 {
        a.swipe(-1, 240, 400);
    }
    assert_eq!(
        a.current(),
        Screen::Help,
        "finishing the replay comes back to Help"
    );
}

/// on the page, and Cinder is always the first row.
#[test]
fn the_palette_row_opens_the_picker_and_a_tap_picks() {
    let mut a = App::unlocked();
    a.set_palettes(vec![slate(), paper()], Vec::new());
    on_settings_row(&mut a, ROW_PALETTE);
    a.display_activate();
    assert_eq!(a.current(), Screen::Palette);
    assert!(
        a.take_palettes_stale(),
        "the folder is read again on the way in"
    );
    assert_eq!((a.palette_id(), a.palette_name()), ("cinder", "Cinder"));
    a.tap(240, palette_row_y(1));
    assert_eq!(
        (a.palette_id(), a.palette_name()),
        ("paper", "Paper"),
        "sorted by name: Paper before Slate"
    );
    assert_eq!(a.palette, paper().tokens);
    assert_eq!(a.current(), Screen::Palette, "picking stays on the page");
    a.tap(240, palette_row_y(2));
    assert_eq!(a.palette_id(), "slate");
    a.tap(240, palette_row_y(0));
    assert_eq!(a.palette_id(), "cinder");
    assert_eq!(a.palette, CINDER);
}

/// Added puts the newest file first; Cinder stays on top either way.
#[test]
fn the_picker_sorts_by_name_or_by_date_added() {
    let mut a = App::unlocked();
    let (mut p, mut s2) = (paper(), slate());
    p.added = 100;
    s2.added = 200;
    a.set_palettes(vec![p, s2], Vec::new());
    a.push(Screen::Palette);
    let (x, w) = crate::kit::chip_span(1, 2);
    a.tap(
        x + w / 2,
        crate::chrome::HEADER_BOTTOM + crate::kit::SECTION_H + crate::kit::CHIP_H / 2,
    );
    assert_eq!(a.palette_sort(), "added");
    a.tap(240, palette_row_y(1));
    assert_eq!(a.palette_id(), "slate", "newest first");
    a.set_palette_sort("name");
    a.tap(240, palette_row_y(1));
    assert_eq!(a.palette_id(), "paper");
    a.set_palette_sort("nonsense");
    assert_eq!(a.palette_sort(), "name", "an unknown word is the default");
}

/// The settings file is read at boot, before /contents can be relied on, so the folder can
/// come back empty. The saved choice has to survive that — it is what gets written back.
#[test]
fn a_saved_palette_that_has_not_loaded_is_kept_rather_than_forgotten() {
    let mut a = App::unlocked();
    a.set_palette_wanted(" Slate ");
    assert_eq!(
        a.palette_id(),
        "slate",
        "trimmed, lowercased, and remembered"
    );
    assert_eq!(a.palette, CINDER, "nothing is loaded yet, so Cinder draws");
    a.set_palettes(Vec::new(), Vec::new());
    assert_eq!(
        a.palette_id(),
        "slate",
        "an empty read must not overwrite the choice"
    );
    a.set_palettes(vec![slate()], Vec::new());
    assert_eq!(
        a.palette,
        slate().tokens,
        "and it applies the moment it loads"
    );
    assert_eq!(a.palette_name(), "Slate");
}

#[test]
fn a_palette_with_its_own_accent_takes_the_accent_row_out_of_play() {
    let mut a = App::unlocked();
    a.set_palettes(vec![paper()], Vec::new());
    a.set_palette_wanted("paper");
    assert!(a.palette_pins_accent());
    on_settings_row(&mut a, ROW_ACCENT);
    let before = a.accent;
    a.display_activate();
    assert_eq!(
        a.accent, before,
        "Select must not cycle an accent that is not in use"
    );
    assert_eq!(a.toast, "This palette sets its own accent");
    // A tap where the last swatch would be changes nothing either, and explains the same way.
    a.toast.clear();
    let y = crate::display::row_top(ROW_ACCENT) + crate::kit::ROW_H() / 2;
    let x = (0..crate::canvas::W as i32)
        .find(|&x| crate::display::accent_hit(x, y) == Some(Accent::COUNT - 1))
        .expect("the last swatch has a hit zone");
    a.tap(x, y);
    assert_eq!(a.accent, before);
    assert_eq!(a.toast, "This palette sets its own accent");
}

#[test]
fn opening_settings_asks_the_shell_to_read_the_folder_again() {
    let mut a = App::unlocked();
    assert!(!a.take_palettes_stale());
    a.push(Screen::Settings);
    assert!(a.take_palettes_stale(), "arriving on Settings is the cue");
    assert!(
        !a.take_palettes_stale(),
        "once per visit, not once per frame"
    );
    a.push(Screen::Settings);
    assert!(
        !a.take_palettes_stale(),
        "already there, so nothing new to read"
    );
    a.push(Screen::Display);
    assert!(
        a.take_palettes_stale(),
        "arriving on Display — where the palette is picked — is too"
    );
}

/// An empty folder is a picker with Cinder in it and the ADD row saying where files go.
#[test]
fn an_empty_folder_leaves_cinder_and_says_where_palettes_go() {
    let mut a = App::unlocked();
    on_settings_row(&mut a, ROW_PALETTE);
    a.display_activate();
    assert_eq!(a.palette_order(), vec![None]);
    let add = crate::palette_list::item_at(240, palette_row_y(1) + crate::kit::SECTION_H, 0, 1, 0);
    assert_eq!(add, Some(crate::palette_list::Item::Add));
    a.tap(240, palette_row_y(1) + crate::kit::SECTION_H);
    assert_eq!(
        a.palette_id(),
        "cinder",
        "the ADD row explains; it picks nothing"
    );
}

/// A refused file is listed on the picker with the first thing wrong with it.
#[test]
fn skipped_files_are_listed_with_their_first_problem() {
    let mut a = App::unlocked();
    let why = vec![
        "bad.palette: day.ink on day.bg: contrast 1.00, needs at least 4.50; line 9: unknown key"
            .to_string(),
    ];
    assert!(
        a.set_palettes(vec![slate()], why.clone()),
        "a new set is news"
    );
    assert!(!a.set_palettes(vec![slate()], why), "the same set is not");
    let s = crate::palette_list::skipped_from(&a.palette_skipped[0]);
    assert_eq!(s.file, "bad.palette");
    assert_eq!(
        s.why,
        "day.ink on day.bg: contrast 1.00, needs at least 4.50"
    );
}

/// The receiver runs only while its page is in the stack, and a code arriving there is
/// answered there.
#[test]
fn the_receiver_is_on_only_while_its_page_is_open() {
    let mut a = App::unlocked();
    a.bt_on = true;
    a.push(Screen::Receiver);
    let (x, y) = (240, crate::receiver::SWITCH_Y + 10);
    assert_eq!(a.tap(x, y), vec![Action::RxChanged]);
    assert!(a.rx_on());
    a.set_bt_prompt(1, "ARTHURS-PC", 114363);
    assert_eq!(
        a.current(),
        Screen::Receiver,
        "the code is shown on the receiver, not Devices"
    );
    a.set_bt_prompt(0, "", 0);
    a.pop();
    assert!(!a.rx_on(), "leaving the page is what turns it off");
}

#[test]
fn the_receiver_needs_the_radio_to_start() {
    let mut a = App::unlocked();
    a.bt_on = false;
    a.push(Screen::Receiver);
    assert_eq!(
        a.tap(240, crate::receiver::SWITCH_Y + 10),
        Vec::<Action>::new()
    );
    assert_eq!(a.toast, "Bluetooth is off. Turn it on first.");
    assert!(!a.rx_on());

    a.set_bt_on(true);
    assert_eq!(
        a.tap(240, crate::receiver::SWITCH_Y + 10),
        vec![Action::RxChanged]
    );
    assert!(a.rx_on());
}

#[test]
fn receiver_codec_preferences_are_independent_and_reconnect_once_per_change() {
    let mut a = App::unlocked();
    let tx_pref = a.bt_codec();
    a.push(Screen::Receiver);

    let (x, y) = crate::receiver::codec_pref_chip(1);
    assert_eq!(a.tap(x, y), vec![Action::RxCodecChanged]);
    assert_eq!(a.rx_codec_pref(), 1);
    assert_eq!(
        a.bt_codec(),
        tx_pref,
        "receiver selection does not alter transmit preference"
    );
    assert!(a.toast.contains("Saved"));
    assert!(
        a.tap(x, y).is_empty(),
        "the already-selected chip does not reconnect"
    );

    assert_eq!(
        a.tap(240, crate::receiver::SWITCH_Y + 10),
        vec![Action::RxChanged]
    );
    let (x, y) = crate::receiver::codec_pref_chip(2);
    assert_eq!(a.tap(x, y), vec![Action::RxCodecChanged]);
    assert_eq!(a.rx_codec_pref(), 2);
    assert!(a.toast.contains("Reconnect"));

    a.pop();
    a.push(Screen::Receiver);
    assert_eq!(
        a.rx_codec_pref(),
        2,
        "the preference survives leaving the page"
    );
    a.set_rx_codec_pref(255);
    assert_eq!(
        a.rx_codec_pref(),
        0,
        "an invalid restored value falls back to Auto"
    );
}

/// A pairing request that arrives on another screen brings the user to where it can be
/// answered; clearing it does not move them.
#[test]
fn an_incoming_pairing_prompt_opens_devices() {
    let mut a = App::unlocked();
    assert_ne!(a.current(), Screen::Pairing);
    a.set_bt_prompt(1, "ARTHURS-PC", 114363);
    assert_eq!(a.current(), Screen::Pairing);
    let depth = a.stack.len();
    a.set_bt_prompt(1, "ARTHURS-PC", 54782);
    assert_eq!(
        a.stack.len(),
        depth,
        "a second prompt does not stack another Devices"
    );
    a.set_bt_prompt(0, "", 0);
    assert_eq!(
        a.current(),
        Screen::Pairing,
        "the answer leaves the user where they are"
    );
}

#[test]
fn reset_settings_goes_back_to_cinder_but_keeps_what_the_folder_holds() {
    let mut a = App::unlocked();
    a.set_palettes(vec![slate()], Vec::new());
    a.set_palette_wanted("slate");
    let _ = a.reset_settings();
    assert_eq!(a.palette_id(), "cinder");
    assert_eq!(a.palette, CINDER);
    assert_eq!(
        a.palettes.len(),
        1,
        "the file is still in the folder; only the choice resets"
    );
}

/// The whole point: the chosen palette is what reaches the panel.
#[test]
fn the_palette_reaches_the_pixels() {
    let _scale = crate::text::scale_guard();
    let fonts = FontSet::load();
    let np = NowPlaying {
        skin: None,
        tape: None,
        title: "",
        artist: "",
        codec: "",
        badge: "",
        clock: "12:00",
        battery: 50,
        elapsed: "",
        remaining: "",
        progress: 0.0,
        art: "",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: false,
        shuffle: false,
        repeat: 0,
        viz_seed: 0.0,
        viz_kind: 0,
        viz_size: 0,
        page: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        scrubbing: false,
        lyrics: false,
    };
    // The background is the colour most of the frame is painted in.
    let dominant = |a: &mut App| {
        let mut c = Canvas::new();
        a.render(&mut c, &fonts, &np);
        let mut n = std::collections::HashMap::new();
        for &p in &c.buf {
            *n.entry(p).or_insert(0u32) += 1;
        }
        n.into_iter()
            .max_by_key(|&(_, k)| k)
            .map(|(p, _)| p)
            .unwrap()
    };
    let mut a = App::unlocked();
    a.go(Screen::Menu);
    assert_eq!(dominant(&mut a), crate::canvas::to_u32(Theme::day().bg));
    a.set_palettes(vec![slate()], Vec::new());
    a.set_palette_wanted("slate");
    assert_eq!(
        dominant(&mut a),
        0x0e1116,
        "the Menu is painted in slate's day.bg"
    );
}
