# Cinder

[![ci](https://github.com/superwilso/Cinder/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/superwilso/Cinder/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/superwilso/Cinder?sort=semver&include_prereleases)](https://github.com/superwilso/Cinder/releases)
[![downloads](https://img.shields.io/github/downloads/superwilso/Cinder/total)](https://github.com/superwilso/Cinder/releases)
[![license](https://img.shields.io/github/license/superwilso/Cinder)](LICENSE)

**A replacement music player for the Sony NW-A55 Walkman and the rest of the NW-A50 series.**
It swaps Sony's music app for a faster one. Sony's audio services, effects and DAC path underneath
stay as they are. You install it from a PC over USB, and the same installer puts Sony's app back.

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

- **A fast player.** About 4.6 MB of native code with no UI framework, instead of Sony's Qt app.
- **A real queue.** Up Next with "play next", drag to reorder, and playlists made on the player.
- **USB-DAC in and LDAC out at the same time.** Use the Walkman as a PC's sound card and pass the
  audio on to Bluetooth headphones. Sony's app blocks this with a dialog; the hardware can do it.
- **Every Sony sound feature**: DSEE HX, VPT, DC Phase Linearizer, Vinyl Processor, the 10-band EQ
  and tone control. These are Sony's own services, driven by Cinder, not reimplemented.
- **Built-in extras**: a `.scrobbler.log` scrobbler, lyrics (`.lrc` or embedded tags), library
  search, SensMe™ channels, FM radio with a real signal meter, and colour palettes you can drop on
  the drive.
- **Your library as it is.** Cinder reads the same database as Sony's player, so music, playlists
  and liked songs stay put.

## Status

It works as a daily player on the developer's own unit. Each [`CHANGELOG.md`](CHANGELOG.md) entry says
whether it has run on hardware (*device-verified*) or not yet. Feature by feature:
[`cinder-home/STATUS.md`](cinder-home/STATUS.md).

**Known issues**

- **v0.3.9 does not start after a fresh install.** The player keeps showing Sony's app
  ([#16](../../issues/16)). This is fixed in **0.3.11-rc1 and later**, so use the newest release
  from the [releases page](../../releases), including pre-releases, until the next stable one.
- **Walkman One (Mr Walkman):** the release package is rejected. Walkman One changes the key the
  player's updater accepts, so the updater drops the package silently and the player restarts
  unchanged. From the next release, the installer warns when it sees Walkman One's `CFW` folder.
  Cinder itself does run on Walkman One 3.02 (device-verified 2026-09-21), but getting it there
  needs a differently sealed package that has not been tested yet. Details: [Coming from Walkman One](#coming-from-walkman-one).
- **Not built yet:** Bluetooth receiver mode (the Walkman as a speaker) and FM recording.
- **Lyrics, search and SensMe channels are new** and not yet confirmed on a player's screen.
- **One test unit.** Other NW-A50-series models share its firmware but have not been tried.

## Supported players

| Model | State |
|---|---|
| NW-A55 / A56 / A57 (NW-A50 series) | **The target.** Developed on one 64 GB unit; the other models are expected to work but have not been tried. A [device report](../../issues/new/choose) either way helps. |
| NW-A50 series running Walkman One | See Known issues above. |
| A40 / A30 series, ZX300, WM1A/Z, DMP-Z1 | **Not supported.** Use [Wampy](https://github.com/unknown321/wampy), for now, which covers the whole MT8590 family. If you have the device and are willing to test, get in contact through github.|

## Install

1. **Back up the player with [wbrt](https://github.com/unknown321/wbrt).** This device has no other
   recovery path.
2. Download the installer from the [releases page](../../releases):
   - **Windows:** `cinder-installer-windows-x64.exe`. Accept the administrator prompt, because the
     last step needs raw access to the drive.
   - **Linux:** `cinder-installer-linux-x64`, run with `sudo`.
   - **macOS** can stage the files but cannot send the final command. Use Windows or Linux.
3. Connect the Walkman in USB mass-storage mode and run the installer. Choose **Install**, pick the
   optional parts, and confirm. The player reboots into its own updater, applies the package, and
   starts into Cinder. **Leave the cable in until it does.**

The installer has **Install**, **Update** (keeps your previous choices) and **Uninstall** (puts
Sony's app back; music and settings are untouched). It reads the player's own logs, so it says
what is really installed. From the next release, it also says why the last start went to Sony's
player, if it did. The `.exe` is unsigned: check it against the `SHA256SUMS` attached to each
release.

Full walkthrough, every option explained, and the developer build: [`install.md`](install.md).
Removing it: [`UNINSTALL.md`](UNINSTALL.md).

## If something goes wrong

The player has an **escape ladder**. Each step depends on less than the one before it.

- **Boot with the USB cable connected** and you get Sony's player. This needs no filesystem. The
  one exception is the first boot after an install, which ignores the cable once.
- **Put a file named `cinderhome_off` on the drive** and you get Sony's player until it is removed.
  A file named `cinderhome_clear` clears a latched failure and tries Cinder again.
- **A build that fails to start** reverts to Sony's player on its own after four boots.

Read [`RECOVERY.md`](RECOVERY.md) **before** you need it. Cinder's log is `cinderhome.log` in the
root of the drive, and it is the first thing to attach to an issue.

## Coming from Walkman One

- **Sound:** Walkman One's "plus modes" are a 3-byte change to Sony's audio library. Cinder
  reproduces them from your own stock files with no flashing: the `signature` install option. Its
  external tunings are other models' firmware, which Cinder cannot reproduce. Measured at the jack,
  neither changes the signal
  ([unknown321's measurements](https://github.com/unknown321/wampy/blob/master/MAKING_OF_VOLUME_TABLES.md#there-is-more)).
- **Volume curve:** the NW-WM1A curve is available as an install option if you supply Sony's table
  file ([how](install.md#the-volume-curve-tables)).
- **Installing over Walkman One:** not with a release package yet (see Known issues). If you want
  to help, run `nvpstr kas` on your player and post the answer in an issue.
- **Reverting to stock first** works: Cinder installs on stock 1.02. If you reverted and Cinder
  still does not start, you are almost certainly on v0.3.9 ([#16](../../issues/16)). Use a newer
  release.

Cinder's own teardown of Walkman One: [`analysis/RE_walkmanone_extract.md`](analysis/RE_walkmanone_extract.md).

## Contributing and building

Issues and pull requests are welcome, from reverse engineering to UI work to testing.
**The most useful contribution is a device report:** run one item from
[`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) and file the result, pass or fail.
[`CONTRIBUTING.md`](CONTRIBUTING.md) has the local checks, the safety rules for boot-path code, and
how releases are cut. The docs index is [`docs/README.md`](docs/README.md).

| Path | What it is |
|---|---|
| `cinder-home/` | The C++ Home app: lifecycle, watchdogs, the glue to Sony's services, the LDAC bridge, the installer payload |
| `player/` | The Rust UI (`cinder-ui`), its C boundary (`cinder-ffi`), the library reader, and host tools that render every screen to PNG without a device |
| `installer/` | The end-user installer: Windows GUI and text interface, dependency-free Rust |
| `analysis/`, `docs/` | Reverse-engineering notes, audits and plans |

To work on the UI with no device: `cd player && cargo run --release -p cinder-host` renders every
screen to `player/out/`. The device build needs a matched cross toolchain; see
[`install.md`](install.md) and [`CLAUDE.md`](CLAUDE.md) Parts A–D.

## Related projects

| | |
|---|---|
| [Wampy](https://github.com/unknown321/wampy) | Skinnable replacement UI for the whole MT8590 Walkman family. Its `MAKING_OF` write-ups mapped this platform first. |
| [wbrt](https://github.com/unknown321/wbrt) | Full eMMC backup and restore. The brick insurance for every project here. |
| [Walkman One](https://www.mrwalkman.com/) | Sony's own player with the region and feature locks removed. |
| [Rockbox `nwztools`](https://github.com/Rockbox/rockbox/tree/master/utils/nwztools) | `.UPG` packing, per-model keys and the NVP map that make any of this possible. |
| [scrobbler](https://github.com/unknown321/scrobbler) | On-device Last.fm scrobbling. If it is installed, Cinder's own scrobbler stands down. |
| [Flint](https://github.com/superwilso/flint) / [Sony-sync](https://github.com/superwilso/Sony-sync) | The developer's PC sync tools. Flint writes SensMe data, likes and scrobbles. |

## Screenshots

These are real renders of the shipping UI at the player's 480×800, made by `cinder-host`. The cover
art is generated.

| Up Next, reordering | Album | Track info | Sound |
|---|---|---|---|
| <img src="docs/screenshots/up-next-reorder.png" width="190" alt="Reordering Up Next"> | <img src="docs/screenshots/album.png" width="190" alt="Album"> | <img src="docs/screenshots/track-info.png" width="190" alt="Track info"> | <img src="docs/screenshots/sound.png" width="190" alt="Sony sound effects"> |

| Bluetooth | USB-DAC | FM radio | Night theme |
|---|---|---|---|
| <img src="docs/screenshots/bluetooth.png" width="190" alt="Bluetooth and LDAC"> | <img src="docs/screenshots/usb-dac.png" width="190" alt="USB-DAC"> | <img src="docs/screenshots/fm-radio.png" width="190" alt="FM radio"> | <img src="docs/screenshots/now-playing-night.png" width="190" alt="Night theme"> |

## License

Cinder's own code is MIT ([`LICENSE`](LICENSE)). The bundled fonts are SIL OFL 1.1 (see the
`*-OFL.txt` beside each). The repository holds no Sony code or artwork: the reverse-engineering
notes describe interfaces, and the material they were worked out from is kept out of the tree.
Rockbox's `nwztools` (GPL) is used as an external tool, not vendored.

Not affiliated with or endorsed by Sony. "Walkman" and "SensMe" are Sony's marks.

As a disclaimer, a large amount of the reverse engineering work, documentation, and Rust and C++
code was written by Claude. All work was supervised and checked by a human.
