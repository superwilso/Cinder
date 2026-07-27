//! Host preview backend: render every screen to PNG for device-free iteration.

use cinder_ui::bluetooth::Bt;
use cinder_ui::library::Tab;
use cinder_ui::menu::MenuItem;
use cinder_ui::sound::Sound;
use cinder_ui::{
    bluetooth, eq, fm, library, lock, menu, now_playing, pairing, receiver, settings, shelf, sound,
    up_next, usbdac, Canvas, FontSet, Library, Theme, H, W,
};

fn save(c: &Canvas, name: &str) {
    let img = image::RgbImage::from_raw(W as u32, H as u32, c.to_rgb_bytes()).expect("buffer size");
    let path = format!("out/{name}.png");
    img.save(&path).expect("save png");
    println!("wrote {path}");
}

/// Load a raw NxN RGB thumbnail (the on-device art-cache format) so the host preview can render
/// REAL covers pulled off the device — the only way to check the cover draw path without flashing.
///   CINDER_PREVIEW_T48=<file> CINDER_PREVIEW_T96=<file> cargo run -p cinder-host
fn preview_thumb(var: &str, edge: usize) -> Option<cinder_ui::art::Image> {
    let path = std::env::var(var).ok()?;
    let rgb = std::fs::read(path).ok()?;
    (rgb.len() == edge * edge * 3).then(|| cinder_ui::art::Image { w: edge, h: edge, rgb })
}

fn main() {
    let fonts = FontSet::load();
    std::fs::create_dir_all("out").ok();

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
        viz_on: true,
        viz_levels: None,
        scrubbing: false,
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

    let menu_items = [
        MenuItem { icon: "note", label: "Now Playing", value: "Atlas Hands · 1:47", active: true },
        MenuItem { icon: "library", label: "Library", value: "124 albums · 1,842 tracks", active: false },
        MenuItem { icon: "queue", label: "Up Next", value: "8 tracks · 41:24", active: false },
        MenuItem { icon: "radio", label: "FM Radio", value: "88.6 MHz", active: false },
        MenuItem { icon: "eq", label: "Equalizer", value: "Custom A1", active: false },
        MenuItem { icon: "sound", label: "Sound Settings", value: "DSEE HX · VPT · Vinyl", active: false },
        MenuItem { icon: "bt", label: "Bluetooth", value: "WH-1000XM5 · LDAC", active: false },
        MenuItem { icon: "usb", label: "USB-DAC", value: "Off", active: false },
        MenuItem { icon: "rx", label: "BT Receiver", value: "Off", active: false },
        MenuItem { icon: "settings", label: "Settings", value: "System · Storage · About", active: false },
        MenuItem { icon: "note", label: "Help & Controls", value: "Button map · features", active: false },
    ];

    let snd = Sound {
        dsee: true,
        vinyl: false,
        vpt: "Studio",
        dcphase: "Low A",
        normalizer: true,
        clearaudio: false,
        eq_preset: "A1",
        bt_codec: Some("LDAC"),
    };
    let bt = Bt { on: true, connected: Some("WH-1000XM5"), codec_sel: 0, ldac_quality: 0 };
    let eq_bands: [i8; 10] = [2, 3, 1, 0, -1, 0, 2, 3, 2, 1];
    let mut lib = Library::sample();
    // Sample albums all carry album_id 0, so one pulled thumbnail stands in for every row —
    // enough to check placement, scaling and the day/night dim against a real cover.
    if let Some(t48) = preview_thumb("CINDER_PREVIEW_T48", 48) {
        for id in 0..8 {
            lib.thumbs.insert(id, t48.clone());
        }
        println!("preview: using real device thumbnails");
    }
    let lib = lib;

    for (name, theme) in [("day", Theme::day()), ("night", Theme::night())] {
        let render_set: &[(&str, &dyn Fn(&mut Canvas))] = &[
            ("now_playing", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts, &np)),
            ("now_playing_sleep", &|c: &mut Canvas| { now_playing::render(c, &theme, &fonts, &np); now_playing::sleep_badge(c, &theme, &fonts, 23); }),
            // Nothing loaded — the state the device actually boots into. Never rendered here
            // before, which is how an empty codec badge shipped as a bare stroked box.
            ("now_playing_idle", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts,
                &now_playing::NowPlaying { title: "", artist: "", codec: "", badge: "", elapsed: "",
                                           remaining: "", progress: 0.0, playing: false, liked: false,
                                           art: "", viz_on: false, ..np })),
            ("onboard_0_welcome", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 0)),
            ("onboard_1_controls", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 1)),
            ("onboard_2_features", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 2)),
            ("onboard_3_done", &|c: &mut Canvas| cinder_ui::onboarding::render(c, &theme, &fonts, 3)),
            ("shelf", &|c: &mut Canvas| {
                now_playing::render(c, &theme, &fonts, &np);
                shelf::render(c, &theme, &fonts, "Now Playing · Atlas Hands", "1:47 / 4:32",
                    &[Some(shelf::Pin { title: "Library · Albums", sub: "Saved 2 min ago" }), None, None]);
            }),
            ("lock", &|c: &mut Canvas| lock::render(c, &theme, &fonts, &lk)),
            ("menu", &|c: &mut Canvas| menu::render(c, &theme, &fonts, &menu_items)),
            ("up_next", &|c: &mut Canvas| {
                match lib.album_groups.first().and_then(|g| g.albums.iter().find(|a| !a.track_list.is_empty())) {
                    Some(al) => up_next::render(c, &theme, &fonts, &al.name, &al.track_list, 1),
                    None => up_next::render(c, &theme, &fonts, "", &[], 0),
                }
            }),
            ("library_songs", &|c: &mut Canvas| {
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &lib);
                // nav draws the Now Playing return bar over the library screens; mirror that here
                // so the preview shows the real bottom of the screen, not a list running to the edge.
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands", "Benjamin Francis Leftwich", true);
            }),
            // Songs sorted by ADDED (sort chip index 4) — shows the SORT chip label + reorder.
            ("library_songs_added", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 4, 0, None, &lib)),
            ("library_albums", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, None, &lib)),
            // Albums with the first album's accordion expanded (tracks listed inline).
            ("library_albums_expanded", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, Some(0), &lib)),
            // Albums flat-ordered A-Z (ORDER chip index 1 — no artist headers).
            ("library_albums_az", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 1, None, &lib)),
            ("library_artists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Artists, 0, 0, 0, 0, None, &lib)),
            ("library_playlists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Playlists, 0, 0, 0, 0, None, &lib)),
            ("artist", &|c: &mut Canvas| library::artist(c, &theme, &fonts)),
            ("eq", &|c: &mut Canvas| eq::render(c, &theme, &fonts, &eq_bands, "A1", 4)),
            ("sound", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 0, false)),
            ("sound_bypass", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 5, true)),
            ("settings", &|c: &mut Canvas| settings::render(c, &theme, &fonts, 1,
                &settings::SettingsView { night: theme.night, viz_name: "Bars", viz_on: true, usb_dac: false, battery_care: true, storage: "12.4 / 58 GB", sleep: "30 MIN", brightness: "4 / 5", screen_off: "OFF" })),
            ("bluetooth", &|c: &mut Canvas| bluetooth::render(c, &theme, &fonts, &bt)),
            ("pairing", &|c: &mut Canvas| pairing::render(c, &theme, &fonts, 2, Some(1))),
            ("receiver", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, true)),
            ("fm", &|c: &mut Canvas| fm::render(c, &theme, &fonts, 88.6)),
            ("usbdac", &|c: &mut Canvas| usbdac::render(c, &theme, &fonts, true, true, "LDAC", Some("WH-1000XM5"), "A1", true)),
        ];
        for (screen, draw) in render_set {
            let mut c = Canvas::new();
            draw(&mut c);
            save(&c, &format!("{screen}_{name}"));
        }
    }

    // Navigator demo: drive the nav state machine through a press sequence and dump the
    // resulting frames — proves the screen-aware render dispatch (cinder-ffi uses the same).
    use cinder_ui::nav::{App, Button};
    let mut app = App::unlocked();
    let steps: &[(&str, Option<Button>)] = &[
        ("nav_0_now_playing", None),
        ("nav_1_menu", Some(Button::Up)),       // NowPlaying -> Menu
        ("nav_2_menu_library", Some(Button::Down)), // highlight "Library"
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
            .map(|a| ArtistRow { name: a.to_string(), albums: 7, tracks: 56, arts: vec![format!("{a}0"), format!("{a}1")] })
            .collect();
        let big = Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default() };

        let mut app = App::unlocked();
        app.press(Button::Up); // Menu
        app.press(Button::Down); // -> Library row
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
            .map(|(_, a, _)| ArtistRow { name: a.to_string(), albums: 1, tracks: 8, arts: vec![a.to_string()] })
            .collect();

        let mut app = App::unlocked();
        app.press(Button::Up);
        app.press(Button::Down);
        app.press(Button::Select);
        app.set_library(Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default() });
        app.press(Button::Left); // -> Songs
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "i18n_library_songs");

        let np_jp = now_playing::NowPlaying { title: "夜に駆ける", artist: "YOASOBI", ..np };
        let mut c2 = Canvas::new();
        now_playing::render(&mut c2, &Theme::night(), &fonts, &np_jp);
        save(&c2, "i18n_now_playing");
    }

    // Volume HUD over Now Playing (press Vol Up a few times).
    {
        let mut app = App::unlocked();
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        app.press(Button::VolUp);
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "overlay_volume");
    }

    // Album drill-in: Library → Albums → Select an album → its track list.
    {
        let mut app = App::unlocked();
        app.press(Button::Up); // Menu
        app.press(Button::Down); // Library row
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
    {
        let mut app = App::unlocked();
        app.press(Button::Up); // Menu
        for _ in 0..4 {
            app.press(Button::Down);
        }
        app.press(Button::Select); // -> Equalizer (menu idx 4)
        for _ in 0..4 {
            app.press(Button::Right); // select band 4
        }
        app.press(Button::Up);
        app.press(Button::Up); // boost it
        let mut c = Canvas::new();
        app.render(&mut c, &fonts, &np);
        save(&c, "eq_interactive");
    }

    // Visualiser TYPES: render Now Playing with each viz kind (mid-animation) so they can be diffed.
    for k in 0..cinder_ui::viz::COUNT {
        let np_k = now_playing::NowPlaying { viz_seed: 1.7, viz_kind: k, ..np };
        let mut c = Canvas::new();
        now_playing::render(&mut c, &Theme::day(), &fonts, &np_k);
        save(&c, &format!("viz_{}_{}", k, cinder_ui::viz::name(k).to_lowercase()));
    }
}
