//! Up Next — ONE list: what has played, what is playing, and everything that will play after it,
//! in the order it will play. Adding a track or an album puts it in that list (straight after the
//! playing track, or at the very end); nothing else decides the order. Windowed like the library
//! lists so a whole-library shuffle scrolls. When nothing is playing a clean empty state is shown.

use crate::canvas::W;
use crate::model::SongRow;
use crate::text::{self, Family, FontSet, Weight};
use crate::theme::Theme;
use crate::widgets::{fill_rect, hline, right, sty};
use crate::Canvas;

pub const RH: i32 = crate::scale::TRACK_ROW_H;
// Must equal `library::list_bottom()`: the scrollbar this screen draws is `library::scrollbar`,
// and `sbar_begin` measures the thumb's travel against the LIBRARY's bottom. Two independent
// literals happened to agree (800 - 64); deriving it means they cannot quietly stop agreeing.
const LIST_BOTTOM: i32 = crate::H as i32 - crate::chrome::NP_BAR_H;
const LIST_TOP: i32 = crate::chrome::HEADER_BOTTOM;

/// The reorder grab handle's hit strip on a reorderable row — a queued one or an upcoming one.
/// Wide, because this device has no d-pad and reordering is a thumb-only gesture, but it STOPS
/// short of the right edge: the last `library::SBAR_GRAB_W` px belong to the scrollbar drag, and
/// one strip cannot serve both. A vertical drag starting here reorders; anywhere else it scrolls,
/// which is the same start-point ownership rule the scrub rail uses.
pub const GRIP_X0: i32 = 424;
pub const GRIP_X1: i32 = W as i32 - crate::library::SBAR_GRAB_W;

/// The "clear Up Next" chip in the header: `(x, y, w, h)`. An explicit, labelled control rather
/// than a gesture, because emptying the list is the one action here that cannot be undone.
pub const CLEAR_CHIP: (i32, i32, i32, i32) = (388, 48, 70, 28);
/// The SHUFFLE chip, immediately left of it. Shuffles every track in the list except the one
/// playing (see `App::queue_shuffle`).
pub const SHUFFLE_CHIP: (i32, i32, i32, i32) = (302, 48, 78, 28);

fn in_rect(r: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    let (rx, ry, rw, rh) = r;
    (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y)
}

pub fn hit_clear_chip(x: i32, y: i32) -> bool {
    in_rect(CLEAR_CHIP, x, y)
}
pub fn hit_shuffle_chip(x: i32, y: i32) -> bool {
    in_rect(SHUFFLE_CHIP, x, y)
}

/// The CLEAR control on the PREVIOUSLY PLAYED heading — the only thing that empties the play
/// history (`App::history_clear`). Deliberately NOT the queue's CLEAR chip: one destroys what you
/// asked to hear next and the other what you already heard, and one button for both would be the
/// overload this screen has had cleaned out of it before.
///
/// On the heading rather than in the header bar, because it belongs to one section and the header
/// bar's chips belong to the screen. That also means it has no fixed y: the heading moves with the
/// scroll, so the hit test asks the LAYOUT where it is rather than carrying a rect that would go
/// stale the moment the list moved.
pub const HIST_CLEAR_X0: i32 = 372;
pub const HIST_CLEAR_X1: i32 = W as i32 - 14;

/// Is `(x, y)` on that control? False whenever the PREVIOUSLY PLAYED heading is not the slot under
/// the finger, so it can never steal a tap from a row that happens to share its column.
pub fn hit_history_clear(l: &Layout, x: i32, y: i32, scroll_px: i32) -> bool {
    (HIST_CLEAR_X0..HIST_CLEAR_X1).contains(&x)
        && matches!(l.at(y, scroll_px), Some(Slot::Head(Section::History)))
}

// ── THE MOVABLE SPAN ────────────────────────────────────────────────────────────────────────────
//
// Every row except the playing one, in the order it is drawn: the played tracks, then everything
// still to come. ONE index space over the two of them.
//
// Up Next used to hold two lists below NOW PLAYING — NEXT IN QUEUE (the user's own picks) and NEXT
// FROM <ALBUM> (the rest of what was started) — and a pick always played before the album resumed.
// So "play this album, then that one" could not be said: queueing the second album put it in front
// of the rest of the first. Merged 2026-09-23 at the owner's request into ONE list in play order.
// Adding a track or album puts it straight after the playing track or at the very end, and a drag
// can move any upcoming row anywhere in it.
//
// What makes a drag safe is unchanged: the span starts strictly AFTER the playing row, so
// `context_idx` can never be disturbed by one, whichever direction the row travels.

/// A row being dragged to a new position in the movable span.
///
/// `y`/`grab_off` are in SCREEN space, not content space, so the floating row keeps sitting under
/// the finger while the list auto-scrolls beneath it — deriving the float from `from * RH` instead
/// would make it slide away from the thumb the moment the edge-scroll kicked in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RowDrag {
    /// Movable-span index the finger picked up.
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

impl RowDrag {
    /// Top of the floating row, in screen coords.
    pub fn float_top(&self) -> i32 {
        self.y - self.grab_off
    }
}

/// Height of Up Next's scrolling window.
pub fn queue_view_h() -> i32 {
    LIST_BOTTOM - LIST_TOP
}

/// Is this x on the grab handle? Says nothing about the ROW: the caller pairs this with the slot
/// under the finger, because the same column is inert on the playing row.
pub fn queue_grip_hit(x: i32) -> bool {
    (GRIP_X0..GRIP_X1).contains(&x)
}

/// The movable span in the order it is currently DRAWN: `from` lifted out and re-inserted at `to`.
///
/// The span's slots stay where they are; it is the CONTENT that flows through them, which is what
/// makes a drag look like one list reflowing. On release `App::movable_move` performs the storage
/// move the preview promised.
fn drag_order(len: usize, drag: Option<RowDrag>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if let Some(d) = drag {
        if d.from < len && d.to < len {
            let it = order.remove(d.from);
            order.insert(d.to, it);
        }
    }
    order
}

// ── The layout ──────────────────────────────────────────────────────────────────────────────────
//
//     PREVIOUSLY PLAYED     what you have actually played, newest last (see `App::history`)
//     NOW PLAYING           the current track
//     NEXT FROM <ALBUM>     everything after it, in play order — "NEXT UP" once the list holds
//                           more than the playing album
//
// Sections with nothing in them are omitted, headers and all. Everything below is driven from
// `layout()`, so the renderer, the tap, the reorder drag and the swipe all read the same geometry.

/// Height of a section heading row.
pub const HDR_H: i32 = 34;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    History,
    Now,
    /// Everything after the playing track.
    Next,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// A section heading. Never tappable.
    Head(Section),
    /// A track that has already played — index into the HISTORY list, not into the sequence.
    /// Oldest first, so the row nearest NOW PLAYING is the one that played most recently.
    History(usize),
    /// The playing track — index into the sequence.
    Current(usize),
    /// A track after the playing one — index into the sequence.
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
    /// Content-space top of the first HISTORY row, if there is a history section. Stored rather
    /// than searched for: the span's pixel arithmetic is per-section, because section headings sit
    /// between them and belong to no row.
    history_top_px: Option<i32>,
    /// The same two numbers for the upcoming rows: content-space top of the first one, and the
    /// SEQUENCE index it holds. The section starts one past the playing track, so the offset is
    /// what converts between a drag's relative index and the absolute one `Slot::Upcoming` carries.
    upcoming_top_px: Option<i32>,
    upcoming_first: usize,
}

/// The layout's SHAPE, without materialising the slot list. Pure arithmetic, O(1) in the length of
/// the sequence.
///
/// `layout()` allocates one entry per track, and after a "Shuffle all songs" that sequence is the
/// entire library — 3,600-odd slots. The render path's auto-follow needs exactly two numbers out of
/// it, so it asks for those instead of building the whole thing to read the top of one row.
///
/// `metrics()` and `layout()` MUST agree; `metrics_matches_layout` sweeps them against each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub current_top: Option<i32>,
    pub content_h: i32,
}

/// `hist` is the length of the shell's play history — an independent list, so it is passed in
/// rather than derived from `current`. `len`/`current` describe the sequence.
pub fn metrics(hist: usize, len: usize, current: Option<usize>) -> Metrics {
    let mut y = 0i32;
    let mut current_top = None;
    if hist > 0 {
        y += HDR_H + hist as i32 * RH; // history header + the played rows
    }
    if let Some(cur) = current {
        y += HDR_H; // NOW PLAYING header
        current_top = Some(y);
        y += RH;
        if cur + 1 < len {
            y += HDR_H + (len - cur - 1) as i32 * RH;
        }
    }
    Metrics { current_top, content_h: y }
}

impl Metrics {
    pub fn max_scroll_px(&self) -> i32 {
        (self.content_h - queue_view_h()).max(0)
    }
    /// Same rule as `Layout::follow_scroll` — kept next to it so the two cannot drift.
    pub fn follow_scroll(&self) -> i32 {
        match self.current_top {
            Some(top) => (top - queue_view_h() / 3).clamp(0, self.max_scroll_px()),
            None => 0,
        }
    }
}

/// Build the slot list. `len`/`current` describe the sequence (`current == None` when nothing is
/// playing); `hist` is the play history's length.
pub fn layout(hist: usize, len: usize, current: Option<usize>) -> Layout {
    let mut l = Layout::default();
    // RESERVE UP FRONT. Every track is a slot, and after a "Shuffle all songs" that is the whole
    // library — growing from empty meant a dozen reallocations and memcpys of a list that ends up
    // ~29 KB, once per painted frame. Three spare for the section headings.
    l.slots = Vec::with_capacity(hist + len + 3);
    let mut y = 0;
    let push = |l: &mut Layout, s: Slot, y: &mut i32| {
        l.slots.push((s, *y));
        *y += s.h();
    };
    if hist > 0 {
        push(&mut l, Slot::Head(Section::History), &mut y);
        l.history_top_px = Some(y);
        for i in 0..hist {
            push(&mut l, Slot::History(i), &mut y);
        }
    }
    if let Some(cur) = current {
        push(&mut l, Slot::Head(Section::Now), &mut y);
        l.current_top = Some(y);
        push(&mut l, Slot::Current(cur), &mut y);
        if cur + 1 < len {
            push(&mut l, Slot::Head(Section::Next), &mut y);
            l.upcoming_top_px = Some(y);
            l.upcoming_first = cur + 1;
            for i in cur + 1..len {
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
        // BINARY SEARCH, not a scan. `slots` is built in ascending `top` order and after a
        // "Shuffle all songs" it holds one entry per track — 3,463 on the reference device. This
        // runs on every tap, on every frame of a reorder drag, and on every swipe classification.
        //
        // `partition_point` gives the first slot starting AFTER cy; the candidate is the one
        // before it, and it only matches if cy is inside that slot's own height (the gap between
        // two sections is not a slot).
        let index = self.slots.partition_point(|(_, top)| *top <= cy);
        let (slot, top) = *self.slots.get(index.checked_sub(1)?)?;
        (cy < top + slot.h()).then_some(slot)
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

    /// How many upcoming rows there are, their content-space top, and the sequence index of the
    /// first one.
    pub fn upcoming_len(&self) -> usize {
        self.slots.iter().filter(|(s, _)| matches!(s, Slot::Upcoming(_))).count()
    }
    pub fn upcoming_top(&self) -> Option<i32> {
        self.upcoming_top_px
    }
    pub fn upcoming_first(&self) -> usize {
        self.upcoming_first
    }

    /// How many rows the history section holds.
    pub fn history_len(&self) -> usize {
        self.slots.iter().filter(|(s, _)| matches!(s, Slot::History(_))).count()
    }
    pub fn history_top(&self) -> Option<i32> {
        self.history_top_px
    }

    /// Length of the movable span: every played track, then every upcoming one. Every row on the
    /// screen except the playing one, in the order they are drawn.
    pub fn movable_len(&self) -> usize {
        self.history_len() + self.upcoming_len()
    }

    /// THE FIRST INDEX A ROW MAY BE DROPPED ON — one past the end of the history.
    ///
    /// **History is a source but never a destination.** You can reach back into what you have
    /// played and pull a track into what is coming; you cannot push a track that has not played
    /// yet into the record of what has. Requested 2026-09-15 in exactly those terms.
    pub fn drop_first(&self) -> usize {
        self.history_len()
    }

    /// What movable index `i` refers to, as a slot. The ONE place that knows where the boundary is.
    pub fn movable_slot(&self, i: usize) -> Option<Slot> {
        let h = self.history_len();
        if i < h {
            Some(Slot::History(i))
        } else if i < h + self.upcoming_len() {
            Some(Slot::Upcoming(self.upcoming_first + (i - h)))
        } else {
            None
        }
    }

    /// The movable index of a slot, or None if it is not a movable row.
    pub fn movable_index(&self, slot: Slot) -> Option<usize> {
        let h = self.history_len();
        match slot {
            Slot::History(i) if i < h => Some(i),
            Slot::Upcoming(i) => {
                let rel = i.checked_sub(self.upcoming_first)?;
                (rel < self.upcoming_len()).then(|| h + rel)
            }
            _ => None,
        }
    }

    /// Which movable index a floating row would be DROPPED on, from its top edge in screen coords.
    ///
    /// NOT a single division. The span is contiguous in INDEX but not in PIXELS: the NOW PLAYING
    /// heading, row and the NEXT heading sit between the two sections, so each section is measured
    /// from its own top. Anything in or above the history pins to the first droppable position —
    /// "play this next" — which is what the parted list shows while the finger is still down.
    ///
    /// `from` matters for one reason: a row lifted OUT of the history leaves it one shorter, so the
    /// first droppable position is one lower too — otherwise a played track could never be dropped
    /// straight after the playing one.
    pub fn movable_slot_for(&self, from: usize, float_top: i32, scroll_px: i32) -> usize {
        let (hlen, ulen) = (self.history_len(), self.upcoming_len());
        let len = hlen + ulen;
        if len == 0 {
            return 0;
        }
        let floor = self.drop_first() - usize::from(from < self.drop_first());
        let clamp = |i: i32| (i.max(floor as i32) as usize).min(len - 1);
        // Content-space centre of the floating row.
        let cy = float_top - LIST_TOP + scroll_px + RH / 2;
        match (self.upcoming_top(), ulen > 0) {
            (Some(ut), true) if cy >= ut => clamp(hlen as i32 + (cy - ut).div_euclid(RH)),
            // Over the history, NOW PLAYING or the NEXT heading: the front of what is coming,
            // which is the floor — one lower for a row that came out of the history.
            (Some(_), true) => clamp(0),
            _ => clamp(len as i32 - 1),
        }
    }

    /// Content-space top of movable index `i` as it is DRAWN — used to place the lifted row and to
    /// keep the float from drifting off the slot it will land in.
    pub fn movable_top(&self, i: usize) -> Option<i32> {
        self.movable_slot(i).and_then(|s| self.top_of(s))
    }
}

// ── The renderer ────────────────────────────────────────────────────────────────────────────────

fn section_label(sec: Section, album: &str) -> String {
    match sec {
        Section::History => "PREVIOUSLY PLAYED".into(),
        Section::Now => "NOW PLAYING".into(),
        Section::Next => {
            if album.is_empty() {
                "NEXT UP".into()
            } else {
                format!("NEXT FROM {}", album.to_uppercase())
            }
        }
    }
}

/// Everything the screen needs to draw itself. Grouped into a struct because the row renderer
/// wants most of it and a nine-argument function is how the two halves drift apart.
pub struct QueueView<'a> {
    /// Names the NEXT heading: the playing album while everything still to come is from it, empty
    /// ("NEXT UP") once the list holds anything else.
    pub album: &'a str,
    /// The whole sequence, in play order. Empty when nothing is playing.
    pub tracks: &'a [SongRow],
    /// Index of the playing track within `tracks`.
    pub current: Option<usize>,
    /// What has already played, oldest first — the shell's real history, not a slice of `tracks`.
    pub history: &'a [SongRow],
    pub lib: &'a crate::model::Library,
    pub scroll_px: i32,
    pub drag: Option<RowDrag>,
    pub swipe: Option<crate::library::SwipeRow>,
    pub sbar_active: bool,
}

/// The header caption: how much is still to come, or what a drag does.
pub fn caption(left: usize, dragging: bool) -> String {
    match (dragging, left) {
        (true, _) => String::from("DRAG TO REORDER"),
        (false, 1) => String::from("1 TRACK LEFT"),
        (false, n) => format!("{n} TRACKS LEFT"),
    }
}

/// Draw the whole Up Next screen. Returns the layout it drew, so `nav` can hit-test against
/// exactly what is on the glass rather than rebuilding it and hoping the two agree.
pub fn render_view(c: &mut Canvas, t: &Theme, f: &FontSet, v: &QueueView) -> Layout {
    c.fill(t.bg);
    let l = layout(v.history.len(), v.tracks.len(), v.current);

    if l.slots.is_empty() {
        let _ = crate::chrome::header(c, t, f, "Up Next", None);
        text::draw(c, f, 22.0, 360.0, "Up Next is empty.",
                   &sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, t.ink, 0.0));
        text::draw(c, f, 22.0, 386.0, "Play or queue a track and it appears here.",
                   &sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
        return l;
    }

    let y0 = crate::chrome::header(c, t, f, "Up Next", None);
    // CLEAR empties what is still to come, so it only appears when something is. SHUFFLE needs
    // two tracks besides the playing one — one other track has only one order.
    let left = l.upcoming_len();
    let can_clear = left > 0;
    let can_shuffle = v.current.is_some() && v.tracks.len() >= 3;
    if can_clear {
        chip(c, t, f, CLEAR_CHIP, "CLEAR");
    }
    if can_shuffle {
        let (sx, sy, sw, sh) = SHUFFLE_CHIP;
        crate::widgets::stroke_rect(c, sx, sy, sw, sh, t.line, 1);
        crate::icons::shuffle(c, (sx + 17) as f32, (sy + sh / 2) as f32, 13.0, t.dim);
        crate::widgets::center(c, f, (sx + 46) as f32, (sy + sh / 2 + 4) as f32, "MIX",
                               &sty(Family::Mono, Weight::Bold, 11.0, t.dim, 0.14));
    }
    // The caption goes between the title and whichever chip is furthest left, so it can never run
    // under either. At a large UI scale the long form does not fit — measured on the device at the
    // owner's scale, "7 TRACKS LEFT" ran into the title — so it falls back to "7 LEFT", and to
    // nothing rather than an overlap.
    let cap_right = if can_shuffle { SHUFFLE_CHIP.0 } else if can_clear { CLEAR_CHIP.0 } else { 458 };
    if v.current.is_some() {
        let cs = sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1);
        let room = (cap_right - 12) as f32 - (crate::chrome::header_title_end(f, "Up Next") + 12.0);
        let long = caption(left, v.drag.is_some());
        let short = if v.drag.is_some() { String::from("DRAG") } else { format!("{left} LEFT") };
        if let Some(cap) = [long, short].into_iter().find(|s| text::measure(f, s, &cs) <= room) {
            right(c, f, (cap_right - 12) as f32, 65.0, &cap, &cs);
        }
    }

    let scroll = v.scroll_px.clamp(0, l.max_scroll_px());
    // ONE order over the whole movable span, so a lifted row parts the rows below it whichever
    // section they belong to. The slots stay put; the CONTENT flows through them.
    let hlen = v.history.len();
    let ufirst = l.upcoming_first();
    let morder = drag_order(l.movable_len(), v.drag);
    // One place that turns a span index into the row it names, so the draw loop, the float and the
    // well cannot disagree about what index 7 is. `true` = a played row.
    let span_row = |i: usize| -> Option<(&SongRow, bool)> {
        if i < hlen {
            v.history.get(i).map(|r| (r, true))
        } else {
            v.tracks.get(ufirst + i - hlen).map(|r| (r, false))
        }
    };

    // START AT THE FIRST VISIBLE SLOT: `slots` is in ascending `top` order, so it is a binary
    // search rather than a walk past everything above the window (3,600 slots after a "Shuffle
    // all songs"). A slot is above the window when `top + h <= scroll`.
    let first = l.slots.partition_point(|(s, top)| top + s.h() <= scroll);
    // How many MOVABLE rows are scrolled off the top, because `morder` is indexed by drawn
    // position — the same arithmetic the binary search replaced, per section.
    let mut mseen = match l.history_top() {
        Some(ht) => (((scroll - ht).max(0) / RH) as usize).min(hlen),
        None => 0,
    } + match l.upcoming_top() {
        Some(ut) => (((scroll - ut).max(0) / RH) as usize).min(left),
        None => 0,
    };

    c.set_clip_y(y0, LIST_BOTTOM);
    for (slot, top) in &l.slots[first..] {
        let y = y0 + top - scroll;
        if y >= LIST_BOTTOM {
            break;
        }
        match *slot {
            Slot::Head(sec) => {
                let col = if sec == Section::Now { t.acc } else { t.faint };
                let hs = sty(Family::Mono, Weight::Regular, 11.0, col, 0.18);
                // The label's budget stops short of the CLEAR control on the one heading that has
                // one, so a long album name can never run underneath it.
                let budget = if sec == Section::History {
                    (HIST_CLEAR_X0 - 32) as f32
                } else {
                    (W as f32) - 44.0
                };
                let lbl = crate::widgets::fit(f, &section_label(sec, v.album), &hs, budget);
                text::draw(c, f, 22.0, (y + HDR_H - 11) as f32, &lbl, &hs);
                if sec == Section::History {
                    right(c, f, HIST_CLEAR_X1 as f32, (y + HDR_H - 11) as f32, "CLEAR",
                          &sty(Family::Mono, Weight::Bold, 11.0, t.dim, 0.14));
                }
                hline(c, y + HDR_H - 1, t.line);
            }

            Slot::Current(i) => {
                if let Some(song) = v.tracks.get(i) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel);
                    fill_rect(c, 0, y, 4, RH, t.acc);
                    track_row(c, t, f, song, v.lib, y, 0, false, true, false);
                }
            }
            // Both movable kinds are drawn by one arm, because under a drag the content in an
            // upcoming slot can be a played track and vice versa — that reflow IS the preview of
            // where the row will land. What a row is drawn AS follows the CONTENT; its NUMBER
            // follows the SLOT, so the positions read 01, 02, 03 while the rows flow through them.
            Slot::History(_) | Slot::Upcoming(_) => {
                let mi = morder.get(mseen).copied().unwrap_or(0);
                mseen += 1;
                if v.drag.map(|d| d.from) == Some(mi) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel); // the well the row came out of
                } else if let Some((song, past)) = span_row(mi) {
                    let n = match *slot {
                        Slot::Upcoming(i) => i - ufirst + 1,
                        _ => 0,
                    };
                    if past && n == 0 {
                        // History is dimmed — it is what is behind you. Still tappable (that is how
                        // you go back a track) and GRIPPY: a played track can be dragged back into
                        // what is coming. No number: its position is in the past.
                        track_row(c, t, f, song, v.lib, y, 0, true, false, true);
                    } else {
                        let sw = v
                            .swipe
                            .filter(|s| (y..y + RH).contains(&s.y) && s.dx != 0)
                            .map(|s| s.dx);
                        if let Some(dx) = sw {
                            crate::library::swipe_reveal(c, t, f, y, RH, dx,
                                                         crate::library::SwipeIntent::Remove);
                        }
                        track_row(c, t, f, song, v.lib, y, n, false, false, true);
                        if sw.is_some() {
                            c.clear_offset_x();
                        }
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
    // across the header. Drawn at the position it would LAND on, and never dimmed: a row lifted
    // out of the history is on its way back into what is coming.
    if let Some(d) = v.drag {
        if let Some((song, _)) = span_row(d.from) {
            let ft = d.float_top().clamp(y0 - RH / 2, LIST_BOTTOM - RH / 2);
            c.set_clip_y(y0, LIST_BOTTOM);
            fill_rect(c, 0, ft, W as i32, RH, t.row_sel);
            fill_rect(c, 0, ft, 4, RH, t.acc);
            hline(c, ft, t.line);
            hline(c, ft + RH, t.line);
            track_row(c, t, f, song, v.lib, ft, d.to.saturating_sub(hlen) + 1, false, false, true);
            grip(c, t, ft, true);
            c.clear_clip();
        }
    }

    if l.max_scroll_px() > 0 {
        crate::library::scrollbar(c, t, y0, LIST_BOTTOM, scroll, l.content_h, v.sbar_active);
    }
    l
}

/// A labelled header chip.
fn chip(c: &mut Canvas, t: &Theme, f: &FontSet, r: (i32, i32, i32, i32), label: &str) {
    let (x, y, w, h) = r;
    crate::widgets::stroke_rect(c, x, y, w, h, t.line, 1);
    crate::widgets::center(c, f, (x + w / 2) as f32, (y + h / 2 + 4) as f32, label,
                           &sty(Family::Mono, Weight::Bold, 11.0, t.dim, 0.14));
}

/// One row of the list: a played track, the playing one, or one still to come. `past` dims it;
/// `now` marks it playing; `n` is its position among the upcoming rows (0 = no number).
///
/// `grippy` draws the reorder handle and gives up the width it needs. Every row that moves asks for
/// it; the playing row does not, because a handle there would be a control that does nothing.
fn track_row(c: &mut Canvas, t: &Theme, f: &FontSet, song: &SongRow,
             lib: &crate::model::Library, y: i32, n: usize, past: bool, now: bool, grippy: bool) {
    let cy = (y + RH / 2) as f32;
    let idx_col = if now { t.acc } else { t.faint };
    let idx = if now { Some("\u{25b6}".to_string()) } else if n > 0 { Some(format!("{n:02}")) } else { None };
    if let Some(idx) = idx {
        text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 12.0, idx_col, 0.0));
    }
    // Played rows fade their art too, so the eye finds the current row without reading a word.
    let dim = if past { 0.34 } else if t.night { 0.30 } else { 1.0 };
    crate::library::thumb(c, t, lib, song.album_id, &song.art, 46, y + (RH - 48) / 2, 48, dim);
    let title_col = if now { t.acc } else if past { t.dim } else { t.ink };
    let tst = sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, title_col, 0.0);
    // The text budget and the duration column both shift in when the handle is there.
    let (tw, aw, dx) = if grippy { (262.0, 276.0, 410.0) } else { (306.0, 320.0, 458.0) };
    text::draw(c, f, 100.0, cy - 2.0, &crate::widgets::fit(f, &song.title, &tst, tw), &tst);
    let ast = sty(Family::Sans, Weight::Regular, 15.0, if past { t.faint } else { t.dim }, 0.0);
    text::draw(c, f, 100.0, cy + 16.0, &crate::widgets::fit(f, &song.artist, &ast, aw), &ast);
    right(c, f, dx, cy + 4.0, &song.dur,
          &sty(Family::Mono, Weight::Regular, 13.0, t.faint, 0.0));
    if grippy {
        grip(c, t, y, false);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The binary search must answer exactly what the linear scan it replaced answered — for every
    /// pixel of a layout that has history, a current row and an upcoming tail, including the gaps
    /// between sections where the answer is None.
    #[test]
    fn at_matches_a_linear_scan_everywhere() {
        for (hist, len, cur) in [
            (100usize, 200usize, Some(100usize)),
            (0, 5, Some(0)),
            (39, 40, Some(39)),
            (0, 0, None),
            // A history that owes nothing to the sequence: played tracks with nothing playing, and
            // a sequence with nothing played before it.
            (12, 0, None),
            (0, 8, Some(4)),
        ] {
            let l = layout(hist, len, cur);
            for scroll in [0, 37, 500, l.max_scroll_px()] {
                for y in LIST_TOP - 2..LIST_BOTTOM + 2 {
                    let want = if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
                        None
                    } else {
                        let cy = y - LIST_TOP + scroll.max(0);
                        l.slots.iter().find(|(s, top)| cy >= *top && cy < *top + s.h()).map(|(s, _)| *s)
                    };
                    assert_eq!(l.at(y, scroll), want, "y={y} scroll={scroll} hist={hist} len={len} cur={cur:?}");
                }
            }
        }
    }

    #[test]
    fn metrics_matches_layout() {
        for hist in [0usize, 1, 7] {
            for len in [0usize, 1, 2, 5, 40] {
                // `None`, plus every valid current index.
                let currents: Vec<Option<usize>> =
                    std::iter::once(None).chain((0..len).map(Some)).collect();
                for cur in currents {
                    let l = layout(hist, len, cur);
                    let m = metrics(hist, len, cur);
                    assert_eq!(m.content_h, l.content_h, "content_h at hist={hist} len={len} cur={cur:?}");
                    assert_eq!(m.current_top, l.current_top, "current_top at hist={hist} len={len} cur={cur:?}");
                    assert_eq!(m.follow_scroll(), l.follow_scroll());
                    assert_eq!(m.max_scroll_px(), l.max_scroll_px());
                    // The slot list is in ascending top order and its heights add up to content_h —
                    // the two invariants the binary searches in `at`, the draw loop and `mseen` rest on.
                    let mut y = 0;
                    for (slot, top) in &l.slots {
                        assert_eq!(*top, y, "slots are not contiguous at {slot:?}");
                        y += slot.h();
                    }
                    assert_eq!(y, l.content_h);
                    // The movable span and the slot list agree in both directions.
                    let upcoming = cur.map_or(0, |c| len - c - 1);
                    assert_eq!(l.movable_len(), hist + upcoming);
                    assert_eq!(l.drop_first(), hist, "history is never a destination");
                    for i in 0..l.movable_len() {
                        let slot = l.movable_slot(i).expect("index inside the span has a slot");
                        assert_eq!(l.movable_index(slot), Some(i), "round trip failed at {i}");
                        assert!(l.top_of(slot).is_some(), "{slot:?} is not drawn");
                    }
                    assert_eq!(l.movable_slot(l.movable_len()), None);
                    for (slot, _) in &l.slots {
                        let movable = matches!(slot, Slot::History(_) | Slot::Upcoming(_));
                        assert_eq!(l.movable_index(*slot).is_some(), movable, "{slot:?} disagrees about being movable");
                    }
                }
            }
        }
    }

    /// ONE list below NOW PLAYING: no separate queue section, whatever is in the sequence.
    #[test]
    fn everything_after_the_playing_track_is_one_section() {
        let l = layout(2, 6, Some(1));
        let heads: Vec<Section> = l
            .slots
            .iter()
            .filter_map(|(s, _)| match s {
                Slot::Head(h) => Some(*h),
                _ => None,
            })
            .collect();
        assert_eq!(heads, vec![Section::History, Section::Now, Section::Next]);
        let up: Vec<Slot> = l.slots.iter().map(|(s, _)| *s).filter(|s| matches!(s, Slot::Upcoming(_))).collect();
        assert_eq!(up, (2..6).map(Slot::Upcoming).collect::<Vec<_>>());
        // The last track playing: no NEXT section at all.
        let l = layout(0, 6, Some(5));
        assert!(!l.slots.iter().any(|(s, _)| *s == Slot::Head(Section::Next)));
    }

    /// Every pixel of the movable span resolves to the row DRAWN there — the rule the whole drag
    /// rests on. The trap: NOW PLAYING and the NEXT heading sit between the two sections, so one
    /// division from one origin would report the wrong index past the boundary.
    #[test]
    fn a_float_over_a_row_lands_on_that_row_across_the_section_boundary() {
        for (hist, len, cur) in [(3usize, 20usize, Some(4usize)), (0, 9, Some(0)), (5, 6, Some(2))] {
            let l = layout(hist, len, cur);
            for i in 0..l.movable_len() {
                let slot = l.movable_slot(i).unwrap();
                let top = l.top_of(slot).unwrap();
                let float_top = LIST_TOP + top;
                // A droppable row lands on itself; a history row resolves to the first droppable
                // index, which is one lower when the row came out of the history.
                let floor = l.drop_first() - usize::from(i < l.drop_first());
                let want = i.max(floor);
                assert_eq!(l.movable_slot_for(i, float_top, 0), want,
                           "float over {slot:?} resolved elsewhere (hist={hist} len={len} cur={cur:?})");
            }
            // Above the list pins to the first droppable slot — "play this next".
            assert_eq!(l.movable_slot_for(l.movable_len(), LIST_TOP - 400, 0), l.drop_first());
            if l.drop_first() > 0 {
                assert_eq!(l.movable_slot_for(0, LIST_TOP - 400, 0), l.drop_first() - 1);
            }
        }
    }

    /// The draw loop binary-searches its way to the first visible slot; it must land on exactly
    /// the slot a linear scan would have.
    #[test]
    fn the_first_visible_slot_is_found_by_search_not_by_walking() {
        let l = layout(100, 200, Some(100));
        let max = l.max_scroll_px();
        for scroll in [0, 1, RH - 1, RH, RH + 1, HDR_H, 500, 1234, max / 2, max] {
            let want = l.slots.iter().position(|(s, top)| top + s.h() > scroll).unwrap_or(l.slots.len());
            let got = l.slots.partition_point(|(s, top)| top + s.h() <= scroll);
            assert_eq!(got, want, "first visible slot disagrees at scroll={scroll}");
        }
    }

    /// `mseen` is the number of MOVABLE rows scrolled off the top, and `morder` is indexed by it —
    /// an off-by-one here draws the wrong track in every row below the fold.
    #[test]
    fn the_skipped_movable_row_count_is_computed_not_counted() {
        for hist in [0usize, 4, 9] {
            let l = layout(hist, 60, Some(30));
            let max = l.max_scroll_px();
            for scroll in 0..=max {
                let counted = l
                    .slots
                    .iter()
                    .filter(|(s, top)| matches!(s, Slot::History(_) | Slot::Upcoming(_)) && top + s.h() <= scroll)
                    .count();
                // Exactly the expression the draw loop uses.
                let computed = match l.history_top() {
                    Some(ht) => (((scroll - ht).max(0) / RH) as usize).min(hist),
                    None => 0,
                } + match l.upcoming_top() {
                    Some(ut) => (((scroll - ut).max(0) / RH) as usize).min(l.upcoming_len()),
                    None => 0,
                };
                assert_eq!(computed, counted, "hist={hist} scroll={scroll}");
            }
        }
    }

    /// The history is its OWN list: it does not have to be a prefix of the sequence, and it can
    /// exist with nothing playing at all.
    #[test]
    fn the_history_section_is_independent_of_the_sequence() {
        let l = layout(4, 0, None);
        assert_eq!(
            l.slots.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![
                Slot::Head(Section::History),
                Slot::History(0), Slot::History(1), Slot::History(2), Slot::History(3),
            ],
        );
        // …and a sequence playing with nothing played before it draws no history section at all,
        // even though `current` is well past 0.
        let l = layout(0, 8, Some(4));
        assert!(!l.slots.iter().any(|(s, _)| matches!(s, Slot::History(_) | Slot::Head(Section::History))));
    }

    #[test]
    fn the_caption_counts_what_is_left() {
        assert_eq!(caption(1, false), "1 TRACK LEFT");
        assert_eq!(caption(12, false), "12 TRACKS LEFT");
        assert_eq!(caption(12, true), "DRAG TO REORDER");
    }
}
