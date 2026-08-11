//! Up Next — the play queue. Now data-driven: it shows the **current album** (the album the
//! now-playing track belongs to, resolved from the library), highlighting the playing row and the
//! tracks that follow. Windowed like the library lists so long albums scroll. When nothing is
//! playing / the track isn't in the library, a clean empty state is shown (no fake data).

use crate::canvas::W;
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use crate::Canvas;

pub const RH: i32 = 62;
// Must equal `library::list_bottom()`: the scrollbar this screen draws is `library::scrollbar`,
// and `sbar_begin` measures the thumb's travel against the LIBRARY's bottom. Two independent
// literals happened to agree (800 - 64); deriving it means they cannot quietly stop agreeing.
const LIST_BOTTOM: i32 = crate::H as i32 - crate::chrome::NP_BAR_H;
const LIST_TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The reorder grab handle's hit strip on a user-queue row. Wide, because this device has no d-pad
/// and reordering is a thumb-only gesture, but it STOPS short of the right edge: the last
/// `library::SBAR_GRAB_W` px belong to the scrollbar drag, and one strip cannot serve both. A
/// vertical drag starting here reorders; anywhere else it scrolls, which is the same start-point
/// ownership rule the scrub rail uses.
pub const GRIP_X0: i32 = 424;
pub const GRIP_X1: i32 = W as i32 - crate::library::SBAR_GRAB_W;

/// The "clear the queue" chip in the header: `(x, y, w, h)`. An explicit, labelled control rather
/// than a gesture, because emptying the queue is the one action here that cannot be undone.
pub const CLEAR_CHIP: (i32, i32, i32, i32) = (388, 48, 70, 28);

pub fn hit_clear_chip(x: i32, y: i32) -> bool {
    let (cx, cy, cw, ch) = CLEAR_CHIP;
    (cx..cx + cw).contains(&x) && (cy..cy + ch).contains(&y)
}

/// A queue row being dragged to a new position.
///
/// `y`/`grab_off` are in SCREEN space, not content space, so the floating row keeps sitting under
/// the finger while the list auto-scrolls beneath it — deriving the float from `from * RH` instead
/// would make it slide away from the thumb the moment the edge-scroll kicked in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct QueueDrag {
    /// Index in the queue the finger picked up.
    pub from: usize,
    /// Index it would land on if released now. The other rows part to show this slot.
    pub to: usize,
    /// Finger y when the row was picked up. Kept so the shell can report TOTAL travel from the
    /// gesture's start — the same thing it measures for a swipe — instead of per-event deltas,
    /// which drift whenever the driver coalesces events.
    pub start_y: i32,
    /// Current finger y, UI screen coords.
    pub y: i32,
    /// Where inside the row the finger grabbed it, so the row doesn't jump on pick-up.
    pub grab_off: i32,
}

impl QueueDrag {
    /// Top of the floating row, in screen coords.
    pub fn float_top(&self) -> i32 {
        self.y - self.grab_off
    }
}

/// Height of the user queue's scrolling window.
pub fn queue_view_h() -> i32 {
    LIST_BOTTOM - LIST_TOP
}

/// Is this x on the grab handle?
pub fn queue_grip_hit(x: i32) -> bool {
    (GRIP_X0..GRIP_X1).contains(&x)
}

/// The queue in the order it is currently DRAWN: `from` lifted out and re-inserted at `to`.
fn drag_order(len: usize, drag: Option<QueueDrag>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if let Some(d) = drag {
        if d.from < len && d.to < len {
            let it = order.remove(d.from);
            order.insert(d.to, it);
        }
    }
    order
}

// ── The unified queue layout ────────────────────────────────────────────────────────────────────
// Up Next used to be TWO mutually exclusive screens: the current album auto-scrolled to the playing
// track, OR — the moment you swipe-queued a single song — the user queue on its own, with no
// now-playing row anywhere on it. Queueing one track therefore hid what was playing, and the queue
// itself never followed playback.
//
// One list now, in Apple Music's order:
//
//     PREVIOUSLY PLAYED     album tracks before the current one
//     NOW PLAYING           the current track
//     NEXT IN QUEUE         the user's own swipe-queued picks (reorderable, removable)
//     NEXT FROM <ALBUM>     the rest of the album
//
// Sections with nothing in them are omitted, headers and all. Everything below is driven from
// `layout()`, so the renderer, the tap, the reorder drag and the swipe all read the same geometry
// — the rule this file already followed for the album window and lost for the user queue.

/// Height of a section heading row.
pub const HDR_H: i32 = 34;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    History,
    Now,
    Queue,
    Album,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// A section heading. Never tappable.
    Head(Section),
    /// An album track BEFORE the playing one — index into the album's track list.
    History(usize),
    /// The playing track — index into the album's track list.
    Current(usize),
    /// A user-queued track — index into the USER QUEUE. The only kind that reorders or removes.
    Queued(usize),
    /// An album track after the playing one — index into the album's track list.
    Upcoming(usize),
}

impl Slot {
    pub fn h(&self) -> i32 {
        match self {
            Slot::Head(_) => HDR_H,
            _ => RH,
        }
    }
    /// Is this a row a finger can act on?
    pub fn is_row(&self) -> bool {
        !matches!(self, Slot::Head(_))
    }
}

/// The whole screen as a list of slots with their content-space tops.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub slots: Vec<(Slot, i32)>, // (slot, top in CONTENT space)
    pub content_h: i32,
    /// Content-space top of the NOW PLAYING row, if there is one. This is what the auto-follow
    /// scrolls to.
    pub current_top: Option<i32>,
}

/// Build the slot list. `album_len`/`current` describe the album the playing track belongs to
/// (`current == None` when nothing is playing or the track isn't in the library); `queued` is the
/// user queue's length.
pub fn layout(album_len: usize, current: Option<usize>, queued: usize) -> Layout {
    let mut l = Layout::default();
    let mut y = 0;
    let push = |l: &mut Layout, s: Slot, y: &mut i32| {
        l.slots.push((s, *y));
        *y += s.h();
    };
    if let Some(cur) = current {
        if cur > 0 {
            push(&mut l, Slot::Head(Section::History), &mut y);
            for i in 0..cur {
                push(&mut l, Slot::History(i), &mut y);
            }
        }
        push(&mut l, Slot::Head(Section::Now), &mut y);
        l.current_top = Some(y);
        push(&mut l, Slot::Current(cur), &mut y);
    }
    if queued > 0 {
        push(&mut l, Slot::Head(Section::Queue), &mut y);
        for i in 0..queued {
            push(&mut l, Slot::Queued(i), &mut y);
        }
    }
    if let Some(cur) = current {
        if cur + 1 < album_len {
            push(&mut l, Slot::Head(Section::Album), &mut y);
            for i in cur + 1..album_len {
                push(&mut l, Slot::Upcoming(i), &mut y);
            }
        }
    }
    l.content_h = y;
    l
}

impl Layout {
    /// The slot under screen-`y` at this scroll offset.
    pub fn at(&self, y: i32, scroll_px: i32) -> Option<Slot> {
        if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
            return None;
        }
        let cy = y - LIST_TOP + scroll_px.max(0);
        self.slots
            .iter()
            .find(|(s, top)| cy >= *top && cy < *top + s.h())
            .map(|(s, _)| *s)
    }
    /// Content-space top of a slot, for placing a lifted row.
    pub fn top_of(&self, want: Slot) -> Option<i32> {
        self.slots.iter().find(|(s, _)| *s == want).map(|(_, t)| *t)
    }
    pub fn max_scroll_px(&self) -> i32 {
        (self.content_h - queue_view_h()).max(0)
    }
    /// Scroll that puts NOW PLAYING a third of the way down the window — the Apple Music resting
    /// position, which keeps a couple of played tracks visible above it instead of pinning it to
    /// the top with the history off-screen.
    pub fn follow_scroll(&self) -> i32 {
        match self.current_top {
            Some(top) => (top - queue_view_h() / 3).clamp(0, self.max_scroll_px()),
            None => 0,
        }
    }
    /// Queue indices in DRAWN order while `drag` is lifted (see `drag_order`).
    pub fn queued_len(&self) -> usize {
        self.slots.iter().filter(|(s, _)| matches!(s, Slot::Queued(_))).count()
    }
    /// Content-space top of the first user-queue row, if the queue section exists.
    pub fn queue_top(&self) -> Option<i32> {
        self.slots
            .iter()
            .find(|(s, _)| matches!(s, Slot::Queued(0)))
            .map(|(_, t)| *t)
    }
    /// Which queue index a floating row is over, from its top edge in screen coords. Same
    /// half-row rule as before, but measured from the queue SECTION's top rather than the
    /// window's, because the queue no longer starts at row 0 of the screen.
    pub fn queue_slot_for(&self, float_top: i32, scroll_px: i32) -> usize {
        let len = self.queued_len();
        if len == 0 {
            return 0;
        }
        let base = self.queue_top().unwrap_or(0);
        let centre = float_top - LIST_TOP + scroll_px - base + RH / 2;
        (centre.div_euclid(RH)).clamp(0, len as i32 - 1) as usize
    }
}

// ── The unified renderer ────────────────────────────────────────────────────────────────────────

fn section_label(sec: Section, album: &str) -> String {
    match sec {
        Section::History => "PREVIOUSLY PLAYED".into(),
        Section::Now => "NOW PLAYING".into(),
        Section::Queue => "NEXT IN QUEUE".into(),
        Section::Album => {
            if album.is_empty() {
                "NEXT UP".into()
            } else {
                format!("NEXT FROM {}", album.to_uppercase())
            }
        }
    }
}

/// Everything the unified screen needs to draw itself. Grouped into a struct because the row
/// renderer wants most of it and a nine-argument function is how the two halves drift apart.
pub struct QueueView<'a> {
    pub album: &'a str,
    /// The album the playing track belongs to, in play order. Empty when nothing is playing.
    pub tracks: &'a [SongRow],
    /// Index of the playing track within `tracks`.
    pub current: Option<usize>,
    /// The user's own swipe-queued picks.
    pub queue: &'a [SongRow],
    pub lib: &'a crate::model::Library,
    pub scroll_px: i32,
    pub drag: Option<QueueDrag>,
    pub swipe: Option<crate::library::SwipeRow>,
    pub sbar_active: bool,
}

/// Draw the whole Up Next screen. Returns the layout it drew, so `nav` can hit-test against
/// exactly what is on the glass rather than rebuilding it and hoping the two agree.
pub fn render_view(c: &mut Canvas, t: &Theme, f: &FontSet, v: &QueueView) -> Layout {
    c.fill(t.bg);
    let l = layout(v.tracks.len(), v.current, v.queue.len());

    if l.slots.is_empty() {
        let _ = crate::chrome::header(c, t, f, "Up Next", None);
        text::draw(c, f, 22.0, 360.0, "Nothing queued.",
                   &sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0));
        text::draw(c, f, 22.0, 386.0, "Play a track and its album appears here.",
                   &sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
        return l;
    }

    let y0 = crate::chrome::header(c, t, f, "Up Next", None);
    // The CLEAR chip belongs to the user queue, so it only appears when there is one to clear.
    if !v.queue.is_empty() {
        let (chx, chy, chw, chh) = CLEAR_CHIP;
        let cap = if v.drag.is_some() {
            String::from("DRAG TO REORDER")
        } else {
            format!("{} QUEUED", v.queue.len())
        };
        right(c, f, (chx - 12) as f32, 65.0, &cap,
              &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));
        crate::widgets::stroke_rect(c, chx, chy, chw, chh, t.line, 1);
        crate::widgets::center(c, f, (chx + chw / 2) as f32, (chy + chh / 2 + 4) as f32, "CLEAR",
                               &sty(Family::Mono, Weight::Bold, 11.0, t.dim, 0.14));
    } else if !v.tracks.is_empty() {
        right(c, f, 458.0, 65.0, &format!("{} TRACKS", v.tracks.len()),
              &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));
    }

    let scroll = v.scroll_px.clamp(0, l.max_scroll_px());
    // Queue rows are drawn in their would-be order while a row is lifted; every other kind keeps
    // its place, so the reorder only permutes the section it belongs to.
    let qorder = drag_order(v.queue.len(), v.drag);
    let mut qseen = 0usize;

    c.set_clip_y(y0, LIST_BOTTOM);
    for (slot, top) in &l.slots {
        let y = y0 + top - scroll;
        let is_q = matches!(slot, Slot::Queued(_));
        if y + slot.h() <= y0 {
            if is_q {
                qseen += 1;
            }
            continue;
        }
        if y >= LIST_BOTTOM {
            break;
        }
        match *slot {
            Slot::Head(sec) => {
                let col = if sec == Section::Now { t.acc } else { t.faint };
                let hs = sty(Family::Mono, Weight::Regular, 11.0, col, 0.18);
                let lbl = crate::widgets::fit(f, &section_label(sec, v.album), &hs, (W as f32) - 44.0);
                text::draw(c, f, 22.0, (y + HDR_H - 11) as f32, &lbl, &hs);
                hline(c, y + HDR_H - 1, t.line);
            }
            Slot::History(i) | Slot::Upcoming(i) => {
                if let Some(song) = v.tracks.get(i) {
                    // History is dimmed — it is context, not a destination, and Apple Music reads
                    // the same way. Still tappable: that is how you go back a track.
                    let past = matches!(*slot, Slot::History(_));
                    album_row(c, t, f, song, v.lib, y, i + 1, past, false);
                }
            }
            Slot::Current(i) => {
                if let Some(song) = v.tracks.get(i) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel);
                    fill_rect(c, 0, y, 4, RH, t.acc);
                    album_row(c, t, f, song, v.lib, y, i + 1, false, true);
                }
            }
            Slot::Queued(_) => {
                let qi = qorder.get(qseen).copied().unwrap_or(0);
                qseen += 1;
                if v.drag.map(|d| d.from) == Some(qi) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel); // the well the row came out of
                } else if let Some(song) = v.queue.get(qi) {
                    let sw = v
                        .swipe
                        .filter(|s| (y..y + RH).contains(&s.y) && s.dx != 0)
                        .map(|s| s.dx);
                    if let Some(dx) = sw {
                        crate::library::swipe_reveal(c, t, f, y, RH, dx,
                                                     crate::library::SwipeIntent::Remove);
                    }
                    queue_row(c, t, f, song, v.lib, y, qseen);
                    if sw.is_some() {
                        c.clear_offset_x();
                    }
                }
            }
        }
        if slot.is_row() {
            hline(c, y + slot.h(), t.line);
        }
    }
    c.clear_clip();

    // The lifted row, last so it sits over everything and clipped so an over-drag can't smear
    // across the header.
    if let Some(d) = v.drag {
        if let Some(song) = v.queue.get(d.from) {
            let ft = d.float_top().clamp(y0 - RH / 2, LIST_BOTTOM - RH / 2);
            c.set_clip_y(y0, LIST_BOTTOM);
            fill_rect(c, 0, ft, W as i32, RH, t.row_sel);
            fill_rect(c, 0, ft, 4, RH, t.acc);
            hline(c, ft, t.line);
            hline(c, ft + RH, t.line);
            queue_row(c, t, f, song, v.lib, ft, d.to + 1);
            grip(c, t, ft, true);
            c.clear_clip();
        }
    }

    if l.max_scroll_px() > 0 {
        crate::library::scrollbar(c, t, y0, scroll, l.content_h, v.sbar_active);
    }
    l
}

/// An album-side row (history, current or upcoming). `past` dims it; `now` marks it playing.
fn album_row(c: &mut Canvas, t: &Theme, f: &FontSet, song: &SongRow,
             lib: &crate::model::Library, y: i32, n: usize, past: bool, now: bool) {
    let cy = (y + RH / 2) as f32;
    let idx_col = if now { t.acc } else { t.faint };
    let idx = if now { "\u{25b6}".to_string() } else { format!("{n:02}") };
    text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 12.0, idx_col, 0.0));
    // Played rows fade their art too, so the eye finds the current row without reading a word.
    let dim = if past { 0.34 } else if t.night { 0.30 } else { 1.0 };
    crate::library::thumb(c, t, lib, song.album_id, &song.art, 46, y + (RH - 48) / 2, 48, dim);
    let title_col = if now { t.acc } else if past { t.dim } else { t.ink };
    let tst = sty(Family::Sans, Weight::SemiBold, 20.0, title_col, 0.0);
    text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, 306.0), &tst);
    let ast = sty(Family::Sans, Weight::Regular, 15.0, if past { t.faint } else { t.dim }, 0.0);
    text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, 320.0), &ast);
    right(c, f, 458.0, cy + 4.0, &song.dur,
          &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
}

/// One user-queue row's content at screen-`y`. `n` is the position label (1-based).
fn queue_row(c: &mut Canvas, t: &Theme, f: &FontSet, song: &SongRow, lib: &crate::model::Library,
             y: i32, n: usize) {
    let cy = (y + RH / 2) as f32;
    text::draw(c, f, 22.0, cy + 4.0, &format!("{n:02}"),
        &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
    // Real cover here too — the user queue had the same gradient-only problem as the album
    // window above it.
    crate::library::thumb(c, t, lib, song.album_id, &song.art,
                          46, y + (RH - 48) / 2, 48, if t.night { 0.30 } else { 1.0 });
    let tst = sty(Family::Sans, Weight::SemiBold, 20.0, t.ink, 0.0);
    text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, 262.0), &tst);
    let ast = sty(Family::Sans, Weight::Regular, 15.0, t.dim, 0.0);
    text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, 276.0), &ast);
    // Duration moved in from 458 to clear the grab handle.
    right(c, f, 410.0, cy + 4.0, &song.dur, &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
    grip(c, t, y, false);
}

/// The reorder grab handle: three stacked bars, the universal "drag me" mark. Accent while the row
/// is lifted so the gesture reads as engaged even though the finger covers the icon.
fn grip(c: &mut Canvas, t: &Theme, y: i32, lifted: bool) {
    let col = if lifted { t.acc } else { t.faint };
    let cy = y + RH / 2;
    let w = GRIP_X1 - GRIP_X0 - 8;
    for k in -1..=1 {
        fill_rect(c, GRIP_X0 + 4, cy + k * 7 - 1, w, 2, col);
    }
}
