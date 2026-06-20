//! Host preview backend: render every screen to PNG for device-free iteration.

use cinder_ui::bluetooth::Bt;
use cinder_ui::library::Tab;
use cinder_ui::menu::MenuItem;
use cinder_ui::sound::Sound;
use cinder_ui::{
    bluetooth, eq, fm, library, lock, menu, now_playing, pairing, receiver, settings, sound,
    up_next, usbdac, Canvas, FontSet, Theme, H, W,
};

fn save(c: &Canvas, name: &str) {
    let img = image::RgbImage::from_raw(W as u32, H as u32, c.to_rgb_bytes()).expect("buffer size");
    let path = format!("out/{name}.png");
    img.save(&path).expect("save png");
    println!("wrote {path}");
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
        liked: true,
        playing: true,
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
    let bt = Bt { on: true, connected: Some("WH-1000XM5"), codec: "LDAC" };
    let eq_bands: [i8; 10] = [2, 3, 1, 0, -1, 0, 2, 3, 2, 1];

    for (name, theme) in [("day", Theme::day()), ("night", Theme::night())] {
        let render_set: &[(&str, &dyn Fn(&mut Canvas))] = &[
            ("now_playing", &|c: &mut Canvas| now_playing::render(c, &theme, &fonts, &np)),
            ("lock", &|c: &mut Canvas| lock::render(c, &theme, &fonts, &lk)),
            ("menu", &|c: &mut Canvas| menu::render(c, &theme, &fonts, &menu_items)),
            ("up_next", &|c: &mut Canvas| up_next::render(c, &theme, &fonts, 0)),
            ("library_songs", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Songs, 0)),
            ("library_albums", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Albums, 0)),
            ("library_artists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Artists, 0)),
            ("library_playlists", &|c: &mut Canvas| library::render(c, &theme, &fonts, Tab::Playlists, 0)),
            ("artist", &|c: &mut Canvas| library::artist(c, &theme, &fonts)),
            ("eq", &|c: &mut Canvas| eq::render(c, &theme, &fonts, &eq_bands, "A1")),
            ("sound", &|c: &mut Canvas| sound::render(c, &theme, &fonts, &snd)),
            ("settings", &|c: &mut Canvas| settings::render(c, &theme, &fonts, theme.night, false)),
            ("bluetooth", &|c: &mut Canvas| bluetooth::render(c, &theme, &fonts, &bt)),
            ("pairing", &|c: &mut Canvas| pairing::render(c, &theme, &fonts, 2, Some(1))),
            ("receiver", &|c: &mut Canvas| receiver::render(c, &theme, &fonts, true)),
            ("fm", &|c: &mut Canvas| fm::render(c, &theme, &fonts, 88.6)),
            ("usbdac", &|c: &mut Canvas| usbdac::render(c, &theme, &fonts, true, "A1", true)),
        ];
        for (screen, draw) in render_set {
            let mut c = Canvas::new();
            draw(&mut c);
            save(&c, &format!("{screen}_{name}"));
        }
    }
}
