use super::*;
use crate::model::{PlaylistRow, TrackStat};
use crate::playlist_edit as pe;
use crate::view_edit as ve;

fn app() -> App {
    let mut a = App::unlocked();
    a.set_library(Library::sample());
    a
}

/// An app with one of Cinder's own playlists, six tracks long, open on its page.
fn on_own_playlist() -> (App, i64) {
    let mut a = app();
    let rows: Vec<SongRow> = a.lib.songs.iter().take(6).cloned().collect();
    a.set_playlists(vec![PlaylistRow {
        id: -77,
        name: "Night Bus".into(),
        tracks: rows.len() as u32,
        user: true,
        track_list: rows,
        ..Default::default()
    }]);
    a.go(Screen::Library);
    a.open_playlist_by_id(-77);
    assert_eq!(a.current(), Screen::Playlist);
    (a, -77)
}

fn ids(a: &App) -> Vec<i64> {
    a.playlist_row()
        .unwrap()
        .track_list
        .iter()
        .map(|s| s.object_id)
        .collect()
}

const EDIT: (i32, i32) = (440, 62);

/// Press the keyboard's DONE where it is drawn.
fn kb_done(a: &mut App) -> Vec<Action> {
    let page = a.kb_page;
    let (row, col) = (0..crate::keyboard::ROWS)
        .flat_map(|r| (0..12).map(move |c| (r, c)))
        .find(|&(r, c)| crate::keyboard::key_at(page, r, c) == Some(crate::keyboard::Key::Done))
        .expect("DONE is on this page");
    let (x, y, w, h) = crate::keyboard::key_rect(page, row, col).unwrap();
    a.tap(x + w / 2, y + h / 2)
}

/// Plan 1c in the editor: a tap on a title glues it to the row above and again unglues it; the
/// top row cannot be glued; DONE hands the glue over with the order.
#[test]
fn the_playlist_editor_glues_a_row_to_the_one_above() {
    let _g = crate::text::scale_guard();
    let (mut a, id) = on_own_playlist();
    let n = ids(&a).len();
    a.tap(EDIT.0, EDIT.1);
    let mid = |i: usize| pe::row_top(i, 0) + pe::RH() / 2;
    assert!(
        a.tap(200, mid(0)).is_empty(),
        "the top row has nothing above it"
    );
    a.tap(200, mid(2));
    a.tap(200, mid(3));
    a.tap(200, mid(3));
    assert_eq!(a.tap(EDIT.0, EDIT.1), vec![Action::PlaylistSaveEdit(id)]);
    let (_, order, glued) = a.take_playlist_edit().unwrap();
    assert_eq!(order.len(), n);
    let mut want = vec![false; n];
    want[2] = true;
    assert_eq!(glued, want);
    assert_eq!(
        a.playlist_row().map(|p| p.glued.clone()),
        Some(want),
        "the page shows it at once"
    );
}

/// The whole editor, by touch: EDIT, remove a row, drag a row, UNDO, DONE — and the action the
/// shell gets carries exactly the order on screen.
#[test]
fn the_playlist_editor_moves_removes_undoes_and_saves() {
    let (mut a, id) = on_own_playlist();
    let before = ids(&a);
    assert!(a.tap(EDIT.0, EDIT.1).is_empty());
    assert_eq!(a.current(), Screen::PlaylistEdit);

    // × on the second row: gone at once, and nothing has been written.
    let y1 = pe::row_top(1, 0) + pe::RH() / 2;
    assert!(a.tap(pe::REMOVE_X0 + 20, y1).is_empty());
    assert_eq!(a.pl_edit.as_ref().unwrap().order, [0, 2, 3, 4, 5]);
    assert_eq!(
        ids(&a),
        before,
        "the playlist itself is untouched until DONE"
    );

    // ≡ on the first row, dragged down two rows.
    let y0 = pe::row_top(0, 0) + pe::RH() / 2;
    assert!(
        !a.reorder_begin(200, y0),
        "a drag that starts on the title scrolls"
    );
    assert!(a.reorder_begin(30, y0), "the handle lifts the row");
    a.reorder_track(2 * pe::RH());
    assert!(
        a.reorder_release().is_empty(),
        "nothing for the shell until DONE"
    );
    assert_eq!(a.pl_edit.as_ref().unwrap().order, [2, 3, 0, 4, 5]);

    // UNDO takes back the move, and only the move.
    assert!(a.tap(440, pe::LABEL_Y + 20).is_empty());
    assert_eq!(a.pl_edit.as_ref().unwrap().order, [0, 2, 3, 4, 5]);
    // Do the move again and save.
    assert!(a.reorder_begin(30, y0));
    a.reorder_track(2 * pe::RH());
    a.reorder_release();

    assert_eq!(a.tap(EDIT.0, EDIT.1), vec![Action::PlaylistSaveEdit(id)]);
    assert_eq!(a.current(), Screen::Playlist, "DONE returns to the page");
    let want: Vec<i64> = [2usize, 3, 0, 4, 5].iter().map(|&i| before[i]).collect();
    assert_eq!(
        a.take_playlist_edit(),
        Some((id, want.clone(), vec![false; want.len()]))
    );
    assert_eq!(ids(&a), want, "the page shows the saved order at once");
    assert!(a.playlist_row().unwrap().edited);
    assert_eq!(a.take_playlist_edit(), None, "taken once");
}

/// Back leaves without saving, and DONE with nothing changed writes nothing.
#[test]
fn the_playlist_editor_saves_only_on_done_and_only_a_change() {
    let (mut a, _) = on_own_playlist();
    let before = ids(&a);
    a.tap(EDIT.0, EDIT.1);
    a.tap(pe::REMOVE_X0 + 20, pe::row_top(0, 0) + pe::RH() / 2);
    assert!(a.press(Button::Back).is_empty());
    assert_eq!(a.current(), Screen::Playlist);
    assert_eq!(ids(&a), before);
    assert!(a.pl_edit.is_none() && a.take_playlist_edit().is_none());

    a.tap(EDIT.0, EDIT.1);
    assert!(
        a.tap(EDIT.0, EDIT.1).is_empty(),
        "DONE with no change is not a write"
    );
    assert_eq!(a.current(), Screen::Playlist);
    assert!(!a.playlist_row().unwrap().edited);
}

/// Sony's playlists are not Cinder's to edit: no EDIT is drawn and the tap opens nothing.
#[test]
fn a_sony_playlist_offers_no_editor() {
    let mut a = app();
    a.go(Screen::Library);
    let i = a
        .lib
        .playlists
        .iter()
        .position(|p| !p.user && !p.smart)
        .expect("the sample has Sony rows");
    a.open_playlist(i);
    a.tap(EDIT.0, EDIT.1);
    assert_eq!(a.current(), Screen::Playlist);
}

/// Rating by touch: Track information's stars rate the playing song, the same star again
/// clears it, and the album's header reads the mean.
#[test]
fn the_stars_on_track_information_rate_the_playing_song() {
    let mut a = app();
    let album = a.lib.albums_flat()[0].clone();
    a.set_play_context(album.track_list.clone(), 0);
    let playing = album.track_list[0].object_id;
    a.set_track_info(vec![
        ("Rating".into(), "-".into()),
        ("Title".into(), "x".into()),
    ]);
    a.go(Screen::NowPlaying);
    a.push(Screen::TrackInfo);
    // The row heights the renderer would have measured: two one-line rows.
    a.set_track_info_rows_for_test(vec![44, 44]);

    let y = crate::track_info::TOP + 20;
    let star = |n: i32| 176 + (n - 1) * crate::track_info::STAR_PITCH + 22;
    assert_eq!(a.tap(star(4), y), vec![Action::RateTrack(playing, 4)]);
    assert_eq!(a.lib.stat(playing).rating, 4);
    assert_eq!(
        a.lib.album_rating(&album),
        Some(4),
        "one rated track is the album's rating"
    );
    assert_eq!(a.tap(star(2), y), vec![Action::RateTrack(playing, 2)]);
    assert_eq!(
        a.tap(star(2), y),
        vec![Action::RateTrack(playing, 0)],
        "the same star clears it"
    );
    assert_eq!(a.lib.stat(playing), TrackStat::default());
    assert!(
        a.lib.stats.is_empty(),
        "a cleared track leaves no entry behind"
    );
    assert_eq!(a.lib.album_rating(&album), None);
    // Left of the stars is the label, not a star.
    assert!(a.tap(60, y).is_empty());
}

/// A smart playlist end to end: SMART on the Playlists tab, a name, a rule, SAVE — and it is
/// on top of the list holding exactly what the rule matches, plays as that list, and follows
/// a rating made afterwards.
#[test]
fn a_smart_playlist_is_made_saved_listed_and_played() {
    let mut a = app();
    let (s0, s1) = (a.lib.songs[0].object_id, a.lib.songs[1].object_id);
    a.set_track_stat(
        s0,
        TrackStat {
            rating: 5,
            plays: 3,
            last_played: 10,
        },
    );
    a.go(Screen::Library);
    a.lib_tab = Tab::Playlists;
    let (_, ny, _, nh) = library::new_playlist_rect();
    a.tap(library::NEW_SPLIT_X + 40, ny + nh / 2);
    assert_eq!(a.current(), Screen::ViewEdit);
    a.tap(100, ve::NAME_Y + 20);
    assert_eq!(a.current(), Screen::Keyboard);
    a.type_for_test("Best");
    kb_done(&mut a);
    assert_eq!(a.current(), Screen::ViewEdit);
    // RATING ▸ 4+ (chip 3 of 5).
    let (cx, cw) = crate::kit::chip_span(3, 5);
    a.tap(cx + cw / 2, ve::section_top(0) + crate::kit::SECTION_H + 10);
    assert!(
        a.tap(440, 62).is_empty(),
        "SAVE is screen state; the shell writes the file by diff"
    );

    assert_eq!(
        a.current(),
        Screen::Playlist,
        "a new one lands inside itself"
    );
    let p = a.playlist_row().unwrap().clone();
    assert!(p.smart && p.name == "Best" && !p.user);
    assert_eq!(
        p.track_list.iter().map(|s| s.object_id).collect::<Vec<_>>(),
        [s0]
    );
    assert!(a.lib.playlists[0].smart, "smart playlists lead the tab");
    assert!(a.views_body().contains("[Best]\nrating=4\n"));

    // It plays as the list on screen, through the same channel the Songs tab uses.
    let (bx, by, _, bh) = library::playlist_play_band();
    assert_eq!(a.tap(bx + 20, by + bh / 2), vec![Action::PlayListAt(0)]);
    assert_eq!(a.take_play_list(), [s0]);
    let (sx, sy, _, sh) = library::playlist_shuffle_band();
    assert_eq!(a.tap(sx + 20, sy + sh / 2), vec![Action::ShuffleList]);
    assert_eq!(a.take_play_list(), [s0]);

    // A rating made later is in the list the next time it is built.
    a.set_track_stat(
        s1,
        TrackStat {
            rating: 4,
            ..Default::default()
        },
    );
    assert_eq!(a.playlist_row().unwrap().tracks, 2);
    // "Add to playlist" never offers it, and the picker does not count it as one of Sony's.
    assert!(a.user_playlists().iter().all(|(_, n, _)| n != "Best"));
}

/// EDIT on a smart playlist opens its rules; Delete asks, then removes it and leaves its page.
#[test]
fn a_smart_playlist_is_edited_and_deleted_from_its_page() {
    let mut a = app();
    a.set_views_body("[Old]\nrating=3\n");
    a.go(Screen::Library);
    a.open_playlist_by_id(crate::views::smart_id("Old"));
    assert_eq!(a.current(), Screen::Playlist);
    a.tap(EDIT.0, EDIT.1);
    assert_eq!(a.current(), Screen::ViewEdit);
    // SORT ▸ Plays, SAVE: back on the same page, same name, new rule.
    let (cx, cw) = crate::kit::chip_span(1, 5);
    let sort = ve::SECTIONS
        .iter()
        .position(|s| *s == ve::Section::Sort)
        .unwrap();
    a.tap(
        cx + cw / 2,
        ve::section_top(sort) + crate::kit::SECTION_H + 10,
    );
    a.tap(440, 62);
    assert_eq!(a.current(), Screen::Playlist);
    assert_eq!(a.views[0].sort, crate::views::ViewSort::Plays);
    assert_eq!(a.playlist_row().unwrap().name, "Old");

    a.tap(EDIT.0, EDIT.1);
    a.tap(200, ve::DELETE_Y() + 20);
    assert!(a.modal_open());
    let (x, y) = crate::confirm::confirm_button_centre(crate::confirm::Ask::DeleteView);
    a.tap(x, y);
    assert!(a.views.is_empty());
    assert_eq!(
        a.current(),
        Screen::Library,
        "the page it deleted must not stay open"
    );
    assert!(a.lib.playlists.iter().all(|p| !p.smart));
    // Back out of the editor of a NEW view saves nothing.
    a.lib_tab = Tab::Playlists;
    a.open_view_edit(None);
    a.press(Button::Back);
    assert!(a.views.is_empty() && a.view_edit.is_none());
}

/// The shell's playlist refresh replaces the stored rows; the smart ones are put back on top
/// and the open page is found again by id.
#[test]
fn a_playlist_refresh_keeps_the_smart_rows_and_the_open_page() {
    let (mut a, id) = on_own_playlist();
    a.set_views_body("[Mine]\n");
    assert!(a.lib.playlists[0].smart);
    assert_eq!(a.playlist_row().unwrap().id, id);
    let rows: Vec<PlaylistRow> = a
        .lib
        .playlists
        .iter()
        .filter(|p| !p.smart)
        .cloned()
        .collect();
    a.set_playlists(rows);
    assert_eq!(
        a.lib.playlists.iter().filter(|p| p.smart).count(),
        1,
        "not doubled, not dropped"
    );
    assert_eq!(a.playlist_row().unwrap().id, id);
}

/// Settings ▸ Shuffle cycles songs → albums → artists, and the shuffle button then deals whole
/// albums in their own order, carrying on with the playing one first.
#[test]
fn shuffle_by_album_deals_whole_albums_after_the_playing_one() {
    let mut a = App::unlocked();
    assert_eq!(a.shuffle_by(), "songs", "the default changes nothing");
    a.go(Screen::Settings);
    a.settings_sel = crate::settings::ROW_SHUFFLE;
    a.settings_activate();
    assert_eq!(a.shuffle_by(), "albums");
    let rows: Vec<SongRow> = (0..4i64)
        .flat_map(|al| {
            (1..=5).map(move |t| SongRow {
                title: format!("{al}-{t}"),
                object_id: al * 10 + t as i64,
                album_id: al + 1,
                disc: 1,
                track: t,
                artist: format!("Artist {}", al / 2),
                ..Default::default()
            })
        })
        .collect();
    a.set_play_context(rows.clone(), 7); // album 2 (id 2), track 3
    assert_eq!(a.queue_shuffle(), vec![Action::QueueChanged]);
    let got: Vec<(i64, i32)> = a.context().iter().map(|s| (s.album_id, s.track)).collect();
    assert_eq!(
        &got[..5],
        &[(2, 3), (2, 4), (2, 5), (2, 1), (2, 2)],
        "the playing album first, from the playing track"
    );
    for chunk in got[5..].chunks(5) {
        assert!(
            chunk.iter().all(|(al, _)| *al == chunk[0].0),
            "an album was split: {got:?}"
        );
        assert_eq!(
            chunk.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
    }
    // Shuffle off puts the original order back, as it does for songs.
    a.unshuffle_context();
    assert_eq!(
        a.context().iter().map(|s| s.object_id).collect::<Vec<_>>(),
        rows.iter().map(|s| s.object_id).collect::<Vec<_>>()
    );

    // By artist: two artists, each one's two albums back to back.
    a.set_shuffle_by("artists");
    a.set_play_context(rows, 0);
    a.queue_shuffle();
    let artists: Vec<&str> = a.context().iter().map(|s| s.artist.as_str()).collect();
    assert_eq!(
        artists.windows(2).filter(|w| w[0] != w[1]).count(),
        1,
        "{artists:?}"
    );
}

/// The artist page: albums newest first, then the most played, then every song — and a most
/// played row plays the artist's list from that song.
#[test]
fn the_artist_page_orders_albums_by_year_and_lists_the_most_played() {
    use crate::model::{AlbumRow, ArtistGroup};
    let mk = |name: &str, year: &str, base: i64| AlbumRow {
        name: name.into(),
        artist: "A".into(),
        year: year.into(),
        tracks: 2,
        album_id: base,
        track_list: (0..2)
            .map(|i| SongRow {
                title: format!("{name}{i}"),
                object_id: base + i,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut lib = Library {
        album_groups: vec![ArtistGroup {
            artist: "A".into(),
            albums: vec![
                mk("Old", "1999", 10),
                mk("Undated", "", 20),
                mk("New", "2021", 30),
            ],
        }],
        ..Default::default()
    };
    let p = library::artist_page(&lib, "A", true);
    assert_eq!(
        p.albums
            .iter()
            .map(|(_, a)| a.name.as_str())
            .collect::<Vec<_>>(),
        ["New", "Old", "Undated"]
    );
    assert_eq!(
        p.albums[0].0, 2,
        "the flat index still opens the right album"
    );
    assert!(p.top.is_empty(), "nothing played: no MOST PLAYED section");
    assert!(!p
        .rows
        .iter()
        .any(|(_, r)| matches!(r, library::ArtistRowKind::TopSection)));

    for (id, plays) in [(10, 2u32), (31, 9), (20, 5), (30, 1)] {
        lib.stats.insert(
            id,
            TrackStat {
                plays,
                ..Default::default()
            },
        );
    }
    let p = library::artist_page(&lib, "A", true);
    let top: Vec<i64> = p.top.iter().map(|&i| p.tracks[i].song.object_id).collect();
    assert_eq!(top, [31, 20, 10], "three, most played first");
    let (vy, i) = p
        .rows
        .iter()
        .find_map(|(vy, r)| match r {
            library::ArtistRowKind::Top(i) => Some((*vy, *i)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        library::artist_hit(&p, 0, library::artist_content_top() + vy + 4),
        Some(library::ArtistHit::Track(i))
    );
}

/// Preview is opt-in like SensMe: without the component the Menu has no row for it, and the
/// rows below do not shift under the cursor.
#[test]
fn the_preview_menu_row_appears_only_when_the_component_is_installed() {
    let mut a = App::unlocked();
    a.set_sensme_enabled(true);
    // Less Wi-Fi's row too: tests run without the Wi-Fi option.
    assert!(!a.menu_visible().iter().any(|m| m.0 == Screen::Zappin));
    assert_eq!(a.menu_len(), MENU.len() - 2);
    a.set_preview_enabled(true);
    assert!(a.menu_visible().iter().any(|m| m.0 == Screen::Zappin));
    assert_eq!(a.menu_len(), MENU.len() - 1);
    // Turning it off with the cursor on the last row keeps the cursor on a drawn row.
    a.menu_idx = a.menu_len() - 1;
    a.set_preview_enabled(false);
    assert!(a.menu_idx < a.menu_len());
}

#[test]
fn menu_preview_opens_a_page_without_starting_playback() {
    let mut a = App::unlocked();
    a.set_preview_enabled(true);
    a.set_play_context(vec![SongRow::default()], 0);
    a.go(Screen::Menu);
    let row = a
        .menu_visible()
        .iter()
        .position(|m| m.1 == "Preview")
        .unwrap();
    a.menu_idx = row;
    assert!(a.press(Button::Select).is_empty());
    assert_eq!(a.current(), Screen::Zappin);
    assert_eq!(a.tap(350, 420), vec![Action::ZappinLength(15_000)]);
    assert_eq!(a.tap(240, 500), vec![Action::ZappinStart(15_000)]);
}

#[test]
fn menu_entry_returns_to_active_page_and_controls_are_tappable() {
    let mut a = App::unlocked();
    a.set_preview_enabled(true);
    a.set_play_context(vec![SongRow::default()], 0);
    a.go(Screen::Menu);
    let row = a
        .menu_visible()
        .iter()
        .position(|m| m.1 == "Preview")
        .unwrap();
    a.menu_idx = row;
    a.set_zappin_active(true);
    assert!(a.press(Button::Select).is_empty());
    assert_eq!(a.current(), Screen::Zappin);
    assert_eq!(a.tap(240, 590), vec![Action::ZappinFull]);
    assert_eq!(a.tap(240, 670), vec![Action::ZappinEnd]);
}

#[test]
fn play_pause_keeps_its_normal_transport_action_during_preview() {
    let mut a = App::unlocked();
    a.set_play_context(vec![SongRow::default()], 0);
    a.set_zappin_active(true);
    assert_eq!(a.press(Button::Play), vec![Action::PlayPause]);
}

#[test]
fn preview_progress_uses_shell_state_and_full_completion_routes_to_now_playing() {
    let mut a = App::unlocked();
    a.set_play_context(vec![SongRow::default()], 0);
    a.go(Screen::Zappin);
    a.set_zappin_active(true);
    a.set_zappin_progress(2_500, 15_000, 2, 9);
    assert_eq!(a.zappin_elapsed_ms, 2_500);
    assert_eq!(a.zappin_length_ms, 15_000);
    assert_eq!((a.zappin_current, a.zappin_total), (2, 9));
    // This method is called only after the shell confirms ordinary PlayerService playback.
    a.complete_zappin_full_play();
    assert_eq!(a.current(), Screen::NowPlaying);
    assert!(!a.zappin_active);
    assert!(!a.zappin_completed);
}

#[test]
fn active_preview_menu_subtitle_is_short_and_status_only() {
    let _english = crate::i18n::enter(crate::i18n::Language::English);
    let mut a = App::unlocked();
    a.set_play_context(vec![SongRow::default()], 0);
    a.set_zappin_active(true);
    assert_eq!(a.zappin_menu_subtitle(), "Preview active · tap to return");
    a.set_zappin_active(false);
    a.set_zappin_completed(true);
    assert!(a.zappin_menu_subtitle().is_empty());
}

#[test]
fn preview_mode_refusal_uses_the_localized_toast_key() {
    let _chinese = crate::i18n::enter(crate::i18n::Language::SimplifiedChinese);
    let mut a = App::unlocked();
    a.notify("Preview is unavailable in this audio mode");
    assert_eq!(a.toast_for_render(), "当前音频模式无法使用巡听");
}

#[test]
fn preview_is_unavailable_without_current_music_context() {
    let mut a = App::unlocked();
    a.set_preview_enabled(true);
    a.go(Screen::Menu);
    let row = a
        .menu_visible()
        .iter()
        .position(|m| m.1 == "Preview")
        .unwrap();
    a.menu_idx = row;
    assert!(a.press(Button::Select).is_empty());
}

#[test]
fn inactive_preview_rejects_disabled_transport_and_completion_controls() {
    let mut a = App::unlocked();
    a.set_play_context(vec![SongRow::default()], 0);
    a.go(Screen::Zappin);
    for (x, y) in [(240, 590), (240, 670)] {
        assert!(a.tap(x, y).is_empty(), "disabled preview tap at ({x}, {y})");
    }
}

/// The y of the first row Settings ▸ Wi-Fi's hit test gives `want` for, found through the same
/// hit test the finger goes through.
fn wifi_y(on: bool, want: crate::wifi::Hit) -> i32 {
    (0..crate::canvas::H as i32)
        .find(|&y| crate::wifi::hit(240, y, on, want == crate::wifi::Hit::Update) == want)
        .expect("that row is on the page")
}
fn wifi_list_y(networks: usize, want: crate::wifi::ListHit) -> i32 {
    (0..crate::canvas::H as i32)
        .find(|&y| crate::wifi::hit_list(240, y, networks) == want)
        .expect("that row is on the page")
}

/// Settings ▸ Wi-Fi: the switch asks the shell once (a second tap while it works does
/// nothing); Network opens the list and scans; a secured network goes through the password
/// keyboard and back to the list, an open one joins at once, WEP is refused, and the join is
/// handed over exactly once.
#[test]
fn wifi_switch_password_and_join_reach_the_shell_once() {
    use crate::wifi::{Hit, ListHit};
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    a.set_wifi_status("state=off\n", "");
    let y = wifi_y(false, Hit::Toggle);
    assert_eq!(a.tap(240, y), vec![Action::WifiPower(true)]);
    assert!(a.tap(240, y).is_empty(), "busy: no second command");
    a.set_wifi_status("state=on\n", "");
    assert_eq!(
        a.tap(240, wifi_y(true, Hit::Networks)),
        vec![Action::WifiScan]
    );
    assert_eq!(a.current(), Screen::WifiNetworks);
    a.set_wifi_scan("-50\twpa2\t6162\n-60\topen\t6364\n-70\twep\t6566\n");

    let y0 = wifi_list_y(3, ListHit::Network(0));
    assert!(
        a.tap(240, y0).is_empty(),
        "a secured network asks for its password first"
    );
    assert_eq!(a.current(), Screen::Keyboard);
    a.kb_text_for_test("pass word1");
    let done = (0..crate::keyboard::ROWS)
        .flat_map(|r| (0..12).map(move |c| (r, c)))
        .find(|&(r, c)| crate::keyboard::key_at(0, r, c) == Some(crate::keyboard::Key::Done))
        .and_then(|(r, c)| crate::keyboard::key_rect(0, r, c))
        .unwrap();
    assert_eq!(
        a.tap(done.0 + done.2 / 2, done.1 + done.3 / 2),
        vec![Action::WifiJoin]
    );
    assert_eq!(a.current(), Screen::WifiNetworks);
    assert_eq!(
        a.take_wifi_join(),
        Some((b"ab".to_vec(), "pass word1".to_string()))
    );
    assert_eq!(a.take_wifi_join(), None, "the password is handed over once");

    a.set_wifi_status("state=on\n", "");
    let y1 = wifi_list_y(3, ListHit::Network(1));
    assert_eq!(
        a.tap(240, y1),
        vec![Action::WifiJoin],
        "an open network joins at once"
    );
    assert_eq!(a.take_wifi_join(), Some((b"cd".to_vec(), String::new())));
    assert!(a.tap(240, y1).is_empty(), "busy: the list ignores taps");

    a.set_wifi_status("state=on\n", "");
    let y2 = wifi_list_y(3, ListHit::Network(2));
    assert!(a.tap(240, y2).is_empty(), "WEP is refused");
    assert_eq!(a.take_wifi_join(), None);

    a.pop();
    assert_eq!(
        a.tap(240, wifi_y(true, Hit::Toggle)),
        vec![Action::WifiPower(false)]
    );
    a.set_wifi_status("state=off\n", "");
    assert!(a.wifi_networks.is_empty(), "off forgets the list");
}

/// Media servers: opening searches; a server, then a folder, open a level each (taps wait while
/// one loads); a song plays; Back goes up a level and a late answer for the level left is
/// dropped; the strip stops the song, or on the server list searches again.
#[test]
fn media_servers_browse_play_and_back() {
    use crate::media_server::{hit_strip, Entry};
    use crate::wifi::Hit;
    let row_y = |i: i32| {
        (0..crate::canvas::H as i32)
            .find(|&y| crate::media_server::row_at(y, 0, 10) == Some(i as usize))
            .unwrap()
    };
    let strip_y = (0..crate::canvas::H as i32)
        .find(|&y| hit_strip(y))
        .unwrap();
    let e = |t: &str, folder: bool, unplayable: bool| Entry {
        title: t.into(),
        folder,
        unplayable,
        ..Default::default()
    };
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert_eq!(
        a.tap(240, wifi_y(true, Hit::Media)),
        vec![Action::MediaSearch]
    );
    assert_eq!(a.current(), Screen::MediaServers);
    assert!(
        a.tap(240, row_y(0)).is_empty(),
        "nothing to tap while it searches"
    );
    a.media_set_servers(vec![e("NAS", true, false)]);
    assert_eq!(
        a.tap(240, row_y(0)),
        vec![Action::MediaOpen { depth: 0, index: 0 }]
    );
    // A folder: "Search this server" first, so the server's rows are one further down.
    a.media_push(1, "NAS", vec![e("Albums", true, false)], true);
    assert!(a.tap(240, row_y(0)).is_empty());
    assert_eq!(a.current(), Screen::Keyboard);
    a.kb_text = "sigur".into();
    assert_eq!(kb_done(&mut a), vec![Action::MediaFind { depth: 1 }]);
    assert_eq!(
        (a.current(), a.media_query()),
        (Screen::MediaServers, "sigur")
    );
    a.media_push(2, "sigur", vec![], false);
    a.pop();
    assert_eq!(
        a.tap(240, row_y(1)),
        vec![Action::MediaOpen { depth: 1, index: 0 }]
    );
    a.media_push(
        2,
        "Albums",
        vec![e("a", false, false), e("b.m4a", false, true)],
        false,
    );
    assert!(
        a.tap(240, row_y(1)).is_empty(),
        "a format it cannot play: nothing"
    );
    assert_eq!(
        a.tap(240, row_y(0)),
        vec![Action::MediaOpen { depth: 2, index: 0 }]
    );
    a.media_set_playing("a (1/2)");
    assert_eq!(a.tap(240, strip_y), vec![Action::RadioStop]);
    // Back: Albums → NAS; an answer still on its way for a deeper level is dropped.
    a.pop();
    assert_eq!(a.media_levels.len(), 2);
    a.media_push(3, "late", vec![], false);
    assert_eq!(a.media_levels.len(), 2);
    a.pop();
    assert_eq!(a.media_levels.len(), 1);
    assert_eq!(a.current(), Screen::MediaServers, "the server list, still");
    assert_eq!(a.tap(240, strip_y), vec![Action::MediaSearch]);
    a.media_fail("no network");
    a.pop();
    assert_eq!(a.current(), Screen::Wifi);
}

/// Phone remote: only while connected; on and off reach cinder-ffi; the link going takes it down.
/// Speakers: opening the page searches; a speaker tapped plays there; "Play here" brings it
/// back; nothing acts while Wi-Fi is not connected.
#[test]
fn speakers_scan_cast_and_play_here() {
    use crate::wifi::{Hit, SpeakerHit};
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert_eq!(
        a.tap(240, wifi_y(true, Hit::Speakers)),
        vec![Action::SpeakersScan]
    );
    assert_eq!(a.current(), Screen::WifiSpeakers);
    a.set_speakers(vec![
        ("Kitchen".into(), "Sonos One".into()),
        ("Lounge".into(), "Kodi".into()),
    ]);
    let y = |a: &App, h| {
        (0..crate::canvas::H as i32)
            .find(|&y| crate::wifi::hit_speakers(240, y, 2, a.casting()) == h)
            .unwrap()
    };
    let at = y(&a, SpeakerHit::Speaker(1));
    assert_eq!(a.tap(240, at), vec![Action::SpeakerPlay(1)]);
    assert!(a.casting());
    let again = y(&a, SpeakerHit::Speaker(1));
    assert!(
        a.tap(240, again).is_empty(),
        "the speaker playing, tapped again: nothing"
    );
    let here = y(&a, SpeakerHit::Here);
    assert_eq!(a.tap(240, here), vec![Action::SpeakerStop]);
    assert!(!a.casting());
    a.set_wifi_status("state=off\n", "");
    let first = y(&a, SpeakerHit::Speaker(0));
    assert!(
        a.tap(240, first).is_empty(),
        "Wi-Fi off: the list does nothing"
    );
}

/// Internet radio ▸ FIND: the keyboard, then the search; a result tapped is added and played.
#[test]
fn radio_find_searches_and_adds() {
    let mut a = App::unlocked();
    a.push_for_test(Screen::NetRadio);
    let (fx, fy) = (0..crate::canvas::H as i32)
        .find_map(|y| crate::netradio::hit_find(440, y, 0).then_some((440, y)))
        .unwrap();
    a.set_wifi_status("state=off\n", "");
    assert!(a.tap(fx, fy).is_empty());
    assert_eq!(
        a.current(),
        Screen::NetRadio,
        "not connected: FIND does nothing"
    );
    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert!(a.tap(fx, fy).is_empty());
    assert_eq!(a.current(), Screen::Keyboard);
    a.kb_text = "  jazz ".into();
    let done = (0..crate::canvas::H as i32)
        .flat_map(|y| (0..crate::canvas::W as i32).step_by(8).map(move |x| (x, y)))
        .find(|&(x, y)| crate::keyboard::hit(a.kb_page, x, y) == Some(crate::keyboard::Key::Done))
        .unwrap();
    assert_eq!(a.tap(done.0, done.1), vec![Action::RadioFind]);
    assert_eq!((a.current(), a.radio_query()), (Screen::RadioFind, "jazz"));
    let y0 = (0..crate::canvas::H as i32)
        .find(|&y| crate::netradio::hit_found(240, y, 2) == Some(0))
        .unwrap();
    assert!(
        a.tap(240, y0).is_empty(),
        "still searching: results are not there yet"
    );
    a.set_radio_found(
        vec![
            crate::netradio::Found {
                name: "Jazz FM".into(),
                url: "http://j/1".into(),
                sub: "MP3 128".into(),
            },
            crate::netradio::Found {
                name: "KJazz".into(),
                url: "http://k/2".into(),
                sub: "AAC 64".into(),
            },
        ],
        String::new(),
    );
    let y1 = (0..crate::canvas::H as i32)
        .find(|&y| crate::netradio::hit_found(240, y, 2) == Some(1))
        .unwrap();
    assert_eq!(a.tap(240, y1), vec![Action::RadioAdd(1)]);
    a.set_radio_stations("KJazz | http://k/2\n");
    a.play_radio_station(0);
    assert_eq!(a.current(), Screen::NetRadio);
    assert_eq!(a.take_radio(), Some((0, "http://k/2".to_string())));
}

#[test]
fn phone_remote_switch() {
    let mut a = App::unlocked();
    a.push_for_test(Screen::WifiRemote);
    let y = crate::chrome::HEADER_BOTTOM + 5;
    a.set_wifi_status("state=connected\nssid=6162\nip=192.0.2.5\n", "");
    assert_eq!(a.tap(240, y), vec![Action::Remote(true)]);
    assert!(a.opt(Opt::WifiRemote), "saved: it comes back on with Wi-Fi");
    a.set_remote(true, "123456", 8080);
    assert_eq!(a.remote_address(), "http://192.0.2.5:8080");
    a.set_wifi_status("state=off\n", "");
    assert!(
        !a.remote_on() && a.take_remote_dropped(),
        "the link going takes the remote down"
    );
    assert!(!a.take_remote_dropped(), "once");
    a.set_wifi_status("state=connected\nssid=6162\nip=192.0.2.5\n", "");
    a.set_remote(true, "123456", 8080);
    let forget = crate::chrome::HEADER_BOTTOM + crate::kit::ROW_H() + 5;
    assert!(a.tap(240, forget).is_empty(), "no phones, no Forget row");
    a.set_remote_phones(2);
    assert_eq!(a.tap(240, forget), vec![Action::RemoteForget]);
    assert_eq!(a.tap(240, y), vec![Action::Remote(false)]);
    assert!(!a.opt(Opt::WifiRemote) && !a.remote_on());
}

/// The phone's Play replaces the list without the on-player question; its queue buttons put
/// library tracks after the playing one or at the end.
#[test]
fn phone_remote_plays_and_queues() {
    let mut a = App::unlocked();
    let ids: Vec<i64> = a.lib.songs.iter().take(3).map(|s| s.object_id).collect();
    assert_eq!(a.remote_play(ids.clone(), 1), vec![Action::PlayListAt(1)]);
    assert_eq!(a.take_play_list(), ids);
    assert!(a.remote_play(vec![], 0).is_empty());
    assert_eq!(a.remote_queue(&ids[..1], false), vec![Action::QueueChanged]);
    assert_eq!(a.context().len(), 1);
    assert_eq!(a.remote_queue(&ids[1..], true), vec![Action::QueueChanged]);
    assert_eq!(
        a.context()[1].object_id,
        ids[1],
        "next goes straight after the playing one"
    );
    assert!(
        a.remote_queue(&[-999_999], true).is_empty(),
        "unknown ids queue nothing"
    );
}

/// Internet radio: opening it asks for the file, a station plays (its URL handed over once),
/// the playing one stops.
#[test]
fn radio_plays_and_stops() {
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert_eq!(
        a.tap(240, wifi_y(true, crate::wifi::Hit::Radio)),
        vec![Action::RadioOpen]
    );
    assert_eq!(a.current(), Screen::NetRadio);
    a.set_radio_stations("A | http://a.example/1\nB | https://b.example/2\n");
    let y1 = (0..crate::canvas::H as i32)
        .find(|&y| crate::netradio::hit(240, y, 2, 0) == Some(1))
        .unwrap();
    assert_eq!(a.tap(240, y1), vec![Action::RadioPlay]);
    assert_eq!(a.take_radio(), Some((1, "https://b.example/2".to_string())));
    assert_eq!(a.take_radio(), None, "taken once");
    a.set_radio_status("state=playing\nstation=1\ntitle=X - Y\n");
    assert_eq!(
        a.tap(240, y1),
        vec![Action::RadioStop],
        "the playing one stops"
    );
    assert!(a.take_radio().is_none());
}

/// The four options are plain saved switches, off by default. Set the clock, ListenBrainz and
/// Check for updates switched on while connected act at once. Tapping the joined network asks
/// before forgetting it.
#[test]
fn wifi_wps_row_asks_once_and_waits() {
    use crate::wifi::ListHit;
    let mut a = App::unlocked();
    a.push_for_test(Screen::WifiNetworks);
    let wps_y = (0..crate::canvas::H as i32)
        .find(|&y| crate::wifi::hit_list(240, y, 0) == ListHit::Wps)
        .unwrap();
    a.set_wifi_status("state=off\n", "");
    assert!(a.tap(240, wps_y).is_empty(), "off: nothing to press yet");
    a.set_wifi_status("state=on\n", "");
    assert_eq!(a.tap(240, wps_y), vec![Action::WifiWps]);
    assert!(a.tap(240, wps_y).is_empty(), "one wait at a time");
    a.set_wifi_status("state=connected\n", "");
    assert_eq!(a.tap(240, wps_y), vec![Action::WifiWps]);
}

#[test]
fn wifi_options_and_forget() {
    use crate::wifi::{Hit, ListHit};
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    assert!(
        !a.opt(Opt::WifiAutostart)
            && !a.opt(Opt::WifiClock)
            && !a.opt(Opt::WifiLb)
            && !a.opt(Opt::WifiUpdates)
    );
    assert!(
        !a.opt(Opt::WifiLyrics) && !a.opt(Opt::WifiArt),
        "the fetches are off on a fresh install"
    );
    assert!(!a.opt(Opt::WifiAirplay), "AirPlay off on a fresh install");
    assert!(
        !a.opt(Opt::WifiPlayTo),
        "Play to this player off on a fresh install"
    );
    assert!(!a.wifi_drive(), "the shared drive off on a fresh install");
    a.set_wifi_status("state=off\n", "");
    assert_eq!(
        a.tap(240, wifi_y(false, Hit::AirPlay)),
        vec![Action::WifiAirPlay]
    );
    assert!(a.opt(Opt::WifiAirplay));
    a.tap(240, wifi_y(false, Hit::Options));
    assert_eq!(a.current(), Screen::WifiOptions);
    let opt_y = |h| {
        (0..crate::canvas::H as i32)
            .find(|&y| crate::wifi::hit_options(y) == h)
            .unwrap()
    };
    for h in crate::wifi::OPTIONS {
        assert!(a.tap(240, opt_y(h)).is_empty(), "off: {h:?} only saves");
    }
    assert!(
        a.opt(Opt::WifiAutostart)
            && a.opt(Opt::WifiClock)
            && a.opt(Opt::WifiLb)
            && a.opt(Opt::WifiUpdates)
    );
    assert!(
        a.opt(Opt::WifiLyrics) && a.opt(Opt::WifiArt) && a.opt(Opt::WifiPlayTo) && a.wifi_drive()
    );
    for h in crate::wifi::OPTIONS {
        a.tap(240, opt_y(h));
    }
    assert!(
        !a.opt(Opt::WifiAutostart)
            && !a.opt(Opt::WifiClock)
            && !a.opt(Opt::WifiLb)
            && !a.opt(Opt::WifiUpdates)
    );
    assert!(!a.opt(Opt::WifiPlayTo) && !a.wifi_drive());

    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert_eq!(a.tap(240, opt_y(Hit::Clock)), vec![Action::WifiTimeSync]);
    assert_eq!(
        a.tap(240, opt_y(Hit::ListenBrainz)),
        vec![Action::WifiListenBrainz]
    );
    assert_eq!(
        a.tap(240, opt_y(Hit::Updates)),
        vec![Action::WifiUpdateCheck]
    );
    assert!(a.tap(240, opt_y(Hit::AutoStart)).is_empty());
    a.set_wifi_lb_note("Sent 3 songs");
    a.tap(240, opt_y(Hit::ListenBrainz));
    assert!(
        a.wifi_lb_note.is_empty(),
        "switching it off clears its note"
    );

    a.push_for_test(Screen::WifiNetworks);
    a.set_wifi_scan("-50\twpa2\t6162\n");
    assert!(a.tap(240, wifi_list_y(1, ListHit::Network(0))).is_empty());
    assert_eq!(
        a.confirm,
        Some(crate::confirm::Ask::WifiForget),
        "the joined network asks first"
    );
    let yes = (0..crate::canvas::H as i32)
        .find(|&y| {
            crate::confirm::hit(crate::confirm::Ask::WifiForget, 300, y)
                == crate::confirm::Hit::Confirm
        })
        .unwrap();
    assert_eq!(a.tap(300, yes), vec![Action::WifiForget]);
    assert_eq!(a.take_wifi_join(), None, "forgetting is not a join");
}

/// Settings ▸ Wi-Fi ▸ Install Cinder v…: only with a newer release AND the link up, asks
/// first, and one tap starts one install.
#[test]
fn wifi_install_update_asks_then_runs_once() {
    use crate::wifi::Hit;
    let mut a = App::unlocked();
    a.push_for_test(Screen::Wifi);
    a.set_wifi_status("state=connected\nssid=6162\n", "");
    assert!(!a.update_offered(), "no offer, no row");
    a.set_update_offer("v0.4.1");
    assert!(a.update_offered());
    let row = wifi_y(true, Hit::Update);
    // Flat and off the cable: no dialog, a reason instead. On the cable, or charged, it asks.
    a.set_battery_detail(12, "Discharging", "Good", 3500, 0, 0, "");
    assert!(a.tap(240, row).is_empty());
    assert_eq!(
        a.confirm, None,
        "12% on the battery alone does not start an update"
    );
    assert!(a.toast.contains("30%"), "{}", a.toast);
    a.set_battery_detail(12, "Not charging", "Good", 3500, 0, 0, "");
    assert!(
        !a.too_flat_to_update(),
        "a cable in, the charge held by Battery care"
    );
    a.set_battery_detail(64, "Discharging", "Good", 3900, 0, 0, "");
    assert!(a.tap(240, row).is_empty());
    assert_eq!(a.confirm, Some(crate::confirm::Ask::WifiUpdate));
    let yes = (0..crate::canvas::H as i32)
        .find(|&y| {
            crate::confirm::hit(crate::confirm::Ask::WifiUpdate, 300, y)
                == crate::confirm::Hit::Confirm
        })
        .unwrap();
    assert_eq!(a.tap(300, yes), vec![Action::WifiUpdateInstall]);
    assert!(a.tap(240, row).is_empty());
    assert_eq!(a.confirm, None, "running: a second tap does nothing");
    a.set_update_install_note("The download did not match its checksum", true);
    a.tap(240, row);
    assert_eq!(
        a.confirm,
        Some(crate::confirm::Ask::WifiUpdate),
        "failed: can try again"
    );
    a.confirm = None;
    a.set_wifi_status("state=on\n", "");
    assert!(!a.update_offered(), "no link, no row");
}
