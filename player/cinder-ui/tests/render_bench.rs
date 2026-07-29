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

/// Library-tab frame cost at REAL library size (the device DB: 3349 songs, 305 albums,
/// 170 artists). The Artists tab reported as "slow to load" on device, and a guess about why is
/// exactly what this harness exists to replace.
#[test]
#[ignore]
fn bench_library_tabs() {
    use cinder_ui::library::{self, Tab};
    use cinder_ui::model::{AlbumRow, ArtistGroup, ArtistRow, Library, SongRow};

    let mut songs = Vec::new();
    let mut album_groups: Vec<ArtistGroup> = Vec::new();
    let mut artists = Vec::new();
    let mut aid = 0i64;
    for a in 0..170 {
        let name = format!("Artist {a:03}");
        let mut albums = Vec::new();
        for k in 0..2 {
            aid += 1;
            let an = format!("Album {aid:03}");
            let track_list: Vec<SongRow> = (0..10)
                .map(|i| SongRow {
                    title: format!("{an} track {i}"), artist: name.clone(), dur: "3:20".into(),
                    art: an.clone(), object_id: aid * 100 + i, album_id: aid,
                    ..Default::default()
                })
                .collect();
            songs.extend(track_list.iter().cloned());
            albums.push(AlbumRow {
                name: an.clone(), artist: name.clone(), year: "2019".into(), tracks: 10,
                art: an, album_id: aid, added: aid, track_list,
            });
            let _ = k;
        }
        artists.push(ArtistRow {
            name: name.clone(), albums: 2, tracks: 20,
            arts: albums.iter().map(|x| x.name.clone()).collect(),
            album_ids: albums.iter().map(|x| x.album_id).collect(),
        });
        album_groups.push(ArtistGroup { artist: name, albums });
    }
    let mut lib = Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default() };
    println!("library: {} songs, {} albums, {} artists",
        lib.songs.len(), lib.album_count(), lib.artists.len());

    let t = Theme::day();
    let f = FontSet::load();
    let mut c = Canvas::new();
    let n = 100;

    // Gradients only — the state a fresh device is in before the art cache fills.
    for (tab, name) in [(Tab::Songs, "songs"), (Tab::Albums, "albums"),
                        (Tab::Artists, "artists"), (Tab::Playlists, "playlists")] {
        time_it(&format!("library {name} (gradients)"), n, || {
            library::render(&mut c, &t, &f, tab, 0, 0, 0, 0, None, &lib, None)
        });
    }
    for (tab, name) in [(Tab::Songs, "songs"), (Tab::Artists, "artists")] {
        time_it(&format!("az_render {name}"), n, || {
            library::az_render(&mut c, &t, &f, tab, &lib, 0, 0)
        });
    }

    // With the art cache populated — what the device looks like once the builder has run.
    let img = cinder_ui::art::Image { w: 48, h: 48, rgb: vec![90u8; 48 * 48 * 3] };
    for id in 1..=aid {
        lib.thumbs.insert(id, img.clone());
    }
    for (tab, name) in [(Tab::Songs, "songs"), (Tab::Albums, "albums"), (Tab::Artists, "artists")] {
        time_it(&format!("library {name} (real covers)"), n, || {
            library::render(&mut c, &t, &f, tab, 0, 0, 0, 0, None, &lib, None)
        });
    }

    // Resolving an artist page — this happens on EVERY frame the page is up, plus once per
    // scroll tick, so its cost is a per-frame cost, not a one-off.
    let who = lib.artists[0].name.clone();
    time_it("artist_page resolve", n, || {
        let _ = library::artist_page(&lib, &who).tracks.len();
    });
    let page = library::artist_page(&lib, &who);
    time_it("artist_view render", n, || {
        library::artist_view(&mut c, &t, &f, &lib, &page, 0, 0, None)
    });
}
