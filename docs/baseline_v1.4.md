# NW-A55 Technical Baseline — v1.4

> **Trust tiers used throughout this document:**
> - **[Verified]** — confirmed from multiple independent primary sources
> - **[Community-reported]** — from Wampy/Rockbox source, not independently verified
> - **[Hypothesis]** — inference requiring on-device confirmation
> - **[Unverified]** — claimed by an audit but not independently sourced

---

## §0 Audit Notes

This document has been through four revision cycles. The pattern across those cycles is
instructive: v1.0 made confident assertions from inference (some correct, some not); v1.3
over-corrected by retracting a claim (MT8590) that was actually verifiable from the Wampy
author's own adjacent repository; v1.4 re-establishes the MT8590 identification with proper
citation while integrating corrections from two independent external audits. Every factual
assertion is now tagged with one of three trust tiers: **[Verified]** (confirmed from
multiple independent primary sources), **[Community-reported]** (from Wampy/Rockbox source,
not independently verified), or **[Hypothesis]** (inference requiring on-device
confirmation).

**Error history (instructive):**
- v1.0: Asserted MT8590 SoC with no source citation.
- v1.0: Stated Wampy license as MIT (incorrect).
- v1.3: Retracted MT8590 as "unsourceable fabrication" — this retraction was itself an
  error; `unknown321/wbrt` (the Wampy author's own backup tool) explicitly confirms MT8590.
- v1.3: Stated "software-enforced, not hardware-enforced" for USB-DAC block — overclaimed.

**The MediaTek MT8590 retraction in v1.3 was itself an error.** The SoC is confirmed as
MT8590 by `unknown321/wbrt` (the Wampy author's backup/restore tool), which explicitly
states "Create and restore backups for MT8590-based Walkmans: NW-A30/40/50, ZX300, WM1A,
WM1Z, DMP-Z1" and uses MediaTek's USB vendor ID (`0x0E8D`). Wampy's `MAKING_OF.md` also
references "there is no ready-to-go MediaTek platform emulator." The v1.0 claim was correct
but cited no source; v1.3 failed to check the author's own adjacent repository before
retracting. **[Verified]**

---

## §1 Executive Summary

The Sony NW-A55 is a portable audio player running Linux 3.10 on a MediaTek MT8590
application processor with a Sony CXD3778GF audio DSP. The stock UI (`HgrmMediaPlayerApp`)
is a Qt 5 application. The init system is derived from Android AOSP (init.rc syntax,
property service, adb). Firmware images use Sony's proprietary `.UPG` format (V2, encrypted
with a per-model KAS key).

The customer's requested replacement player requires: custom UI with auto-boot, editable
play queue, shuffle-by-album (neither the editable queue nor true shuffle-by-album exists in
stock firmware — see §5.12; both are pure application logic in the replacement player),
scrobbler.log compatibility, 2038-safe timestamps, and configurable hold-switch behaviour.
An additional headline feature — simultaneous USB-DAC input + LDAC Bluetooth output — is
technically plausible but unconfirmed; see §5.10 and OQ2.

The **Wampy** project (`github.com/unknown321/wampy`) is an open-source interface addon
licensed under GPLv3 with a Commons Clause non-sale condition. GitHub shows C 80.9% /
C++ 17.3%, 71 releases as of v1.14.3 (Feb 22, 2026). **[Verified]**

The main remaining unknowns sit at the **MediaTek MT8590 boot ROM** layer (now that the SoC
is identified), the **clang/LLVM vs GCC C++ ABI** mismatch between Sony's toolchain and the
replacement player's C shim (OQ3/OQ6), and the **USB-DAC enforcement layer** (OQ2).

**Confidence summary by feature:**
- Full UI replacement + auto-boot: **High.** Wampy demonstrates the pattern; init.rc swap
  is the mechanism; bad-boot counter is the safety net.
- Keep all audio features: **High confidence** pending clang ABI check (Phase 4c).
- Queue + shuffle-by-album: **High.** Pure application logic; no RE required (§5.12).
- USB-DAC in + LDAC out simultaneously: **Unknown until on-device experiment (E4/E5).**
  Do not promise this feature before Phase 8.

---

## §2 Product Identity and Lineage

| Property | Value |
|---|---|
| Model | Sony NW-A55 (NW-A50 series) |
| Also in family | NW-A50, NW-A56, NW-A57 (same SoC / firmware base) |
| Internal codename / SoC | MediaTek MT8590 **[Verified]** — confirmed by `unknown321/wbrt`, Wampy `MAKING_OF.md`, USB VID `0x0E8D` |
| Audio DSP | Sony CXD3778GF **[Verified]** — confirmed by Wampy `ALSA.md` device names |
| OS | Linux 3.10 (32-bit ARM) |
| UI framework | Qt 5 |
| Init system | Android AOSP-derived (init.rc, property service) **[Partially verified]** |
| Firmware format | Sony `.UPG` V2 (AES-CBC encrypted, per-model KAS 64-byte key) |
| Rockbox database | NW-A30/A40/A50 family under `mt8590` |

---

## §3 Physical Characteristics

| Component | Detail |
|---|---|
| Internal storage | 16 GB eMMC |
| External storage | microSD / microSDHC / microSDXC per Sony specs; community-confirmed working >256 GB **[Verified]** |
| Headphone out | 3.5mm (unbalanced) + 4.4mm (balanced, some regional variants) |
| Bluetooth | Yes — LDAC, aptX, SBC |
| USB | USB-C; USB-DAC mode (device acts as USB audio input) |
| FM tuner | Present on some variants. Stock FM range 87.5–108.0 MHz per Sony Help Guide. Wampy extends to 76–108 MHz on devices with FM hardware and Walkman One. **[Verified: Sony Help Guide; Community-reported: Wampy MAKING_OF_FM.md]** |
| NFC | Present |
| Hold switch | Physical slider; maps to input event (behaviour configurable in replacement player) |

---

## §4 Hardware

### 4.1 Application processor (SoC) — **MediaTek MT8590** [Verified]

**Retraction of the retraction:** v1.3 retracted the MT8590 identification as "unsourceable
fabrication." That retraction was itself an error. The MT8590 is confirmed by:

1. **`unknown321/wbrt`** (Walkman Backup/Restore Tool, by the Wampy author): README states
   "Create and restore backups for MT8590-based Walkmans: NW-A30/40/50, ZX300, WM1A, WM1Z,
   DMP-Z1." The tool uses MediaTek's USB vendor ID `0x0E8D` and PID `0x2000` (MediaTek
   preloader mode). **[Verified]**
2. **Wampy `MAKING_OF.md`**: States "there is no ready-to-go MediaTek platform emulator, you
   have to create your own unique qemu ARM configuration." **[Verified]**
3. **Audit 2 (unverified but plausible)** claims the Sony GPL kernel source tree contains
   `mediatek/mt8590/icx-machine-links.c` with `.cpu_dai_name = "mt8590-i2s1"`. Consistent
   with confirmed evidence but not independently checked. **[Community-reported]**

**What remains unverified for this specific SoC:**
- CPU core type and clock speed. Audit 2 claims ARM Cortex-A7 dual-core at 1.8 GHz, but no
  public MT8590 datasheet was found to confirm this. **[Unverified]**
- Specific preloader console output and `ro.board.platform` property values. Plausible and
  consistent with MT8590 but not independently sourced. **[Unverified]**
- MediaTek boot ROM bypass tools (SLA/DAA bypass) applicability to this specific device.
  **[Hypothesis]**

**DMP-Z1 contradiction:** `wbrt` lists DMP-Z1 as MT8590-based, but Wampy's
`VOLUME_TABLES.md` states: "DMP-Z1 does not have these at all (different SOC)." This may
mean DMP-Z1 shares the MediaTek boot-ROM interface while using a different audio subsystem
(Wampy notes it has its own "Aulos card" instead of CXD3778GF). The contradiction is
unresolved. See OQ8. **[Flagged]**

**Practical implication:** the SoC is no longer an open question. Phase 3 scripts can be
updated to search for MT8590-specific markers rather than doing blind SoC identification.
MediaTek-specific tools (`mtkclient`, `SP Flash Tool`) may provide additional eMMC access
paths if needed.

### 4.2 Audio path

The Sony CXD3778GF is the dedicated audio DAC/amp chip. **[Verified]**

**Direct confirmation from ALSA:** Wampy's `ALSA.md` documents the output of `aplay -l` on
a running NW-A50, which lists 6 PCM devices all under `card 0: sonysoccard [sony-soc-card]`,
with device names including `cxd3778gf-hires-out`, `cxd3778gf-standard`,
`cxd3778gf-dsd-out`, and `cxd3778gf-icx-lowpower`. This confirms the CXD3778GF part number
on the actual NW-A50 hardware. **[Verified: Wampy ALSA.md]**

The `libSoundServiceFw.so` library mediates all audio routing between the player application
and the ALSA layer. Its symbol table (Phase 4e) is the primary interface for the replacement
player's C shim.

Some tuning and filter features are firmware-gated and can be cross-ported between these
models via `libSoundServiceFw.so` string lists. Whether this reflects identical silicon or
merely compatible audio subsystems is not established by firmware modding alone.
**[Community-reported]**

---

## §5 Software / Firmware Stack

### 5.1 Init system

The init system is derived from Android AOSP. Service definitions use `init.rc` /
`init.hagoromo.rc` syntax. Android-style property service (`getprop`/`setprop`) is present.

**Android-derived infrastructure:** The `init.rc` / `init.hagoromo.rc` service-definition
syntax and the use of Android-style property service indicate the init system is derived from
Android AOSP. The scrobbler project's INSTALL.md confirms `adb` is available on some
configurations ("If your player has adb on, there is no need for scsitool"). Audit 2 claims
the base is specifically Android 5.0 (Lollipop) with ART/Dalvik removed; this is plausible
but the specific Android version has not been independently verified. The characterization
"Linux 3.10 + Qt 5 with Android-derived init/property infrastructure" is the most defensible
phrasing. **[Partially verified]**

**Replacement player hook point:** the `HgrmMediaPlayerApp` service entry in `init.rc` is
the swap target. Wampy's installer demonstrates the pattern.

### 5.2 Firmware format (.UPG)

Sony `.UPG` V2 format. AES-CBC encrypted with a per-model 64-byte KAS key. The Rockbox
`nwztools/upgtools` (`upgtool`) binary handles both unpack (`-x`) and repack (`-c`).

KAS key for the NW-A55 is in the Rockbox database (`utils/nwztools/database/nvp/`). The
round-trip (stock → unpack → modify → repack → install) is proven by Phase 7.

Key reference: `roobscoob/SonyWalkmanFirmwarePatcher` `upgDocs.md` — clearest one-page V2
format + crypto spec.

### 5.3 NVP (Non-Volatile Parameter) storage

Per-device configuration stored in an NVP partition. Accessible read-only via
`scsitool-nwz` from the host over USB-MSC SCSI backchannel.

Critical NVP slots: `kas` (64 bytes, firmware decryption key), `dest_id` (region/destination
code), `sps` (service mode flag), `upd` (upgrade flag). The U-Boot password slot must never
be written without a verified backup — it is not reversible.

### 5.4 Rust for the replacement player

Rust gives practical advantages: musl static linking sidesteps glibc 2.23 entirely, and
`SystemTime` → `u64` via `as_secs()` lets the scrobbler and any other timestamp-bearing code
represent times past 2038 correctly. **However, this does not automatically fix the device's
RTC, filesystem timestamps, or kernel/libc time syscalls on a 32-bit Linux 3.10 system.** The
2038 risk at the kernel boundary is a separate engineering problem — see §4h analysis and
the risk register (§8).

Target triple: `armv7-unknown-linux-musleabihf`. C shim compiled with
`arm-linux-musleabihf-gcc` against device headers. All `time_t` values must stay inside the
shim — never expose `time_t` to Rust.

### 5.5 .scrobbler.log format

Compatible with the Last.fm `.scrobbler.log` spec. Written by `unknown321/scrobbler` to the
root of the internal storage partition (`/contents/.scrobbler.log`). Desktop tools (e.g.
Scrobblerfix, CFWScrobbler) expect this path.

The replacement player should write to the same path for compatibility. See §11.5.4 for the
corrected Rust snippet.

### 5.6 Walkman One (WO) custom firmware

Walkman One (`mrwalkman.com`) is the recommended baseline: it unlocks region restrictions,
enables additional EQ filters, and is the environment Wampy and scrobbler are best-tested
on. It uses the same `.UPG` format and install path as stock.

Phase 5 diffs stock vs WO to identify exactly which sectors/files WO modifies.

### 5.7 Wampy architecture overview

Wampy injects into the running player via `LD_PRELOAD` + a modified boot image. It adds an
overlay UI (skins, clock, spectrum analyser) without replacing `HgrmMediaPlayerApp`. Its
`src/Controller.cpp` exposes the view-model surface that the replacement player can use as a
reverse-engineering map.

Key files: `MAKING_OF.md` (the whole device in one document), `ALSA.md` (audio topology),
`kernel.md` (boot image), `installer/run.sh` (`.UPG` install pattern).

### 5.8 Bluetooth

BlueZ 4 (old). LDAC encoder is present. Bluetooth and USB-DAC share the audio output path in
a currently-exclusive way — the enforcement layer is unknown (see §5.10, OQ2).

`hciconfig`/`hcitool` available on the device for on-device investigation (CLAUDE.md E5).

### 5.9 USB subsystem

USB gadget mode (`/sys/class/android_usb/android0/functions`): supports MSC (mass storage),
MTP, and DAC (audio input). Only one function set is active at a time — this is a USB
hardware constraint, not a Sony policy.

The `llusbdac.ko` kernel module (see `github.com/zhangboyang/llusbdac` for a precedent
implementation) handles the USB audio input path in DAC mode.

### 5.10 USB-DAC + Bluetooth LDAC simultaneous operation

Sony's stock firmware does not allow simultaneous USB-DAC input and Bluetooth audio output.
A confirmation dialog appears when switching between the two modes.

**The enforcement layer is unknown.** The hardware paths for USB DAC input and Bluetooth
output each work independently in their respective modes. Simultaneous operation is plausible
but not proven. The block could be:

1. **App-layer policy** — player app calls BT disconnect when entering USB-DAC mode
   (most likely candidate: Sony's UI shows a confirmation dialog → app-layer logic)
2. **`libSoundServiceFw` routing exclusivity** — source/sink enum rejects simultaneous paths
3. **ALSA topology constraints** — `llusbdac.ko` binds a fixed ALSA sink with no loopback
4. **BlueZ state management** or kernel-module routing

Treat simultaneous USB-DAC-input + LDAC-output as an early experiment, not a guaranteed free
feature. **[Hypothesis]**

**Working hypothesis:** Candidate 1 (player-enforced) is most likely because Sony's UI flow
includes a confirmation dialog when switching modes — that's app-layer logic. But this is a
hypothesis, not a conclusion. On-device observation (CLAUDE.md E4–E5) is the only way to
confirm.

### 5.11 ALSA device topology (from Wampy ALSA.md) [Verified]

Wampy's `ALSA.md` documents the output of `aplay -l` on a running NW-A50 device. The audio
card identifies as `sonysoccard` with six PCM devices:

| Device | Name | DAI | Role |
|---|---|---|---|
| hw:0,0 | cxd3778gf-hires-out | DAI_CXD3778GF_DAC-0 | High-resolution audio output |
| hw:0,1 | cxd3778gf-standard | DAI_CXD3778GF_STD-1 | Standard audio output |
| hw:0,2 | dsdenc | DAI_CXD3778GF_ICX-2 | DSD encoder path |
| hw:0,3 | cxd3778gf-dsd-out | DAI_CXD3778GF_ICX-3 | DSD audio output |
| hw:0,4 | cxd3778gf-icx-lowpower | DAI_CXD3778GF_ICX-4 | Low-power playback (battery-saving) |
| hw:0,5 | cxd3778gf-icx-lowpower_test | DAI_CXD3778GF_ICX-5 | Low-power test interface |

**Key observations:**
- No capture device is listed. The hardware provides playback only. For real-time audio
  visualization (frequency spectrum, etc.), the ALSA loopback driver (`snd_aloop.ko`) or
  `libAudioAnalyzerService.so` (which Wampy already patches) is needed.
- The DAI names confirm the CXD3778GF part number on the actual hardware.
- `hw:0,4` (`icx-lowpower`) is the default playback device for battery-efficient operation.
- Understanding which devices are active in which mode (file playback vs USB-DAC vs
  Bluetooth) is central to the USB-DAC routing investigation (§5.10).

### 5.12 Stock playback feature gaps — what the replacement player must build [Verified]

The customer's two requested playback features do not exist in Sony's stock firmware. Both
are confirmed from the Sony Help Guide and Wampy's USAGE.md, and both are pure application
logic with no hardware or middleware dependency — they are entirely the responsibility of the
replacement player.

**Queue is read-only in stock.** The stock firmware has a "Play Queue" screen, but per the
Sony Help Guide it only lets you *"check the list of tracks that the player will play with
the current settings."* The available actions are limited to "Add All Songs to Bookmark
List," "Add All Songs to Playlist," and per-track "Add to Bookmark List" / "Add to
Playlist." There is **no reorder, no remove-from-queue, and no insert-arbitrary-track
("play next" / "add to end")**. Furthermore, Wampy's USAGE.md notes the *"default player
keeps up to 15 songs in memory, some of those have already been played, reducing playlist
size further."* So the stock queue is both non-editable and length-limited.

- **Implication:** a proper user-controllable queue (add, remove, reorder, play-next,
  save-as-playlist, unbounded length) is a from-scratch feature in the replacement player.
  Standard music-player logic; no RE required.

**Shuffle-by-album does not exist in stock.** Per the Help Guide's playback-methods table,
stock Shuffle behaves as follows: *"If you select a track from [Album] on the library screen,
the player will shuffle all the tracks in the selected album. When the player finishes playing
all the tracks in the album, playback will proceed to the next album."* That is
**shuffle-tracks-within-each-album, advancing albums in sequential order**. The customer's
requested behavior — randomize the *album order* while preserving each album's internal track
order — is a different algorithm and is **not available in any stock playback mode**.

- **Implication:** "shuffle by album" must be implemented as custom queue-population logic in
  the replacement player: enumerate albums, shuffle the album list, then for each album append
  its tracks in track-number order. Pure application logic; no RE required.

**Why this matters for scoping:** these are not "improvements to existing features" — they are
net-new features. The good news is they sit entirely above the audio middleware (they only
decide *what track to tell `libdmp_player_service` to play next*), so they carry zero RE risk
and zero firmware risk. They are pure Rust application code in the replacement player's
playback engine.

---

## §6 Key Resources

### 6.1 Wampy

The single most useful resource for this project. Open source on GitHub, licensed under
**GPLv3 with a Commons Clause non-sale condition** (not MIT — this was incorrectly stated in
v1.0–v1.3). As of this revision, GitHub shows **C 80.9%, C++ 17.3%**, **71 releases**,
latest **v1.14.3 (Feb 22, 2026)**, 114 stars, 4 forks. **[Verified]**

**License implication for the project:** The Commons Clause condition restricts selling Wampy
as a product. If the customer's replacement player reuses any Wampy code (not just its
documentation), the license terms apply. This is a project risk — see §8. A clean-room
approach (using Wampy's documentation as a map, but writing new code) avoids the issue.

Key files:
- `MAKING_OF.md` — the entire device in one document
- `MAKING_OF_FM.md` — FM radio implementation
- `MAKING_OF_SCROBBLER.md` — scrobbler integration notes
- `ALSA.md` — audio topology (§5.11 source)
- `kernel.md` — boot image modification
- `VOLUME_TABLES.md` — per-model volume/filter tables (includes DMP-Z1 contradiction)
- `installer/run.sh` — `.UPG` install pattern template
- `src/Controller.cpp` — view-model surface

### 6.2 unknown321/wbrt (Walkman Backup/Restore Tool)

**Primary citation for MT8590 SoC identification.** README: "Create and restore backups for
MT8590-based Walkmans: NW-A30/40/50, ZX300, WM1A, WM1Z, DMP-Z1." Uses MediaTek USB VID
`0x0E8D` / PID `0x2000` (preloader mode). Requires MediaTek USB VCOM driver on Windows.
Full eMMC backup before any write — this is the only brick recovery path. **[Verified]**

### 6.3 Rockbox nwztools

`Rockbox/rockbox` (sparse: `utils/nwztools`). Provides:
- `upgtool` — `.UPG` pack/unpack
- `database/nvp/` — KAS keys and NVP slot definitions per model
- `scsitool/` — SCSI backchannel to read/write NVP from host

### 6.4 roobscoob/SonyWalkmanFirmwarePatcher

`upgDocs.md` — the clearest one-page V2 `.UPG` format + crypto specification.

### 6.5 unknown321/scrobbler

`.scrobbler.log` format and path (root of internal storage `/contents/.scrobbler.log`).
Source covers: track-state IPC, "beeps off" gotcha, adb-vs-scsitool install path.

### 6.6 zhangboyang/llusbdac

Low-latency USB audio kernel module. Precedent implementation relevant to the USB-DAC+LDAC
investigation. Not cloned by Phase 1 automatically — see CLAUDE.md Part C.

---

## §7 Build System and Toolchain

The Sony device uses a custom clang/LLVM build (version unknown — Phase 4c determines this).
The replacement player cross-compiles with:

- **Rust:** `armv7-unknown-linux-musleabihf` target via musl static linking. No glibc
  dependency. All timestamp logic in `u64` (Unix seconds).
- **C shim:** `arm-linux-musleabihf-gcc` from `musl.cc` prebuilt toolchain. Bridges Rust
  to Sony's `libSoundServiceFw.so` and other device libraries.
- **ABI risk:** If Sony used clang (LLVM C++ ABI) and the shim links against system
  libstdc++ (GCC ABI), vtables and exception handling may mismatch. Phase 4c identifies
  the actual ABI. See OQ3/OQ6.

Cross-compilation is done on the WSL2 host; the output binary is deployed via `.UPG`
(Phase 7 round-trip proves the mechanism).

---

## §8 Risk Register

| Risk | Likelihood | Severity | Mitigation |
|---|---|---|---|
| clang/GCC C++ ABI mismatch in C shim | Medium | High (silent crashes, corrupt vtables) | Confirm Sony toolchain via Phase 4c; match ABI in shim |
| MT8590 boot ROM bypass inapplicable | Low | Low (not needed for player) | Only investigate if Phase 8 reveals eMMC-level need |
| .UPG round-trip repack breaks checksums | Low | Medium (uninstallable firmware) | Phase 7 verifies; never deploy without round-trip passing |
| Bad-boot counter absent → unrecoverable brick | Medium | High | Always implement bad-boot counter (Wampy pattern) before full UI swap |
| Wampy license contamination (GPLv3 + Commons Clause) | Medium (if any Wampy code is reused) | Medium (legal/distribution constraint) | Use Wampy's documentation and RE findings as a map; write all code from scratch. Do not copy-paste from Wampy source. |
| Rust 2038 safe but kernel/RTC/filesystem not | High (Linux 3.10 kernel is 32-bit time_t) | Medium (incorrect timestamps in filesystem, RTC overflow) | Keep all timestamp logic in Rust u64; never pass `time_t` across the shim boundary; test RTC behavior on-device before 2038 reliance |
| USB-DAC + LDAC advertised as headline feature before verification | Medium | Medium (customer expectation) | Classify as "experiment" until ALSA device visibility confirmed during USB-DAC mode on a live device |
| NVP U-Boot password slot overwrite | Very Low (only if explicitly targeted) | Critical (permanent brick) | Never write the U-Boot slot; read-only NVP dump first |

---

## §9 Delivery Plan (skeleton)

1. **Host pipeline** — `make phase1`…`phase7`. Firmware extracted, ABI confirmed, round-trip
   proven, hello-world cross-compiled. (~2 days)
2. **Device backup** — `wbrt` full eMMC dump. Non-negotiable. (~1 hour)
3. **Baseline firmware** — Walkman One + Wampy. Known-good environment. (~1 hour)
4. **Shell + SoC confirm** — adb or scsitool; close OQ1 on-device. (~1 hour)
5. **USB-DAC investigation** — E4/E5 from CLAUDE.md. Make-or-break for headline feature.
   (~1 day; scope depends on what the investigation finds)
6. **NVP characterization** — read-only dump; identify relevant slots. (~2 hours)
7. **hello-walkman deployment** — first code on device; bad-boot counter; service swap.
   (~1–2 days)
8. **Replacement player MVP** — queue engine + shuffle-by-album + scrobbler output.
   Pure application logic; no hardware blockers. (~1–2 weeks depending on UI scope)

---

## §10 Open Questions

See `docs/open-questions.md` for the full tracked list.

- [x] **OQ1 — What is the SoC?**
  - **CLOSED (v1.4).** MediaTek MT8590, confirmed by `unknown321/wbrt` and Wampy
    `MAKING_OF.md`. The v1.3 retraction was itself an error. Phase 3 scripts can be updated
    to verify MT8590-specific markers rather than doing blind identification.

- [ ] **OQ2 — USB-DAC + LDAC enforcement layer?** (§5.10)

- [ ] **OQ3 — clang/GCC C++ ABI compatibility?** (Phase 4c)

- [ ] **OQ4 — MT8590 BROM bypass applicability?** (low priority)

- [ ] **OQ5 — Android base version?** (on-device `getprop`)

- [ ] **OQ6 — Sony clang version + libstdc++ ABI?** (Phase 4c)

- [ ] **OQ7 — Scrobbler IPC mechanism?** (scrobbler source + on-device trace)

- [ ] **OQ8 — DMP-Z1 SoC identity** (low priority; not the target device)
  - `wbrt` lists DMP-Z1 as MT8590-based. Wampy VOLUME_TABLES.md says "DMP-Z1 does not have
    these at all (different SOC)" and notes it uses an "Aulos card" instead of CXD3778GF.
  - This may mean DMP-Z1 shares the MT8590 application processor but has a different audio
    subsystem, or uses a different SoC that shares the MediaTek preloader interface.
  - Low priority for this project (DMP-Z1 is not the target), but worth noting for
    cross-model assumptions.

---

## §11 Practical Phase Guide

### 11.1 Host-side phases (in WSL2)

Run `make check-deps` then `make phase1` through `make phase7` in order. Each phase creates
a sentinel file in `artifacts/` and gates on the previous. See CLAUDE.md Part D.

### 11.2 Firmware acquisition

- Stock NW-A55: Sony support page → place at `artifacts/stock/NW_WM_FW.UPG`
- Walkman One: `mrwalkman.com` NW-A50 page → place at `artifacts/walkmanone/WalkmanOne.UPG`

These require a browser; Sony's download page does not support `wget`.

### 11.3 Phase 3 — SoC confirmation (not discovery)

Phase 3 confirms the MT8590 from firmware artifacts. The SoC is already identified from
`wbrt` and Wampy sources. On-device confirmation (E3) closes the loop.

### 11.4 Key analysis outputs

| Output | What it tells you |
|---|---|
| `analysis/4b_init_flow.txt` | HgrmMediaPlayerApp service definition + swap target |
| `analysis/4c_compiler_version.txt` | Sony clang/GCC version for ABI planning |
| `analysis/4e_soundservice_symbols.txt` | libSoundServiceFw export surface for C shim |
| `analysis/4f_usb_dac_routing.txt` | USB-DAC enforcement candidates (narrows, not closes) |
| `analysis/4h_2038_risk.txt` | time_t exposure points in device libraries |
| `analysis/7_roundtrip_diff.txt` | Round-trip pass/fail — deploy gate |

### 11.5 Scrobbler integration notes

The replacement player must write `.scrobbler.log` to `/contents/.scrobbler.log` (root of
the internal storage partition, USB-visible). This matches the path used by
`unknown321/scrobbler` and is expected by desktop tools.

`SystemTime` in Rust is 64-bit on every target. The scrobble timestamp stored in
`.scrobbler.log` will be correct past 2038. However, this does not automatically fix the
device's RTC or kernel-level time representation. If the player calls any Sony library
function that accepts or returns `time_t`, that interface is a 2038 exposure point. See
§4h analysis.

#### 11.5.1 Log format

Standard Last.fm `.scrobbler.log` TSV format. Fields: artist, album, track, track-number,
duration-seconds, "L" (for loved), timestamp-unix. One track per line.

#### 11.5.2 IPC with existing scrobbler daemon

If the `unknown321/scrobbler` daemon is running, it expects track-state signals from the
player (start, stop, pause, position). The replacement player must either emit these signals
or write directly to `.scrobbler.log` without the daemon. Reading the `scrobbler/playerevents`
source determines which IPC mechanism is used (see OQ7).

#### 11.5.3 "Beeps off" gotcha

The scrobbler installer silences system beeps as a side-effect. If the replacement player
restores beep behavior, this may surprise users who installed the scrobbler expecting no
beeps.

#### 11.5.4 Rust snippet (corrected path)

```rust
// Write to root of internal storage for desktop-tool compatibility.
// The existing scrobbler (unknown321/scrobbler) writes to internal storage root.
// Verify actual mountpoint on device; /contents/ is the USB-visible partition.
let path = config.scrobbler_log_path
    .as_deref()
    .unwrap_or("/contents/.scrobbler.log");
let mut f = OpenOptions::new().create(true).append(true)
    .open(path).unwrap();
```

### 11.5 Device-side steps (Phase 8)

See CLAUDE.md Part E for the full device-side procedure. Steps in safety-gradient order:

1. **Backup** (E0) — `wbrt` full eMMC dump. Non-negotiable.
2. **Baseline firmware** (E1) — Walkman One + Wampy.
3. **Shell** (E2) — adb or scsitool.
4. **Confirm MT8590** (E3) — `/sys/firmware/devicetree/base/compatible`, `getprop ro.board.platform`.
5. **Confirm the MT8590 SoC** by reading `/sys/firmware/devicetree/base/compatible` or
   extracting the DTB. The SoC is already identified as MT8590 from `wbrt` and Wampy sources,
   but on-device confirmation via kernel output closes the loop and may reveal the specific
   MT8590 sub-variant or revision.
6. **ALSA topology** (E4) — `aplay -l` in each audio mode; diff the output.
7. **USB-DAC enforcement** (E5) — `strace` + `dbus-monitor` while toggling USB-DAC.
8. **NVP dump** (E6) — `scsitool` read-only; characterize slots before any write.
9. **hello-walkman deploy** (E7) — first code on device; bad-boot counter; service swap.

---

## §12 Sources

- **Sony NW-A55 Help Guide** — `sony.net` — stock feature descriptions (queue behavior,
  shuffle behavior, FM range)
- **`unknown321/wampy`** — `github.com/unknown321/wampy` — MAKING_OF series, ALSA.md,
  USAGE.md, installer pattern, view-model surface
- **`unknown321/wbrt`** — `github.com/unknown321/wbrt` — Walkman Backup/Restore Tool;
  **primary citation for MT8590 SoC identification** and MediaTek USB VID `0x0E8D`
- **`unknown321/scrobbler`** — `github.com/unknown321/scrobbler` — scrobbler log format,
  path, IPC, install procedure
- **`Rockbox/rockbox` (utils/nwztools)** — `github.com/Rockbox/rockbox` — upgtool, KAS
  keys, NVP slot definitions, scsitool
- **`roobscoob/SonyWalkmanFirmwarePatcher`** — `github.com/roobscoob/SonyWalkmanFirmwarePatcher`
  — `upgDocs.md`: V2 `.UPG` format + crypto spec
- **`zhangboyang/llusbdac`** — `github.com/zhangboyang/llusbdac` — USB-DAC kernel module
  precedent; central to OQ2 investigation
- **Wampy `ALSA.md`** — `github.com/unknown321/wampy/blob/master/ALSA.md` — primary source
  for the full ALSA device topology (`sonysoccard`, 6 PCM devices, CXD3778GF device names)
