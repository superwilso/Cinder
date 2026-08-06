//! Shared chrome — the status bar (`CStatus` in cinder-proto-screens1.jsx),
//! used by every screen. Left: clock + codec badge + NIGHT. Right: menu ·
//! bookmark · bt · battery.

use crate::canvas::Canvas;
use crate::icons;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

fn fill_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb888) {
    Rectangle::new(Point::new(x, y), Size::new(w.max(0) as u32, h.max(0) as u32))
        .into_styled(PrimitiveStyle::with_fill(col))
        .draw(c)
        .ok();
}

fn sty(fam: Family, weight: Weight, size: f32, color: Rgb888, tracking: f32) -> TextStyle {
    TextStyle { fam, weight, size, color, tracking }
}

/// The live status-bar payload (clock, codec badge, battery %). Set ONCE per frame — by
/// `nav::App::render` on device, and by the host harness for previews — then read by every
/// screen's `status_bar()` call.
///
/// This used to be three parameters, and **13 of the 15 screens passed the literals
/// `"14:32" / "FLAC 24/96" / 78`**: outside Now Playing and the Lock screen the device showed a
/// frozen fake clock and a fake 78% battery. Routing it through one per-frame slot means a screen
/// physically cannot draw a stale status bar — there is nowhere left to hardcode one.
#[derive(Clone, Default)]
pub struct Status {
    pub clock: String,
    pub badge: String,
    pub battery: u8,
}

thread_local! {
    static STATUS: core::cell::RefCell<Status> = const { core::cell::RefCell::new(
        Status { clock: String::new(), badge: String::new(), battery: 0 }
    ) };
}

/// Publish this frame's status-bar values. Rendering is single-threaded (device: under the
/// cinder-ffi mutex; host: the main thread), so a thread-local needs no locking.
pub fn set_status(clock: &str, badge: &str, battery: u8) {
    STATUS.with(|s| {
        let mut s = s.borrow_mut();
        s.clock.clear();
        s.clock.push_str(clock);
        s.badge.clear();
        s.badge.push_str(badge);
        s.battery = battery;
    });
}

pub fn status_bar(c: &mut Canvas, t: &Theme, f: &FontSet) {
    let st = STATUS.with(|s| s.borrow().clone());
    status_bar_with(c, t, f, &st.clock, &st.badge, st.battery)
}

fn status_bar_with(c: &mut Canvas, t: &Theme, f: &FontSet, clock: &str, badge: &str, battery: u8) {
    // left: clock + codec badge + (NIGHT)
    let cx = text::draw(c, f, 18.0, 22.0, clock, &sty(Family::Mono, Weight::Regular, 13.0, t.dim, 0.06));
    // Skip the whole badge when there is no codec string. Drawing it unconditionally left a bare
    // 12px accent-stroked rectangle floating next to the clock whenever nothing was loaded —
    // caught on a live device screenshot; the host harness never renders that state.
    let mut nx = cx;
    if !badge.is_empty() {
        let bst = sty(Family::Mono, Weight::Regular, 11.0, t.acc, 0.12);
        let bw = text::measure(f, badge, &bst);
        let bx = cx + 12.0;
        Rectangle::new(Point::new((bx - 6.0) as i32, 7), Size::new((bw + 12.0) as u32, 18))
            .into_styled(PrimitiveStyle::with_stroke(t.acc, 1))
            .draw(c)
            .ok();
        nx = text::draw(c, f, bx, 21.0, badge, &bst);
    }
    if t.night {
        text::draw(c, f, nx + 12.0, 21.0, "NIGHT", &sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.18));
    }

    // right: menu ≡, bookmark, bt, [battery]
    icons::menu(c, 368.0, 17.0, 18.0, t.dim);
    icons::bookmark(c, 392.0, 17.0, 15.0, t.dim);
    icons::bt(c, 414.0, 17.0, 15.0, t.faint);
    let batt = format!("{}", battery);
    let bs = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.04);
    let bwid = text::measure(f, &batt, &bs);
    text::draw(c, f, 448.0 - bwid, 21.0, &batt, &bs);
    Rectangle::new(Point::new(452, 11), Size::new(18, 11))
        .into_styled(PrimitiveStyle::with_stroke(t.faint, 1))
        .draw(c)
        .ok();
    fill_rect(c, 470, 14, 2, 4, t.faint); // nub
    fill_rect(c, 454, 13, (14.0 * battery as f32 / 100.0) as i32, 7, t.faint); // charge
}

/// Screen header (`CHeader`): back chevron + title (27/700) + optional right caption.
/// Returns the y where content below the header should start.
pub fn header(c: &mut Canvas, t: &Theme, f: &FontSet, title: &str, right: Option<&str>) -> i32 {
    icons::back(c, 30.0, 62.0, 20.0, t.dim);
    let ts = sty(Family::Sans, Weight::Bold, 30.0, t.ink, -0.01);
    let title_end = text::draw(c, f, 50.0, 70.0, title, &ts);
    if let Some(r) = right {
        let rs = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1);
        // Clamp the caption to the space right of the title (never let it overlap the title).
        let avail = (458.0 - (title_end + 16.0)).max(0.0);
        let r = crate::widgets::fit(f, r, &rs, avail);
        let rw = text::measure(f, &r, &rs);
        text::draw(c, f, 458.0 - rw, 65.0, &r, &rs);
    }
    91
}
