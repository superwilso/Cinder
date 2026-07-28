# Cinder — production-readiness gap list

Assessed **2026-07-27** against `afdd930`. Companion to
[`../cinder-home/STATUS.md`](../cinder-home/STATUS.md) (current state) and
[`../cinder-home/ROADMAP.md`](../cinder-home/ROADMAP.md) (forward plan). This file answers one
question only: **what stands between the tree as it is and a device the owner can rely on with no
PC in the room.**

"Production ready" is taken to mean: it boots, it plays, it survives a week of pocket use, nothing
it draws is a lie, and every failure mode has an escape that needs no cable.

---

## Where it stands

| Gate | State |
|---|---|
| Host tests | **133 passing** (`cargo test --workspace`) |
| Launcher recovery matrix | **24/24** (`cinder-home/tools/test_launcher.sh`) |
| qemu preflight | PASS (both channels) |
| GLIBC floor | ≤ 2.23 |
| Channels packed | `dist/dev`, `dist/stable` |
| Audio on real hardware | **verified 2026-07-27** — position advancing 1000 ms/s, `ALSA pcm4p` RUNNING, `hw:0,4` = `cxd3778gf-icx-lowpower` (the low-power S-Master DAC, so 3.5 mm is already the battery-efficient route) |

**And the headline number: 25 commits have landed since the last time any of this ran on the
device** (`eb07f7f`, the Framework-pump fix, was the last hardware-verified commit).

---

## Blocking

### B1 — Verification debt: one device session

Everything after `eb07f7f` is untested on hardware, and several of those commits touch the **boot
path**: brightness applied at boot, the idle screen-off timer, the render-loop rate change, the
analyzer's demand-start, auto-MSC gating, and the dark-panel paint skip. A wedge in any of them
ends in a bad-boot revert with 25 commits as the bisect surface.

The existing safety gradient is the mitigation and it should be followed exactly:
`cinder-probe` has no easel lifecycle, so it **cannot** affect boot — run it, and read its output,
before repointing the `.appcfg`. Boot with the cable **out** (a cable at boot is itself rung 0 of
the escape ladder).

Individual unverified assumptions, each roughly one boot to settle:

| Assumption | Where | If wrong |
|---|---|---|
| `media_origin_t::Begin == 0` | `cinder_audio_seek_ms` | drag-to-seek seeks from the wrong origin |
| Sony's `AudioAnalyzerService` emits frames — `cinder-probe --analyzer` has **never been run**. *(2026-07-28: the likely cause was found and fixed — Cinder never called `SetPassband`, and the service reports nothing until it is told which bands to analyse. See the Wampy comparison in STATUS.md.)* | `viz_analyzer_tick` | the visualiser never draws at all; the synthetic fallback was deliberately removed |
| `duration_raw` is milliseconds (diagnostic shipped in `1ccb7bc`) | `onPlayTimeUpdated` | progress-bar scale is wrong |
| Idle screen-off wakes reliably | `screen_auto_wake` | a failed wake is indistinguishable from a dead device |
| `amixer` master reaches the hardware audibly | `volume_write_now` | Vol± stays HUD-only |
| Backlight write at boot picks the right node | `apply_brightness` | screen at the wrong level, or black |

### B2 — Goal #3 has never been executed

USB-DAC → LDAC. The RE is complete, `ldac-bridge` builds, `ldac-bridge/TEST.md` is written, and it
is **0% run**. It executes under **stock** firmware, so it needs neither Cinder installed nor any
boot risk — it is the cheapest high-value thing on this list and it is the stated reason the
project exists.

### B3 — Controls that draw as real and do nothing

The 2026-07-27 dead-UI audit removed everything that showed *fabricated state*. What remains is the
inert set — drawn, tappable, no effect:

| Control | Cost to finish |
|---|---|
| **Now Playing shuffle / repeat** | low — `NodeTrackSequence::SetOneTrackMode` is exported and Cinder already pre-shuffles its own queues. **This is the one a user hits in the first hour.** |
| Bluetooth radio toggle | needs the `BtTransmitterService` C++ shim (see B4) |
| Bluetooth "Pair new device" | same |
| `Screen::Fm`, `Screen::Receiver` (no `tap()` branch at all) | Sony tuner/BT services not RE'd |
| Settings ▸ Database | triggers Sony's MTP re-indexer — complex |
| EQ footer "Save Sound Preset" | nothing to do: the EQ already persists on every change. Reword or give it a named-preset store. |
| `pairing.rs` — a complete screen with **no `Screen::Pairing`** | unreachable except from the preview harness |

Production decision required **per control: wire it or delete it.** A tappable control that does
nothing is the same category of untruth as the hardcoded clock that was just removed — it teaches
the user the app is unreliable.

### B4 — Bluetooth is entirely unwired

No BT client exists. The radio cannot be powered, nothing can be paired, and the codec selector —
which *is* functional as a stored preference — never reaches `BtTransmitterService`. Goal #7
(*keep all audio effects and try to apply them to Bluetooth audio*) cannot begin until this shim
exists. It is the same ABI boundary as `ldac-bridge`, so B2 and B4 share most of their groundwork.

**Latent, and it will bite the moment BT works: Cinder's volume keys cannot change Bluetooth
volume.** They write `amixer -c0 'master volume'`, which is a CXD3778GF codec register — and the BT
transmit path never touches that codec. It is decode → *we* write raw PCM into an AF_UNIX socket
(`BtTransmitterService::GetSocketName`) → the MTK Bluetooth chip. No codec register is in that path,
so today's volume control would silently do nothing on BT.

Sony solves this with a route-aware layer Cinder has no equivalent of:
`pst::services::volume::VolumeService::SetVolume(unsigned)` plus a `VolumeCondition` (the route) and
an `AvlsCondition` (the regional loudness cap). One volume goes in; the service decides whether that
means a DAC gain register or `BtTransmitterService::SetCurrentVolume(uint8_t)` — AVRCP Absolute
Volume, 7-bit, so 0..127.

So when BT lands, `apply_volume()` has to branch on the output route. Two things make that easier
than it sounds, and one harder:
- **The BT ceiling is 128 steps**, slightly *finer* than the 0..120 the wired path uses.
- **Cinder is the PCM producer** for the BT pipe, so a digital pre-scale before the socket write
  gives step granularity finer than any protocol limit — half-steps included — at the cost of bit
  depth, which is negligible for a trim of a fraction of a dB and is not for a full digital volume.
- **Harder:** if the headphones report no absolute-volume support
  (`IsSupportedAbsoluteVolume()`), Sony falls back to injecting AVRCP VOLUME_UP/DOWN key events
  through `/dev/uinput`, and the step size is then entirely the headphones' choice. In that mode the
  pre-scale is the only lever there is.

---

## Not blocking a flash, but required before calling it done

### N1 — Nothing has been soaked

Cinder has never run for hours. Unmeasured: memory growth over a long session, log growth within a
single long-lived boot (the launcher rotates one generation per boot, so growth *within* a boot is
unbounded), and the first-build cost of the art cache across a 304-album library. The art cache
itself is bounded by construction — 34.5 KB/album on ext4 `/data`, ≈10 MB for this library — so it
is not a risk, just unmeasured.

### N2 — Goal #1 is unmeasured

*"Faster boot time and better battery life"* is the founding claim of the project and there is no
number for either, stock vs Cinder. Both are straightforward to measure on the next device session
and both are load-bearing for whether the replacement was worth doing.

### N3 — No panic hook

The workspace sets `panic = "abort"`, so a Rust panic kills the process, appmgr calls
`android_reboot`, and the device reverts to stock. The panic message *does* reach
`/contents/cinderhome.log` through the launcher's stderr redirect, so this is not silent — but
there is no hook recording the current screen and UI state alongside it. Cheap insurance for a
failure mode whose only user-visible symptom is "it rebooted".

### N4 — Goal #10 (2038) is deliberately partial

Satisfied in userland: the musl components carry 64-bit `time_t` and Cinder's own timestamps are
`i64`. `cinder-home` is *forced* to 32-bit `time_t` because it must link Sony's glibc-2.23 C++
libraries. A true device-wide fix needs kernel + glibc + RTC and is a separate project. **This is
a documented design decision, not a gap** — it is listed here only so it does not read as one.

---

## Suggested order

1. **LDAC test under stock** (B2) — independent of everything else, zero boot risk, highest value.
2. **`cinder-probe`** — `--analyzer`, `--pump`, and a `--discover` PlayStatus dump *with music
   actually playing* (the 07-25 dump was all zeros because nothing was).
3. **Flash `dist/dev`, cable out** — then soak a full day of real use (B1, N1, N2).
4. **Wire shuffle/repeat; resolve the rest of the inert set either way** (B3).
5. **Measure boot time and battery against stock** (N2).
6. **Flash `dist/stable`** for daily use.

Steps 1–2 need the device but not a flash. Step 4 is the only item on this list that can be done
entirely offline.
