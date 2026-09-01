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

### Fixed

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

[Unreleased]: https://github.com/superwilso/Cinder/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/superwilso/Cinder/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/superwilso/Cinder/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/superwilso/Cinder/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/superwilso/Cinder/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/superwilso/Cinder/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/superwilso/Cinder/releases/tag/v0.1.0
