//! Icon set ported from finalists-shared.jsx (24x24 viewBox SVG strokes).
//! Drawn as scaled polylines / filled polygons on the Canvas. Each fn takes a
//! centre (cx, cy), display size `s` (px box), and colour.

use crate::canvas::Canvas;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle, Triangle};

/// Map a 24-unit viewBox coord into a canvas point for an icon centred at (cx,cy), box `s`.
fn p(cx: f32, cy: f32, s: f32, x: f32, y: f32) -> Point {
    Point::new(
        (cx + (x - 12.0) / 24.0 * s).round() as i32,
        (cy + (y - 12.0) / 24.0 * s).round() as i32,
    )
}

fn stroke_w(s: f32) -> u32 {
    ((s * 1.7 / 24.0).round() as u32).max(1)
}

fn polyline(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888, w: u32, pts: &[(f32, f32)]) {
    for win in pts.windows(2) {
        Line::new(p(cx, cy, s, win[0].0, win[0].1), p(cx, cy, s, win[1].0, win[1].1))
            .into_styled(PrimitiveStyle::with_stroke(col, w))
            .draw(c)
            .ok();
    }
}

fn polygon(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888, pts: &[(f32, f32)]) {
    for i in 1..pts.len().saturating_sub(1) {
        Triangle::new(
            p(cx, cy, s, pts[0].0, pts[0].1),
            p(cx, cy, s, pts[i].0, pts[i].1),
            p(cx, cy, s, pts[i + 1].0, pts[i + 1].1),
        )
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
    }
}

fn vbox(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888, w: u32, x: f32, y: f32, bw: f32, bh: f32, fill: bool) {
    let tl = p(cx, cy, s, x, y);
    let br = p(cx, cy, s, x + bw, y + bh);
    let style = if fill {
        PrimitiveStyle::with_fill(col)
    } else {
        PrimitiveStyle::with_stroke(col, w)
    };
    Rectangle::with_corners(tl, br).into_styled(style).draw(c).ok();
}

fn disc(c: &mut Canvas, cx: i32, cy: i32, d: i32, col: Rgb888) {
    embedded_graphics::primitives::Circle::new(Point::new(cx - d / 2, cy - d / 2), d as u32)
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

pub fn play(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    polygon(c, cx, cy, s, col, &[(8.0, 5.2), (18.5, 12.0), (8.0, 18.8)]);
}

pub fn pause(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    vbox(c, cx, cy, s, col, 0, 6.5, 5.0, 3.4, 14.0, true);
    vbox(c, cx, cy, s, col, 0, 14.1, 5.0, 3.4, 14.0, true);
}

pub fn prev(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(7.0, 5.0), (7.0, 19.0)]);
    polygon(c, cx, cy, s, col, &[(18.0, 6.2), (9.8, 12.0), (18.0, 17.8)]);
}

pub fn next(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(17.0, 5.0), (17.0, 19.0)]);
    polygon(c, cx, cy, s, col, &[(6.0, 6.2), (14.2, 12.0), (6.0, 17.8)]);
}

pub fn shuffle(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(3.0, 6.5), (6.6, 6.5), (17.0, 17.5), (21.0, 17.5)]);
    polyline(c, cx, cy, s, col, w, &[(3.0, 17.5), (6.6, 17.5), (9.5, 14.2)]);
    polyline(c, cx, cy, s, col, w, &[(13.8, 9.6), (17.0, 6.5), (21.0, 6.5)]);
    polyline(c, cx, cy, s, col, w, &[(18.6, 4.0), (21.2, 6.5), (18.6, 9.0)]);
    polyline(c, cx, cy, s, col, w, &[(18.6, 15.0), (21.2, 17.5), (18.6, 20.0)]);
}

pub fn repeat(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(4.0, 13.0), (4.0, 9.8), (7.3, 6.5), (20.0, 6.5)]);
    polyline(c, cx, cy, s, col, w, &[(17.4, 3.9), (20.0, 6.5), (17.4, 9.1)]);
    polyline(c, cx, cy, s, col, w, &[(20.0, 11.0), (20.0, 14.2), (16.7, 17.5), (4.0, 17.5)]);
    polyline(c, cx, cy, s, col, w, &[(6.6, 14.9), (4.0, 17.5), (6.6, 20.1)]);
}

pub fn heart(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    // two humps + lower tip, scaled into the 24-box
    let r = (s * 5.2 / 24.0) as i32;
    let hx = (s * 4.6 / 24.0) as i32;
    let hy = (cy - s * 2.0 / 24.0) as i32;
    disc(c, cx as i32 - hx, hy, r * 2, col);
    disc(c, cx as i32 + hx, hy, r * 2, col);
    polygon(c, cx, cy, s, col, &[(3.6, 10.5), (20.4, 10.5), (12.0, 20.3)]);
}

pub fn queue(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(4.0, 6.0), (20.0, 6.0)]);
    polyline(c, cx, cy, s, col, w, &[(4.0, 12.0), (14.0, 12.0)]);
    polyline(c, cx, cy, s, col, w, &[(4.0, 18.0), (11.0, 18.0)]);
    polygon(c, cx, cy, s, col, &[(16.5, 14.8), (21.0, 17.5), (16.5, 20.2)]);
}

pub fn eq(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(6.0, 4.0), (6.0, 20.0)]);
    polyline(c, cx, cy, s, col, w, &[(12.0, 4.0), (12.0, 20.0)]);
    polyline(c, cx, cy, s, col, w, &[(18.0, 4.0), (18.0, 20.0)]);
    polyline(c, cx, cy, s, col, w, &[(3.6, 14.2), (8.4, 14.2)]);
    polyline(c, cx, cy, s, col, w, &[(9.6, 8.2), (14.4, 8.2)]);
    polyline(c, cx, cy, s, col, w, &[(15.6, 16.2), (20.4, 16.2)]);
}

pub fn bt(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(
        c, cx, cy, s, col, w,
        &[(6.0, 7.2), (17.0, 16.8), (11.5, 21.5), (11.5, 2.5), (17.0, 7.2), (6.0, 16.8)],
    );
}

pub fn library(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    vbox(c, cx, cy, s, col, w, 4.0, 4.0, 6.6, 6.6, false);
    vbox(c, cx, cy, s, col, w, 13.4, 4.0, 6.6, 6.6, false);
    vbox(c, cx, cy, s, col, w, 4.0, 13.4, 6.6, 6.6, false);
    vbox(c, cx, cy, s, col, w, 13.4, 13.4, 6.6, 6.6, false);
}

pub fn bookmark(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(
        c, cx, cy, s, col, w,
        &[(6.5, 3.5), (17.5, 3.5), (17.5, 21.0), (12.0, 16.8), (6.5, 21.0), (6.5, 3.5)],
    );
}

pub fn menu(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s).max(2);
    polyline(c, cx, cy, s, col, w, &[(4.0, 7.5), (20.0, 7.5)]);
    polyline(c, cx, cy, s, col, w, &[(4.0, 12.0), (20.0, 12.0)]);
    polyline(c, cx, cy, s, col, w, &[(4.0, 16.5), (20.0, 16.5)]);
}

pub fn lock(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    vbox(c, cx, cy, s, col, w, 5.0, 10.5, 14.0, 9.5, false);
    polyline(c, cx, cy, s, col, w, &[(8.0, 10.5), (8.0, 7.5), (10.0, 5.6), (14.0, 5.6), (16.0, 7.5), (16.0, 10.5)]);
}

pub fn chevron(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s).max(2);
    polyline(c, cx, cy, s, col, w, &[(9.0, 5.0), (16.0, 12.0), (9.0, 19.0)]);
}

/// Chevron pointing UP — "this opens upward", used on the Now Playing return bar.
pub fn chevron_up(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s).max(2);
    polyline(c, cx, cy, s, col, w, &[(5.0, 16.0), (12.0, 9.0), (19.0, 16.0)]);
}

pub fn back(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s).max(2);
    polyline(c, cx, cy, s, col, w, &[(15.0, 5.0), (8.0, 12.0), (15.0, 19.0)]);
}

fn circle_stroke(c: &mut Canvas, cx: f32, cy: f32, s: f32, x: f32, y: f32, r: f32, col: Rgb888, w: u32) {
    let center = p(cx, cy, s, x, y);
    let d = ((r * 2.0 / 24.0 * s).round() as u32).max(2);
    embedded_graphics::primitives::Circle::with_center(center, d)
        .into_styled(PrimitiveStyle::with_stroke(col, w))
        .draw(c)
        .ok();
}

fn vdisc(c: &mut Canvas, cx: f32, cy: f32, s: f32, x: f32, y: f32, r: f32, col: Rgb888) {
    let ctr = p(cx, cy, s, x, y);
    let d = ((r * 2.0 / 24.0 * s).round() as i32).max(2);
    disc(c, ctr.x, ctr.y, d, col);
}

pub fn note(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(9.0, 17.5), (9.0, 5.0), (20.0, 2.8), (20.0, 15.0)]);
    vdisc(c, cx, cy, s, 6.5, 17.5, 2.6, col);
    vdisc(c, cx, cy, s, 17.5, 15.0, 2.6, col);
}

pub fn radio(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    vbox(c, cx, cy, s, col, w, 3.0, 8.5, 18.0, 11.0, false);
    circle_stroke(c, cx, cy, s, 8.2, 14.0, 2.4, col, w);
    polyline(c, cx, cy, s, col, w, &[(14.0, 12.2), (18.0, 12.2)]);
    polyline(c, cx, cy, s, col, w, &[(14.0, 15.8), (18.0, 15.8)]);
    polyline(c, cx, cy, s, col, w, &[(7.0, 8.5), (17.5, 3.4)]);
}

pub fn sound(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polygon(c, cx, cy, s, col, &[(4.0, 9.5), (7.4, 9.5), (13.0, 4.8), (13.0, 19.2), (7.4, 14.5), (4.0, 14.5)]);
    polyline(c, cx, cy, s, col, w, &[(16.0, 9.2), (17.3, 12.0), (16.0, 14.8)]);
    polyline(c, cx, cy, s, col, w, &[(18.6, 6.6), (20.5, 12.0), (18.6, 17.4)]);
}

pub fn usb(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(12.0, 21.0), (12.0, 4.6)]);
    polyline(c, cx, cy, s, col, w, &[(9.8, 6.8), (12.0, 4.0), (14.2, 6.8)]);
    polyline(c, cx, cy, s, col, w, &[(12.0, 14.5), (7.5, 12.0), (7.5, 9.4)]);
    polyline(c, cx, cy, s, col, w, &[(12.0, 12.0), (16.5, 10.0), (16.5, 7.2)]);
    vdisc(c, cx, cy, s, 7.5, 8.0, 1.3, col);
    vbox(c, cx, cy, s, col, w, 15.4, 5.0, 2.4, 2.4, false);
    vdisc(c, cx, cy, s, 12.0, 19.0, 1.6, col);
}

pub fn rx(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    polyline(c, cx, cy, s, col, w, &[(9.0, 7.6), (18.0, 15.4), (13.5, 19.2), (13.5, 4.8), (18.0, 8.6), (9.0, 16.4)]);
    polyline(c, cx, cy, s, col, w, &[(3.0, 9.0), (3.0, 15.0)]);
    polyline(c, cx, cy, s, col, w, &[(5.6, 7.0), (5.6, 17.0)]);
}

pub fn settings(c: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgb888) {
    let w = stroke_w(s);
    circle_stroke(c, cx, cy, s, 12.0, 12.0, 3.1, col, w);
    polyline(c, cx, cy, s, col, w, &[(12.0, 2.5), (12.0, 5.5)]);
    polyline(c, cx, cy, s, col, w, &[(12.0, 18.5), (12.0, 21.5)]);
    polyline(c, cx, cy, s, col, w, &[(2.5, 12.0), (5.5, 12.0)]);
    polyline(c, cx, cy, s, col, w, &[(18.5, 12.0), (21.5, 12.0)]);
    polyline(c, cx, cy, s, col, w, &[(5.3, 5.3), (7.4, 7.4)]);
    polyline(c, cx, cy, s, col, w, &[(16.6, 16.6), (18.7, 18.7)]);
    polyline(c, cx, cy, s, col, w, &[(18.7, 5.3), (16.6, 7.4)]);
    polyline(c, cx, cy, s, col, w, &[(7.4, 16.6), (5.3, 18.7)]);
}
