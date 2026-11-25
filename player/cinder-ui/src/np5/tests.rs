use super::*;

const ALL: [Hit; 7] = [
    Hit::Like,
    Hit::Info,
    Hit::Shuffle,
    Hit::Prev,
    Hit::PlayPause,
    Hit::Next,
    Hit::Repeat,
];

#[test]
fn every_control_has_a_target_at_least_56_px() {
    for size in [Size::Big, Size::Huge] {
        for n in 2..=5 {
            let l = layout(size, true, n);
            for h in ALL
                .iter()
                .copied()
                .chain((0..n as u8).map(Hit::Toolbar))
                .chain([Hit::Lyrics])
            {
                let (_, _, w, ht) = l
                    .at(h)
                    .unwrap_or_else(|| panic!("{size:?}: no {h:?}"))
                    .bounds();
                let min = if h == Hit::Lyrics { 44 } else { 56 };
                assert!(w >= min && ht >= min, "{size:?} {h:?} is {w}x{ht}");
            }
        }
    }
}

#[test]
fn a_tap_on_each_drawn_control_answers_that_control() {
    for size in [Size::Big, Size::Huge] {
        let g = geo(size);
        let l = layout(size, false, 4);
        let ty = g.row_top + g.row_h / 2;
        let xs = button_cx(g);
        for (x, h) in xs.iter().zip([
            Hit::Shuffle,
            Hit::Prev,
            Hit::PlayPause,
            Hit::Next,
            Hit::Repeat,
        ]) {
            assert_eq!(l.hit(*x, ty), Some(h), "{size:?}");
        }
        let (hx, hy, hw, hh) = heart_rect(g);
        assert_eq!(l.hit(hx + hw / 2, hy + hh / 2), Some(Hit::Like));
        assert_eq!(l.hit(SIDE + 10, g.title_top + 10), Some(Hit::Info));
        for i in 0..4 {
            assert_eq!(l.hit(i * 120 + 60, 790), Some(Hit::Toolbar(i as u8)));
        }
    }
}

#[test]
fn the_rail_band_belongs_to_the_rail() {
    for size in [Size::Big, Size::Huge] {
        let l = layout(size, true, 5);
        for y in l.rail.grab_top..=l.rail.grab_bot {
            for x in (0..480).step_by(8) {
                assert_eq!(
                    l.hit(x, y),
                    None,
                    "{size:?} ({x},{y}) is a target inside the rail band"
                );
            }
        }
    }
}

#[test]
fn nothing_runs_off_the_screen_or_into_anything() {
    let f = FontSet::load();
    let t = Theme::day();
    let np = NowPlaying {
        title: "Atlas Hands (a much longer remix title that has to scroll)",
        artist: "Benjamin Francis Leftwich",
        ..sample()
    };
    for size in [Size::Big, Size::Huge] {
        let mut c = Canvas::new();
        c.track_text();
        render(
            &mut c,
            &t,
            &f,
            &np,
            size,
            &[
                Tool::Library,
                Tool::UpNext,
                Tool::Bluetooth,
                Tool::Places,
                Tool::Settings,
            ],
        );
        assert_eq!(c.oob_x(), 0, "{size:?}");
        assert!(
            c.text_collisions().is_empty(),
            "{size:?}: {:?}",
            c.text_collisions()
        );
    }
}

#[test]
fn tool_tokens_round_trip() {
    for t in Tool::ALL {
        assert_eq!(Tool::from_token(t.token()), Some(t));
    }
    assert_eq!(Tool::from_token("nope"), None);
}
