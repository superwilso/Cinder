//! First-run onboarding + re-viewable Help/Controls. A short paged intro shown ONCE on first boot
//! (Welcome → Controls → Features → Done), persisted so it doesn't reappear; also openable any time
//! from the Menu ("Help & Controls"). Touch-navigated (the NW-A55 has no d-pad): tap the right side
//! = next / finish, tap the left side = back a page, left-edge swipe = skip. The Controls page
//! matters most — it teaches the touch + transport-button model the rest of Cinder uses.

use crate::canvas::W;
use crate::icons;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};
use crate::Canvas;

pub const PAGES: usize = 4;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, page: usize) {
    c.fill(t.bg);
    match page {
        0 => welcome(c, t, f),
        1 => controls(c, t, f),
        2 => features(c, t, f),
        _ => done(c, t, f),
    }
    page_dots(c, t, page);
    footer(c, t, f, page);
}

fn welcome(c: &mut Canvas, t: &Theme, f: &FontSet) {
    // accent wordmark + tagline
    text::draw(c, f, 36.0, 300.0, "CINDER", &sty(Family::Sans, Weight::Bold, 52.0, t.acc, 0.02));
    text::draw(c, f, 38.0, 340.0, "Your music, clean and quiet.", &sty(Family::Sans, Weight::Regular, 18.0, t.ink, 0.0));
    text::draw(c, f, 38.0, 366.0, "A replacement player for the Walkman.", &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0));
}

// One "key → action" row on the Controls page.
fn ctl(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, key: &str, action: &str) {
    text::draw(c, f, 38.0, y as f32, key, &sty(Family::Mono, Weight::Bold, 14.0, t.acc, 0.04));
    text::draw(c, f, 168.0, y as f32, action, &sty(Family::Sans, Weight::Regular, 16.0, t.ink, 0.0));
}

fn controls(c: &mut Canvas, t: &Theme, f: &FontSet) {
    text::draw(c, f, 36.0, 90.0, "Controls", &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0));

    // The physical buttons (the only ones the device has) — all transport + power.
    text::draw(c, f, 38.0, 124.0, "BUTTONS", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    let mut y = 152;
    let buttons: [(&str, &str); 5] = [
        ("PLAY", "Play / pause"),
        ("\u{25C1} REWIND", "Previous track"),
        ("SKIP \u{25B7}", "Next track"),
        ("VOL + / \u{2212}", "Volume"),
        ("POWER", "Wake / sleep   ·   HOLD switch locks"),
    ];
    for (k, a) in buttons {
        ctl(c, t, f, y, k, a);
        y += 46;
    }

    // Everything else is the touchscreen.
    y += 18;
    text::draw(c, f, 38.0, y as f32, "TOUCH", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    y += 28;
    let touch: [(&str, &str); 4] = [
        ("Tap", "Open / select"),
        ("Swipe \u{2195}", "Scroll lists"),
        ("Left edge \u{2192}", "Go back"),
        ("Tap top bar", "Open the menu"),
    ];
    for (k, a) in touch {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
}

// One feature bullet.
fn bullet(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, head: &str, sub: &str) {
    fill_rect(c, 38, y - 9, 4, 14, t.acc); // accent tick
    text::draw(c, f, 54.0, y as f32, head, &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0));
    text::draw(c, f, 54.0, (y + 19) as f32, sub, &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0));
}

fn features(c: &mut Canvas, t: &Theme, f: &FontSet) {
    text::draw(c, f, 36.0, 92.0, "What's inside", &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0));
    let mut y = 160;
    let items: [(&str, &str); 5] = [
        ("Library", "Songs, albums, artists — scrolls thousands of tracks."),
        ("Sound", "10-band EQ + DSEE/VPT/Vinyl/ClearAudio+, A/B compare."),
        ("Visualiser", "Five real-time types on Now Playing."),
        ("Sleep timer & battery care", "In Settings — pauses playback; caps charging at 90%."),
        ("Night mode", "Dims the screen to minimal light (Settings \u{25B8} Theme)."),
    ];
    for (h, s) in items {
        bullet(c, t, f, y, h, s);
        y += 64;
    }
}

fn done(c: &mut Canvas, t: &Theme, f: &FontSet) {
    icons::note(c, 240.0, 280.0, 40.0, t.acc);
    let st = sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0);
    let w = text::measure(f, "You're all set", &st) as i32;
    text::draw(c, f, ((W as i32 - w) / 2) as f32, 360.0, "You're all set", &st);
    let s2 = sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0);
    let w2 = text::measure(f, "Tap to start listening.", &s2) as i32;
    text::draw(c, f, ((W as i32 - w2) / 2) as f32, 392.0, "Tap to start listening.", &s2);
}

// Page-position dots near the bottom.
fn page_dots(c: &mut Canvas, t: &Theme, page: usize) {
    let n = PAGES as i32;
    let gap = 16;
    let total = (n - 1) * gap;
    let mut x = W as i32 / 2 - total / 2;
    for i in 0..n {
        let on = i as usize == page;
        let r = if on { 4 } else { 3 };
        let col = if on { t.acc } else { t.line };
        fill_rect(c, x - r, 720 - r, r * 2, r * 2, col);
        x += gap;
    }
}

fn footer(c: &mut Canvas, t: &Theme, f: &FontSet, page: usize) {
    let hint = if page + 1 >= PAGES {
        "TAP TO START   \u{2022}   SWIPE FROM LEFT EDGE TO SKIP"
    } else {
        "TAP NEXT   \u{2022}   TAP LEFT TO GO BACK   \u{2022}   EDGE-SWIPE SKIPS"
    };
    let st = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.14);
    let w = text::measure(f, hint, &st) as i32;
    text::draw(c, f, ((W as i32 - w) / 2) as f32, 760.0, hint, &st);
}
