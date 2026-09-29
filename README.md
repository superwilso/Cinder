# Cinder

[![ci](https://github.com/superwilso/Cinder/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/superwilso/Cinder/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/superwilso/Cinder?sort=semver&include_prereleases)](https://github.com/superwilso/Cinder/releases)
[![downloads](https://img.shields.io/github/downloads/superwilso/Cinder/total)](https://github.com/superwilso/Cinder/releases)
[![license](https://img.shields.io/github/license/superwilso/Cinder)](LICENSE)

**A replacement music player for the Sony NW-A55 Walkman and the rest of the NW-A50 series.**
It replaces Sony's music app with a faster one and keeps Sony's audio services, effects and DAC path
underneath. Install it from a PC over USB; the same installer puts Sony's app back.

> This is a personal project that I made for myself. If people want to tweak it and adapt it to
> their own use, please do, but I can't promise features or time for it. Raise issues in the issues
> section, but I can't promise they will be patched. **Back up with
> [wbrt](https://github.com/unknown321/wbrt) before you install**, and if you have the time and
> skill, please contribute pull requests. I have deliberately not included any way to sponsor or pay
> for this project. I want it to be free, and I don't want the obligation for support that comes
> with payment; as I said, I made this for me. The best way to support it is to test, give feedback
> and patch. **Cinder is still in testing and development, whatever GitHub's labels say.**

<p align="center">
  <img src="docs/screenshots/now-playing.png" width="220" alt="Now playing">
  <img src="docs/screenshots/up-next.png" width="220" alt="Up Next queue">
  <img src="docs/screenshots/library-albums.png" width="220" alt="Album library">
</p>

## What you get

- **Fast.** About 4.6 MB of native code, no UI framework. Sony's app is Qt.
- **A real queue.** Up Next with play next, drag to reorder, and playlists made on the player.
- **USB-DAC in, LDAC out, at once.** Use the Walkman as a PC sound card and send the audio on to
  Bluetooth headphones. Sony's app blocks this; the hardware doesn't.
- **All of Sony's sound features:** DSEE HX, VPT, DC Phase Linearizer, Vinyl Processor, the 10-band
  EQ and tone control. Cinder drives Sony's own services rather than reimplementing them.
- **Better battery.** With the screen off, Cinder powers the display hardware down and lets the chip
  reach its deepest idle state. Sony's audio path is untouched.
- **Extras:** a `.scrobbler.log` scrobbler, lyrics (`.lrc` or embedded), library search, SensMe™
  channels, FM radio with a signal meter, and colour palettes you drop on the drive.
- **Your layout.** Open on the Library, Now Playing, the Menu or wherever you left off. Optional
  pull-down panel for brightness, Bluetooth, night mode and the sleep timer.
- **Your library as it is.** Cinder reads Sony's database, so music, playlists and liked songs stay.

## Status

A daily player on the developer's own unit. Every [`CHANGELOG.md`](CHANGELOG.md) entry says whether
it has run on hardware (*device-verified*). Per-feature state: [`cinder-home/STATUS.md`](cinder-home/STATUS.md).

**Known issues**

- **v0.3.9 doesn't start after a fresh install** ([#16](../../issues/16)). Fixed in **v0.3.12**:
  install it over the top.
- **Walkman One:** the release package is rejected, silently. Walkman One changes the key the
  updater accepts. The installer warns when it sees Walkman One's `CFW` folder. Cinder itself runs on
  Walkman One 3.02, but a package sealed for it hasn't been tested yet. See
  [Coming from Walkman One](#coming-from-walkman-one).
- **Not built yet:** Bluetooth receiver mode (Walkman as a speaker) and FM recording.
- **One test unit.** Other NW-A50-series models share its firmware but haven't been tried.

## Supported players

| Model | State |
|---|---|
| NW-A55 / A56 / A57 (NW-A50 series) | **The target.** Developed on one 64 GB A55. Other models should work; a [device report](../../issues/new/choose) either way helps. |
| NW-A50 series on Walkman One | See Known issues. |
| A40 / A30 series, ZX300, WM1A/Z, DMP-Z1 | **Not supported.** Use [Wampy](https://github.com/unknown321/wampy), which covers the whole MT8590 family. If you own one and can test, get in touch through GitHub. |

## Install

1. **Back up the player with [wbrt](https://github.com/unknown321/wbrt).** There is no other
   recovery path for this device.
2. Download the installer from [Releases](../../releases):
   - **Windows:** `cinder-installer-windows-x64.exe`. Accept the administrator prompt; the last step
     needs raw drive access.
   - **Linux:** `cinder-installer-linux-x64`, run with `sudo`.
   - **macOS** can stage the files but can't send the final command. Use Windows or Linux.
3. Connect the Walkman in USB mass-storage mode, run the installer, choose **Install** and your
   options. The player reboots into its updater, installs, and starts Cinder. **Leave the cable in
   until it does.**

The installer also has **Update** (keeps your choices) and **Uninstall** (restores Sony's app; music
and settings are untouched). It reads the player's logs, so it reports what is really installed.
The `.exe` is unsigned; check it against `SHA256SUMS` on the release.

Every option, and the developer build: [`install.md`](install.md). Removing it: [`UNINSTALL.md`](UNINSTALL.md).

## If something goes wrong

Each way back to Sony's player depends on less than the one before:

- **Boot with the USB cable connected** → Sony's player. Needs no filesystem. (The first boot after
  an install ignores the cable once.)
- **A file named `cinderhome_off` on the drive** → Sony's player until you delete it.
  `cinderhome_clear` clears a latched failure and tries Cinder again.
- **A build that fails to start** reverts to Sony's player by itself after four boots.

Read [`RECOVERY.md`](RECOVERY.md) **before** you need it. Attach `cinderhome.log` (root of the
drive) to any issue.

## Coming from Walkman One

- **Sound:** Walkman One's plus modes are a 3-byte change to Sony's audio library. Cinder's
  `signature` install option makes the same change to your own stock files, no flashing. Its
  external tunings are other models' firmware and can't be reproduced. Measured at the jack, neither
  changes the signal ([unknown321's measurements](https://github.com/unknown321/wampy/blob/master/MAKING_OF_VOLUME_TABLES.md#there-is-more)).
- **Volume curve:** Cinder removes Sony's regional volume limit by default; the `region` install
  option keeps it. The NW-WM1A curve is an install option if you supply Sony's table file
  ([how](install.md#the-volume-curve-tables)).
- **Installing over Walkman One:** not with a release package yet. To help, run `nvpstr kas` on your
  player and post the output in an issue.
- **Reverting to stock first** works; Cinder installs on stock 1.02. If it then doesn't start,
  you're on v0.3.9 ([#16](../../issues/16)): use a newer release.

Teardown of Walkman One: [`analysis/RE_walkmanone_extract.md`](analysis/RE_walkmanone_extract.md).

## Contributing and building

Issues and pull requests are welcome. **The most useful contribution is a device report:** run one
item from [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) and post the result, pass or fail.
[`CONTRIBUTING.md`](CONTRIBUTING.md) covers the local checks, the rules for boot-path code, and
releases. Docs index: [`docs/README.md`](docs/README.md).

| Path | Contents |
|---|---|
| `cinder-home/` | The C++ Home app: lifecycle, watchdogs, glue to Sony's services, the LDAC bridge, the installer payload |
| `player/` | The Rust UI (`cinder-ui`), its C boundary (`cinder-ffi`), the library reader, and host tools that render every screen to PNG |
| `installer/` | The end-user installer (Windows GUI and text UI), dependency-free Rust |
| `analysis/`, `docs/` | Reverse-engineering notes, audits and plans |

UI work needs no device: `cd player && cargo run --release -p cinder-host` renders every screen to
`player/out/`. The device build needs a matched cross toolchain; see [`install.md`](install.md) and
[`CLAUDE.md`](CLAUDE.md) Parts A–D.

## Related projects

| | |
|---|---|
| [Wampy](https://github.com/unknown321/wampy) | Skinnable replacement UI for the whole MT8590 Walkman family. Its `MAKING_OF` write-ups mapped this platform first. |
| [wbrt](https://github.com/unknown321/wbrt) | Full eMMC backup and restore. Brick insurance for everything here. |
| [Walkman One](https://www.mrwalkman.com/) | Sony's player with region and feature locks removed. |
| [Rockbox `nwztools`](https://github.com/Rockbox/rockbox/tree/master/utils/nwztools) | `.UPG` packing, per-model keys and the NVP map. |
| [scrobbler](https://github.com/unknown321/scrobbler) | On-device Last.fm scrobbling. If installed, Cinder's scrobbler steps aside. |
| [Flint](https://github.com/superwilso/flint) / [Sony-sync](https://github.com/superwilso/Sony-sync) | The developer's PC sync tools. Flint writes SensMe data and syncs likes and scrobbles. |

## Screenshots

All 38 screens, from Now Playing to the Windows installer: [`docs/SCREENSHOTS.md`](docs/SCREENSHOTS.md).

## License

Cinder's code is MIT ([`LICENSE`](LICENSE)). Bundled fonts are SIL OFL 1.1 (`*-OFL.txt` beside
each). The repository contains no Sony code or artwork; the reverse-engineering notes describe
interfaces only. Rockbox's `nwztools` (GPL) is used as an external tool, not vendored.

Not affiliated with or endorsed by Sony. "Walkman" and "SensMe" are Sony's marks.

As a disclaimer, a large amount of the reverse engineering work, documentation, and Rust and C++
code was written by Claude. All work was supervised and checked by a human.
