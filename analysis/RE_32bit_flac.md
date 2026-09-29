# 32-bit FLAC — why one file stopped all audio

Reported 2026-09-29: playing "Anything" by Sprain stopped the audio, and Cinder showed
**AUDIO STOPPED — RESTART** (`player/cinder-ui/src/chrome.rs`). Nothing else in the library did it.
The file was being played over Bluetooth at the time, but the output is not involved.

## 1. The file

Read from the device 2026-09-29 (the first 256 KB, copied through `/tmp` and pulled):

| field | value |
|---|---|
| path | `/data/mnt/internal/MUSIC/Sprain - Sprain/02 - Sprain - Anything.flac` |
| STREAMINFO | 44 100 Hz, 2 channels, **32 bits per sample** |
| vendor string | `reference libFLAC 1.4.2 20221022` |
| first frame header | `ff f8 c9 ae 00 f3` → sample-size code **7** |

The library database agrees. `object_ext_int` akey 78 is `BITS_PER_SAMPLE` (named in the schema
table, `prop_name`). Across the library it holds 16 for 3 410 tracks and 32 for exactly one: this
file. MTPDB also marks it `is_high_resolution = 1`.

## 2. The decoder

Sony's `libPlayerService.so` (`/system/vendor/sony/lib`) carries its FLAC decoder statically. The
build string is `reference libFLAC 1.3.2 20170101`.

In libFLAC 1.3.2, `read_frame_header_()` (`stream_decoder.c`) maps the frame header's 3-bit
sample-size field like this:

| code | 1.3.2 | 1.4.x |
|---|---|---|
| 0 | take it from STREAMINFO | same |
| 1, 2 | 8, 12 bits | same |
| **3** | reserved → `is_unparseable = true` | reserved |
| 4, 5, 6 | 16, 20, 24 bits | same |
| **7** | reserved → `is_unparseable = true` | **32 bits** |

FLAC 1.4.0 (2022) gave code 7 a meaning — 32 bits — and `flac` writes it for every 32-bit frame. A
1.3.2 decoder marks each such frame unparseable and resynchronises to the next one, which is also
code 7. It never produces a sample, and it never reports an error the player acts on.

## 3. Why the file is accepted at all

The open path in the same library (Ghidra `FUN_00086eb0`, which owns the string
`unsupported bits per sample %u`) checks STREAMINFO's depth with:

```c
if ((bps - 8 < 0x19) && ((1 << (bps - 8)) & 0x1010101) != 0)   /* 8, 16, 24, 32 */
```

32 passes. So the track opens, the graph is built, SetTrackSequence returns 0, and Play is accepted.
Then the decoder starves the renderer, PlayerService stops answering in time, and the next Cinder
call that is guarded by the IPC watchdog times out. That sets `g_ipc_dead`, which is the banner.

Not established: which guarded call timed out first. The device log that held that boot had
already rotated out by the time the device was next connected (only two boots are kept).

Ruled out: the mono shim (`cinder-home/src/mono_sum.h`). The Bluetooth socket stream is 16-bit
whatever the source, and the shim changes bytes only when mono is on.

## 4. What Cinder does

`CinderDb::undecodable_paths()` (`player/cinder-db/src/lib.rs`) lists every track whose
`BITS_PER_SAMPLE` is above 24 and whose path ends in `.flac`. The FFI keeps the set, and
`cinder_uri_undecodable()` answers for one URI.

Two layers:

1. **Before Up Next.** `set_pending()` in the FFI, which every library play goes through (a tapped
   song, an album, a list, a shuffle), drops such files with `drop_undecodable()` before
   `set_play_context` builds Up Next. If the song the user chose was one of them, a toast says so:
   "Skipped a 32-bit FLAC" when something else still plays, "Can't play a 32-bit FLAC" when nothing
   does, and then Up Next and the playing album are left exactly as they were. The first build
   filtered only in the shell (layer 2), and on the device a single tapped 32-bit song replaced Up
   Next with itself while the old album stayed loaded.
2. **Backstop.** `play_pending_sequence()` in `cinder-home/src/main.cpp` is the only place the app
   hands URIs to PlayerService. It leaves such files out the same way it leaves out a truncated URI,
   keeping the start index aligned. It catches what reaches Up Next without `set_pending`: a song
   swiped into the queue, or a queue saved by an older build.

A shuffle that passes over one says nothing, so the toast does not come back every time the file is
dealt. Now Playing follows the service's own current URI, so a skipped file never shows as playing.

*Device, 2026-09-29 (dev build, W1 player):* the set loaded at startup with this one file; tapping
it logged the skip twice, the toast showed, and there was no AUDIO STOPPED banner. That run was
before layer 1 existed.

Only FLAC is filtered. A 32-bit WAV or AIFF goes through a different decoder that this note has not
looked at.

## 5. For the owner

Convert such files to 24-bit to play them on this player, for example `sox x.flac -b 24 y.flac`.
24-bit keeps 144 dB of range, more than the DAC resolves.
