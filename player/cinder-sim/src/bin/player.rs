//! The virtual player: the real FFI (cinder-ffi, everything the Home app runs above the C++ shell)
//! on a map of a real player, in a 480x800 window.
//!
//! Run:  tools/map_player.sh MAP            (once, with the player on adb)
//!       cd player && cargo run -p cinder-sim --bin player -- MAP
//!
//! It stands in for the shell, `cinder-home/src/main.cpp`, as far as the UI can tell: it loads the
//! player's settings, resume queue and library database from the map, sets the battery the map
//! read, and passes taps, drags and keys in the way the shell does. What the shell would carry out
//! (play, the EQ, Bluetooth) is printed, not done: there is no Sony service behind it. The shell's
//! own logic is tested by cinder-home/harness instead.
//!
//! The map is never written: its Cinder files are copied into MAP/run/ at start, and the settings
//! save there.
//!
//! Mouse = the touchscreen: click to tap, drag up or down to scroll, drag right from the left edge
//! for Back. Keys: Backspace = Back, Space = Play/Pause, H = Home, = / - = volume, N / B = next /
//! previous, L = the Hold switch, P = Power, Q / Esc = quit.
//!
//! Without a window: `--taps "x,y;x,y"` taps those points a second apart, then `--shot FILE` saves
//! the screen as a PNG and exits — for checking a change against a real player's data in a script.
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const W: usize = 480;
const H: usize = 800;

/// Copy `rel` (a device path) from the map's fs/ into run/, if the map has it.
fn stage(map: &Path, run: &Path, rel: &str) -> Option<PathBuf> {
    let from = map.join("fs").join(rel.trim_start_matches('/'));
    let to = run.join(rel.trim_start_matches('/'));
    if from.is_dir() {
        copy_dir(&from, &to);
    } else if from.is_file() {
        std::fs::create_dir_all(to.parent()?).ok()?;
        std::fs::copy(&from, &to).ok()?;
    } else {
        return None;
    }
    Some(to)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).ok();
    for e in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).ok();
        }
    }
}

fn cpath(p: &Path) -> CString {
    CString::new(p.to_string_lossy().as_bytes()).unwrap()
}

fn read_num(map: &Path, rel: &str) -> Option<i32> {
    std::fs::read_to_string(map.join("fs").join(rel.trim_start_matches('/')))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// What the shell would have done with an action code.
fn carry(what: &str, code: i32) {
    if code != 0 {
        eprintln!("player: {what} -> action {code} (the shell would carry this out)");
    }
}

fn main() {
    let map = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: player MAP (from tools/map_player.sh)"),
    );
    assert!(
        map.join("fs").is_dir(),
        "{} has no fs/: run tools/map_player.sh first",
        map.display()
    );
    let run = map.join("run");
    std::fs::remove_dir_all(&run).ok();
    std::fs::create_dir_all(&run).unwrap();

    let frame = Arc::new(Mutex::new(vec![0u32; W * H]));
    let sink = frame.clone();
    cinder_ffi::set_virtual_present(Box::new(move |buf: &[u32]| {
        if let Ok(mut f) = sink.lock() {
            f.copy_from_slice(&buf[..W * H]);
        }
    }));
    if let Some(art) = stage(&map, &run, "/data/cinder/artcache") {
        std::env::set_var("CINDER_ART_CACHE", art);
    }
    assert_eq!(cinder_ffi::cinder_render_init(), 0, "render init");

    // The shell's boot order: settings, then the library, then the resume queue.
    match stage(&map, &run, "/contents/cinder_settings.conf") {
        Some(p) => eprintln!(
            "player: settings -> {}",
            cinder_ffi::cinder_settings_load(cpath(&p).as_ptr())
        ),
        None => eprintln!("player: the map has no settings file — defaults"),
    }
    match stage(&map, &run, "/db/MTPDB.dat") {
        Some(p) => eprintln!(
            "player: library open -> {}",
            cinder_ffi::cinder_db_open(cpath(&p).as_ptr())
        ),
        None => eprintln!("player: the map has no /db/MTPDB.dat — empty library"),
    }
    if let (Some(q), Some(r)) = (
        stage(&map, &run, "/data/cinder/queue.conf"),
        stage(&map, &run, "/data/cinder/resume.conf"),
    ) {
        eprintln!(
            "player: resume -> {}",
            cinder_ffi::cinder_resume_load(cpath(&q).as_ptr(), cpath(&r).as_ptr())
        );
    }
    if let Some(pct) = read_num(&map, "/sys/class/power_supply/battery/capacity") {
        cinder_ffi::cinder_set_battery(pct);
    }

    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if let Some(shot) = flag("--shot") {
        let taps: Vec<(i32, i32)> = flag("--taps")
            .unwrap_or_default()
            .split(';')
            .filter_map(|t| t.split_once(','))
            .filter_map(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
            .collect();
        let settle = |secs: u64| {
            let until = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < until {
                cinder_ffi::cinder_clock_tick();
                cinder_ffi::cinder_render_tick();
                std::thread::sleep(Duration::from_millis(16));
            }
        };
        settle(2);
        for (x, y) in taps {
            carry("tap", cinder_ffi::cinder_tap(x, y));
            settle(1);
        }
        let c = CString::new(shot.clone()).unwrap();
        cinder_ffi::cinder_request_screenshot(c.as_ptr());
        settle(1);
        eprintln!("player: screen saved to {shot}");
        return;
    }

    let mut window = Window::new("Cinder · virtual player", W, H, WindowOptions::default())
        .expect("window (WSLg or an X server)");
    window.set_target_fps(60);
    let keys: &[(Key, i32)] = &[
        (Key::Backspace, 5),
        (Key::Space, 7),
        (Key::H, 8),
        (Key::Equal, 9),
        (Key::Minus, 10),
        (Key::P, 11),
        (Key::L, 12),
        (Key::N, 13),
        (Key::B, 14),
        (Key::Up, 0),
        (Key::Down, 1),
        (Key::Left, 2),
        (Key::Right, 3),
        (Key::Enter, 4),
    ];
    // A contact: where it started, where it was last, and whether it has become a drag.
    type Point = (f32, f32);
    let mut touch: Option<(Point, Point, bool)> = None;
    let mut last_second = Instant::now();
    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        for &(k, code) in keys {
            if window.is_key_pressed(k, KeyRepeat::Yes) {
                carry("key", cinder_ffi::cinder_input(code));
            }
        }
        let pos = window.get_mouse_pos(MouseMode::Clamp);
        let down = window.get_mouse_down(MouseButton::Left);
        match (&mut touch, down, pos) {
            (None, true, Some(p)) => {
                cinder_ffi::cinder_touch_down();
                touch = Some((p, p, false));
            }
            (Some((start, last, drag)), true, Some(p)) => {
                if !*drag && (p.1 - start.1).abs() > 12.0 {
                    *drag = true;
                }
                if *drag {
                    cinder_ffi::cinder_touch_drag((last.1 - p.1) as i32);
                }
                *last = p;
            }
            (Some((start, last, drag)), false, _) => {
                let (dx, dy) = (last.0 - start.0, last.1 - start.1);
                if start.0 < 40.0 && dx > 80.0 && dy.abs() < 60.0 {
                    carry("edge swipe", cinder_ffi::cinder_input(5));
                } else if !*drag && dx.abs() < 12.0 && dy.abs() < 12.0 {
                    carry(
                        "tap",
                        cinder_ffi::cinder_tap(start.0 as i32, start.1 as i32),
                    );
                }
                touch = None;
            }
            _ => {}
        }
        if last_second.elapsed() >= Duration::from_secs(1) {
            last_second = Instant::now();
            cinder_ffi::cinder_clock_tick();
            carry("night schedule", cinder_ffi::cinder_night_tick());
        }
        cinder_ffi::cinder_render_tick();
        let f = frame.lock().unwrap().clone();
        window.update_with_buffer(&f, W, H).unwrap();
    }
}
