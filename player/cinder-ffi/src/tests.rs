/// The tests that put a `Render` into the process-global `cell()` take turns: `cargo test` runs
/// on several threads, and two of them installing their own fixture at once read each other's
/// state (seen on CI as a resume position of 0 where the fixture set 12000).
fn global_render() -> std::sync::MutexGuard<'static, ()> {
    static GLOBAL_RENDER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GLOBAL_RENDER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stock 1.02 has boot_complete.sh and /opt2 too; only Walkman One has /system/etc/.mod.
#[test]
fn stock_with_sonys_boot_script_is_not_walkman_one() {
    let root = std::env::temp_dir().join(format!("cinder_w1_{}", std::process::id()));
    std::fs::create_dir_all(root.join("sbin")).unwrap();
    std::fs::create_dir_all(root.join("opt2/appresouce")).unwrap();
    std::fs::write(root.join("sbin/boot_complete.sh"), "sleep 45\n").unwrap();
    assert!(!super::is_walkman_one(&root), "Sony's own 1.02 layout");
    std::fs::create_dir_all(root.join("system/etc/.mod/adler")).unwrap();
    assert!(super::is_walkman_one(&root), "Walkman One's mod tree");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_folder_lists_its_tracks_in_file_name_order_not_tag_order() {
    // Tags say B then A (track 1, 2); the files say A then B. A folder follows the files (#45).
    let t = |name: &str, title: &str, track: i32| {
        (
            cinder_db::Track {
                filename: format!("/contents/MUSIC/Album/{name}"),
                ..Default::default()
            },
            cinder_ui::model::SongRow {
                title: title.into(),
                track,
                ..Default::default()
            },
        )
    };
    let (tracks, songs): (Vec<_>, Vec<_>) = [t("02 b.flac", "Bee", 1), t("01 a.flac", "Ay", 2)]
        .into_iter()
        .unzip();
    let (folders, _) = super::build_folders(&tracks, &songs);
    let album = folders.iter().find(|f| f.name == "Album").unwrap();
    let titles: Vec<&str> = album.tracks.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, ["Ay", "Bee"]);
    assert_eq!(album.total, 2);
}

/// `cinder_advanced.conf` is read as the shell reads it (main.cpp `adv_on`).
#[test]
fn advanced_switches_read_like_the_shell() {
    let on = |text: &str| super::adv_on_in(text, "sony_bookmarks");
    assert!(on("sony_bookmarks\n"));
    assert!(on("  sony_bookmarks = on\r\n"));
    assert!(on("sony_bookmarks=1"));
    assert!(!on("#sony_bookmarks\n"));
    assert!(!on("sony_bookmarks=off"));
    assert!(!on("sony_bookmarks_extra\n"));
    assert!(
        !on("sony_bookmarks\nsony_bookmarks=no\n"),
        "the last line wins"
    );
}

/// A bookmark list's row draws no edit bar.
#[test]
fn a_read_only_list_is_not_a_users_row() {
    let store = super::playlists::Store {
        lists: vec![super::playlists::Playlist {
            id: -5,
            name: "Bookmark List 1".into(),
            read_only: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(!super::user_playlist_rows(&store, None)[0].user);
}

/// Tapping an album, playlist or channel plays it in order: shuffle goes off, and only a
/// change is saved (`play_in_order`).
#[test]
fn playing_in_order_turns_shuffle_off_once() {
    let mut np = super::Np {
        shuffle: true,
        ..Default::default()
    };
    assert!(
        super::shuffle_off(&mut np),
        "a change must be reported so it is saved"
    );
    assert!(!np.shuffle);
    assert!(!super::shuffle_off(&mut np), "already off: nothing to save");
    assert!(!np.shuffle);
}

/// Starting a book ends repeat all and repeat album, keeps repeat one, and reports only a change.
#[test]
fn a_book_ends_repeat_all_and_album_but_not_one() {
    for (was, now, changed) in [
        (2, 0, true),
        (REPEAT_ALBUM, 0, true),
        (1, 1, false),
        (0, 0, false),
    ] {
        let mut np = super::Np {
            repeat: was,
            ..Default::default()
        };
        assert_eq!(super::loop_off(&mut np), changed, "repeat {was}");
        assert_eq!(np.repeat, now, "repeat {was}");
    }
}

use super::*;
use crate::likes::liked_load;

/// "No filter is active" — what `Action::Shuffle` passes when the Library's filter row is on
/// ALL. A named function rather than a closure at each call site so the tests below read as
/// "this scope, unfiltered".
fn keep_all(_: &cinder_db::Track) -> bool {
    true
}

/// STOPWATCH, NOT A TEST. Opt-in profile of the boot-path library build, because the ~4.8 s
/// of dead time after boot (cinder_db_open at t=2.4 to "restore playback context" at t=7.2,
/// measured on device 2026-09-04) is a cost nobody has ever broken down. Threading it was
/// tried and reverted; the cheaper question — where does the time actually GO — was never
/// asked. Absolute numbers here are host numbers and mean nothing; the RATIO between the
/// phases is what transfers to the device.
///
///   CINDER_PROFILE_DB=artifacts/MTPDB_dev.dat \
///     cargo test -p cinder-ffi profile_library_build -- --nocapture --test-threads=1
///
/// Silently passes when the variable is unset, so it costs a normal `cargo test` nothing.
#[test]
fn profile_library_build() {
    let Ok(path) = std::env::var("CINDER_PROFILE_DB") else {
        return;
    };
    let t = |label: &str, d: std::time::Duration| {
        println!("  {label:<28} {:>9.1} ms", d.as_secs_f64() * 1000.0);
    };

    let t0 = std::time::Instant::now();
    let db = cinder_db::Db::open(&path).expect("open");
    let open = t0.elapsed();
    t("Db::open", open);

    // The individual queries build_library issues, timed on their own. Run BEFORE the full
    // build so SQLite's page cache is as cold for them as it is for the real first build.
    let t1 = std::time::Instant::now();
    let years = db.release_years();
    t("  db.release_years", t1.elapsed());

    let t1 = std::time::Instant::now();
    let songs_sorted = db.tracks(cinder_db::Sort::Title).unwrap_or_default();
    t("  db.tracks(Title)", t1.elapsed());

    let t1 = std::time::Instant::now();
    let album_order = db.tracks_album_order().unwrap_or_default();
    t("  db.tracks_album_order", t1.elapsed());

    let t1 = std::time::Instant::now();
    let albums = db.albums().unwrap_or_default();
    t("  db.albums", t1.elapsed());

    let t1 = std::time::Instant::now();
    let lib = build_library(&db);
    let build = t1.elapsed();
    t("build_library (whole)", build);

    println!(
        "  -> {} tracks, {} albums, {} artists, {} years, {} sorted, {} album-order, {} album rows",
        lib.songs.len(),
        lib.album_count(),
        lib.artists.len(),
        years.len(),
        songs_sorted.len(),
        album_order.len(),
        albums.len(),
    );
    t("TOTAL (open + build)", open + build);
}

/// FM state has to survive a reboot. A scan is a deliberate ten-second wait the user watches,
/// and the dial is where they left the radio — losing either is the same defect the shelf pins
/// had. This pins the on-disk shape, because the failure mode is silent: nothing errors, the
/// values simply come back as defaults.
#[test]
fn fm_dial_and_stations_survive_a_save() {
    let mut app = cinder_ui::nav::App::new();
    app.fm_report_khz(105_400);
    app.fm_set_stations(&[98_300, 106_200, 91_000]);

    // What settings_body writes for FM, built the same way it builds it.
    let mut body = format!("fm_khz={}\n", app.fm_khz());
    let st = app.fm_stations();
    body.push_str(&format!(
        "fm_stations={}\n",
        st.iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ));
    assert_eq!(body, "fm_khz=105400\nfm_stations=98300,106200,91000\n");

    // …and what the loader makes of it again.
    let mut back = cinder_ui::nav::App::new();
    for line in body.lines() {
        let (k, v) = line.split_once('=').unwrap();
        match k {
            "fm_khz" => back.fm_report_khz(v.parse().unwrap()),
            "fm_stations" => {
                let list: Vec<i32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
                back.fm_set_stations(&list);
            }
            _ => {}
        }
    }
    assert_eq!(back.fm_khz(), 105_400);
    assert_eq!(back.fm_stations(), &[98_300, 106_200, 91_000]);
}

/// A hand-edited or truncated config must not put the tuner outside the band, and must not stop
/// a boot. Out-of-band entries are dropped; the dial clamps.
#[test]
fn fm_settings_reject_garbage_without_failing() {
    let mut app = cinder_ui::nav::App::new();
    let list: Vec<i32> = "0,87400,98300,not_a_number,108100,106200"
        .split(',')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .filter(|k| (cinder_ui::fm::MIN_KHZ..=cinder_ui::fm::MAX_KHZ).contains(k))
        .collect();
    assert_eq!(
        list,
        vec![98_300, 106_200],
        "out-of-band and unparseable both dropped"
    );
    app.fm_set_stations(&list);
    assert_eq!(app.fm_stations(), &[98_300, 106_200]);

    app.fm_report_khz(999_999);
    assert_eq!(
        app.fm_khz(),
        cinder_ui::fm::MAX_KHZ,
        "clamped, not stored raw"
    );
}

/// A stopped analyzer must not leave its last frame on screen. The bars fall to nothing and the
/// buffer is dropped, so the visualiser goes absent rather than holding a snapshot — the same
/// reason the synthetic animation was removed.
/// The panic hook's screen table must line up with `screen_ord`, or a crash report names the
/// wrong screen — which is worse than naming none, because it sends the reader somewhere else.
///
/// This caught a real one: `BtCodec` had ordinal 25 against a 25-entry table, so a panic on
/// the codec screen indexed past the end and reported no screen at all. The list below is the
/// part that has to be kept complete — hence the length assertion.
#[test]
fn every_screen_has_a_distinct_panic_name() {
    use cinder_ui::nav::Screen as S;
    let all = [
        S::Lock,
        S::NowPlaying,
        S::Menu,
        S::Library,
        S::Album,
        S::Artist,
        S::Playlist,
        S::UpNext,
        S::Eq,
        S::Sound,
        S::Bluetooth,
        S::Settings,
        S::Fm,
        S::UsbDac,
        S::Receiver,
        S::Onboarding,
        S::UsbStorage,
        S::Shelf,
        S::Pairing,
        S::GenreFilter,
        S::TrackInfo,
        S::Folders,
        S::ClockSet,
        S::Advanced,
        S::Tone,
        S::BtCodec,
        S::Keyboard,
        S::PlaylistPick,
        S::TrackPick,
        S::Device,
        S::VizSet,
        S::Lyrics,
        S::Search,
        S::SensMe,
        S::Display,
        S::Palette,
        S::Help,
        S::DacEq,
        S::PlaylistEdit,
        S::ViewEdit,
        S::Soundscape,
        S::Nc,
        S::Presets,
        S::Books,
        S::Language,
        S::Zappin,
        S::Skins,
        S::VolCurve,
        S::Storage,
        S::Wifi,
        S::WifiNetworks,
        S::NetRadio,
        S::WifiRemote,
        S::WifiOptions,
        S::WifiSpeakers,
        S::RadioFind,
        S::MediaServers,
        S::Alarm,
        S::PlaylistCover,
    ];
    // One ordinal (42) is spare — see SCREEN_NAMES.
    const RESERVED: usize = 1;
    assert_eq!(
        all.len() + RESERVED,
        SCREEN_NAMES.len(),
        "table and variant list disagree"
    );
    let mut seen = std::collections::BTreeSet::new();
    for sc in all {
        let i = screen_ord(sc) as usize;
        assert!(
            i < SCREEN_NAMES.len(),
            "{sc:?} maps past the end of the name table"
        );
        assert!(
            seen.insert(i),
            "{sc:?} shares an ordinal with another screen"
        );
    }
}

/// The optional profile members (2026-10-04) survive a save when they are followed, and a
/// spare's copy of one that is NOT followed is replaced by the live value — so a hand-edited
/// or stale `bank_mono` cannot make mono differ between outputs behind the owner's back.
#[test]
fn followed_members_survive_a_save_and_unfollowed_ones_are_shared() {
    use cinder_ui::nav::{SoundSetup, FOLLOW_DAC_EQ, FOLLOW_MONO};
    let spare = SoundSetup {
        mono: true,
        linear_amp: true,
        dac_eq: [1, 2, 3, -4, 5],
        ..SoundSetup::default()
    };
    let body = format!("setup=0\n{}", setup_body(&spare));
    assert!(
        body.contains("bank_mono=1\nbank_amp=1\nbank_dac_eq=1,2,3,-4,5\n"),
        "{body}"
    );
    let load = |follow: u8| {
        let mut pl = ProfileLoad::default();
        for line in body.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(pl.line(k, v), "{k} is a profile key and was not taken");
        }
        let mut app = cinder_ui::nav::App::new();
        app.set_profile_follow(follow);
        pl.install(&mut app);
        app.setup_inactive()
    };
    let got = load(FOLLOW_MONO | FOLLOW_DAC_EQ);
    assert_eq!(
        (got.mono, got.linear_amp, got.dac_eq),
        (true, false, [1, 2, 3, -4, 5])
    );
    let got = load(0);
    assert_eq!(
        (got.mono, got.linear_amp, got.dac_eq),
        (false, false, [0; 5]),
        "none followed: all live"
    );
}

/// Both sound profiles and the per-output choice survive a save and a load — the whole of each,
/// Advanced included — and the live one comes back as the JACK's, because that is the output a
/// boot starts on.
#[test]
fn both_profiles_and_the_output_map_survive_a_save() {
    use cinder_ui::nav::SoundSetup;
    let spare = SoundSetup {
        dsee: true,
        vpt: true,
        vpt_mode: 2,
        dc: true,
        dc_type: 4,
        clear: false,
        balance: 62,
        eq_preset: 0,
        eq_bands: [1, -2, 3, -4, 5, -6, 7, -8, 9, -10],
        src_direct: true,
        clear_phase: true,
        dsee_custom: true,
        dsee_mode: 3,
        vinyl_type: 2,
        tone: true,
        tone_bands: [4, -6, 8],
        ..SoundSetup::default()
    };
    let body = format!(
        "setup=0\n{}{}jack_eq=10band\nprofile_bt=b\n",
        setup_body(&spare),
        presets_body(&[spare, SoundSetup::default()], &["Late night".into()])
    );

    let mut pl = ProfileLoad::default();
    for line in body.lines() {
        let (k, v) = line.split_once('=').unwrap();
        assert!(pl.line(k, v), "{k} is a profile key and was not taken");
    }
    assert!(
        !pl.line("eq", "0,0"),
        "the live profile's keys are not ProfileLoad's"
    );
    let mut app = cinder_ui::nav::App::new();
    pl.install(&mut app);
    assert_eq!(app.setup_inactive(), spare);
    assert_eq!(app.setup_idx(), 0);
    assert_eq!(app.presets(), &[spare, SoundSetup::default()]);
    assert_eq!(
        app.preset_names(),
        &["Late night".to_string(), String::new()]
    );
    assert!(!app.jack_eq_dac());
}

/// An album of one long file with a sheet beside it lists the sheet's tracks with their
/// lengths; a file of the same album without one keeps its own row; an album with no sheet
/// lists nothing extra.
#[test]
fn cue_rows_come_from_the_sheet_beside_a_long_file() {
    use cinder_ui::model::SongRow;
    let dir = std::env::temp_dir().join(format!("cinder-cuerows-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let flac = dir.join("Album.flac");
    std::fs::write(&flac, b"").unwrap();
    std::fs::write(
            dir.join("Album.cue"),
            "FILE \"Album.flac\" WAVE\n TRACK 01 AUDIO\n  TITLE \"One\"\n  INDEX 01 00:00:00\n TRACK 02 AUDIO\n  TITLE \"Two\"\n  INDEX 01 10:00:00\n",
        )
        .unwrap();
    let row = |id: i64, t: &str| SongRow {
        title: t.into(),
        object_id: id,
        dur: "3:00".into(),
        ..Default::default()
    };
    let mut long = std::collections::BTreeMap::new();
    long.insert(1, (flac.to_str().unwrap().to_string(), 25 * 60 * 1000));
    let rows = cue_rows(&[row(1, "Album"), row(2, "Bonus")], &long);
    let none = cue_rows(&[row(2, "Bonus")], &long);
    std::fs::remove_dir_all(&dir).ok();
    let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, ["One", "Two", "Bonus"]);
    assert_eq!((rows[1].file, rows[1].start_ms), (0, 600_000));
    assert_eq!(rows[0].dur, fmt_time(600_000));
    assert_eq!(
        rows[1].dur,
        fmt_time(15 * 60 * 1000),
        "the last runs to the file's end"
    );
    assert_eq!((rows[2].file, rows[2].dur.as_str()), (1, "3:00"));
    assert!(none.is_empty());
}

/// Track info ▸ ALBUM SOUND: the albums' presets and the sound an album's preset replaced come
/// back after a restart, so leaving the album still puts that sound back.
#[test]
fn album_sounds_survive_a_save() {
    use cinder_ui::nav::SoundSetup;
    let prev = SoundSetup {
        dsee: true,
        balance: 40,
        ..SoundSetup::default()
    };
    let body = format!(
        "{}{}album_sound=1\tKind of Blue\u{1f}Miles Davis\n",
        presets_body(&[SoundSetup::default()], &[]),
        setup_body(&prev).replace("bank_", "album_prev_"),
    );
    let mut pl = ProfileLoad::default();
    for line in body.lines() {
        let (k, v) = line.split_once('=').unwrap();
        assert!(pl.line(k, v), "{k} was not taken");
    }
    let mut app = cinder_ui::nav::App::new();
    pl.install(&mut app);
    assert_eq!(
        app.album_sounds(),
        &[("Kind of Blue\u{1f}Miles Davis".to_string(), 1)]
    );
    assert_eq!(app.album_sound_prev(), Some(&prev));
}

/// No `jack_eq` key: the DAC, unless the file's live 10-band curve is not flat.
#[test]
fn an_older_file_keeps_a_10_band_curve_on_the_jack() {
    for (eq, want) in [([0i8; 10], true), ([4; 10], false)] {
        let mut app = cinder_ui::nav::App::new();
        app.set_eq_bands(eq);
        ProfileLoad::default().install(&mut app);
        assert_eq!(app.jack_eq_dac(), want);
    }
}

/// A settings file from before R5 has a spare with no Advanced values. Advanced was shared by
/// A and B then, so the spare takes the live ones — the upgrade must not change what B sounds
/// like. Garbage values are ignored and out-of-range gains are clamped, never passed on.
#[test]
fn an_older_file_and_a_damaged_one_both_load() {
    let mut app = cinder_ui::nav::App::new();
    app.set_adv_flags(0b1_0001); // Source Direct + Tone Control, set before profiles existed
    app.set_tone_bands([6, 0, -6]);
    app.set_vinyl_type(3);
    let mut pl = ProfileLoad::default();
    for (k, v) in [
        ("setup", "0"),
        ("bank_sound", "1"),
        ("bank_balance", "50"),
        ("bank_preset", "3"),
    ] {
        pl.line(k, v);
    }
    pl.install(&mut app);
    let b = app.setup_inactive();
    assert!(
        b.src_direct && b.tone && b.dsee,
        "the spare lost the shared Advanced values"
    );
    assert_eq!((b.tone_bands, b.vinyl_type), ([6, 0, -6], 3));

    let mut pl = ProfileLoad::default();
    for (k, v) in [
        ("bank_sound", "zz"),
        ("bank_tone", "99,-99,x"),
        ("bank_eq", "127,-128"),
        ("profile_bt", "q"),
        ("profile_usb", "B"),
        ("bank_adv", "255"),
        ("bank_dsee_mode", "999"),
    ] {
        assert!(pl.line(k, v));
    }
    assert_eq!(
        pl.bank.tone_bands,
        [cinder_ui::tone::BAND_MAX, -cinder_ui::tone::BAND_MAX, 0]
    );
    assert_eq!(
        &pl.bank.eq_bands[..2],
        &[crate::EQ_BAND_MAX, -crate::EQ_BAND_MAX]
    );
    assert_eq!(
        pl.bank.adv_bits(),
        0b1_1111,
        "bit 5 (the amp) is not a profile value"
    );
    // The out-of-range mode is clamped when the spare goes live.
    let mut app = cinder_ui::nav::App::new();
    pl.bank_seen = true;
    pl.install(&mut app);
    app.select_setup(1);
    assert_eq!(app.dsee_mode(), cinder_ui::advanced::DSEE_MODES.len() - 1);
}

/// Settings come from the mirror when the file cannot be read, and a good read refreshes it.
#[test]
fn settings_fall_back_to_the_mirror() {
    let dir = std::env::temp_dir().join(format!("cinder_mirror_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (file, mirror) = (dir.join("a.conf"), dir.join("m.conf"));
    let (f, m) = (file.to_str().unwrap(), mirror.to_str().unwrap());
    std::fs::write(&file, "volume=40\n").unwrap();
    assert_eq!(read_settings(f, Some(m)).unwrap(), "volume=40\n");
    assert_eq!(std::fs::read_to_string(&mirror).unwrap(), "volume=40\n");
    std::fs::remove_file(&file).unwrap();
    assert_eq!(read_settings(f, Some(m)).unwrap(), "volume=40\n");
    assert!(read_settings(f, None).is_err());
    assert_eq!(settings_mirror("/tmp/x.conf"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The settings write leaves the old file or the new one, never part of one, and no temporary
/// file behind.
#[test]
fn the_settings_write_is_atomic_and_leaves_nothing_behind() {
    let dir = std::env::temp_dir().join(format!("cinder_atomic_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cinder_settings.conf");
    let p = path.to_str().unwrap();
    write_atomic(p, "one\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\n");
    write_atomic(p, "two, and longer\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "two, and longer\n");
    assert!(!dir.join("cinder_settings.conf.tmp").exists());
    // A write that cannot happen leaves the old file exactly as it was.
    let missing = dir.join("no_such_dir").join("x.conf");
    assert!(write_atomic(missing.to_str().unwrap(), "x").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "two, and longer\n");
    let _ = std::fs::remove_dir_all(&dir);
}

// Run the global-FFI roundtrip in an isolated test process. The normal test suite has other
// cases that use `cell()` concurrently; a subprocess lets this one exercise the actual FFI
// load/get/save path without racing other tests that use the global Render.
/// Run one `_child` test of this module in a process of its own: the FFI keeps its Render (and
/// the UI scale and fonts) in process globals, which other tests on other threads also set.
fn run_child(name: &str, env: &str) {
    let exe = std::env::current_exe().unwrap();
    let mut command = match std::env::var_os("CINDER_FFI_TEST_RUNNER").filter(|r| !r.is_empty()) {
        Some(runner) => {
            let mut c = std::process::Command::new(runner);
            c.arg(exe);
            c
        }
        None => std::process::Command::new(exe),
    };
    let output = command
        .args(["--exact", name, "--nocapture"])
        .env(env, "1")
        .env("CINDER_FFI_HEADLESS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// EVERY SAVED SETTING COMES BACK. Each value in the settings file is changed (0 and 1 swap,
/// other numbers move by one, `on`/`off` swap), loaded, and saved again: a key whose changed
/// value does not survive is one the loader never reads, or reads into something the saver
/// does not write — a setting that quietly resets on every restart.
#[test]
fn every_saved_setting_survives_a_restart() {
    run_child(
        "tests::every_saved_setting_survives_a_restart_child",
        "CINDER_SETTINGS_RT_CHILD",
    );
}

#[test]
fn every_saved_setting_survives_a_restart_child() {
    if std::env::var_os("CINDER_SETTINGS_RT_CHILD").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("cinder_settings_rt_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cinder_settings.conf");
    let path_c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(cinder_render_init(), 0);
    let body = |_: ()| settings_body(cell().lock().unwrap().as_ref().unwrap());
    let before = body(());
    let changed: Vec<(String, String)> = before
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            let w = match v {
                "0" => "1".to_string(),
                "1" => "0".to_string(),
                "on" => "off".to_string(),
                "off" => "on".to_string(),
                _ => match v.parse::<i64>() {
                    Ok(n) => (n + 1).to_string(),
                    Err(_) => v.to_string(),
                },
            };
            (k.to_string(), w)
        })
        .collect();
    let text: String = changed.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(&path, text).unwrap();
    assert_eq!(
        cinder_settings_load(path_c.as_ptr()) & 1,
        1,
        "the file was read"
    );
    let after = body(());
    let got: std::collections::HashMap<&str, &str> =
        after.lines().filter_map(|l| l.split_once('=')).collect();
    // Not restored, on purpose: every boot starts in day; the two timeouts snap to a preset
    // (a changed value is off the list); a profile member not followed takes the live value;
    // the swipe keys are action names, so `on` is not one.
    const BY_DESIGN: &[&str] = &[
        "night",
        "screen_off",
        "auto_off",
        "bank_amp",
        "np_swipe_up",
        "np_swipe_down",
    ];
    let lost: Vec<String> = changed
        .iter()
        .filter(|(k, _)| !BY_DESIGN.contains(&k.as_str()))
        .filter(|(k, v)| got.get(k.as_str()) != Some(&v.as_str()))
        .map(|(k, v)| format!("{k}: wrote {v}, read back {:?}", got.get(k.as_str())))
        .collect();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        lost.is_empty(),
        "settings that reset on a restart:\n{}",
        lost.join("\n")
    );
}

#[test]
fn receiver_codec_settings_ffi_roundtrip() {
    let exe = std::env::current_exe().unwrap();
    let mut command = if let Some(runner) =
        std::env::var_os("CINDER_FFI_TEST_RUNNER").filter(|runner| !runner.is_empty())
    {
        let mut command = std::process::Command::new(runner);
        command.arg(exe);
        command
    } else {
        std::process::Command::new(exe)
    };
    let output = command
        .args([
            "--exact",
            "tests::receiver_codec_settings_ffi_roundtrip_child",
            "--nocapture",
        ])
        .env("CINDER_RX_CODEC_FFI_CHILD", "1")
        .env("CINDER_FFI_HEADLESS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated FFI roundtrip failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn receiver_codec_settings_ffi_roundtrip_child() {
    if std::env::var_os("CINDER_RX_CODEC_FFI_CHILD").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("cinder_rx_codec_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cinder_settings.conf");
    let path_c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();

    assert_eq!(cinder_render_init(), 0, "headless Render initializer");
    std::fs::write(&path, "bt_codec=1\nbt_ldac_quality=2\nrx_codec=2\n").unwrap();
    assert_eq!(cinder_settings_load(path_c.as_ptr()), 1);
    assert_eq!(cinder_get_rx_codec_pref(), 2);
    assert_eq!(cinder_get_bt_codec(), 1);
    assert_eq!(cinder_get_bt_ldac_quality(), 2);
    {
        let mut guard = cell().lock().unwrap();
        let r = guard.as_mut().unwrap();
        let body = settings_body(r);
        assert!(body.lines().any(|line| line == "rx_codec=2"));
        assert!(body.lines().any(|line| line.starts_with("bt_codec=")));
        assert!(body
            .lines()
            .any(|line| line.starts_with("bt_ldac_quality=")));
        assert_eq!(
            carry_action(r, &cinder_ui::nav::Action::RxCodecChanged),
            Some(58)
        );
    }

    {
        let mut guard = cell().lock().unwrap();
        let r = guard.as_mut().unwrap();
        r.app.set_rx_codec_pref(3);
        save_settings(r);
    }
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.lines().any(|line| line == "rx_codec=3"));
    assert!(saved.lines().any(|line| line == "bt_codec=1"));
    assert!(saved.lines().any(|line| line == "bt_ldac_quality=2"));

    // A pre-feature file and malformed/out-of-range entries all resolve to Auto on a fresh App.
    for contents in [
        "bt_codec=1\nbt_ldac_quality=2\n",
        "rx_codec=bogus\n",
        "rx_codec=255\n",
    ] {
        std::fs::write(&path, contents).unwrap();
        cinder_render_shutdown();
        assert_eq!(cinder_render_init(), 0, "headless Render reinitialization");
        assert_eq!(cinder_settings_load(path_c.as_ptr()), 1);
        assert_eq!(cinder_get_rx_codec_pref(), 0, "config: {contents:?}");
    }

    // Big view is the default: on with no `big_view` line (first boot, older file); a saved
    // `big_view=0` is kept. The file has no other keys, so nothing else is in play.
    let big = || cell().lock().unwrap().as_ref().unwrap().app.big_view();
    assert!(big(), "no big_view line = Big view");
    std::fs::write(&path, "big_view=0\n").unwrap();
    assert_eq!(cinder_settings_load(path_c.as_ptr()), 1);
    assert!(!big(), "a saved big_view=0 is kept");
    cinder_render_shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The SensMe grid names the time-of-day channel from a table in cinder-ui, which cannot see
/// cinder-db's. Hold the two together here, where both are in reach.
#[test]
fn the_time_of_day_names_match_the_channel_table() {
    for (id, name) in cinder_ui::model::SENSME_TIME_NAMES {
        assert_eq!(
            cinder_db::SENSME_CHANNELS[id as usize],
            name,
            "channel {id}"
        );
    }
}

#[test]
fn a_stale_spectrum_decays_away_instead_of_freezing() {
    let mut lv = vec![1.0f32, 0.5, 0.25];
    // One 100 ms frame: every bar steps down by 100/400 = 0.25.
    assert!(viz_decay_levels(&mut lv, 100));
    assert!((lv[0] - 0.75).abs() < 1e-6, "got {lv:?}");
    assert!((lv[1] - 0.25).abs() < 1e-6, "got {lv:?}");
    assert!(
        lv[2] <= 0.0,
        "the smallest bar should have bottomed out: {lv:?}"
    );
    // Keep going: it must reach empty, not hover just above zero forever.
    for _ in 0..10 {
        viz_decay_levels(&mut lv, 100);
    }
    assert!(lv.is_empty(), "bars never cleared: {lv:?}");
}

/// Nothing to decay = nothing to repaint. A visualiser that reported "changed" every frame
/// while empty would defeat the dirty-flag render and repaint a static screen forever.
#[test]
fn decaying_an_empty_spectrum_is_not_a_change() {
    let mut lv: Vec<f32> = Vec::new();
    assert!(!viz_decay_levels(&mut lv, 100));
    let mut lv = vec![0.0f32; 4];
    assert!(
        viz_decay_levels(&mut lv, 100),
        "all-zero bars still need clearing once"
    );
    assert!(lv.is_empty());
    assert!(!viz_decay_levels(&mut lv, 100), "and then never again");
}

/// A long stall — the panel was dark for an hour — must collapse the bars in one step, not
/// produce a negative level or a spike when the screen comes back.
#[test]
fn a_huge_frame_gap_collapses_the_bars_cleanly() {
    let mut lv = vec![1.0f32, 0.4];
    assert!(viz_decay_levels(&mut lv, 3_600_000));
    assert!(lv.is_empty(), "a long gap should clear outright: {lv:?}");
}

#[test]
fn time_formatting() {
    assert_eq!(fmt_time(0), "0:00");
    assert_eq!(fmt_time(7000), "0:07");
    assert_eq!(fmt_time(272_000), "4:32");
    assert_eq!(fmt_time(3_661_000), "1:01:01");
    assert_eq!(fmt_time(-5), "0:00"); // clamps
}

#[test]
fn codec_from_extension() {
    assert_eq!(
        codec_label("/music/x.flac", true),
        ("FLAC · Hi-Res".into(), "FLAC HR".into())
    );
    assert_eq!(
        codec_label("/music/x.mp3", false),
        ("MP3".into(), "MP3".into())
    );
    assert_eq!(
        codec_label("/music/noext", false),
        ("PCM".into(), "PCM".into())
    );
    assert_eq!(
        codec_label("/a/b.DSF", true),
        ("DSF · Hi-Res".into(), "DSF HR".into())
    );
}

#[test]
fn track_fills_now_playing_and_times() {
    let t = cinder_db::Track {
        genre_id: None,
        object_id: 1,
        title: "Atlas Hands".into(),
        artist: "Benjamin Francis Leftwich".into(),
        album_artist: "Benjamin Francis Leftwich".into(),
        album: "Last Smoke".into(),
        filename: "/music/atlas.flac".into(),
        disc_no: 1,
        track_no: 1,
        duration_raw: Some(272_000),
        is_hires: true,
        album_id: Some(10),
        othumb_id: Some(100),
        added: 5000,
        releaseyear_id: Some(30),
    };
    let mut np = Np::default();
    apply_track(&mut np, &t);
    set_progress(&mut np, 136_000, 272_000); // 50% of 4:32
    assert_eq!(np.title, "Atlas Hands");
    assert_eq!(np.artist, "Benjamin Francis Leftwich");
    assert_eq!(np.badge, "FLAC HR");
    assert_eq!(np.elapsed, "2:16"); // 50% of 4:32
    assert_eq!(np.remaining, "-2:16");
    assert!((np.progress - 0.5).abs() < 1e-6);
}

fn fixture_db() -> cinder_db::Db {
    cinder_db::Db::open_in_memory_with(|conn| {
            conn.execute_batch(
                r#"
            CREATE TABLE albums  (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE artists (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT, imagefile TEXT, face_x INTEGER, face_y INTEGER, face_w INTEGER, face_h INTEGER);
            CREATE TABLE schema  (prop_type INTEGER, akey INTEGER, data_type INTEGER, prop_name TEXT, PRIMARY KEY(prop_type,akey));
            CREATE TABLE object_ext_int (object_id INTEGER, akey INTEGER, value INTEGER DEFAULT 0, PRIMARY KEY(object_id,akey));
            CREATE TABLE images  (id INTEGER PRIMARY KEY, dataform INTEGER, dataoffset INTEGER, datasize INTEGER, value TEXT, digest TEXT, bmpfile TEXT, bmpwidth INTEGER, bmpheight INTEGER);
            CREATE TABLE releaseyears (id INTEGER PRIMARY KEY, initial INTEGER, sort_str TEXT, search_str TEXT, value TEXT);
            CREATE TABLE object_body (
                object_id INTEGER PRIMARY KEY AUTOINCREMENT, object_type INTEGER NOT NULL,
                parent_id INTEGER, reference_id INTEGER,
                child_index INTEGER, media_type INTEGER DEFAULT 0, format INTEGER DEFAULT 0,
                initial INTEGER, sort_str TEXT, search_str TEXT, title TEXT DEFAULT "",
                addedtime INTEGER DEFAULT 0, filename TEXT, filesize INTEGER,
                series_no INTEGER, disc_no INTEGER, is_high_resolution INTEGER,
                album_id INTEGER, artist_id INTEGER, albumartist_id INTEGER, releaseyear_id INTEGER, othumb_id INTEGER, mthumb_id INTEGER);
            INSERT INTO albums  VALUES (10,0,'last smoke','last smoke','Last Smoke');
            INSERT INTO albums  VALUES (11,0,'harvest','harvest','Harvest Moon');
            INSERT INTO artists VALUES (20,0,'leftwich','leftwich','Benjamin Francis Leftwich',NULL,0,0,0,0);
            INSERT INTO artists VALUES (21,0,'cold','cold','Cold Stone & Sea',NULL,0,0,0,0);
            INSERT INTO schema  VALUES (1,7,2,'DURATION');
            INSERT INTO releaseyears VALUES (30,0,'2012','2012','2012');
            INSERT INTO releaseyears VALUES (31,0,'1992','1992','1992');
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (1,1,1,'Atlas Hands','/music/atlas.flac',1,1,1,10,20,30,5000);
            UPDATE object_body SET albumartist_id=20 WHERE object_id=1;
            -- A GUEST on Last Smoke: its TRACK artist is someone else, its ALBUM artist is not.
            -- Grouping by track artist is what shattered compilations on the real device.
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,albumartist_id,releaseyear_id,addedtime)
              VALUES (2,1,1,'Box of Stones','/music/box.flac',2,1,1,10,21,20,30,5001);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,releaseyear_id,addedtime)
              VALUES (3,1,1,'Harvest Moon','/music/harvest.flac',1,1,0,11,21,31,4000);
            UPDATE object_body SET albumartist_id=21 WHERE object_id=3;
            -- non-audio rows that must NOT appear in the library (folder mt=0, cover image mt=3)
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,album_id)
              VALUES (7,3,0,'MUSIC','MUSIC',NULL);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,album_id)
              VALUES (8,2,3,'Cover','/music/Cover.jpg',10);
            -- A playlist container + its entries (Sony's real shape: type-1 container, type-3
            -- entries referencing tracks by object_id, ordered by child_index). Object 7 above is
            -- a type-3 row with no parent — an orphan, which must not become a ghost playlist.
            INSERT INTO object_body (object_id,object_type,parent_id,media_type,title,filename) VALUES (60,1,0,0,'Night Bus',NULL);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (61,3,60,3,0);
            INSERT INTO object_body (object_id,object_type,parent_id,reference_id,child_index) VALUES (62,3,60,1,1);
            INSERT INTO object_ext_int VALUES (1,7,272000);
            INSERT INTO object_ext_int VALUES (2,7,272000);
            INSERT INTO object_ext_int VALUES (3,7,272000);
            -- SensMe, as the stock scanner writes it once a PC tool has tagged the files: track 1
            -- in channels 0 and 8, track 3 in channel 0. Track 2 was never analysed.
            INSERT INTO schema VALUES (1,50,119,'WMCHANNELINFO');
            INSERT INTO schema VALUES (1,52,2,'SENSMETEMPO');
            INSERT INTO schema VALUES (1,60,119,'SABI');
            INSERT INTO object_ext_int VALUES (1,50,515);
            INSERT INTO object_ext_int VALUES (1,52,131);
            INSERT INTO object_ext_int VALUES (1,60,75763);
            INSERT INTO object_ext_int VALUES (3,50,3);
            INSERT INTO object_ext_int VALUES (3,52,91);
            "#,
            )
        })
        .unwrap()
}

/// The EDITABLE row must win a name collision.
///
/// Our playlists are `.m3u8` files; Sony's scanner indexes those same files into the media DB
/// under the same names, so every playlist the user owns appears in BOTH lists. Whichever the
/// merge keeps is the one the UI gets, and only ours carries `user: true` — which is what makes
/// the page draw + TRACKS / RENAME / DELETE at all.
///
/// Sony's used to come first, so every one of the reference device's 8 playlists arrived
/// read-only and nothing on the device could be edited.
#[test]
fn our_editable_playlist_beats_sonys_copy_of_it() {
    let row = |id: i64, name: &str, tracks: u32, user: bool| cinder_ui::model::PlaylistRow {
        id,
        name: name.to_string(),
        tracks,
        art: name.to_string(),
        user,
        track_list: Vec::new(),
        ..Default::default()
    };
    // Same names, different case, as the de-dup folds case.
    let ours = vec![
        row(-1, "Late Night On The Bus Mix 08", 24, true),
        row(-2, "Teef", 24, true),
    ];
    let sonys = [
        row(33582, "late night on the bus mix 08", 24, false),
        row(33406, "TEEF", 24, false),
        row(33431, "Hi-fi", 22, false), // Sony-only: no file of ours, so it must survive
    ];

    let merged = merge_playlist_rows(ours, &sonys);
    assert_eq!(merged.len(), 3, "one row per name, not per source");

    let by = |n: &str| {
        merged
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(n))
            .unwrap()
            .clone()
    };
    assert!(
        by("Teef").user,
        "the collision must keep OUR row, or it cannot be edited"
    );
    assert_eq!(
        by("Teef").id,
        -2,
        "and keep our id, which is what writes the file back"
    );
    assert!(by("Late Night On The Bus Mix 08").user);
    // A playlist only Sony knows about is still listed — just not editable.
    assert!(!by("Hi-fi").user);

    assert!(
        merged
            .windows(2)
            .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()),
        "still sorted by name"
    );
}

#[test]
fn build_library_from_db() {
    let db = fixture_db();
    let lib = build_library(&db);
    assert_eq!(lib.songs.len(), 3);
    assert_eq!(lib.album_count(), 2);
    assert_eq!(lib.artists.len(), 2);
    // albums grouped under their artist
    let bfl = lib
        .album_groups
        .iter()
        .find(|g| g.artist == "Benjamin Francis Leftwich")
        .unwrap();
    assert_eq!(bfl.albums.len(), 1);
    assert_eq!(bfl.albums[0].name, "Last Smoke");
    assert_eq!(bfl.albums[0].tracks, 2);
    // release year resolved from releaseyears + newest addedtime carried for the ORDER chip
    assert_eq!(bfl.albums[0].year, "2012");
    assert_eq!(bfl.albums[0].added, 5001);
    // the song rows carry the sort keys used by the Songs SORT chip
    let atlas_row = lib.songs.iter().find(|s| s.title == "Atlas Hands").unwrap();
    assert_eq!(atlas_row.year, 2012);
    assert_eq!(atlas_row.album_id, 10);
    // the resolved song carries the object_id the shell plays
    let atlas = lib.songs.iter().find(|s| s.title == "Atlas Hands").unwrap();
    assert_eq!(atlas.object_id, 1);
    // (duration formatting is covered by `track_fills_now_playing_and_times`; the
    // in-memory fixture can't exercise it because Db caches the DURATION akey at open,
    // before this test populates the schema table.)
    // artist track counts
    let bfl_artist = lib
        .artists
        .iter()
        .find(|a| a.name == "Benjamin Francis Leftwich")
        .unwrap();
    assert_eq!(bfl_artist.tracks, 2);
    assert_eq!(bfl_artist.albums, 1);
}

/// The fixture library plus three audiobook chapters in two books by one author, filed the way
/// Sony's scanner files them: under a `LEARNING` folder at the root of internal storage.
fn fixture_db_with_books() -> cinder_db::Db {
    let db = fixture_db();
    db.conn()
            .execute_batch(
                r#"
            INSERT INTO albums  VALUES (12,0,'book a','book a','Book A');
            INSERT INTO albums  VALUES (13,0,'book b','book b','Book B');
            INSERT INTO artists VALUES (22,0,'author one','author one','Author One',NULL,0,0,0,0);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,albumartist_id,addedtime)
              VALUES (101,1,1,'Chapter 2','/contents/LEARNING/Author One/Book A/02.mp3',2,1,0,12,22,22,6000);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,albumartist_id,addedtime)
              VALUES (102,1,1,'Chapter 1','/contents/LEARNING/Author One/Book A/01.mp3',1,1,0,12,22,22,6000);
            INSERT INTO object_body (object_id,object_type,media_type,title,filename,series_no,disc_no,is_high_resolution,album_id,artist_id,albumartist_id,addedtime)
              VALUES (103,1,1,'Prologue','/contents/LEARNING/Author One/Book B/01.mp3',1,1,0,13,22,22,6001);
            "#,
            )
            .unwrap();
    db
}

/// Audiobooks have their own screen. They must not turn up in Songs, Albums or Artists — the
/// music library is exactly what it was without them.
#[test]
fn audiobooks_stay_out_of_the_music_library() {
    let lib = build_library(&fixture_db_with_books());
    assert_eq!(lib.songs.len(), 3, "no chapter in Songs");
    assert_eq!(lib.album_count(), 2, "no book among the albums");
    assert_eq!(lib.artists.len(), 2, "no author among the artists");
    assert!(!lib.songs.iter().any(|s| s.title.starts_with("Chapter")));
}

/// The same rows, kept: grouped one book per folder, chapters in file order, by author.
#[test]
fn audiobooks_are_grouped_one_book_per_folder() {
    let lib = build_library(&fixture_db_with_books());
    let names: Vec<(&str, &str, usize)> = lib
        .books
        .iter()
        .map(|b| (b.author.as_str(), b.title.as_str(), b.chapters.len()))
        .collect();
    assert_eq!(
        names,
        vec![("Author One", "Book A", 2), ("Author One", "Book B", 1)]
    );
    let a = &lib.books[0];
    assert_eq!(a.dir, "/contents/LEARNING/Author One/Book A");
    // inserted as 02 then 01: the chapters come back in file order
    let titles: Vec<&str> = a.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, vec!["Chapter 1", "Chapter 2"]);
    assert!(a.paths[0].ends_with("/01.mp3") && a.paths[1].ends_with("/02.mp3"));
    assert!(fixture_db().tracks(cinder_db::Sort::Title).is_ok());
    assert!(build_library(&fixture_db()).books.is_empty());
}

/// "Shuffle everything" and "shuffle by artist" are about music; with audiobooks on, a book's
/// chapters are never dealt among songs. With the component off they are music like any other.
#[test]
fn shuffles_and_artist_play_leave_audiobooks_out_when_the_component_is_on() {
    let db = fixture_db_with_books();
    for scope in [
        cinder_ui::nav::ShuffleScope::AllSongs,
        cinder_ui::nav::ShuffleScope::ByAlbum,
        cinder_ui::nav::ShuffleScope::ByArtist,
    ] {
        for _ in 0..8 {
            let (v, _) = shuffle_tracks(Some(&db), scope, &|_| true, true).unwrap();
            assert!(
                v.iter().all(|t| !books::is_audiobook(&t.filename)),
                "{scope:?}"
            );
        }
    }
    assert!(artist_tracks(Some(&db), "Author One", true).is_none());
    // Off: the author is an ordinary artist and the chapters are tracks.
    assert_eq!(
        artist_tracks(Some(&db), "Author One", false).unwrap().len(),
        3
    );
    let (all, _) = shuffle_tracks(
        Some(&db),
        cinder_ui::nav::ShuffleScope::AllSongs,
        &|_| true,
        false,
    )
    .unwrap();
    assert_eq!(all.len(), 6, "every track, chapters included");
}

/// The installer's option off: nothing is set apart. The library is what it was before
/// audiobooks existed — `LEARNING` files are songs, books are albums — and there is no list.
#[test]
fn with_the_component_off_the_learning_folder_is_just_music() {
    let lib = build_library_with(&fixture_db_with_books(), None, false);
    assert!(lib.books.is_empty(), "no book list, so no book button");
    assert_eq!(lib.songs.len(), 6, "the chapters are songs");
    assert_eq!(lib.album_count(), 4, "and the books are albums");
    assert_eq!(lib.artists.len(), 3, "and the author is an artist");
}

/// SensMe reaches the browsable library: the bitmasks land on the song rows, the channels are
/// built from them, empty channels are dropped, and an unanalysed track is in none of them.
#[test]
fn build_library_reads_sensme_channels() {
    let lib = build_library(&fixture_db());
    assert_eq!(
        lib.sensme_tracks, 2,
        "two of the three tracks were analysed"
    );
    let names: Vec<&str> = lib.channels.iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        vec!["Active", "Morning"],
        "only the channels with members"
    );
    let active = &lib.channels[0];
    assert_eq!(active.id, 0);
    let titles: Vec<&str> = active
        .tracks
        .iter()
        .map(|&i| lib.songs[i as usize].title.as_str())
        .collect();
    assert_eq!(titles, vec!["Atlas Hands", "Harvest Moon"]);
    assert_eq!(
        lib.channels[1].tracks.len(),
        1,
        "Morning has only the first track"
    );
    let box_of_stones = lib
        .songs
        .iter()
        .find(|s| s.title == "Box of Stones")
        .unwrap();
    assert_eq!(box_of_stones.sensme, 0, "never analysed, so in no channel");
}

/// A channel resolves to real tracks in the order the screen shows them, and "everything
/// analysed" is every analysed track once — not the channels concatenated, which would play a
/// track in two channels twice.
#[test]
fn a_sensme_channel_resolves_to_its_tracks_in_order() {
    let db = fixture_db();
    let lib = build_library(&db);
    let uris = |chan: u8| {
        sensme_tracks(Some(&db), &lib, chan)
            .map(|s| s.into_iter().map(|t| t.filename).collect::<Vec<_>>())
    };
    assert_eq!(
        uris(0),
        Some(vec![
            "/music/atlas.flac".to_string(),
            "/music/harvest.flac".to_string()
        ]),
    );
    assert_eq!(uris(8), Some(vec!["/music/atlas.flac".to_string()]));
    assert_eq!(
        uris(cinder_ui::sensme::ALL),
        Some(vec![
            "/music/atlas.flac".to_string(),
            "/music/harvest.flac".to_string()
        ]),
        "the track in two channels appears once",
    );
    // A channel nothing is in, and one that does not exist, play nothing rather than
    // everything — the failure that turns "this channel is empty" into "shuffle the library".
    assert_eq!(uris(5), None);
    assert_eq!(uris(200), None);
    assert!(
        sensme_tracks(None, &lib, 0).is_none(),
        "no database, no sequence"
    );
}

/// Playlists reach the browsable library, and the orphan type-3 row (object 7, no parent)
/// does not become a ghost playlist.
#[test]
fn build_library_includes_real_playlists() {
    let lib = build_library(&fixture_db());
    assert_eq!(lib.playlists.len(), 1, "one real playlist, no ghosts");
    assert_eq!(lib.playlists[0].name, "Night Bus");
    assert_eq!(lib.playlists[0].id, 60);
    assert_eq!(lib.playlists[0].tracks, 2);
}

/// "Shuffle a playlist" plays one of the playlists the tab lists, Cinder's own included, and
/// skips one the filter empties. It did nothing on a player whose playlists were all Cinder's,
/// because it only looked in Sony's database (2026-11-02).
#[test]
fn shuffle_a_playlist_picks_from_cinders_own_playlists_too() {
    let mut r = zappin_render();
    r.db = Some(fixture_db());
    r.plists = playlists::Store {
        lists: vec![playlists::Playlist {
            id: -1,
            name: "Mine".into(),
            entries: vec![playlists::Entry {
                uri: "/music/box.flac".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    // Sony's database lists none, as on the owner's player.
    refresh_playlists(&mut r);
    let (seq, pre) = shuffle_a_playlist(&r, &keep_all).expect("Cinder's playlist");
    assert_eq!(uris_of(seq), vec!["/music/box.flac".to_string()]);
    assert_eq!(pre.len(), 1);

    // With Sony's "Night Bus" listed too and a filter that empties it, every press still plays.
    r.db_playlists = build_library(r.db.as_ref().unwrap()).playlists;
    refresh_playlists(&mut r);
    assert_eq!(r.app.library().playlists.len(), 2);
    let only_box = |t: &cinder_db::Track| t.filename == "/music/box.flac";
    for _ in 0..16 {
        let (seq, _) = shuffle_a_playlist(&r, &only_box).expect("never nothing");
        assert_eq!(uris_of(seq), vec!["/music/box.flac".to_string()]);
    }
}

#[test]
/// Plan 1c: a glued pair in a playlist stays together, in order, through the playlist's own
/// shuffle band, by songs, every time — and only while that playlist is what plays.
fn a_glued_pair_survives_the_playlist_shuffle() {
    let mut r = zappin_render();
    r.db = Some(fixture_db());
    let entry = |u: &str, glued: bool| playlists::Entry {
        uri: u.into(),
        glued,
        ..Default::default()
    };
    r.plists = playlists::Store {
        lists: vec![playlists::Playlist {
            id: -1,
            name: "Glued".into(),
            entries: vec![
                entry("/music/harvest.flac", false),
                entry("/music/atlas.flac", false),
                entry("/music/box.flac", true),
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    refresh_playlists(&mut r);
    r.app.set_shuffle_by("songs");
    for _ in 0..200 {
        carry_action_now(&mut r, &cinder_ui::nav::Action::ShufflePlaylist(-1));
        let a = r
            .pending_play
            .iter()
            .position(|u| u == "/music/atlas.flac")
            .unwrap();
        assert_eq!(
            r.pending_play.get(a + 1).map(String::as_str),
            Some("/music/box.flac"),
            "{:?}",
            r.pending_play
        );
    }
    assert!(
        !r.app.glue_pairs().is_empty(),
        "the playlist's glue is live"
    );
    carry_action_now(&mut r, &cinder_ui::nav::Action::PlayPlaylist(-1));
    assert!(
        !r.app.glue_pairs().is_empty(),
        "playing it in order keeps it"
    );
}

/// #53: a playlist's Shuffle band, and "random playlist · shuffled", follow Settings ▸ Shuffle.
/// With "albums", album 10's two songs stay together and in order; with "songs" they need not.
#[test]
fn a_playlist_shuffle_follows_the_shuffle_setting() {
    let mut r = zappin_render();
    r.db = Some(fixture_db());
    let entry = |u: &str| playlists::Entry {
        uri: u.into(),
        ..Default::default()
    };
    r.plists = playlists::Store {
        lists: vec![playlists::Playlist {
            id: -1,
            name: "Mixed".into(),
            entries: vec![
                entry("/music/box.flac"),
                entry("/music/harvest.flac"),
                entry("/music/atlas.flac"),
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    refresh_playlists(&mut r);
    let together = |seq: &[String]| {
        let a = seq.iter().position(|u| u == "/music/atlas.flac").unwrap();
        let b = seq.iter().position(|u| u == "/music/box.flac").unwrap();
        b == a + 1
    };
    r.app.set_shuffle_by("albums");
    for _ in 0..25 {
        assert_eq!(
            carry_action_now(&mut r, &cinder_ui::nav::Action::ShufflePlaylist(-1)),
            Some(8)
        );
        assert_eq!(r.pending_play.len(), 3);
        assert!(
            together(&r.pending_play),
            "album 10 split or reordered: {:?}",
            r.pending_play
        );
        let (seq, _) = shuffle_a_playlist(&r, &keep_all).unwrap();
        assert!(together(&uris_of(seq)), "the random-playlist band too");
    }
    r.app.set_shuffle_by("songs");
    let split = (0..200).any(|_| {
        carry_action_now(&mut r, &cinder_ui::nav::Action::ShufflePlaylist(-1));
        !together(&r.pending_play)
    });
    assert!(
        split,
        "by songs, album 10's songs are dealt apart sometimes"
    );
}

/// File URIs of a resolved sequence — what the shell actually hands PlayerService.
fn uris_of(seq: Vec<cinder_db::Track>) -> Vec<String> {
    seq.into_iter().map(|t| t.filename).collect()
}

/// The URIs handed to the shell are in saved (child_index) order, NOT title order — this is
/// what makes a playlist play as the user arranged it.
#[test]
fn play_playlist_resolves_uris_in_saved_order() {
    let db = fixture_db();
    let uris: Vec<String> = playlist_tracks(Some(&db), 60)
        .unwrap()
        .into_iter()
        .map(|t| t.filename)
        .collect();
    assert_eq!(
        uris,
        vec![
            "/music/harvest.flac".to_string(),
            "/music/atlas.flac".to_string()
        ]
    );
}

/// Browsing groups by ALBUM ARTIST. The fixture's second track has a different TRACK artist
/// (a guest) but the same album artist — exactly the shape that shattered compilations on the
/// real device, where 24 albums spanned several track artists and one DJ mix spanned 26,
/// producing 26 one-track "albums" under 26 different people.
/// R4's data reaches the song rows: the album artist only where it differs from the track
/// artist (the guest), the container from the file name, and the stats file keyed across from
/// PATH to object id by the build that has both in hand.
#[test]
fn the_library_carries_album_artist_format_and_stats() {
    let db = fixture_db();
    let tracks = db.tracks(cinder_db::Sort::Title).unwrap();
    let guest = tracks
        .iter()
        .find(|t| t.artist != t.album_artist && !t.album_artist.is_empty())
        .expect("the fixture has a guest track");
    let own = tracks.iter().find(|t| t.object_id == 1).unwrap();

    let dir = std::env::temp_dir().join(format!("cinder_ffi_stats_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = stats::Store::open(dir.join("cinder_stats.tsv"));
    store.rate(&own.filename, 5);
    store.count_play(&guest.filename, 1_790_000_000);
    store.rate("/music/not-in-the-library.flac", 3);

    let lib = build_library_with(&db, Some(&store), true);
    let row = |id: i64| lib.songs.iter().find(|s| s.object_id == id).unwrap();
    assert_eq!(
        row(own.object_id).album_artist,
        "",
        "same as the artist: nothing stored"
    );
    assert_eq!(row(own.object_id).group_artist(), own.artist);
    assert_eq!(
        row(guest.object_id).group_artist(),
        guest.album_artist,
        "a guest files under the album artist"
    );
    assert_eq!(row(own.object_id).format, cinder_ui::model::Format::Flac);
    assert_eq!(lib.stat(own.object_id).rating, 5);
    assert_eq!(lib.stat(guest.object_id).plays, 1);
    assert_eq!(
        lib.stats.len(),
        2,
        "a path the library does not hold maps to nothing"
    );
    // The album page's rating is the mean of what is rated in it.
    let album = lib
        .albums_flat()
        .into_iter()
        .find(|a| a.track_list.iter().any(|s| s.object_id == own.object_id))
        .unwrap();
    assert_eq!(lib.album_rating(album), Some(5));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn browsing_groups_by_album_artist_not_track_artist() {
    let lib = build_library(&fixture_db());
    // One album, one artist group — not one group per guest.
    assert_eq!(
        lib.album_groups.len(),
        2,
        "expected one group per ALBUM artist"
    );
    let smoke = lib
        .album_groups
        .iter()
        .find(|g| g.albums.iter().any(|a| a.name == "Last Smoke"))
        .expect("Last Smoke must appear under an album-artist group");
    assert_eq!(smoke.artist, "Benjamin Francis Leftwich");
    assert_eq!(
        smoke
            .albums
            .iter()
            .filter(|a| a.name == "Last Smoke")
            .count(),
        1,
        "the album was split by a guest track artist"
    );
    // The guest IS a real album artist in its own right (it owns Harvest Moon), so it belongs
    // in the tab — but its guest appearance on someone else's album must not credit it with a
    // second album. That is the precise damage track-artist grouping did.
    let guest_artist = lib
        .artists
        .iter()
        .find(|a| a.name == "Cold Stone & Sea")
        .expect("an album artist must appear in the Artists tab");
    assert_eq!(
        guest_artist.albums, 1,
        "a guest appearance was counted as an album: {:?}",
        guest_artist.arts
    );
    // And its track count is its OWN album's, not inflated by the guest track.
    assert_eq!(
        guest_artist.tracks, 1,
        "a guest track was counted against the wrong artist"
    );
    // …but the SONG row still credits the guest, which is where it belongs.
    let guest = lib
        .songs
        .iter()
        .find(|s| s.title == "Box of Stones")
        .unwrap();
    assert_eq!(
        guest.artist, "Cold Stone & Sea",
        "song rows must show the track artist"
    );
}

/// Every shuffle scope resolves to a non-empty queue drawn only from real tracks, and none of
/// them leak the non-audio rows (folder / cover image) the library filters out.
#[test]
fn shuffle_scopes_resolve_to_real_tracks() {
    use cinder_ui::nav::ShuffleScope as S;
    let db = fixture_db();
    let all: std::collections::BTreeSet<&str> = [
        "/music/atlas.flac",
        "/music/box.flac",
        "/music/harvest.flac",
    ]
    .into_iter()
    .collect();
    for scope in [S::AllSongs, S::ByAlbum, S::ByArtist, S::Playlist] {
        let (seq, pre) = shuffle_tracks(Some(&db), scope, &keep_all, false)
            .unwrap_or_else(|| panic!("{scope:?} empty"));
        let uris = uris_of(seq.clone());
        assert!(!uris.is_empty());
        for u in &uris {
            assert!(
                all.contains(u.as_str()),
                "{scope:?} produced a non-track: {u}"
            );
        }
        // EVERY scope reports the order it replaced, and it describes exactly the sequence
        // handed back — `App::note_pre_shuffle` refuses one of a different length, so a scope
        // that got this wrong would silently leave shuffle a one-way door.
        let mut a: Vec<i64> = pre.clone();
        let mut b: Vec<i64> = seq.iter().map(|t| t.object_id).collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(
            a, b,
            "{scope:?}: the pre-shuffle order is not this sequence"
        );
    }
    // AllSongs is the whole library; ByAlbum keeps every track too (it only reorders albums).
    assert_eq!(
        shuffle_tracks(Some(&db), S::AllSongs, &keep_all, false)
            .unwrap()
            .0
            .len(),
        3
    );
    assert_eq!(
        shuffle_tracks(Some(&db), S::ByAlbum, &keep_all, false)
            .unwrap()
            .0
            .len(),
        3
    );
}

/// The band's caption names the active filter, so the band has to obey it — on BOTH axes. The
/// Hi-Res one was never wired in: with "Shuffle Hi-Res" on the glass, every scope shuffled the
/// whole library.
#[test]
fn a_shuffle_band_plays_only_what_the_filter_leaves() {
    use cinder_ui::nav::ShuffleScope as S;
    let db = fixture_db();
    // A filter nothing survives: every scope must DECLINE rather than fall back to the
    // unfiltered library, because starting something the caption did not promise is worse
    // than doing nothing and saying so.
    let keep_none = |_: &cinder_db::Track| false;
    for scope in [S::AllSongs, S::ByAlbum, S::ByArtist, S::Playlist] {
        if let Some((seq, _)) = shuffle_tracks(Some(&db), scope, &keep_none, false) {
            panic!("{scope:?} ignored the filter: {} tracks", seq.len());
        }
    }
    // …and a predicate that keeps one track keeps exactly that one.
    let one = |t: &cinder_db::Track| t.filename == "/music/harvest.flac";
    let (seq, pre) = shuffle_tracks(Some(&db), S::AllSongs, &one, false).expect("one survivor");
    assert_eq!(uris_of(seq), vec!["/music/harvest.flac".to_string()]);
    assert_eq!(pre.len(), 1);
}

/// A smart playlist's own shuffle mode (2026-10-04): by album it never splits an album, by
/// songs it is still every track, and nothing is lost or repeated either way.
#[test]
fn a_smart_playlists_shuffle_mode_deals_whole_albums() {
    use cinder_ui::shuffle::ShuffleBy;
    let db = fixture_db();
    let all = db.tracks_album_order().unwrap();
    let n = all.len();
    for by in ShuffleBy::ALL {
        for _ in 0..25 {
            let uris = uris_of(deal_tracks(all.clone(), by, &mut rng()));
            let mut sorted = uris.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), n, "{by:?} lost or repeated a track: {uris:?}");
            if by == ShuffleBy::Albums {
                let atlas = uris.iter().position(|u| u == "/music/atlas.flac").unwrap();
                let boxs = uris.iter().position(|u| u == "/music/box.flac").unwrap();
                assert_eq!(boxs, atlas + 1, "album 10 was split or reordered: {uris:?}");
            }
        }
    }
}

/// "TRACKS IN ORDER": ByAlbum may reorder albums but must never split one up or reorder
/// the tracks inside it.
#[test]
fn shuffle_by_album_keeps_albums_intact_and_in_sequence() {
    use cinder_ui::nav::ShuffleScope as S;
    let db = fixture_db();
    // Album 10 = atlas then box (series_no 1,2); album 11 = harvest alone.
    for _ in 0..25 {
        let uris = uris_of(
            shuffle_tracks(Some(&db), S::ByAlbum, &keep_all, false)
                .unwrap()
                .0,
        );
        let atlas = uris.iter().position(|u| u == "/music/atlas.flac").unwrap();
        let boxs = uris.iter().position(|u| u == "/music/box.flac").unwrap();
        assert_eq!(boxs, atlas + 1, "album 10 was split or reordered: {uris:?}");
    }
}

/// The play order handed to PlayerService: the audible track leads, then the rest of the
/// list — and NO TWO ADJACENT ENTRIES ARE THE SAME FILE.
///
/// Queueing the song you are listening to produced `[A, A, …]`. PlayerService plays A twice,
/// and the second copy does not change the URI — which is the only thing the shell reports a
/// track start on — so the list and the service parted ways.
#[test]
fn an_unknown_year_is_not_printed() {
    assert!(!is_real_year("0"));
    assert!(!is_real_year(""));
    assert!(!is_real_year(" 0 "));
    assert!(is_real_year("1993"));
    assert!(
        is_real_year("1990s"),
        "a label that is not a number is still a label"
    );
}

/// A long sequence is cut to a window that CONTAINS the tapped track.
#[test]
fn a_long_sequence_keeps_the_tapped_track() {
    let m = MAX_PLAY_SEQUENCE;
    assert_eq!(play_window(10, 3), (0, 10));
    assert_eq!(play_window(m + 100, 0), (0, m));
    let (lo, hi) = play_window(3 * m, 600);
    assert!((lo..hi).contains(&600) && hi - lo == m, "{lo}..{hi}");
    assert_eq!(
        600 - lo,
        m / 4,
        "a quarter of the window before the tapped track"
    );
    let (lo, hi) = play_window(3 * m, 3 * m - 1);
    assert_eq!(
        (lo, hi),
        (2 * m, 3 * m),
        "the last track keeps the window inside the list"
    );
}

/// A 32-bit FLAC never reaches Up Next, and the start stays on the song the user chose — or
/// the one after it when the chosen song is the one taken out.
#[test]
fn undecodable_tracks_leave_the_sequence_before_up_next_sees_it() {
    let t = |f: &str| cinder_db::Track {
        filename: f.to_string(),
        ..Default::default()
    };
    let names = |v: &[cinder_db::Track]| v.iter().map(|t| t.filename.clone()).collect::<Vec<_>>();
    let seq = || vec![t("a"), t("wide"), t("c"), t("d")];
    let bad: std::collections::HashSet<String> = ["wide".to_string()].into();

    let (kept, start, dropped) = drop_undecodable(seq(), 2, &bad);
    assert_eq!(
        (names(&kept), start, dropped),
        (vec!["a".into(), "c".into(), "d".into()], 1, false),
        "a song after the removed one keeps pointing at itself"
    );
    let (kept, start, dropped) = drop_undecodable(seq(), 1, &bad);
    assert_eq!(
        (kept[start].filename.as_str(), dropped),
        ("c", true),
        "chosen and removed: the next song"
    );
    let (_, start, _) = drop_undecodable(seq(), 0, &bad);
    assert_eq!(start, 0, "a song before the removed one is untouched");

    let (kept, start, dropped) = drop_undecodable(vec![t("a"), t("wide")], 1, &bad);
    assert_eq!(
        (names(&kept), start, dropped),
        (vec!["a".to_string()], 0, true),
        "the last song removed wraps to the first, as the shell's backstop does"
    );
    let (kept, _, dropped) = drop_undecodable(vec![t("wide")], 0, &bad);
    assert!(
        kept.is_empty() && dropped,
        "nothing playable is reported, not papered over"
    );
    let (kept, start, dropped) = drop_undecodable(seq(), 3, &Default::default());
    assert_eq!(
        (kept.len(), start, dropped),
        (4, 3, false),
        "an empty set changes nothing"
    );
}

#[test]
fn replaygain_follows_the_song_playing_and_the_one_after_it() {
    let seq: Vec<String> = ["/a", "/b", "/c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(rg_songs("/a", None, &seq), ("/a", "/b"));
    assert_eq!(
        rg_songs("/c", None, &seq),
        ("/c", ""),
        "the last song has no next"
    );
    assert_eq!(rg_songs("", None, &seq), ("", ""), "nothing playing");
    assert_eq!(
        rg_songs("/x", None, &seq),
        ("/x", ""),
        "a song the list does not hold"
    );
    // The shell has started /b; Sony still reports /a for a second.
    assert_eq!(rg_songs("/a", Some("/b"), &seq), ("/b", "/c"));
}

#[test]
fn the_play_order_never_repeats_a_file_back_to_back() {
    let u = |s: &str| Some(s.to_string());
    // The current track queued against itself collapses to one entry.
    assert_eq!(
        play_order(Some("/a.flac"), [u("/a.flac"), u("/b.flac")]),
        vec!["/a.flac".to_string(), "/b.flac".to_string()],
    );
    // …and so does a doubled pick.
    assert_eq!(
        play_order(Some("/a.flac"), [u("/b.flac"), u("/b.flac"), u("/c.flac")]),
        vec![
            "/a.flac".to_string(),
            "/b.flac".to_string(),
            "/c.flac".to_string()
        ],
    );
    // But a DELIBERATE repeat with something in between is kept: the URI changes at each
    // boundary there, so each copy is reported and consumed exactly as it should be.
    assert_eq!(
        play_order(Some("/a.flac"), [u("/b.flac"), u("/a.flac")]),
        vec![
            "/a.flac".to_string(),
            "/b.flac".to_string(),
            "/a.flac".to_string()
        ],
    );
    // Rows that no longer resolve to a file drop out rather than shortening the list around
    // them — and dropping one must not make its neighbours adjacent duplicates by accident.
    assert_eq!(
        play_order(Some("/a.flac"), [None, u("/a.flac"), None, u("/b.flac")]),
        vec!["/a.flac".to_string(), "/b.flac".to_string()],
    );
    // No lead at all — a repeat-all lap, which starts from the top of the context.
    assert_eq!(
        play_order(None, [u("/a.flac"), u("/a.flac"), u("/b.flac")]),
        vec!["/a.flac".to_string(), "/b.flac".to_string()],
    );
    assert!(play_order(None, [None, None]).is_empty());
    assert!(play_order(None, []).is_empty());
}

/// A SKIP LEADS WITH THE QUEUE, not with the track being skipped away from.
///
/// Every other caller of `play_order` passes the audible track as the lead, because it is
/// staying audible. The skip path is the one that passes `None`: the current track is being
/// abandoned on purpose, so index 0 has to be the user's own pick. Without this the sequence
/// began with the album track the queue was meant to come BEFORE, which is the reported bug —
/// a skip played the album's next song and only then went to the queue.
#[test]
fn a_skip_puts_the_queue_at_the_head_of_the_sequence() {
    let u = |s: &str| Some(s.to_string());
    // Playing A with Q queued and B, C still ahead in the album. The boundary/early-rebuild
    // form keeps A audible at the head...
    assert_eq!(
        play_order(Some("/a.flac"), [u("/q.flac"), u("/b.flac"), u("/c.flac")]),
        vec![
            "/a.flac".to_string(),
            "/q.flac".to_string(),
            "/b.flac".to_string(),
            "/c.flac".to_string()
        ],
    );
    // ...and the SKIP form drops it, so the very next thing heard is the pick, and the album
    // still resumes exactly where it was left.
    assert_eq!(
        play_order(None, [u("/q.flac"), u("/b.flac"), u("/c.flac")]),
        vec![
            "/q.flac".to_string(),
            "/b.flac".to_string(),
            "/c.flac".to_string()
        ],
    );
    // Skipping onto a pick that IS the album's next track collapses to one copy, so the
    // adjacent-duplicate rule still holds on this path (it is what stops a track playing twice
    // and never being consumed out of the queue).
    assert_eq!(
        play_order(None, [u("/b.flac"), u("/b.flac"), u("/c.flac")]),
        vec!["/b.flac".to_string(), "/c.flac".to_string()],
    );
    // AND WITH NO PICKS AT ALL, which is the shuffle case. `ShuffleToggle` defers exactly like
    // a queue edit — it permutes `context[idx + 1..]` and raises `queue_pending` — so after it
    // PlayerService still holds the sequence from BEFORE the shuffle. A skip has to rebuild
    // from the context as it stands now, and with an empty queue that is just the tail, whose
    // index 0 is the next track in the CURRENT order. Requiring picks here is what made a skip
    // straight after a shuffle toggle play the un-shuffled successor.
    assert_eq!(
        play_order(None, [u("/shuf1.flac"), u("/shuf2.flac"), u("/shuf3.flac")]),
        vec![
            "/shuf1.flac".to_string(),
            "/shuf2.flac".to_string(),
            "/shuf3.flac".to_string()
        ],
    );
}

/// No DB → no action (rather than an empty queue).
#[test]
fn shuffle_without_db_is_ignored() {
    assert!(shuffle_tracks(
        None,
        cinder_ui::nav::ShuffleScope::AllSongs,
        &keep_all,
        false
    )
    .is_none());
}

/// Unknown id, or no DB at all → no action, rather than handing the shell an empty sequence.
#[test]
fn play_playlist_unknown_is_ignored() {
    let db = fixture_db();
    assert!(playlist_tracks(Some(&db), 999).is_none());
    assert!(playlist_tracks(None, 60).is_none());
}

#[test]
fn empty_title_falls_back_to_filename() {
    let t = cinder_db::Track {
        genre_id: None,
        object_id: 2,
        title: String::new(),
        artist: String::new(),
        album_artist: String::new(),
        album: String::new(),
        filename: "/music/box.flac".into(),
        disc_no: 0,
        track_no: 0,
        duration_raw: None,
        is_hires: false,
        othumb_id: None,
        album_id: None,
        added: 0,
        releaseyear_id: None,
    };
    let mut np = Np::default();
    apply_track(&mut np, &t);
    set_progress(&mut np, 0, 0); // unknown duration → blank remaining, zero bar
    assert_eq!(np.title, "box.flac");
    assert_eq!(np.elapsed, "0:00");
    assert_eq!(np.remaining, "");
    assert_eq!(np.progress, 0.0);
}

/// The file install_cinderhome.sh writes, as it writes it; unknown tags (a language a newer
/// installer knows) are skipped, and no file is English only.
#[test]
fn installed_languages_read_the_installers_tags() {
    use cinder_ui::i18n::Language;
    let path = std::env::temp_dir().join(format!("cinder_langs_{}", std::process::id()));
    std::fs::write(&path, "EN ZH_CN XX_YY\n").unwrap();
    let p = path.to_str().unwrap();
    assert_eq!(
        installed_languages(p),
        [Language::English, Language::SimplifiedChinese]
    );
    std::fs::remove_file(&path).unwrap();
    assert!(installed_languages(p).is_empty());
}

/// The URI copy must report the FULL length, not the copied length, so the caller can tell a
/// truncated path from a complete one. A truncated path looks perfectly valid and would queue a
/// file that doesn't exist; the shell relies on `len >= cap` to skip those. Reachable in
/// practice with deep UTF-8 (CJK) paths, which is why the buffer size is not a safe assumption.
#[test]
fn uri_copy_reports_full_length_so_truncation_is_detectable() {
    let mut buf = [0 as std::ffi::c_char; 32];
    let long = "/contents/MUSIC/".to_string() + &"ま".repeat(50) + "/track.flac";
    assert!(
        long.len() > buf.len(),
        "test needs a URI longer than the buffer"
    );

    let got = unsafe { copy_str_into(&long, buf.as_mut_ptr(), buf.len() as libc::c_int) };
    assert_eq!(
        got as usize,
        long.len(),
        "must return the FULL length, not the copied one"
    );
    assert!(
        got >= buf.len() as libc::c_int,
        "caller must be able to detect truncation"
    );
    assert_eq!(
        buf[buf.len() - 1],
        0,
        "still NUL-terminated inside the buffer"
    );

    // One that fits reports its own length, which is < cap — the "safe to use" signal.
    let short = "/contents/a.flac";
    let got = unsafe { copy_str_into(short, buf.as_mut_ptr(), buf.len() as libc::c_int) };
    assert_eq!(got, short.len() as libc::c_int);
    assert!(got < buf.len() as libc::c_int);
    let back = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    assert_eq!(back.to_str().unwrap(), short);
}

/// The liked list is plain text on purpose: a torn line from a power cut must be SKIPPED, not
/// take the whole list with it. This is a hand-curated list the user cannot otherwise back up.
#[test]
fn liked_load_skips_garbage_instead_of_failing() {
    let dir = std::env::temp_dir().join(format!("cinder_liked_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("liked.conf");
    // A realistic torn write: a valid run, a half-written line, a blank, then more valid ids.
    std::fs::write(&p, "101\n202\n\nnot-a-number\n  303  \n\u{0}\n404").unwrap();
    let set = liked_load(p.to_str().unwrap());
    assert_eq!(
        set.iter().copied().collect::<Vec<_>>(),
        vec![101, 202, 303, 404]
    );
    // A missing file is an empty list, not an error — first run has no file.
    assert!(liked_load("/nonexistent/cinder/liked.conf").is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn prev_rewinds_past_the_grace_window() {
    // The reported bug: ◁ was an unconditional PlayController::PrevTrack(), so at the head of
    // a queue it did nothing at all, and mid-track it jumped away instead of restarting.
    assert!(
        !prev_means_restart(0, 240_000),
        "at the very start ◁ steps back a track"
    );
    assert!(
        !prev_means_restart(3_000, 240_000),
        "the grace window itself still steps back"
    );
    assert!(
        prev_means_restart(3_001, 240_000),
        "past it, ◁ rewinds to the start"
    );
    assert!(prev_means_restart(200_000, 240_000));
    // Unknown duration (track not in the DB) → no position to rewind within, so step back and
    // let the shell's PrevTrack-failed fallback cover the head-of-sequence case.
    assert!(!prev_means_restart(90_000, 0));
}

#[test]
fn set_progress_advances_and_clamps() {
    let mut np = Np::default();
    set_progress(&mut np, 60_000, 180_000); // 1:00 of 3:00
    assert_eq!(np.elapsed, "1:00");
    assert_eq!(np.remaining, "-2:00");
    assert!((np.progress - (1.0 / 3.0)).abs() < 1e-4);
    // clamps past the end
    set_progress(&mut np, 999_000, 180_000);
    assert_eq!(np.elapsed, "3:00");
    assert_eq!(np.remaining, "-0:00");
    assert!((np.progress - 1.0).abs() < 1e-6);
}

// ── drag-to-seek geometry ────────────────────────────────────────────────────────────────
// The rail fraction is what turns a finger x into a seek target, so it has to agree exactly
// with the drawn rail at both ends. An off-by-a-few-pixels mapping is the difference between
// "seek to the end" and "seek to 99%, then the track ends on its own a moment later".
#[test]
fn rail_fraction_maps_the_drawn_rail_and_clamps_outside_it() {
    use cinder_ui::now_playing::{rail_fraction, RAIL_W, RAIL_X0};
    assert_eq!(rail_fraction(RAIL_X0), 0.0);
    assert_eq!(rail_fraction(RAIL_X0 + RAIL_W), 1.0);
    assert!((rail_fraction(RAIL_X0 + RAIL_W / 2) - 0.5).abs() < 1e-3);
    // Outside the rail clamps rather than extrapolating: a thumb landing left of x=24 or
    // right of the end must mean 0% / 100%, never a negative or >1 seek target.
    assert_eq!(rail_fraction(0), 0.0);
    assert_eq!(rail_fraction(479), 1.0);
    assert_eq!(rail_fraction(-500), 0.0);
}

// The grab band must be thick enough for a thumb but must never reach the transport row —
// the play/pause circle is centred at y=692 with radius 44, so anything from 648 is its.
#[test]
fn rail_grab_band_is_thumb_sized_and_clears_the_transport_row() {
    use cinder_ui::now_playing::{RAIL_GRAB_BOT, RAIL_GRAB_TOP, RAIL_Y};
    const {
        assert!(RAIL_GRAB_TOP <= RAIL_Y, "band must include the rail itself");
        assert!(RAIL_GRAB_BOT >= RAIL_Y + 4);
        assert!(
            RAIL_GRAB_BOT - RAIL_GRAB_TOP >= 40,
            "too thin to hit with a thumb"
        );
        assert!(
            RAIL_GRAB_BOT < 692 - 44,
            "band overlaps the play/pause target"
        );
    }
}

// ── the settings file is not trusted input ──────────────────────────────────────────────
// It lives on /contents: vfat, world-writable, and shared with any PC the player is plugged
// into. Every EQ site in the UI clamps to ±BAND_MAX, so Cinder's own values are always in
// range — but this loader parses `i8`, which accepts -128..127, and an out-of-range gain does
// not clamp inside Sony's DSP. It ZEROES the band. So a corrupted or hand-edited line used to
// silently flatten a band instead of pinning it to maximum, draw its knob outside the EQ
// field, and then be written straight back out on the next save.
#[test]
fn eq_band_max_matches_the_ui_limit() {
    // If these drift apart the screen and the DSP disagree about what maximum boost means.
    assert_eq!(EQ_BAND_MAX, cinder_ui::eq::BAND_MAX);
}

#[test]
fn out_of_range_eq_gains_clamp_rather_than_zeroing_the_band() {
    // The clamp the loader applies, asserted directly on the same expression.
    let clamp = |g: i8| g.clamp(-EQ_BAND_MAX, EQ_BAND_MAX);
    assert_eq!(
        clamp(127),
        EQ_BAND_MAX,
        "i8 max must pin to +20, not reach the DSP"
    );
    assert_eq!(clamp(-128), -EQ_BAND_MAX, "i8 min must pin to -20");
    assert_eq!(
        clamp(100),
        EQ_BAND_MAX,
        "a hand-edited 100 pins to +20, not 0"
    );
    assert_eq!(clamp(21), EQ_BAND_MAX, "one over the top pins");
    assert_eq!(clamp(-21), -EQ_BAND_MAX, "one under the bottom pins");
    for g in -EQ_BAND_MAX..=EQ_BAND_MAX {
        assert_eq!(clamp(g), g, "in-range gain {g} must survive exactly");
    }
}

#[test]
fn a_clamped_band_still_lands_inside_the_eq_field() {
    // The other half of the same defect: the EQ screen maps value -> y through BAND_MAX, so a
    // gain of 100 would draw its knob far outside the field it belongs to. Clamped values are
    // by construction the ones the screen can draw.
    use cinder_ui::eq::{value_at_y, BAND_MAX};
    for g in [-BAND_MAX, 0, BAND_MAX] {
        assert!((-BAND_MAX..=BAND_MAX).contains(&g));
    }
    // value_at_y is the inverse and clamps too — nothing the field can produce is out of range.
    for y in -500..1200 {
        let v = value_at_y(y);
        assert!((-BAND_MAX..=BAND_MAX).contains(&v), "y={y} produced {v}");
    }
}

struct ZappinNullPresent;
impl present::PresentTarget for ZappinNullPresent {
    fn present(&mut self, _: &[u32]) {}
}

/// The phone remote as a phone sees it: the code once, then the library, search and Play by
/// cookie, and Play reaching the player's queue of commands.
#[test]
fn phone_remote_end_to_end() {
    use std::io::{Read, Write};
    let _render = global_render();
    let _g = remote::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cinder-remote-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CINDER_REMOTE_PHONES", dir.join("phones"));
    *cell().lock().unwrap() = Some(zappin_render());
    let port = remote::start("123456").expect("a free port");
    let ask = |req: String| {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    };
    let login = |code: &str| {
        let body = format!("code={code}");
        ask(format!(
            "POST /login HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    };
    assert!(login("000000").contains("Location: /?wrong"));
    let ok = login("123456");
    let token = ok
        .split("cinder=")
        .nth(1)
        .and_then(|t| t.get(..32))
        .expect("a cookie")
        .to_string();
    let cookie = format!("Cookie: cinder={token}\r\n");
    let get = |p: &str| ask(format!("GET {p} HTTP/1.1\r\n{cookie}\r\n"));
    assert!(ask("GET /api/albums HTTP/1.1\r\n\r\n".into()).starts_with("HTTP/1.1 403"));
    assert!(get("/").contains("Up Next"), "the page, not the login");
    let albums = get("/api/albums");
    assert!(
        albums.starts_with("HTTP/1.1 200") && albums.contains("\"n\":"),
        "{albums}"
    );
    let (title, id) = read(|r| {
        let s = &r.app.library().songs[0];
        (s.title.clone(), s.object_id)
    });
    let q: String = title.split_whitespace().next().unwrap().into();
    assert!(
        get(&format!("/api/search?q={}", drive::pct_encode(&q))).contains(&format!("\"id\":{id}"))
    );
    assert!(get("/api/album/0").contains("\"tracks\":["));
    let post = |p: &str, body: &str, header: &str| {
        ask(format!(
            "POST {p} HTTP/1.1\r\n{cookie}{header}Content-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    };
    let play = format!("{id}");
    assert!(
        post("/api/play?at=0", &play, "").starts_with("HTTP/1.1 403"),
        "no X-Cinder"
    );
    assert!(post("/api/play?at=0", &play, "X-Cinder: 1\r\n").starts_with("HTTP/1.1 204"));
    assert_eq!(remote::take_cmd(), Some(remote::Cmd::Play(vec![id], 0)));
    assert!(post("/api/queue?next=1", &play, "X-Cinder: 1\r\n").starts_with("HTTP/1.1 204"));
    assert_eq!(
        cinder_remote_take_action(),
        0,
        "queued in-process; the shell has nothing to do"
    );
    assert!(get("/api/queue").contains(&format!("\"id\":{id}")));
    remote::stop();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 2026-11-11, casting to a Sonos: the 1 Hz tick moved the bar on from the PAUSED player's
/// last position and length, so Now Playing jumped between that and the speaker's every second.
#[test]
fn a_cast_keeps_the_speakers_position_on_now_playing() {
    let _cast = cast::tests::E2E.lock().unwrap_or_else(|e| e.into_inner());
    cast::tests::set_status_for_test(Some(cast::Status {
        on: true,
        playing: true,
        pos_ms: 58_000,
        dur_ms: 60_000,
        ..Default::default()
    }));
    let mut r = zappin_render();
    r.cast_was_on = true;
    r.np.playing = true;
    set_progress(&mut r.np, 58_000, 60_000);
    // The player's own, paused song: a stale anchor 114 s into five minutes.
    (r.cur_duration_ms, r.play_pos_ms, r.real_pos_ms) = (300_000, 114_000, 114_000);
    let now = std::time::Instant::now();
    r.real_pos_at = now - std::time::Duration::from_secs(5);
    advance_position(&mut r, now, 1_000);
    let shown = (r.np.elapsed.clone(), r.np.remaining.clone());
    let drag_over = seek_length(&r);
    cast::tests::set_status_for_test(None);
    assert_eq!(
        shown,
        ("0:58".to_string(), "-0:02".to_string()),
        "the speaker's place stays"
    );
    assert_eq!(
        drag_over, 60_000,
        "a drag on the bar spans the speaker's song"
    );
    // With no cast the same tick moves the player's own bar, as before.
    advance_position(&mut r, now, 1_000);
    assert_eq!(r.np.elapsed, "1:59");
}

/// 2026-11-11: Track information stayed empty after a restart until the player itself played
/// a song, and never followed a speaker's songs. Both now fill it through this one helper.
#[test]
fn track_information_fills_for_a_song_the_player_did_not_start() {
    let mut r = zappin_render();
    let t = cinder_db::Track {
        title: "No Other Way".into(),
        artist: "Paolo Nutini".into(),
        filename: "/nonexistent/No Other Way.flac".into(),
        ..Default::default()
    };
    fill_track_info(&mut r, &t, false);
    let rows = r.app.track_info_rows();
    assert!(
        rows.iter()
            .any(|(k, v)| k == "Title" && v == "No Other Way"),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(k, v)| k == "Artist" && v == "Paolo Nutini"),
        "{rows:?}"
    );
}

/// Sound ▸ Advanced ▸ Headphone EQ: each tap plays the next profile on the drive and then off;
/// a file it cannot read plays nothing, and the next tap still moves on.
#[test]
fn the_headphone_eq_row_cycles_the_profiles_and_writes_the_filters() {
    let dir = std::env::temp_dir().join(format!("cinder-hpeq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("A ParametricEQ.txt"),
        "Preamp: -3 dB\nFilter 1: ON PK Fc 1000 Hz Gain 3 dB Q 1\n",
    )
    .unwrap();
    std::fs::write(dir.join("b.txt"), "GraphicEQ: 20 -1").unwrap();
    let mut r = zappin_render();
    r.hpeq_dir = Some(dir.clone());
    let tap = |r: &mut Render| carry_action_now(r, &cinder_ui::nav::Action::NextHpEq);

    tap(&mut r);
    assert_eq!(r.hpeq_pref, "A ParametricEQ");
    let body = std::fs::read_to_string(HPEQ_FILE).unwrap();
    assert!(body.lines().any(|l| l.starts_with("44100 ")), "{body}");

    tap(&mut r);
    assert_eq!(r.hpeq_pref, "b", "the broken file is where the row stands");
    assert!(
        !std::path::Path::new(HPEQ_FILE).exists(),
        "and it plays nothing"
    );

    tap(&mut r);
    assert_eq!(r.hpeq_pref, "", "after the last: off");
    tap(&mut r);
    assert_eq!(r.hpeq_pref, "A ParametricEQ", "and round again");
    tap(&mut r);
    tap(&mut r);
    assert!(!std::path::Path::new(HPEQ_FILE).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_grid_drawn_before_the_cover_keys_arrive_still_gets_its_covers() {
    // 2026-11-15: a player that started on the album grid asked for every cover before
    // post_up loaded the keys, filed each as a miss, and showed gradients until a rescan.
    let dir = std::env::temp_dir().join(format!("cinder_grid_keys_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("CINDER_ART_CACHE", &dir);
    let mut r = zappin_render();
    let mut lib = cinder_ui::Library::sample();
    for (n, a) in lib
        .album_groups
        .iter_mut()
        .flat_map(|g| g.albums.iter_mut())
        .enumerate()
    {
        a.album_id = n as i64 + 1; // the sample's are all 0, which the grid skips
    }
    r.app.set_library(lib);
    r.app.set_lib_views_str("list,grid,list,list");
    r.app.open_library_for_test(cinder_ui::library::Tab::Albums);
    let id = *r
        .app
        .grid_cover_wants()
        .first()
        .expect("the grid wants covers");

    load_grid_covers(&mut r); // no keys yet
    assert!(!r.grid_miss.contains(&id), "no key yet is not a miss");

    let key = art_cache::key_of("/contents/MUSIC/a/01.flac");
    std::fs::write(dir.join(format!("{key:016x}.t96")), vec![7u8; 96 * 96 * 3]).unwrap();
    r.art_cache_keys.insert(id, key);
    // The worker reads it; a later frame collects it.
    let t0 = std::time::Instant::now();
    while !r.app.has_grid_cover(id) && t0.elapsed() < std::time::Duration::from_secs(5) {
        load_grid_covers(&mut r);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(r.app.has_grid_cover(id));
    std::env::remove_var("CINDER_ART_CACHE");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Internet radio, then FM, own Now Playing while they play; Like can't touch the hidden
/// library song, the library's player can't overwrite them, and the song comes back after.
#[test]
fn a_live_source_owns_now_playing_until_it_ends() {
    use cinder_ui::nav::live;
    let mut r = zappin_render();
    let song = cinder_db::Track {
        object_id: 7,
        title: "Library song".into(),
        artist: "Library artist".into(),
        ..Default::default()
    };
    apply_track(&mut r.np, &song);
    r.last_track = Some(song);
    r.app
        .set_radio_stations("Radio Paradise|https://stream.example/rp\n");
    r.app
        .set_radio_status("state=playing\nstation=0\ntitle=Massive Attack - Teardrop\nnote=\n");

    live_np(&mut r, live::RADIO, true);
    assert_eq!(r.np.title, "Teardrop");
    assert_eq!(r.np.artist, "Massive Attack · Radio Paradise");
    assert!(r.np.playing);
    assert_eq!(r.art_key, Some(-1), "no library cover on a stream");
    r.live = Some((
        live::RADIO,
        std::time::Instant::now() - std::time::Duration::from_secs(65),
    ));
    live_np(&mut r, live::RADIO, true);
    assert_eq!(r.np.elapsed, "1:05", "the time since the stream started");

    // A station with no titles shows its name; a pause shows paused.
    r.app
        .set_radio_status("state=playing\nstation=0\ntitle=\nnote=\n");
    live_np(&mut r, live::RADIO, false);
    assert_eq!(r.np.title, "Radio Paradise");
    assert!(!r.np.playing);

    r.app.fm_report_khz(98_500);
    live_np(&mut r, live::FM, true);
    assert_eq!(r.np.title, "98.5 MHz");

    live_np(&mut r, 0, false);
    assert_eq!(r.np.title, "Library song");
    assert_eq!(r.np.artist, "Library artist");
    assert!(r.live.is_none());
}

fn zappin_render() -> Render {
    let present = Sink::Threaded(present::PresentThread::start(|| Ok(ZappinNullPresent)).unwrap());
    let np = Np {
        codec: "—".into(),
        battery: 100,
        ..Default::default()
    };
    Render {
        album_cover_id: None,
        grid_miss: Default::default(),
        grid_more: false,
        present,
        canvas: Canvas::new(),
        fonts: FontSet::load(),
        night: false,
        np,
        db: None,
        app: cinder_ui::nav::App::unlocked(),
        scrob: None,
        last_track: None,
        album_lap_staged: false,
        queue_edit_refused: false,
        play_history: Vec::new(),
        rewind_from: None,
        play_pos_ms: 0,
        cur_duration_ms: 0,
        last_pos: std::time::Instant::now(),
        real_pos_ms: -1,
        real_pos_at: std::time::Instant::now(),
        tap: pcm_tap::Tap::new(pcm_tap::SHM_DIR),
        sig: vizsig::SigState::default(),
        tap_at: None,
        tap_peak: 0.0,
        tap_log_at: None,
        fm_seek_dir: 1,
        fm_power: false,
        fm_bt: false,
        fm_rec: false,
        scrub_ms: None,
        scrub_act: None,
        pending_screenshot: None,
        sleep_remaining_ms: 0,
        rescan_left_ms: 0,
        sleep_fire: false,
        settings_path: None,
        last_saved_body: String::new(),
        palette_dir: None,
        resume_path: None,
        resume_last_body: String::new(),
        resume_pos_path: None,
        resume_pos_last: String::new(),
        resume_pos_at: std::time::Instant::now(),
        resume_pending: None,
        dirty: true,
        viz_phase: 2.0,
        last_viz: std::time::Instant::now(),
        marquee_due: None,
        viz_levels: Vec::new(),
        viz_peak: 0.0,
        viz_peaks: Vec::new(),
        viz_held_ms: Vec::new(),
        viz_at: std::time::Instant::now(),
        queue_pending: false,
        rg_started: None,
        resume_stale: false,
        queue_flush: false,
        pending_play: Vec::new(),
        undecodable: std::collections::HashSet::new(),
        paths: Default::default(),
        pending_bt_device: None,
        duration_checked: false,
        last_tick: std::time::Instant::now(),
        boot: std::time::Instant::now(),
        last_scrob: std::time::Instant::now(),
        liked: std::collections::BTreeSet::new(),
        liked_path: None,
        plists: playlists::Store::default(),
        stats: stats::Store::default(),
        listen: stats::Listen::default(),
        books: books::Store::default(),
        books_changed: std::time::Instant::now(),
        book_seek_ms: 0,
        speakers: Vec::new(),
        media_servers: Vec::new(),
        media_server: None,
        media_levels: Vec::new(),
        media_np: None,
        live: None,
        cast_ids: Vec::new(),
        cast_shown: None,
        cast_was_on: false,
        cue: None,
        skin: None,
        tape: None,
        skin_tried: false,
        font_tried: false,
        tape_tried: false,
        tape_pref: String::new(),
        skin_pref: String::new(),
        hpeq_pref: String::new(),
        hpeq_dir: None,
        book_hold: None,
        stats_changed: std::time::Instant::now(),
        views_path: None,
        views_saved: String::new(),
        db_playlists: Vec::new(),
        pending_play_start: 0,
        zappin_candidates: Vec::new(),
        zappin_session: None,
        zappin_generation: 0,
        zappin_confirmed: None,
        zappin_active: false,
        zappin_preview_ms: 4_000,
        zappin_resume_hold: false,
        zappin_handoff_uris: Vec::new(),
        zappin_handoff_restore: false,
        zappin_handoff_restore_uri: None,
        zappin_internal_handoff_ready: false,
        zappin_handoff_confirmed: false,
        zappin_origin_uris: Vec::new(),
        zappin_origin_start: 0,
        zappin_origin_pos_ms: 0,
        zappin_origin_track: None,
        art_full: None,
        art_thumb: None,
        art_cache_keys: std::collections::HashMap::new(),
        art_key: None,
        db_path: None,
    }
}

fn active_zappin_fixture() -> Render {
    let mut r = zappin_render();
    let db = fixture_db();
    let lib = build_library(&db);
    let current = lib.songs.iter().find(|s| s.object_id == 1).unwrap().clone();
    let track = db.track_by_object_id(1).unwrap().unwrap();
    r.app.set_library(lib);
    r.app.set_play_context(vec![current], 0);
    r.last_track = Some(track);
    r.db = Some(db);
    r.play_pos_ms = 12_000;
    zappin_start(&mut r, 4_000);
    assert!(r.zappin_active);
    r
}

/// The production DB stores basename in `object_body.filename` and reconstructs its path from
/// parent folders. This tiny fixture intentionally keeps the direct object_body path for other
/// tests, so callbacks that exercise `track_by_filename` use a basename-backed copy.
fn active_zappin_uri_fixture() -> Render {
    let mut r = active_zappin_fixture();
    let db = r.db.as_ref().unwrap();
    db.conn()
        .execute_batch(
            "UPDATE object_body SET filename='atlas.flac' WHERE object_id=1;
             UPDATE object_body SET filename='box.flac' WHERE object_id=2;
             UPDATE object_body SET filename='harvest.flac' WHERE object_id=3;",
        )
        .unwrap();
    // New paths in the same database: drop the cached map, as reopening it does.
    r.paths = Default::default();
    // Leave the first pass before re-reading the track, or the restart below would restore
    // the pre-rename origin (B1: a held pass always hands its origin back first).
    zappin_release_hold(&mut r);
    r.last_track = r.db.as_ref().unwrap().track_by_object_id(1).unwrap();
    assert_eq!(r.last_track.as_ref().unwrap().duration_raw, Some(272_000));
    zappin_start(&mut r, 4_000);
    r
}

#[test]
fn review_zappin_full_same_uri_waits_for_zero_then_restarts_both_listen_clocks() {
    let _global = global_render();
    let mut r = active_zappin_uri_fixture();
    let track = r.last_track.as_ref().unwrap();
    let path = track.filename.clone();
    let length_s = (track.duration_raw.unwrap_or(0) / 1000) as u32;
    r.listen.set_track(&path, length_s);
    assert!(r.listen.tick_ms(true, 89_000).is_none());
    let log = std::env::temp_dir().join(format!("zappin-full-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let mut scrob = scrobble::Scrobbler::new(&log, "test");
    let meta = scrobble::Track {
        artist: track.artist.clone(),
        album: track.album.clone(),
        title: track.title.clone(),
        track_no: track.track_no.max(0) as u32,
        length_s,
    };
    scrob.set_track(meta, 1);
    scrob.tick_ms(true, 89_000);
    r.scrob = Some(scrob);
    assert_eq!(
        carry_action(&mut r, &cinder_ui::nav::Action::ZappinFull),
        Some(54)
    );
    assert!(
        r.zappin_resume_hold,
        "the handoff remains held until PlayerService confirms it"
    );

    *cell().lock().unwrap() = Some(r);
    let uri = std::ffi::CString::new(path).unwrap();
    // A stale sample-position event cannot release the hold or start listen credit.
    assert_eq!(cinder_set_now_playing_uri(uri.as_ptr(), 0.28, 1, 80), 0);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(
        cinder_zappin_confirm_handoff(uri.as_ptr(), 0.28, 1),
        0,
        "the explicit confirmation API rejects the stale sample offset"
    );
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    // Metadata is still metadata-only, even when the callback happens to carry zero.
    assert_eq!(cinder_set_now_playing_uri(uri.as_ptr(), 0.0, 1, 80), 0);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(
        cinder_zappin_confirm_handoff(uri.as_ptr(), 0.0, 1),
        1,
        "same-URI Full releases only after explicit live-position confirmation"
    );
    let mut r = cell().lock().unwrap().take().unwrap();
    assert!(!r.zappin_resume_hold);
    assert!(r.listen.tick_ms(true, 1_000).is_none());
    r.scrob.as_mut().unwrap().tick_ms(true, 1_000);
    assert!(!log.exists() || !std::fs::read_to_string(&log).unwrap().contains("\tL\t"));
    let _ = std::fs::remove_file(log);
}

#[test]
fn review_zappin_source_exit_restores_original_track_and_releases_hold() {
    let _global = global_render();
    let mut r = active_zappin_uri_fixture();
    // A sample of another track has been reported: last_track/play_pos are the SAMPLE's.
    r.last_track = r.db.as_ref().unwrap().track_by_object_id(2).unwrap();
    r.play_pos_ms = 45_000;
    let path = std::env::temp_dir().join(format!("zappin-resume-{}.txt", std::process::id()));
    let original = "track=1\npos=12000\n";
    std::fs::write(&path, "stale").unwrap();
    r.resume_pos_path = Some(path.to_string_lossy().into_owned());
    *cell().lock().unwrap() = Some(r);
    // FM/USB/power abandon: no confirmation will ever come, so the hold is released now and
    // the ordinary state is the pre-preview track at its bookmark.
    assert_eq!(cinder_zappin_abandon(), 1);
    assert_eq!(cinder_zappin_hold_active(), 0);
    assert_eq!(cinder_play_position_ms(), 12_000);
    cinder_resume_flush();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    // The FM/USB exit re-arms the resume from last_track: it must name the original track.
    cinder_resume_rearm();
    {
        let guard = cell().lock().unwrap();
        let r = guard.as_ref().unwrap();
        assert_eq!(r.last_track.as_ref().unwrap().object_id, 1);
        let (uris, start, pos) = r.resume_pending.clone().unwrap();
        assert!(uris[start].ends_with("atlas.flac"));
        assert_eq!(pos, 12_000);
    }
    assert_eq!(cinder_zappin_abandon(), 0, "a second abandon is a no-op");
    cell().lock().unwrap().take();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn review_zappin_restart_while_held_keeps_the_original_origin() {
    let mut r = active_zappin_uri_fixture();
    let origin = r.zappin_origin_uris.clone();
    // Sample of track 2, then End: the hold stays on, last_track/pos describe the sample.
    r.last_track = r.db.as_ref().unwrap().track_by_object_id(2).unwrap();
    r.play_pos_ms = 45_000;
    zappin_stop(&mut r, true);
    assert!(r.zappin_resume_hold);
    zappin_start(&mut r, 4_000);
    assert!(r.zappin_resume_hold);
    assert_eq!(r.zappin_origin_pos_ms, 12_000, "never the sample's offset");
    assert_eq!(r.zappin_origin_track.as_ref().unwrap().object_id, 1);
    assert_eq!(r.zappin_origin_uris, origin);
    assert_eq!(r.last_track.as_ref().unwrap().object_id, 1);
}

#[test]
fn review_zappin_origin_start_maps_rows_to_playable_uris() {
    let mut r = active_zappin_uri_fixture();
    let library = build_library(r.db.as_ref().unwrap());
    let row = |id| {
        library
            .songs
            .iter()
            .find(|s| s.object_id == id)
            .unwrap()
            .clone()
    };
    let mut missing = row(2);
    missing.object_id = 999; // no file behind it: dropped from the playable list
    r.app.set_play_context(vec![missing, row(1), row(2)], 1);
    zappin_start(&mut r, 4_000);
    assert_eq!(r.zappin_origin_uris.len(), 2);
    assert_eq!(r.zappin_origin_start, 0, "row 1 is the first PLAYABLE uri");
    assert!(zappin_prepare_origin_queue(&mut r));
    assert!(r.pending_play[r.pending_play_start].ends_with("atlas.flac"));
}

#[test]
fn review_zappin_samples_do_not_enter_rewind_history() {
    let _global = global_render();
    let r = active_zappin_uri_fixture();
    *cell().lock().unwrap() = Some(r);
    let sample = std::ffi::CString::new("box.flac").unwrap();
    assert_eq!(cinder_set_now_playing_uri(sample.as_ptr(), 0.1, 1, 80), 0);
    let sample = std::ffi::CString::new("harvest.flac").unwrap();
    assert_eq!(cinder_set_now_playing_uri(sample.as_ptr(), 0.1, 1, 80), 0);
    let r = cell().lock().unwrap().take().unwrap();
    assert!(
        r.play_history.is_empty(),
        "◁ after a preview must not reach a sample"
    );
}

#[test]
fn review_zappin_fresh_handoff_accepts_short_track_inside_absolute_window() {
    let mut r = active_zappin_uri_fixture();
    r.pending_play = vec!["box.flac".into()];
    r.pending_play_start = 0;
    zappin_abandon_session(&mut r);
    zappin_arm_handoff(&mut r, false);
    let mut short =
        r.db.as_ref()
            .unwrap()
            .track_by_object_id(2)
            .unwrap()
            .unwrap();
    short.duration_raw = Some(60_000);
    r.last_track = Some(short);
    // 2 s into a 60 s track is 3.3 %: outside a 1 % rule, inside the 2.5 s window.
    assert!(!zappin_confirm_handoff(
        &mut r,
        "box.flac",
        3_000.0 / 60_000.0,
        true
    ));
    assert!(zappin_confirm_handoff(
        &mut r,
        "box.flac",
        2_000.0 / 60_000.0,
        true
    ));
    assert!(!r.zappin_resume_hold);
    assert_eq!(
        r.play_history.last().map(|t| t.object_id),
        Some(1),
        "◁ from the new play returns to the pre-preview track"
    );
}

#[test]
fn review_zappin_release_of_fresh_handoff_starts_the_new_track() {
    let mut r = active_zappin_uri_fixture();
    r.pending_play = vec!["box.flac".into()];
    r.pending_play_start = 0;
    zappin_abandon_session(&mut r);
    zappin_arm_handoff(&mut r, false);
    r.last_track = r.db.as_ref().unwrap().track_by_object_id(2).unwrap();
    r.play_pos_ms = 45_000; // stale sample offset
                            // A timeout / source change: released without a confirmation, as a fresh play.
    zappin_release_hold(&mut r);
    assert!(!r.zappin_resume_hold);
    assert!(r.zappin_handoff_uris.is_empty());
    assert_eq!(r.last_track.as_ref().unwrap().object_id, 2);
    assert_eq!(r.play_pos_ms, 0);
}

#[test]
fn review_zappin_end_then_play_restores_queue_position_and_same_uri_listen() {
    let _global = global_render();
    let mut r = active_zappin_uri_fixture();
    let path = std::env::temp_dir().join(format!("zappin-resume-play-{}.txt", std::process::id()));
    r.resume_pos_path = Some(path.to_string_lossy().into_owned());
    std::fs::write(&path, "track=1\npos=12000\n").unwrap();
    let track = r.last_track.as_ref().unwrap();
    let track_path = track.filename.clone();
    let expected_pending = track_path.clone();
    let uri = std::ffi::CString::new(track_path.clone()).unwrap();
    r.listen.set_track(&track.filename, 272);
    assert!(r.listen.tick_ms(true, 89_000).is_none());
    let scrob_path = std::env::temp_dir().join("zappin-resume-play.log");
    let mut scrob = scrobble::Scrobbler::new(&scrob_path, "test");
    scrob.set_track(
        scrobble::Track {
            artist: track.artist.clone(),
            album: track.album.clone(),
            title: track.title.clone(),
            track_no: track.track_no.max(0) as u32,
            length_s: 272,
        },
        1,
    );
    scrob.tick_ms(true, 89_000);
    r.scrob = Some(scrob);
    *cell().lock().unwrap() = Some(r);

    cinder_zappin_end();
    assert_eq!(cinder_zappin_prepare_normal_queue(), 1);
    {
        let guard = cell().lock().unwrap();
        let r = guard.as_ref().unwrap();
        assert_eq!(r.pending_play, vec![expected_pending]);
        assert_eq!(r.pending_play_start, 0);
        assert!(r.zappin_resume_hold);
        assert_eq!(r.play_pos_ms, 12_000);
    }
    assert_eq!(
        cinder_set_now_playing_uri(uri.as_ptr(), 12_000.0 / 272_000.0, 1, 80),
        0
    );
    assert!(
        cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold,
        "metadata updates never release the persistence hold"
    );
    assert_eq!(
        cinder_zappin_confirm_handoff(uri.as_ptr(), 12_000.0 / 272_000.0, 1),
        1,
        "same-URI restore confirms only at the captured bookmark"
    );
    assert_eq!(
        cinder_zappin_take_handoff_confirmed(),
        1,
        "a live matching URI at the restored checkpoint explicitly confirms the handoff"
    );
    assert_eq!(
        cinder_zappin_take_handoff_confirmed(),
        0,
        "confirmation is consumed once"
    );
    let mut r = cell().lock().unwrap().take().unwrap();
    assert!(!r.zappin_resume_hold);
    assert_eq!(
        r.listen.tick_ms(true, 47_000),
        Some(track_path.as_str()),
        "same URI restores its prior listen clock"
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "track=1\npos=12000\n"
    );
    let _ = std::fs::remove_file(scrob_path);
}

#[test]
fn review_zappin_restore_handoff_accepts_next_track_from_prepared_queue() {
    let _global = global_render();
    let mut r = active_zappin_uri_fixture();
    r.zappin_origin_uris = vec!["atlas.flac".into(), "box.flac".into()];
    r.zappin_origin_start = 0;
    r.zappin_origin_pos_ms = 12_000;
    *cell().lock().unwrap() = Some(r);

    cinder_zappin_end();
    assert_eq!(cinder_zappin_prepare_normal_queue(), 1);
    {
        let guard = cell().lock().unwrap();
        let r = guard.as_ref().unwrap();
        assert_eq!(r.pending_play, vec!["atlas.flac", "box.flac"]);
        assert_eq!(r.zappin_handoff_uris, vec!["atlas.flac", "box.flac"]);
        assert_eq!(r.zappin_handoff_restore_uri.as_deref(), Some("atlas.flac"));
        assert!(r.zappin_handoff_restore);
    }

    // PlayerService can advance before its first callback. A live URI from the restored normal
    // queue confirms the handoff even when it is not the queue's original start URI, but stale
    // queue positions and URIs outside the prepared sequence do not.
    let outside = std::ffi::CString::new("outside.flac").unwrap();
    assert_eq!(cinder_set_now_playing_uri(outside.as_ptr(), 0.0, 1, 80), -1);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(cinder_zappin_confirm_handoff(outside.as_ptr(), 0.0, 1), 0);
    let next = std::ffi::CString::new("box.flac").unwrap();
    assert_eq!(cinder_set_now_playing_uri(next.as_ptr(), 0.20, 1, 80), 0);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(
        cinder_zappin_confirm_handoff(next.as_ptr(), 0.20, 1),
        0,
        "the next URI must begin at zero, not inherit the sample offset"
    );
    assert_eq!(cinder_set_now_playing_uri(next.as_ptr(), 0.0, 1, 80), 0);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(cinder_zappin_confirm_handoff(next.as_ptr(), 0.0, 1), 1);
    assert_eq!(cinder_zappin_take_handoff_confirmed(), 1);
    let r = cell().lock().unwrap().take().unwrap();
    assert!(!r.zappin_resume_hold);
    assert_eq!(r.last_track.as_ref().unwrap().filename, "box.flac");
}

#[test]
fn review_zappin_explicit_new_song_arms_handoff_only_after_pending_queue_exists() {
    let _global = global_render();
    let mut r = active_zappin_uri_fixture();
    let path = std::env::temp_dir().join(format!("zappin-new-song-{}.txt", std::process::id()));
    r.resume_pos_path = Some(path.to_string_lossy().into_owned());
    std::fs::write(&path, "track=1\npos=12000\n").unwrap();
    r.pending_play = vec!["box.flac".into()];
    r.pending_play_start = 0;
    *cell().lock().unwrap() = Some(r);
    assert_eq!(cinder_zappin_abandon_for_normal_play(), 1);
    {
        let guard = cell().lock().unwrap();
        let r = guard.as_ref().unwrap();
        assert!(r.zappin_resume_hold);
        assert_eq!(r.zappin_handoff_uris, vec!["box.flac"]);
        assert!(
            !r.zappin_handoff_restore,
            "explicit selection starts a fresh Listen clock"
        );
        assert_eq!(
            r.pending_play,
            vec!["box.flac"],
            "abandon preserves newly built queue"
        );
    }
    let uri = std::ffi::CString::new("box.flac").unwrap();
    assert_eq!(cinder_set_now_playing_uri(uri.as_ptr(), 0.0, 1, 80), 0);
    assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
    assert_eq!(cinder_zappin_confirm_handoff(uri.as_ptr(), 0.0, 1), 1);
    assert_eq!(cinder_zappin_take_handoff_confirmed(), 1);
    cinder_resume_flush();
    let mut r = cell().lock().unwrap().take().unwrap();
    assert!(!r.zappin_resume_hold);
    assert_eq!(r.last_track.as_ref().unwrap().filename, "box.flac");
    assert!(
        r.listen.tick_ms(true, 1_000).is_none(),
        "new normal song starts its own clock at zero"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "track=2\npos=0\n");
    let _ = std::fs::remove_file(r.resume_pos_path.unwrap());
}

#[test]
fn review_zappin_internal_shuffle_and_queue_change_cancel_before_mutation() {
    let _global = global_render();
    for action in [
        cinder_ui::nav::Action::ShuffleToggle,
        cinder_ui::nav::Action::QueueChanged,
    ] {
        let mut r = active_zappin_uri_fixture();
        let db = r.db.as_ref().unwrap();
        let mut tracks = Vec::new();
        let library = build_library(db);
        for id in [1, 2, 3] {
            tracks.push(
                library
                    .songs
                    .iter()
                    .find(|s| s.object_id == id)
                    .unwrap()
                    .clone(),
            );
        }
        r.app.set_play_context(tracks, 0);
        zappin_start(&mut r, 4_000);
        let result = carry_action(&mut r, &action);
        assert!(
            !r.zappin_active,
            "internal FFI action must invalidate preview identity: {action:?}"
        );
        assert!(
            r.zappin_resume_hold,
            "cancelling a preview is not a normal-play handoff"
        );
        assert!(r.zappin_session.is_none());
        assert!(
            r.queue_flush,
            "the shell must receive a complete post-mutation sequence"
        );
        assert_eq!(
            r.pending_play.len(),
            3,
            "never leave PlayerService on the one-URI sample"
        );
        assert_eq!(
            r.pending_play_start, 0,
            "the current preview occurrence becomes ordinary play from zero"
        );
        assert!(r.pending_play[r.pending_play_start].ends_with("atlas.flac"));
        assert_eq!(
            r.play_pos_ms, 0,
            "sample progress is never handed to ordinary playback"
        );
        assert!(!r.zappin_handoff_restore, "this is a new full-play clock");
        assert!(r.zappin_internal_handoff_ready);
        if matches!(action, cinder_ui::nav::Action::ShuffleToggle) {
            assert_eq!(
                result,
                Some(36),
                "the shell must stop owning its preview sequence"
            );
            assert!(r.np.shuffle);
        } else {
            assert_eq!(result, Some(36));
            assert!(
                !r.queue_pending,
                "the full queue flush supersedes deferred mutation"
            );
        }

        let uri = std::ffi::CString::new(r.pending_play[r.pending_play_start].clone()).unwrap();
        *cell().lock().unwrap() = Some(r);
        assert_eq!(
            cinder_zappin_abandon_internal_mutation(),
            1,
            "shell cancellation clears local session state"
        );
        {
            let guard = cell().lock().unwrap();
            let r = guard.as_ref().unwrap();
            assert_eq!(
                r.pending_play.len(),
                3,
                "shell cancellation preserves the prepared full sequence"
            );
            assert!(
                r.zappin_resume_hold,
                "hold remains until PlayerService confirms the URI"
            );
            assert!(r.zappin_internal_handoff_ready);
        }
        assert_eq!(cinder_set_now_playing_uri(uri.as_ptr(), 0.0, 1, 80), 0);
        assert!(cell().lock().unwrap().as_ref().unwrap().zappin_resume_hold);
        assert_eq!(cinder_zappin_confirm_handoff(uri.as_ptr(), 0.0, 1), 1);
        let mut r = cell().lock().unwrap().take().unwrap();
        assert!(
            !r.zappin_resume_hold,
            "confirmed ordinary play releases the hold"
        );
        assert!(!r.zappin_internal_handoff_ready);
        assert!(
            r.listen.tick_ms(true, 1_000).is_none(),
            "the full track clock starts at zero"
        );
    }
}

#[test]
fn review_zappin_stale_candidate_callback_after_advance_is_rejected() {
    let _global = global_render();
    let mut r = active_zappin_fixture();
    let db = r.db.as_ref().unwrap();
    let library = build_library(db);
    let first = library
        .songs
        .iter()
        .find(|s| s.object_id == 1)
        .unwrap()
        .clone();
    let middle = library
        .songs
        .iter()
        .find(|s| s.object_id == 2)
        .unwrap()
        .clone();
    // A/B/A is deliberate: the first and third URI are identical, but they are distinct
    // occurrences and an old callback for index 0 must not confirm index 2.
    let tracks = vec![first.clone(), middle, first];
    r.app.set_play_context(tracks, 0);
    zappin_start(&mut r, 4_000);
    assert_eq!(r.zappin_candidates.len(), 3);
    let generation = r.zappin_generation;
    let first = r.zappin_candidates[0].clone();
    let last = r.zappin_candidates[2].clone();
    assert_eq!(first.uri, last.uri);
    assert_eq!(first.object_id, last.object_id);
    *cell().lock().unwrap() = Some(r);
    assert_eq!(cinder_zappin_advance(1), 1);
    assert_eq!(cinder_zappin_advance(1), 2);
    assert_eq!(
        cinder_zappin_confirm_candidate(
            generation,
            0,
            first.object_id,
            std::ffi::CString::new(first.uri.clone()).unwrap().as_ptr(),
        ),
        0,
        "late callback for the first A occurrence is rejected"
    );
    assert_eq!(
        cinder_zappin_confirm_candidate(
            generation,
            2,
            last.object_id,
            std::ffi::CString::new(last.uri).unwrap().as_ptr(),
        ),
        1,
        "the current A occurrence is accepted by its own index"
    );
    let r = cell().lock().unwrap().take().unwrap();
    let session = r.zappin_session.unwrap();
    assert_eq!(session.index(), 2);
    assert!(session.accepts(generation, 2));
    assert!(!session.accepts(generation, 0));
    assert!(r.zappin_resume_hold);
}

#[test]
fn review_zappin_length_switch_updates_future_plans_and_session() {
    let mut r = active_zappin_fixture();
    assert_eq!(r.zappin_preview_ms, 4_000);
    assert!(r
        .zappin_candidates
        .iter()
        .all(|c| c.plan.length_ms == Some(4_000)));
    assert_eq!(
        carry_action(&mut r, &cinder_ui::nav::Action::ZappinLength(15_000)),
        Some(57)
    );
    assert_eq!(r.zappin_preview_ms, 15_000);
    assert_eq!(r.zappin_session.unwrap().preview_ms(), 15_000);
    assert!(r
        .zappin_candidates
        .iter()
        .all(|c| c.plan.length_ms == Some(15_000)));
    // The already-playing shell sample retains its captured bound; only a future candidate
    // reads the refreshed plan and receives the new 15-second limit.
    assert_eq!(r.zappin_candidates[0].plan.length_ms, Some(15_000));
    assert!(r.zappin_resume_hold);
}
