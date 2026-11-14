# Changing the queue without restarting the track

2026-10-11, Walkman One 3.02 (`libPlayerServiceClientUtil.so`, the same code as stock A50 1.02 by
symbol). Measured on one player.

## The problem

Cinder changed the queue by building a new `NodeTrackSequence` and calling
`PlayController::SetTrackSequence`. That call reopens the player: the track restarts, and Cinder
seeked back to where it was. To hide the restart, edits were held until a track boundary, where
the same hand-over cut into the first second of the next song. That was the stutter.

## What Sony's code does

`SetTrackSequence` sends only a handle. PlayerService never holds the list: it calls back into the
client's `NodeTrackSequence` (`Alloc`, `AllocNext`, `AllocPrev`, `OnNextTrack`, `OnPrevTrack`) each
time it needs a track. So the list can change under it.

`NodeTrackSequence<UriInfo>` layout, from the disassembly:

| Offset | Field |
|---|---|
| +0x08 | root `Node*` |
| +0x0c | current path, a `vector<int>` of child positions |
| +0x18 | id of the current node |
| +0x1c | `recursive_mutex`, taken by every callback and by `EditNode` |
| +0x48 | the `function<void(UpdateReason,int)>` given to the constructor |

`Node<UriInfo>` is 32 bytes: +0 pointer to its child container (a `vector<Node>` first), +8 parent,
+0xc id. Ids come from one global counter (`UpdateId` @0xb3dd).

`EditNode(function<int(Node&, int)>)` @0xbfcd is Sony's own in-place edit:

1. Lock the mutex.
2. `EditNodeImpl` @0xc111 notes the current node's id, then calls the function with the root.
3. It finds the current node again by that id (`GetNodeByIdInternal`). If the id is gone it falls
   back to item 0.
4. It rebuilds the current path, then calls the constructor's callback. No IPC call is made.

`MoveTo(int)` @0xbdf5 and `GetCurrentIndexes` @0xbe9d are local bookkeeping too.

## What Cinder does now

`cinder_audio_edit_sequence` (`cinder-audio/src/player_shim.cpp`) builds the new tree from JSON as
before, then inside `EditNode`:

1. `root = std::move(*fresh)` (`Node::operator=(Node&&)` @0xa6cd).
2. `root.ConnectToParent(nullptr)` @0xb38d. Without it the moved children still point at the source
   node, and `GetPathInternal` walks parent links to null: SIGSEGV at +0xc, seen on the first try.
3. Write the old current id into the new node at the playing track's position, so step 3 above
   finds it and the sequence never points at another track.

`play_pending_sequence` uses this when the track at the new list's start is the one playing, and
falls back to `SetTrackSequence` otherwise.

## Measured

| Check | Result |
|---|---|
| Queue a song 12 s into a 29 s pink-noise track, jack recorded | no 10 ms window more than 6 dB under the median: no dropout |
| Stream state across the edit | `RUNNING` throughout, no `SetTrackSequence` in the log |
| Next track at the boundary | the queued song |
| Two songs queued in a row | played in the order queued |

## Not measured

- An edit made in the last second or two of a track. If PlayerService has already asked for the
  next track, that boundary may still use the old one.
- Bluetooth output, and a queue edit while shuffle is on.
- Whether the boundary itself is gapless: that is Sony's path and was not changed.

## Timing (2026-10-12)

The edit is issued the moment the queue changes, not held for the last 2.5 s of the song. Held
that late, PlayerService had already picked its next track: the old next song started and was
corrected a second or two in (owner's report, a day of use). If the in-place edit is refused,
`cinder_queue_edit_refused` holds it for the end of the song, so the player is never reopened
mid-song. Measured: an edit about 40 s into a song logged `edit in place: n=12 start=0` at once
with the stream RUNNING. Not measured: the boundary after it.

## The sequence's callback: an early "moved to the next song" (2026-11-14)

The `function<void(UpdateReason,int)>` given to the constructor is called on a binder thread in
Cinder's own process. Timed on stock A50 1.02 with two 15 s songs, the jack recorded, every signal
stamped with `CLOCK_MONOTONIC`:

| Event | Callback | When |
|---|---|---|
| Song ends, the next starts (gapless) | reason 2, int 0 | As the new song's first samples reach the sound service: 0.24 s before `libcinder_mono.so` sees them, 1.24 s before they are heard |
| Next button | reason 2, int 0 | 0.2 s before `onPlayTimeUpdated(0, …)` |
| In-place edit (`EditNode`) | reason 0, int 0 | At the edit |
| Last song of the list near its end | reason 5, int 0 | 0.25 s before the last samples pass |
| Previous button, a new `SetTrackSequence` | none | The shell starts those itself |

`onPlayTimeUpdated` for the new song arrives one second after reason 2: the jack's ALSA buffer
(`hw:0,4`, 44.1 kHz) is 44104 frames and stays full, so what the service reports is what is heard.
The shell's track-change poll runs off that report, which is right for the screen and a second
late for anything applied to the samples.

ReplayGain uses reason 2 (`on_sequence` in `cinder-audio/src/player_shim.cpp`). Measured over five
changes: the new song's level is in place within 0.15 s of its first sample; it was 0.98 s.
Not measured: other sample rates, Bluetooth.
