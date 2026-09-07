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
        viz_peaks: None,
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
    let mut lib = Library { songs, album_groups, artists, playlists: Vec::new(), thumbs: Default::default(), genres: Vec::new(), ..Default::default() };
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
            library::render(&mut c, &t, &f, tab, 0, 0, 0, 0, None, &lib, None, false)
        });
    }
    for (tab, name) in [(Tab::Songs, "songs"), (Tab::Artists, "artists")] {
        time_it(&format!("az_render {name}"), n, || {
            library::az_render(&mut c, &t, &f, tab, &library::az_present(tab, &lib, 0, 0), 0, 0)
        });
    }

    // With the art cache populated — what the device looks like once the builder has run.
    let img = cinder_ui::art::Image { w: 48, h: 48, rgb: vec![90u8; 48 * 48 * 3] };
    for id in 1..=aid {
        lib.thumbs.insert(id, img.clone());
    }
    for (tab, name) in [(Tab::Songs, "songs"), (Tab::Albums, "albums"), (Tab::Artists, "artists")] {
        time_it(&format!("library {name} (real covers)"), n, || {
            library::render(&mut c, &t, &f, tab, 0, 0, 0, 0, None, &lib, None, false)
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
        library::artist_view(&mut c, &t, &f, &lib, &page, 0, 0, None, false)
    });

    // The album drill-in draws a 96x96 cover. With no decoded artwork that is a generated
    // gradient — 9,216 pixels of table lookup, squared distance and a sqrt — and until
    // 2026-08-20 it was recomputed on every single frame the page was up.
    let mut bare = Library::sample();
    bare.thumbs.clear();
    if let Some(album) = bare.albums_flat().first().map(|a| (*a).clone()) {
        time_it("album_view (gradient cover)", n, || {
            library::album_view(&mut c, &t, &f, &album, 0, 0, None, None, false)
        });
    }
}

/// How much of a Library/Up Next frame is REBUILT ORDERING rather than pixels?
///
/// `library::render` calls `song_order` (a full sort of every track) or `albums_build` (two
/// `albums_flat()` vectors, a sort, and a row vector) on every painted frame, and `nav::render`
/// clones the whole playback context on every Up Next frame. None of those three depend on
/// anything that changes between frames — they depend on the library, the sort chip and which
/// accordion is open — so whatever they cost is pure waste at 60 Hz. This splits the derived work
/// out of the frame so the memo is sized against a number rather than a hunch.
#[test]
#[ignore]
fn bench_derived_state() {
    use cinder_ui::library::{self, Tab};
    use cinder_ui::model::{AlbumRow, ArtistGroup, Library, SongRow};

    // The device library, at its measured size: 3746 tracks over 304 albums.
    let mut songs = Vec::new();
    let mut album_groups: Vec<ArtistGroup> = Vec::new();
    let mut aid = 0i64;
    for a in 0..152 {
        let name = format!("Artist {a:03}");
        let mut albums = Vec::new();
        for _ in 0..2 {
            aid += 1;
            let an = format!("Album {aid:03}");
            let track_list: Vec<SongRow> = (0..12)
                .map(|i| SongRow {
                    title: format!("{an} track {i:02}"), artist: name.clone(), dur: "3:20".into(),
                    art: an.clone(), object_id: aid * 100 + i, album_id: aid,
                    disc: 1, track: i as i32, added: aid, year: 2019, genre_id: (i % 3) + 1,
                    is_hires: i % 7 == 0,
                })
                .collect();
            songs.extend(track_list.iter().cloned());
            albums.push(AlbumRow {
                name: an.clone(), artist: name.clone(), year: "2019".into(), tracks: 12,
                art: an, album_id: aid, added: aid, track_list,
            });
        }
        album_groups.push(ArtistGroup { artist: name, albums });
    }
    let lib = Library { songs, album_groups, artists: Vec::new(), playlists: Vec::new(),
                        thumbs: Default::default(), genres: Vec::new(), ..Default::default() };
    println!("library: {} songs, {} albums", lib.songs.len(), lib.album_count());

    let n = 200;
    // The three sorts a user can actually be sitting on.
    for (s, name) in [(0usize, "TITLE"), (1, "ARTIST A-Z"), (5, "ALBUM")] {
        time_it(&format!("song_order sort={name}"), n, || {
            let _ = library::song_order(&lib, s).len();
        });
    }
    for (s, name) in [(0usize, "ARTIST"), (1, "A-Z"), (3, "YEAR")] {
        time_it(&format!("albums_build order={name}"), n, || {
            let _ = library::albums_build(&lib, s, None).rows.len();
        });
    }
    time_it("albums_flat", n, || { let _ = lib.albums_flat().len(); });
    time_it("az_present songs", n, || { let _ = library::az_present(Tab::Songs, &lib, 0, 0); });
    time_it("max_scroll_px albums", n, || {
        let _ = library::max_scroll_px(Tab::Albums, &lib, 0, None);
    });

    // Up Next: nav::render clones the whole context, then builds the slot layout twice (once for
    // the auto-follow, once inside render_view).
    let ctx: Vec<SongRow> = lib.songs.clone();
    time_it("context clone (shuffle-all)", n, || { let _ = ctx.clone().len(); });
    time_it("up_next::layout (shuffle-all)", n, || {
        let _ = cinder_ui::up_next::layout(ctx.len(), Some(ctx.len() / 2), 3, false).slots.len();
    });

    let t = Theme::day();
    let f = FontSet::load();
    let mut c = Canvas::new();
    let queue: Vec<SongRow> = ctx[..3].to_vec();
    let view = cinder_ui::up_next::QueueView {
        album: "Album 001", tracks: &ctx, current: Some(ctx.len() / 2), queue: &queue,
        pick: None, lib: &lib, scroll_px: 0, drag: None, swipe: None, sbar_active: false,
    };
    time_it("up_next::render_view", n, || {
        let _ = cinder_ui::up_next::render_view(&mut c, &t, &f, &view);
    });

    // The REALISTIC case. scroll_px = 0 above is the worst case for the first-visible-slot search
    // (the window is already at the top, so there is nothing to skip); in normal use the auto-follow
    // parks NOW PLAYING a third of the way down, which after a shuffle-all is ~1800 slots in.
    let follow = cinder_ui::up_next::metrics(ctx.len(), Some(ctx.len() / 2), 3, false).follow_scroll();
    let view_followed = cinder_ui::up_next::QueueView { scroll_px: follow, ..view };
    time_it("up_next::render_view (followed)", n, || {
        let _ = cinder_ui::up_next::render_view(&mut c, &t, &f, &view_followed);
    });

    // The same screen with an ALBUM-sized context, to separate "drawing 14 rows" from "the cost
    // scales with how long the sequence is".
    let small: Vec<SongRow> = ctx[..12].to_vec();
    let view_small = cinder_ui::up_next::QueueView {
        album: "Album 001", tracks: &small, current: Some(6), queue: &queue,
        pick: None, lib: &lib, scroll_px: 0, drag: None, swipe: None, sbar_active: false,
    };
    time_it("up_next::render_view (album)", n, || {
        let _ = cinder_ui::up_next::render_view(&mut c, &t, &f, &view_small);
    });
    time_it("up_next::layout (album)", n, || {
        let _ = cinder_ui::up_next::layout(12, Some(6), 3, false).slots.len();
    });
    time_it("context clone (album)", n, || { let _ = small.clone().len(); });
}
