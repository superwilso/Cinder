//! Device-navigation simulator — drives the EXACT on-device navigator (`cinder_ui::nav::App`)
//! from the keyboard, in a 480x800 window (shown on the Windows desktop via WSLg). This is the
//! real device UX: the same state machine, screens, scrolling, volume HUD, album drill-in and
//! day/night the panel runs — only the hardware buttons are replaced by keys.
//!
//! Run:  cd player && cargo run -p cinder-sim --bin device
//!
//! Keys → device buttons:
//!   Arrows = Up/Down/Left/Right   Enter = Select   Backspace = Back   Tab = Option
//!   Space = Play/Pause   = / -    = Volume +/-     H = Home   P = Power (screen on/off)
//!   L = the HOLD SWITCH (lock/unlock) — the navigator boots LOCKED, and per the device only the
//!   Hold switch unlocks (Power just toggles the panel), so without this key the sim could not be
//!   woken at all. Q/Esc = quit
//!
//! **Mouse = the touchscreen.** The NW-A55 has no d-pad — touch is its primary navigation — so a
//! keyboard-only sim could not reach most of the UI (the Shelf sheet, the progress rail, list rows,
//! the library tabs, the Settings sliders). The pointer is wired through the SAME classifier
//! `cinder-home/src/main.cpp` runs on real evdev frames, with the same thresholds, so a gesture
//! here takes the same path it takes on the panel:
//!   left-edge → right = Back · ~stationary = tap · mostly-vertical past slop = live drag + fling
//!   · horizontal = swipe · and a contact the UI CLAIMS (`scrub_begin`) drives a slider instead.

use cinder_ui::nav::{App, Button};
use cinder_ui::now_playing::NowPlaying;
use cinder_ui::{Canvas, FontSet, Library, H, W};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

/// Pointer state for the touch classifier — mirrors the shell's `g_touch_*` / `g_drag_*` globals.
#[derive(Default)]
struct Touch {
    down: bool,
    start: (i32, i32),
    cur: (i32, i32),
    drag: bool,
    last_y: i32,
    scrub: bool,
}

impl Touch {
    /// Contact begins. The UI gets first refusal on the gesture (progress rail / UI-scale slider);
    /// a claim routes everything to `scrub_*` and skips tap/swipe/drag entirely.
    fn press(&mut self, app: &mut App, x: i32, y: i32) {
        app.stop_fling();
        *self = Touch { down: true, start: (x, y), cur: (x, y), last_y: y, ..Default::default() };
        self.scrub = app.scrub_begin(x, y);
    }

    /// Contact moves. Vertical past a 12px slop promotes to a live drag (streams pixel deltas).
    fn motion(&mut self, app: &mut App, x: i32, y: i32) {
        if !self.down {
            return;
        }
        self.cur = (x, y);
        if self.scrub {
            app.scrub_move(x, y);
            return;
        }
        if !self.drag {
            let (dx, dy) = (x - self.start.0, y - self.start.1);
            if dy.abs() > 12 && dy.abs() > dx.abs() {
                self.drag = true;
                self.last_y = y;
            }
            return;
        }
        let d = y - self.last_y;
        if d != 0 {
            app.scroll_px(-d); // finger up = show later rows
            self.last_y = y;
        }
    }

    /// Contact ends — classify exactly as the shell does at finger-up.
    fn release(&mut self, app: &mut App) -> Vec<cinder_ui::nav::Action> {
        let acts = if !self.down {
            vec![]
        } else if self.scrub {
            app.scrub_end()
        } else if self.drag {
            vec![] // (the shell hands the release velocity to cinder_touch_fling here)
        } else {
            let (sx, sy) = self.start;
            let (cx, cy) = self.cur;
            let (dx, dy) = (cx - sx, cy - sy);
            if sx <= 38 && dx >= 120 {
                app.press(Button::Back) // left-edge → rightward
            } else if dx.abs() < 26 && dy.abs() < 26 {
                app.tap(cx, cy) // ~stationary (26px: sloppy thumbs read as micro-drags)
            } else if dx.abs() > dy.abs() && dx.abs() >= 60 {
                app.swipe(if dx < 0 { -1 } else { 1 }, sx, sy)
            } else {
                vec![]
            }
        };
        *self = Touch::default();
        acts
    }
}

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
    // Boot LOCKED, like the device — unless `--unlocked` is passed, which matches what the real
    // shell actually does at bring-up (`render_up()` constructs `App::unlocked()`). Needed for
    // scripted/headless runs: only the Hold switch unlocks, and a synthetic X server with no
    // window manager can't deliver the keystroke.
    let boot = std::time::Instant::now();
    let unlocked = std::env::args().any(|a| a == "--unlocked");
    let mut app = if unlocked { App::unlocked() } else { App::new() };
    app.set_library(big_library());

    // Sample now-playing (on the device this is pushed from PlayerService each second).
    let mut np = NowPlaying {
        title: if std::env::args().any(|a| a == "--long-title") {
            "Sinfonia concertante for Violin, Viola and Orchestra in E-flat major, K. 364 — III. Presto"
        } else {
            "Atlas Hands"
        },
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
        scrubbing: false,
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
    let mut touch = Touch::default();
    let mut was_down = false;
    while window.is_open() && !window.is_key_down(Key::Q) && !window.is_key_down(Key::Escape) {
        for k in window.get_keys_pressed(KeyRepeat::Yes) {
            if k == Key::L {
                app.set_hold(!app.locked); // the physical Hold switch — the ONLY unlock
            } else if let Some(b) = key_to_button(k) {
                app.press(b);
            } else if k == Key::V {
                app.set_viz_kind((app.viz_kind() + 1) % cinder_ui::viz::COUNT); // cycle visualiser type
            }
        }
        // ── touchscreen ──
        let down = window.get_mouse_down(MouseButton::Left);
        let pos = window.get_mouse_pos(MouseMode::Clamp).map(|(x, y)| (x as i32, y as i32));
        if let Some((mx, my)) = pos {
            if down && !was_down {
                touch.press(&mut app, mx, my);
            } else if down {
                touch.motion(&mut app, mx, my);
            }
        }
        if !down && was_down {
            let acts = touch.release(&mut app);
            // The device shell hands these to PlayerService; here, print them so a scripted run
            // can see WHICH action a gesture produced, not just the frame it left behind.
            for a in &acts {
                println!("action: {a:?}");
            }
        }
        was_down = down;
        app.tick(); // advance HUD/overlay countdowns (volume), like the device pump
        // Advance the title marquee, exactly as cinder_render_tick does on device. Without this
        // the sim renders long titles frozen at phase 0 and the scroll looks broken here while
        // working on hardware — the sim's whole value is that a gesture takes the same path.
        cinder_ui::widgets::set_marquee_ms(boot.elapsed().as_millis() as u32);
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
            album_ids: Vec::new(),
        })
        .collect();
    songs.sort_by(|x, y| x.title.cmp(&y.title));
    Library { songs, album_groups, artists: artist_rows, playlists: Vec::new(), thumbs: Default::default(), genres: Vec::new(), ..Default::default() }
}
