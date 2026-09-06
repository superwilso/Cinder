# Audit — the queue and playback system (2026-09-06)

A full read of everything between "the user asks for a song" and "PlayerService is holding a
sequence": `cinder-ui`'s queue/context model (`nav.rs`, `up_next.rs`), the resolution and flush
layer (`cinder-ffi/src/lib.rs`), the shell's transport and end-of-queue watcher
(`cinder-home/src/main.cpp`), and the shim that owns the `NodeTrackSequence`
(`cinder-audio/src/player_shim.cpp`). Audited against `d9bcea0`.

**Twelve defects, all twelve fixed here.** Nine are host-testable and now have tests; three are
device-facing and are pinned by three new harness scenarios that FAIL on the code before this
change. Nothing found in this audit needs a device session to close.

Gates after these changes: **347 `cinder-ui` tests** (up from 342), **88 `cinder-ffi`**, 21
`cinder-db`, the 8-case UI overflow matrix, and **30 harness scenarios** (up from 27).

---

## The shape of the system, as it actually is

Worth stating plainly, because most of the defects below are two parts of it disagreeing.

* **`App::context`** is what is playing — the album, playlist or shuffle scope the shell resolved
  when the user started it — with `context_idx` pointing into it.
* **`App::queue`** is the user's own swipe-queued picks. They play FIRST, then the context resumes.
* PlayerService has **no insert and no reorder**. The only way to change a sequence is
  `SetTrackSequence` with a whole new one, which costs a measured 360–450 ms pause/seek/play cycle.
  So a queue edit is recorded in-process (`queue_pending`) and flushed at a **track boundary**,
  where re-issuing resets a position that is already ~0 and nothing is audible.
* The order handed over is `[the track playing] + queue + context[idx+1..]` (`play_order_uris`).
  Leading with the current track is what keeps a mid-track re-issue from restarting it.
* The **only** signal that a track started is the URI changing
  (`poll_now_playing` → `cinder_set_now_playing_uri` → `App::track_started`). Several defects below
  are that sentence's consequences.

---

## The defects

### 1. Right-swiping a track on the album page queued it and never told the shell

`nav.rs`, `swipe()`, `Screen::Album if dir > 0`. The arm called a wrapper — `enqueue()` — which
**discarded the `Vec<Action>` its callee returned**, and then returned `vec![]`:

```rust
if let Some(s) = song { self.enqueue(s, y); }
vec![]                              // ← Action::QueueChanged thrown away
```

So the track went into the UI's queue, the "Added to queue" toast popped, Up Next showed it — and
nothing ever set `queue_pending`, so no flush was scheduled and PlayerService never learned about
it. The track played wherever the *old* sequence said, not where the screen did. It only took
effect if some later queue edit happened to flush.

The mirror gesture one arm up (`dir < 0`, Play Next) always returned the action, which is exactly
why this was invisible: the two gestures are symmetric and only one of them worked.

**Fixed:** the arm returns `enqueue_at(s, y, QueueAt::Later)` like every other queue path, and
`enqueue()` — which existed only to lose the return value — is gone.
Test: `both_album_page_swipes_tell_the_shell_the_queue_changed`.

### 2. Up Next named the *previous* song for the whole of a swipe-queued one

Two rules that are each correct, and are wrong together:

* a pick is removed from `queue` the moment it starts (`track_started`) — that is what stops it
  replaying on the next re-issue;
* a pick must **not** move `context_idx` — "play this next, then carry on where I was" is the whole
  promise of the queue, and the index is what "where I was" means.

Between them, nothing in the app held the playing pick. `Screen::UpNext` asked the context what was
playing (`cur = Some(self.context_idx)`), and that still named **the track the pick interrupted**.
So for the entire duration of a swipe-queued song, the screen drew NOW PLAYING over the song that
had just finished, and the pick itself had already vanished from NEXT IN QUEUE.

The same hole reached the resume files: the playing pick is in neither saved list, so a power cycle
mid-pick came back on the context row underneath it.

**Fixed:** `App::playing_pick` holds it, a new `up_next::Slot::CurrentPick` gives it the NOW
PLAYING row, the context row it interrupted joins the history above it, and `playback_encode`
writes `pick=<id>` (resolved and restored by `cinder_resume_load`, which now leads the resume
sequence with it).
Tests: `a_playing_pick_is_the_now_playing_row_not_the_track_it_interrupted`,
`a_playing_pick_survives_a_reboot`, plus `pick` swept through
`metrics_matches_layout` and `at_matches_a_linear_scan_everywhere`.

### 3. Pressing SHUFFLE on a paused player started the music

`carry_action`, `Action::ShuffleToggle`. The toggle defers its re-issue to a boundary while
playing, and took an "immediate path" when not:

```rust
if r.np.playing { r.queue_pending = true; return None; }
r.pending_play  = play_order_uris(r, &current);
r.queue_flush   = true;
return Some(36);                    // → CINDER_ACT_QUEUE_CHANGED
```

The shell answers 36 with `play_pending_sequence`, which ends in `ChangePlayState(Play)` and
`set_transport(true)`. Shuffle is not a transport control, and on a device carried in a pocket a
control that begins playing when nobody asked is the worst kind of surprise. It is reachable the
moment you turn the player on: boot, tap the shuffle icon, and it starts.

**Fixed:** the not-playing branch banks `queue_pending` like the playing one. It costs nothing —
`queue_shuffle` and `unshuffle_context` both leave the current track alone by construction, so the
reordered tail only has to be in place before PlayerService reaches it.

### 4. Shuffle-off was still a one-way door after a Library "Shuffle …" band

`note_pre_shuffle` exists so that turning shuffle OFF restores the running order, and five of the
six shuffle entry points call it (`PlayIndex`, `PlayPlaylist`, `PlayPlaylistAt`, `ShufflePlaylist`,
`ShuffleArtist`). `Action::Shuffle(scope)` — the four **Library bands**, i.e. the likeliest way
anyone starts a shuffle at all — did not. So: press "Shuffle all songs", then press the shuffle
icon to turn it off, and the icon went dark while the sequence stayed permuted for the rest of the
session. That is the same "the control says one thing and the player does another" class the toggle
was fixed for on 2026-08-18.

**Fixed:** `shuffle_tracks` now returns `(sequence, pre_shuffle_ids)` for every scope — the scope's
own natural order (title order for All Songs, album order for By Album, the artist's catalogue
order, the playlist's saved order) — and the caller hands it to `note_pre_shuffle`.
Test: `shuffle_scopes_resolve_to_real_tracks` now asserts every scope reports an order that
describes exactly the sequence it handed back (a mismatched length is silently refused by
`note_pre_shuffle`, so a wrong one would have been invisible).

### 5. The shuffle band ignored the Hi-Res filter while its caption named it

The Songs-tab band's caption is built from `Library::filter_name()`
(`library.rs:1088`), which composes **both** filter axes — "Shuffle Rock", "Shuffle Rock · Hi-Res",
"Shuffle Hi-Res". `Action::Shuffle` filtered on `filter_genre` only. Hi-Res was added later as an
independent second axis and this arm was never taught about it, so with "Shuffle Hi-Res" on the
glass the band shuffled the whole library. On the reference device that is the difference between
**1 track and 3,463**.

**Fixed:** the filter is a predicate built from `Library::passes` — the same one every filtered
*list* asks — and it is applied **inside** `shuffle_tracks`, before the random artist or playlist is
picked. Filtering afterwards would mean the band silently does nothing whenever it lands on a scope
the filter empties, and would break the pre-shuffle order from §4. A filter that leaves nothing
declines (and says so) rather than falling back to the unfiltered library.
Test: `a_shuffle_band_plays_only_what_the_filter_leaves`.

### 6. Repeat-all looped a truncated sequence after any queue edit

`cinder_audio_restart_sequence` replays `g_last_uris` — the URI list the **shim** last handed over.
That is not the context whenever a queue flush has happened, because a flush hands over
`[current] + queue + context[idx+1..]`. Swipe-queue one song at track 5 of a twelve-track album and
every subsequent lap played 5–12; **tracks 1–4 never played again.** The same applies after a
shuffle toggle, which flushes through the same path.

**Fixed:** `cinder_repeat_all_prepare()` builds the lap from Cinder's own state — anything still
queued, then the whole context from the top — into the ordinary pending-play channel, and the shell
plays that. `cinder_audio_restart_sequence` is kept as the fallback for the case where the UI has no
context to rebuild from (a sequence started before a library reload), where replaying the tail beats
stopping.
Scenario: `repeat-all`.

### 7. Repeat-all fired on a pause near the end of *any* track

The queue-end signal is the shape measured on 2026-08-26 (DEVICE_TESTS.md 3f): position pinned at
the duration, `playing` gone 1 → 0, URI unchanged. The watcher tested exactly that:

```cpp
const bool at_end = cur >= tot - 1500 && cinder_audio_is_playing() == 0;
```

**A pause inside the last 1.5 s of a track makes the identical shape.** So pausing a second before
the end of track 3 of 12, with repeat-all on, restarted the whole queue from track 1 — the device
appearing to jump backwards on its own.

**Fixed:** two more gates.
* `cinder_on_last_track()` — only the final entry of the sequence PlayerService was given can be a
  queue end. (`pending_play` is the sequence as handed over and is not rewritten as playback
  advances, so its last entry is that track.)
* `!g_user_paused` — and on the final track, a pause is still a pause. `g_playing` cannot answer
  this: the now-playing poll overwrites it with the service's own view a few seconds after every
  press, so by the time the watcher runs, a deliberate pause and a queue that ran out are identical.
  `g_user_paused` is written only by `set_transport`, so it survives the poll.

Scenarios: `repeat-all-mid` (the middle-track pause, which restarts the queue on the old code) and
`repeat-all-off`.

### 8. A queue edit owed against a replaced sequence still fired

`Hit::ClearQueue` (the "you have a queue — clear it?" answer) emits `QueueChanged` **and then** the
play action, and `cinder_tap` carries both. `QueueChanged` sets `queue_pending`; the play action
calls `set_pending`, which rebuilds everything — and left `queue_pending` standing. It then fired
2.5 s before the end of the **first track of whatever the user had just started**: a
`SetTrackSequence` + seek, the measured 360–450 ms round trip, to install a sequence identical to
the one already playing. Down the jack that is a glitch near the end of a song; the same trade the
BT lead-in avoidance already refuses to pay.

**Fixed:** `set_pending` clears `queue_pending` (and `resume_stale`, below) — it has just rebuilt
everything the flush would have rebuilt.

### 9. A queue edit made before the first ▶ after a boot was dropped

`cinder_resume_load` snapshots a URI list at boot; `cinder_resume_take_pending` hands that snapshot
over on the first ▶. Anything the user did in between — swipe-queued a song, reordered the queue,
pressed shuffle — happened to `App`, not to the snapshot. So the first press played the sequence as
it was at boot, and the edit only appeared at the boundary after it.

**Fixed:** `mark_queue_pending()` (one place, two callers) marks the armed resume stale, and
`cinder_resume_take_pending` rebuilds from live state when it is. Rebuilding costs a library query,
which is why it happens at the press and not at the edit: an edit made before the first ▶ is exactly
the case where there is nothing audible to be late for.

### 10. A queued copy of the currently-playing track could never be consumed

`play_order_uris` leads with the current track and then appends the queue. Swipe-queue the song you
are listening to and the order becomes `[A, A, …]`. PlayerService plays A twice — and **the second
copy does not change the URI**, which is the only thing the shell reports a track start on. So
`App::track_started` never ran, the pick was never consumed out of the queue, and the next flush put
it back. A phantom Up Next row, and a song that played twice, every time, for ever.

**Fixed:** the ordering rule is now a pure function, `play_order`, which drops an entry that is
identical to the one immediately before it. **Only adjacent ones** — queueing the same song twice
with something between them is a thing people do deliberately, and there the URI does change at each
boundary, so each copy is reported and consumed exactly as it should be. Collapsing those would be
silently refusing an instruction rather than fixing a defect.
Test: `the_play_order_never_repeats_a_file_back_to_back`.

### 11. Tapping a queue row silently emptied the queue

`Action::PlayQueueAt` went through `set_pending` → `set_play_context`, which is the "the user
started something new" path: it makes the sequence the **context** and clears the queue. Everything
played in the right order, so nothing looked wrong — what actually happened is that the user's
hand-built picks stopped being picks. NEXT IN QUEUE and its CLEAR chip disappeared, and the next
album tapped anywhere in the app replaced them **with no "you have a queue" prompt**, because there
was no longer a queue to ask about.

**Fixed:** `App::queue_play_at(n)` drops the picks ahead of `n` (skipping past them is what the tap
means) and leaves the rest queued and the context untouched; the FFI hands over the same order any
other flush would.
Test: `tapping_a_queue_row_keeps_the_queue_a_queue`.

### 12. MIX dealt the same hand after every boot

`queue_shuffle` seeds a small xorshift from `shuffle_seed`, which starts at a constant and is never
persisted or re-seeded. So the Nth press of MIX in a session always produced the Nth permutation of
the same generator: press it once on the same album after two different boots and the "random" order
was **identical both times**. (`apply_shuffle`, used by the library bands, seeds from the clock and
was always fine — this is the toggle and the MIX chip only.)

**Fixed:** `App::seed_shuffle(u64)`, called once at start-up from `cinder-ffi` with a clock-derived
value. The constant stays as the starting value so that cinder-ui's 347 tests remain deterministic —
the crate has no clock and must not grow one.
Test: `the_shuffle_seed_is_the_shells_to_set`.

### Also, smaller

* **The Menu's "Up Next" subtitle said "Queue empty" over a screen listing twelve tracks.** It
  described only the user queue, while the row opens a screen showing the whole sequence. It now
  reports the picks when there are any (they are what CLEAR empties and what the replace prompt is
  about) and otherwise what is genuinely still to come. Covered by
  `live_menu_subtitles_report_real_state`.
* **The 512-track cap was applied twice and announced once.** `set_pending` truncates a new context
  and logs it, but a flush rebuilds `[current] + queue + tail`, which can be longer than the context
  it came from — and `play_pending_sequence` then cut it back to 512 from a fixed-size buffer in
  silence. The cap now lives in `play_order`, where it can say that Up Next is showing more than
  will play.

---

## What the harness could not see, and now can

The end-of-queue half of `poll_now_playing` — repeat-all, and the transport glyph after a queue runs
out — **was unreachable from any scenario at all.** The generated stub for `cinder_audio_position`
returns a scripted int and does nothing else, which is exactly wrong for a call whose answer is in
its **out-parameters**: `cinder_audio_position(&cur, &tot)` left both at the caller's `-1`, so
`tot > 0` was false in every scenario ever written and the whole branch was dead code as far as the
harness was concerned.

`cinder_audio_position` is now hand-written in `harness.cpp` (and excluded from generation by a
named `HAND_WRITTEN` set, so the exclusion is explicit rather than a silent gap), serving a
scriptable `(position, duration)` pair. `cinder_harness_play_position(cur_ms, total_ms)` is how a
scenario says "parked at the end of a track" — deliberately a fixed pair rather than an advancing
clock, because the states worth asserting on are "at the end" and "in the middle", and a scenario
that has to wait out a track's real length to reach the interesting second is a scenario nobody
writes.

Both repeat-all scenarios were checked against the pre-fix code and **fail on it**:

```
== repeat-all       FAIL the lap is built from Cinder's own context
                    FAIL not the shim's last URI list …          (got 1, want 0)
== repeat-all-mid   FAIL and nothing restarted the sequence …    (got 1, want 0)
```

---

## Verification status

| # | Defect | Fixed | Host-tested | Device-verified |
|---|---|---|---|---|
| 1 | Album-page right-swipe never reached the shell | ✓ | ✓ | — |
| 2 | Up Next named the previous song during a pick | ✓ | ✓ | — |
| 3 | Shuffle toggle started playback while paused | ✓ | — (FFI, needs a `Render`) | — |
| 4 | Shuffle-off a no-op after a Library band | ✓ | ✓ | — |
| 5 | Shuffle band ignored the Hi-Res filter | ✓ | ✓ | — |
| 6 | Repeat-all looped a truncated sequence | ✓ | ✓ (harness) | — |
| 7 | Repeat-all fired on a pause near a track end | ✓ | ✓ (harness) | — |
| 8 | Stale `queue_pending` across a new context | ✓ | — | — |
| 9 | Queue edit before the first ▶ was dropped | ✓ | — | — |
| 10 | A queued copy of the current track was never consumed | ✓ | ✓ | — |
| 11 | Tapping a queue row emptied the queue | ✓ | ✓ | — |
| 12 | MIX permutation identical every boot | ✓ | ✓ | — |

**Nothing here is device-verified**, and three of them (3, 8, 9) are arguments about a `Render` the
host cannot construct — they are read-and-reasoned, not measured. What to look for on the next
device session:

* **§3** — boot, do not press ▶, tap the shuffle icon. Nothing should be audible, and the log should
  NOT contain `queue: apply now` / `play_tracks`.
* **§6/§7** — with repeat-all on: play an album, swipe-queue one track mid-album, let it run to the
  end. The lap should start at **track 1**, and the log should say
  `repeat-all: queue ended — restarting it from the first track` exactly once. Separately, pause a
  second before the end of a middle track: nothing should happen.
* **§9** — boot, swipe-queue a song, then press ▶. It should play after the resumed track, not be
  lost.

## What was audited and found sound

Recorded so the next audit does not re-derive it:

* `App::track_started`'s ordering rule (the pick/context reconcile) and its `idx + 1` special case.
* The boundary-flush lead, its Bluetooth exemption, and the last-track exception added on
  2026-08-19 — all still correct, and §7's `cinder_on_last_track` is that same test extracted.
* The transport banking added 2026-08-31 (`g_skip_pending`, the NET rule, `TRANSPORT_PENDING_MAX`,
  the backlog drop on a new play).
* `cinder_prepare_previous_play` / `prev_means_restart` — the ◁ rewind rule.
* `unshuffle_context`'s rank-and-follow (it follows the audible track, not the index).
* `Layout`/`Metrics` agreement and the three binary searches in `up_next.rs`.
* `OneTrackMode::On == 2` — the repeat-one enum corrected on 2026-08-26 is what the shipping path
  uses; repeat-one is applied before `SetTrackSequence` and on the live path, and both are right.
