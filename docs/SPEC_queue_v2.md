# Up Next v2 — a context chain

Status: DESIGN, revised 2026-09-22 after two adversarial reviews.
Audience: whoever implements this — the owner, or a contributor picking it up cold.

Goal, in the owner's words: *the best combination of Spotify, Apple Music and MusicBee's queue and
shuffle systems, to give the user the most control.*

The first draft of this document proposed collapsing the queue and the playing sequence into one
`Vec<Entry>`. **That design is rejected.** §2 says why, because the reasons are the same ones that
constrain any future attempt.

---

## 1. What each reference player is good at

| | What it gives | Cinder today |
|---|---|---|
| **Spotify** | Two gestures, no menus: *Play next* / *Add to queue*. The queue cuts in front of the album; the album resumes after. Autoplay when it runs dry. | **Has it.** Swipe left = next, swipe right = later. `play_order_uris` (cinder-ffi/src/lib.rs:2295) = `[current] + queue + context tail`. No autoplay. |
| **Apple Music** | *Playing Next* is one editable list — the rest of the album is as draggable and as removable as a hand-pick. | **Most of it.** `movable_move` (nav.rs:4816-4890) already drags across `history ++ queue ++ context tail`. Only swipe-to-**remove** is queue-only (nav.rs:5137-5147). |
| **MusicBee** | Queued items numbered in place; shuffle by album / by artist; repeat album; stop after current; Auto-DJ refill; queue saveable as a playlist. | **Little of it.** Shuffle is tracks-only, repeat is off/all/one, and the other four do not exist. |

Cinder is **ahead** of all three in three places. Nothing below may cost them:

- `queue_play_at` (nav.rs:5349) — tapping the third queued row plays it and the two above it
  **follow**. Spotify discards them.
- `unshuffle_context` (nav.rs:5543) — shuffle off restores the real order and keeps the audible
  track. Spotify cannot go back at all.
- `history` (nav.rs:1107) — a genuine listen log, 100 deep, persisted, consecutive repeats
  collapsed. Neither Spotify nor Apple shows one in the queue view.

---

## 1a. Bugs found while mapping this

Defects in what ships, not gaps in the design. Two are fixed; the rest are open and none of them
needs any refactor.

**FIXED 2026-09-22**

- ~~Ordered play obeyed a sticky `np.shuffle`~~ — one press of a Shuffle band left every album
  tapped afterwards shuffled, across reboots, with nothing on screen to explain it. Ordered-play
  paths now call `play_in_order` (lib.rs:2142). `apply_shuffle` became dead code and was deleted,
  which is the proof that every ordered path had been going through it.
- ~~Every Shuffle band destroyed the user's queue with no prompt~~ — the four bands returned their
  action directly and skipped `start_play_action` (nav.rs:1687), the funnel that raises the
  "you have a queue" card. On the Playlists tab the PLAY band asked and the SHUFFLE band beside it
  did not. All four now route through the funnel;
  `every_shuffle_band_asks_before_it_clears_the_queue` covers it.

**OPEN, cheapest first**

1. **The CLEAR chip is not confirmed.** nav.rs:3125-3129 clears immediately, `notify` only. A
   comment at nav.rs:5341 asserts the opposite about its own code ("the CLEAR chip beside it …
   has both"). Either gate it or delete the false comment; do not leave both.
2. **Phantom queue rows.** A pick whose file has been deleted is dropped from the sequence by
   `play_order`'s `.flatten()` (lib.rs:2360) but stays in `queue` and in Up Next for ever, because
   `track_started` is the only consumer and nothing ever starts it. It keeps inflating the queue
   count and keeps triggering the `QueueOnPlay` prompt. Tapping it reorders the queue and plays
   nothing (lib.rs:2831). Queueing the **currently playing** track does the same, via the
   no-adjacent-duplicates rule.
3. **Repeat-all replays picks every lap.** `cinder_repeat_all_prepare` (lib.rs:3758) builds
   `queue + WHOLE context` with `lead = None`, ignoring `playing_pick` — so a surviving pick plays
   again at the head of each lap and the lap restarts from the top of the context rather than
   continuing.
4. **"Shuffle all songs" only ever plays 512 tracks.** `set_pending` truncates to
   `MAX_PLAY_SEQUENCE` **before** the sequence becomes the context (lib.rs:2242), so on a
   3355-track library the other 2843 never play and the persisted `ctx=` holds 512 ids. The shell
   caps it again independently in a fixed `static char bufs[512][512]`
   (cinder-home/src/main.cpp:8125). Raising either needs a device measurement —
   `SetTrackSequence` is known flat at ~0.25 s only up to 512. The cheap half is to make the cut
   **visible** rather than an `eprintln!`.
5. **`NEXT FROM` lies after a shuffle-all.** The heading is the *current track's* album
   (nav.rs:6553) over a tail that is the whole (truncated) library.

---

## 2. Why one flat list was rejected

The rejected design gave every row an `Origin` (`Pick` or `Ctx(id)`) and put them all in one
`Vec<Entry>`, deriving the section headings from runs. It is what Apple Music and MusicBee are, and
on paper it expresses everything in §3. Three findings kill it:

1. **The accessors return borrowed slices.** `context()` (nav.rs:5407), `queue()` (nav.rs:5569) and
   `history()` (nav.rs:5289) are all `-> &[SongRow]`. A run of `Entry` is contiguous but its
   elements are not `SongRow`, and picks interleaved with context rows are not contiguous at all.
   There is no borrow that produces either.
2. **The owning alternative is a measured regression.** Returning `Vec<SongRow>` reintroduces
   exactly the clone removed at nav.rs:6515-6526: *"499 µs a frame on the host (~7 ms on device)
   and about 15,000 string allocations … the largest allocation churn left in the app, on a heap
   whose churn has already caused one on-device allocator abort."* `play_order_uris`,
   `PlayContextAt` and `cinder_repeat_all_prepare` each clone the whole context, on the render
   thread under the global mutex — the shape of the 2026-08-18 device freeze.
3. **`object_id` stops being unique.** Both reconciliation paths key on it: a pick is *found in
   `queue` and removed* (nav.rs:5445), and a context move uses a strict `idx + 1` rule that exists
   because searching caused a real bug (nav.rs:5456). One list with duplicates has neither.

Two smaller ones worth recording: `metrics()` is deliberately O(1) in sequence length
(up_next.rs:236) and is called on the render path — deriving section boundaries by walking a
3355-entry list would put that back to O(n) per frame. And `up[..up_idx]` would become a second,
conflicting history, regressing `history` to the "earlier in this album" list it was built to
replace (up_next.rs:155).

**The rule this leaves behind:** the queue and the playing sequence stay separate lists, and no
accessor on the render path may allocate per row.

---

## 3. The model: `context` becomes a chain

One change, and it is additive:

```rust
pub struct Run {
    /// Stable across reorders. What `pre_shuffle` and the label key on — NOT the position.
    pub id: u32,
    /// "ABBEY ROAD", "MY PLAYLIST", "ALL SONGS". Set by the play path that created the run.
    pub label: String,
    pub rows: Vec<SongRow>,
}
```

```rust
context: Vec<Run>,      // was Vec<SongRow>
run_idx: usize,         // which run is playing
context_idx: usize,     // index inside THAT run — meaning unchanged
queue: Vec<SongRow>,    // unchanged
history: Vec<SongRow>,  // unchanged
playing_pick: Option<SongRow>,  // unchanged
```

Today's state is the chain with exactly one run, so every existing behaviour is the `len() == 1`
case.

**The slices survive.** `context()` becomes `&self.context[self.run_idx].rows` — still a borrow,
still no allocation. `context_idx()` and `queue()` are untouched. That is the whole reason to
prefer this shape: the hot path does not change.

Sequence handed to PlayerService:

```
[current] + queue + cur_run[context_idx+1..] + runs[run_idx+1..].flat_map(rows)
```

One extra chained iterator in `play_order_uris`. No clone, no per-row query.

### What it buys

| Want | How |
|---|---|
| **Play album B after album A finishes** | push a `Run` onto `context` |
| Chain A → B → C | push two |
| `THEN ‹B›` headings under `NEXT FROM ‹A›` | one section per run |
| Repeat **album** | lap `context[run_idx]` |
| Un-shuffle only the album you shuffled | `pre_shuffle` keyed by `Run::id` |

### What it does NOT buy, and must not be sold as buying

**Shuffle by album/artist is orthogonal to this.** After "Shuffle all songs" the context is one run
of 3355 rows with no album structure, so there are no runs to permute. It needs grouping by
`SongRow::album_id` (which exists, model.rs:13) — a grouping over row fields, not over runs.
Shuffle-by-artist additionally needs an `album_artist` field that `SongRow` **does not have**; the
Artists tab groups on album-artist-with-artist-fallback, so grouping on `artist` would split
compilations and shatter "Various Artists" albums. Add the field first.

---

## 4. Control surface

### Row gestures

| Gesture | Action | Position |
|---|---|---|
| swipe LEFT | Play next | `queue.insert(0, …)` — **unchanged**. A second "play next" jumps the first; `swiping_a_row_queues_next_or_later` (nav.rs:9134) asserts it and that is the intended distinction. |
| swipe RIGHT | Play later | `queue.push(…)` — unchanged |
| **long press (new)** | sheet | Play next · **Play after this album** · Play last · Add to playlist |

"Play after this album" is the one new position and the owner's named scenario. The *mechanism* is
already half-built: `movable_move` clamps a drop at `.min(self.context.len())` (nav.rs:4885), so a
pick dragged to the bottom of Up Next already lands after the last context track. What does not
exist is doing it in one action, or doing it to a whole album — dragging twelve tracks down one row
at a time is not a gesture anyone will use.

**Single track vs album.** A queued *track* stays a `queue` pick. A queued *album* mints a `Run`,
because only a run can be repeat-album'd, un-shuffled or labelled as a unit. `enqueue_album_at`
(nav.rs:5225) currently splits an album into N individual picks; that becomes run creation.

### Shuffle becomes a mode

```rust
pub enum Shuffle { Off, Tracks, Albums, Artists }
```

- **Off** — `unshuffle_context`, already built.
- **Tracks** — `queue_shuffle`, already built; permutes the current run's tail only.
- **Albums / Artists** — group the run's rows by `album_id` / `album_artist`, permute the groups,
  keep each group internally ordered. Needs the field additions above.

`queue` picks are never shuffled. That is already the rule; it stays.
A **RESHUFFLE** chip re-rolls inside the current mode.

### Repeat gains one position

```rust
pub enum Repeat { Off, All, One, Album }
```

Off / All / One exist (lib.rs:3294; All is shell-driven off the queue-boundary signal measured
2026-08-26). **Album** laps the current run.

⚠ `cinder_repeat_all_prepare` reads `context()`. Once that means *the current run*, repeat-**all**
silently becomes repeat-album and drops the rest of the chain. It compiles, it runs, and with one
run it is identical — so nothing catches it until chaining ships. It must be changed to walk the
whole chain **in the same commit that introduces runs.**

### Three cheap wins, independent of all of the above

Do not use these to justify the refactor; they need none of it.

1. **Stop after current track.** A chip on Now Playing. A bool and a check at the boundary.
2. **Save queue as playlist.** The only route into `PlaylistPick` today is Track Info's "Add to
   playlist", one track (nav.rs:3083).
3. **Autoplay refill.** When the sequence empties, append N more. Cinder can do this **offline**:
   SensMe channels are in `MTPDB` and artist/album grouping is in `cinder-db`.

---

## 5. Migration

`self.context` is used **35** times in nav.rs and `self.queue` **38**; none go through an accessor.
The context ones become `self.context[self.run_idx].rows`, which is mechanical but must be read
line by line, because several are `remove`/`insert`/`sort_by_key` on the vector itself.

### Things that are not derivations and need a decision each

| Site | Why it needs one |
|---|---|
| `set_play_context` (nav.rs:5377) | Must say whether it replaces the chain or appends a run, and where kept picks sit. It also decides whether `if r.app.queue().is_empty()` (lib.rs:2267) still picks the right branch — i.e. whether the "keep queue" fix still holds. |
| `boundary_preempts_picks` (lib.rs:2340) | Gates on `idx_after == idx_before + 1`. Across a run boundary that is `len-1 → 0`, so it stops firing and the 2026-09-11 "plays the album track, then the queue" bug returns over Bluetooth. It must take an absolute position in the flattened sequence. |
| `cinder_repeat_all_prepare` (lib.rs:3758) | See the warning in §4. |
| `note_pre_shuffle` / `playback_restore` (nav.rs:2164, 2190) | Both guard on `ids.len() == self.context.len()`. Against a run they either reject a valid order — making shuffle-off a one-way door again — or accept another run's order that happens to match in length and reorder the wrong album. `pre_shuffle` becomes keyed by `Run::id`. |
| `movable_move` (nav.rs:4849) | Its safety argument is that nothing it touches is at or before `context_idx`. Restate it per run, and define what dragging a row from run B into run A does (it joins A). |
| `set_context_playing` (nav.rs:5456) | With a chain, the same `object_id` can appear in two runs. Search the current run first, then forward. |

### Persistence

`queue.conf` is `ctx=` / `idx=` / `q=` / `pick=` / `pre=` / `hist=` (nav.rs:2101). v2 adds runs.
A v1 file loads as **one run**: `ctx=` → `Run { id: 0, label: "", rows }`, everything else
unchanged. `pick=` must keep working — it exists for a named bug (a reboot mid-pick came back on
the track *before* it) and `playing_pick` is not going away in this design.

### Testing

**P1 is not provable by the existing suite.** The `boundary_preempts_picks` tests (lib.rs:7306)
call the pure function with hand-written integers and never touch `App`; they pass unchanged while
the caller stops detecting the case. `metrics_matches_layout` (up_next.rs:882) sweeps `metrics()`
against `layout()`, not against the app. New tests must drive `App` end-to-end across a run
boundary and across a playing pick.

---

## 6. Phasing

| Phase | What lands | Cost |
|---|---|---|
| **P0** | The open bugs in §1a — none need runs | small, do first |
| **P1** | `Run`, `run_idx`, derived `context()`, persistence with v1 fallback, **plus** the `repeat_all` and `boundary_preempts_picks` fixes in the same commit | the 35 sites + new end-to-end tests |
| **P2** | "Play after this album" + `THEN ‹B›` sections | **the owner's named scenario** |
| **P3** | `up_next.rs` N-section layout — `Slot`/`Section`/`metrics`/`layout`/`movable_*` all assume exactly one album section. ~600 of its 1046 lines, plus ~150 in nav.rs. Cache the run table on `App`; do not walk `up` per call. | its own phase — it cannot ride along in P2 |
| **P4** | Swipe-to-remove on context rows (Apple parity; drag already works) | small |
| **P5** | `album_artist` on `SongRow`, then shuffle modes Albums/Artists | medium |
| **P6** | Repeat-album, Stop after current, Save queue as playlist | small, independent |
| **P7** | Autoplay refill | medium |

P0 first. P1+P2 is the smallest thing that answers the original ask.
