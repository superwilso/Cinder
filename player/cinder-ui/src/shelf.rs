//! Shelf sheet (`CShelfSheet`) — a bottom-sheet overlay drawn over the current
//! screen: pin the current place to one of three slots, restore instantly, and an
//! Undo of the last navigation. Ported from cinder-proto-screens1.jsx.
//!
//! `render` dims whatever is already in the canvas (the screen behind) and draws
//! the opaque sheet over the lower portion. `hit` maps a click to a `ShelfHit` so
//! the geometry lives in one place.
use crate::text::{Family, Weight};
use crate::widgets::{center, fill_rect, right, stroke_rect, sty};
use crate::{icons, text, Canvas, FontSet, Theme};

pub struct Pin<'a> {
    pub title: &'a str,
    pub sub: &'a str,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ShelfHit {
    None,
    Close,
    Undo,
    Pin,
    Go(usize),
    Clear(usize),
}

// Sheet geometry (shared by render + hit).
const TOP: i32 = 406; // sheet top y
const UNDO_Y: i32 = 480;
const PIN_BTN: (i32, i32, i32, i32) = (382, 558, 76, 48); // x,y,w,h
const SLOT0_Y: i32 = 640;
const SLOT_DY: i32 = 46;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, this_title: &str, this_sub: &str, pins: &[Option<Pin>; 3]) {
    // 1. dim the screen behind (≈55% black backdrop)
    for px in c.buf.iter_mut() {
        let r = ((*px >> 16) & 0xff) * 45 / 100;
        let g = ((*px >> 8) & 0xff) * 45 / 100;
        let b = (*px & 0xff) * 45 / 100;
        *px = (r << 16) | (g << 8) | b;
    }
    // 2. sheet panel + accent top border
    fill_rect(c, 0, TOP, 480, 800 - TOP, t.bg);
    fill_rect(c, 0, TOP, 480, 1, t.acc);

    // header
    icons::bookmark(c, 24.0, (TOP + 18) as f32, 16.0, t.ink);
    text::draw(c, f, 48.0, (TOP + 30) as f32, "Shelf", &sty(Family::Sans, Weight::Bold, 22.0, t.ink, 0.0));
    right(c, f, 458.0, (TOP + 28) as f32, "CLOSE ×", &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.08));

    let cap = sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.18);

    // HISTORY
    text::draw(c, f, 22.0, 466.0, "HISTORY", &cap);
    stroke_rect(c, 22, UNDO_Y, 208, 46, t.line, 1);
    text::draw(c, f, 36.0, (UNDO_Y + 19) as f32, "\u{2039} Undo", &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
    text::draw(c, f, 36.0, (UNDO_Y + 36) as f32, "Previous screen", &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    stroke_rect(c, 242, UNDO_Y, 216, 46, t.line, 1);
    text::draw(c, f, 256.0, (UNDO_Y + 19) as f32, "Redo \u{203a}", &sty(Family::Sans, Weight::SemiBold, 15.0, t.faint, 0.0));
    text::draw(c, f, 256.0, (UNDO_Y + 36) as f32, "\u{2014}", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.0));

    // THIS PLACE
    text::draw(c, f, 22.0, 544.0, "THIS PLACE", &cap);
    stroke_rect(c, 22, 558, 436, 48, t.line, 1);
    text::draw(c, f, 36.0, 580.0, this_title, &sty(Family::Sans, Weight::SemiBold, 16.0, t.ink, 0.0));
    text::draw(c, f, 36.0, 596.0, this_sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
    let (px, py, pw, ph) = PIN_BTN;
    fill_rect(c, px, py + 6, pw, ph - 12, t.acc);
    center(c, f, (px + pw / 2) as f32, (py + ph / 2 + 4) as f32, "Pin", &sty(Family::Sans, Weight::Bold, 14.0, t.acc_ink, 0.0));

    // PINNED · N/3
    let filled = pins.iter().filter(|p| p.is_some()).count();
    text::draw(c, f, 22.0, 626.0, &format!("PINNED \u{00b7} {}/3", filled), &cap);
    for (i, slot) in pins.iter().enumerate() {
        let y = SLOT0_Y + i as i32 * SLOT_DY;
        match slot {
            Some(p) => {
                stroke_rect(c, 22, y, 436, 40, t.line, 1);
                text::draw(c, f, 36.0, (y + 24) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.acc, 0.0));
                text::draw(c, f, 58.0, (y + 17) as f32, p.title, &sty(Family::Sans, Weight::SemiBold, 15.0, t.ink, 0.0));
                text::draw(c, f, 58.0, (y + 32) as f32, p.sub, &sty(Family::Mono, Weight::Regular, 11.0, t.dim, 0.0));
                right(c, f, 406.0, (y + 24) as f32, "GO \u{203a}", &sty(Family::Mono, Weight::Regular, 12.0, t.acc, 0.0));
                right(c, f, 450.0, (y + 24) as f32, "\u{00d7}", &sty(Family::Mono, Weight::Regular, 14.0, t.faint, 0.0));
            }
            None => {
                // dashed border (drawn as dashes) + hint
                let mut dx = 22;
                while dx < 458 {
                    fill_rect(c, dx, y, 7, 1, t.line);
                    fill_rect(c, dx, y + 39, 7, 1, t.line);
                    dx += 14;
                }
                text::draw(c, f, 36.0, (y + 24) as f32, &format!("{}", i + 1), &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
                text::draw(c, f, 58.0, (y + 24) as f32, "Empty slot \u{2014} pin here", &sty(Family::Sans, Weight::Regular, 14.0, t.faint, 0.0));
            }
        }
    }
}

/// Map a click to a shelf action (geometry mirrors `render`).
pub fn hit(x: i32, y: i32) -> ShelfHit {
    if y < TOP {
        return ShelfHit::Close; // tap the dimmed backdrop
    }
    // CLOSE × in the header (top-right)
    if (TOP + 14..TOP + 40).contains(&y) && x > 396 {
        return ShelfHit::Close;
    }
    if (UNDO_Y..UNDO_Y + 46).contains(&y) && (22..230).contains(&x) {
        return ShelfHit::Undo;
    }
    let (px, py, pw, ph) = PIN_BTN;
    if (py..py + ph).contains(&y) && (px..px + pw).contains(&x) {
        return ShelfHit::Pin;
    }
    for i in 0..3 {
        let sy = SLOT0_Y + i * SLOT_DY;
        if (sy..sy + 40).contains(&y) {
            if (410..458).contains(&x) {
                return ShelfHit::Clear(i as usize);
            }
            if (348..410).contains(&x) {
                return ShelfHit::Go(i as usize);
            }
            return ShelfHit::Pin; // body / empty slot → pin the current place
        }
    }
    ShelfHit::None
}
