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
    /// Open `Library::channels[i]`.
    Channel(usize),
    /// Shuffle every analysed track — the channel list's first row.
    All,
    /// Play the open channel from its member `i`.
    Track(usize),
}

/// The rows of the level currently open.
pub fn rows(lib: &Library, channel: Option<usize>) -> Vec<Row> {
    match channel {
        // One channel: its members.
        Some(i) => match lib.channels.get(i) {
            Some(ch) => (0..ch.tracks.len()).map(Row::Track).collect(),
            None => Vec::new(),
        },
        // The list of channels. "Shuffle all" leads it, and only when there is more than one
        // channel to shuffle across — with a single channel it would be the same button twice.
        None => {
            if lib.channels.is_empty() {
                return Vec::new();
            }
            let mut out = Vec::with_capacity(lib.channels.len() + 1);
            if lib.channels.len() > 1 {
                out.push(Row::All);
            }
            out.extend((0..lib.channels.len()).map(Row::Channel));
            out
        }
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
        None => TOP,
    }
}

pub fn content_h(lib: &Library, channel: Option<usize>) -> i32 {
    rows(lib, channel).len() as i32 * ROW_H
}

pub fn max_scroll_px(lib: &Library, channel: Option<usize>) -> i32 {
    (content_h(lib, channel) - (LIST_BOTTOM - list_top(channel))).max(0)
}

/// Screen-y of visual row `r` at `scroll`.
pub fn row_top(r: usize, channel: Option<usize>, scroll: i32) -> i32 {
    list_top(channel) + r as i32 * ROW_H - scroll
}

/// Which row is under `y`? `None` above or below the list, or past the last row.
pub fn row_at(lib: &Library, channel: Option<usize>, y: i32, scroll: i32) -> Option<Row> {
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
    text::draw(c, f, (px + 52) as f32, (cy - 3) as f32, "Play channel", &lst);
    let cl = count_label(tracks);
    text::draw(c, f, (px + 52) as f32, (cy + 15) as f32, &cl, &sst);

    icons::shuffle(c, (sx + 28) as f32, cy as f32, 18.0, t.acc_ink);
    text::draw(c, f, (sx + 48) as f32, (cy + 6) as f32, "Shuffle", &lst);
}

fn name_style(t: &Theme, strong: bool) -> TextStyle {
    sty(Family::Sans, if strong { Weight::SemiBold } else { Weight::Regular }, 18.0, t.ink, 0.0)
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

#[allow(clippy::too_many_arguments)]
pub fn render(
    c: &mut Canvas,
    t: &Theme,
    f: &FontSet,
    lib: &Library,
    channel: Option<usize>,
    scroll_px: i32,
    sbar_active: bool,
) {
    let top = list_top(channel);
    let scroll = scroll_px.clamp(0, max_scroll_px(lib, channel));
    c.fill(t.bg);
    let sub = subtitle(lib, channel);
    let y0 = crate::chrome::header(c, t, f, title(lib, channel), (!sub.is_empty()).then_some(&sub));

    let all = rows(lib, channel);
    if channel.is_some() {
        let n = channel.and_then(|i| lib.channels.get(i)).map_or(0, |ch| ch.tracks.len());
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
            Row::All => {
                icons::shuffle(c, 34.0, cy as f32, 19.0, t.acc);
                let ns = name_style(t, true);
                text::draw(c, f, 62.0, (cy + 6) as f32, "Shuffle all analysed", &ns);
                let cs = sty(Family::Mono, Weight::Regular, 11.0, t.faint, 0.12);
                let cl = count_label(lib.sensme_tracks as usize);
                let w = text::measure(f, &cl, &cs);
                text::draw(c, f, 440.0 - w, (cy + 5) as f32, &cl, &cs);
                icons::chevron(c, 452.0, cy as f32, 9.0, t.faint);
            }
            Row::Channel(i) => {
                let Some(ch) = lib.channels.get(*i) else { continue };
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
                text::draw(c, f, 34.0, (cy + 1) as f32,
                           &crate::widgets::fit(f, &s.title, &ns, 320.0), &ns);
                let ss = sty(Family::Sans, Weight::Regular, 13.0, t.dim, 0.0);
                text::draw(c, f, 34.0, (cy + 19) as f32,
                           &crate::widgets::fit(f, &s.artist, &ss, 320.0), &ss);
                let ds = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.06);
                let w = text::measure(f, &s.dur, &ds);
                text::draw(c, f, 458.0 - w, (cy + 5) as f32, &s.dur, &ds);
            }
        }
        hline(c, y + ROW_H - 1, t.line);
    }
    c.clear_clip();
    scrollbar(c, t, top, LIST_BOTTOM, scroll, content_h(lib, channel), sbar_active);
    let _ = W;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChannelRow, SongRow};

    fn lib() -> Library {
        let song = |n: &str| SongRow { title: n.into(), dur: "3:00".into(), ..Default::default() };
        Library {
            songs: vec![song("a"), song("b"), song("c")],
            channels: vec![
                ChannelRow { id: 0, name: "Active", tracks: vec![0, 2] },
                ChannelRow { id: 8, name: "Morning", tracks: vec![1] },
            ],
            sensme_tracks: 3,
            ..Default::default()
        }
    }

    /// Every row the renderer draws must be the row the hit test finds under it, at any scroll.
    /// Both levels, because they have different list tops — the channel page gives its first 80 px
    /// to the band, and a hit test that forgot would play the wrong track by one row per screen.
    #[test]
    fn the_hit_test_agrees_with_the_rows_at_every_scroll() {
        let l = lib();
        for channel in [None, Some(0)] {
            let all = rows(&l, channel);
            assert!(!all.is_empty());
            for (r, want) in all.iter().enumerate() {
                for scroll in [0, 9, ROW_H, ROW_H * 2 - 1] {
                    let y = row_top(r, channel, scroll) + ROW_H / 2;
                    if (list_top(channel)..LIST_BOTTOM).contains(&y) {
                        assert_eq!(row_at(&l, channel, y, scroll), Some(*want),
                                   "row {r} at scroll {scroll}, channel {channel:?}");
                    }
                }
            }
            // Past the last row is nothing, not the last row again.
            let past = row_top(all.len(), channel, 0) + ROW_H / 2;
            if past < LIST_BOTTOM {
                assert_eq!(row_at(&l, channel, past, 0), None);
            }
        }
    }

    /// The channel list leads with "shuffle all", and a channel opens onto its members only.
    #[test]
    fn the_two_levels_have_the_rows_they_should() {
        let l = lib();
        assert_eq!(rows(&l, None), vec![Row::All, Row::Channel(0), Row::Channel(1)]);
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

    /// With one channel there is nothing for "shuffle all" to shuffle ACROSS, so the row that would
    /// duplicate the channel's own band is not drawn.
    #[test]
    fn one_channel_does_not_get_a_shuffle_all_row() {
        let mut l = lib();
        l.channels.truncate(1);
        assert_eq!(rows(&l, None), vec![Row::Channel(0)]);
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
