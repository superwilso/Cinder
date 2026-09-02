# Changelog

All notable changes to Cinder are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While the major version is `0`, the
minor version moves for anything user-visible and the patch version for fixes.

**Entries are honest about verification.** A line marked *device-verified* was executed on real
hardware. A line marked *device-unverified* is code-complete and gated on a hardware session — a
distinction this project tracks deliberately, because most of what ships here cannot be proven by
a test suite. See [`cinder-home/STATUS.md`](cinder-home/STATUS.md) for the standing matrix.

This file was backfilled from git history on 2026-09-01. Releases before then are summarised at the
level the commit history supports; from `v0.1.6` onward, entries are written as changes land.

## [Unreleased]

## [0.1.8] — 2026-09-02

### Fixed

- **The screen never actually went dark.** `set_backlight` has recorded since 2026-08-19 that the
  sysfs node alone does not darken this panel — measured with the node at 0, DisplayService at 2 and
  the panel still lit — and handles it. But neither path that blanks the screen went through it:
  `screen_auto_off` (idle timeout) and `screen_toggle`'s off-branch each carried their own copy of a
  raw `fputc('0')` to the node and stopped there. So the idle timer and the Power button both
  "turned the screen off" by writing a number the service overrides. Both now call one `panel_dark()`
  that caches the service's level and then zeroes both halves — the duplication is why the defect
  existed twice. *This is a standing bug in its own right and NOT the `g_ipc_dead` wedge; it was
  briefly attributed to that latch while diagnosing v0.1.7, which was wrong.* Two harness scenarios
  (`blank-idle`, `blank-order`) pin it, and both fail against the old code.
- **Losing Sony IPC was completely silent.** When `run_guarded` unwinds a Sony call it latches
  `g_ipc_dead`, and from that moment the device cannot play, pause, skip, drive Bluetooth volume or
  sleep its panel until it is restarted — with the only trace a line in `cinderhome.log`, which
  cannot be read from the device. The status strip now carries a persistent `AUDIO STOPPED —
  RESTART` banner in the codec badge's place, on every screen, set from the guard's recovery path
  through a lock-free atomic (`cinder_set_ipc_dead`) because that path must not take a mutex. The
  banner replaces the badge and the NIGHT label and nothing else — an early version returned before
  the right-hand block and dropped the battery readout, which is the one indicator a degraded device
  most needs. Four host tests.
- **New music copied over USB never appeared.** Nothing asked MediaStore to scan when a transfer
  finished — the DB reload on USB-MSC exit re-reads a store that has never heard of the new album,
  and the only trigger was a Settings row you had to know about. Exiting mass storage now arms the
  same bounded rescan campaign. *Device-unverified.*

### Changed

- **`.gitattributes` pins the release payload to byte-exact.** `release.yml` builds on
  `windows-latest`, where Git's `core.autocrlf` rewrote LF→CRLF at checkout for the two *text*
  members of the payload — so their hashes stopped matching and the v0.1.7 release failed its own
  manifest gate (run 33643552973) with every binary passing and only the text files failing. Nothing
  was stale; the bytes were changed in transit. This was latent from the day the gate was added:
  v0.1.6 was never tagged, so v0.1.7 was the first tag it ever ran on. `cinder-home/dist/**` is now
  `-text`, a new `tools/check_payload_attrs.sh` runs in CI on every push so the gate can no longer
  make its debut during a release, and `verify_payload_manifest.sh` names line endings as the cause
  when only text members fail. It also mattered beyond CI: `cinder-signature.sh` is executed on the
  device, and a CRLF shebang is `#!/bin/sh\r`.

## [0.1.7] — 2026-09-02

### Fixed

- **Skipping back could kill audio, Bluetooth and the screen-off timer for the rest of the boot.**
  `PlayController::PrevTrack` takes a `PrevTrackOption const*` and dereferences it eight
  instructions in, before any null check — there is no null check — and both `cinder_audio_prev_track`
  and `cinder_audio_prev_group` passed `nullptr`. The SIGSEGV landed inside a guarded call, which
  latches `g_ipc_dead` and refuses all further Sony IPC for the boot: the UI kept working while
  playback could not be paused or skipped, the panel would not sleep, the Bluetooth switch read OFF
  while a headphone stayed connected, and the volume rocker no longer drove the BT sink. It needed a
  restart, and the only trace was one line in `cinderhome.log`. Both calls now pass a real
  zero-initialised option. Rare only because `◁` reaches `PrevTrack` just within the 3 s restart
  grace and with no Cinder history to step back through. *Device-verified — diagnosed from
  `cinderhome.log.1` @134.479 (`sig=11 … PrevTrack+0x13`, `addr=(nil)`), whose fault offset is the
  `ldr r2, [r1, #0]` at `libPlayerServiceClient.so` `0x30d4`, to the instruction.*
- **Jumping to a track in Up Next threw away a "Shuffle all songs" order.** The tap emitted
  `PlayIndex`, and `PlayIndex` resolves an object id to its **album** — the only context an object
  id carries — so picking a song four rows down replaced the shuffled library with that song's
  album. The same defect `PlayPlaylistAt` was added for, reached from the other side. Up Next now
  emits `PlayContextAt(row)`: the shell restarts the sequence it already holds at a new index,
  without rebuilding or re-shuffling it. Rows are mapped through the same resolution filter as the
  sequence, so a file deleted since the context was built cannot slide the start onto the wrong
  track. *Device-unverified — two host tests pin it.*
- **Long titles were unreadable.** Now Playing fitted the title to 372px and the artist to whatever
  the codec left, so a classical or remix title lost exactly the part that identifies it
  ("Sinfonia concertante for Violi…"). The title and artist now scroll — dwell, slide, dwell, slide
  back — on both the day and night layouts, clipped to their own box by a new horizontal clip band
  on `Canvas`. List rows keep the ellipsis: forty animating rows would be noise, and a truncated row
  is still enough to pick from. A line that fits is drawn exactly as before and requests no
  repaints, so the common case costs nothing. *Device-unverified — five host tests plus rendered
  frames.*
- **`Settings ▸ Database` only ever scanned part of the library.** One `MediaScanner::Scan()`
  returns `rc=0` and stops well short of the whole tree: measured 2026-09-01, two presses took the
  store from 2,560 → 2,569 → 2,727 tracks, and it stopped there because nobody pressed a third
  time — leaving 696 tracks in 70 whole SD-card album folders absent from `MTPDB.dat`, which is the
  "some albums aren't detected" report. Folders were missing all-or-nothing, never partially, which
  is an interrupted walk rather than a tag or codec problem. A rescan now keeps re-issuing the scan
  from the housekeeping tick until the store stops changing (`db_signature`), bounded at 12 rounds
  10 s apart. *Device-verified as to the symptom and the increments; why one `Scan()` stops early is
  not established — the NULL listener cannot report it.*

- **The UI wedged after a quick run of track skips.** Transport presses were carried out inside
  `input_pump`'s evdev drain loop, where each one is a synchronous ~400 ms Sony round trip — so a
  finger on the glass kept the loop fed and starved the paint, the now-playing poll, the sleep
  timer and the auto power-off for as long as the user kept tapping. Presses are now queued and
  applied one step per frame by `transport_tick()`, the pending steps are net (▷▷×5 then ◁×2 is
  three steps, not seven), the backlog is capped, and the drain loop is bounded at 8 rounds per
  node per frame. Measured off-device: 40 taps in 4 s left the app issuing skips 13.1 s after the
  last one with housekeeping having run once in fifteen seconds; after the fix, 3.7 s and 13 of 15.
  Two harness scenarios (`rapid-skip`, `rapid-skip-touch`) pin it. *Device-unverified — the
  mechanism and the numbers are the harness's.*
- **`SIGALRM` was delivered to the wrong thread, turning recoverable timeouts into boots to stock.**
  `alarm()` is process-directed, and every thread Sony and cinder-ffi create inside `deferred_up()`
  inherited an unblocked `SIGALRM`. A guarded call that timed out usually had its signal land on one
  of the eight threads parked in a condition variable, where the handler fell through to "un-guarded
  hang", latched the bad-boot counter and `_exit(42)`. The handler now forwards a stray `SIGALRM` to
  the watchdog owner with `pthread_kill`. Probabilistic, which is why it read as "spamming skip
  *sometimes* crashes it". *Device-verified — three reboots produced the diagnosis.*
- **Calling back into Sony IPC after a guard recovery crashed the app.** `siglongjmp` out of an IPC
  abandons a half-built `std::string`/`std::vector` inside the client; the next call in dereferences
  it and faults in `libc++`. A five-second cool-down was tried first and was not enough — the object
  is broken, not busy. A recovery now ends Sony IPC for the life of the process: the UI, panel,
  buttons, library and timers survive; audio and Bluetooth need a restart. *Device-verified.*
- **`/contents` unmounted by something else is now reclaimed.** Sony's stack unmounts the music
  volume when a cable appears. Album art read inside that window failed, and the failure was cached
  in `art_key` for the rest of the boot, so an album stayed a grey gradient even after the volume
  came back. `cinder_db_open` now clears `art_key` and `last_track` so the current track re-derives
  immediately, the log and the scrobbler are re-pointed at the reappeared volume, and the mount is
  retried with a back-off.
- **The guard-recovery log line was truncated.** Its buffer was 192 bytes for a 209-byte message, so
  the most important line the fault path prints lost its tail. `-Wformat-truncation` had this red in
  CI.
- **The crash/hang guard could break more than the call it was guarding.** One `run_guarded` served
  every caller, and four of its call sites are not Sony IPC. A slow mount or `statvfs` set
  `g_ipc_dead` and cost the user audio and Bluetooth for the boot; and once that flag was set by
  anything at all, the guard refused *everything* — including the `/contents` reclaim, so one
  recovered transport timeout meant a cable plugged in later left the library missing and the album
  art grey until a reboot. Worse, `siglongjmp` does not run destructors: unwinding out of
  `cinder_db_open`, which holds the render `Mutex` across the SQLite open and library build, left
  that mutex locked with no owner for the life of the process.

  The guard is now three, chosen by what an abandoned call leaves behind — nothing (`GUARD_LOCAL`,
  recover and carry on), a half-built Sony client (`GUARD_IPC`, unchanged), or something that
  outlives the call such as a held mutex or an unreaped `system()` child (`GUARD_FATAL`, never
  unwound — a hang there is a clean labelled `_exit(42)` into the escape ladder, which is what the
  ladder is for). `guard_selftest.cpp` went from 4 tests to 9; the new ones fail on the old logic.
- **Album-art decode did not compile** after `zune-jpeg` 0.5 and `png` 0.18 changed their reader
  traits. Fixed, and `png` 0.18's `output_buffer_size()` now returning `Option` is handled with `?`
  — a second net under the dimension gate, where 0.17 would have allocated on a wrapped value.
  Verified beyond the host suite with a full `cinder-home/build.sh stable`: ARM link clean at
  `GLIBC ≤ 2.18`, under the device's 2.23, with `rusqlite` 0.40's newly bundled SQLite.

- **3.5 mm volume was unresponsive with the screen off.** Reported directly. The volume path was
  not at fault — the frame pacing was. With the panel dark the render loop sleeps in `poll()` to the
  next 1 Hz housekeeping deadline, which is the single biggest battery lever in the app and is right
  for events, because `poll()` returns immediately on one. But two things the rocker needs are not
  events, they are *deadlines* serviced at the top of the loop: the synthesized ramp's next step
  (every 120 ms while held) and the trailing write of a coalesced ramp (an `amixer` write is a
  fork+exec of `/bin/sh`, so steps are batched and the level the user stopped on is written
  afterwards). Nothing woke the loop for either, so releasing the rocker within 150 ms of the last
  write left the final level sitting in `g_vol_pending` until the next housekeeping tick — up to a
  second of nothing after you stopped pressing.

  It only shows with the screen off *deliberately*: an idle blank wakes on any non-Power key, so the
  panel lights, the budget snaps to 16 ms and the defect disappears. Power-off keeps it dark, which
  is the pocket case the rocker exists for. The loop now never sleeps past owed volume work, and
  sleeps exactly as long as before when none is owed. Rule and reasoning in `src/frame_budget.h`,
  checked by `tools/framebudget_selftest.cpp` (the harness cannot reach this — its clock is virtual
  and darkening the panel needs a `carry_out` it stubs).
- **The EQ shim forwarded gains the DSP silently zeroes.** `SetEq10BandValue` takes half-dB units
  and a value outside ±20 does not clamp inside Sony's service — it **zeroes the band**. The UI
  could never produce one (every site in the EQ screen clamps), but the settings loader parses `i8`
  from a file on `/contents`, which is vfat and writable by any PC the player is plugged into. One
  corrupted or hand-edited line silently flattened a band, drew its knob outside the EQ field, and
  was written back out on the next save. Clamped now in the loader *and* in the shim — the loader
  protects Cinder's model, the shim protects the service, which cannot defend itself and fails
  silently. Found while writing `cinder-audio`'s first-ever test (`SHORTCOMINGS.md` §A2).
- **`cinder_get_eq_bands` could leave its output uninitialised.** It returned early when the
  renderer was not up, leaving the caller's `signed char bands[10]` as stack garbage on its way into
  the DSP. Not reachable today, which is how it would have stayed until it wasn't. It now always
  writes all ten, flat when there is nothing to report.
- **`decode_jpeg` had no test.** JPEG is what every cover in a real library is (`magic=FFD8FFE0`,
  embedded in FLACs), and it was the one decoder with no coverage — which is why a semver-major
  bump could break it silently. Three tests added with 8×8 fixtures: RGB channel order, the
  grayscale fan-out branch, and a truncated file.

### Added

- **The Linux installer is published again** (it last shipped in `v0.1.2`). It stages every file and
  then stops — Sony's `SoftwareUpdateTool.exe` performs the USB handoff and has no Linux equivalent
  — and now says so and exits 0 instead of reporting a failure, printing the three manual steps that
  finish the job.
- **Repository groundwork:** `CHANGELOG.md`, `CODE_OF_CONDUCT.md`, issue templates (including one
  for reporting a `DEVICE_CHECKLIST.md` item run on real hardware), a pull-request template with a
  blast-radius section, `docs/README.md` as the documentation index, README badges, and a
  `cargo audit` job in CI with a weekly cron.
- `dependabot.yml`, scoped so it can only propose what CI can actually judge: semver-major updates
  are ignored for every crate the device links, because `cinder-home/build.sh` is the only thing
  that checks this tree against glibc 2.23 and no hosted runner runs it.

### Changed

- `cinder-home/dist/dev/` is no longer tracked. It was 74 of the 117 committed ARM-binary revisions
  in a 1.3 GB `.git`, and it is not what anyone installs — build it with `build.sh dev` when needed.
- `cinder-home/ROADMAP.md` now says which date it is a snapshot of. The forward plan is
  `docs/DEVICE_CHECKLIST.md`.
- **A release can no longer be cut without verification.** `tools/release.sh` rebuilds the stable
  channel and refuses to tag unless every committed byte matches — real, and until now entirely
  opt-in, because `git tag && git push --tags` reaches the workflow directly. The runner cannot
  repeat that comparison (no glibc-2.23 cross toolchain, which is why `dist/` is committed at all),
  so `release.sh` now records what it verified — every payload hash, plus the tag it was verified
  for — and `release.yml` checks the record still describes the tree. A tag cut without the script
  finds a manifest naming the previous version and fails. Shared script, so CI and a contributor run
  the same check (`tools/verify_payload_manifest.sh`). Not a signature; it closes the accident.

*Device-verified 2026-09-01 on an NW-A55: booted in 27.4 s with bootcount 0, library identical at
2560 tracks / 256 albums / 166 artists, zero guard recoveries, zero faults, and the `/contents`
reclaim observed running and succeeding at 13.3 s. Full write-up:
[`docs/AUDIT_2026-09-01.md`](docs/AUDIT_2026-09-01.md).*

## [0.1.5] — 2026-08-30

### Added

- Theme work and album-art handling in the UI (`theme.rs`, `art.rs`), and a wider `cinder-probe`
  (+327 lines of new probes).
- Harness coverage for the `pst` fake service.

### Fixed

- A batch of fixes across `main.cpp`, the FFI boundary and the navigation/settings screens.

## [0.1.4] — 2026-08-26

### Added

- **Device settings page** and a **battery UI**.
- Bluetooth and NFC work, with a full Bluetooth audit written up in
  [`docs/AUDIT_2026-08-26_bluetooth.md`](docs/AUDIT_2026-08-26_bluetooth.md).

### Changed

- Device checks completed and documented — see [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md).
- `build.sh` and `flash.sh` are executable by default again.

## [0.1.3] — 2026-08-20

### Added

- **One-click Windows install.** The installer now stages the payload and hands off to Sony's own
  `SoftwareUpdateTool.exe`, which performs the USB handoff and reboots the player into Cinder. No
  WSL, `usbipd`, or manual SCSI command.

### Changed

- Release workflow reworked: releases publish directly rather than as drafts, with `SHA256SUMS`
  attached.

## [0.1.2] — 2026-08-20

### Added

- **On-screen keyboard and playlist picker** — this device has no d-pad, so text entry is touch.
- **Liked songs**, with import and a TSV export that crosses to the PC
  ([`docs/LIKES_SYNC.md`](docs/LIKES_SYNC.md)).
- SD-card playlist and likes sync ([`docs/PLAYLISTS.md`](docs/PLAYLISTS.md)).
- Bluetooth disconnect handling, a Bluetooth socket observer, an HCI snoop-log decoder, and a
  self-test for the edge logic.
- Battery-consumption measurement tooling for Bluetooth playback (`tools/btpower*`).

### Changed

- Display backlight control and font fallback both reworked.
- Database rescans paced rather than run per poll.

### Fixed

- Playlist refresh produced duplicates and sorted wrongly.

## [0.1.1] — 2026-08-18

### Added

- **`cinder-fm`** — setuid-root helper giving userspace FM tuner register access via
  `/proc/regmon/Si4708icx`.
- **`cinder-voltable`** — volume-table installation helper.

## [0.1.0] — 2026-08-18

First tagged release.

### Added

- **FM tuner** support.
- **`cinder-clock`** — sets system time and the RTC. The device has no Sony service that sets the
  clock, and `cinder-home` runs as uid 100, so this is a setuid helper doing `settimeofday` plus the
  RTC ioctl.
- **A/B sound setups** — two complete saved sound configurations.
- Advanced sound controls.
- Device test documentation ([`docs/DEVICE_TESTS.md`](docs/DEVICE_TESTS.md)).

### Documented

- The wired-headphone volume-change pop: 26 pops below volume 100 against 1 above, and it is not
  the shell or any mixer control ([`docs/`](docs/)).

[Unreleased]: https://github.com/superwilso/Cinder/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/superwilso/Cinder/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/superwilso/Cinder/compare/v0.1.5...v0.1.7
[0.1.5]: https://github.com/superwilso/Cinder/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/superwilso/Cinder/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/superwilso/Cinder/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/superwilso/Cinder/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/superwilso/Cinder/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/superwilso/Cinder/releases/tag/v0.1.0
