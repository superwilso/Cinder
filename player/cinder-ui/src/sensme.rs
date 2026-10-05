//! SensMe channels — browsing Sony's own analysis of the library.
//!
//! **Cinder computes none of this.** A PC tool writes an analysis tag into each file — Sony's
//! Music Center for PC, or [Flint](https://github.com/superwilso/flint) — the player's own scanner
//! parses it, and what lands in MTPDB is a channel bitmask, five axes and a chorus position per
//! track (`analysis/RE_sensme_musiccenter.md` §6, §9). `cinder-db` reads those rows, `cinder-ffi`
//! turns them into [`ChannelRow`]s, and this screen is the browse view over them. Which tool wrote
//! the tag is invisible here and must stay that way: the two write different amounts into the file,
//! and the same rows into the database.
//!
//! Two levels, one screen, exactly like `folders.rs`: the channel list, and one channel's tracks.
//! `nav` holds which channel is open, and Back inside one returns to the list.
//!
//! THE CHANNEL LIST IS A GRID (design handoff 5d): two columns of 68 px tiles, each with its track
//! count and a 4 px bar that says how big the channel is next to the biggest one, then a fixed foot
//! with "Follow the time of day" and one primary button. The button shuffles every analysed track —
//! or, with the switch on, plays the channel for the hour (Morning, Daytime, Evening, Night,
//! Midnight). The grid is expressed once, in [`tile_rect`] / [`tile_at`].
//!
//! The layout is expressed ONCE, in [`list_top`]/[`row_top`]/[`row_at`], and both `render` and the
//! hit test read it — the recurring defect in this file's neighbours is a render and a hit test
//! that each compute the same geometry and then drift.

use crate::art;
use crate::canvas::W;
use crate::library::{scrollbar, shuffle_band_rect, LIST_BOTTOM};
use crate::model::Library;
use crate::text::{self, Family, FontSet, TextStyle, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, sty};
use crate::{icons, Canvas};

/// Screen-y of the first row of the CHANNEL LIST.
pub const TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// Row height — the same 62 every other track row uses (`scale::TRACK_ROW_H`). A channel row is
/// the same height as a track row deliberately: the audit counted eleven row heights in this app
/// and adding a twelfth to look different is how that happened.
pub const ROW_H: i32 = crate::scale::TRACK_ROW_H;

/// `y_below` for the channel page's PLAY | SHUFFLE band — straight under the header, since this
/// page has no cover or stat pair above it (the playlist page's 134 leaves room for its cover).
pub const BAND_Y: i32 = crate::chrome::HEADER_BOTTOM;

/// Share of the band width given to PLAY, as the playlist page splits its own.
const PLAY_FRAC: i32 = 62;

/// The channel id that means "every analysed track", not one channel. Sony's own player offers
/// this as a fourteenth tile ("shuffle all") beside its thirteen channels.
pub const ALL: u8 = u8::MAX;

/// What a visual row is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// Open `Library::channels[i]` (a tile of the grid).
    Channel(usize),
    /// Play the open channel from its member `i`.
    Track(usize),
}

// ── The channel grid ─────────────────────────────────────────────────────────────────────────────

pub const TILE_H: i32 = 68;
const TILE_GAP: i32 = 8;
const PITCH: i32 = TILE_H + TILE_GAP;
const TILE_W: i32 = (crate::kit::RIGHT - crate::kit::LEFT - TILE_GAP) / 2;
/// The grid, under its CHANNELS label, down to the fixed foot.
pub const GRID_TOP: i32 = TOP + crate::kit::SECTION_H;
/// The foot: the time-of-day switch row, then the primary button, then a margin.
const FOOT_H: i32 = crate::kit::ROW_H + crate::kit::BUTTON_H + 12;
pub const GRID_BOTTOM: i32 = LIST_BOTTOM - FOOT_H;
pub const FOLLOW_Y: i32 = GRID_BOTTOM;
pub const BUTTON_Y: i32 = GRID_BOTTOM + crate::kit::ROW_H;

/// Tile `i`'s rectangle on screen at `scroll`: `(x, y, w, h)`.
pub fn tile_rect(i: usize, scroll: i32) -> (i32, i32, i32, i32) {
    let (col, row) = ((i % 2) as i32, (i / 2) as i32);
    (
        crate::kit::LEFT + col * (TILE_W + TILE_GAP),
        GRID_TOP + row * PITCH - scroll,
        TILE_W,
        TILE_H,
    )
}

/// Which of `n` tiles is under `(x, y)`. The gaps belong to no tile, and nothing outside the grid's
/// band answers — a half-scrolled tile under the label or the foot is not there to tap.
pub fn tile_at(n: usize, x: i32, y: i32, scroll: i32) -> Option<usize> {
    if !(GRID_TOP..GRID_BOTTOM).contains(&y) {
        return None;
    }
    (0..n).find(|&i| {
        let (tx, ty, tw, th) = tile_rect(i, scroll);
        (tx..tx + tw).contains(&x) && (ty..ty + th).contains(&y)
    })
}

fn grid_h(n: usize) -> i32 {
    if n == 0 {
        0
    } else {
        n.div_ceil(2) as i32 * PITCH - TILE_GAP
    }
}

/// Is `y` on the "Follow the time of day" row?
pub fn follow_hit(y: i32) -> bool {
    (FOLLOW_Y..FOLLOW_Y + crate::kit::ROW_H).contains(&y)
}

/// Is `(x, y)` on the primary button?
pub fn button_hit(x: i32, y: i32) -> bool {
    crate::kit::primary_button_hit(BUTTON_Y, x, y)
}

/// Sony's five time-of-day channels, by id (`cinder_db::SENSME_CHANNELS`): Morning 8, Daytime 9,
/// Evening 10, Night 11, Midnight 12.
///
/// THE HOURS ARE CINDER'S, not Sony's: the stock player's own boundaries have not been read out of
/// it. These follow the names — morning until 10, daytime until 16, evening until 19, night until
/// 23, midnight after — and this function is the only place that knows them.
pub fn time_channel_id(hour: u8) -> u8 {
    match hour {
        5..=9 => 8,
        10..=15 => 9,
        16..=18 => 10,
        19..=22 => 11,
        _ => 12,
    }
}

/// The rows of the level currently open.
pub fn rows(lib: &Library, channel: Option<usize>) -> Vec<Row> {
    match channel {
        // One channel: its members.
        Some(i) => match lib.channels.get(i) {
            Some(ch) => (0..ch.tracks.len()).map(Row::Track).collect(),
            None => Vec::new(),
        },
        // The grid of channels. "Shuffle all" is the foot's button now, not a row.
        None => (0..lib.channels.len()).map(Row::Channel).collect(),
    }
}

/// Top of the scrolling list. The channel page gives up the band's height to it; the channel list
/// starts straight under the header.
pub fn list_top(channel: Option<usize>) -> i32 {
    match channel {
        Some(_) => {
            let (_, by, _, bh) = shuffle_band_rect(BAND_Y);
            by + bh + 8
        }
        None => GRID_TOP,
    }
}

pub fn content_h(lib: &Library, channel: Option<usize>) -> i32 {
    match channel {
        None => grid_h(lib.channels.len()),
        Some(_) => rows(lib, channel).len() as i32 * ROW_H,
    }
}

/// The bottom of the scrolling area: the list runs to the Now Playing bar, the grid to its foot.
pub fn list_bottom(channel: Option<usize>) -> i32 {
    if channel.is_none() {
        GRID_BOTTOM
    } else {
        LIST_BOTTOM
    }
}

pub fn max_scroll_px(lib: &Library, channel: Option<usize>) -> i32 {
    (content_h(lib, channel) - (list_bottom(channel) - list_top(channel))).max(0)
}

/// Screen-y of visual row `r` at `scroll`.
pub fn row_top(r: usize, channel: Option<usize>, scroll: i32) -> i32 {
    list_top(channel) + r as i32 * ROW_H - scroll
}

/// Which row is under `y`? `None` above or below the list, or past the last row. The channel list
/// is a grid, which needs an x: ask [`tile_at`] for it.
pub fn row_at(lib: &Library, channel: Option<usize>, y: i32, scroll: i32) -> Option<Row> {
    channel?;
    let top = list_top(channel);
    if !(top..LIST_BOTTOM).contains(&y) {
        return None;
    }
    let r = ((y - top + scroll.max(0)) / ROW_H) as usize;
    rows(lib, channel).get(r).copied()
}

pub fn play_band() -> (i32, i32, i32, i32) {
    let (bx, by, bw, bh) = shuffle_band_rect(BAND_Y);
    (bx, by, bw * PLAY_FRAC / 100, bh)
}

pub fn shuffle_band() -> (i32, i32, i32, i32) {
    let (bx, by, bw, bh) = shuffle_band_rect(BAND_Y);
    let pw = bw * PLAY_FRAC / 100;
    (bx + pw, by, bw - pw, bh)
}

fn in_rect((rx, ry, rw, rh): (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y)
}

/// True if `(x, y)` is on the PLAY half of the channel page's band.
pub fn hit_play_band(x: i32, y: i32) -> bool {
    in_rect(play_band(), x, y)
}

/// True if `(x, y)` is on the SHUFFLE half.
pub fn hit_shuffle_band(x: i32, y: i32) -> bool {
    in_rect(shuffle_band(), x, y)
}

/// The header title for the level open.
pub fn title(lib: &Library, channel: Option<usize>) -> &str {
    match channel.and_then(|i| lib.channels.get(i)) {
        Some(ch) => ch.name,
        None => "SensMe",
    }
}

/// The header's right-hand caption: how many tracks, so the count is visible before the list is
/// scrolled. Empty on the channel list, where the rows carry their own counts.
pub fn subtitle(lib: &Library, channel: Option<usize>) -> String {
    match channel.and_then(|i| lib.channels.get(i)) {
        Some(ch) => count_label(ch.tracks.len()),
        None => String::new(),
    }
}

fn count_label(n: usize) -> String {
    match n {
        1 => "1 TRACK".to_string(),
        n => format!("{n} TRACKS"),
    }
}

/// The PLAY | SHUFFLE band, drawn as ONE accent rect with a divider — one band with two verbs,
/// the way the playlist page's reads, rather than two stacked accent blocks.
fn band(c: &mut Canvas, t: &Theme, f: &FontSet, tracks: usize) {
    let (px, py, pw, ph) = play_band();
    let (sx, _, sw, _) = shuffle_band();
    fill_rect(c, px, py, pw + sw, ph, t.acc);
    fill_rect(c, sx, py + 10, 1, ph - 20, t.acc_ink);

    let cy = py + ph / 2;
    let lst = sty(Family::Sans, Weight::Bold, 18.0, t.acc_ink, 0.0);
    let sst = sty(Family::Mono, Weight::Regular, 11.0, t.acc_ink, 0.06);
    icons::play(c, (px + 30) as f32, cy as f32, 17.0, t.acc_ink);
    text::draw(
        c,
        f,
        (px + 52) as f32,
        (cy - 3) as f32,
        "Play channel",
        &lst,
    );
    let cl = count_label(tracks);
    text::draw(c, f, (px + 52) as f32, (cy + 15) as f32, &cl, &sst);

    icons::shuffle(c, (sx + 28) as f32, cy as f32, 18.0, t.acc_ink);
    text::draw(c, f, (sx + 48) as f32, (cy + 6) as f32, "Shuffle", &lst);
}

fn name_style(t: &Theme, strong: bool) -> TextStyle {
    sty(
        Family::Sans,
        if strong {
            Weight::SemiBold
        } else {
            Weight::Regular
        },
        18.0,
        t.ink,
        0.0,
    )
}

/// What the screen says when there is no analysis at all.
///
/// This is the whole feature's failure mode, and it is a DATA one: the code is fine, the library
/// simply has no SensMe tags in it. Saying so, and saying what produces them, is the difference
/// between a blank screen and an instruction — the rule this project wrote down after shipping a
/// Bluetooth receiver screen that could not work and did not say so.
pub const EMPTY_TITLE: &str = "No analysed tracks";
pub const EMPTY_BODY: [&str; 3] = [
    "SensMe channels come from an analysis tag inside",
    "each file, written on a PC by Flint or by Sony's",
    "Music Center. Tag your files, then rescan.",
];

/// The grid's foot state, from `nav`.
#[derive(Clone, Copy, Default)]
pub struct Foot {
    /// "Follow the time of day" is on.
    pub follow: bool,
    /// The hour on the status-bar clock, when there is one.
    pub hour: Option<u8>,
}

/// The channel the foot's button would play, as an index into `lib.channels`: the time-of-day one
/// when following and it has tracks, otherwise `None` (every analysed track, shuffled).
pub fn foot_channel(lib: &Library, foot: Foot) -> Option<usize> {
    if !foot.follow {
        return None;
    }
    let id = time_channel_id(foot.hour?);
    lib.channels.iter().position(|ch| ch.id == id)
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    channel: Option<usize>,
    scroll_px: i32,
    sbar_active: bool,
    foot: Foot,
) {
    let top = list_top(channel);
    let scroll = scroll_px.clamp(0, max_scroll_px(lib, channel));
    c.fill(t.bg);
    let sub = subtitle(lib, channel);
    let y0 = crate::chrome::header(
        c,
        t,
        f,
        title(lib, channel),
        (!sub.is_empty()).then_some(&sub),
    );

    let all = rows(lib, channel);
    if channel.is_some() {
        let n = channel
            .and_then(|i| lib.channels.get(i))
            .map_or(0, |ch| ch.tracks.len());
        band(c, t, f, n);
    }

    if all.is_empty() {
        let ts = sty(Family::Sans, Weight::SemiBold, 18.0, t.ink, 0.0);
        let bs = sty(Family::Sans, Weight::Regular, crate::scale::ROW, t.dim, 0.0);
        text::draw(c, f, 22.0, (y0 + 48) as f32, EMPTY_TITLE, &ts);
        for (i, line) in EMPTY_BODY.iter().enumerate() {
            text::draw(c, f, 22.0, (y0 + 80 + 24 * i as i32) as f32, line, &bs);
        }
        return;
    }

    if channel.is_none() {
        grid(c, t, f, lib, scroll, foot);
        scrollbar(
            c,
            t,
            top,
            GRID_BOTTOM,
            scroll,
            content_h(lib, None),
            sbar_active,
        );
        return;
    }

    c.set_clip_y(top, LIST_BOTTOM);
    // Only the visible window — a channel can hold the whole library.
    let first = (scroll / ROW_H).max(0) as usize;
    for (r, row) in all.iter().enumerate().skip(first) {
        let y = row_top(r, channel, scroll);
        if y >= LIST_BOTTOM {
            break;
        }
        let cy = y + ROW_H / 2;
        match row {
            Row::Channel(i) => {
                let Some(ch) = lib.channels.get(*i) else {
                    continue;
                };
                // A gradient tile keyed by the channel's name, the way a coverless album row is
                // drawn — cached, so a screenful costs a blit each rather than a gradient each.
                art::block_cached(c, t, 22, y + (ROW_H - 40) / 2, 40, 40, ch.name, 1.0);
                let ns = name_style(t, true);
                text::draw(c, f, 76.0, (cy + 6) as f32, ch.name, &ns);
                let cs = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.12);
                let cl = count_label(ch.tracks.len());
                let w = text::measure(f, &cl, &cs);
                text::draw(c, f, 440.0 - w, (cy + 5) as f32, &cl, &cs);
                icons::chevron(c, 452.0, cy as f32, 9.0, t.faint);
            }
            Row::Track(i) => {
                let Some(s) = channel
                    .and_then(|ci| lib.channels.get(ci))
                    .and_then(|ch| ch.tracks.get(*i))
                    .and_then(|&ix| lib.songs.get(ix as usize))
                else {
                    continue;
                };
                let ns = name_style(t, false);
                text::draw(
                    c,
                    f,
                    34.0,
                    (cy + 1) as f32,
                    &crate::widgets::fit(f, &s.title, &ns, 320.0),
                    &ns,
                );
                let ss = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
                text::draw(
                    c,
                    f,
                    34.0,
                    (cy + 19) as f32,
                    &crate::widgets::fit(f, &s.artist, &ss, 320.0),
                    &ss,
                );
                let ds = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.06);
                let w = text::measure(f, &s.dur, &ds);
                text::draw(c, f, 458.0 - w, (cy + 5) as f32, &s.dur, &ds);
            }
        }
        hline(c, y + ROW_H - 1, t.line);
    }
    c.clear_clip();
    scrollbar(
        c,
        t,
        top,
        LIST_BOTTOM,
        scroll,
        content_h(lib, channel),
        sbar_active,
    );
    let _ = W;
}

/// The channel grid and its foot.
fn grid(c: &mut Canvas, t: &Theme, f: &FontSet, lib: &Library, scroll: i32, foot: Foot) {
    use crate::kit;
    kit::section_label(c, t, f, TOP, "CHANNELS", None);
    let biggest = lib
        .channels
        .iter()
        .map(|ch| ch.tracks.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let now = foot_channel(lib, foot);
    c.set_clip_y(GRID_TOP, GRID_BOTTOM);
    for (i, ch) in lib.channels.iter().enumerate() {
        let (x, y, w, h) = tile_rect(i, scroll);
        if y + h < GRID_TOP || y >= GRID_BOTTOM {
            continue;
        }
        fill_rect(c, x, y, w, h, t.panel);
        crate::widgets::stroke_rect(
            c,
            x,
            y,
            w,
            h,
            if now == Some(i) { t.acc } else { t.line },
            1,
        );
        let ns = sty(
            Family::Sans,
            Weight::SemiBold,
            crate::scale::ROW,
            t.ink,
            0.0,
        );
        let cs = sty(
            Family::Mono,
            Weight::Regular,
            crate::scale::CAPTION,
            t.faint,
            0.1,
        );
        // NOW marks the channel the button will play; it takes its width from the name.
        let tag_w = if now == Some(i) {
            let tst = sty(
                Family::Mono,
                Weight::Regular,
                crate::scale::CAPTION,
                t.acc,
                0.14,
            );
            let tw = text::measure(f, "NOW", &tst);
            text::draw(c, f, (x + w - 12) as f32 - tw, (y + 26) as f32, "NOW", &tst);
            tw + 8.0
        } else {
            0.0
        };
        let name = crate::widgets::fit(f, ch.name, &ns, (w - 24) as f32 - tag_w);
        text::draw(c, f, (x + 12) as f32, (y + 28) as f32, &name, &ns);
        let cl = count_label(ch.tracks.len());
        text::draw(
            c,
            f,
            (x + 12) as f32,
            (y + 50) as f32,
            &crate::widgets::fit(f, &cl, &cs, (w - 24) as f32),
            &cs,
        );
        // The count bar: how big this channel is next to the biggest one.
        let bw = w - 24;
        fill_rect(c, x + 12, y + h - 10, bw, 4, t.line);
        let fill = (bw as usize * ch.tracks.len() / biggest) as i32;
        fill_rect(c, x + 12, y + h - 10, fill.max(2), 4, t.acc);
    }
    c.clear_clip();

    // The foot, ruled off from a half-scrolled tile above it.
    hline(c, FOLLOW_Y, t.line);
    let sub = match (foot.follow, foot.hour.map(time_channel_id)) {
        (false, _) => "Play the channel for the hour".to_string(),
        (true, None) => "Waiting for the clock".to_string(),
        (true, Some(id)) => {
            let name = crate::model::SENSME_TIME_NAMES
                .iter()
                .find(|(i, _)| *i == id)
                .map_or("", |(_, n)| n);
            match now {
                Some(_) => format!("Now: {name}"),
                None => format!("Now: {name} — nothing in it yet"),
            }
        }
    };
    kit::row(
        c,
        t,
        f,
        FOLLOW_Y,
        kit::ROW_H,
        &crate::kit::Row::new("Follow the time of day")
            .sub(&sub)
            .trail(crate::kit::Trail::Switch(foot.follow)),
    );
    let label = match now.and_then(|i| lib.channels.get(i)) {
        Some(ch) => format!("Play {}", ch.name),
        None => "Shuffle all analysed".to_string(),
    };
    kit::primary_button(c, t, f, BUTTON_Y, &label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChannelRow, SongRow};

    fn lib() -> Library {
        let song = |n: &str| SongRow {
            title: n.into(),
            dur: "3:00".into(),
            ..Default::default()
        };
        Library {
            songs: vec![song("a"), song("b"), song("c")],
            channels: vec![
                ChannelRow {
                    id: 0,
                    name: "Active",
                    tracks: vec![0, 2],
                },
                ChannelRow {
                    id: 8,
                    name: "Morning",
                    tracks: vec![1],
                },
            ],
            sensme_tracks: 3,
            ..Default::default()
        }
    }

    /// Every row the renderer draws must be the row the hit test finds under it, at any scroll —
    /// the channel page's list (under its band), and the grid's tiles (under their label).
    #[test]
    fn the_hit_test_agrees_with_the_rows_at_every_scroll() {
        let l = lib();
        let channel = Some(0);
        let all = rows(&l, channel);
        for (r, want) in all.iter().enumerate() {
            for scroll in [0, 9, ROW_H, ROW_H * 2 - 1] {
                let y = row_top(r, channel, scroll) + ROW_H / 2;
                if (list_top(channel)..LIST_BOTTOM).contains(&y) {
                    assert_eq!(
                        row_at(&l, channel, y, scroll),
                        Some(*want),
                        "row {r} at scroll {scroll}"
                    );
                }
            }
        }
        let past = row_top(all.len(), channel, 0) + ROW_H / 2;
        if past < LIST_BOTTOM {
            assert_eq!(row_at(&l, channel, past, 0), None);
        }
        // The grid: every tile answers at its centre, the gaps belong to nobody, and nothing
        // under the foot answers even when a scrolled tile would be drawn there.
        for n in [1usize, 2, 5, 13] {
            for scroll in [0, 30] {
                for i in 0..n {
                    let (x, y, w, h) = tile_rect(i, scroll);
                    let (cx, cy) = (x + w / 2, y + h / 2);
                    let want = (GRID_TOP..GRID_BOTTOM).contains(&cy).then_some(i);
                    assert_eq!(
                        tile_at(n, cx, cy, scroll),
                        want,
                        "tile {i} of {n} at scroll {scroll}"
                    );
                }
            }
        }
        let (x0, _, w0, _) = tile_rect(0, 0);
        assert_eq!(
            tile_at(2, x0 + w0 + 4, GRID_TOP + 30, 0),
            None,
            "the gap between columns"
        );
        assert_eq!(
            row_at(&l, None, GRID_TOP + 10, 0),
            None,
            "the grid needs an x: ask tile_at"
        );
    }

    /// The grid is the channels; a channel opens onto its members only.
    #[test]
    fn the_two_levels_have_the_rows_they_should() {
        let l = lib();
        assert_eq!(rows(&l, None), vec![Row::Channel(0), Row::Channel(1)]);
        assert_eq!(rows(&l, Some(0)), vec![Row::Track(0), Row::Track(1)]);
        assert_eq!(rows(&l, Some(1)), vec![Row::Track(0)]);
        // A channel index that no longer exists resolves to nothing rather than panicking.
        assert!(rows(&l, Some(9)).is_empty());
        assert_eq!(title(&l, Some(9)), "SensMe");
        assert_eq!(title(&l, Some(1)), "Morning");
        assert_eq!(subtitle(&l, Some(0)), "2 TRACKS");
        assert_eq!(subtitle(&l, Some(1)), "1 TRACK");
        assert_eq!(subtitle(&l, None), "");
    }

    /// The foot's button plays the time-of-day channel only when following AND that channel has
    /// tracks; otherwise it shuffles everything analysed. The hours map onto Sony's five.
    #[test]
    fn the_foot_follows_the_clock_when_asked() {
        let l = lib(); // Active (id 0) and Morning (id 8)
        assert_eq!(
            foot_channel(
                &l,
                Foot {
                    follow: false,
                    hour: Some(7)
                }
            ),
            None,
            "not following"
        );
        assert_eq!(
            foot_channel(
                &l,
                Foot {
                    follow: true,
                    hour: Some(7)
                }
            ),
            Some(1),
            "07:00 is Morning"
        );
        assert_eq!(
            foot_channel(
                &l,
                Foot {
                    follow: true,
                    hour: Some(21)
                }
            ),
            None,
            "Night has nothing"
        );
        assert_eq!(
            foot_channel(
                &l,
                Foot {
                    follow: true,
                    hour: None
                }
            ),
            None,
            "no clock yet"
        );
        let ids: Vec<u8> = (0..24).map(time_channel_id).collect();
        assert!(ids.iter().all(|id| (8..=12).contains(id)));
        assert_eq!(
            (
                time_channel_id(4),
                time_channel_id(5),
                time_channel_id(12),
                time_channel_id(17),
                time_channel_id(20),
                time_channel_id(23)
            ),
            (12, 8, 9, 10, 11, 12)
        );
        // The foot sits below the grid and above the Now Playing bar.
        assert!(BUTTON_Y + crate::kit::BUTTON_H <= LIST_BOTTOM);
        assert!(follow_hit(FOLLOW_Y + 10) && !follow_hit(GRID_BOTTOM - 1));
        assert!(button_hit(240, BUTTON_Y + 20));
    }

    /// A library nobody has analysed has no rows, nothing to scroll, and an empty state that says
    /// what would produce some.
    #[test]
    fn nothing_analysed_is_an_explained_empty_screen() {
        let empty = Library::default();
        assert!(rows(&empty, None).is_empty());
        assert_eq!(max_scroll_px(&empty, None), 0);
        assert_eq!(content_h(&empty, None), 0);
        assert!(row_at(&empty, None, TOP + 10, 0).is_none());
        assert!(EMPTY_BODY.iter().any(|l| l.contains("Flint")));
        assert!(EMPTY_BODY.iter().any(|l| l.contains("Music Center")));
    }

    /// The band's two halves tile the accent rect exactly and never overlap: a gap would be a dead
    /// strip in the middle of the biggest control on the page.
    #[test]
    fn the_band_halves_tile_the_band() {
        let (bx, by, bw, bh) = shuffle_band_rect(BAND_Y);
        let (px, py, pw, ph) = play_band();
        let (sx, sy, sw, sh) = shuffle_band();
        assert_eq!((px, py, ph), (bx, by, bh));
        assert_eq!((sy, sh), (by, bh));
        assert_eq!(px + pw, sx);
        assert_eq!(sx + sw, bx + bw);
        assert!(hit_play_band(px + 4, by + bh / 2));
        assert!(hit_shuffle_band(sx + 4, by + bh / 2));
        assert!(!hit_play_band(sx + 4, by + bh / 2));
        assert!(!hit_shuffle_band(px + 4, by + bh / 2));
        // And the band is above the list, so a tap on it can never also be a row.
        assert!(by + bh <= list_top(Some(0)));
    }
}
