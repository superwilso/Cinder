# Queue system audit — 2026-09-08

Prompted by: *"skipping … not starting from the start of the next song, or jumping what would be
the next song excluding the queue only to skip it and go to the queue."*

Every path that builds, edits, defers or consumes the user queue was read end to end. Four defects,
three of them the same shape: **a sequence handed to PlayerService that does not contain the queue,
with nothing left owing a flush that would put it there.**

## How the queue actually works

PlayerService has no insert. The only way to change what plays next is `SetTrackSequence`, which is
a pause/seek/play round trip measured at 360–450 ms and **restarts the music**. So a queue edit is
not applied when it is made. It is recorded (`Action::QueueChanged` → `queue_pending`) and deferred
to a moment where re-issuing is free:

* **the early rebuild** — 2.5 s before the current track ends (skipped on Bluetooth, where the
  round trip disrupts a stream the sink is buffering; except on the last track of the sequence,
  where there is no boundary coming and silence is worse than a glitch);
* **the track boundary** — the new track's position is ~0, so the reset is inaudible.

The sequence itself is assembled by `play_order_uris` → `play_order`: `[current] + [user picks] +
[context after the current track]`, with **no two adjacent entries naming the same file**.

## D1 — a skip never consulted the queue *(the reported bug)*

Both deferral points happen when a track **runs out**. A skip is neither: it changes the track
immediately, nowhere near the 2.5 s window, so the deferred edit was still sitting there and
`PlayController::NextTrack()` advanced inside the sequence PlayerService was given *last* time —
the one with no queue in it.

The user got the album's next track: the exact track the queue was meant to come before. The queue
then took over at the boundary after it. That is "it jumps to what would be the next song excluding
the queue, only to skip it and go to the queue", precisely.

**Fixed.** `cinder_prepare_skip_play()` stages `[queue…] + [context tail]` and the shell plays it at
index 0 instead of calling `NextTrack`. Re-issuing here is free for the same reason a boundary is:
the current track is being abandoned on purpose, so there is no playback to protect. It returns 0 —
one `NextTrack`, no rebuild — whenever the live sequence already leads with the queue, so ordinary
skipping through an album is unchanged. The queued track starts at 0 (`restore_position = false`),
which is the other half of the report.

## D2 — "Keep queue" kept the picks on screen but not in the sequence

`set_play_context` clears the user's picks unless the "Keep queue?" answer preserved them. When it
preserved them, `set_pending` still built the sequence from the **context alone** and then set
`queue_pending = false` — so no flush was ever owed. The kept picks sat in Up Next looking queued
and could not play, for the rest of the session, unless the queue happened to be edited again for
some unrelated reason.

**Fixed.** When the queue survives, `set_pending` rebuilds through `play_order_uris`: the track the
user just asked for plays now, their picks follow, then the rest of the new context. The empty-queue
case — the ordinary one, and the "Clear queue?" answer — takes the old path untouched.

## D3 — the resume path could emit the same file twice in a row

`cinder_resume_load` assembled `[current] + queue + context[idx+1..]` by concatenation, bypassing
`play_order` and therefore the no-adjacent-duplicates rule. A boot whose saved context is empty
resumes on `queue.first()`, which makes the leading track *also* the head of the queue: `[Q1, Q1,
…]`. PlayerService plays it twice, the URI does not change at that boundary, so no track start is
reported, `App::track_started` never runs, the pick is never consumed out of the queue and comes
back on the next flush.

That is the identical defect `play_order` was written for; this path just never went through it.
**Fixed** — same assembly as everything else now.

## D4 — a skip inside the early-rebuild window could jump backwards

The early rebuild stages a sequence and raises `queue_flush`; the shell issues it on the next
housekeeping pass. A skip landing in that window called `NextTrack` against the sequence
PlayerService still held, and the staged flush was then issued on top **starting at index 0** — the
track just skipped away from. Playback would jump backwards a track.

The window is short (poll and flush are the same housekeeping pass, milliseconds apart) which is
presumably why it was never reported.

**Fixed.** A staged-but-unissued `queue_flush` counts as deferred work, so the skip path takes over
and clears it.

## Also changed: a boundary re-issue that changed nothing

The commonest boundary is a **user pick starting**: `track_started` consumes it out of the queue and
asks for a flush, but the sequence already running — issued 2.5 s earlier by the early rebuild — has
exactly this remainder after the track that just began. Re-handing it produced an audible restart at
the front of *every* queued track, for no change at all.

The boundary flush now compares what it would send against the tail of the last sequence handed
over and skips the round trip when they match — but only when no flush is still staged, because
then `pending_play` is a sequence PlayerService has never seen and matching against it would drop
the edit on the floor.

## Checked and found correct

| path | verdict |
|---|---|
| `App::track_started` | correct, including the "pick that is also the next context row" exception |
| `play_order` adjacent-duplicate rule | correct; queueing the song you are listening to collapses, and the pick is consumed at the next boundary |
| `Action::PlayQueueAt` | correct — leads with the tapped row, keeps the rest queued, leaves the context alone |
| `Action::PlayContextAt` | correct — no re-shuffle, which is the 2026-09-02 fix |
| `cinder_repeat_all_prepare` | correct — a lap leads with the queue, and clears what is owed |
| `queue_shuffle` | correct — only the remainder moves; picks and history are left alone |
| queue persistence (`ctx`/`q`/`idx`/`pick`/`pre`) | correct — the playing pick is saved separately, since a pick leaves the queue when it starts |
| `MAX_PLAY_SEQUENCE` (512) truncation | safe — the queue leads, so truncation can only drop the far context tail |
| Bluetooth deferral | correct — the early rebuild is skipped except on the last track, where silence would otherwise win |

## Not fixed, recorded

`App::queue_play_at(n)` does `queue.drain(..n)` — tapping the third queued row **discards the two
above it**. That reads as deliberate ("skip to this one") and it is what the row tap has always
done, but it is the one destructive action on this screen that has no confirmation and no undo,
unlike the "Clear the queue" chip which has both. Worth a decision, not a silent change.
