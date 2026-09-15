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

/// The "clear the queue" chip in the header: `(x, y, w, h)`. An explicit, labelled control rather
/// than a gesture, because emptying the queue is the one action here that cannot be undone.
pub const CLEAR_CHIP: (i32, i32, i32, i32) = (388, 48, 70, 28);
/// The SHUFFLE chip, immediately left of it. Shuffles what is still to come — not the whole
/// sequence, and not the user's own picks (see `App::queue_shuffle`).
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
// Everything below NOW PLAYING, in the order it will actually play: the user's queued picks first,
// then the rest of the play context. ONE index space over the two of them.
//
// This reverses a rule this file used to state twice: "a drag NEVER crosses between them … moving a
// row from one to the other is queueing or un-queueing it, which is a different operation with a
// different meaning, not a reorder." That is true about the STORAGE — the two rows live in
// different vectors, and the release below really does move a `SongRow` between them — but it was
// never true about what the screen shows. The rows are drawn as one column, in play order, and a
// list you can see as one list but only rearrange in two halves is a list with an invisible wall
// down it. Requested 2026-09-15: everything in the queue movable anywhere.
//
// What makes it safe is unchanged from when only the album half moved: the span starts strictly
// AFTER the playing row, so `context_idx` can never be disturbed by a drag, whichever direction the
// row travels. `pre_shuffle` is likewise left alone — see `movable_move`.
//
// Position in the span IS position in what plays next, so the visual order and the play order agree
// by construction. Dragging an album track up past the queue boundary makes it play sooner, which
// is what the picture already promised; the storage move is the consequence, not the point.

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

/// Height of the user queue's scrolling window.
pub fn queue_view_h() -> i32 {
    LIST_BOTTOM - LIST_TOP
}

/// Is this x on the grab handle? Says nothing about the ROW: the caller pairs this with the slot
/// under the finger, because the same column is inert on a played row and on the playing one.
pub fn queue_grip_hit(x: i32) -> bool {
    (GRIP_X0..GRIP_X1).contains(&x)
}

/// The movable span in the order it is currently DRAWN: `from` lifted out and re-inserted at `to`.
///
/// One order over BOTH sections, so a row lifted out of the queue parts the album rows below it and
/// vice versa. The span's slots stay where they are; it is the CONTENT that flows through them,
/// which is what makes a cross-section drag look like one list reflowing rather than two lists
/// arguing. On release `App::movable_move` performs the storage move the preview promised.
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

// ── The unified queue layout ────────────────────────────────────────────────────────────────────
// Up Next used to be TWO mutually exclusive screens: the current album auto-scrolled to the playing
// track, OR — the moment you swipe-queued a single song — the user queue on its own, with no
// now-playing row anywhere on it. Queueing one track therefore hid what was playing, and the queue
// itself never followed playback.
//
// One list now, in Apple Music's order:
//
//     PREVIOUSLY PLAYED     what you have actually played, newest last (see `App::history`)
//     NOW PLAYING           the current track
//     NEXT IN QUEUE         the user's own swipe-queued picks (reorderable, removable)
//     NEXT FROM <ALBUM>     the rest of the album
//
// PREVIOUSLY PLAYED used to be `context[..current]` — the album tracks before the playing one. That
// is not a history, it is a spelling of "earlier in this album": start a different album and
// everything you played before it vanished, and it could never hold a swipe-queued pick at all,
// because a pick is not in the context. It is a real accumulated list now, kept by the shell.
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
    /// A track that has already played — index into the HISTORY list, not into the album. Oldest
    /// first, so the row nearest NOW PLAYING is the one that played most recently.
    History(usize),
    /// The playing track — index into the album's track list.
    Current(usize),
    /// The playing track is a USER PICK, not a context row. It has already been taken out of the
    /// queue (that is what stops it replaying), so it is not `Queued(_)` either — without this
    /// slot the screen had nowhere to put it and drew the context row the pick INTERRUPTED as
    /// NOW PLAYING, i.e. named the previous song for the whole of the pick.
    CurrentPick,
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
    /// Content-space top of the first user-queue row. Stored rather than searched for: the draw
    /// loop needs it once a frame to work out how many queue rows are above the window, and a
    /// linear find over a sequence that can be the whole library is not the way to answer an
    /// arithmetic question.
    queue_top_px: Option<i32>,
    /// The same two numbers for NEXT FROM: content-space top of its first row, and the CONTEXT
    /// index that row holds. The section does not start at context index 0 — it starts one past
    /// the playing track — so the offset is what converts between a drag's relative index and the
    /// absolute one `Slot::Upcoming` carries.
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
/// `metrics()` and `layout()` MUST agree; `metrics_matches_layout` sweeps them against each other,
/// because two sources of one truth is the bug class this screen was rewritten to remove.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub current_top: Option<i32>,
    pub content_h: i32,
}

/// `pick` = a user-queued track is the one actually playing. It takes the NOW PLAYING row and the
/// context resumes below the queue.
///
/// `hist` is the length of the shell's play history — an independent list now, not a slice of the
/// context, so it is passed in rather than derived from `current`.
pub fn metrics(hist: usize, album_len: usize, current: Option<usize>, queued: usize, pick: bool) -> Metrics {
    let mut y = 0i32;
    let mut current_top = None;
    let history = hist;
    if history > 0 {
        y += HDR_H + history as i32 * RH; // history header + the played rows
    }
    if current.is_some() || pick {
        y += HDR_H; // NOW PLAYING header
        current_top = Some(y);
        y += RH;
    }
    if queued > 0 {
        y += HDR_H + queued as i32 * RH;
    }
    if let Some(cur) = current {
        if cur + 1 < album_len {
            y += HDR_H + (album_len - cur - 1) as i32 * RH;
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

/// Build the slot list. `album_len`/`current` describe the album the playing track belongs to
/// (`current == None` when nothing is playing or the track isn't in the library); `queued` is the
/// user queue's length; `hist` the play history's.
pub fn layout(hist: usize, album_len: usize, current: Option<usize>, queued: usize, pick: bool) -> Layout {
    let mut l = Layout::default();
    // RESERVE UP FRONT. Every track is a slot, and after a "Shuffle all songs" that is the whole
    // library — growing from empty meant a dozen reallocations and memcpys of a list that ends up
    // ~29 KB, once per painted frame. Measured: this is most of what an Up Next frame costs beyond
    // the ~14 rows it actually draws. Four spare for the section headings.
    l.slots = Vec::with_capacity(hist + album_len + queued + 5);
    let mut y = 0;
    let push = |l: &mut Layout, s: Slot, y: &mut i32| {
        l.slots.push((s, *y));
        *y += s.h();
    };
    let history = hist;
    if history > 0 {
        push(&mut l, Slot::Head(Section::History), &mut y);
        for i in 0..history {
            push(&mut l, Slot::History(i), &mut y);
        }
    }
    if current.is_some() || pick {
        push(&mut l, Slot::Head(Section::Now), &mut y);
        l.current_top = Some(y);
        match (pick, current) {
            (true, _) => push(&mut l, Slot::CurrentPick, &mut y),
            (false, Some(cur)) => push(&mut l, Slot::Current(cur), &mut y),
            (false, None) => unreachable!("guarded by current.is_some() || pick"),
        }
    }
    if queued > 0 {
        push(&mut l, Slot::Head(Section::Queue), &mut y);
        l.queue_top_px = Some(y);
        for i in 0..queued {
            push(&mut l, Slot::Queued(i), &mut y);
        }
    }
    if let Some(cur) = current {
        if cur + 1 < album_len {
            push(&mut l, Slot::Head(Section::Album), &mut y);
            l.upcoming_top_px = Some(y);
            l.upcoming_first = cur + 1;
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
        // BINARY SEARCH, not a scan. `slots` is built in ascending `top` order and after a
        // "Shuffle all songs" it holds one entry per track — 3,463 on the reference device. This
        // runs on every tap, on every frame of a reorder drag, and on every swipe classification,
        // so a linear find here is a scan of the whole library under a moving finger.
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
    /// How many user-queue rows the span starts with.
    pub fn queued_len(&self) -> usize {
        self.slots.iter().filter(|(s, _)| matches!(s, Slot::Queued(_))).count()
    }
    /// Content-space top of the first user-queue row, if the queue section exists.
    pub fn queue_top(&self) -> Option<i32> {
        self.queue_top_px
    }

    /// How many rows NEXT FROM holds, its content-space top, and the context index of its first
    /// row.
    pub fn upcoming_len(&self) -> usize {
        self.slots.iter().filter(|(s, _)| matches!(s, Slot::Upcoming(_))).count()
    }
    pub fn upcoming_top(&self) -> Option<i32> {
        self.upcoming_top_px
    }
    pub fn upcoming_first(&self) -> usize {
        self.upcoming_first
    }

    /// Length of the movable span: every queued pick, then every upcoming context row.
    pub fn movable_len(&self) -> usize {
        self.queued_len() + self.upcoming_len()
    }

    /// What movable index `i` refers to, as a slot. The span is the two sections concatenated, so
    /// this is the ONE place that knows the boundary sits at `queued_len()`.
    pub fn movable_slot(&self, i: usize) -> Option<Slot> {
        let q = self.queued_len();
        if i < q {
            Some(Slot::Queued(i))
        } else if i < q + self.upcoming_len() {
            Some(Slot::Upcoming(self.upcoming_first + (i - q)))
        } else {
            None
        }
    }

    /// The movable index of a slot, or None if it is not a movable row.
    pub fn movable_index(&self, slot: Slot) -> Option<usize> {
        match slot {
            Slot::Queued(i) if i < self.queued_len() => Some(i),
            Slot::Upcoming(i) => {
                let rel = i.checked_sub(self.upcoming_first)?;
                (rel < self.upcoming_len()).then(|| self.queued_len() + rel)
            }
            _ => None,
        }
    }

    /// Which movable index a floating row is over, from its top edge in screen coords.
    ///
    /// NOT a single division. The span is contiguous in INDEX but not in PIXELS: the NEXT FROM
    /// heading sits between the two sections, so a lifted row crossing the boundary travels
    /// `HDR_H` px that belong to no row at all. Dividing the whole span by `RH` from the queue's
    /// top would therefore report an index one too high for every album row — the row under the
    /// finger and the row that moves would stop being the same row, which is the specific failure
    /// this screen's geometry rules exist to prevent. So each section is measured from its own
    /// top, and the heading between them resolves to the boundary index.
    pub fn movable_slot_for(&self, float_top: i32, scroll_px: i32) -> usize {
        let qlen = self.queued_len();
        let ulen = self.upcoming_len();
        let len = qlen + ulen;
        if len == 0 {
            return 0;
        }
        // Content-space centre of the floating row.
        let cy = float_top - LIST_TOP + scroll_px + RH / 2;
        // Above the span entirely (over NOW PLAYING or the history) pins to the front: "play this
        // next" is what dragging a row to the top means, and refusing the drop would just strand
        // the row.
        let qtop = self.queue_top();
        let utop = self.upcoming_top();
        if let (Some(qt), true) = (qtop, qlen > 0) {
            if cy < qt + qlen as i32 * RH {
                return ((cy - qt).div_euclid(RH)).clamp(0, qlen as i32 - 1) as usize;
            }
        }
        if let (Some(ut), true) = (utop, ulen > 0) {
            if cy < ut {
                return qlen; // on the NEXT FROM heading — the boundary between the two
            }
            return (qlen as i32 + (cy - ut).div_euclid(RH)).clamp(0, len as i32 - 1) as usize;
        }
        (len - 1).min(((cy - qtop.unwrap_or(0)).div_euclid(RH)).clamp(0, len as i32 - 1) as usize)
    }

    /// Content-space top of movable index `i` as it is DRAWN — used to place the lifted row and to
    /// keep the float from drifting off the slot it will land in.
    pub fn movable_top(&self, i: usize) -> Option<i32> {
        self.movable_slot(i).and_then(|s| self.top_of(s))
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
    /// What has already played, oldest first — the shell's real history, not a slice of `tracks`.
    pub history: &'a [SongRow],
    /// The pick that is PLAYING right now, if the transport is on one. It has already left
    /// `queue` (a pick is consumed when it starts), so this is the only handle on it.
    pub pick: Option<&'a SongRow>,
    pub lib: &'a crate::model::Library,
    pub scroll_px: i32,
    pub drag: Option<RowDrag>,
    pub swipe: Option<crate::library::SwipeRow>,
    pub sbar_active: bool,
}

/// Draw the whole Up Next screen. Returns the layout it drew, so `nav` can hit-test against
/// exactly what is on the glass rather than rebuilding it and hoping the two agree.
pub fn render_view(c: &mut Canvas, t: &Theme, f: &FontSet, v: &QueueView) -> Layout {
    c.fill(t.bg);
    let l = layout(v.history.len(), v.tracks.len(), v.current, v.queue.len(), v.pick.is_some());

    if l.slots.is_empty() {
        let _ = crate::chrome::header(c, t, f, "Up Next", None);
        text::draw(c, f, 22.0, 360.0, "Nothing queued.",
                   &sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, t.ink, 0.0));
        text::draw(c, f, 22.0, 386.0, "Play a track and its album appears here.",
                   &sty(Family::Sans, Weight::Regular, 16.0, t.dim, 0.0));
        return l;
    }

    let y0 = crate::chrome::header(c, t, f, "Up Next", None);
    // CLEAR belongs to the user queue, so it only appears when there is one to clear. SHUFFLE
    // belongs to the CONTEXT — there is something to shuffle whenever tracks remain after the
    // current one, queue or no queue.
    let can_clear = !v.queue.is_empty();
    let can_shuffle = v.current.map_or(false, |c| c + 1 < v.tracks.len());
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
    // The caption goes left of whichever chip is furthest left, so it can never run under one.
    let cap_right = if can_shuffle { SHUFFLE_CHIP.0 } else if can_clear { CLEAR_CHIP.0 } else { 458 };
    let cap = if v.drag.is_some() {
        String::from("DRAG TO REORDER")
    } else if can_clear {
        format!("{} QUEUED", v.queue.len())
    } else {
        format!("{} TRACKS", v.tracks.len())
    };
    if !v.tracks.is_empty() || can_clear {
        right(c, f, (cap_right - 12) as f32, 65.0, &cap,
              &sty(Family::Mono, Weight::Regular, 12.0, t.faint, 0.1));
    }

    let scroll = v.scroll_px.clamp(0, l.max_scroll_px());
    // ONE order over the whole movable span, so a lifted row parts the rows below it whichever
    // section they belong to. The slots stay put; the CONTENT flows through them.
    let qlen = v.queue.len();
    let ufirst = l.upcoming_first();
    let morder = drag_order(l.movable_len(), v.drag);

    // START AT THE FIRST VISIBLE SLOT. This loop used to begin at slot 0 and `continue` past
    // everything above the window — which is O(the whole sequence) to draw the ~14 rows on screen,
    // and after a "Shuffle all songs" that is 3,600 iterations a frame. `slots` is built in
    // ascending `top` order, so the first visible one is a binary search.
    //
    // A slot is above the window when `top + h <= scroll` — the same test the old `continue` made,
    // rearranged so it does not mention `y0`.
    let first = l.slots.partition_point(|(s, top)| top + s.h() <= scroll);
    // `qseen` counts the QUEUE rows that were skipped, because `qorder` is indexed by drawn
    // position. The old loop accumulated it while walking past them; skipping the walk means
    // computing it, which is the same arithmetic the binary search just replaced.
    // …and the same arithmetic for the span: queue rows scrolled off, plus upcoming rows scrolled
    // off. Summed rather than tracked separately because `morder` is indexed by span position.
    let mut mseen = match l.queue_top() {
        Some(qt) => (((scroll - qt).max(0) / RH) as usize).min(qlen),
        None => 0,
    } + match l.upcoming_top() {
        Some(ut) => (((scroll - ut).max(0) / RH) as usize).min(l.upcoming_len()),
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
            Slot::History(i) => {
                if let Some(song) = v.history.get(i) {
                    // History is dimmed — it is context, not a destination, and Apple Music reads
                    // the same way. Still tappable: that is how you go back a track.
                    //
                    // NO track number. A history row can have come from any album, or from a
                    // swipe-queued pick with no album position at all, so a number here would be
                    // the one thing on the row that was not about the row.
                    album_row(c, t, f, song, v.lib, y, 0, true, false, false);
                }
            }
            Slot::Current(i) => {
                if let Some(song) = v.tracks.get(i) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel);
                    fill_rect(c, 0, y, 4, RH, t.acc);
                    album_row(c, t, f, song, v.lib, y, i + 1, false, true, false);
                }
            }
            Slot::CurrentPick => {
                if let Some(song) = v.pick {
                    fill_rect(c, 0, y, W as i32, RH, t.panel);
                    fill_rect(c, 0, y, 4, RH, t.acc);
                    // No track NUMBER: a pick has no position in the album under it, and printing
                    // the context row's number here is what made the old screen unreadable.
                    album_row(c, t, f, song, v.lib, y, 0, false, true, false);
                }
            }
            // BOTH movable kinds are drawn by the same arm, because under a drag the content in
            // a queue slot can be an album track and vice versa — that reflow IS the preview of
            // where the row will land. What each row is drawn AS follows the content, never the
            // slot it is sitting in.
            Slot::Queued(_) | Slot::Upcoming(_) => {
                let mi = morder.get(mseen).copied().unwrap_or(0);
                mseen += 1;
                if v.drag.map(|d| d.from) == Some(mi) {
                    fill_rect(c, 0, y, W as i32, RH, t.panel); // the well the row came out of
                } else if mi < qlen {
                    if let Some(song) = v.queue.get(mi) {
                        let sw = v
                            .swipe
                            .filter(|s| (y..y + RH).contains(&s.y) && s.dx != 0)
                            .map(|s| s.dx);
                        if let Some(dx) = sw {
                            crate::library::swipe_reveal(c, t, f, y, RH, dx,
                                                         crate::library::SwipeIntent::Remove);
                        }
                        queue_row(c, t, f, song, v.lib, y, mseen);
                        if sw.is_some() {
                            c.clear_offset_x();
                        }
                    }
                } else if let Some(song) = v.tracks.get(ufirst + mi - qlen) {
                    album_row(c, t, f, song, v.lib, y, ufirst + mi - qlen + 1, false, false, true);
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
        let song = if d.from < qlen {
            v.queue.get(d.from)
        } else {
            v.tracks.get(ufirst + d.from - qlen)
        };
        if let Some(song) = song {
            let ft = d.float_top().clamp(y0 - RH / 2, LIST_BOTTOM - RH / 2);
            c.set_clip_y(y0, LIST_BOTTOM);
            fill_rect(c, 0, ft, W as i32, RH, t.row_sel);
            fill_rect(c, 0, ft, 4, RH, t.acc);
            hline(c, ft, t.line);
            hline(c, ft + RH, t.line);
            // Both lifted rows carry a grip, whichever list they came from: it is the thing that
            // says "this row is in your hand". The row functions draw it faint; the accent
            // overdraw below is what marks it as lifted.
            // Drawn as what it IS, at the position it would LAND on — the two halves of the
            // answer the drag is being asked for.
            if d.from < qlen {
                queue_row(c, t, f, song, v.lib, ft, d.to + 1);
            } else {
                album_row(c, t, f, song, v.lib, ft, ufirst + d.from - qlen + 1, false, false, true);
            }
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

/// An album-side row (history, current or upcoming). `past` dims it; `now` marks it playing.
///
/// `grippy` draws the reorder handle and gives up the width it needs. Only the UPCOMING rows ask
/// for it: they are the ones that move. A handle on a played row or on the playing one would be a
/// control that does nothing, which is worse than no control at all.
fn album_row(c: &mut Canvas, t: &Theme, f: &FontSet, song: &SongRow,
             lib: &crate::model::Library, y: i32, n: usize, past: bool, now: bool, grippy: bool) {
    let cy = (y + RH / 2) as f32;
    let idx_col = if now { t.acc } else { t.faint };
    // `n == 0` means "this row has no track number" — a history row (any album, or a pick with no
    // album position at all) or the playing pick. Printing "00" there was the old screen's way of
    // naming a position that does not exist.
    let idx = if now { Some("\u{25b6}".to_string()) } else if n > 0 { Some(format!("{n:02}")) } else { None };
    if let Some(idx) = idx {
        text::draw(c, f, 22.0, cy + 4.0, &idx, &sty(Family::Mono, Weight::Regular, 12.0, idx_col, 0.0));
    }
    // Played rows fade their art too, so the eye finds the current row without reading a word.
    let dim = if past { 0.34 } else if t.night { 0.30 } else { 1.0 };
    crate::library::thumb(c, t, lib, song.album_id, &song.art, 46, y + (RH - 48) / 2, 48, dim);
    let title_col = if now { t.acc } else if past { t.dim } else { t.ink };
    let tst = sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, title_col, 0.0);
    // The text budget and the duration column both shift in when the handle is there, by exactly
    // the amounts `queue_row` uses — the handle sits in the same column on both lists, so the
    // clearance it needs is the same number, not a second one that has to be kept in step.
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
    let tst = sty(Family::Sans, Weight::SemiBold, crate::scale::ROW, t.ink, 0.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The binary search must answer exactly what the linear scan it replaced answered — for every
    /// pixel of a layout that has history, a current row, a queue and an album tail, including the
    /// gaps between sections where the answer is None.
    #[test]
    fn at_matches_a_linear_scan_everywhere() {
        for (hist, album_len, cur, queued, pick) in [
            (100usize, 200usize, Some(100usize), 4usize, false),
            (0, 5, Some(0), 0, false),
            (39, 40, Some(39), 3, false),
            (0, 0, None, 6, false),
            // The same sweep with a user pick on the NOW PLAYING row, which moves every section
            // below it down by one.
            (101, 200, Some(100), 4, true),
            (1, 5, Some(0), 0, true),
            (40, 40, Some(39), 3, true),
            (0, 0, None, 6, true),
            // …and a history that owes nothing to the context at all, which is the whole point of
            // it being its own list: played tracks with no album on screen, and an album on screen
            // with nothing played before it.
            (12, 0, None, 0, false),
            (0, 8, Some(4), 2, false),
        ] {
            let l = layout(hist, album_len, cur, queued, pick);
            for scroll in [0, 37, 500, l.max_scroll_px()] {
                for y in LIST_TOP - 2..LIST_BOTTOM + 2 {
                    let want = if !(LIST_TOP..LIST_BOTTOM).contains(&y) {
                        None
                    } else {
                        let cy = y - LIST_TOP + scroll.max(0);
                        l.slots.iter().find(|(s, top)| cy >= *top && cy < *top + s.h()).map(|(s, _)| *s)
                    };
                    assert_eq!(l.at(y, scroll), want,
                               "y={y} scroll={scroll} hist={hist} album={album_len} cur={cur:?}                                 queued={queued} pick={pick}");
                }
            }
        }
    }

    #[test]
    fn metrics_matches_layout() {
        for hist in [0usize, 1, 7] {
        for album_len in [0usize, 1, 2, 5, 40] {
            for queued in [0usize, 1, 3, 12] {
                // `None`, plus every valid current index, plus one past the end.
                let currents: Vec<Option<usize>> =
                    std::iter::once(None).chain((0..=album_len).map(Some)).collect();
                for cur in currents {
                    // layout() only draws a current row when the index is inside the album.
                    let cur = cur.filter(|c| *c < album_len);
                    // …and both shapes: with a user pick on the NOW PLAYING row and without.
                    for pick in [false, true] {
                        let l = layout(hist, album_len, cur, queued, pick);
                        let m = metrics(hist, album_len, cur, queued, pick);
                        assert_eq!(
                            m.content_h, l.content_h,
                            "content_h disagrees at hist={hist} album={album_len} cur={cur:?}                              queued={queued} pick={pick}"
                        );
                        assert_eq!(
                            m.current_top, l.current_top,
                            "current_top disagrees at hist={hist} album={album_len} cur={cur:?}                              queued={queued} pick={pick}"
                        );
                        assert_eq!(m.follow_scroll(), l.follow_scroll());
                        assert_eq!(m.max_scroll_px(), l.max_scroll_px());
                        // Whatever the shape, the slot list is in ascending top order and its
                        // heights add up to content_h — the two invariants the binary searches
                        // in `at`, the draw loop and `mseen` all rest on.
                        let mut y = 0;
                        for (slot, top) in &l.slots {
                            assert_eq!(*top, y, "slots are not contiguous at {slot:?}");
                            y += slot.h();
                        }
                        assert_eq!(y, l.content_h);
                        // The movable span and the slot list must agree in both directions: every
                        // index names a slot, every movable slot names that index back, and
                        // nothing outside the span claims one.
                        assert_eq!(l.movable_len(), queued + l.upcoming_len());
                        for i in 0..l.movable_len() {
                            let slot = l.movable_slot(i).expect("index inside the span has a slot");
                            assert_eq!(l.movable_index(slot), Some(i), "round trip failed at {i}");
                            assert!(l.top_of(slot).is_some(), "{slot:?} is not drawn");
                        }
                        assert_eq!(l.movable_slot(l.movable_len()), None);
                        for (slot, _) in &l.slots {
                            let movable = matches!(slot, Slot::Queued(_) | Slot::Upcoming(_));
                            assert_eq!(l.movable_index(*slot).is_some(), movable,
                                       "{slot:?} disagrees about being movable");
                        }
                    }
                }
            }
        }
        }
    }

    /// Every pixel of the movable span resolves to the row DRAWN there — the rule the whole
    /// cross-section drag rests on.
    ///
    /// The trap this exists for: the span is contiguous in INDEX but not in PIXELS. The NEXT FROM
    /// heading sits inside it, so `HDR_H` px belong to no row at all, and measuring the whole span
    /// with one division from the queue's top reports an index one too high for every album row.
    /// The row under the finger and the row that moves would stop being the same row.
    #[test]
    fn a_float_over_a_row_lands_on_that_row_across_the_section_boundary() {
        for (hist, album_len, cur, queued) in
            [(3usize, 20usize, Some(4usize), 3usize), (0, 9, Some(0), 1), (5, 6, Some(2), 0)]
        {
            let l = layout(hist, album_len, cur, queued, false);
            for i in 0..l.movable_len() {
                let slot = l.movable_slot(i).unwrap();
                let top = l.top_of(slot).unwrap();
                // Place the float exactly on the row, at scroll 0, and ask where it would land.
                let float_top = LIST_TOP + top;
                assert_eq!(l.movable_slot_for(float_top, 0), i,
                           "float over {slot:?} resolved elsewhere (hist={hist} album={album_len} \
                            cur={cur:?} queued={queued})");
            }
            // Above the span pins to the front — "play this next" — rather than refusing the drop.
            assert_eq!(l.movable_slot_for(LIST_TOP - 400, 0), 0);
        }
    }

    /// The draw loop now binary-searches its way to the first visible slot instead of walking the
    /// whole sequence. It must land on exactly the slot the old linear scan would have: the first
    /// whose BOTTOM is still below the top of the window.
    #[test]
    fn the_first_visible_slot_is_found_by_search_not_by_walking() {
        let l = layout(100, 200, Some(100), 4, false);
        let max = l.max_scroll_px();
        for scroll in [0, 1, RH - 1, RH, RH + 1, HDR_H, 500, 1234, max / 2, max] {
            let want = l.slots.iter().position(|(s, top)| top + s.h() > scroll).unwrap_or(l.slots.len());
            let got = l.slots.partition_point(|(s, top)| top + s.h() <= scroll);
            assert_eq!(got, want, "first visible slot disagrees at scroll={scroll}");
        }
    }

    /// `mseen` is the number of MOVABLE rows scrolled off the top, and `morder` is indexed by it —
    /// so an off-by-one here draws the wrong track in every row below the fold. The draw loop
    /// computes it as two pieces of arithmetic instead of counting rows on the way past; the two
    /// must agree at every scroll offset, for both sections at once.
    #[test]
    fn the_skipped_movable_row_count_is_computed_not_counted() {
        for (hist, queued) in [(0usize, 1usize), (4, 3), (9, 12), (0, 0), (6, 0)] {
            let l = layout(hist, 60, Some(30), queued, false);
            let max = l.max_scroll_px();
            for scroll in 0..=max {
                let counted = l
                    .slots
                    .iter()
                    .filter(|(s, top)| {
                        matches!(s, Slot::Queued(_) | Slot::Upcoming(_)) && top + s.h() <= scroll
                    })
                    .count();
                // Exactly the expression the draw loop uses.
                let computed = match l.queue_top() {
                    Some(qt) => (((scroll - qt).max(0) / RH) as usize).min(queued),
                    None => 0,
                } + match l.upcoming_top() {
                    Some(ut) => (((scroll - ut).max(0) / RH) as usize).min(l.upcoming_len()),
                    None => 0,
                };
                assert_eq!(computed, counted, "hist={hist} queued={queued} scroll={scroll}");
            }
        }
    }

    /// No queue section means no queue rows to skip, whatever the scroll.
    #[test]
    fn no_queue_section_skips_nothing() {
        let l = layout(0, 60, Some(30), 0, false);
        assert_eq!(l.queue_top(), None);
    }

    /// The history is its OWN list: it does not have to be a prefix of the album, and it can exist
    /// with no album on screen at all. Both were impossible before, and both are the point.
    #[test]
    fn the_history_section_is_independent_of_the_context() {
        // Played tracks, nothing playing, nothing queued: a history section and nothing else.
        let l = layout(4, 0, None, 0, false);
        assert_eq!(
            l.slots.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![
                Slot::Head(Section::History),
                Slot::History(0), Slot::History(1), Slot::History(2), Slot::History(3),
            ],
        );
        // …and an album playing with nothing played before it draws no history section at all,
        // even though `current` is well past 0.
        let l = layout(0, 8, Some(4), 0, false);
        assert!(!l.slots.iter().any(|(s, _)| matches!(s, Slot::History(_) | Slot::Head(Section::History))));
    }
}
