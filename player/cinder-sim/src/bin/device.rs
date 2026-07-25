//! Device-navigation simulator — drives the EXACT on-device navigator (`cinder_ui::nav::App`)
//! from the keyboard, in a 480x800 window (shown on the Windows desktop via WSLg). This is the
//! real device UX: the same state machine, screens, scrolling, volume HUD, album drill-in and
//! day/night the panel runs — only the hardware buttons are replaced by keys.
//!
//! Run:  cd player && cargo run -p cinder-sim --bin device
//!
//! Keys → device buttons:
//!   Arrows = Up/Down/Left/Right   Enter = Select   Backspace = Back   Tab = Option
//!   Space = Play/Pause   = / -    = Volume +/-     H = Home   P = Power (lock/sleep)
//!   (the navigator boots LOCKED — press any key to wake, like the device)   Q/Esc = quit

use cinder_ui::nav::{App, Button};
use cinder_ui::now_playing::NowPlaying;
use cinder_ui::{Canvas, FontSet, Library, H, W};
use minifb::{Key, KeyRepeat, Window, WindowOptions};

fn key_to_button(k: Key) -> Option<Button> {
    Some(match k {
        Key::Up => Button::Up,
        Key::Down => Button::Down,
        Key::Left => Button::Left,
        Key::Right => Button::Right,
        Key::Enter => Button::Select,
        Key::Backspace => Button::Back,
        Key::Tab => Button::Option,
        Key::Space => Button::Play,
        Key::Equal => Button::VolUp,
        Key::Minus => Button::VolDown,
        Key::H => Button::Home,
        Key::P => Button::Power,
        _ => return None,
    })
}

fn main() {
    let fonts = FontSet::load();
    // Boot LOCKED, like the device. A bigger sample library so scrolling is visible.
    let mut app = App::new();
    app.set_library(big_library());

    // Sample now-playing (on the device this is pushed from PlayerService each second).
    let mut np = NowPlaying {
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
        viz_levels: None,
    };

    let mut window = Window::new(
        "Cinder · NW-A55  [arrows·Enter·Backspace·Tab·Space·=/-vol·H·P·V=viz · Q quits]",
        W,
        H,
        WindowOptions { scale: minifb::Scale::X2, ..WindowOptions::default() },
    )
    .expect("open window (WSLg provides the display)");
    window.set_target_fps(60);

    let mut c = Canvas::new();
    while window.is_open() && !window.is_key_down(Key::Q) && !window.is_key_down(Key::Escape) {
        for k in window.get_keys_pressed(KeyRepeat::Yes) {
            if let Some(b) = key_to_button(k) {
                app.press(b);
            } else if k == Key::V {
                app.set_viz_kind((app.viz_kind() + 1) % cinder_ui::viz::COUNT); // cycle visualiser type
            }
        }
        app.tick(); // advance HUD/overlay countdowns (volume), like the device pump
        // Animate the visualiser while "playing" on Now Playing (mirrors cinder-ffi on device).
        if np.playing && app.is_now_playing() && app.viz_on() {
            np.viz_seed += 0.15;
        }
        app.render(&mut c, &fonts, &np);
        window.update_with_buffer(&c.buf, W, H).expect("blit");
    }
}

/// A few hundred rows so the windowed list scrolling + album drill-in are exercisable.
fn big_library() -> Library {
    use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, SongRow};
    let artists = [
        "Benjamin Francis Leftwich", "Cold Stone & Sea", "Glass Atlas", "Hollow Pines",
        "Neon Cartography", "Petal & Wire", "Vesper Lane", "Aurora Bay", "Slow Tide",
    ];
    let kinds = ["Drift", "Ember", "Lantern", "Quartz", "Halcyon", "Bloom"];
    let albums_n = ["Nightfall", "Driftwood", "Halo", "Cinder", "Last Smoke", "After the Rain"];

    let mut songs = Vec::new();
    let mut album_groups = Vec::new();
    let mut oid = 0i64;
    for (ai, a) in artists.iter().enumerate() {
        let mut group_albums = Vec::new();
        for k in 0..(3 + ai % 3) {
            let aname = format!("{} — {}", albums_n[(ai + k) % albums_n.len()], k + 1);
            let ntr = 8 + (k as u32 % 4);
            let mut track_list = Vec::new();
            for i in 0..ntr {
                let row = SongRow {
                    title: format!("{} {}", kinds[(i as usize + k) % kinds.len()], i + 1),
                    artist: a.to_string(),
                    dur: format!("{}:{:02}", 3 + (i % 4), (i * 13) % 60),
                    art: aname.clone(),
                    object_id: oid,
                    album_id: (ai * 10 + k) as i64,
                    disc: 1,
                    track: i as i32 + 1,
                    year: (2009 + (ai + k) % 15) as i32,
                    ..Default::default()
                };
                oid += 1;
                track_list.push(row.clone());
                songs.push(row);
            }
            group_albums.push(AlbumRow {
                name: aname.clone(),
                artist: a.to_string(),
                year: format!("{}", 2009 + (ai + k) % 15),
                tracks: ntr,
                art: aname,
                album_id: (ai * 10 + k) as i64,
                added: (2009 + (ai + k) % 15) as i64,
                track_list,
                ..Default::default()
            });
        }
        album_groups.push(ArtistGroup { artist: a.to_string(), albums: group_albums });
    }
    let artist_rows = artists
        .iter()
        .enumerate()
        .map(|(ai, a)| ArtistRow {
            name: a.to_string(),
            albums: (3 + ai % 3) as u32,
            tracks: 30 + ai as u32,
            arts: vec![format!("{a} 0"), format!("{a} 1")],
        })
        .collect();
    songs.sort_by(|x, y| x.title.cmp(&y.title));
    Library { songs, album_groups, artists: artist_rows, playlists: Vec::new() }
}
