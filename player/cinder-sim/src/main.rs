//! Interactive desktop simulator — runs the exact cinder-ui screens in a
//! 480x800 window (WSLg shows it on the Windows desktop). Click to navigate;
//! keyboard shortcuts jump to any screen. The Canvas buffer (0x00RRGGBB) is
//! exactly minifb's pixel format, so display is a direct blit.

use cinder_ui::bluetooth::{self, Bt};
use cinder_ui::data::{PAIRED, SONGS};
use cinder_ui::library::{self, Tab};
use cinder_ui::menu::{self, MenuItem};
use cinder_ui::sound::{self, Sound};
use cinder_ui::{
    eq, fm, lock, now_playing, pairing, receiver, settings, shelf, up_next, usbdac, Canvas,
    FontSet, Theme, H, W,
};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Lock,
    NowPlaying,
    Menu,
    UpNext,
    Library,
    Artist,
    Eq,
    Sound,
    Settings,
    Bluetooth,
    Pairing,
    Receiver,
    Fm,
    UsbDac,
}

// per-track codec / status badge, indexed parallel to data::SONGS
const CODECS: [(&str, &str); 8] = [
    ("FLAC · 24bit / 96.0 kHz", "FLAC 24/96"),
    ("FLAC · 24bit / 96.0 kHz", "FLAC 24/96"),
    ("DSD · 2.8 MHz", "DSD 2.8"),
    ("FLAC · 24bit / 88.2 kHz", "FLAC 24/88"),
    ("FLAC · 16bit / 44.1 kHz", "FLAC 16/44"),
    ("ALAC · 24bit / 48.0 kHz", "ALAC 24/48"),
    ("FLAC · 24bit / 96.0 kHz", "FLAC 24/96"),
    ("FLAC · 24bit / 192 kHz", "FLAC 24/192"),
];

const VPTS: [&str; 4] = ["Off", "Studio", "Club", "Concert Hall"];
const DCS: [&str; 5] = ["Off", "Standard A", "Standard B", "Low A", "Low B"];
const BT_CODECS: [&str; 4] = ["LDAC", "aptX HD", "aptX", "SBC"]; // transmit cycler (no AAC)
const EQ_PRESETS: [(&str, [i8; 10]); 5] = cinder_ui::data::EQ_PRESETS;
const FM_PRESETS: [f32; 6] = cinder_ui::data::FM_PRESETS;

struct App {
    screen: Screen,
    night: bool,
    playing: bool,
    track: usize,
    liked: bool,
    tab: Tab,
    sort: usize,
    eq_bands: [i8; 10],
    eq_preset: usize,
    dsee: bool,
    vinyl: bool,
    vpt: usize,
    dc: usize,
    normalizer: bool,
    clearaudio: bool,
    bt_on: bool,
    bt_conn: Option<usize>,
    bt_codec: usize,
    fm_freq: f32,
    usb_dac: bool,
    rx: bool,
    shuffle: bool,
    repeat: u8, // 0 off · 1 all · 2 one
    shelf_open: bool,
    pins: [Option<(String, String)>; 3],
    history: Vec<Screen>,
    lib: cinder_ui::Library,
}

fn menu_items(app: &App) -> Vec<MenuItem<'static>> {
    let _ = app;
    vec![
        MenuItem { icon: "note", label: "Now Playing", value: "tap to open", active: true },
        MenuItem { icon: "library", label: "Library", value: "124 albums · 1,842 tracks", active: false },
        MenuItem { icon: "queue", label: "Up Next", value: "8 tracks · 41:24", active: false },
        MenuItem { icon: "radio", label: "FM Radio", value: "88.6 MHz", active: false },
        MenuItem { icon: "eq", label: "Equalizer", value: "Custom A1", active: false },
        MenuItem { icon: "sound", label: "Sound Settings", value: "DSEE HX · VPT · Vinyl", active: false },
        MenuItem { icon: "bt", label: "Bluetooth", value: "WH-1000XM5 · LDAC", active: false },
        MenuItem { icon: "usb", label: "USB-DAC", value: "Off", active: false },
        MenuItem { icon: "rx", label: "BT Receiver", value: "Off", active: false },
        MenuItem { icon: "settings", label: "Settings", value: "System · Storage · About", active: false },
    ]
}

fn hit(x: i32, y: i32, cx: i32, cy: i32, r: i32) -> bool {
    (x - cx).pow(2) + (y - cy).pow(2) <= r * r
}

fn header_back(x: i32, y: i32) -> bool {
    (45..78).contains(&y) && x < 64
}

fn handle_click(app: &mut App, x: i32, y: i32) {
    // Shelf overlay intercepts every click while open.
    if app.shelf_open {
        match shelf::hit(x, y) {
            shelf::ShelfHit::Close => app.shelf_open = false,
            shelf::ShelfHit::Undo => {
                app.shelf_open = false;
                if let Some(s) = app.history.pop() {
                    app.screen = s;
                }
            }
            shelf::ShelfHit::Pin => {
                if let Some(slot) = app.pins.iter().position(|p| p.is_none()) {
                    app.pins[slot] = Some((format!("Now Playing · {}", SONGS[app.track].t), "Just now".into()));
                }
            }
            shelf::ShelfHit::Go(_) => {
                app.shelf_open = false;
                app.screen = Screen::NowPlaying;
            }
            shelf::ShelfHit::Clear(i) => app.pins[i] = None,
            shelf::ShelfHit::None => {}
        }
        return;
    }
    // status-bar ≡ → Menu, bookmark → Shelf (except Lock which wakes first)
    if app.screen != Screen::Lock && y < 34 && (356..384).contains(&x) {
        app.screen = Screen::Menu;
        return;
    }
    if app.screen != Screen::Lock && y < 34 && (386..414).contains(&x) {
        app.shelf_open = true;
        return;
    }
    // header back chevron → Menu on sub-screens
    if matches!(
        app.screen,
        Screen::UpNext | Screen::Library | Screen::Artist | Screen::Eq | Screen::Sound | Screen::Settings | Screen::Bluetooth | Screen::Pairing | Screen::Receiver | Screen::Fm | Screen::UsbDac
    ) && header_back(x, y)
    {
        app.screen = if app.screen == Screen::Artist { Screen::Library } else { Screen::Menu };
        return;
    }

    match app.screen {
        Screen::Lock => app.screen = Screen::NowPlaying,
        Screen::Menu => {
            if y >= 91 {
                let row = (y - 91) / 63;
                app.screen = match row {
                    0 => Screen::NowPlaying,
                    1 => Screen::Library,
                    2 => Screen::UpNext,
                    3 => Screen::Fm,
                    4 => Screen::Eq,
                    5 => Screen::Sound,
                    6 => Screen::Bluetooth,
                    7 => Screen::UsbDac,
                    8 => Screen::Receiver,
                    9 => Screen::Settings,
                    _ => Screen::Menu,
                };
            }
        }
        Screen::NowPlaying => {
            if hit(x, y, 240, 692, 42) {
                app.playing = !app.playing;
            } else if hit(x, y, 130, 692, 30) {
                app.track = (app.track + SONGS.len() - 1) % SONGS.len();
            } else if hit(x, y, 350, 692, 30) {
                app.track = (app.track + 1) % SONGS.len();
            } else if hit(x, y, 44, 692, 28) {
                app.shuffle = !app.shuffle;
            } else if hit(x, y, 436, 692, 28) {
                app.repeat = (app.repeat + 1) % 3;
            } else if y > 744 {
                // bottom toolbar: heart · queue · eq · bt · library
                if x < 96 {
                    app.liked = !app.liked;
                } else if x < 192 {
                    app.screen = Screen::UpNext;
                } else if x < 288 {
                    app.screen = Screen::Eq;
                } else if x < 384 {
                    app.screen = Screen::Bluetooth;
                } else {
                    app.screen = Screen::Library;
                }
            }
        }
        Screen::UpNext => {
            if (91..587).contains(&y) {
                let row = ((y - 91) / 62) as usize;
                if row < SONGS.len() {
                    app.track = row;
                    app.playing = true;
                    app.screen = Screen::NowPlaying;
                }
            }
        }
        Screen::Library => {
            if matches!(app.tab, Tab::Songs) && (45..78).contains(&y) && x > 340 {
                app.sort = (app.sort + 1) % 3; // tap the SORT chip in the header
            } else if (95..126).contains(&y) {
                app.tab = if x < 85 { Tab::Songs } else if x < 170 { Tab::Albums } else if x < 268 { Tab::Artists } else { Tab::Playlists };
            } else if (128..200).contains(&y) {
                // the accent "shuffle …" row at the top of every tab
                app.shuffle = true;
                app.playing = true;
                app.screen = Screen::NowPlaying;
            } else if y >= 205 {
                match app.tab {
                    Tab::Songs => {
                        let row = ((y - 205) / 62) as usize;
                        if row < SONGS.len() {
                            app.track = row;
                            app.playing = true;
                            app.screen = Screen::NowPlaying;
                        }
                    }
                    Tab::Artists => app.screen = Screen::Artist,
                    Tab::Albums | Tab::Playlists => {
                        // open the tapped item into Now Playing (play first track)
                        app.playing = true;
                        app.screen = Screen::NowPlaying;
                    }
                }
            }
        }
        Screen::Eq => {
            // preset pills row (y ~ 97..127)
            if (97..127).contains(&y) {
                let idx = ((x - 22) / 60).clamp(0, 4) as usize;
                if idx < EQ_PRESETS.len() {
                    app.eq_preset = idx;
                    app.eq_bands = EQ_PRESETS[idx].1;
                }
            }
        }
        Screen::Sound => {
            if y >= 91 {
                match (y - 91) / 64 {
                    0 => app.dsee = !app.dsee,
                    1 => app.vinyl = !app.vinyl,
                    2 => app.vpt = (app.vpt + 1) % VPTS.len(),
                    3 => app.dc = (app.dc + 1) % DCS.len(),
                    4 => app.normalizer = !app.normalizer,
                    5 => app.clearaudio = !app.clearaudio,
                    _ => {}
                }
            }
        }
        Screen::Settings => {
            // Theme seg (first row after DISPLAY eyebrow, y ~ 115..173, right side)
            if (115..173).contains(&y) && x > 350 {
                app.night = x >= 408;
            } else if (445..503).contains(&y) {
                // "USB mode" row → USB-DAC
                app.screen = Screen::UsbDac;
            }
        }
        Screen::Bluetooth => {
            if (50..78).contains(&y) && x >= 420 {
                app.bt_on = !app.bt_on;
            } else if y >= 700 && y < 752 {
                app.screen = Screen::Pairing; // pair new device
            } else if app.bt_on && app.bt_conn.is_some() && (188..232).contains(&y) {
                // connected-card buttons: Disconnect (left) / Quality (right)
                if x < 240 {
                    app.bt_conn = None;
                } else {
                    app.bt_codec = (app.bt_codec + 1) % BT_CODECS.len();
                }
            } else if app.bt_on {
                let p0 = if app.bt_conn.is_some() { 284 } else { 210 };
                if y >= p0 {
                    let row = ((y - p0) / 58) as usize;
                    if row < PAIRED.len() {
                        app.bt_conn = Some(row);
                        app.bt_codec = if PAIRED[row].kind.contains("LDAC") { 0 } else { 3 };
                    }
                }
            }
        }
        Screen::Pairing => {
            // tap a discovered device → connect & return to Bluetooth
            app.bt_on = true;
            app.bt_conn = Some(0);
            app.bt_codec = 0;
            app.screen = Screen::Bluetooth;
        }
        Screen::Receiver => {
            if (50..78).contains(&y) && x >= 420 {
                app.rx = !app.rx;
            }
        }
        Screen::UsbDac => {
            if (50..78).contains(&y) && x >= 420 {
                app.usb_dac = !app.usb_dac;
            }
        }
        Screen::Fm => {
            // tune buttons row (y ~ 336..380)
            if (336..380).contains(&y) {
                if x < 140 {
                    app.fm_freq = (app.fm_freq - 0.1).max(76.0);
                } else if x < 240 {
                    app.fm_freq = (app.fm_freq - 1.7).max(76.0);
                } else if x < 340 {
                    app.fm_freq = (app.fm_freq + 2.1).min(108.0);
                } else {
                    app.fm_freq = (app.fm_freq + 0.1).min(108.0);
                }
                app.fm_freq = (app.fm_freq * 10.0).round() / 10.0;
            } else if y >= 418 {
                // preset grid (cols 22/170/318, rows 418 & 480, h52)
                let col = if x < 160 { 0 } else if x < 308 { 1 } else { 2 };
                let r = if (418..470).contains(&y) { 0 } else if (480..532).contains(&y) { 1 } else { -1 };
                if r >= 0 {
                    let idx = (r as usize) * 3 + col;
                    if idx < FM_PRESETS.len() {
                        app.fm_freq = FM_PRESETS[idx];
                    }
                }
            }
        }
        Screen::Artist => {
            if (100..122).contains(&y) && x < 64 {
                app.screen = Screen::Library; // artist page's own back chevron
            } else if y > 180 {
                // tap an album / top track → play in Now Playing
                app.playing = true;
                app.screen = Screen::NowPlaying;
            }
        }
    }
}

fn render(app: &App, c: &mut Canvas, theme: &Theme, fonts: &FontSet) {
    let i = app.track;
    let (codec, badge) = CODECS[i];
    match app.screen {
        Screen::Lock => lock::render(c, theme, fonts, &lock::Lock {
            clock: "14:32", big_clock: "23:41", title: SONGS[i].t, artist: SONGS[i].a, badge, battery: 78, progress: 0.39,
        }),
        Screen::NowPlaying => now_playing::render(c, theme, fonts, &now_playing::NowPlaying {
            title: SONGS[i].t, artist: SONGS[i].a, codec, badge, clock: "14:32", battery: 78,
            elapsed: "1:47", remaining: "-2:45", progress: 0.39, art: SONGS[i].art, art_full: None, art_thumb: None, liked: app.liked, playing: app.playing,
            shuffle: app.shuffle, repeat: app.repeat, viz_seed: 2.0, viz_kind: 0, viz_on: true, viz_levels: None, scrubbing: false,
        }),
        Screen::Menu => menu::render(c, theme, fonts, &menu_items(app)),
        Screen::UpNext => {
            let tracks: Vec<cinder_ui::model::SongRow> = SONGS
                .iter()
                .enumerate()
                .map(|(i, s)| cinder_ui::model::SongRow {
                    title: s.t.into(), artist: s.a.into(), dur: s.d.into(), art: s.art.into(), object_id: i as i64,
                    ..Default::default()
                })
                .collect();
            up_next::render(c, theme, fonts, "Now Playing", &tracks, app.track)
        }
        Screen::Library => library::render(c, theme, fonts, app.tab, app.track, 0, app.sort, 0, None, &app.lib),
        Screen::Artist => library::artist(c, theme, fonts),
        Screen::Eq => eq::render(c, theme, fonts, &app.eq_bands, EQ_PRESETS[app.eq_preset].0, 0),
        Screen::Sound => sound::render(c, theme, fonts, &Sound {
            dsee: app.dsee, vinyl: app.vinyl, vpt: VPTS[app.vpt], dcphase: DCS[app.dc],
            normalizer: app.normalizer, clearaudio: app.clearaudio, eq_preset: EQ_PRESETS[app.eq_preset].0,
            bt_codec: if app.bt_on && app.bt_conn.is_some() { Some(BT_CODECS[app.bt_codec]) } else { None },
        }, 0, false),
        Screen::Settings => settings::render(c, theme, fonts, 0,
            &settings::SettingsView { night: app.night, viz_name: "Bars", viz_on: true, usb_dac: app.usb_dac, battery_care: false, storage: "12.4 / 58 GB", sleep: "OFF", brightness: "4 / 5", screen_off: "OFF" }),
        Screen::Bluetooth => bluetooth::render(c, theme, fonts, &Bt {
            on: app.bt_on,
            connected: app.bt_conn.map(|r| PAIRED[r].name),
            codec_sel: app.bt_codec as u8,
            ldac_quality: 0,
        }),
        Screen::Pairing => pairing::render(c, theme, fonts, 3, Some(1)),
        Screen::Receiver => receiver::render(c, theme, fonts, app.rx),
        Screen::Fm => fm::render(c, theme, fonts, app.fm_freq),
        Screen::UsbDac => usbdac::render(c, theme, fonts, app.usb_dac, app.usb_dac && app.bt_on,
            BT_CODECS[app.bt_codec], app.bt_conn.map(|r| PAIRED[r].name), EQ_PRESETS[app.eq_preset].0, app.dsee),
    }

    // Shelf is an overlay drawn on top of the current screen.
    if app.shelf_open {
        let this_title = format!("Now Playing · {}", SONGS[i].t);
        let this_sub = format!("1:47 / {}", SONGS[i].d);
        let pins = [
            app.pins[0].as_ref().map(|(t, s)| shelf::Pin { title: t, sub: s }),
            app.pins[1].as_ref().map(|(t, s)| shelf::Pin { title: t, sub: s }),
            app.pins[2].as_ref().map(|(t, s)| shelf::Pin { title: t, sub: s }),
        ];
        shelf::render(c, theme, fonts, &this_title, &this_sub, &pins);
    }
}

fn main() {
    let fonts = FontSet::load();
    let mut app = App {
        screen: Screen::Lock,
        night: false,
        playing: true,
        track: 0,
        liked: true,
        tab: Tab::Songs,
        sort: 0,
        eq_bands: EQ_PRESETS[3].1,
        eq_preset: 3,
        dsee: true,
        vinyl: false,
        vpt: 1,
        dc: 3,
        normalizer: true,
        clearaudio: false,
        bt_on: true,
        bt_conn: Some(0),
        bt_codec: 0,
        fm_freq: 88.6,
        usb_dac: false,
        rx: false,
        shuffle: false,
        repeat: 1,
        shelf_open: false,
        pins: [None, None, None],
        history: Vec::new(),
        lib: cinder_ui::Library::sample(),
    };

    let mut window = Window::new(
        "NW-A55 · Cinder  [click to navigate · N night · space play · ←/→ track · Esc menu · L lock]",
        W,
        H,
        WindowOptions::default(),
    )
    .expect("open window");
    window.set_target_fps(30);

    let mut mouse_was_down = false;
    let mut last_screen = app.screen;
    while window.is_open() && !window.is_key_down(Key::Q) {
        for k in window.get_keys_pressed(KeyRepeat::No) {
            match k {
                Key::N => app.night = !app.night,
                Key::Space => app.playing = !app.playing,
                Key::Right => app.track = (app.track + 1) % SONGS.len(),
                Key::Left => app.track = (app.track + SONGS.len() - 1) % SONGS.len(),
                Key::Escape => app.screen = Screen::Menu,
                Key::L => app.screen = Screen::Lock,
                Key::Key1 => app.screen = Screen::Lock,
                Key::Key2 => app.screen = Screen::NowPlaying,
                Key::Key3 => app.screen = Screen::Menu,
                Key::U => app.screen = Screen::UpNext,
                Key::B => app.screen = Screen::Library,
                Key::A => app.screen = Screen::Artist,
                Key::E => app.screen = Screen::Eq,
                Key::S => app.screen = Screen::Sound,
                Key::G => app.screen = Screen::Settings,
                Key::T => app.screen = Screen::Bluetooth,
                Key::P => app.screen = Screen::Pairing,
                Key::R => app.screen = Screen::Receiver,
                Key::F => app.screen = Screen::Fm,
                Key::D => app.screen = Screen::UsbDac,
                _ => {}
            }
        }
        let down = window.get_mouse_down(MouseButton::Left);
        if down && !mouse_was_down {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
                handle_click(&mut app, mx as i32, my as i32);
            }
        }
        mouse_was_down = down;

        // Track navigation history for the Shelf's Undo.
        if app.screen != last_screen {
            app.history.push(last_screen);
            if app.history.len() > 64 {
                app.history.remove(0);
            }
            last_screen = app.screen;
        }

        let theme = if app.night { Theme::night() } else { Theme::day() };
        let mut c = Canvas::new();
        render(&app, &mut c, &theme, &fonts);
        window.update_with_buffer(&c.buf, W, H).expect("blit");
    }
}
