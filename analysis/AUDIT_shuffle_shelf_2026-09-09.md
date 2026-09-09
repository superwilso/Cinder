# Audit — shuffle, and the Shelf (2026-09-09)

Prompted by: *"look at the shelfed things and also look at shuffle as I think there are a lot of
bugs there to do with how it interacts with the queue and songs not starting from the beginning."*

**Three defects, all fixed.** Two in shuffle, one in the Shelf. The shuffle pair explains both
halves of the report — a skip after a shuffle plays the wrong track, and a track can be replayed
from the top — and they are the same class as the queue skip bug fixed on 2026-09-08: **a deferred
sequence change that a press or a boundary does not consult.**

## How shuffle actually works

`Action::ShuffleToggle` does **not** hand PlayerService a new sequence. Replacing one costs a
measured 360–450 ms pause/seek/play cycle and is audible under a playing track, so the toggle
permutes `context[idx + 1..]` in-process (`App::queue_shuffle`, or `unshuffle_context` going off)
and raises `queue_pending` — exactly what a queue edit does. Both permute only entries AFTER the
current track, by construction, so the audible song is identical either way and only the tail
differs. The rebuilt sequence is then issued at the next free moment: the early rebuild 2.5 s before
the track ends, or the track boundary itself.

That design is right. Both defects are things that happen *in between*.

## S1 — a skip straight after a shuffle toggle played the OLD order

`cinder_prepare_skip_play` (added 2026-09-08 for the queue) took over a skip only when there were
**picks in the queue**. A shuffle toggle raises the same `queue_pending`, but most people shuffle an
album with nothing hand-queued — so the queue is empty, the function returned 0, the skip fell
through to `PlayController::NextTrack()`, and PlayerService walked the sequence it was given
**before** the shuffle.

Turn shuffle on, press skip, get the next *album* track. Un-shuffling had the mirror of it: skip
still played the shuffled successor.

**Fixed** by dropping the "must have picks" test. With an empty queue `play_order_uris(r, None)` is
just `context[idx + 1..]`, whose index 0 is the next track in whatever order the context holds
**now** — the right answer for a shuffle toggle and a queue edit alike, which is why one test covers
both. A skip with nothing deferred at all still costs one `NextTrack` and no rebuild.

## S2 — a stale early rebuild replayed the track that had just finished

This is the "songs not starting from the beginning" half, and it is worse than it sounds: the song
does not fail to start at the beginning — **the previous one starts again from the beginning.**

The early rebuild fires 2.5 s before the end, consumes `queue_pending`, and stages
`[current] + …` with `queue_flush` raised for the shell to issue. If the track runs out before the
shell gets to it — a slow housekeeping pass, a Sony round trip in the way — then at the track
boundary `queue_pending` is already `false`, so the boundary handler skipped its block entirely and
left the stale sequence standing. The shell then handed PlayerService a list whose index 0 was the
track that had **just finished**. Playback jumped backwards and replayed it.

**Fixed** by making the boundary re-derive on `queue_pending || queue_flush`. Re-deriving costs
nothing when the staged sequence was already right — the `already_live` test drops a re-issue that
changes nothing — and when it was stale this is the only place left that can catch it.

## H1 — the Shelf accepted pins the pin gesture forbids

`shelf_tap` refuses to pin a screen that is not a "place" (`pinnable()` — modals, onboarding, the
lock screen, Tone, the BT codec picker). The **decoder did not re-check**, and the two are not
equivalent:

* `screen_token`/`screen_from_token` cover screens `pinnable` excludes — `tone`, `btcodec`;
* the token table is an **on-disk format** that deliberately outlives any one build, so a pin saved
  when the whitelist differed still decodes;
* `cinder_settings.conf` is plain text a user can edit.

A record naming one of those restored the user into a mode they never asked for — precisely what
`pinnable` exists to prevent. **Fixed**: the whitelist is enforced at both ends, so the rule cannot
be true on one side and not the other.

## Checked and found correct

| path | verdict |
|---|---|
| `App::queue_shuffle` | correct — only `context[idx + 1..]` moves; picks and history untouched |
| `App::unshuffle_context` | correct — restores the recorded order and follows the audible track to its new index |
| `note_pre_shuffle` | correct — refuses an order that does not describe this context, which is what stops shuffle being a one-way door |
| shuffle while a **pick** is playing | correct — `context_idx` names the interrupted row, so the tail still starts at the first unplayed context track |
| `ShuffleToggle` while **paused** | correct — deliberately does not start playback; a control that begins playing unasked is the worst kind of surprise on a pocket device |
| `Action::PlayContextAt` (Up Next row tap) | correct — no re-shuffle, and `restore_position = false`, so a tapped row starts at 0 |
| `cinder_prepare_previous_play` | correct — walks Cinder's own history, `restore_position = false`, and cannot produce adjacent duplicates |
| Shelf pin encode | correct — `|` and newlines are stripped from every label, so a track title cannot corrupt the config |
| Shelf pin restore | correct — resolves album/artist/playlist by IDENTITY, falls back to the stored index, clamps scroll against the current library |
| Back with the Shelf open | correct — closes the overlay; transport and volume still work, other navigation is suppressed |

## Note, not a defect

Pressing SHUFFLE with fewer than two tracks left toasts "Nothing left to shuffle" **and still turns
the indicator on**. Nothing was permuted and no pre-shuffle order was recorded, so the toggle is
inert for the current context — but it is not wrong: `r.np.shuffle` is what the NEXT context is
built with, and that genuinely will be shuffled. The toast and the indicator are answering two
different questions. Left alone deliberately.

## Gates

362 `cinder-ui` tests (up from 361), 89 `cinder-ffi`, 21 `cinder-db`, installer tests, the C/C++
syntax sweep over 23 files, and all 9 C++ self-tests. Built and installed; clean boot, bad-boot
counter cleared.
