use super::*;
use crate::kit::Trail;
use crate::style::{Look, Style};
use crate::FontSet;

#[test]
fn the_cinder_theme_file_is_cinder() {
    assert_eq!(
        DataLook::parse("cinder", CINDER_CONF),
        Ok(DataLook::default())
    );
}

#[test]
fn the_cinder_theme_draws_exactly_what_the_code_does() {
    // The text size is process-wide; another test changing it between the two draws moved a pixel.
    let _scale = crate::text::scale_guard();
    let f = FontSet::load();
    for night in [false, true] {
        let mut t = if night { Theme::night() } else { Theme::day() };
        let want = kit_preview(&t, &f).to_rgb_bytes();
        t.style = Style::Data;
        assert!(
            kit_preview(&t, &f).to_rgb_bytes() == want,
            "night={night}: the Cinder theme moved a pixel"
        );
    }
}

#[test]
fn every_problem_is_reported_with_its_line() {
    let e = DataLook::parse(
        "ledger",
        "format = 2\nrow = cards\nshape.radius = 40\ntype.row = 19 / 500\nrow = rule\nwhat = 1\nlayout.places = list\n",
    )
    .unwrap_err();
    let all = e.join("\n");
    for want in [
        "line 2: `row` wants rule | card | plain | mono, got `cards`",
        "line 3: `shape.radius` wants a whole number from 0 to 24",
        "line 4: `type.row`",
        "line 5: `row` is set twice",
        "line 6: unknown key `what`",
        "line 7: `layout.places` is not supported",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
}

#[test]
fn a_theme_without_format_2_is_refused() {
    assert!(DataLook::parse("a", "name = A\n").unwrap_err()[0].contains("format = 2"));
    assert!(DataLook::parse("a", "format = 3\n").unwrap_err()[0].contains("format 2"));
}

#[test]
fn inline_colours_go_through_the_palette_checks() {
    let ok = DataLook::parse("slate", "format = 2\nday.bg = #0e1116\n").unwrap();
    assert_eq!(ok.colours.unwrap().day.bg, 0x0e1116);
    let unreadable = DataLook::parse("x", "format = 2\nday.ink = #0d0c0b\n").unwrap_err();
    assert!(
        unreadable.iter().all(|e| e.starts_with("colours: ")),
        "{unreadable:?}"
    );
    let both = DataLook::parse("x", "format = 2\npalette = paper\nday.bg = #0e1116\n").unwrap_err();
    assert!(both.iter().any(|e| e.contains("not both")));
}

#[test]
fn the_board_example_parses_apart_from_what_comes_later() {
    let body = "name = Ledger\nformat = 2\npalette = paper\ntype.title = 34 / 800\ntype.row = 21 / 600\n\
                shape.radius = 6\nshape.cover = 12\ndensity = big\nrow = card # rule | card\nrow.sel = bar\n\
                chip = fill\nswitch = pill\nplay = ring\n";
    let d = DataLook::parse("ledger", body).unwrap();
    assert_eq!(
        (d.row, d.sel, d.chip, d.switch, d.play),
        (
            RowV::Card,
            SelV::Bar,
            ChipV::Fill,
            SwitchV::Pill,
            PlayV::Ring
        )
    );
    assert_eq!((d.radius, d.cover_radius, d.title.0), (6, 12, 34.0));
}

/// What changed between two canvases outside rows `y0..y1` (and columns `x0..x1`).
fn outside(a: &[u8], b: &[u8], (x0, y0, x1, y1): (i32, i32, i32, i32)) -> usize {
    (0..a.len() / 3)
        .filter(|&i| {
            let (x, y) = ((i % W) as i32, (i / W) as i32);
            !(x >= x0 && x < x1 && y >= y0 && y < y1) && a[i * 3..i * 3 + 3] != b[i * 3..i * 3 + 3]
        })
        .count()
}

/// The fixed boxes are the tap test's, so no variant may draw outside them (but for the 1 px a
/// 2 px ring sits outside its edge, as Cinder's own chip ring does).
#[test]
fn every_variant_draws_inside_its_box() {
    let _scale = crate::text::scale_guard();
    let f = FontSet::load();
    let t = Theme::day();
    let mut blank = Canvas::new();
    blank.fill(t.bg);
    let blank = blank.to_rgb_bytes();
    for radius in [0, 6, 24] {
        for &row in RowV::ALL {
            for &sel in SelV::ALL {
                let d = DataLook {
                    row,
                    sel,
                    radius,
                    ..Default::default()
                };
                let mut c = Canvas::new();
                c.fill(t.bg);
                d.row(
                    &mut c,
                    &t,
                    &f,
                    200,
                    64,
                    &Row::new("Atlas Hands").sel(true).trail(Trail::Switch(true)),
                );
                assert_eq!(
                    outside(&c.to_rgb_bytes(), &blank, (0, 199, W as i32, 265)),
                    0,
                    "{row:?} {sel:?} r{radius}"
                );
            }
        }
        for &chip in ChipV::ALL {
            let d = DataLook {
                chip,
                radius,
                ..Default::default()
            };
            let mut c = Canvas::new();
            c.fill(t.bg);
            d.chips(&mut c, &t, &f, 300, &["15", "30", "45"], Some(1), 3);
            assert_eq!(
                outside(&c.to_rgb_bytes(), &blank, (0, 299, W as i32, 301 + CHIP_H)),
                0,
                "{chip:?} r{radius}"
            );
        }
        for &switch in SwitchV::ALL {
            for on in [false, true] {
                let d = DataLook {
                    switch,
                    radius,
                    ..Default::default()
                };
                let mut c = Canvas::new();
                c.fill(t.bg);
                d.switch(&mut c, &t, 100, 100, on);
                assert_eq!(
                    outside(
                        &c.to_rgb_bytes(),
                        &blank,
                        (100, 100, 100 + SWITCH_W, 100 + SWITCH_H)
                    ),
                    0,
                    "{switch:?} {on}"
                );
            }
        }
    }
}

#[test]
fn round_shapes_at_radius_0_are_the_square_ones() {
    let col = Rgb888::new(200, 100, 50);
    let (mut a, mut b) = (Canvas::new(), Canvas::new());
    fill_round(&mut a, 10, 10, 50, 30, 0, col);
    stroke_round(&mut a, 100, 10, 50, 30, 0, col, 2);
    fill_rect(&mut b, 10, 10, 50, 30, col);
    stroke_rect(&mut b, 100, 10, 50, 30, col, 2);
    assert!(a.to_rgb_bytes() == b.to_rgb_bytes());
}

#[test]
fn every_shipped_theme_loads() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/themes");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let id = e.file_name().into_string().unwrap();
        let body = std::fs::read_to_string(e.path().join(FILE_NAME)).unwrap();
        if let Err(p) = DataLook::parse(&id, &body) {
            panic!("{id}: {p:?}");
        }
        n += 1;
    }
    assert!(n >= 4);
}
