use super::*;
use crate::i18n::{enter, Language};

#[test]
fn moved_sound_controls_and_nc_keep_their_domain_values_in_both_languages() {
    for language in [Language::English, Language::SimplifiedChinese] {
        let _language = enter(language);
        let mut a = App::unlocked();
        a.set_language(language);
        a.push_for_test(Screen::Sound);
        a.set_nc_plug(true, true);
        assert!(a.tap(240, crate::sound::TOP + 10).is_empty());
        assert_eq!(a.current(), Screen::Nc);
        let auto = crate::nc::centre(crate::nc::Hit::Mode(1), 0, true);
        assert!(a.tap(240, auto).is_empty());
        assert_eq!(a.nc_items(), Some(("ainc", "nw500n")));
        a.set_language(language.next());
        assert_eq!(a.nc_items(), Some(("ainc", "nw500n")));
        a.set_nc_plug(false, false);
        assert_eq!(a.current(), Screen::Sound);
        assert_eq!(a.nc_items(), None);
        a.tap(
            240,
            crate::sound::row_top(crate::sound::ROW_ADVANCED, 0) + 20,
        );
        assert_eq!(a.current(), Screen::Advanced);
        let vpt = crate::advanced::row_top(crate::advanced::ROW_VPT, false, 0) + 20;
        assert_eq!(a.tap(240, vpt), vec![Action::SoundChanged]);
        assert!(a.snd_vpt);
        a.scroll_px(10_000);
        assert_eq!(a.adv_scroll_px, crate::advanced::max_scroll(false));
        a.tap(
            240,
            crate::advanced::row_top(crate::advanced::ROW_DAC_EQ, false, a.adv_scroll_px) + 20,
        );
        assert_eq!(a.current(), Screen::DacEq);
    }
}

/// The installer decides which languages exist on the player; the saved choice only picks
/// among them.
#[test]
fn only_installed_languages_can_be_chosen() {
    let mut a = App::unlocked();
    a.set_language(Language::SimplifiedChinese);
    assert_eq!(a.language(), Language::English, "English-only by default");
    a.set_languages(&[Language::SimplifiedChinese]);
    assert_eq!(a.language(), Language::SimplifiedChinese);
    a.set_language(Language::English);
    assert_eq!(a.language(), Language::SimplifiedChinese);
    assert!(!a.settings_lang());
    a.set_languages(&[]);
    assert_eq!(a.language(), Language::English);
    // Hidden row: the cursor steps over it.
    a.go(Screen::Settings);
    a.open_settings_page(crate::settings::page_of(crate::settings::ROW_RESET));
    a.settings_sel = crate::settings::ROW_RESET;
    a.press(Button::Down);
    assert_eq!(a.settings_sel, crate::settings::ROW_FIRMWARE);
    a.press(Button::Up);
    assert_eq!(a.settings_sel, crate::settings::ROW_RESET);
}

#[test]
fn language_is_reversible_without_a_transport_action_or_losing_preferences() {
    let mut a = App::unlocked();
    a.set_languages(&Language::ALL);
    a.playing = true;
    a.brightness = 3;
    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_LANGUAGE;
    assert_eq!(a.language(), Language::English);
    assert!(a.settings_activate().is_empty());
    assert_eq!(a.current(), Screen::Language, "the row opens the list");
    let row = |i: i32| crate::language::TOP + i * crate::kit::ROW_H() + crate::kit::ROW_H() / 2;
    a.tap(240, row(1));
    assert_eq!(a.language(), Language::SimplifiedChinese);
    assert_eq!(a.current(), Screen::Language, "picking stays on the page");
    assert!(a.playing);
    assert_eq!(a.brightness, 3);
    a.tap(240, row(0));
    assert_eq!(a.language(), Language::English);
    assert!(a.playing);
    assert_eq!(a.brightness, 3);
    let top =
        crate::chrome::HEADER_BOTTOM + crate::settings::row_top_px(crate::settings::ROW_LANGUAGE);
    let scroll = (top - 200).max(0);
    assert_eq!(
        crate::settings::row_at(
            top - scroll + 20,
            scroll,
            crate::settings::page_of(crate::settings::ROW_LANGUAGE),
            true
        ),
        Some(crate::settings::ROW_LANGUAGE)
    );
}

#[test]
fn fm_actions_and_english_track_information_keys_work_in_both_languages() {
    for language in [Language::English, Language::SimplifiedChinese] {
        let _language = enter(language);
        let mut a = App::unlocked();
        a.set_language(language);
        a.push_for_test(Screen::Fm);
        assert_eq!(a.tap(400, 63), vec![Action::FmPower(true)]);
        assert_eq!(a.tap(400, 63), vec![Action::FmPower(false)]);
        assert_eq!(a.tap(180, 355), vec![Action::FmSeek(-1)]);
        assert_eq!(a.tap(280, 355), vec![Action::FmSeek(1)]);
        assert_eq!(a.tap(240, 640), vec![Action::FmScan]);

        let mut b = App::unlocked();
        b.set_language(language);
        let artist = b.lib.artists[0].name.clone();
        b.push_for_test(Screen::TrackInfo);
        b.set_track_info(vec![("Artist".into(), artist.clone())]);
        b.set_track_info_rows_for_test(vec![44]);
        b.tap(240, crate::track_info::TOP + 4);
        assert_eq!(b.current(), Screen::Artist);
        assert_eq!(b.track_info_rows()[0], ("Artist".into(), artist));
        assert!(crate::track_info::is_rating("Rating"));
        assert!(!crate::track_info::is_rating("评分"));
    }
}
