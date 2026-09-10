//! First-run onboarding + re-viewable Help/Controls. A short paged intro shown ONCE on first boot
//! (Welcome → Controls → Gestures → Features → Done), persisted so it doesn't reappear; also openable any time
//! from the Menu ("Help & Controls"). Touch-navigated (the NW-A55 has no d-pad): tap the right side
//! = next / finish, tap the left side = back a page, left-edge swipe = skip. The Controls page
//! matters most — it teaches the touch + transport-button model the rest of Cinder uses.

use crate::canvas::W;
use crate::icons;
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};
use crate::Canvas;

pub const PAGES: usize = 5;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, page: usize) {
    c.fill(t.bg);
    match page {
        0 => welcome(c, t, f),
        1 => controls(c, t, f),
        2 => gestures(c, t, f),
        3 => features(c, t, f),
        _ => done(c, t, f),
    }
    page_dots(c, t, page);
    footer(c, t, f, page);
}

fn welcome(c: &mut Canvas, t: &Theme, f: &FontSet) {
    // accent wordmark + tagline
    crate::widgets::draw_fit(c, f, 36.0, 300.0, "CINDER", &sty(Family::Sans, Weight::Bold, 52.0, t.acc, 0.02), 458.0);
    crate::widgets::draw_fit(c, f, 38.0, 340.0, "Your music, clean and quiet.", &sty(Family::Sans, Weight::Regular, 18.0, t.ink, 0.0), 458.0);
    crate::widgets::draw_fit(c, f, 38.0, 366.0, "A replacement player for the Walkman.", &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0), 458.0);
}

// One "key → action" row on the Controls page.
fn ctl(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, key: &str, action: &str) {
    crate::widgets::draw_fit(c, f, 38.0, y as f32, key, &sty(Family::Mono, Weight::Bold, 14.0, t.acc, 0.04), 458.0);
    crate::widgets::draw_fit(c, f, 168.0, y as f32, action, &sty(Family::Sans, Weight::Regular, 16.0, t.ink, 0.0), 458.0);
}

fn controls(c: &mut Canvas, t: &Theme, f: &FontSet) {
    crate::widgets::draw_fit(c, f, 36.0, 90.0, "Controls", &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0), 458.0);

    // The physical buttons (the only ones the device has) — all transport + power.
    crate::widgets::draw_fit(c, f, 38.0, 124.0, "BUTTONS", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18), 458.0);
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
    crate::widgets::draw_fit(c, f, 38.0, y as f32, "TOUCH", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18), 458.0);
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


/// The gesture vocabulary that is NOT obvious from looking at the screen.
///
/// Split off the Controls page rather than appended to it: that page was already full at five
/// buttons and four touch rows, and the swipe/drag set has grown well past what fits under it.
///
/// EVERY ROW HERE IS A GESTURE THAT EXISTS. The temptation with a help screen is to describe the
/// app you meant to write — so each of these is pinned to its implementation:
///   * row swipes      -> `library::SwipeIntent` (rightward queues, leftward plays next)
///   * queue-row swipe -> the same, with `SwipeIntent::Remove`
///   * the handle      -> `up_next::GRIP_X0..GRIP_X1`, drawn on queued AND upcoming rows
///   * bottom edge     -> the shell's `SHELF_EDGE_Y` band + `nav::shelf_swipe_open`
///   * Now Playing ↔   -> `nav::np_page` above `now_playing::PAGE_BOT`, skip below it
///   * title tap       -> `now_playing::hit_info` -> Track information -> Add to playlist
fn gestures(c: &mut Canvas, t: &Theme, f: &FontSet) {
    crate::widgets::draw_fit(c, f, 36.0, 90.0, "Gestures", &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0), 458.0);

    crate::widgets::draw_fit(c, f, 38.0, 124.0, "IN ANY LIST", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18), 458.0);
    let mut y = 152;
    let lists: [(&str, &str); 4] = [
        ("Swipe \u{2192}", "Add to the queue"),
        ("Swipe \u{2190}", "Play it next"),
        ("Drag \u{2261}", "Reorder \u{2014} queue or album"),
        ("Swipe \u{2191} bottom", "Open the Shelf"),
    ];
    for (k, a) in lists {
        ctl(c, t, f, y, k, a);
        y += 46;
    }

    y += 18;
    crate::widgets::draw_fit(c, f, 38.0, y as f32, "NOW PLAYING", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18), 458.0);
    y += 28;
    let np: [(&str, &str); 3] = [
        ("Swipe \u{2194} art", "Cover \u{00b7} spectrum \u{00b7} level"),
        ("Swipe \u{2194} below", "Previous / next track"),
        ("Tap the title", "Track info \u{00b7} add to a playlist"),
    ];
    for (k, a) in np {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
}

// One feature bullet.
fn bullet(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, head: &str, sub: &str) {
    fill_rect(c, 38, y - 9, 4, 14, t.acc); // accent tick
    crate::widgets::draw_fit(c, f, 54.0, y as f32, head, &sty(Family::Sans, Weight::SemiBold, 17.0, t.ink, 0.0), 458.0);
    crate::widgets::draw_fit(c, f, 54.0, (y + 19) as f32, sub, &sty(Family::Sans, Weight::Regular, 14.0, t.dim, 0.0), 458.0);
}

fn features(c: &mut Canvas, t: &Theme, f: &FontSet) {
    crate::widgets::draw_fit(c, f, 36.0, 92.0, "What's inside", &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0), 458.0);
    let mut y = 150;
    // COUNTS COME FROM THE CODE. "Five real-time types" was written when there were five; there
    // are `viz::COUNT` = 8, and a help screen that miscounts the thing it is pointing at is worse
    // than one that stays vague.
    let items: [(&str, &str); 7] = [
        ("Library", "Songs, albums, artists, folders \u{2014} thousands of tracks."),
        ("Up Next", "One list: history, now playing, your queue, the album."),
        ("Playlists", "Make them on the device \u{2014} rename, add and remove tracks."),
        ("Sound", "10-band EQ + DSEE/VPT/Vinyl/ClearAudio+, A/B compare."),
        ("Bluetooth & USB-DAC", "LDAC out, and a USB sound card that keeps Bluetooth."),
        ("Visualiser", "Eight real-time types on Now Playing."),
        ("Sleep timer & battery care", "In Settings \u{2014} pauses playback; caps charging at 90%."),
    ];
    for (h, s) in items {
        bullet(c, t, f, y, h, s);
        y += 60;
    }
}

fn done(c: &mut Canvas, t: &Theme, f: &FontSet) {
    icons::note(c, 240.0, 280.0, 40.0, t.acc);
    let st = sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0);
    crate::widgets::center(c, f, (W / 2) as f32, 360.0, "You're all set", &st);
    let s2 = sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0);
    crate::widgets::center(c, f, (W / 2) as f32, 392.0, "Tap to start listening.", &s2);
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
    // SHORT ENOUGH TO FIT. `center` does not clamp, so an over-long hint is centred and loses BOTH
    // ends — the previous wording ran past the right edge and the word it cut was "SKIPS", i.e.
    // the escape hatch. Measured against the 480 px panel at 11 px mono with 0.14 tracking.
    let hint = if page + 1 >= PAGES {
        "TAP TO START   \u{2022}   EDGE-SWIPE: SKIP"
    } else {
        "TAP: NEXT   \u{2022}   LEFT: BACK   \u{2022}   EDGE-SWIPE: SKIP"
    };
    let st = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.14);
    crate::widgets::center(c, f, (W / 2) as f32, 760.0, hint, &st);
}
