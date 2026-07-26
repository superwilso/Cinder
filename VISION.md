# Cinder — Project Vision

**What this is:** a from-scratch replacement music player for the Sony NW-A55/A50 Walkman, running
as the device's real Home app in place of Sony's Qt `HgrmMediaPlayerApp` — while keeping every one
of Sony's audio services (DSP, codecs, LDAC) intact.

**Why it exists:** stock firmware is heavy, slow to boot, and blocks combinations the hardware is
perfectly capable of — most notably USB-DAC input and LDAC output at the same time. Cinder keeps
Sony's audio engine and replaces only the UI layer that gets in the way.

**What it is not:** a reimplementation of the audio stack. The Hagoromo audio services
(`SoundServiceFw`, `PlayerService`, `BtTransmitterService`, `EffectCtrlDmp`, …) are separate
processes and Cinder drives them over their existing binder IPC. That's what keeps EQ, DSEE HX,
VPT, Vinyl and every other Sony effect working.

> Companion docs: [`cinder-home/STATUS.md`](cinder-home/STATUS.md) = the current feature matrix.
> [`cinder-home/ROADMAP.md`](cinder-home/ROADMAP.md) = the near-term prioritized backlog.
> This file = the *why* and the long arc. Where they disagree, STATUS.md wins on current state.

---

## The ten goals

User-authored, living list. Numbered as originally given; append, don't prune.

| # | Goal | State |
|---|------|-------|
| 1 | **Faster boot + better battery** — kill the heavy Qt app, lean native UI | ✅ Largely achieved. Cinder is a ~3 MB native binary with dirty-flag rendering vs Sony's Qt stack. GPU present path landed 2026-07-26 (vsync-paced, no triple memcpy). Not yet *measured* against stock — quantifying this is outstanding. |
| 2 | **Improved UI/UX** — the Cinder design (warm amber on near-black) | ✅ All 14+ screens built and rendering. Type scale and touch-target pass done 2026-07-26 to better match Sony's larger, more legible sizing. |
| 3 | **USB-DAC in with LDAC *and* 3.5 mm out** — the headline | ◐ **The next major push.** All RE complete; `ldac-bridge` builds. Two on-device unknowns remain. See below. |
| 4 | **Night mode** — dark palette *and* dimmer backlight | ✅ Both. Day/Night palettes plus real backlight dimming (auto-detected node). |
| 5 | **Battery-efficient scrobbler** — native, not the heavy add-on | ✅ Writes `/contents/.scrobbler.log`. |
| 6 | **Queue and shelf** — both genuinely absent from stock | ✅ Shelf (pin/jump-back) and Up Next both wired; swipe-to-queue works. |
| 7 | **Keep all audio effects, and try to apply them to Bluetooth** | ◐ All Sony DSP is wired and working (EQ, DSEE HX, VPT, DC Phase, Vinyl, Normalizer, ClearAudio+). Applying DSP to the LDAC transmit path is a stretch goal that needs RE — stock may bypass DSP on transmit. |
| 8 | **Keep using the built-in sound card** — no extra software mixing | ✅ By construction: playback routes through `cxd3778gf` (card0) exactly as stock does. |
| 9 | **Lock screen: touch off, physical buttons live** | ✅ True keylock — Hold switch is the only unlocker; Power just toggles the backlight. |
| 10 | **Fix the 32-bit time / 2038 problem** | ◐ Partial *by necessity*. See the engineering tension below. |

### The 2038 tension (goal #10)

Worth stating plainly because it constrains the toolchain:

- The device is glibc **2.23** on a 32-bit 3.10 kernel → **32-bit `time_t` device-wide**.
- `cinder-home` must link Sony's glibc-2.23 C++ libraries, so it is **forced** to 32-bit `time_t`
  for ABI compatibility. The 64-bit-time symbols simply don't exist in glibc 2.23.
- Genuine Y2038-safety therefore lives in the **musl** components (musl 1.2+ has 64-bit `time_t`
  on all architectures) and in keeping Cinder's own timestamps as `i64` internally, so Cinder's
  own data never wraps.
- A true device-wide fix needs kernel + glibc + RTC all on 64-bit time — a kernel rebuild, and a
  separate project. **Do not let #10 push us to upgrade the device glibc**: that is the single
  highest brick-risk change available.

Practical reading: satisfy #10 in userland, document that the underlying clock stays 32-bit.

---

## Architecture in one page

```
        ┌──────────────────────────────────────────────┐
        │  appmgr (hagoromo2)  — launches the Home app │
        └───────────────────┬──────────────────────────┘
                            │ execs, then waits for the easel
                            │ Foreground handshake (or reboots)
                            ▼
        ┌──────────────────────────────────────────────┐
        │  cinder-home   (C++, glibc + libc++)         │
        │  easel::ApplicationBase + CuiAppModule       │
        │  ├── lifecycle handshake, watchdogs, guards  │
        │  └── drives Sony services over binder IPC ───┼──▶ PlayerService, SoundServiceFw,
        │                                              │    EffectCtrlDmp, BtTransmitterService…
        │      cinder-ffi  (Rust, cdylib-in-binary)    │
        │      ├── render tick, input, scrobbler, DB   │
        │      └── present: GPU (EGL/GLES2) │ software │──▶ /dev/graphics/fb0 or Mali
        │            cinder-ui (pure render, no I/O)   │
        └──────────────────────────────────────────────┘
```

**Toolchain split, and why:** the UI/logic is Rust; the easel/Sony-IPC shim is C++ built against
the device's own libraries. They're separate because Sony's C++ ABI (libc++, glibc 2.23) can't be
mixed with the lean musl world. `ldac-bridge` is a third, separate armhf-glibc binary for the same
reason.

**Safety model, and why it matters:** appmgr *reboots the device* if the Home app fails its
foreground handshake — an early `SIGKILL` experiment caused a genuine boot loop. So everything is
defensive: `run_guarded` around every Sony IPC call, per-frame watchdogs, a bad-boot counter that
auto-reverts to stock, and a probe binary (`cinder-probe`) that exercises risky code paths with no
easel lifecycle at all.

---

## The road ahead

### Now — a development loop that doesn't cost the developer a recovery every cycle
Two rounds of this, both on 2026-07-26. First: the bad-boot counter latched permanently after two
impatient reboots, sending every subsequent boot to stock. The counter now clears ~8 s after the
first painted frame (not after the ~100 s feature-init chain), `MAXBAD` is 4, and the latch
self-heals when a newer binary is installed. Screenshot capture landed so the UI can be inspected
remotely rather than described.

Then the device bricked outright — a logo boot-loop that needed a wbrt restore — and the cause was
the safety net itself. **Every escape depended on `/contents`, and `/contents` was what had
failed.** It is vfat (no journal) *and* is the partition handed to the PC for USB-MSC, so repeated
mode switching both corrupts it and takes it away. When it stopped mounting, the counter could not
advance and the launcher's log redirect made `sh` exit before `exec`, so appmgr rebooted forever
with the net silently disabled.

The lesson generalised into a rule: **an escape must depend on less than the thing it rescues.**
The ladder is now explicitly ordered by dependency — a USB cable at boot (needs nothing at all),
then the counter on `/data`/ext4, then the USB-MSC flag files, then the uninstaller, then wbrt.
The launcher refuses to run when it cannot persist its own counter, and 18 sandboxed scenarios in
`cinder-home/tools/test_launcher.sh` gate every build. That harness immediately caught a second
instance of the original bug: `:` is a POSIX special builtin, so a failed redirect on it *exits*
the shell rather than returning non-zero.

### Next — goal #3, USB-DAC → LDAC
The feature the project was started for. Research is *done*:
- LDAC transmit is **non-ALSA** — PCM goes over an abstract AF_UNIX socket to
  `BtTransmitterService`, which encodes and hands off to the MTK BT chip.
- Control-plane vtable indices are extracted: `SetCurrentSource=12`, `SetLdacSoundQuality=18`,
  `SetLdac=20`, `GetSocketName=29`.
- Stock's block is **app policy only** — a UI overlay plus an explicit `RequestDisconnection()`.
  Cinder's fix is simply not to reproduce it.
- There is **no audio mutex**: stock already runs USB capture and DAC playback concurrently.

Two unknowns need hardware, both with a diagnostic table in `ldac-bridge/TEST.md`:
1. Does `SetCurrentSource(true)` actually make the server open its socket?
2. Capture contention — stock owns the UAC capture card, so `snd_pcm_open` may return `-EBUSY`.

### Then — finish the device-gated tail
Volume calibration, real keycodes, seek-accurate progress, playlist schema, BT radio toggle, live
codec apply, FM/receiver/pairing screens.

### Later — measure goal #1
Boot time and battery life vs stock have never been quantified. Worth doing once the loop is quick.

---

## Principles

- **Never brick.** Every change keeps a path back to stock. The bad-boot counter is the net;
  `.appcfg.real` is the parachute; `wbrt` is the last resort.
- **An escape must depend on less than the thing it rescues.** Learned the expensive way on
  2026-07-26: a safety net stored on the partition that fails is not a safety net. Rung 0 of the
  ladder needs no filesystem at all.
- **Prove it offline first.** 69 tests, host PNG renders of every screen, a GLIBC ≤2.23 gate, and a
  qemu construction preflight against the device's real libraries — all before anything is flashed.
- **`cinder-probe` before `cinder-home`.** Risky code paths get exercised with no easel lifecycle,
  so they cannot affect boot.
- **Reuse Sony's engine.** Every effect kept working is a feature not lost.
- **The device is someone's daily music player.** Batch disruptive changes; don't leave it broken.
