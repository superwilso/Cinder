# Plan: hardware first

Started 2026-11-18. The question: if the NW-A50's hardware is the only limit, what should Cinder
stop depending on? This file is the plan and the research ledger. Each phase has its research gate
(what must be known before code), what it removes, and how it is checked on a player.

## The finding behind it

Cinder runs inside Sony's userland and drives about 28 closed services. Most of the project's
hardest bugs come from that layer, not from the hardware:

- reboots when a Sony service restarts or one IPC call hangs (`fr_hang`, the shutdown barrier);
- the library store, written by Sony's scanner, which an unclean shutdown can corrupt;
- PlayerService's FLAC decoder (libFLAC 1.3.2) cannot play 32-bit files;
- Bluetooth: an opaque stack (`mtkbt` + `libBtMw`), connect jams, volume that stops working.

The hardware is reachable without that layer: the kernel builds from Sony's source, the
CXD3778GF is a plain ALSA card with six playback devices (hi-res, standard, DSD, low-power), the
Bluetooth controller has a raw HCI character device (`/dev/stpbt`), and Cinder already decodes
MP3 and AAC itself (internet radio, media servers, "Play to this player").

**The order below takes what each step gives back for the least risk first.** Every phase stands
on its own; none needs a later one.

## Measured 2026-11-18 (owner's NW-A55, dev build)

| What | Reading | Source |
|---|---|---|
| Bluetooth listening, screen off | 9.7 %/h (62 mV/h), 0.82 h | `tools/battery_track.sh report`; the tracker's `pcm` column only sees the jack, so it filed this as "idle" |
| Screen on | 51.7 %/h, 0.19 h (thin) | same run |
| Suspend to RAM when idle off the cable | works (`resumed from RAM r12=0x20` after Power) | `cinderhome.log` |
| Suspends that end within 6–20 s without a key | 3 in a row after a Bluetooth drop: `r12=0x7fffffff`, `0x7fffffff`, `0x400` (LOW_BAT) | `cinderhome.log` 2800–2850 s |
| `cinder-home` resident memory | 115 MB of 467 MB; 32 MB free | `/proc/*/status` |
| Sony services resident | each `hagodaemon` 1.2–11.7 MB, plus a 1 MB `logwrapper` per service | same |
| AVRCP down while A2DP streams | `AvSrc=5 Avrcp=1` after player-initiated connects: 7 volume steps unanswered, one Cinder repair (hang-up + redial) | `cinderhome.log` 4148–4697 s |

## Phase 0 — Bluetooth volume and the drops it causes

**What is wrong.** When the player redials the headphones (`RequestLastDeviceConnection`), A2DP
comes up and AVRCP sometimes does not. Volume presses then reach nothing, and Cinder's repair
(`bt_vol_watch.h`) hangs up and redials once per boot. That repair is a disconnect the listener
hears. The headphones connecting by themselves did not show it.

**Where Sony decides.** `libBtMw.so` `FUN_00027920` (the A2DP source state handler): on the
connect event it records whether the previous state was 3 (`+0x4d = state == 3`), and on the next
event connects AVRCP only if that flag is set — directly (`btmwAvrcpSrcConnect`) or after
1 s through timer `0x52`. A connect that completes from any other state never opens AVRCP, and
nothing retries. `btmwAvrcpSrcConnect` is exported.

**Research gate — answered 2026-11-18.**
1. *Which states miss the connect:* not needed for the fix below, which does not depend on why.
2. `btmwTimerStart(id, n, unit, fn, arg)` is exported, takes its own mutex (safe from any
   thread), and runs `fn` on the middleware's timer thread (unit 1 = ms). Timer `0x52`'s callback
   (file `0x1878d`, not exported) checks the A2DP state and opens AVRCP. A watcher can pass its
   own callback that calls the exported `btmwAvrcpSrcConnect()`; that function does nothing
   unless AVRCP is disconnected (`+8 == 1`), but it does not check A2DP, so the watcher must.
3. `cinder-hagowrap` already loads `HAGO_DIR/<first service name>.so` into any service:
   `preload/BtCommonService.so` lands in `hagoromo27`, which maps `libBtMw.so` (checked on the
   player). Readers: `btmwAvSrcGetInstance()` (+4 = A2DP state, 5 while streaming) and
   `btmwAvrcpSrcIsDisconnected()`.

**Fix, two steps.**
- *a, Cinder only:* 4 s after any link-up, read `GetAvrcpConnectionStatus`; A2DP up and AVRCP down
  → repair at once (before anyone presses volume), allowed more than once per boot, never twice
  within a minute.
- *b, the real one:* a watcher in the Bluetooth process that opens AVRCP on the middleware's
  thread when A2DP is up without it for 2 s. No hang-up.

**Open, noted 2026-11-18 for later.** Twice right after a connect, holding volume up made the
headphones report 40 → 72 (and 40 → 76), then a report of exactly 38 with no key pressed, then
the climb resumed. Part of it was Cinder's held-key repeat advancing the bar several steps per
tick on a sink that claims absolute volume (fixed the same day: Bluetooth repeat is one step per
tick). Who sends the 38 is not known: Sony's transmitter or VolumeService pushing a stored level,
or the headphones. If it still happens with the repeat fix, look here first; an own Bluetooth
stack (Phase 4) would remove the question.

**Check.** Headphones off, reboot, switch them on and let the reconnect ladder link them, ten
times: `Avrcp=2` within 3 s every time, no `bt-vol: … no answer` lines, no redial.

## Phase 1 — Battery

**Research gate.**
1. Make the tracker honest: count Bluetooth playback (Cinder's own state, not the jack PCM) so
   "idle" means idle.
2. `r12=0x7fffffff` after a suspend: an aborted suspend (woken before the CPU went down) or a
   reading taken too early. `icx_pm_helper` and `/proc/last_kmsg` say which.
3. The screen-on cost split: backlight vs the 1040 MHz floor Cinder holds while the screen is on
   (the scroll-stall fix) vs the render loop. One reading each, same brightness.
4. Bluetooth listening: a stock-firmware reading with the same headphones and codec, to know
   whether 9.7 %/h is Cinder's cost or the radio's.

**Lead, 2026-11-18.** After the headphones dropped (case closed, screen dark), three suspends in
a row ended 6–20 s in with `r12=0x7fffffff` twice and `0x400` (LOW_BAT) once. The reconnect
ladder retried at each resume, but its timer runs on Cinder's clock, which stops in suspend: the
retry is what an overdue timer does on waking, not the waker. What ends these suspends is open.
`spm_r12` is copied from SPM's PCM register 9 only after SPM really slept
(`mt_spm_sleep.c` `spm_output_wake_reason`), so `0x7fffffff` is a value SPM returned, not a stale
one. The kernel prints its own decode on each resume (`[SPM] wake up by … (r12)(raw_sta)`):
the next time it happens, `dmesg | grep "wake up by"` before the buffer rolls.

**Levers, in order of expected size.** The 1040 MHz floor only while a finger is down or a
scroll is moving; nothing in Cinder's loop waking with the screen off; the aborted suspends;
Sony's 10 Hz pollers (`KeyService`, the LED thread).

**Check.** `tools/battery_track.sh` runs of at least an hour per state, before and after.

## Phase 2 — Cinder's own library index

**Today.** `cinder-db` reads Sony's `/db/MTPDB.dat`; Sony's `MediaScanner` writes it; Cinder
keeps a known-good copy and restores it on damage (`db_snapshot_keep`/`_restore`). PlayerService
takes plain paths (`NodeTrackSequence` passes a `uri` starting with `/` through untouched), so
playback does not need Sony's index. `cinder_stats.tsv` is already keyed by path. The queue
(`queue.conf`, `resume.conf`) and playlists hold MTPDB object ids.

**Plan.** A Cinder scanner: walk `/contents/MUSIC` and the card, compare size + mtime with its own
index (SQLite on `/contents`), read tags only for what changed. Object ids carry over: on the
first run each track takes its MTPDB id by path, new tracks get ids above the highest; the queue
and playlists keep working untouched. Sony's scanner is no longer called by Cinder.

**Research gate.**
1. Tag readers for every format the player lists: FLAC, MP3 (ID3v2.2–2.4, v1), MP4/M4A/ALAC,
   WAV, AIFF, DSF, DFF, APE, WMA (ASF), and the embedded art each carries. Cinder already reads
   several for lyrics and ReplayGain.
2. SensMe. The channel bitmask is computed on the player by Sony's `Smu` code inside
   `libMediaStoreService.so` (`0x540dc` → `0x568a8`), not exported. **Decided:** Cinder keeps
   asking Sony's scanner to run in the background, and the index takes only the SensMe values
   from MTPDB, by path. A damaged MTPDB then costs SensMe data until the next scan, not the
   library.
3. *Measured:* the owner's card holds 4,835 files under `MUSIC` (4,773 FLAC); a full walk takes
   2.4 s; reading the first 8 KB of 400 files from a cold cache took 13.7 s through the shell
   (34 ms a file, fork included), so a first scan is 1–2 minutes and later ones read only changed
   files.
4. What else reads MTPDB: Sony's stock UI after "boot to stock" (it rescans itself), and Sony's
   own MediaStoreService scans after USB (cannot be stopped from Cinder; it no longer matters).

**Built 2026-11-19 (night): the index, not yet the switch.** `cinder_db::index` (player/cinder-db/src/
index.rs): walks `/contents/MUSIC` and `/contents_ext/MUSIC`, skips files whose size and mtime are
unchanged, reads FLAC tags (STREAMINFO length, Vorbis comments; an ID3v2 tag in front is skipped),
keeps each track's MTPDB id by path and gives new files ids above MTPDB's, drops rows for files that
are gone. One small write per changed file, so a reboot mid-scan costs only that row. Lives in
`/db/cinder/library.db` (ext4, never shared over USB, ~80 MB free): `/data` is a 35 MB partition
and an install needs about 8.5 MB of it free (a 1.7 MB index there blocked one); a night on
`/contents` showed FAT doubling a first scan (91 s against 43 s). Dev hook: `touch /tmp/cinder_index.scan` logs the comparison with MTPDB.

On the owner's player: **4,914 tracks and 441 albums, identical to MTPDB, every id carried over**;
first scan 43 s, a scan with nothing changed 3.3 s. 22 tracks differ, all deliberately: where a file
repeats ARTIST (`Dead Calm` + `Masumi`) Sony keeps the last value and the index the first (the main
artist), and Sony keeps a trailing space in a title that the index trims. Other formats are indexed
by path with the file name as title (no ID3/MP4 reader yet; the owner's library has none).

Next: the UI's `Library` built from the index instead of MTPDB (albums by album artist + album, ids
unchanged so queue/playlists/likes/stats keep working, SensMe values joined from MTPDB by path),
then a scan after USB and at boot instead of asking Sony's scanner, then ID3 and MP4 readers.

**Check.** Same track, album and artist counts as MTPDB on the owner's library (passed for the
index); add/remove/rename over USB shows up in seconds; a forced reboot mid-scan loses nothing.

## Phase 3 — Cinder decodes, Sony's sound service still plays

**Found 2026-11-18.** `libSoundService.so` is a public client library:
`SoundService::CreateTrack(TrackType, TrackParam const&, Err*, vector<string> const&)` returns an
`AudioTrack` with `Start`, `Write(TrackBuffer{ptr,size}, …)`, `Pause`, `Stop`, `Drop`,
`SendEos`, `SetMute`, `SetSourceType`; there is a `DsdTrackParam` overload for native DSD. Five
Sony players use it (PlayerService, AudioInPlayer, BtPlayer, OneTrackPlayer,
UsbDeviceAudioPlayer). `AudioInPlayerExhal` is the template: `MakeTrackParam` fills rate (+0),
bits (+4: 16/24/32), channels (+8), bytes per sample (+0x2c: 2/3/4) and fixed fields;
`CreateTrack(type 0, …)`; then `Write` loops a period at a time. The service applies Sony's whole
effect chain (DSEE HX, the EQs, VPT, DC Phase, Vinyl), the output power policy, and routes to
Bluetooth itself (it holds the transmitter socket).

**So Cinder can own decoding and keep every Sony effect.** It removes PlayerService's limits:
32-bit FLAC, the IPC hangs, the queue rebuilds; and gives true gapless and crossfade, because
Cinder writes one continuous stream.

**Research gate.**
1. *Answered:* every Sony player (PlayerService, BtPlayer, USB-DAC, AudioIn) creates **type 0**
   (`music`), so they are mutually exclusive by design; the client creates the shared memory
   (`shm_open` `O_CREAT|O_EXCL|O_RDWR`, mode 0660) and Sony's `system` user is uid 100, the
   same as `cinder-home`, so permissions are not in the way. `TrackParam` is 0x64 bytes;
   AudioIn's fill (above) covers 44.1–192 kHz at 16/24/32 bits.
2. *Answered:* with nothing playing there is no `TK_MUSIC` shared memory, so PlayerService holds
   no track while stopped. Still to see on the player: effects and Bluetooth routing on a track
   PlayerService did not create (first spike), and what `SetSourceType` must say for DSEE HX.
3. Decoders: FLAC (pure Rust), MP3 (minimp3, in tree), AAC/ALAC (FDK in tree / pure Rust ALAC),
   WAV/AIFF, DSD (raw, through the DSD track). WMA, APE and MQA stay on PlayerService: the engine
   falls back per file.
4. Seek, pause, position reporting and the analyzer tap with Cinder as the producer.

**Spike, 2026-11-18: PASS on the jack.** `cinder-probe --sstrack [s] [hz]` (dlopens
`libSoundService.so`, runs as uid 100 via `busybox setuidgid system`) played a 440 Hz tone through
Cinder's own type-0 track: `pcm4p` (`hw:0,4`, the jack's music output) RUNNING, 5.0 s written in
4.1 s with no short writes (the service's queue paces `Write`), the track and its shared memory
gone after delete. The call order is AudioInPlayerExhal's: CreateTrack → Init (+0x08) →
GetTrackEnvironment (+0x0c, period 8192 bytes at 44.1/16) → Start (+0x10) → Write (+0x14) … →
Drop (+0x2c) → Stop (+0x30) → delete (+0x04). Start without Init answers 1 ("track is not
started").

**Two traps learned.** (1) A track whose process dies without deleting it is never freed:
PlayerService then cannot create its own and music is silent until a reboot (happened twice in
the spike; the probe now releases its track from the crash handler). The engine must delete its
track on every exit path, and the launcher's supervisor must know a dead `cinder-home` can leave
one. (2) A paused PlayerService keeps its track; it has to be closed
(`cinder_audio_release_sequence` + `cinder_audio_close_player`, as FM does) before Cinder's track
can exist. Dev flag for that: `touch /tmp/cinder_player.release`.

**Cinder's own FLAC decoder, 2026-11-18.** `player/cinder-ffi/src/flac.rs`: 4- to 32-bit,
i64 prediction, CRC-8-checked frame sync, SEEKTABLE or bisection seek, C interface
`cinder_flac_open/info/read/seek/close`. Host tests: five fixtures (16-bit LPC and fixed, 24-bit
96 kHz, 32-bit, mono) and five of the owner's tracks decode to their own STREAMINFO MD5; seeks are
sample-exact. On the player (`cinder-probe --sstflac <file> [s]`): a 16-bit Rumours track played
through Cinder's track at **22× real time** (about 4.5 % of a core); a 32-bit test file played as
**`S32_LE` at the DAC**, 28× real time — the file class that hangs PlayerService. (The owner's
"Sprain – Anything" is now 16-bit in the library, so no real 32-bit track was at hand.)

**Bluetooth and effects: PASS 2026-11-18** (owner listening, LDAC headphones). The same Rumours
track through Cinder's track went to the headphones by itself — no jack PCM open, BTIF at about
500 interrupts a second — and with Sony's 10-band EQ moved to +10/+10/+6 dB on the lowest bands
(`devset.sh eq=…`, applied live) the owner heard the change. So a Cinder-decoded track gets Sony's
routing and effect chain exactly as PlayerService's does. Phase 3's gate is passed: next is the
engine (FLAC through Cinder's track, PlayerService kept for every other format).

**The engine, built 2026-11-18** (`cinder-audio/src/engine.hpp`, off unless `own_engine` is in
`cinder_advanced.conf`). An all-FLAC queue goes to Cinder's track; any other queue stays with
PlayerService, which is closed first so its track is free. Songs of one format follow each other
in one track (gapless); a change of rate or depth drains, then opens a new track. Play, pause,
seek, next and previous (song and album), Up Next edits, repeat one, ReplayGain and position all
go through it. Mono plays on both sides; files with more than two channels are skipped. Because
the device build aborts on a Rust panic, the decoder was fuzzed with 35,000 damaged copies of the
fixtures (`damaged_files_never_panic`, `CINDER_FLAC_FUZZ=N` for more); it found a seek underflow
on truncated files, now fixed, and the predictors now wrap instead of overflowing. Installed on the
owner's player with `own_engine` on. **Device test, 11-18 night (checklist 51.28):** play, pause, resume, seek, the resume position, repeat one, next, previous and an automatic song change (PCM never stops; Now Playing follows) pass. Not yet driven: album skip, 32-bit through the engine, end of queue.

**Mixed queues and crossfade, built 2026-11-18 (night).** The engine now takes any queue whose
first song is FLAC; at the first song that is not (reached by playing on, or by a skip), it plays
out and hands the whole queue to PlayerService at that song (`cinder_audio_engine_poll`, on the
shell's 1 s housekeeping, because Sony's player is driven from that thread only). One way only:
PlayerService keeps the queue it was handed. Not device-tested: the owner's library is all FLAC
(no other audio file on either store). Crossfade: `crossfade=N` (seconds) in
`cinder_advanced.conf`, off by default; between songs of the same rate and depth the next song is
opened N seconds before the end and mixed in with linear gains; repeat one, a pause or a seek
cancel it. On the player with `crossfade=4`: the next song started during the old one with the PCM
never stopping and Now Playing following; the timestamped trace (11-19) shows the fade starting
4.0 s before the end and the new song heard 0.6 s later. The owner's ear decides the curve. The end
of the queue frees the track, and repeat-all restarts it through the engine. A seek that arrived
while a start was still opening the track used to be dropped (`go` only jumped when `playing`);
the engine now tracks "paused by the listener" instead.

**Check.** The owner's library plays end to end with effects audibly on, over the jack and LDAC;
the 32-bit file plays; gapless album shows no gap at the jack (capture rig).

## Phase 4 — Own Bluetooth stack (spike first)

**Found.** `/dev/stpbt` (MTK's HCI channel, 192:0, `system`) exists; the kernel has no Bluetooth
core (`CONFIG_BT` unset, no `/dev/vhci`). The Wi-Fi work already built kernel modules from Sony's
tree plus backports; Bluetooth is in the same backports.

**What it would give.** An open stack (BlueZ) with a known connect and reconnect policy, the
headphones' battery level, codec control, and logs. LDAC from Android's open encoder (Apache 2.0),
aptX from an open implementation, AAC from the FDK already shipped.

**What it costs.** Everything Sony's Bluetooth services do today must exist again: A2DP source and
sink (receiver mode), AVRCP both ways, NFC pairing, the BLE remote. Weeks, not days.

**Spike (go/no-go, read-only to Sony's setup).** Build `bluetooth.ko` + `hci_vhci.ko`; with
`mtkbt` stopped, bridge `/dev/stpbt` to `/dev/vhci`; `btmgmt info` must show the controller and a
scan must find the headphones. Open: whether the chip needs vendor init commands beyond what the
WMT driver does at power-on.

**Read 2026-11-18 (kernel source and firmware, nothing run).** The MT8590 uses the built-in
`conn_soc` driver: opening `/dev/stpbt` powers the Bluetooth core on through WMT
(`mtk_wcn_wmt_func_on`) and closing it powers it off; `read`/`write` pass raw H4 bytes from STP's
queue, so a bridge must split the stream into whole packets before handing each to `/dev/vhci`.
The vendor init lives in `libbtdrv.so` (`BT_InitDevice` → `GORM_Init`): it reads the 64-byte NVRAM
record `/data/BT_Addr` (address, voice, codec, radio, sleep, feature, TX power offset and coex
fields, per its own log format) and sends vendor HCI commands built from it, starting with
`GORMcmd_HCC_Set_Local_BD_Addr`.

**The power-on commands, decoded 2026-11-18** (`bt_init_script_consys`, seven builders in
`libbtdrv.so`; `persist.mtk.wcn.combo.chipid` selects the table; offsets are into the 64-byte
`/data/BT_Addr` record, whose fields run address 6, voice 2, codec 4, radio 6, sleep 7, feature 2,
TX power offset 3, coex 6):

| # | HCI opcode | Parameters | What |
|---|---|---|---|
| 1 | `0x1009` then `0xFC1A` | 6 bytes, record 0x00 | read the chip's address, then set the one in the record |
| 2 | `0xFC79` | 6 bytes, record 0x0C | radio settings |
| 3 | `0xFC93` | 3 bytes, record 0x1B | TX power offset |
| 4 | `0xFC7A` | 7 bytes, record 0x12 | sleep mode |
| 5 | `0x0C03` | none | HCI Reset |
| 6 | `0xFCBE` | `05` | firmware log on, only if `persist.bt.contdebuglog` is set |
| 7 | `0xFC5F` | 42 bytes, fixed | firmware log setup, same condition |

So a BlueZ bridge needs no firmware download and no undocumented handshake: open `/dev/stpbt`
(WMT powers the core), send 1–5 built from `/data/BT_Addr`, then hand the stream to `/dev/vhci`.
Commands 6–7 are optional.

**Bridge for the spike only.** A `/dev/vhci` bridge puts a user process in the path of every
packet (hundreds of wake-ups a second on LDAC). Shipping, the same job belongs in a small kernel
module that registers an `hci_dev` and calls STP directly (`mtk_wcn_stp_send_data` /
`_receive_data`, the functions `stp_chrdev_bt.c` wraps), with no user-space hop, which is fewer
copies than Sony's own stack (`mtkbt` also reads `/dev/stpbt` from user space). It ships with the
kernel option (partition 8). Battery: Bluetooth listening is dominated by BTIF interrupts (about
500 a second), not the codec, so the bigger lever is fewer, larger transfers; measure the
interrupt rate under BlueZ against Sony's before deciding. The voice, codec, feature and coex fields are not sent at power-on
(Sony's stack may send them later, at SCO or coex time; the HCI trace would show it).

## Phase 5 — Boot time and memory

**Measured 2026-11-18, an hour after boot:** 46 MB resident (34 MB anonymous), so most of the
115 MB below grows with use (art, fonts, the library model), not at start.

**The memory line, 2026-11-18.** `cinder-home` logs `mem (…)` every 30 minutes (and on
`touch /tmp/cinder_mem.report`, dev builds): resident, malloc's heap, and the UI's own holdings
(`cinder_mem_report`: thumbnails, covers, Now Playing art, scaled and gradient caches, glyphs,
device fallback fonts parsed, library size). First reading, a minute after boot: resident 47 MB,
heap 30 MB (27.8 MB in use) plus 5.7 MB mmapped; thumbnails 3 MB (441), Now Playing art 0.7 MB,
glyphs 18 KB, no fallback font parsed; 4,914 songs, 268 albums. About 24 MB of the heap is not in
those counters yet (library rows, parsed bundled fonts, Sony's client libraries). The 30-minute
lines over a day of use show what grows toward the 115 MB below.

**Measured earlier.** Of `cinder-home`'s 115 MB, 94 MB is anonymous mappings and 14 MB heap; libraries
are small. The split inside the 94 MB needs Cinder's own counters (art caches, parsed fonts, the
library model, frame buffers).

**Research gate.** Why `cinder-home` holds 115 MB (art caches, fonts, framebuffer shadows); which
Sony services Cinder never calls (candidates: OneTrackPlayer, AudioAnalyzer, Recorder,
RemoteCmdInjCtrl, Nfc, Memmon, Update) and whether `appmgr` or the shutdown barrier waits on
them. Removing a service is a boot-image (`init.hagoromo.rc`) change on partition 8, which the
kernel option already writes.

**Check.** The boot timeline (`docs/` boot measurements) before and after; free memory after an
hour of use.

## Next steps (2026-11-19)

In order; each builds and passes the harness before the next starts.

1. ~~The engine's song-change timing~~ — done 11-19: no fault; the engine logs CLOCK_MONOTONIC and
   the shell counts from its own start (~10 s later), and comparing the two made a 10 s gap.
2. **The Library from the index** — built and on the owner's player 11-19 (`own_index`): 4,914
   songs, 441 albums, 268 artists from the index, the same as MTPDB; a scan after boot (5.3 s,
   nothing changed, no reload) and after USB storage, and the Library reloads when a scan changed
   something. A root that is not mounted keeps its rows. **Now identical to MTPDB** (0 of 4,914
   differ, 11-19 evening): the 22 differences were Sony's own rules, which the index now follows: a
   repeated field keeps its LAST value (`ARTIST=Kelela`, `ARTIST=Fousheé` lists as Fousheé), spaces
   are kept, and a name stops at 255 UTF-16 units. A change of rules re-reads every file once
   (`PRAGMA user_version`, ids kept; 4,914 files in 30 s). Writes go 200 files to a transaction (a
   first scan on `/contents` took 91 s one commit per file). Original plan: build
   `cinder_ui::Library` from `/contents/.cinder/library.db` (ids unchanged; SensMe, art keys and
   play counts joined by id), MTPDB stays the fallback when the index is missing or empty.
3. **When to scan:** at boot after `/contents` mounts, and after USB storage is released; never
   during USB storage. Sony's scanner keeps running for SensMe only.
4. **ID3 (MP3) and MP4 (M4A/ALAC) tag readers** — built 11-19 (ID3v2.2–2.4 and v1; MP4 `ilst` and
   the `mvhd` length), fuzzed with 4,000 damaged buffers. MP3 length (Layer III) from the Xing,
   Info or VBRI frame count, else the constant bitrate; host-tested only (the owner's library is all
   FLAC).
5. **/data headroom** — done 11-19. `/data` (35 MB) cannot grow without rewriting the eMMC partition
   table, but `/db` (ext4, 94 MB, ~80 MB free, never shared over USB, already holding Cinder's MTPDB
   copy) has room: the album-art cache (16 MB for 441 albums, ~37 KB an album) and the index moved
   there, the cache moving its files over once. The cache keeps 30 MB of `/db` free for MTPDB, its
   copy and a scan's journal. `/cache` (23 MB, empty) is the next spare if needed.
6. **Bluetooth kernel modules and the go/no-go tool** — 11-19. `wifi/build-bluetooth.sh` builds
   `compat.ko`, `bluetooth.ko` and `hci_vhci.ko` (plus the Wi-Fi modules: one backports build must
   carry both, its `compat.ko` differs from `build.sh`'s) with the player's vermagic. Not loaded yet.
   `cinder-home/tools/btspike.c` (static, musl) opens `/dev/stpbt`, sends Sony's power-on commands
   built from `/data/BT_Addr`, and runs an HCI inquiry. On the owner's player with Sony's Bluetooth
   off (nothing held the node; the kernel log shows WMT powering the core on at open and off at
   close): **all six power-on commands and the inquiry command completed with status 0** — the core
   takes commands from a program that is not Sony's stack. No device answered a 10 s inquiry at
   1 a.m. **Radio proven 11-19:** with the owner's headphones in pairing mode, a 15 s inquiry got
   an answer at −58 dBm (`GO`, exit 0). Next: load the Bluetooth modules and bridge `/dev/stpbt`
   to an HCI device, then pair and stream from a stack of our own.
7. **A command channel for tests, no screen** (owner, 11-19) — first cut built and device-tested
   11-19: `tools/devctl.sh cmd state | play | pause | next | prev | 'seek MS' | 'repeat 0-3'` (several
   at once, one answer a line). Dev builds only; cinder-home reads `/tmp/cinder_cmd` (keyed on the
   nanosecond mtime) and answers in `/tmp/cinder_cmd.out`. Added and device-tested the same day:
   `'find TEXT'` (album ids), `'album ID|TEXT [SONG]'` (SONG counts from 1), `restore`, and
   `devctl.sh queue save|restore DIR`, which puts a queue back with no reboot. Testing `restore`
   found a real fault: ▶ on a pending resume, with the service paused on the same song, edited the
   list and never played (fixed). `'shuffle 0|1'` added after; one track by path was dropped,
   `album NAME SONG` already plays any song. Behind a dark screen an answer can take over 5 s
   (devctl waits 10).
8. **Battery runs** (Phase 1) wait for the owner: an hour each, player left alone.

## Limits this removes

| Limit today | Phase |
|---|---|
| Volume stops reaching Bluetooth headphones; the repair hangs up | 0 |
| A 32-bit FLAC hangs playback | 3 |
| Gaps between tracks, no crossfade under Sony's player | 3 |
| Library lost or 0 tracks after an unclean shutdown mid-scan | 2 |
| New music waits for Sony's scanner (rounds of up to 60) | 2 |
| No headphone battery level; connect jams | 4 |
| 17 s boot, 32 MB free | 5 |
