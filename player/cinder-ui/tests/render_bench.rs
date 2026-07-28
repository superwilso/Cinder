//! Render timing harness. NOT a gate — `#[ignore]`d, so `cargo test` never runs it and a slow CI
//! machine can never fail a build. Run it deliberately:
//!
//!     cargo test -p cinder-ui --release --test render_bench -- --ignored --nocapture
//!
//! It exists because the 2026-07-28 optimisation pass started from a guess (the visualiser must be
//! the expensive part) that was wrong by two orders of magnitude: the visualiser costs ~30 us and
//! the album art behind it cost ~8000. Host numbers are not device numbers, but the RATIOS are
//! what tell you where to look, and they transfer.
use cinder_ui::canvas::Canvas;
use cinder_ui::now_playing::{self, NowPlaying};
use cinder_ui::text::FontSet;
use cinder_ui::Theme;

fn np<'a>(page: u8, viz_size: u8, viz_kind: u8, levels: &'a [f32]) -> NowPlaying<'a> {
    NowPlaying {
        title: "Atlas Hands",
        artist: "Benjamin Francis Leftwich",
        codec: "FLAC · 24bit / 96.0 kHz",
        badge: "FLAC 24/96",
        clock: "14:32",
        battery: 78,
        elapsed: "1:47",
        remaining: "-2:45",
        progress: 0.39,
        art: "atlas hands",
        art_full: None,
        art_thumb: None,
        liked: false,
        playing: true,
        shuffle: false,
        repeat: 0,
        viz_seed: 2.0,
        viz_kind,
        viz_size,
        viz_levels: Some(levels),
        page,
        scrubbing: false,
    }
}

fn time_it(label: &str, iters: u32, mut f: impl FnMut()) {
    f();
    let t0 = std::time::Instant::now();
    for _ in 0..iters {
        f();
    }
    let us = t0.elapsed().as_micros() as f64 / iters as f64;
    println!("{label:<34} {us:>8.1} us/frame");
}

#[test]
#[ignore]
fn bench_pages() {
    let t = Theme::day();
    let f = FontSet::load();
    let levels: Vec<f32> = (0..36).map(|i| 0.2 + 0.7 * ((i as f32 * 0.5).sin().abs())).collect();
    let n = 200;

    let mut c = Canvas::new();
    time_it("cover, viz OFF", n, || now_playing::render(&mut c, &t, &f, &np(0, 0, 0, &levels)));
    time_it("cover, VEIL bars", n, || now_playing::render(&mut c, &t, &f, &np(0, 1, 0, &levels)));
    time_it("cover, FULL bars", n, || now_playing::render(&mut c, &t, &f, &np(0, 2, 0, &levels)));
    for (k, name) in [(0u8, "bars"), (1, "ribbon"), (2, "line"), (7, "pulse")] {
        time_it(&format!("spectrum page, {name}"), n, || {
            now_playing::render(&mut c, &t, &f, &np(1, 0, k, &levels))
        });
    }
    time_it("level page", n, || now_playing::render(&mut c, &t, &f, &np(2, 0, 0, &levels)));

    // What does the visualiser alone cost, with no screen around it?
    time_it("just canvas fill", n, || c.fill(t.bg));
    for (k, name) in [(0u8, "bars"), (1, "ribbon"), (2, "line")] {
        time_it(&format!("viz alone 432x348 {name}"), n, || {
            cinder_ui::viz::draw(&mut c, 24, 154, 432, 348, 36, 3, 2.0,
                                 cinder_ui::viz::from_index(k), t.acc, t.line, Some(&levels), 255, 255);
        });
    }
    // The real device path: a decoded 480x480 cover blitted 1:1.
    let img = cinder_ui::art::Image { w: 480, h: 480, rgb: vec![90u8; 480 * 480 * 3] };
    time_it("art::draw_image 480x480", n, || {
        cinder_ui::art::draw_image(&mut c, &t, 0, 34, &img, 1.0)
    });
    time_it("art::block gradient 480x480", n, || {
        cinder_ui::art::block(&mut c, &t, 0, 34, 480, 480, "atlas hands", 1.0)
    });
    let npi = NowPlaying { art_full: Some(&img), ..np(0, 1, 0, &levels) };
    time_it("cover w/ real image, VEIL", n, || now_playing::render(&mut c, &t, &f, &npi));

    time_it("viz alone 432x64 VEIL bars", n, || {
        cinder_ui::viz::draw(&mut c, 24, 444, 432, 64, 36, 3, 2.0,
                             cinder_ui::viz::from_index(0), t.acc, t.line, Some(&levels), 0, 180);
    });
}
