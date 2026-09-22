# Up Next v2 — one list, tiered

Status: DESIGN, 2026-09-22. Supersedes nothing yet; `nav.rs`'s two-list model is what ships today.

Goal, in the owner's words: *the best combination of Spotify, Apple Music and MusicBee's queue and
shuffle systems, to give the user the most control.*

---

## 1. What each reference player is actually good at

| | What it gives | Cinder today |
|---|---|---|
| **Spotify** | Two gestures, no menus: *Play next* / *Add to queue*. The queue cuts in front of the album and the album resumes after. Autoplay when it runs dry. | **Has it.** Swipe left = next, swipe right = later; `play_order_uris` = `[current] + queue + context tail`. No autoplay. |
| **Apple Music** | *Playing Next* is **one editable list**. The rest of the album is as draggable and as removable as a hand-pick. | **Half.** `movable_move` (nav.rs:4816-4890) already drags across `history ++ queue ++ context tail`, so the album tail DOES reorder. Swipe-to-remove is still `Slot::Queued` only (nav.rs:5137-5147). `queue_move` is dead code — tests only. |
| **MusicBee** | Desktop-grade: queued items **numbered in place**, shuffle **by album / by artist** (not just by track), **repeat album**, **stop after current**, Auto-DJ refill, queue saveable as a playlist. | **Mostly no.** Shuffle is tracks-only; repeat is off/all/one; no stop-after; no save-as-playlist; no refill. |

Cinder is already **ahead** of all three in three places, and v2 must not lose them:

- `queue_play_at` — tapping the third queued row plays it next and the two above it **follow**.
  Spotify throws them away.
- `unshuffle_context` — shuffle off restores the real album order and keeps the audible track.
  Spotify cannot go back.
- `history` — a genuine listen log (100 deep, persisted), not "the album tracks before this one".
  Neither Spotify nor Apple shows one in the queue view.

---

## 1a. Bugs found while mapping this — fix before, or as part of, P1

These are defects in what ships today, not gaps in the design. Ranked by what a user hits soonest.

1. **"Shuffle all songs" only ever plays 512 tracks.** `set_pending` truncates the resolved
   sequence to `MAX_PLAY_SEQUENCE` **before** it becomes the context (cinder-ffi/src/lib.rs:2242-2249),
   so on a 3355-track library a shuffle-all is permanently a 512-track context. The other 2843
   never play, and the persisted `ctx=` holds 512 ids. The only signal is an `eprintln!`.

2. **Every Shuffle band silently destroys the user's queue.** `start_play_action` (nav.rs:1687) is
   the funnel that raises the "you have a queue" prompt, but `Action::Shuffle` (nav.rs:3677),
   `ShuffleArtist` (3723) and `ShufflePlaylist` (4003) are returned **directly**, bypassing it —
   they land in `set_play_context`, which calls `queue.clear()`. On the playlist screen the *Play*
   band asks and the *Shuffle* band beside it does not.

3. **Repeat-one starves the queue.** With repeat-one on there is no track boundary, so
   `track_started` never runs, the pick is never consumed, and a queued track never plays.
   `g_repeat_one` is sticky across sequences (cinder-audio/src/player_shim.cpp:389,481).
   Caveat: player_shim.cpp:501-505 records that `OneTrackMode` has **never been confirmed to work
   on device**, so this may be moot — that needs settling before it is modelled.

4. **The CLEAR chip is not confirmed.** nav.rs:3125-3129 clears immediately, notify only. A comment
   at nav.rs:5341 asserts the opposite about its own code ("the CLEAR chip beside it … has both").
   Either gate it or delete the false comment.

5. **Phantom queue rows.** A pick whose file has been deleted is dropped from the sequence by
   `play_order`'s `.flatten()` but stays in `queue` and in Up Next for ever — nothing starts it, and
   `track_started` is the only consumer. Tapping it reorders the queue and plays nothing
   (lib.rs:2831). Queueing the **currently playing** track does the same, via the no-adjacent-
   duplicates rule (lib.rs:2360).

6. **Repeat-all replays picks every lap.** `cinder_repeat_all_prepare` (lib.rs:3758) builds
   `queue + WHOLE context` with `lead = None`, ignoring `playing_pick` — so a surviving pick plays
   again at the head of each lap and the lap restarts from the top of the context.

7. **`NEXT FROM` lies after a shuffle-all.** The heading is the *current track's* album
   (nav.rs:6553-6556) over a tail that is the whole (truncated) library.

Items 1, 2 and 7 are one story: the Shuffle bands are the least-guarded path in the player.

## 2. The model

**One list. Every row remembers where it came from.**

```rust
pub enum Origin {
    /// The user put it here by hand. Numbered in the UI — MusicBee's queue markers.
    Pick,
    /// It arrived as part of a sequence the user started. The id groups a RUN of rows under one
    /// `NEXT FROM …` heading, and is what makes chaining album after album expressible.
    Ctx(u32),
}

pub struct Entry {
    pub row: SongRow,
    pub origin: Origin,
}
```

`App` holds:

```rust
up: Vec<Entry>,              // everything: played, playing, and to come
up_idx: usize,               // the row that is audible
ctx_label: Vec<(u32, String)>,   // id -> "ABBEY ROAD"
```

**Headings are derived, never stored.** Walk `up[up_idx+1..]` and cut it into runs by `origin`:
a run of `Pick` draws `NEXT IN QUEUE` with each row numbered 1..n; a run of `Ctx(n)` draws
`NEXT FROM <ctx_label(n)>`. A second, third, fourth context run draws its own heading — which is
exactly what "play this album after that one" looks like on screen.

What the one change buys, all of it falling out of `up` being a plain `Vec`:

| Want | How it is now expressed |
|---|---|
| Queue cuts in front of the album (Spotify) | insert a `Pick` at `up_idx + 1` |
| Queued items numbered (MusicBee) | position within the `Pick` run |
| Reorder or delete **any** row, album tail included (Apple) | `up.swap` / `up.remove` |
| **Play album B after album A finishes** | append a run of `Ctx(new)` at the end of A's run |
| Play B at the very end, after my picks too | append the run at the end of `up` |
| Repeat this album only (MusicBee) | repeat the current `Ctx` run |

The sequence handed to PlayerService collapses to `up[up_idx..]` mapped to URIs.
`play_order_uris`'s (lib.rs:2295) `[current] + queue + tail` concatenation is deleted, and with it the class of
bug its own comments record (a kept queue excluded from the installed sequence for ever; a flush
owed against a context that had already been replaced).

---

## 3. Control surface

### Row gestures

| Gesture | Action | Position |
|---|---|---|
| swipe LEFT | Play next | `Pick` at `up_idx+1`, **after** any picks already waiting there (MusicBee stacking — a second "play next" does not jump the first) |
| swipe RIGHT | Play later | `Pick` appended after the last `Pick` run |
| **long press (new)** | sheet | Play next · **Play after this album** · Play last · Add to playlist |

"Play after this album" is the one genuinely new **gesture**, and it is the owner's named
scenario. The *mechanism* is half-built already: `movable_move` clamps a drop at
`.min(self.context.len())` (nav.rs:4885), so a pick dragged to the very bottom of Up Next already
lands after the last context track. What does not exist is doing it in one action, or doing it to a
whole album — dragging a 12-track album down one row at a time is not a gesture anyone will use.

### Shuffle becomes a MODE, not a one-shot

```rust
pub enum Shuffle { Off, Tracks, Albums, Artists }
```

- **Off** — restore the pre-shuffle order (`unshuffle_context`, already built).
- **Tracks** — permute the upcoming `Ctx` entries (`queue_shuffle`, already built).
- **Albums** — permute the ORDER of album runs; each album keeps its own track order. The mode
  people actually want for a 3355-track library, and the one no phone player does well.
- **Artists** — same, grouped by artist.

`Pick` rows are **never** shuffled. That is already the rule and it stays.
A **RESHUFFLE** chip re-rolls inside the current mode without leaving it.

### Repeat gains one position

```rust
pub enum Repeat { Off, All, One, Album }
```

Off / All / One exist (`Action::RepeatCycle`; All is shell-driven off the queue-boundary signal
measured 2026-08-26). **Album** repeats the current `Ctx` run and is only meaningful once runs
exist — another thing the model makes expressible rather than special-cased.

### Three cheap wins the reference players have and Cinder does not

1. **Stop after current track.** A chip on Now Playing. On a DAP at bedtime this is the single
   most-wanted transport control and it costs a bool plus a check at the track boundary.
2. **Save queue as playlist.** Routes `up[up_idx..]` into the existing `playlist_pick` screen.
3. **Autoplay / refill.** When the list empties, append N more. Cinder can do this **offline**:
   SensMe channels are already in `MTPDB`, and artist/album grouping is already in `cinder-db`.
   "More by this artist" and "same SensMe channel" need no network and no service.

---

## 4. Migration

There are **18** non-test call sites for `context()`, `context_idx()`, `set_play_context()` and
`queue()`, all but one of them in `cinder-ffi/src/lib.rs`. Keep every one of them compiling by
making them **derived views** over `up`:

- `context()` → the rows of the `Ctx` run containing `up_idx`
- `context_idx()` → the offset of `up_idx` inside that run
- `queue()` → the `Pick` entries ahead of `up_idx`

Day one is then a pure refactor with no behaviour change, provable by the existing suite.

### Risks, and what each needs

1. **`pre_shuffle` holds bare object_ids.** With runs it must hold them per run, or shuffle-off
   after a chain reorders across an album boundary.
2. **`queue.conf` format.** Today `ctx=` / `idx=` / `q=` / `hist=`. v2 needs an origin per row.
   Old files must still load — read a v1 file as one `Ctx` run plus one `Pick` run.
3. **The 512 cap is invisible.** `MAX_PLAY_SEQUENCE` truncation logs to stderr only, so Up Next
   already draws rows that will not play. Chaining makes it easy to exceed. v2 owes a visible
   cut-line marker in the list.
4. **One shell-visible action per tap.** `cinder_tap` returns the FIRST `carry_action` code;
   internal actions (`QueueChanged`) return `None` and do not consume it. Every new queue edit
   must stay internal, or it will silently eat the play action beside it.
5. **`playing_pick` disappears.** It exists only because a pick leaves `queue` when it starts.
   With one list nothing leaves, so the field, its persistence key and the bug it was added for
   all go away. Check `track_started` for anything else leaning on it.

---

## 5. Phasing

| Phase | What lands | Proves |
|---|---|---|
| P1 | `Entry`/`Origin`, derived accessors, persistence with v1 fallback | full suite green, no behaviour change |
| P2 | Chaining: "Play after this album", `THEN <B>` headings | **the owner's named scenario** |
| P3 | Shuffle modes Albums/Artists + RESHUFFLE chip | |
| P4 | Album-tail rows become reorderable and removable | Apple parity |
| P5 | Repeat-album, Stop after current, Save queue as playlist | MusicBee parity |
| P6 | Autoplay refill from SensMe channel / artist | Spotify parity, offline |

P1+P2 is the smallest thing that answers the original ask.
