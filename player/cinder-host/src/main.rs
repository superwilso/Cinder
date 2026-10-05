//! Host preview backend: render every screen to PNG for device-free iteration.
//!
//! ```text
//! cargo run -p cinder-host              every preview -> out/<name>.png
//! cargo run -p cinder-host -- --check   compare every preview with golden.txt; exit 1 on any change
//! cargo run -p cinder-host -- --bless   record the pixels as they are now as the new golden.txt
//! cargo run -p cinder-host -- --palette FILE.palette
//!                                       check a palette the way the player will, then draw every
//!                                       preview in it -> out/palette_<id>/
//! ```
//!
//! `golden.txt` holds one pixel hash per preview and `cargo test -p cinder-host` checks it, so a
//! change that moves a single pixel on any screen fails until someone has looked at it and blessed
//! it. That is the guard a render-layer refactor needs — "this changed nothing" becomes a test
//! rather than a claim — and the review aid a deliberate UI change needs: the diff of golden.txt
//! names exactly which screens moved, and no others.

use cinder_ui::bluetooth::Bt;
use cinder_ui::library::Tab;
use cinder_ui::menu::MenuItem;
use cinder_ui::sound::Sound;
use cinder_ui::{
    bluetooth, clockset, eq, fm, library, lock, menu, now_playing, pairing, receiver, settings, shelf, sound,
    up_next, usbdac, Canvas, FontSet, Library, H, W,
};

/// Paired devices for the Bluetooth preview — the list is the body of that screen now.
fn preview_paired() -> Vec<cinder_ui::pairing::PairedDevice> {
    [("WH-1000XM5", "HEADPHONES", true), ("CMF Buds Pro 2", "EARBUDS", false),
     ("WONDERBOOM", "SPEAKER", false)]
        .into_iter()
        .map(|(name, kind, connected)| cinder_ui::pairing::PairedDevice {
            name: name.to_string(), kind: kind.to_string(), connected,
        })
        .collect()
}

fn save_png(c: &Canvas, name: &str) {
    let img = image::RgbImage::from_raw(W as u32, H as u32, c.to_rgb_bytes()).expect("buffer size");
    let path = format!("out/{name}.png");
    img.save(&path).expect("save png");
    println!("wrote {path}");
}

/// What a run needs to know beyond "render everything".
struct Opts {
    /// Reproducible output only. Ignores the `CINDER_PREVIEW_T48` real-cover override, which makes
    /// the pixels depend on a file outside the tree.
    golden: bool,
    /// Draw every preview in this palette instead of Cinder's own (`--palette FILE`).
    palette: Option<cinder_ui::palette::Palette>,
}

/// Load a raw NxN RGB thumbnail (the on-device art-cache format) so the host preview can render
/// REAL covers pulled off the device — the only way to check the cover draw path without flashing.
///   CINDER_PREVIEW_T48=<file> CINDER_PREVIEW_T96=<file> cargo run -p cinder-host
fn preview_thumb(var: &str, edge: usize) -> Option<cinder_ui::art::Image> {
    let path = std::env::var(var).ok()?;
    let rgb = std::fs::read(path).ok()?;
    (rgb.len() == edge * edge * 3).then(|| cinder_ui::art::Image { w: edge, h: edge, rgb })
}

/// Render every preview, handing each finished frame to `out` under its name. One list, used by
/// both the PNG writer and the golden check, so the check can never cover a different set of
/// screens from the one people look at.
/// The sample library with a smart playlist on top and one list marked as edited on the player —
/// what `App::rebuild_smart` and the shell's playlist rows produce between them on the device.
fn r4_playlists(lib: &Library) -> Library {
    let mut l = lib.clone();
    if let Some(first) = l.playlists.first_mut() {
        first.user = true;
        first.edited = true;
    }
    let members: Vec<cinder_ui::model::SongRow> = l.songs.iter().take(5).cloned().collect();
    l.playlists.insert(0, cinder_ui::model::PlaylistRow {
        id: cinder_ui::views::smart_id("Late favourites"),
        name: "Late favourites".into(),
        tracks: members.len() as u32,
        art: "Late favourites".into(),
        smart: true,
        rules: "4+ stars \u{b7} Recent \u{b7} FLAC".into(),
        track_list: members,
        ..Default::default()
    });
    l
}

fn render_all(out: &mut dyn FnMut(&str, &Canvas), opts: &Opts) {
    let mut save = |c: &Canvas, name: &str| out(name, c);
    let fonts = FontSet::load();
    // Every theme below comes from these tokens, so `--palette` repaints the whole set, and the
    // default run is Cinder byte for byte (CINDER.theme is what Theme::day_with/night_with return).
    let tokens = opts.palette.as_ref().map_or(cinder_ui::theme::CINDER, |p| p.tokens);
    let th = move |night: bool, a: cinder_ui::Accent| tokens.theme(night, a);
    let amber = cinder_ui::Accent::Amber;
    let pal_name = opts.palette.as_ref().map_or(cinder_ui::palette::BUILTIN_NAME, |p| p.name.as_str());
    let pal_locked = tokens.accent.is_some();
    let new_app = || {
        let mut a = cinder_ui::nav::App::unlocked();
        if let Some(p) = &opts.palette {
            a.set_palettes(vec![p.clone()], Vec::new());
            a.set_palette_wanted(&p.id);
        }
        a
    };
    // Six devices — one and a half pages. Named so the page turn is obvious at a glance.
    let preview_many_paired = vec![
        pairing::PairedDevice { name: "WH-1000XM4".into(), kind: "Headphones".into(), connected: true },
        pairing::PairedDevice { name: "CMF Buds Pro 2".into(), kind: "Headphones".into(), connected: false },
        pairing::PairedDevice { name: "WONDERBOOM".into(), kind: "Speaker".into(), connected: false },
        pairing::PairedDevice { name: "Car audio".into(), kind: "Car".into(), connected: false },
        pairing::PairedDevice { name: "Kitchen speaker".into(), kind: "Speaker".into(), connected: false },
        pairing::PairedDevice { name: "(unnamed)".into(), kind: String::new(), connected: false },
    ];
    let preview_paired_list = preview_paired();

    let np = now_playing::NowPlaying {
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        codec: "FLAC · 24bit / 96.0 kHz",
        badge: "FLAC 24/96",
        clock: "14:32",
        battery: 78,
        elapsed: "1:47",
        remaining: "-2:45",
        progress: 0.39,
        art: "kind",
        art_full: None,
        art_thumb: None,
        liked: true,
        playing: true,
        shuffle: false,
        repeat: 1,
        viz_seed: 2.0,
        viz_kind: 0,
        viz_size: 1, page: 0,
        viz_levels: None,
        viz_peaks: None,
        viz_sig: None,
        scrubbing: false, lyrics: false,
    };
    let lk = lock::Lock {
        clock: "14:32",
        big_clock: "23:41",
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        badge: "FLAC 24/96",
        battery: 78,
        progress: 0.39,
    };

    // The Menu as the device draws it with SensMe installed: ten rows, in `nav::MENU` order,
    // Library tagged HOME (the default home screen).
    let menu_items = [
        MenuItem { label: "Library", sub: "6 albums · 8 tracks", home: true, active: false },
        MenuItem { label: "Folders", sub: "6 folders", home: false, active: false },
        MenuItem { label: "SensMe", sub: "12 channels · 8 tracks", home: false, active: false },
        MenuItem { label: "FM radio", sub: "Needs wired headphones as the aerial", home: false, active: false },
        MenuItem { label: "Sound", sub: "DSEE HX · VPT", home: false, active: false },
        MenuItem { label: "Bluetooth", sub: "LDAC", home: false, active: false },
        MenuItem { label: "USB-DAC", sub: "Off", home: false, active: false },
        MenuItem { label: "Settings", sub: "Display · playback · system", home: false, active: false },
        MenuItem { label: "Help & controls", sub: "Buttons, swipes, the way back", home: false, active: false },
    ];

    let snd = Sound {
        dsee: true,
        balance: cinder_ui::sound::BALANCE_CENTRE, balance_drag: false, bt_route: false,
        mono: false, mono_live: false,
        vinyl: false,
        vpt: "Studio",
        dcphase: "Low A",
        normalizer: true,
        clearaudio: false,
        eq_preset: "A1",
        bt_codec: Some("LDAC"),
        source_direct: false,
        tone_control: false,
        // Bluetooth is live in these previews (the path ends in BT·LDAC), so the Profile row says so.
        profile_map: [0, 0, 1],
        output: cinder_ui::profile::Output::Bluetooth,
    };
    // The Balance row and Advanced are below the fold since the ENHANCE / SPACE / LEVEL grouping.
    let snd_end = sound::max_scroll();
    let bt = Bt { on: true, connected: Some("WH-1000XM5"), link_known: true, codec_sel: 0, ldac_quality: 0, enhanced: true, enhanced_supported: true, connecting: false, busy_phase: 0.0, link_codec: Some(0x02), paired: &preview_paired_list, fine_volume: "OFF", debug_log: false, profile: "B" };
    let eq_bands: [i8; 10] = [2, 3, 1, 0, -1, 0, 2, 3, 2, 1];
    let mut lib = Library::sample();
    // Sample albums all carry album_id 0, so one pulled thumbnail stands in for every row —
    // enough to check placement, scaling and the day/night dim against a real cover.
    if let Some(t48) = (!opts.golden).then(|| preview_thumb("CINDER_PREVIEW_T48", 48)).flatten() {
        for id in 0..8 {
            lib.thumbs.insert(id, t48.clone());
        }
        println!("preview: using real device thumbnails");
    }
    let lib = lib;
    // The Up Next previews: an album playing, and the same album with nine more tracks queued
    // after it (the real list is built by swiping rows in).
    let (album_name, album_tracks): (String, Vec<cinder_ui::model::SongRow>) = lib
        .album_groups
        .first()
        .and_then(|g| g.albums.iter().find(|a| !a.track_list.is_empty()))
        .map(|a| (a.name.clone(), a.track_list.clone()))
        .unwrap_or_default();
    let queued: Vec<cinder_ui::model::SongRow> = album_tracks
        .iter()
        .cloned()
        .chain(lib.songs.iter().take(9).cloned())
        .collect();

    for (name, theme) in [("day", th(false, amber)), ("night", th(true, amber))] {
        let render_set: &[(&str, &dyn Fn(&mut Canvas))] = &[
            ("now_playing", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts, &np)),
            ("now_playing_sleep", &|c: &mut Canvas| { now_playing::render(c, &theme, &fonts, &np); now_playing::sleep_badge(c, &theme, &fonts, 23, false); }),
            // The Lyrics chip (community B2), beside the sleep badge so both corners are checked.
            ("now_playing_lyrics", &|c: &mut Canvas| {
                now_playing::render(c, &theme, &fonts, &now_playing::NowPlaying { lyrics: true, ..np });
                now_playing::sleep_badge(c, &theme, &fonts, 23, false);
            }),
            // Nothing loaded — the state the device actually boots into. Never rendered here
            // before, which is how an empty codec badge shipped as a bare stroked box.
            ("now_playing_idle", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts,
                &now_playing::NowPlaying { title: "", artist: "", codec: "", badge: "", elapsed: "",
                                           remaining: "", progress: 0.0, playing: false, liked: false,
                                           art: "", viz_size: 0, page: 0, ..np })),
            // The design styles (`cinder_ui::style`): each one playing, with the Lyrics chip and the
            // sleep badge in its corners, on the spectrum page, and with nothing loaded.
            ("now_playing_nocturne", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Nocturne, ..theme };
                now_playing::render(c, &t, &fonts, &np);
            }),
            ("now_playing_nocturne_lyrics", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Nocturne, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { lyrics: true, liked: true, repeat: 3, ..np });
                now_playing::sleep_badge(c, &t, &fonts, 23, false);
            }),
            ("now_playing_nocturne_spectrum", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Nocturne, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { page: 1, ..np });
            }),
            ("now_playing_nocturne_idle", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Nocturne, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { title: "", artist: "", codec: "",
                    badge: "", elapsed: "", remaining: "", progress: 0.0, playing: false, liked: false, art: "",
                    viz_size: 0, page: 0, ..np });
            }),
            ("now_playing_terminal", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Terminal, ..theme };
                now_playing::render(c, &t, &fonts, &np);
            }),
            ("now_playing_terminal_lyrics", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Terminal, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { lyrics: true, liked: true, shuffle: true, repeat: 1, ..np });
                now_playing::sleep_badge(c, &t, &fonts, 23, false);
            }),
            ("now_playing_terminal_level", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Terminal, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { page: 2, ..np });
            }),
            ("now_playing_terminal_idle", &|c: &mut Canvas| {
                let t = cinder_ui::Theme { style: cinder_ui::style::Style::Terminal, ..theme };
                now_playing::render(c, &t, &fonts, &now_playing::NowPlaying { title: "", artist: "", codec: "",
                    badge: "", elapsed: "", remaining: "", progress: 0.0, playing: false, liked: false, art: "",
                    viz_size: 0, page: 0, ..np });
            }),
            // One entry per page, and the NAMES track the pages. These were hand-numbered and went
            // stale the moment a page was inserted: "onboard_2_features" was rendering the new
            // Gestures page under the old name, so the preview said the sweep was fine.
            ("onboard_0_welcome", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 0)),
            ("onboard_1_getting_around", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 1)),
            ("onboard_2_buttons", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 2)),
            ("onboard_3_playing", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 3)),
            ("onboard_4_gestures", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 4)),
            ("onboard_5_features", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 5)),
            ("onboard_6_done", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 6)),
            ("shelf", &|c: &mut Canvas| {
                now_playing::render(c, &theme, &fonts, &np);
                shelf::render(c, &theme, &fonts, "Now Playing · Atlas Hands", "1:47 / 4:32",
                    &std::array::from_fn(|i| match i {
                        0 => Some(shelf::Pin { title: "Library · Albums", sub: "Saved 2 min ago" }),
                        1 => Some(shelf::Pin { title: "Nick Drake · Pink Moon", sub: "Saved yesterday" }),
                        _ => None,
                    }));
            }),
            // The pull-down panel (Settings ▸ Pull-down panel), over the Library it was pulled from.
            ("quick_panel", &|c: &mut Canvas| {
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &lib, None, false, 0, false);
                cinder_ui::chrome::status_bar(c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
                cinder_ui::quick::render(c, &theme, &fonts, &cinder_ui::quick::QuickView {
                    brightness: 4, bt_on: true, bt_device: Some("WH-1000XM5"), night: false, sleep_idx: 2,
                });
            }),
            ("lock", &|c: &mut Canvas| lock::render(c, &theme, &fonts, &lk)),
            ("menu", &|c: &mut Canvas| menu::render(c, &theme, &fonts,
                "NOW › Atlas Hands · Benjamin Francis Leftwich · 1:47", &menu_items, 0)),
            // Up Next: history above the playing track, then the rest of the album. The history
            // is its OWN list, so the preview passes one explicitly.
            ("up_next", &|c: &mut Canvas| {
                let tracks = &album_tracks[..];
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: &album_name, tracks,
                    current: (!tracks.is_empty()).then(|| 2.min(tracks.len() - 1)),
                    history: &tracks[..2.min(tracks.len())],
                    lib: &lib, scroll_px: 0,
                    drag: None, swipe: None, sbar_active: false,
                });
            }),
            // The same list after queueing more after the album: ONE list, headed NEXT UP, in the
            // order it will play. Scrolled so the join between the two is on screen.
            ("up_next_queue", &|c: &mut Canvas| {
                let cur = album_tracks.len().saturating_sub(2);
                let l = up_next::layout(0, queued.len(), (!queued.is_empty()).then_some(cur));
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &queued, current: (!queued.is_empty()).then_some(cur),
                    history: &[], lib: &lib, scroll_px: l.follow_scroll(),
                    drag: None, swipe: None, sbar_active: false,
                });
            }),
            // Mid-reorder: the gesture the device can't be screenshotted through — the row is
            // lifted under a finger that isn't there.
            ("up_next_reorder", &|c: &mut Canvas| {
                let cur = (!queued.is_empty()).then_some(0);
                let l = up_next::layout(0, queued.len(), cur);
                let from = 1usize;
                let grab_off = up_next::RH / 2;
                // The row's screen y comes from the layout — the same rule nav's reorder_begin
                // follows.
                let row_top = cinder_ui::chrome::HEADER_BOTTOM
                    + l.movable_top(from).unwrap_or(0);
                let start_y = row_top + grab_off;
                let y = start_y + 2 * up_next::RH + 14;   // dragged down past two rows
                let d = up_next::RowDrag {
                    from,
                    to: l.movable_slot_for(from, y - grab_off, 0),
                    start_y,
                    y,
                    grab_off,
                };
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &queued, current: cur,
                    history: &[], lib: &lib, scroll_px: 0,
                    drag: Some(d), swipe: None, sbar_active: false,
                });
            }),
            ("playlist_page", &|c: &mut Canvas| {
                match lib.playlists.first() {
                    Some(pl) => library::playlist_view(c, &theme, &fonts, &lib, pl, 0, 0, None, false, None),
                    None => {}
                }
            }),
            // The playlists you MADE: the same page, plus its edit bar and a row armed for
            // removal — the state that is easiest to get wrong and hardest to see in a test.
            ("sound_source_direct", &|c: &mut Canvas| {
                let s = Sound { source_direct: true, ..snd };
                sound::render(c, &theme, &fonts, &s, 0, 0, 0);
            }),
            ("sound_tone_control", &|c: &mut Canvas| {
                let s = Sound { tone_control: true, ..snd };
                sound::render(c, &theme, &fonts, &s, 0, 0, 0);
            }),
            ("playlist_page_own", &|c: &mut Canvas| {
                if let Some(pl) = lib.playlists.first() {
                    let mine = cinder_ui::model::PlaylistRow {
                        user: true, name: "Late Night On The Bus".into(), ..pl.clone()
                    };
                    library::playlist_view(c, &theme, &fonts, &lib, &mine, 0, 1, None, false, Some(1));
                }
            }),
            // The Playlists tab, which is where a playlist gets made.
            ("library_playlists_own", &|c: &mut Canvas| {
                let mut mine = lib.clone();
                if let Some(first) = mine.playlists.first_mut() {
                    first.user = true;
                }
                library::render(c, &theme, &fonts, Tab::Playlists, 0, 0, 0, 0, None, &mine, None, false, 0, false);
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands",
                                          "Benjamin Francis Leftwich", true, 0.39);
            }),
            ("keyboard", &|c: &mut Canvas| {
                cinder_ui::keyboard::render(c, &theme, &fonts, "New playlist",
                                            "Late night on the bus", "Playlist name", 0, true);
            }),
            ("keyboard_symbols", &|c: &mut Canvas| {
                cinder_ui::keyboard::render(c, &theme, &fonts, "Rename playlist",
                                            "2 a.m. mix #3", "Playlist name", 1, false);
            }),
            ("playlist_pick", &|c: &mut Canvas| {
                let targets = [
                    cinder_ui::playlist_pick::Target { name: "Late Night On The Bus", tracks: 42 },
                    cinder_ui::playlist_pick::Target { name: "Sunday", tracks: 9 },
                ];
                cinder_ui::playlist_pick::render_targets(c, &theme, &fonts, "Add to playlist",
                                                         "Atlas Hands", &targets, 3, 0, false);
            }),
            ("track_pick", &|c: &mut Canvas| {
                let songs: Vec<&cinder_ui::model::SongRow> = lib.songs.iter().collect();
                let n = songs.len();
                cinder_ui::playlist_pick::render_tracks(c, &theme, &fonts, "Late Night On The Bus",
                                                        &songs, &|i| i % 3 == 0, 0, 4, "", n, false);
            }),
            // The same screen with a search running — the state that makes it usable on a library
            // of thousands, and the one worth eyeballing.
            ("track_pick_search", &|c: &mut Canvas| {
                let songs: Vec<&cinder_ui::model::SongRow> =
                    lib.songs.iter().filter(|s| s.title.to_lowercase().contains('a')).collect();
                cinder_ui::playlist_pick::render_tracks(c, &theme, &fonts, "Late Night On The Bus",
                                                        &songs, &|i| i % 3 == 0, 0, 4, "a",
                                                        lib.songs.len(), false);
            }),
            ("up_next_remove", &|c: &mut Canvas| {
                let cur = (!queued.is_empty()).then_some(0);
                let l = up_next::layout(0, queued.len(), cur);
                let row_y = cinder_ui::chrome::HEADER_BOTTOM
                    + l.top_of(up_next::Slot::Upcoming(3)).unwrap_or(0) + up_next::RH / 2;
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &queued, current: cur,
                    history: &[], lib: &lib, scroll_px: 0,
                    drag: None,
                    swipe: Some(cinder_ui::library::SwipeRow { y: row_y, dx: 110 }),
                    sbar_active: false,
                });
            }),
            ("library_songs", &|c: &mut Canvas| {
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &lib, None, false, 0, false);
                // nav draws the Now Playing return bar over the library screens; mirror that here
                // so the preview shows the real bottom of the screen, not a list running to the edge.
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands", "Benjamin Francis Leftwich", true, 0.39);
            }),
            // Songs sorted by ADDED (sort chip index 4) — shows the SORT chip label + reorder.
            ("library_songs_added", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 4, 0, None, &lib, None, false, 0, false)),
            // The shuffle band mid-slide and fully away (library::band_slide) — for looking at the
            // seam against the tab strip, which no test can judge.
            ("library_songs_band_half", &|c: &mut Canvas| {
                let mut big = lib.clone();
                let base = big.songs.clone();
                for n in 1..20 {
                    big.songs.extend(base.iter().map(|s| cinder_ui::model::SongRow { object_id: s.object_id + n * 100_000, ..s.clone() }));
                }
                library::render(c, &theme, &fonts, Tab::Songs, 0, 300, 0, 0, None, &big, None, false, 36, false);
            }),
            ("library_songs_band_hidden", &|c: &mut Canvas| {
                let mut big = lib.clone();
                let base = big.songs.clone();
                for n in 1..20 {
                    big.songs.extend(base.iter().map(|s| cinder_ui::model::SongRow { object_id: s.object_id + n * 100_000, ..s.clone() }));
                }
                library::render(c, &theme, &fonts, Tab::Songs, 0, 300, 0, 0, None, &big, None, false, 999, false);
            }),
            ("library_albums", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, None, &lib, None, false, 0, false)),
            // Albums with the first album's accordion expanded (tracks listed inline).
            ("library_albums_expanded", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, Some(0), &lib, None, false, 0, false)),
            // Albums flat-ordered A-Z (ORDER chip index 1 — no artist headers).
            ("library_albums_az", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 1, None, &lib, None, false, 0, false)),
            ("library_artists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Artists, 0, 0, 0, 0, None, &lib, None, false, 0, false)),
            ("library_playlists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Playlists, 0, 0, 0, 0, None, &lib, None, false, 0, false)),
            // NEW PLAYLIST gone with the band (library::band_slide(Playlists)): the rows meet the tabs.
            ("library_playlists_band_hidden", &|c: &mut Canvas| {
                let mut big = lib.clone();
                let base = big.playlists.clone();
                for _ in 0..8 {
                    big.playlists.extend(base.iter().cloned());
                }
                library::render(c, &theme, &fonts, Tab::Playlists, 0, 300, 0, 0, None, &big, None, false, 999, false);
            }),
            // The artist drill-in, built from the SAMPLE LIBRARY like every other list preview —
            // it used to render three hard-coded albums from `data::ARTIST_*` regardless of who
            // the artist was, which is precisely why nothing ever pushed it.
            ("artist", &|c: &mut Canvas| {
                let name = lib.artists.first().map(|a| a.name.as_str()).unwrap_or("");
                let page = library::artist_page(&lib, name, false);
                library::artist_view(c, &theme, &fonts, &lib, &page, 0, 0, None, false);
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands", "Benjamin Francis Leftwich", true, 0.39);
            }),
            // …and with SONGS opened from its header (2026-10-04: folded away until asked for).
            ("artist_songs_open", &|c: &mut Canvas| {
                let name = lib.artists.first().map(|a| a.name.as_str()).unwrap_or("");
                let page = library::artist_page(&lib, name, true);
                library::artist_view(c, &theme, &fonts, &lib, &page, 0, 0, None, false);
            }),
            // ── Redesign R4: ratings, plays, smart playlists, the two editors ──────────────────
            // The album page with its rating in the header's right slot (handoff 5h).
            ("album_rated", &|c: &mut Canvas| {
                if let Some(al) = lib.albums_flat().first() {
                    library::album_view(c, &theme, &fonts, al, 1, 0, None, None, false, Some(4));
                }
            }),
            // The artist page once things have been rated and played: albums newest first with
            // their ratings, then MOST PLAYED, then every song (handoff 5m).
            ("artist_played", &|c: &mut Canvas| {
                let mut l = lib.clone();
                for (id, rating, plays) in [(0i64, 5u8, 14u32), (1, 4, 9), (2, 0, 3), (3, 3, 1)] {
                    l.stats.insert(id, cinder_ui::model::TrackStat { rating, plays, last_played: 1_790_000_000 + id });
                }
                let name = l.artists.first().map(|a| a.name.clone()).unwrap_or_default();
                let page = library::artist_page(&l, &name, false);
                library::artist_view(c, &theme, &fonts, &l, &page, 0, 0, None, false);
            }),
            // The Playlists tab with a smart playlist above the others and an EDITED tag (5g).
            ("library_playlists_smart", &|c: &mut Canvas| {
                let l = r4_playlists(&lib);
                library::render(c, &theme, &fonts, Tab::Playlists, 1, 0, 0, 0, None, &l, None, false, 0, false);
            }),
            ("playlist_page_smart", &|c: &mut Canvas| {
                let l = r4_playlists(&lib);
                library::playlist_view(c, &theme, &fonts, &l, &l.playlists[0], 0, 0, None, false, None);
            }),
            // The playlist editor (5b): at rest with something to undo, and mid-drag.
            ("playlist_edit", &|c: &mut Canvas| {
                cinder_ui::playlist_edit::render(c, &theme, &fonts, &cinder_ui::playlist_edit::EditView {
                    name: "Late Night On The Bus", rows: lib.songs.iter().collect(), scroll_px: 0,
                    drag: None, can_undo: true, sbar_active: false,
                });
            }),
            ("playlist_edit_drag", &|c: &mut Canvas| {
                use cinder_ui::playlist_edit as pe;
                let n = lib.songs.len();
                let (from, grab_off) = (1usize, pe::RH / 2);
                let start_y = pe::row_top(from, 0) + grab_off;
                let y = start_y + 2 * pe::RH + 10;
                let d = up_next::RowDrag { from, to: pe::slot_for(n, y - grab_off, 0), start_y, y, grab_off };
                pe::render(c, &theme, &fonts, &pe::EditView {
                    name: "Late Night On The Bus", rows: lib.songs.iter().collect(), scroll_px: 0,
                    drag: Some(d), can_undo: false, sbar_active: false,
                });
            }),
            // The saved-view editor (5c): one that exists, and a new one with nothing set.
            ("view_edit", &|c: &mut Canvas| {
                let v = cinder_ui::views::SavedView {
                    name: "Late favourites".into(), min_rating: 4,
                    played: cinder_ui::views::Played::Recent, format: cinder_ui::views::FormatRule::Flac,
                    sort: cinder_ui::views::ViewSort::Plays,
                    shuffle: Some(cinder_ui::shuffle::ShuffleBy::Albums),
                };
                cinder_ui::view_edit::render(c, &theme, &fonts,
                    &cinder_ui::view_edit::ViewEditView { draft: &v, matches: 38, existing: true });
            }),
            ("view_edit_new", &|c: &mut Canvas| {
                let v = cinder_ui::views::SavedView::default();
                cinder_ui::view_edit::render(c, &theme, &fonts,
                    &cinder_ui::view_edit::ViewEditView { draft: &v, matches: lib.songs.len(), existing: false });
            }),
            ("eq", &|c: &mut Canvas| eq::render(c, &theme, &fonts, &eq_bands, "A1", 4, None)),
            ("eq_off", &|c: &mut Canvas| eq::render(c, &theme, &fonts, &eq_bands, "A1", 4, Some("Off: Tone Control is on"))),
            ("sound", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 0, 0, 0)),
            ("sound_setup_b", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 5, 1, 0)),
            // The balance slider off-centre and mid-drag: the two states the static preview above
            // never shows, and the ones where the knob can drift off its hit band.
            ("clockset", &|c: &mut Canvas| {
                clockset::render(c, &theme, &fonts, &[2026, 8, 17, 9, 1], clockset::F_MONTH)
            }),
            ("sound_balance", &|c: &mut Canvas| {
                let s = Sound { balance: 14, balance_drag: true, ..snd };
                sound::render(c, &theme, &fonts, &s, sound::ROW_BALANCE, 0, snd_end)
            }),
            // MONO, in both the states it has — and this is the LONGEST subtitle on the screen,
            // which is exactly why it is rendered rather than reasoned about. The 2026-09-06 audit
            // found three unreachable screens by adding them to this matrix; a state that is never
            // drawn is a state whose overflow nobody has checked.
            ("sound_mono", &|c: &mut Canvas| {
                // Off the live path: the honest case for ordinary playback on this device, and the
                // one whose subtitle has to fit. See analysis/RE_mono_audio.md.
                let s = Sound { balance: 14, mono: true, mono_live: false, ..snd };
                sound::render(c, &theme, &fonts, &s, sound::ROW_BALANCE, 0, snd_end)
            }),
            ("sound_mono_live", &|c: &mut Canvas| {
                let s = Sound { mono: true, mono_live: true, ..snd };
                sound::render(c, &theme, &fonts, &s, sound::ROW_BALANCE, 0, snd_end)
            }),
            ("settings", &|c: &mut Canvas| settings::render(c, &theme, &fonts, 1, 0,
                &settings::SettingsView { more: false, shuffle_by: "SONGS", ignore_the: false, quick: false, volume_limit: false, usb_dac: false, battery_care: true, device: "99% · 34.4 °C",
                    database: "3,424 tracks", storage: "12.4 / 58 GB", sleep: "30 MIN", brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", bt_idle_off: false, boot_stock: "SONY", clock: "17 Aug · 09:01" })),
            // Settings ▸ Display (handoff 5k): palette, accent, night, the volume readout, size.
            ("display", &|c: &mut Canvas| cinder_ui::display::render(c, &theme, &fonts, 1,
                &cinder_ui::display::DisplayView { palette: pal_name, accent_locked: pal_locked,
                    accent: cinder_ui::Accent::Amber, night: theme.night, volume_hud: 1, viz: "BARS · VEIL", style: cinder_ui::style::Style::Cinder })),
            // The genre FILTER, both halves: the picker, and what a filtered Songs list looks like.
            // The shuffle band's caption has to follow the filter — shuffling a filtered list
            // shuffles what is on screen, so it must not still promise the whole library.
            ("genre_picker", &|c: &mut Canvas| {
                library::genre_render(c, &theme, &fonts, &lib, 0, false)
            }),
            ("library_songs_filtered", &|c: &mut Canvas| {
                let mut l = lib.clone();
                l.filter_genre = l.genres.first().map(|g| g.id);
                library::render(c, &theme, &fonts, library::Tab::Songs, 0, 0, 0, 0, None, &l, None, false, 0, false);
                let az = library::az_present(library::Tab::Songs, &l, 0, 0);
                library::az_render(c, &theme, &fonts, library::Tab::Songs, &az, 0, 0);
            }),
            // Track information: a long path is the case that decides the layout, so the preview
            // uses one rather than a tidy short filename.
            // Folder browse: the root (two subdirectories) and one directory of tracks, because
            // the two row kinds are what the layout has to keep apart.
            ("folders_root", &|c: &mut Canvas| {
                cinder_ui::folders::render(c, &theme, &fonts, &lib, Some(0), 0, false)
            }),
            ("folders_dir", &|c: &mut Canvas| {
                cinder_ui::folders::render(c, &theme, &fonts, &lib, Some(1), 0, false)
            }),
            // SensMe: the channel list, and one channel open. Both, because the two levels have
            // different geometry — the channel page gives its first 80 px to the PLAY | SHUFFLE
            // band — and the golden gate is the only thing that watches that.
            ("sensme_channels", &|c: &mut Canvas| {
                cinder_ui::sensme::render(c, &theme, &fonts, &lib, None, 0, false, Default::default())
            }),
            // The whole grid (handoff 5d): all thirteen channels populated, following the time of
            // day at 19:00 (Night), so the NOW tile and the "Play Night" button are drawn.
            ("sensme_grid_follow", &|c: &mut Canvas| {
                const NAMES: [&str; 13] = ["Active", "Emotional", "Lounge", "Dance", "Extreme", "Upbeat",
                    "Relax", "Mellow", "Morning", "Daytime", "Evening", "Night", "Midnight"];
                let mut l = lib.clone();
                l.channels = NAMES.iter().enumerate().map(|(id, name)| cinder_ui::model::ChannelRow {
                    id: id as u8,
                    name,
                    tracks: (0..(40 + (id as u32 * 37) % 120)).collect(),
                }).collect();
                cinder_ui::sensme::render(c, &theme, &fonts, &l, None, 0, false,
                    cinder_ui::sensme::Foot { follow: true, hour: Some(19) })
            }),
            ("sensme_channel", &|c: &mut Canvas| {
                cinder_ui::sensme::render(c, &theme, &fonts, &lib, Some(0), 0, false, Default::default())
            }),
            // …and the state every library starts in: nothing analysed, which is a screen that has
            // to explain itself rather than be blank.
            ("sensme_empty", &|c: &mut Canvas| {
                let mut l = lib.clone();
                l.channels.clear();
                l.sensme_tracks = 0;
                cinder_ui::sensme::render(c, &theme, &fonts, &l, None, 0, false, Default::default())
            }),
            ("track_info", &|c: &mut Canvas| {
                let rows: Vec<(String, String)> = vec![
                    ("Rating".into(), "-".into()),
                    ("Title".into(), "Atlas Hands".into()),
                    ("Artist".into(), "Benjamin Francis Leftwich".into()),
                    ("Album".into(), "Last Smoke Before the Snowstorm".into()),
                    ("Genre".into(), "Alternative".into()),
                    ("Year".into(), "2011".into()),
                    ("Track".into(), "3".into()),
                    ("Duration".into(), "4:32".into()),
                    ("Format".into(), "FLAC · Hi-Res".into()),
                    ("Size".into(), "48.2 MB".into()),
                    ("File".into(), "/contents/Music/Benjamin Francis Leftwich/Last Smoke Before the Snowstorm/03 Atlas Hands.flac".into()),
                ];
                cinder_ui::track_info::render(c, &theme, &fonts, &rows, 0, false, 4)
            }),
            ("lyrics", &|c: &mut Canvas| {
                use cinder_ui::lyrics::{Line, Lyrics};
                // Written for this preview. A blank line is an instrumental gap; line 4 is current.
                let words = [
                    "The kettle's on, the window's grey",
                    "another Sunday slips away",
                    "",
                    "We said we'd walk down to the sea",
                    "and read the tide out loud like it was poetry, the whole way down the harbour wall",
                    "but the rain came in at half past three",
                    "so we stayed in",
                    "",
                    "Hold the line, hold the light",
                    "leave the radio on tonight",
                    "Hold the line, hold the light",
                    "we'll be fine",
                ];
                let lyr = Lyrics {
                    lines: words
                        .iter()
                        .enumerate()
                        .map(|(i, w)| Line { at_ms: Some(i as u32 * 4000), text: w.to_string() })
                        .collect(),
                };
                cinder_ui::lyrics::render(c, &theme, &fonts, Some(&lyr), Some(4), 0, false)
            }),
            ("search", &|c: &mut Canvas| {
                let q = "st";
                let songs: Vec<&cinder_ui::model::SongRow> = lib
                    .songs
                    .iter()
                    .filter(|s| s.title.to_lowercase().contains(q) || s.artist.to_lowercase().contains(q))
                    .collect();
                cinder_ui::search::render(c, &theme, &fonts, &songs, q, lib.songs.len(), 0, false)
            }),
            ("library_search_button", &|c: &mut Canvas| {
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &lib, None, false, 0, true)
            }),
            ("bluetooth", &|c: &mut Canvas| bluetooth::render(c, &theme, &fonts, &bt)),
            ("bluetooth_codec", &|c: &mut Canvas| bluetooth::render_codec(c, &theme, &fonts, &bt)),
            // The radio off: the page is inert, so every row is faint and nothing is marked chosen.
            ("bluetooth_codec_off", &|c: &mut Canvas| {
                bluetooth::render_codec(c, &theme, &fonts, &bluetooth::Bt { on: false, ..bt })
            }),
            // The in-flight state this screen had no representation for at all: before, a connect
            // begun from Devices left this card reading "No device connected" until the link
            // resolved, which is what a failure looks like.
            ("bluetooth_connecting", &|c: &mut Canvas| {
                let b = Bt { paired: &preview_paired_list, on: true, connected: None, link_known: true, codec_sel: 0,
                             ldac_quality: 0, enhanced: true, enhanced_supported: true,
                             connecting: true, busy_phase: 0.35, link_codec: None,
                             fine_volume: "OFF", debug_log: false, profile: "B" };
                bluetooth::render(c, &theme, &fonts, &b)
            }),
            // Two real pairings from the device (the same two the 07-29 GetPairedDeviceInfo pass
            // read back), one connected, one with FORGET armed — the preview covers both row states.
            ("pairing", &|c: &mut Canvas| {
                let paired = vec![
                    pairing::PairedDevice { name: "WH-1000XM4".into(), kind: "Headphones".into(), connected: true },
                    pairing::PairedDevice { name: "CMF Buds Pro 2".into(), kind: "Headphones".into(), connected: false },
                ];
                let found = vec![
                    pairing::PairedDevice { name: "Pixel 8".into(), kind: "Phone".into(), connected: false },
                    pairing::PairedDevice { name: "(unnamed)".into(), kind: String::new(), connected: false },
                ];
                pairing::render(c, &theme, &fonts, &paired, &found, Some(1), None, true, 0.35, 0)
            }),
            // MORE PAIRED DEVICES THAN ONE PAGE HOLDS. Previewed because the truncation this
            // replaced was invisible in every other preview: four rows and a header saying six is
            // exactly what the bug looked like, and it was on screen for months. Page 0 here, page
            // 1 below — the pair proves the window moves and that the last page is short, not
            // padded.
            ("pairing_page1", &|c: &mut Canvas| {
                pairing::render(c, &theme, &fonts, &preview_many_paired, &[], None, None, false, 0.0, 0)
            }),
            ("pairing_page2", &|c: &mut Canvas| {
                pairing::render(c, &theme, &fonts, &preview_many_paired, &[], None, None, false, 0.0, 1)
            }),
            // A connect attempt IN FLIGHT on the second paired row: "CONNECTING…" plus the moving
            // spinner. Previewed on its own because the state is transient on device — it is the
            // one screen you cannot hold still long enough to eyeball, and it is exactly where a
            // silent failure would otherwise look identical to success.
            ("pairing_connecting", &|c: &mut Canvas| {
                let paired = vec![
                    pairing::PairedDevice { name: "WH-1000XM4".into(), kind: "Headphones".into(), connected: false },
                    pairing::PairedDevice { name: "CMF Buds Pro 2".into(), kind: "Headphones".into(), connected: false },
                ];
                pairing::render(c, &theme, &fonts, &paired, &[], None, Some(1), false, 0.35, 0)
            }),
            // The modal pairing prompt over the list — the numeric-comparison case, which is what a
            // phone or a modern pair of headphones actually asks for.
            ("pairing_prompt", &|c: &mut Canvas| {
                let paired = vec![
                    pairing::PairedDevice { name: "WH-1000XM4".into(), kind: "Headphones".into(), connected: true },
                ];
                let found = vec![
                    pairing::PairedDevice { name: "Pixel 8".into(), kind: "Phone".into(), connected: false },
                ];
                pairing::render(c, &theme, &fonts, &paired, &found, None, None, false, 0.0, 0);
                pairing::render_prompt(c, &theme, &fonts,
                    &pairing::Prompt { kind: pairing::PROMPT_NUMERIC, name: "Pixel 8".into(), code: 428913 });
            }),
            ("receiver", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, &receiver::Rx {
                on: false, phase: 0, peer: "", codec: 0, freq: 0, bitrate: 0, radio: true,
            })),
            ("receiver_waiting", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, &receiver::Rx {
                on: true, phase: 1, peer: "", codec: 0, freq: 0, bitrate: 0, radio: true,
            })),
            ("receiver_playing", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, &receiver::Rx {
                on: true, phase: 3, peer: "ARTHURS-PC", codec: 2, freq: 3, bitrate: 990, radio: true,
            })),
            ("receiver_code", &|c: &mut Canvas| {
                receiver::render(c, &theme, &fonts, &receiver::Rx {
                    on: true, phase: 1, peer: "", codec: 0, freq: 0, bitrate: 0, radio: true,
                });
                pairing::render_prompt(c, &theme, &fonts,
                    &pairing::Prompt { kind: pairing::PROMPT_NUMERIC, name: "ARTHURS-PC".into(), code: 114363 });
            }),
            ("fm", &|c: &mut Canvas| fm::render(c, &theme, &fonts, &fm::Fm {
                khz: 97300, playing: true,
                stations: [97300, 100000, 107800, 0, 0, 0], n_stations: 3,
                scanning: false, scan_pct: 0, antenna: true, bt_out: false,
                // Preview at a real reading: 12 of a measured 15-count full scale, stereo locked.
                signal: 12, hw: true, stereo: true,
            })),
            ("usbdac", &|c: &mut Canvas| usbdac::render(c, &theme, &fonts, true, true, "LDAC", Some("WH-1000XM5"), "A1", true, Some((44100, 32, 2)), None)),
            // Library views (the header's view button). Last in the set so the glyph cache order —
            // and so every preview above — is unchanged by their arrival.
            ("library_albums_grid", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Albums, library::LibView::Grid);
                library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, None, &l, None, false, 0, false)
            }),
            ("library_albums_grid_az", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Albums, library::LibView::Grid);
                library::render(c, &theme, &fonts, Tab::Albums, 99, 0, 0, 1, None, &l, None, false, 0, true)
            }),
            ("library_albums_compact", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Albums, library::LibView::Compact);
                library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 1, None, &l, None, false, 0, false)
            }),
            ("library_songs_compact", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Songs, library::LibView::Compact);
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &l, None, false, 0, false)
            }),
            ("library_artists_grid", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Artists, library::LibView::Grid);
                library::render(c, &theme, &fonts, Tab::Artists, 0, 0, 0, 0, None, &l, None, false, 0, false)
            }),
            ("library_artists_compact", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Artists, library::LibView::Compact);
                library::render(c, &theme, &fonts, Tab::Artists, 0, 0, 0, 0, None, &l, None, false, 0, false)
            }),
            ("library_playlists_grid", &|c: &mut Canvas| {
                let l = with_view(&lib, Tab::Playlists, library::LibView::Grid);
                library::render(c, &theme, &fonts, Tab::Playlists, 0, 0, 0, 0, None, &l, None, false, 0, false)
            }),
        ];
        for (screen, draw) in render_set {
            let mut c = Canvas::new();
            draw(&mut c);
            // The status strip is drawn by the NAVIGATOR on device (one place, live values), not by
            // each screen — so these direct screen calls have to add it or every preview would be
            // missing the chrome the real thing has, and UI work would be done against a lie.
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, &format!("{screen}_{name}"));
        }
    }

    // Visualiser sweeps: the same frame, the same cover, the SAME spectrum data every time —
    // only the visualiser changes. "Intrusive" is not a thing that can be settled in prose, and
    // comparing styles against different bar heights would be meaningless.
    {
        let theme = th(false, amber);
        // A PLAUSIBLE spectrum, not a test pattern: energy falling off with frequency plus a
        // couple of slow ripples. The first version alternated near-full-scale between adjacent
        // bands, which no real music does, and it made every contour style look like a sawtooth —
        // judging a style against data it will never see is worse than not previewing it.
        let levels: Vec<f32> = (0..36)
            .map(|i| {
                let f = i as f32 / 36.0;
                let tilt = (1.0 - f).powf(0.85);
                let ripple = 0.16 * (i as f32 * 0.55).sin() + 0.09 * (i as f32 * 1.3 + 1.0).sin();
                (tilt * 0.9 + ripple).clamp(0.05, 1.0)
            })
            .collect();
        // How much room it takes: OFF / BELOW ART / VEIL / FULL, all drawn as Bars.
        for size in 0..cinder_ui::viz::SIZE_COUNT {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { viz_size: size, viz_kind: 0, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            let label = cinder_ui::viz::size_name(size).to_lowercase().replace(' ', "_");
            save(&c, &format!("viz_size_{size}_{label}"));
        }
        // The confirmation modal, over Settings — Restart and Power off both go through it.
        for (ask, name) in [(cinder_ui::confirm::Ask::Restart, "restart"),
                            (cinder_ui::confirm::Ask::PowerOff, "poweroff")] {
            let mut c = Canvas::new();
            settings::render(&mut c, &theme, &fonts, settings::ROW_RESTART, settings::max_scroll_px(true),
                &settings::SettingsView { more: true, shuffle_by: "SONGS", ignore_the: false, quick: false, volume_limit: false, usb_dac: false, battery_care: true, device: "99% · 34.4 °C",
                    database: "3,424 tracks", storage: "12.4 / 58 GB", sleep: "30 MIN",
                    brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", bt_idle_off: false, boot_stock: "SONY", clock: "17 Aug · 09:01" });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            cinder_ui::confirm::render(&mut c, &theme, &fonts, ask);
            save(&c, &format!("confirm_{name}"));
        }

        // Settings just after More settings is tapped: unfolded, that row under the header — which
        // must survive the scroll (device report, 2026-07-28).
        {
            let mut c = Canvas::new();
            settings::render(&mut c, &theme, &fonts, settings::ROW_MORE,
                settings::row_top_px(settings::ROW_MORE),
                &settings::SettingsView { more: true, shuffle_by: "SONGS", ignore_the: false, quick: false, volume_limit: false, usb_dac: false, battery_care: true, device: "99% · 34.4 °C",
                    database: "3,424 tracks", storage: "12.4 / 58 GB", sleep: "30 MIN",
                    brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", bt_idle_off: false, boot_stock: "SONY", clock: "17 Aug · 09:01" });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, "settings_scrolled");
        }

        // Settings ▸ Device, in the two states that differ: everything readable while charging,
        // and the degraded case — no charger helper installed and several sysfs reads failing. The
        // second has to stay legible: the readings become dashes rather than the screen looking
        // broken, and a blank value column is what that looked like before `text_or_dash`.
        {
            let mut c = Canvas::new();
            let v = cinder_ui::device::DeviceView {
                percent: 99, status: "Charging", health: "Good", millivolts: 4093, care: true,
                chg_state: 1, chg_fault: 0, charger_raw: "10 AC 78 46 10 04 18",
                temp_cpu: 34400, temp_pmic: 39365, temp_abb: 34400,
                cpu_khz: 1300000, cpu_max_khz: 1300000, cores_online: 1, cores_total: 2,
                governor: "hotplug",
                mem_total_kb: 467512, mem_avail_kb: 159772,
                music_total_mb: 56320, music_free_mb: 1024, data_free_mb: 13,
                uptime_s: 15120, kernel: "3.10.26",
                firmware: cinder_ui::settings::FIRMWARE_LABEL,
                base_fw: "SONY STOCK",
            };
            cinder_ui::device::render(&mut c, &theme, &fonts, &v, 0);
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "20:56", "", 99);
            save(&c, "device");
        }
        // Same screen scrolled to the bottom — the sections below the fold are the reason it
        // scrolls at all, and the footer has to land inside the panel.
        {
            let mut c = Canvas::new();
            let v = cinder_ui::device::DeviceView {
                percent: 99, status: "Charging", health: "Good", millivolts: 4093, care: true,
                chg_state: 1, chg_fault: 0, charger_raw: "10 AC 78 46 10 04 18",
                temp_cpu: 34400, temp_pmic: 39365, temp_abb: 34400,
                cpu_khz: 1300000, cpu_max_khz: 1300000, cores_online: 1, cores_total: 2,
                governor: "hotplug",
                mem_total_kb: 467512, mem_avail_kb: 159772,
                music_total_mb: 56320, music_free_mb: 1024, data_free_mb: 13,
                uptime_s: 15120, kernel: "3.10.26",
                firmware: cinder_ui::settings::FIRMWARE_LABEL,
                base_fw: "SONY STOCK",
            };
            let max = cinder_ui::device::max_scroll_px(&v);
            cinder_ui::device::render(&mut c, &theme, &fonts, &v, max);
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "20:56", "", 99);
            save(&c, "device_scrolled");
        }
        {
            let mut c = Canvas::new();
            let v = cinder_ui::device::DeviceView {
                percent: 46, status: "Discharging", health: "", millivolts: 3781, care: false,
                chg_state: -1, chg_fault: -1, charger_raw: "",
                temp_cpu: 31200, temp_pmic: cinder_ui::device::UNKNOWN, temp_abb: 31000,
                cpu_khz: 598000, cpu_max_khz: 1300000, cores_online: 1, cores_total: 2,
                governor: "",
                mem_total_kb: 467512, mem_avail_kb: 251460,
                music_total_mb: 56320, music_free_mb: 40960,
                data_free_mb: cinder_ui::device::UNKNOWN,
                uptime_s: 273600, kernel: "",
                firmware: cinder_ui::settings::FIRMWARE_LABEL,
                base_fw: "SONY STOCK",
            };
            cinder_ui::device::render(&mut c, &theme, &fonts, &v, 0);
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "20:56", "FLAC 24/96", 46);
            save(&c, "device_degraded");
        }

        // The Now Playing PAGES: swipe the artwork to turn them. Only the block above the title
        // changes — same title, same progress, same transport on every one.
        for page in 0..now_playing::PAGES {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { page, viz_size: 1, viz_kind: 0, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, &format!("np_page_{page}"));
        }
        // Night pages too — the theme has a different layout (compact header, no full-bleed
        // cover), so the pages must be checked there separately or a collision would only show up
        // on device, at night, which is the worst place to find one.
        for page in 0..now_playing::PAGES {
            let nt = th(true, amber);
            let mut c = Canvas::new();
            now_playing::render(&mut c, &nt, &fonts,
                &now_playing::NowPlaying { page, viz_size: 1, viz_kind: 1, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &nt, &fonts, "02:14", "FLAC 24/96", 41);
            save(&c, &format!("np_night_page_{page}"));
        }
        // The spectrum page in every style — this is where the style choice actually shows.
        for kind in 0..cinder_ui::viz::COUNT {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { page: 1, viz_kind: kind, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, &format!("np_spectrum_{kind}_{}", cinder_ui::viz::name(kind).to_lowercase()));
        }
        // And the spectrum page with nothing playing — it must say so, not show an empty graph.
        {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { page: 1, viz_levels: None, ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, "np_spectrum_no_signal");
        }
        // Which style: every VizKind, all at VEIL (the default), so they are judged in the size
        // they will actually be seen in.
        for kind in 0..cinder_ui::viz::COUNT {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { viz_size: 2, viz_kind: kind, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, &format!("viz_kind_{kind}_{}", cinder_ui::viz::name(kind).to_lowercase()));
        }
        // Settings ▸ Visualiser, the screen that owns all of the above. Three shots: the default
        // state, the same screen with peak markers and a fixed scale (the two settings that change
        // what the preview DRAWS rather than only what it says), and the no-signal state, which has
        // to admit the preview is synthetic rather than quietly animating.
        {
            // 64 real bands, as the PCM tap gives them: the same curve, sampled finer.
            let levels64: Vec<f32> = (0..64).map(|i| levels[i * levels.len() / 64]).collect();
            let peaks64: Vec<f32> = levels64.iter().map(|v| (v + 0.18).min(1.0)).collect();
            let shots: [(&str, cinder_ui::vizset::VizSet, usize); 3] = [
                ("vizset_default", cinder_ui::vizset::VizSet {
                    style: "BARS", cover: "VEIL", scale: "DYNAMIC", range: "60 DB",
                    response: "NORMAL", curve: "SMOOTH", peaks: false, window: "AUTO", rate: "20 HZ",
                    bands: "36", columns: 36,
                    levels: Some(&levels), peak_marks: None, sig: None, seed: 2.0,
                    kind: cinder_ui::viz::VizKind::Bars,
                }, cinder_ui::vizset::ROW_STYLE),
                ("vizset_peaks_fixed", cinder_ui::vizset::VizSet {
                    style: "SEGMENTS", cover: "FULL", scale: "FIXED", range: "48 DB",
                    response: "FAST", curve: "LINEAR", peaks: true, window: "125 MS", rate: "45 HZ",
                    bands: "64", columns: 64,
                    levels: Some(&levels64), peak_marks: Some(&peaks64), sig: None, seed: 2.0,
                    kind: cinder_ui::viz::VizKind::Segments,
                }, cinder_ui::vizset::ROW_PEAKS),
                ("vizset_no_signal", cinder_ui::vizset::VizSet {
                    style: "RIBBON", cover: "OFF", scale: "DYNAMIC", range: "72 DB",
                    response: "SMOOTH", curve: "SMOOTH", peaks: false, window: "60 MS", rate: "30 HZ",
                    bands: "24", columns: 24,
                    levels: None, peak_marks: None, sig: None, seed: 2.0,
                    kind: cinder_ui::viz::VizKind::Ribbon,
                }, cinder_ui::vizset::ROW_RATE),
            ];
            for (name, vs, sel) in shots {
                let mut c = Canvas::new();
                cinder_ui::vizset::render(&mut c, &theme, &fonts, &vs, sel);
                cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
                save(&c, name);
            }
        }
        // The three low-ink styles again at BELOW ART, where the band is only 16px — a style that
        // needs height to read would fall apart there and that has to be visible, not assumed.
        for kind in [1u8, 2, 7] {
            let mut c = Canvas::new();
            now_playing::render(&mut c, &theme, &fonts,
                &now_playing::NowPlaying { viz_size: 1, viz_kind: kind, viz_levels: Some(&levels), ..np });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, &format!("viz_below_{}", cinder_ui::viz::name(kind).to_lowercase()));
        }
    }

    // Accent sweep: every selectable colour, on the two screens where the accent does the most
    // work — Now Playing (progress fill, transport, badge) and the Settings picker itself. Rendered
    // day-side; the night halves come out of the same table, so a night-only mistake would be a
    // table typo, and `theme::tests::night_accents_are_dimmer_than_day` already guards that.
    for a in cinder_ui::Accent::ALL {
        let theme = th(false, a);
        let lower = a.name().to_lowercase();

        let mut c = Canvas::new();
        now_playing::render(&mut c, &theme, &fonts, &np);
        cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
        save(&c, &format!("accent_{lower}_now_playing"));

        let mut c = Canvas::new();
        cinder_ui::display::render(&mut c, &theme, &fonts, cinder_ui::display::ROW_ACCENT,
            &cinder_ui::display::DisplayView { palette: pal_name, accent_locked: pal_locked, accent: a,
                night: false, volume_hud: 0, viz: "BARS · VEIL", style: cinder_ui::style::Style::Cinder });
        cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
        save(&c, &format!("accent_{lower}_display"));
    }

    // Navigator demo: drive the nav state machine through a press sequence and dump the
    // resulting frames — proves the screen-aware render dispatch (cinder-ffi uses the same).
    use cinder_ui::nav::Button;
    let mut app = new_app();
    let steps: &[(&str, Option<Button>)] = &[
        ("nav_0_now_playing", None),
        ("nav_1_menu", Some(Button::Up)),       // NowPlaying -> Menu
        ("nav_2_menu_library", None), // the cursor already rests on "Library", the first row
        ("nav_3_library", Some(Button::Select)),    // enter Library
        ("nav_4_library_artists", Some(Button::Right)), // Albums -> Artists (then Right again)
    ];
    for (label, btn) in steps {
        if let Some(b) = btn {
            let _ = app.press(*b);
        }
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, label);
    }

    // Windowing/scroll proof: a large synthetic library (240 songs / 60 albums) driven deep,
    // to confirm list windowing + the scrollbar (real libraries are thousands of rows).
    {
        use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, Library, SongRow};
        let artists_n = ["Hollow Pines", "Vesper Lane", "Glass Atlas", "Petal & Wire",
                         "Cold Stone & Sea", "Neon Cartography", "Aurora Bay", "Slow Tide"];
        let mut songs = Vec::new();
        for i in 0..240 {
            let a = artists_n[i % artists_n.len()];
            songs.push(SongRow {
                title: format!("Track {:03} — {}", i + 1, ["Drift", "Ember", "Lantern", "Quartz"][i % 4]),
                artist: a.to_string(),
                dur: format!("{}:{:02}", 2 + i % 5, i * 7 % 60),
                art: format!("album {}", i / 4),
                object_id: i as i64,
                album_id: (i / 4) as i64,
                disc: 1,
                track: (i % 4) as i32 + 1,
                added: 100_000 - i as i64,
                year: 2000 + (i as i32 % 20),
                genre_id: (i as i64 % 3) + 1,
                is_hires: i % 6 == 0,
                // Bit 0 is Sony's always-set one; the rest spread this synthetic library over six
                // channels so the SensMe list has something long enough to window and scroll.
                sensme: 1 | (1 << (i % 6 + 1)),
                ..Default::default()
            });
        }
        let mut album_groups = Vec::new();
        for (gi, a) in artists_n.iter().enumerate() {
            let albums = (0..7)
                .map(|k| {
                    let n = 8 + (k as u32 % 5);
                    AlbumRow {
                        name: format!("{} — Vol. {}", ["Nightfall", "Driftwood", "Halo", "Cinder"][k % 4], k + 1),
                        artist: a.to_string(),
                        year: format!("{}", 2010 + (gi + k) % 14),
                        tracks: n,
                        art: format!("album {}{}", a, k),
                        album_id: (gi * 10 + k) as i64,
                        added: (2010 + (gi + k) % 14) as i64,
                        track_list: (0..n)
                            .map(|i| SongRow {
                                title: format!("{} {}", ["Drift", "Ember", "Lantern", "Quartz"][i as usize % 4], i + 1),
                                artist: a.to_string(),
                                dur: format!("{}:{:02}", 3 + i % 3, (i * 17) % 60),
                                art: format!("album {}{}", a, k),
                                object_id: (gi * 100 + k * 10 + i as usize) as i64,
                                album_id: (gi * 10 + k) as i64,
                                disc: 1,
                                track: i as i32 + 1,
                                year: (2010 + (gi + k) % 14) as i32,
                                ..Default::default()
                            })
                            .collect(),
                    }
                })
                .collect();
            album_groups.push(ArtistGroup { artist: a.to_string(), albums });
        }
        let artists = artists_n
            .iter()
            .map(|a| ArtistRow { name: a.to_string(), albums: 7, tracks: 56, arts: vec![format!("{a}0"), format!("{a}1")], album_ids: Vec::new() })
            .collect();
        let big = Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default(), genres: Vec::new(), ..Default::default() };

        let mut app = new_app();
        app.press(Button::Up); // Menu — the cursor starts on Library, the first row
        app.press(Button::Select); // enter Library
        app.set_library(big);
        // Songs tab (default tab is Albums; Left → Songs), scroll down 30 rows
        app.press(Button::Left);
        for _ in 0..30 {
            app.press(Button::Down);
        }
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "scroll_library_songs");
        // Albums tab (grouped headers), scroll down 24
        app.press(Button::Right); // Songs → Albums
        for _ in 0..24 {
            app.press(Button::Down);
        }
        let mut c2 = Canvas::new();
        app.render(&mut c2, &fonts, &np);
        save(&c2, "scroll_library_albums");
    }

    // ── i18n proof: non-Latin tags ────────────────────────────────────────────────────────────
    // The bundled fonts are Latin-only (Hanken Grotesk has no Cyrillic/Greek/CJK/Thai at all), so
    // on a device these render as `.notdef` boxes unless the fallback chain in `text.rs` picks up
    // Sony's own fonts from /system. Point CINDER_FONT_DIR at the extracted rootfs to see the
    // fixed version; leave it unset to see exactly what the bug looks like:
    //   CINDER_FONT_DIR=../analysis/binwalk/6.bin/_6.bin.extracted/ext-root/vendor/sony/lib/fonts \
    //     cargo run -p cinder-host
    {
        use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, Library, SongRow};
        let rows: &[(&str, &str, &str)] = &[
            ("君の名は", "RADWIMPS", "4:32"),
            ("夜に駆ける", "YOASOBI", "4:19"),
            ("周杰倫 — 稻香", "周杰倫", "3:43"),
            ("봄날", "방탄소년단", "4:34"),
            ("Чайковский — Вальс цветов", "Пётр Чайковский", "6:41"),
            ("Ελλάδα", "Χατζιδάκις", "3:12"),
            ("ลาบ", "คาราบาว", "5:07"),
            ("Björk — Jóga", "Björk", "5:04"),
        ];
        let songs: Vec<SongRow> = rows
            .iter()
            .enumerate()
            .map(|(i, (t, a, d))| SongRow {
                title: t.to_string(),
                artist: a.to_string(),
                dur: d.to_string(),
                art: format!("i18n {i}"),
                object_id: i as i64,
                album_id: i as i64,
                disc: 1,
                track: i as i32 + 1,
                year: 2020,
                ..Default::default()
            })
            .collect();
        let album_groups = rows
            .iter()
            .enumerate()
            .map(|(i, (t, a, _))| ArtistGroup {
                artist: a.to_string(),
                albums: vec![AlbumRow {
                    name: t.to_string(),
                    artist: a.to_string(),
                    year: "2020".into(),
                    tracks: 8,
                    art: format!("i18n {i}"),
                    album_id: i as i64,
                    added: 2020,
                    track_list: songs.clone(),
                }],
            })
            .collect();
        let artists = rows
            .iter()
            .map(|(_, a, _)| ArtistRow { name: a.to_string(), albums: 1, tracks: 8, arts: vec![a.to_string()] , album_ids: Vec::new() })
            .collect();

        let mut app = new_app();
        app.press(Button::Up); // Menu — the cursor starts on Library
        app.press(Button::Select);
        app.set_library(Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default(), genres: Vec::new(), ..Default::default() });
        app.press(Button::Left); // -> Songs
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "i18n_library_songs");

        let np_jp = now_playing::NowPlaying { title: "夜に駆ける", artist: "YOASOBI", ..np };
        let mut c2 = Canvas::new();
        now_playing::render(&mut c2, &th(true, amber), &fonts, &np_jp);
        save(&c2, "i18n_now_playing");
    }

    // Volume HUD over Now Playing (press Vol Up a few times).
    {
        let mut app = new_app();
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "overlay_volume");
    }

    // The Minimal volume readout (handoff 2e): the same presses, with Display ▸ Volume on Minimal.
    {
        let mut app = new_app();
        app.set_volume_hud("minimal");
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "overlay_volume_minimal");
    }

    // Album drill-in: Library → Albums → Select an album → its track list.
    {
        let mut app = new_app();
        app.press(Button::Up); // Menu — the cursor starts on Library
        app.press(Button::Select); // enter Library (Albums tab default)
        app.press(Button::Down); // move to 2nd album
        app.press(Button::Select); // drill into the album
        app.press(Button::Down);
        app.press(Button::Down); // highlight 3rd track
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "album_drill");
    }

    // EQ interactivity: enter EQ, move to band 4, push it up — the selected band highlights.
    //
    // NAMED, not counted. This used to walk four Downs from the Menu on the strength of a comment
    // that said "Equalizer (menu idx 4)" — and the menu has gained rows since, so the four Downs
    // land on FM Radio and this preview has been quietly saving a picture of the RADIO under the
    // name `eq_interactive`. A preview that renders the wrong screen is worse than a missing one:
    // it is what you look at to decide the screen is fine.
    {
        let mut app = new_app();
        app.go_for_preview(cinder_ui::nav::Screen::Eq);
        for _ in 0..4 {
            app.press(Button::Right); // select band 4
        }
        app.press(Button::Up);
        app.press(Button::Up); // boost it
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "eq_interactive");
    }

    // ── Swipe-to-queue, frame by frame ────────────────────────────────────────────────────────
    // The gesture used to act only on release: nothing moved, then a toast appeared. These frames
    // are the whole point of the change — the row travels with the finger, and the panel behind it
    // goes accent-coloured at exactly the travel where releasing will commit, so the gesture says
    // what it will do BEFORE you let go.
    {
        let theme = th(false, amber);
        // A row in the middle of the Songs list, picked from the same geometry the renderer uses
        // rather than a literal — the frames have to sit on a real row or the reveal never shows.
        let row_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) * 2 + 24;
        let travels = [0, 30, 60, 100, 160, 240];
        for (i, raw) in travels.iter().enumerate() {
            for (dir, tag) in [(1, "queue"), (-1, "play_next")] {
                let dx = library::swipe_offset(raw * dir);
                let mut c = Canvas::new();
                library::render(&mut c, &theme, &fonts, Tab::Songs, 99, 0, 0, 0, None, &lib,
                    Some(cinder_ui::library::SwipeRow { y: row_y, dx }), false, 0, false);
                cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
                cinder_ui::chrome::np_bar(&mut c, &theme, &fonts, "Atlas Hands",
                    "Benjamin Francis Leftwich", true, 0.39);
                let armed = if library::swipe_armed(dx) { "armed" } else { "held" };
                save(&c, &format!("swipe_{tag}_{i}_{}px_{armed}", dx.abs()));
            }
        }
    }

    // ── UI SCALE sweep ────────────────────────────────────────────────────────────────────────
    // Every screen at every stop. The scale multiplies TYPE only (row heights and tap targets are
    // fixed), so the failure mode to look for here is text colliding with a neighbour or running
    // past a fixed-position value — which is exactly what these frames are for.
    {
        use cinder_ui::nav::Screen;
        for pct in [80u32, 100, 120, 140] {
            cinder_ui::text::set_scale_pct(pct);
            for (name, screen) in [
                ("library", Screen::Library),
                ("settings", Screen::Settings),
                ("display", Screen::Display),
                ("bluetooth", Screen::Bluetooth),
                ("sound", Screen::Sound),
                ("nowplaying", Screen::NowPlaying),
                ("upnext", Screen::UpNext),
                ("eq", Screen::Eq),
            ] {
                let mut app = new_app();
                app.go_for_preview(screen);
                let mut c = Canvas::new();
                app.render(&mut c, &fonts, &np);
                save(&c, &format!("uiscale_{pct}_{name}"));
            }
            // Settings scrolled to the end, where the value column is densest.
            let mut app = new_app();
            app.go_for_preview(Screen::Settings);
            app.scroll_px(10_000);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, &format!("uiscale_{pct}_settings_bottom"));
        }
        cinder_ui::text::set_scale_pct(100);
    }

    // The palette picker (handoff 5j): the palettes that ship in cinder-ui/palettes, plus one file
    // the player refuses, so the SKIPPED section is drawn too. At 100% and 140%.
    {
        use cinder_ui::nav::Screen;
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../cinder-ui/palettes");
        let mut files: Vec<(String, Result<String, String>)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let body = std::fs::read_to_string(e.path()).map_err(|e| e.to_string());
                        (name, body)
                    })
                    .collect()
            })
            .unwrap_or_default();
        // cinder.palette is the reference copy of the built-in palette, which the player refuses
        // by name; a real folder does not hold it.
        files.retain(|(n, _)| n != "cinder.palette");
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files.push(("neon.palette".to_string(), Ok("name = Neon\nday.ink = #0e0d0c\n".to_string())));
        let (list, skipped) = cinder_ui::palette::load_files(files);
        for pct in [100u32, 140] {
            cinder_ui::text::set_scale_pct(pct);
            let mut app = new_app();
            app.set_palettes(list.clone(), skipped.clone());
            app.go_for_preview(Screen::Palette);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, &format!("palette_picker_{pct}"));
        }
        cinder_ui::text::set_scale_pct(100);
    }

    // Visualiser TYPES: render Now Playing with each viz kind (mid-animation) so they can be diffed.
    for k in 0..cinder_ui::viz::COUNT {
        let np_k = now_playing::NowPlaying { viz_seed: 1.7, viz_kind: k, ..np };
        let mut c = Canvas::new();
        now_playing::render(&mut c, &th(false, amber), &fonts, &np_k);
        save(&c, &format!("viz_{}_{}", k, cinder_ui::viz::name(k).to_lowercase()));
    }

    // Sleep ▸ End of song ("stop after current"): the badge says so instead of a countdown.
    {
        let mut c = Canvas::new();
        let theme = th(false, amber);
        now_playing::render(&mut c, &theme, &fonts, &np);
        now_playing::sleep_badge(&mut c, &theme, &fonts, 0, true);
        save(&c, "now_playing_sleep_song_end");
    }
    // Repeat album: the loop glyph with an A in it.
    {
        let mut c = Canvas::new();
        now_playing::render(&mut c, &th(false, amber), &fonts,
                            &now_playing::NowPlaying { repeat: now_playing::REPEAT_ALBUM, ..np });
        save(&c, "now_playing_repeat_album");
    }

    // Help & controls (handoff 5i): the top, with the way back to Sony, and the end of the list with
    // the pull-down panel switched on, so its row and the replay row are both drawn.
    {
        use cinder_ui::nav::Screen;
        for (name, end) in [("help_top", false), ("help_end", true)] {
            let mut app = new_app();
            app.set_quick_enabled(end);
            app.go_for_preview(Screen::Help);
            if end {
                app.scroll_px(10_000);
            }
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, name);
        }
    }

    // Sound ▸ Advanced with its ninth row, and the DAC EQ it leads to: a shaped curve, and the same
    // curve held flat under Source Direct. LAST on purpose: a new preview can move anti-aliased
    // pixels on screens rendered after it (the glyph cache is keyed to a quarter pixel).
    {
        use cinder_ui::nav::Screen;
        let curve = [6, 0, -4, 2, 12];
        for (name, screen, direct) in [
            ("advanced", Screen::Advanced, false),
            ("dac_eq", Screen::DacEq, false),
            ("dac_eq_source_direct", Screen::DacEq, true),
        ] {
            let mut app = new_app();
            app.set_dac_eq(curve);
            app.set_adv_flags(direct as u8);
            app.go_for_preview(screen);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, name);
        }
    }

    // Sound profiles (R5), driven through the navigator so the previews are the states a user can
    // reach: the Profiles screen on the jack, the same screen with Bluetooth live on B, and the
    // Sound list scrolled to its end with that profile live. After everything else, for the same
    // glyph-cache reason as the block above.
    {
        use cinder_ui::nav::Screen;
        use cinder_ui::profile::{centre, Hit, Output};
        for (name, screen, bt_live) in [
            ("profiles", Screen::Profiles, false),
            ("profiles_bt_live", Screen::Profiles, true),
            ("sound_end_profile_b", Screen::Sound, true),
        ] {
            let mut app = new_app();
            app.go_for_preview(Screen::Profiles);
            app.tap(240, centre(Hit::Output(Output::Bluetooth))); // Bluetooth uses B
            app.set_bt_route(bt_live);
            app.go_for_preview(screen);
            app.scroll_px(10_000);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, name);
        }
    }

    // The visualisers that draw from the decoded audio (the PCM tap), with a signal made to look
    // like music: a bass note and its harmonics, a stereo pair that is mostly but not wholly
    // correlated, meters near -10 dBFS, and five seconds of a moving spectrum for the spectrogram.
    // Day and night, on the spectrum page where they are meant to be looked at. LAST, like the two
    // blocks above.
    {
        use cinder_ui::viz::{Signal, VizKind};
        let tau = std::f32::consts::TAU;
        let wave: Vec<f32> = (0..480)
            .map(|i| {
                let t = i as f32 / 480.0;
                0.55 * (tau * 3.0 * t).sin() + 0.22 * (tau * 9.0 * t + 0.4).sin() + 0.1 * (tau * 23.0 * t + 1.1).sin()
            })
            .collect();
        let mut seed = 12345u32;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for i in 0..512 {
            let t = i as f32 / 512.0;
            let common = 0.5 * (tau * 7.0 * t).sin() + 0.2 * (tau * 31.0 * t).sin();
            left.push(common + 0.18 * noise());
            right.push(common * 0.9 + 0.22 * noise());
        }
        let cols = 48;
        let rows = 108;
        let mut hist = vec![0.0f32; cols * rows];
        for slot in 0..rows {
            // `r` is time, oldest first; the ring stores it starting at the head (37).
            let r = (slot + rows - 37) % rows;
            let beat = if r % 12 < 2 { 0.35 } else { 0.0 };
            for b in 0..cols {
                let f = b as f32 / cols as f32;
                let melody = (-((f - 0.35 - 0.12 * (r as f32 * 0.09).sin()).powi(2)) / 0.004).exp() * 0.6;
                let v = (0.75 - 0.6 * f) * (0.55 + 0.25 * (r as f32 * 0.21 + b as f32 * 0.5).sin()) + melody
                    + if f < 0.15 { beat } else { 0.0 };
                hist[slot * cols + b] = (v * 0.8 - 0.1).clamp(0.0, 1.0);
            }
        }
        let sig = Signal {
            wave: &wave, left: &left, right: &right,
            meter: [0.86, 0.83, 0.71, 0.68], hold: [0.91, 0.88],
            hist: &hist, hist_cols: cols, hist_rows: rows, hist_head: 37,
        };
        let levels48: Vec<f32> = (0..48).map(|b| hist[((37 + rows - 1) % rows) * cols + b]).collect();
        for (kind, slug) in [
            (VizKind::Scope, "scope"), (VizKind::Stereo, "stereo"), (VizKind::Spectrogram, "spectrogram"),
            (VizKind::Meters, "meters"), (VizKind::Radial, "radial"),
        ] {
            let k = (0..cinder_ui::viz::COUNT).find(|&i| cinder_ui::viz::from_index(i) == kind).unwrap();
            for night in [false, true] {
                let t = th(night, amber);
                let mut c = Canvas::new();
                now_playing::render(&mut c, &t, &fonts, &now_playing::NowPlaying {
                    page: 1, viz_kind: k, viz_levels: Some(&levels48), viz_sig: Some(&sig), ..np });
                cinder_ui::chrome::status_bar(&mut c, &t, &fonts, "14:32", "FLAC 24/96", 78);
                save(&c, &format!("viz_signal_{slug}{}", if night { "_night" } else { "" }));
            }
        }
    }

    // Menu ▸ Soundscapes: switched off; rain on its own through the headphones with no Wampy (the
    // note says what over-music needs); and a beach over the music, at night, with the shim loaded.
    {
        use cinder_ui::nav::Screen;
        use cinder_ui::soundscape::Route;
        for (name, on, sound, route, hook, night) in [
            ("soundscape_off", false, 4u8, Route::Off, false, false),
            ("soundscape_rain_alone", true, 4, Route::AloneJack, false, false),
            ("soundscape_beach_music_night", true, 6, Route::OverMusic, true, true),
        ] {
            let mut app = new_app();
            app.night = night;
            app.set_ambient_sound(sound);
            app.set_ambient_on(on);
            app.set_ambient_levels(60, 25);
            app.set_mono_shim(hook);
            app.go_for_preview(Screen::Soundscape);
            app.set_ambient_route(route);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, name);
        }
    }
}

// ── Golden pixel hashes ────────────────────────────────────────────────────────────────────────

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden.txt");

/// FNV-1a over each 32-bit pixel. Not a security hash: the one question it answers is "did any
/// pixel change", and it answers it with no dependency and in a single pass.
fn pixel_hash(c: &Canvas) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &px in &c.buf {
        h ^= px as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Every preview's name and pixel hash, in render order.
fn hashes() -> Vec<(String, u64)> {
    // The i18n previews pull glyphs from CINDER_FONT_DIR when it is set, so a developer who has
    // pointed it at Sony's fonts would otherwise fail the check for reasons that are not in the
    // tree. Golden pixels are the bundled fonts only.
    std::env::remove_var("CINDER_FONT_DIR");
    cinder_ui::text::set_scale_pct(100);
    let mut got = Vec::new();
    render_all(
        &mut |name, c| got.push((name.to_string(), pixel_hash(c))),
        &Opts { golden: true, palette: None },
    );
    got
}

fn read_golden() -> Vec<(String, u64)> {
    let body = std::fs::read_to_string(GOLDEN).unwrap_or_default();
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (h, name) = l.split_once(char::is_whitespace)?;
            Some((name.trim().to_string(), u64::from_str_radix(h, 16).ok()?))
        })
        .collect()
}

fn write_golden(got: &[(String, u64)]) {
    let mut body = String::from(
        "# One pixel hash per cinder-host preview (FNV-1a over the XRGB buffer), in render order.\n\
         # Checked by `cargo test -p cinder-host`. After a deliberate UI change, look at the PNGs\n\
         # (`cargo run -p cinder-host`) and then record the new pixels:\n\
         #   cargo run -p cinder-host -- --bless\n",
    );
    for (name, h) in got {
        body.push_str(&format!("{h:016x} {name}\n"));
    }
    std::fs::write(GOLDEN, body).expect("write golden.txt");
}

/// Human-readable differences between the recorded and the rendered set. Empty = identical.
fn compare(want: &[(String, u64)], got: &[(String, u64)]) -> Vec<String> {
    use std::collections::HashMap;
    let mut out = Vec::new();
    let mut seen: HashMap<&str, u64> = HashMap::new();
    for (name, h) in got {
        // Two previews under one name overwrite each other's PNG, so one of them has never been
        // looked at. That is a defect in the preview list whatever the pixels say.
        if seen.insert(name, *h).is_some() {
            out.push(format!("duplicate {name} (two previews share this name)"));
        }
    }
    let recorded: HashMap<&str, u64> = want.iter().map(|(n, h)| (n.as_str(), *h)).collect();
    for (name, h) in want {
        match seen.get(name.as_str()) {
            None => out.push(format!("missing   {name} (in golden.txt, no longer rendered)")),
            Some(g) if g != h => out.push(format!("changed   {name}")),
            _ => {}
        }
    }
    for (name, _) in got {
        if !recorded.contains_key(name.as_str()) {
            out.push(format!("new       {name} (not in golden.txt yet)"));
        }
    }
    out
}

/// Every preview checked for text the user cannot read: pixels past the left or right edge of the
/// panel (`Canvas::oob_x`), text runs that land on other text (`Canvas::text_collisions`), and text
/// that something drawn afterwards covers — a swatch, an icon, a button (`Canvas::text_hidden`).
/// `tests/ui_overflow.rs` does the same over every screen in a bare state; this covers the states
/// only the previews set up — a full library, paired devices, every UI scale, every palette.
fn audit() -> Vec<String> {
    cinder_ui::canvas::audit_new_canvases(true);
    let mut bad = Vec::new();
    render_all(
        &mut |name, c| {
            if c.oob_x() > 0 {
                bad.push(format!("{name}: {} px past the left or right edge", c.oob_x()));
            }
            for (a, b, n) in c.text_collisions() {
                bad.push(format!("{name}: {a:?} runs into {b:?} ({n} px)"));
            }
            for (a, n) in c.text_hidden() {
                bad.push(format!("{name}: {a:?} is covered by something drawn over it ({n} px)"));
            }
        },
        &Opts { golden: true, palette: None },
    );
    cinder_ui::canvas::audit_new_canvases(false);
    bad
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--audit") => {
            let bad = audit();
            if bad.is_empty() {
                println!("audit: no preview clips, overlaps or covers text");
            } else {
                eprintln!("audit: {} problem(s):\n  {}", bad.len(), bad.join("\n  "));
                std::process::exit(1);
            }
        }
        None => {
            std::fs::create_dir_all("out").ok();
            render_all(&mut |name, c| save_png(c, name), &Opts { golden: false, palette: None });
        }
        Some("--palette") => {
            let Some(path) = std::env::args().nth(2) else {
                eprintln!("usage: cinder-host --palette FILE.palette");
                std::process::exit(2);
            };
            let file = std::path::Path::new(&path);
            let Some(id) = file
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(cinder_ui::palette::palette_stem)
                .map(str::to_ascii_lowercase)
            else {
                eprintln!("{path}: a palette's file name has to end in .palette");
                std::process::exit(2);
            };
            let body = std::fs::read_to_string(file).unwrap_or_else(|e| {
                eprintln!("{path}: {e}");
                std::process::exit(2)
            });
            match cinder_ui::palette::Palette::parse(&id, &body) {
                Err(problems) => {
                    eprintln!("{path} would be skipped by the player:\n  {}", problems.join("\n  "));
                    std::process::exit(1);
                }
                Ok(p) => {
                    let dir = format!("palette_{id}");
                    std::fs::create_dir_all(format!("out/{dir}")).ok();
                    let mut n = 0;
                    render_all(
                        &mut |name, c| {
                            save_png(c, &format!("{dir}/{name}"));
                            n += 1;
                        },
                        &Opts { golden: false, palette: Some(p) },
                    );
                    println!("{path}: loads — {n} previews in out/{dir}/");
                }
            }
        }
        Some("--bless") => {
            let got = hashes();
            write_golden(&got);
            println!("golden.txt: recorded {} previews", got.len());
        }
        Some("--check") => {
            let diff = compare(&read_golden(), &hashes());
            if diff.is_empty() {
                println!("golden.txt: every preview matches");
            } else {
                eprintln!("{} preview(s) differ from golden.txt:\n  {}", diff.len(), diff.join("\n  "));
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("unknown argument {other:?}\nusage: cinder-host [--check | --bless | --audit | --palette FILE]");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph cache is process-wide and ORDER-sensitive (its key rounds the size, so whichever
    /// size fills a key first is what later requests get). Two tests rendering every preview at
    /// once interleave that order and move anti-aliased pixels, so they take turns.
    static RENDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn render_turn() -> std::sync::MutexGuard<'static, ()> {
        RENDER.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// No preview clips text off the panel or runs text into text. See [`audit`].
    #[test]
    fn every_preview_keeps_its_text_on_the_glass_and_apart() {
        let _turn = render_turn();
        let bad = audit();
        assert!(bad.is_empty(), "\n{} problem(s):\n  {}\n", bad.len(), bad.join("\n  "));
    }

    /// Every screen the preview harness can draw, pixel for pixel, against what was last blessed.
    #[test]
    fn every_preview_matches_its_golden_hash() {
        let _turn = render_turn();
        let want = read_golden();
        assert!(!want.is_empty(), "golden.txt is missing or empty — run: cargo run -p cinder-host -- --bless");
        let diff = compare(&want, &hashes());
        assert!(
            diff.is_empty(),
            "\n{} preview(s) differ from golden.txt:\n  {}\n\nIf the change is intended, look at the \
             PNGs (cargo run -p cinder-host) and record it:\n  cargo run -p cinder-host -- --bless\n",
            diff.len(),
            diff.join("\n  ")
        );
    }

    /// The hash has to see a one-pixel change, or the test above proves nothing.
    #[test]
    fn a_single_pixel_changes_the_hash() {
        let mut c = Canvas::new();
        let before = pixel_hash(&c);
        c.buf[W * H / 2] ^= 1;
        assert_ne!(before, pixel_hash(&c));
    }

    #[test]
    fn compare_names_every_kind_of_difference() {
        let want = vec![("a".to_string(), 1), ("b".to_string(), 2), ("gone".to_string(), 3)];
        let got = vec![("a".to_string(), 1), ("b".to_string(), 9), ("fresh".to_string(), 4), ("a".to_string(), 1)];
        let d = compare(&want, &got);
        assert!(d.iter().any(|l| l.starts_with("changed") && l.contains(" b")), "{d:?}");
        assert!(d.iter().any(|l| l.starts_with("missing") && l.contains("gone")), "{d:?}");
        assert!(d.iter().any(|l| l.starts_with("new") && l.contains("fresh")), "{d:?}");
        assert!(d.iter().any(|l| l.starts_with("duplicate") && l.contains(" a")), "{d:?}");
        assert!(compare(&want[..2], &got[..2]).len() == 1, "only b differs there");
    }
}

/// `lib` with one tab switched to `view`, for the Library view previews.
fn with_view(lib: &cinder_ui::Library, tab: library::Tab, view: library::LibView) -> cinder_ui::Library {
    let mut l = lib.clone();
    l.views[tab as usize] = view;
    l
}
