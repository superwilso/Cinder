//! Host preview backend: render every screen to PNG for device-free iteration.

use cinder_ui::bluetooth::Bt;
use cinder_ui::library::Tab;
use cinder_ui::menu::MenuItem;
use cinder_ui::sound::Sound;
use cinder_ui::{
    bluetooth, clockset, eq, fm, library, lock, menu, now_playing, pairing, receiver, settings, shelf, sound,
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
        viz_size: 1, page: 0,
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
        MenuItem { icon: "library", label: "Library", value: "6 albums · 8 tracks", active: false },
        MenuItem { icon: "queue", label: "Up Next", value: "8 tracks · 41:24", active: false },
        MenuItem { icon: "radio", label: "FM Radio", value: "", active: false },
        MenuItem { icon: "eq", label: "Equalizer", value: "A1", active: false },
        MenuItem { icon: "sound", label: "Sound Settings", value: "Off", active: false },
        MenuItem { icon: "bt", label: "Bluetooth", value: "LDAC", active: false },
        MenuItem { icon: "usb", label: "USB-DAC", value: "Off", active: false },
        MenuItem { icon: "rx", label: "BT Receiver", value: "Off", active: false },
        MenuItem { icon: "settings", label: "Settings", value: "System · Storage · About", active: false },
        MenuItem { icon: "note", label: "Help & Controls", value: "Button map · features", active: false },
    ];

    let snd = Sound {
        dsee: true,
        balance: cinder_ui::sound::BALANCE_CENTRE, balance_drag: false, bt_route: false,
        vinyl: false,
        vpt: "Studio",
        dcphase: "Low A",
        normalizer: true,
        clearaudio: false,
        eq_preset: "A1",
        bt_codec: Some("LDAC"),
    };
    let bt = Bt { on: true, connected: Some("WH-1000XM5"), link_known: true, codec_sel: 0, ldac_quality: 0, enhanced: true, enhanced_supported: true, connecting: false, busy_phase: 0.0, link_codec: Some(0x02) };
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
    // A stand-in USER queue for the Up Next previews (the real one is built by swiping rows in).
    let queue: Vec<cinder_ui::model::SongRow> = lib.songs.iter().take(9).cloned().collect();

    for (name, theme) in [("day", Theme::day()), ("night", Theme::night())] {
        let render_set: &[(&str, &dyn Fn(&mut Canvas))] = &[
            ("now_playing", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts, &np)),
            ("now_playing_sleep", &|c: &mut Canvas| { now_playing::render(c, &theme, &fonts, &np); now_playing::sleep_badge(c, &theme, &fonts, 23); }),
            // Nothing loaded — the state the device actually boots into. Never rendered here
            // before, which is how an empty codec badge shipped as a bare stroked box.
            ("now_playing_idle", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts,
                &now_playing::NowPlaying { title: "", artist: "", codec: "", badge: "", elapsed: "",
                                           remaining: "", progress: 0.0, playing: false, liked: false,
                                           art: "", viz_size: 0, page: 0, ..np })),
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
            // The unified queue: history above the playing track, then the user's own queue,
            // then the rest of the album. `current: Some(2)` so the PREVIOUSLY PLAYED section is
            // actually populated in the preview — at track 0 it is omitted entirely.
            ("up_next", &|c: &mut Canvas| {
                let al = lib.album_groups.first()
                    .and_then(|g| g.albums.iter().find(|a| !a.track_list.is_empty()));
                let (album, tracks) = match al {
                    Some(a) => (a.name.as_str(), &a.track_list[..]),
                    None => ("", &[][..]),
                };
                // With a user queue too, so the preview shows all four sections and both chips.
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album, tracks,
                    current: (!tracks.is_empty()).then(|| 2.min(tracks.len() - 1)),
                    queue: &queue[..2.min(queue.len())], lib: &lib, scroll_px: 0,
                    drag: None, swipe: None, sbar_active: false,
                });
            }),
            // The USER queue, at rest and mid-reorder. The second one is the gesture the device
            // can't be screenshotted through: the row is lifted under a finger that isn't there.
            ("up_next_queue", &|c: &mut Canvas| {
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &[], current: None,
                    queue: &queue, lib: &lib, scroll_px: 0,
                    drag: None, swipe: None, sbar_active: false,
                });
            }),
            ("up_next_reorder", &|c: &mut Canvas| {
                let l = up_next::layout(0, None, queue.len());
                let from = 1usize;
                let grab_off = up_next::RH / 2;
                // The queue no longer starts at the top of the list, so the row's screen y comes
                // from the layout — the same rule nav's reorder_begin follows.
                let row_top = cinder_ui::chrome::HEADER_BOTTOM
                    + l.top_of(up_next::Slot::Queued(from)).unwrap_or(0);
                let start_y = row_top + grab_off;
                let y = start_y + 2 * up_next::RH + 14;   // dragged down past two rows
                let d = up_next::QueueDrag {
                    from,
                    to: l.queue_slot_for(y - grab_off, 0),
                    start_y,
                    y,
                    grab_off,
                };
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &[], current: None,
                    queue: &queue, lib: &lib, scroll_px: 0,
                    drag: Some(d), swipe: None, sbar_active: false,
                });
            }),
            ("playlist_page", &|c: &mut Canvas| {
                match lib.playlists.first() {
                    Some(pl) => library::playlist_view(c, &theme, &fonts, &lib, pl, 0, 0, None, false),
                    None => {}
                }
            }),
            ("up_next_remove", &|c: &mut Canvas| {
                let l = up_next::layout(0, None, queue.len());
                let row_y = cinder_ui::chrome::HEADER_BOTTOM
                    + l.top_of(up_next::Slot::Queued(2)).unwrap_or(0) + up_next::RH / 2;
                up_next::render_view(c, &theme, &fonts, &up_next::QueueView {
                    album: "", tracks: &[], current: None,
                    queue: &queue, lib: &lib, scroll_px: 0,
                    drag: None,
                    swipe: Some(cinder_ui::library::SwipeRow { y: row_y, dx: 110 }),
                    sbar_active: false,
                });
            }),
            ("library_songs", &|c: &mut Canvas| {
                library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 0, 0, None, &lib, None, false);
                // nav draws the Now Playing return bar over the library screens; mirror that here
                // so the preview shows the real bottom of the screen, not a list running to the edge.
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands", "Benjamin Francis Leftwich", true, 0.39);
            }),
            // Songs sorted by ADDED (sort chip index 4) — shows the SORT chip label + reorder.
            ("library_songs_added", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Songs, 0, 0, 4, 0, None, &lib, None, false)),
            ("library_albums", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, None, &lib, None, false)),
            // Albums with the first album's accordion expanded (tracks listed inline).
            ("library_albums_expanded", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 0, Some(0), &lib, None, false)),
            // Albums flat-ordered A-Z (ORDER chip index 1 — no artist headers).
            ("library_albums_az", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0, 0, 0, 1, None, &lib, None, false)),
            ("library_artists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Artists, 0, 0, 0, 0, None, &lib, None, false)),
            ("library_playlists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Playlists, 0, 0, 0, 0, None, &lib, None, false)),
            // The artist drill-in, built from the SAMPLE LIBRARY like every other list preview —
            // it used to render three hard-coded albums from `data::ARTIST_*` regardless of who
            // the artist was, which is precisely why nothing ever pushed it.
            ("artist", &|c: &mut Canvas| {
                let name = lib.artists.first().map(|a| a.name.as_str()).unwrap_or("");
                let page = library::artist_page(&lib, name);
                library::artist_view(c, &theme, &fonts, &lib, &page, 0, 0, None, false);
                cinder_ui::chrome::np_bar(c, &theme, &fonts, "Atlas Hands", "Benjamin Francis Leftwich", true, 0.39);
            }),
            ("eq", &|c: &mut Canvas| eq::render(c, &theme, &fonts, &eq_bands, "A1", 4)),
            ("sound", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 0, 0)),
            ("sound_setup_b", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd, 5, 1)),
            // The balance slider off-centre and mid-drag: the two states the static preview above
            // never shows, and the ones where the knob can drift off its hit band.
            ("clockset", &|c: &mut Canvas| {
                clockset::render(c, &theme, &fonts, &[2026, 8, 17, 9, 1], clockset::F_MONTH)
            }),
            ("sound_balance", &|c: &mut Canvas| {
                let s = Sound { balance: 14, balance_drag: true, ..snd };
                sound::render(c, &theme, &fonts, &s, sound::ROW_BALANCE, 0)
            }),
            ("settings", &|c: &mut Canvas| settings::render(c, &theme, &fonts, 1, 0,
                &settings::SettingsView { night: theme.night, viz_name: "Bars", viz_size_label: "VEIL", usb_dac: false, battery_care: true, storage: "12.4 / 58 GB", sleep: "30 MIN", brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", boot_stock: "SONY", clock: "17 Aug · 09:01", accent: cinder_ui::Accent::Amber })),
            // The genre FILTER, both halves: the picker, and what a filtered Songs list looks like.
            // The shuffle band's caption has to follow the filter — shuffling a filtered list
            // shuffles what is on screen, so it must not still promise the whole library.
            ("genre_picker", &|c: &mut Canvas| {
                library::genre_render(c, &theme, &fonts, &lib, 0, false)
            }),
            ("library_songs_filtered", &|c: &mut Canvas| {
                let mut l = lib.clone();
                l.filter_genre = l.genres.first().map(|g| g.id);
                library::render(c, &theme, &fonts, library::Tab::Songs, 0, 0, 0, 0, None, &l, None, false);
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
            ("track_info", &|c: &mut Canvas| {
                let rows: Vec<(String, String)> = vec![
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
                cinder_ui::track_info::render(c, &theme, &fonts, &rows, 0, false)
            }),
            ("bluetooth", &|c: &mut Canvas| bluetooth::render(c, &theme, &fonts, &bt)),
            // The in-flight state this screen had no representation for at all: before, a connect
            // begun from Devices left this card reading "No device connected" until the link
            // resolved, which is what a failure looks like.
            ("bluetooth_connecting", &|c: &mut Canvas| {
                let b = Bt { on: true, connected: None, link_known: true, codec_sel: 0,
                             ldac_quality: 0, enhanced: true, enhanced_supported: true,
                             connecting: true, busy_phase: 0.35, link_codec: None };
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
                pairing::render(c, &theme, &fonts, &paired, &found, Some(1), None, true, 0.35)
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
                pairing::render(c, &theme, &fonts, &paired, &[], None, Some(1), false, 0.35)
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
                pairing::render(c, &theme, &fonts, &paired, &found, None, None, false, 0.0);
                pairing::render_prompt(c, &theme, &fonts,
                    &pairing::Prompt { kind: pairing::PROMPT_NUMERIC, name: "Pixel 8".into(), code: 428913 });
            }),
            ("receiver", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, true)),
            ("fm", &|c: &mut Canvas| fm::render(c, &theme, &fonts, 88.6)),
            ("usbdac", &|c: &mut Canvas| usbdac::render(c, &theme, &fonts, true, true, "LDAC", Some("WH-1000XM5"), "A1", true, Some((44100, 32, 2)), None)),
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
        let theme = Theme::day();
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
            settings::render(&mut c, &theme, &fonts, settings::ROW_RESTART, settings::max_scroll_px(),
                &settings::SettingsView { night: false, viz_name: "Bars", viz_size_label: "VEIL",
                    usb_dac: false, battery_care: true, storage: "12.4 / 58 GB", sleep: "30 MIN",
                    brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", boot_stock: "SONY", clock: "17 Aug · 09:01",
                    accent: cinder_ui::Accent::Amber });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            cinder_ui::confirm::render(&mut c, &theme, &fonts, ask);
            save(&c, &format!("confirm_{name}"));
        }

        // Settings mid-scroll — the header must survive it (device report, 2026-07-28).
        {
            let mut c = Canvas::new();
            settings::render(&mut c, &theme, &fonts, settings::ROW_BRIGHTNESS,
                settings::max_scroll_px() / 2,
                &settings::SettingsView { night: false, viz_name: "Bars", viz_size_label: "VEIL",
                    usb_dac: false, battery_care: true, storage: "12.4 / 58 GB", sleep: "30 MIN",
                    brightness: "4 / 5", screen_off: "OFF", auto_off: "OFF", boot_stock: "SONY", clock: "17 Aug · 09:01",
                    accent: cinder_ui::Accent::Amber });
            cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
            save(&c, "settings_scrolled");
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
            let nt = Theme::night();
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
        let theme = Theme::day_with(a);
        let lower = a.name().to_lowercase();

        let mut c = Canvas::new();
        now_playing::render(&mut c, &theme, &fonts, &np);
        cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
        save(&c, &format!("accent_{lower}_now_playing"));

        let mut c = Canvas::new();
        settings::render(&mut c, &theme, &fonts, settings::ROW_ACCENT, 0,
            &settings::SettingsView { night: false, viz_name: "Bars", viz_size_label: "VEIL", usb_dac: false,
                battery_care: true, storage: "12.4 / 58 GB", sleep: "30 MIN", brightness: "4 / 5",
                screen_off: "OFF", auto_off: "OFF", boot_stock: "SONY", clock: "17 Aug · 09:01", accent: a });
        cinder_ui::chrome::status_bar(&mut c, &theme, &fonts, "14:32", "FLAC 24/96", 78);
        save(&c, &format!("accent_{lower}_settings"));
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
                genre_id: (i as i64 % 3) + 1,
                is_hires: i % 6 == 0,
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
            .map(|(_, a, _)| ArtistRow { name: a.to_string(), albums: 1, tracks: 8, arts: vec![a.to_string()] , album_ids: Vec::new() })
            .collect();

        let mut app = App::unlocked();
        app.press(Button::Up);
        app.press(Button::Down);
        app.press(Button::Select);
        app.set_library(Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default(), genres: Vec::new(), ..Default::default() });
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

    // ── Swipe-to-queue, frame by frame ────────────────────────────────────────────────────────
    // The gesture used to act only on release: nothing moved, then a toast appeared. These frames
    // are the whole point of the change — the row travels with the finger, and the panel behind it
    // goes accent-coloured at exactly the travel where releasing will commit, so the gesture says
    // what it will do BEFORE you let go.
    {
        let theme = Theme::day();
        // A row in the middle of the Songs list, picked from the same geometry the renderer uses
        // rather than a literal — the frames have to sit on a real row or the reveal never shows.
        let row_y = library::list_top(Tab::Songs) + library::row_h(Tab::Songs) * 2 + 24;
        let travels = [0, 30, 60, 100, 160, 240];
        for (i, raw) in travels.iter().enumerate() {
            for (dir, tag) in [(1, "queue"), (-1, "play_next")] {
                let dx = library::swipe_offset(raw * dir);
                let mut c = Canvas::new();
                library::render(&mut c, &theme, &fonts, Tab::Songs, 99, 0, 0, 0, None, &lib,
                    Some(cinder_ui::library::SwipeRow { y: row_y, dx }), false);
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
        use cinder_ui::nav::{App, Screen};
        for pct in [80u32, 100, 120, 140] {
            cinder_ui::text::set_scale_pct(pct);
            for (name, screen) in [
                ("library", Screen::Library),
                ("settings", Screen::Settings),
                ("bluetooth", Screen::Bluetooth),
                ("sound", Screen::Sound),
                ("nowplaying", Screen::NowPlaying),
                ("upnext", Screen::UpNext),
                ("eq", Screen::Eq),
            ] {
                let mut app = App::unlocked();
                app.go_for_preview(screen);
                let mut c = Canvas::new();
                app.render(&mut c, &fonts, &np);
                save(&c, &format!("uiscale_{pct}_{name}"));
            }
            // Settings scrolled to the end, where the value column is densest.
            let mut app = App::unlocked();
            app.go_for_preview(Screen::Settings);
            app.scroll_px(10_000);
            let mut c = Canvas::new();
            app.render(&mut c, &fonts, &np);
            save(&c, &format!("uiscale_{pct}_settings_bottom"));
        }
        cinder_ui::text::set_scale_pct(100);
    }

    // Visualiser TYPES: render Now Playing with each viz kind (mid-animation) so they can be diffed.
    for k in 0..cinder_ui::viz::COUNT {
        let np_k = now_playing::NowPlaying { viz_seed: 1.7, viz_kind: k, ..np };
        let mut c = Canvas::new();
        now_playing::render(&mut c, &Theme::day(), &fonts, &np_k);
        save(&c, &format!("viz_{}_{}", k, cinder_ui::viz::name(k).to_lowercase()));
    }
}
