//! First-run onboarding + re-viewable Help/Controls. A short paged intro shown ONCE on first boot
//! (Welcome → Getting around → Buttons → Playing & Up Next → Gestures → Features → Done),
//! persisted so it doesn't reappear; also openable any time from the Menu ("Help & Controls").
//! Touch-navigated (the NW-A55 has no d-pad): tap the right side = next / finish, tap the left
//! side = back a page, left-edge swipe = skip.
//!
//! EVERY ROW IS SOMETHING THAT EXISTS, pinned to where it is implemented, because a help screen
//! that describes the app you meant to write is worse than none:
//!   * Getting around   -> `chrome::status_hit` (clock zone / bookmark / the rest), the header
//!     chevron + left-edge swipe (`nav::tap`, the shell's edge swipe), `now_playing::TOOLBAR_CX`
//!   * Playing & Up Next -> `Action::PlayListAt` / `PlayPlaylistAt` / album taps, `QueueAt`,
//!     `App::queue_shuffle`, `App::remove_upcoming`, `up_next::GRIP_X0`, `App::queue_clear`
//!   * Buttons          -> `App::power_held` (the Power menu), `App::set_hold`

use crate::canvas::W;
use crate::icons;
use crate::text::{Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, sty};
use crate::Canvas;

pub const PAGES: usize = 7;

pub fn render(c: &mut Canvas, t: &Theme, f: &FontSet, page: usize) {
    c.fill(t.bg);
    match page {
        0 => welcome(c, t, f),
        1 => getting_around(c, t, f),
        2 => controls(c, t, f),
        3 => playing(c, t, f),
        4 => gestures(c, t, f),
        5 => features(c, t, f),
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

fn title(c: &mut Canvas, t: &Theme, f: &FontSet, s: &str) {
    crate::widgets::draw_fit(c, f, 36.0, 90.0, s, &sty(Family::Sans, Weight::Bold, 32.0, t.ink, 0.0), 458.0);
}

fn section(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, s: &str) {
    crate::widgets::draw_fit(c, f, 38.0, y as f32, s, &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18), 458.0);
}

/// One row with a drawn ICON in the key column — the glyph the user will actually see.
/// `None` leaves the icon column empty, so every key on the page still starts at one x.
fn icon_row(c: &mut Canvas, t: &Theme, f: &FontSet, y: i32, draw: Option<fn(&mut Canvas, f32, f32, f32, embedded_graphics::pixelcolor::Rgb888)>, key: &str, action: &str) {
    if let Some(draw) = draw {
        draw(c, 50.0, (y - 5) as f32, 20.0, t.acc);
    }
    crate::widgets::draw_fit(c, f, 72.0, y as f32, key, &sty(Family::Mono, Weight::Bold, 14.0, t.acc, 0.04), 160.0);
    crate::widgets::draw_fit(c, f, 168.0, y as f32, action, &sty(Family::Sans, Weight::Regular, 16.0, t.ink, 0.0), 458.0);
}

/// HOW TO GET AROUND — the page the intro was missing. Every screen has the same furniture, and
/// none of it is labelled on the glass: the clock is a Now Playing button, the strip is the Menu,
/// the bookmark is the Shelf.
fn getting_around(c: &mut Canvas, t: &Theme, f: &FontSet) {
    title(c, t, f, "Getting around");
    section(c, t, f, 124, "THE TOP STRIP, ON EVERY SCREEN");
    let mut y = 152;
    icon_row(c, t, f, y, None, "Clock", "Back to Now Playing");
    y += 46;
    icon_row(c, t, f, y, Some(icons::menu), "Menu", "Every screen, one tap");
    y += 46;
    icon_row(c, t, f, y, Some(icons::bookmark), "Shelf", "Save your place, jump back");
    y += 64;
    section(c, t, f, y - 28, "GOING BACK");
    icon_row(c, t, f, y, Some(icons::back), "Arrow", "Top left of every screen");
    y += 46;
    icon_row(c, t, f, y, None, "Swipe \u{2192}", "In from the left edge");
    y += 64;
    section(c, t, f, y - 28, "NOW PLAYING'S BOTTOM BAR");
    // Drawn as the real bar is: four icons in the same slots, each named underneath.
    let row = y + 8;
    let labels = ["Library", "Up Next", "Bluetooth", "Settings"];
    let lst = sty(Family::Sans, Weight::Regular, 14.0, t.ink, 0.0);
    for (i, cx) in crate::now_playing::TOOLBAR_CX.iter().enumerate() {
        let x = *cx as f32;
        match i {
            0 => icons::library(c, x, row as f32, 24.0, t.acc),
            1 => icons::queue(c, x, row as f32, 24.0, t.acc),
            2 => icons::bt(c, x, row as f32, 23.0, t.acc),
            _ => icons::settings(c, x, row as f32, 24.0, t.acc),
        }
        crate::widgets::center(c, f, x, (row + 36) as f32, labels[i], &lst);
    }
    y = row + 76;
    crate::widgets::draw_fit(c, f, 38.0, y as f32, "Lists show a Now Playing strip at the bottom:",
                             &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0), 458.0);
    crate::widgets::draw_fit(c, f, 38.0, (y + 22) as f32, "tap it to go back to the song.",
                             &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0), 458.0);
}

/// PLAYING AND UP NEXT — the rules the queue follows since it became one list (2026-09-23).
fn playing(c: &mut Canvas, t: &Theme, f: &FontSet) {
    title(c, t, f, "Playing & Up Next");
    section(c, t, f, 124, "ON ANY SONG");
    let mut y = 152;
    let rows: [(&str, &str); 3] = [
        ("Tap", "Play from there, in order"),
        ("Swipe \u{2192}", "Add to Up Next, at the end"),
        ("Swipe \u{2190}", "Play next, after this song"),
    ];
    for (k, a) in rows {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
    y += 18;
    section(c, t, f, y, "IN UP NEXT");
    y += 28;
    let rows: [(&str, &str); 5] = [
        ("Tap", "Jump to that song"),
        ("Drag \u{2261}", "Move it anywhere"),
        ("Swipe", "Remove it"),
        ("MIX", "Shuffle the whole list"),
        ("CLEAR", "Empty what is left"),
    ];
    for (k, a) in rows {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
    crate::widgets::draw_fit(c, f, 38.0, (y + 4) as f32,
                             "Shuffle off puts the list back in order.",
                             &sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0), 458.0);
}

fn controls(c: &mut Canvas, t: &Theme, f: &FontSet) {
    title(c, t, f, "Buttons");

    // The physical buttons (the only ones the device has) — all transport + power.
    section(c, t, f, 124, "ON THE SIDE");
    let mut y = 152;
    let buttons: [(&str, &str); 4] = [
        ("PLAY", "Play / pause"),
        ("\u{25C1} REWIND", "Previous track"),
        ("SKIP \u{25B7}", "Next track"),
        ("VOL + / \u{2212}", "Volume"),
    ];
    for (k, a) in buttons {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
    y += 18;
    section(c, t, f, y, "POWER AND HOLD");
    y += 28;
    let power: [(&str, &str); 3] = [
        ("POWER", "Screen on / off"),
        ("HOLD POWER", "Power off or restart"),
        ("HOLD switch", "Locks the screen; buttons work"),
    ];
    for (k, a) in power {
        ctl(c, t, f, y, k, a);
        y += 46;
    }
}


/// The gesture vocabulary that is NOT obvious from looking at the screen, beyond the list swipes
/// the Playing page teaches.
///   * Now Playing ↔   -> `nav::np_page` above `now_playing::PAGE_BOT`, skip below it
///   * title tap       -> `now_playing::hit_info` -> Track information -> Add to playlist
///   * bottom edge     -> the shell's `SHELF_EDGE_Y` band + `nav::shelf_swipe_open`
///   * the long press  -> `App::reorder_begin_hold`
fn gestures(c: &mut Canvas, t: &Theme, f: &FontSet) {
    title(c, t, f, "Gestures");

    section(c, t, f, 124, "NOW PLAYING");
    let mut y = 152;
    let np: [(&str, &str); 3] = [
        ("Swipe \u{2194} art", "Cover \u{00b7} spectrum \u{00b7} level"),
        ("Swipe \u{2194} below", "Previous / next track"),
        ("Tap the title", "Track info \u{00b7} add to a playlist"),
    ];
    for (k, a) in np {
        ctl(c, t, f, y, k, a);
        y += 46;
    }

    y += 18;
    section(c, t, f, y, "ANYWHERE");
    y += 28;
    let any: [(&str, &str); 3] = [
        ("Swipe \u{2191}", "From the bottom: the Shelf"),
        ("Swipe \u{2195}", "Scroll a list"),
        ("Hold a row", "Up Next: pick it up to move"),
    ];
    for (k, a) in any {
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
        ("Up Next", "One list in play order \u{2014} add, move, remove anything."),
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
    //
    // And short enough at EVERY UI scale: on the owner's player (above 100%) the longer wording came
    // out as "…EDGE-SWIPE…" and lost "SKIP" again. `footer_fits_at_every_scale` holds the line.
    let st = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.14);
    crate::widgets::center(c, f, (W / 2) as f32, 760.0, footer_hint(page), &st);
}

fn footer_hint(page: usize) -> &'static str {
    if page + 1 >= PAGES {
        "TAP: START  \u{2022}  EDGE: SKIP"
    } else {
        "TAP: NEXT  \u{2022}  LEFT: BACK  \u{2022}  EDGE: SKIP"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_fits_at_every_scale() {
        let _g = crate::text::scale_guard();
        let f = FontSet::load();
        let t = Theme::day();
        let st = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.14);
        for idx in 0..crate::text::SCALE_STEPS.len() {
            crate::text::set_scale_idx(idx);
            for page in [0, PAGES - 1] {
                let w = crate::text::measure(&f, footer_hint(page), &st);
                assert!(w <= (W as f32) - 32.0,
                        "page {page} hint is {w} px at {}%", crate::text::SCALE_STEPS[idx]);
            }
        }
        crate::text::set_scale_idx(crate::text::SCALE_STEPS.iter().position(|s| *s == 100).unwrap());
    }
}
