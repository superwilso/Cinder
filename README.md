# Cinder

[![ci](https://github.com/superwilso/Cinder/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/superwilso/Cinder/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/superwilso/Cinder?sort=semver)](https://github.com/superwilso/Cinder/releases/latest)
[![downloads](https://img.shields.io/github/downloads/superwilso/Cinder/total)](https://github.com/superwilso/Cinder/releases)
[![license](https://img.shields.io/github/license/superwilso/Cinder)](LICENSE)
[![device](https://img.shields.io/badge/device-NW--A55%20%2F%20A50%20series-blue)](docs/baseline_v1.4.md)

**Custom firmware for the Sony NW-A55 Walkman** (NW-A50 series): a from-scratch replacement Home
app — native, ~3 MB, running in place of Sony's stock Qt player while keeping every one of Sony's
audio services (DSP, codecs, LDAC) intact underneath it.

<p align="center">
  <img src="docs/screenshots/now-playing.png" width="220" alt="Cinder now-playing screen">
  <img src="docs/screenshots/up-next.png" width="220" alt="Cinder Up Next queue">
  <img src="docs/screenshots/library-albums.png" width="220" alt="Cinder album library">
</p>

<p align="center"><sub><a href="#screenshots">More screenshots ↓</a></sub></p>

## Why

Stock firmware on this device is heavy, slow to boot, and blocks combinations the hardware is
perfectly capable of — most notably running **USB-DAC input and Bluetooth LDAC output at the
same time**, which stock refuses via a UI dialog, not a hardware limit. Cinder replaces only the
UI layer. It doesn't reimplement the audio stack: the Hagoromo services (`SoundServiceFw`,
`PlayerService`, `BtTransmitterService`, `EffectCtrlDmp`, …) are separate processes Cinder drives
over their existing binder IPC, which is what keeps EQ, DSEE HX, VPT, Vinyl and every other Sony
effect working exactly as before.

Full rationale and the living goals list: [`VISION.md`](VISION.md).

## Supported devices

Cinder is **custom firmware for the Sony NW-A50 series** — it replaces the player UI, not the
audio stack. What it has actually been built and run on:

| Model | State |
|---|---|
| **NW-A55** (NW-A55L / A55HN) | **Verified.** Every feature in [`STATUS.md`](cinder-home/STATUS.md) was measured on this hardware. |
| **NW-A56 / NW-A57** | Same `nw-a50` firmware family and the same MT8590 board — expected to work, but **nobody has run it on one**. If you have, an issue either way would be genuinely useful. |
| NW-A45 / A46 / A47 (A40 series) | **Not supported.** Same SoC family, different model firmware; Cinder has never been built or tested for it. |
| NW-A35 / A36 / A37 (A30 series) | **Not supported**, same reason. |
| ZX300, WM1A / WM1Z, DMP-Z1 | **Not supported.** For these, use [Wampy](https://github.com/unknown321/wampy), which covers the whole MT8590 family. |

If you are on a device Cinder does not target, the projects under [Related
projects](#related-projects) do cover it — that list is there to send you to the right one rather
than to keep you here.

## Status

This is a real reverse-engineering project against closed firmware, built and tested on actual
hardware, not a simulator. Current state, feature-by-feature: [`cinder-home/STATUS.md`](cinder-home/STATUS.md).
Forward plan: [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) — the ordered run sheet for
everything that needs the player in hand. (`cinder-home/ROADMAP.md` is a 2026-07-28 snapshot and
says so at the top; it is history, not the plan.)

Headline feature (USB-DAC → LDAC bridge) is proven on hardware. Bluetooth pairing/playback,
NFC tap-to-pair, and the rest of the UI are daily-usable with a handful of items still
device-gated — see STATUS.md for the exact matrix, it's kept current rather than aspirational.

## Install

Download **`cinder-installer-windows-x64.exe`** from the [latest release](../../releases/latest),
plug the Walkman in over USB in mass-storage mode, and run it. It finds the player, asks which
optional parts you want, copies them across, and tells you what to do next.

**On Linux, `cinder-installer-linux-x64` does the whole job — run it with `sudo`.** It stages the
files and then sends the upgrade command itself. That last step is a raw SCSI passthrough, which is
why it needs root; without it the installer still stages everything correctly and tells you what
did not happen and why.

**On macOS the installer stages but cannot finish.** The command is a vendor SCSI passthrough, and
macOS only exposes those through an IOKit `SCSITaskUserClient`, which the kernel will not grant for
a disk it has already mounted — the exact state a staged Walkman is in. Finish from a Linux or
Windows machine; see [`install.md`](install.md).

> **There is no update option in the player's own menus.** This generation has no such entry — the
> upgrade is always triggered by the host over USB. Earlier versions of this README and of the
> installer told you to find **Settings ▸ Device Settings ▸ Update** on the device. That menu does
> not exist, and the Linux binary refused to start at all, so neither path installed anything. Both
> are fixed in v0.1.9.

```
  Cinder installer  (channel: stable)
  player: D:\
  ------------------------------------------------------------
    1  [x]     Power off / Restart menu                   power
    2  [x]     USB mass storage (put music on the device) msc
    3  [x]     Set the clock and RTC                      clock
    4  [x]     Unmount helper for USB mass storage        umount
    5  [ ]     GPU present path (experimental, dev only)  gpunode
    6  <stock> Audio "sound signature"                    signature
  ------------------------------------------------------------
   <number> toggle/cycle   ?<number> describe   i install   q quit
```

Cinder is modular on purpose. Four of its parts are small setuid-root helpers, each buying one
specific feature (power off, USB mass storage, setting the clock) with one specific piece of
attack surface — so each is a choice rather than an assumption, and the descriptions in the picker
say what saying no actually costs you.

The `signature` option patches **three bytes** of Sony's audio HAL to pick which DAC path the
output stream uses and what CPU clock floor is held while playing. That is the entirety of what
Walkman One's paid "sound signature" does; Cinder reproduces its variants byte-for-byte from your
own stock library with **no firmware flash**, and adds three combinations Walkman One doesn't
ship, splitting its two effects apart so each can be judged separately. Derivation:
[`analysis/RE_walkmanone_extract.md`](analysis/RE_walkmanone_extract.md).

The installer only copies files — the firmware write is done by the player's own updater, which
reboots into itself when the host sends it the upgrade command (`SoftwareUpdateTool.exe` on
Windows, the same vendor SCSI command sent directly on Linux). Full walkthrough, every component explained, and the developer build:
**[`install.md`](install.md)**.

## Repo layout

| Path | What it is |
|---|---|
| `cinder-home/` | The C++ easel app — lifecycle, watchdogs, Sony-IPC glue, the LDAC bridge, `cinder-probe` (a no-boot-risk diagnostic binary) |
| `player/cinder-ui/` | The Rust UI — pure render + navigation state machine, no I/O |
| `player/cinder-ffi/` | The Rust↔C++ boundary: render tick, input, scrobbler, SQLite |
| `player/cinder-host/`, `player/cinder-sim/` | Host-side dev tools — render every screen to PNG, or drive the real navigator in a window, without a device |
| `installer/` | The end-user installer — dependency-free Rust, embeds the device binaries and the component catalogue, ships as a single `.exe` |
| `ldac-bridge/` | Standalone LDAC transmit research binary (superseded by the bridge now built into `cinder-home`, kept for the RE trail) |
| `analysis/` | Reverse-engineering findings — per-subsystem `RE_findings.md`, the extracted UI asset catalogue, IPC vtable maps |
| `docs/`, `phases/` | The host-side firmware-analysis pipeline (`make phase1`…`phase7`) and its output docs |
| `design/` | UI design references and handoff notes |

`CLAUDE.md` is the full environment setup + host pipeline + device procedure writeup — it
doubles as onboarding for a human contributor even though it was written for an AI pair.

## Building

```bash
cinder-home/build.sh dev      # or: stable — two channels from one tree
```

Needs a glibc-2.23 + libc++-3.9.0 cross toolchain matching the device's own runtime; `build.sh`
checks for both and exits with what's missing. Full environment setup (WSL2, cross-compilers,
the firmware-analysis pipeline): [`CLAUDE.md`](CLAUDE.md) Parts A–D.

To iterate on the UI without a device at all:

```bash
cd player && cargo build --release -p cinder-host   # renders every screen to PNG
# or drive it live:
cargo build --release -p cinder-sim --bin device     # 480x800 window, real navigator + input
```

To build the end-user installer (embeds whatever is in `cinder-home/dist/<channel>/`):

```bash
cd installer
CINDER_CHANNEL=stable cargo build --release                                  # native
CINDER_CHANNEL=stable cargo build --release --target x86_64-pc-windows-gnu   # .exe, from Linux
```

Releases are cut by `.github/workflows/release.yml` on a `v*` tag. It builds only the installer —
the ARM binaries under `cinder-home/dist/` are committed, so **build and commit `dist/` before
tagging**. See [`install.md`](install.md) for the whole pipeline and how to add a component.

## Cutting a release

Releases are automated, but with one hand-built step that cannot be automated away: the ARM
binaries. Building `cinder-home` needs a glibc-2.23 + libc++-3.9.0 cross toolchain matched to the
player's own runtime, so they are built by a maintainer and **committed** under
`cinder-home/dist/`. Only the installer is built in CI.

That split has exactly one dangerous failure mode — tagging a commit whose `dist/` is stale, which
ships an installer full of last week's binaries with a green tick and no warning. `tools/release.sh`
exists to make that impossible:

```sh
tools/release.sh v1.2.3 --dry-run   # verify everything, touch nothing
tools/release.sh v1.2.3             # verify, tag, push
```

It refuses to tag unless the tree is clean, `installer/Cargo.toml`'s version matches the tag, every
embedded payload file exists, **a fresh `build.sh stable` reproduces the committed `dist/` byte for
byte**, and the installer's own tests pass. It never commits anything — staging stays yours.

Pushing the tag is what triggers `.github/workflows/release.yml`, which builds the Windows and
Linux installers, attaches them plus the two `.upg` files and `SHA256SUMS` to a **published** (not
draft) GitHub release, and marks it pre-release if the tag has a suffix like `-rc1`.

Every other push runs `.github/workflows/ci.yml`, which builds and tests the player and the
installer on both platforms and checks the committed payload is complete and actually ARM — so a
tag is a formality rather than the first time anything gets compiled for Windows.

## Flashing and recovery

**Read [`RECOVERY.md`](RECOVERY.md) before flashing anything.** This device has no public
DFU/EDL recovery path — a bad flash means a full `wbrt` eMMC restore. The project's safety model
(bad-boot counter, crash supervisor, an escape ladder ordered so each rung depends on strictly
less than the one it rescues) exists because of a real brick during development, documented
there. `cinder-home/STATUS.md` STEP 1 is the zero-risk way to test a build before ever flashing
it as the Home app.

## Documentation

[`docs/README.md`](docs/README.md) is the index — it says which document answers which question,
and which ones are history rather than current state.

The four that are always current:

| | |
|---|---|
| [`RECOVERY.md`](RECOVERY.md) | **Read before flashing.** No public DFU or EDL path exists for this device. |
| [`cinder-home/STATUS.md`](cinder-home/STATUS.md) | The feature matrix — current state, kept current rather than aspirational. |
| [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) | The run sheet for anything that needs the player in your hand. |
| [`CHANGELOG.md`](CHANGELOG.md) | What changed in each release, and whether it was verified on hardware. |

## Related projects

Cinder exists because these did the groundwork, and each of them is the right answer to a question
Cinder is the wrong answer to:

| | |
|---|---|
| [**Wampy**](https://github.com/unknown321/wampy) (unknown321) | A skinnable replacement UI across the whole MT8590 Walkman family — A30/A40/A50, ZX300, WM1A/Z, DMP-Z1. Broader device support than Cinder by a long way, and its `MAKING_OF` write-ups are the map this platform was first charted with. |
| [**wbrt**](https://github.com/unknown321/wbrt) (unknown321) | Full eMMC backup and restore over the MediaTek VCOM port. **The brick insurance for every project on this list** — including this one. |
| [**Walkman One**](https://www.mrwalkman.com/) (MrWalkman) | Modified stock firmware: Sony's own player with the region and feature locks removed. If you want stock-but-unlocked rather than a different player, this is it. |
| [**Rockbox `nwztools`**](https://github.com/Rockbox/rockbox/tree/master/utils/nwztools) | The `.UPG` pack/unpack tooling, per-model KAS keys and the NVP slot map that make any of this reachable. |
| [**scrobbler**](https://github.com/unknown321/scrobbler) (unknown321) | Last.fm scrobbling on-device; Cinder writes the same `.scrobbler.log` format. |

## Contributing

Issues and PRs welcome — this covers everything from firmware RE to UI work to just testing on
your own device. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md), which has the local check
commands and the safety rules for anything that runs as root or touches the boot path.

**The single most useful contribution is a device report.** A large part of this project is
code-complete and unverified on hardware; if you own an A50-series player, running one line from
[`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) and filing the result — pass *or* fail —
moves things that no amount of desk work can.

`analysis/` is the research trail; if you're picking up an open question there, that's the place
to start. [`docs/AUDIT_2026-09-01.md`](docs/AUDIT_2026-09-01.md) Part D is the current list of open
decisions.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## Screenshots

Every shot is a real render of the shipping UI at the device's native 480×800, produced by the
`cinder-host` preview harness (`cargo run -p cinder-host`) rather than a mockup. Cover art is
generated placeholder gradients — the harness has no library of its own, and using real album art
here would be someone else's copyright.

### Playing

| Now playing | Up Next | Reordering | Volume |
|---|---|---|---|
| <img src="docs/screenshots/now-playing.png" width="190" alt="Now playing"> | <img src="docs/screenshots/up-next.png" width="190" alt="Up Next"> | <img src="docs/screenshots/up-next-reorder.png" width="190" alt="Dragging a row to reorder"> | <img src="docs/screenshots/volume.png" width="190" alt="Volume overlay"> |

The queue is one list: history, the playing track, your hand-picked queue, and the rest of the
album it came from. Any row below the playing one can be dragged by its handle — including the
`NEXT FROM` section, so you can rearrange the album you're already listening to without
interrupting it.

### Library

| Albums | Songs | Artists | Playlists |
|---|---|---|---|
| <img src="docs/screenshots/library-albums.png" width="190" alt="Album list"> | <img src="docs/screenshots/library-songs.png" width="190" alt="Song list"> | <img src="docs/screenshots/library-artists.png" width="190" alt="Artist list"> | <img src="docs/screenshots/library-playlists.png" width="190" alt="Playlists"> |

| Album | Artist | Folders | Track info |
|---|---|---|---|
| <img src="docs/screenshots/album.png" width="190" alt="Album track list"> | <img src="docs/screenshots/artist.png" width="190" alt="Artist page"> | <img src="docs/screenshots/folders.png" width="190" alt="Folder browser"> | <img src="docs/screenshots/track-info.png" width="190" alt="Track information"> |

### Sound

| Equalizer | Sony DSP | Bluetooth | USB-DAC |
|---|---|---|---|
| <img src="docs/screenshots/equalizer.png" width="190" alt="10-band equalizer"> | <img src="docs/screenshots/sound.png" width="190" alt="Sound effects"> | <img src="docs/screenshots/bluetooth.png" width="190" alt="Bluetooth and LDAC"> | <img src="docs/screenshots/usb-dac.png" width="190" alt="USB-DAC mode"> |

`Sound` drives Sony's own effect services, so DSEE HX, VPT, DC Phase Linearizer, Vinyl Processor
and the rest behave exactly as they do on stock — the footer prints the resulting signal path.

### The rest

| Settings | Shelf | Lock screen | FM radio |
|---|---|---|---|
| <img src="docs/screenshots/settings.png" width="190" alt="Settings"> | <img src="docs/screenshots/shelf.png" width="190" alt="Shelf shortcuts"> | <img src="docs/screenshots/lock.png" width="190" alt="Lock screen"> | <img src="docs/screenshots/fm-radio.png" width="190" alt="FM radio"> |

### Visualisers and themes

| Bars | Ribbon | Night — playing | Night — library |
|---|---|---|---|
| <img src="docs/screenshots/visualiser-bars.png" width="190" alt="Bar visualiser"> | <img src="docs/screenshots/visualiser-ribbon.png" width="190" alt="Ribbon visualiser"> | <img src="docs/screenshots/now-playing-night.png" width="190" alt="Now playing, night theme"> | <img src="docs/screenshots/library-albums-night.png" width="190" alt="Library, night theme"> |

Eight visualisers, six accent colours, and a **night theme that is genuinely dim** — it is meant
for a dark room at low brightness, which is why it looks nearly black next to the day theme rather
than merely dark-grey.

## License

Cinder's own code (`cinder-home/`, `player/`, `ldac-bridge/`, `tools/`) is MIT — see
[`LICENSE`](LICENSE).

**Third-party / not ours:** `analysis/ui_assets/` contains UI graphics extracted directly from
Sony's stock firmware for reference during development — that's Sony's copyrighted art, kept here
as research material, not covered by this project's license, and not an original contribution of
this project. Bundled fonts (`player/cinder-ui/assets/fonts/`) are SIL Open Font License 1.1 —
see the `*-OFL.txt` next to each family. `analysis/`'s pipeline references the Rockbox project
(`nwztools`, GPL) for `.UPG` packing/unpacking and per-model firmware keys; that tooling is used
as an external build dependency, not vendored into this repo.

This project is not affiliated with or endorsed by Sony. "Walkman" and related marks belong to
Sony Corporation.

As a disclaimer, a large amount of the reverse engineering work, documentation and rust and C++ code were writen by claude.
All work was supervised and checked by a human.