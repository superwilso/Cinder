# Cinder One — spec

**Status: a proposal, written 2026-09-22.** It exists so decision 2 in
[`VISION_four_builds.md`](VISION_four_builds.md) §7 ("Cinder One — yes or no?") can be answered
against a concrete design rather than a slogan. Nothing here is built. Where a claim rests on static
analysis rather than a measurement on hardware, it says so.

Background, all of it measured: [`../analysis/RE_walkmanone_extract.md`](../analysis/RE_walkmanone_extract.md).

---

## 1. The thesis, restated precisely

Walkman One's audio gains and Walkman One's feature losses have **different causes**.

* The **gains** are file swaps and runtime writes: the audio HAL, the analyser params, the gain
  tables, `dacdat`, and (optionally) a tuning blob in NVRAM.
* The **losses** — FM, VPT, ClearAudio+, Language Study, Line-out — come from the **model swap**.
  W1 rewrites the player's identity to NW-WM1Z, and Sony's own app then hides what a WM1Z does not
  have. Noise cancelling is the one genuine deletion: W1 also drops the NC tables and the DNC
  kernel module.

**We have no reason to swap the identity.** Cinder replaces the Home app, so "Sony's app hides
features on a WM1Z" simply does not apply to us — we decide what our own UI shows. That is the whole
idea: take the gains, decline the swap, keep the features.

> **Cinder One is not a fork of Walkman One. It is the set of things W1 does that are worth doing,
> minus the one thing that causes all of its losses.**

---

## 2. What this means structurally: there is no boot script

This is the most important consequence, and it only became clear once `boot_complete.sh` was
decoded.

Walkman One **needs** a 1452-line script that runs on every boot because it is maintaining a lie.
The player's NVP says WM1Z; the hardware is an A55; and a per-signature, per-mode payload has to be
re-applied every boot to keep the two consistent — different translations per signature × mode,
different analyser params, a different HAL. That machinery exists to service the model swap.

Decline the model swap and nearly all of it evaporates:

| What W1's boot script does every boot | Does Cinder One need it? |
|---|---|
| Re-apply `lang/$SIG/$MODE/*` to `/system/vendor/sony/translations/` | **No.** Those are Sony's Home-app strings. We are the Home app. |
| Re-apply `conf_a/b/c` NVP config images | **No.** That *is* the model swap. |
| Re-apply `adler/$MODE/libaudiohal-adleralsa.so` | **No** — a one-shot patch, already implemented by `cinder-signature.sh`, which additionally splits the ALSA-device change from the CPU-floor change (W1 ships them welded together). |
| Re-apply `anls/$MODE/*` analyser params | Once, at install. Not every boot. |
| Re-apply `gain/gain_{n,l}` to `/usr/share/audio_dac/` | Once, at install — `cinder-voltable` already installs tables this way. |
| Region (`REG`) handling, `nvpflag` writes | Once, if wanted at all (see §5). |
| `boot_count`, `sig`, `/opt2/stock` backups | Only to the extent we write NVP/NVRAM at all. |

**So Cinder One is not a new boot-time component. It is install-time work plus a handful of
runtime helpers, most of which Cinder already has.** That is a much smaller and much safer thing
than "write our own firmware", which is how the idea has been framed until now.

---

## 3. Payload inventory

What an install would actually place, and what it costs. Tiers are by blast radius, not by effort.

### Tier 0 — plain file swaps in `/system`, reversible, already built

| Item | Mechanism | State |
|---|---|---|
| Sound signature | 3-byte patch to `libaudiohal-adleralsa.so` | **Built** — `cinder-signature.sh`, md5-verified both sides, `.stock` backup, atomic |
| Wired volume curve | table into `/proc/icx_audio_cxd3778gf_data/` | **Built** — `cinder-voltable` (setuid) |
| Analyser params | copy `anls/$MODE/*` | Not built. Straight file copy; needs a source (see §6) |

### Tier 1 — runtime writes, no persistent state

| Item | Mechanism | State |
|---|---|---|
| Gain mode (normal / lower) | `ov_127x*.tbl` + `dacdat ovt` | Not built. Stock `dacdat` already accepts the W1 arguments and is byte-identical to W1's |
| Full DAC reprogramming | `dacdat auto BBDMP2_linux …` | Not built. Same binary, same story |
| **FM radio** | `insmod radio-si4708icx.ko`, and either a `regmon`-only bring-up or stock 1.02's `libTunerPlayerService.so` | Screen and chip layer **already built**. See §4 — the library swap may not be needed at all |

### Tier 2 — NVP field writes (recoverable, but persistent)

| Item | Mechanism | Note |
|---|---|---|
| Region / volume cap | `nvpflag` `rflcountry` / `rflsku`, `dacdat limiter_*` | Independent of `fpi`. Decision 4 in the vision doc. Changes what a volume step does to your ears — opt-in and documented, or not at all |

### Tier 3 — block writes to NVRAM and the bootloader

| Item | Mechanism | Note |
|---|---|---|
| External tunings | `dd` `2.bin` → `mmcblk0p3` (NVRAM), `3.bin` → `mmcblk0p7` (**uboot**) | This is the one that writes the bootloader. Decision 3. See §7 |

**Recommendation: ship Tiers 0 and 1. Put Tier 2 behind an explicit opt-in. Leave Tier 3 out of
the installer entirely** — at most, *read and report* which tuning is present, which needs no write
at all.

---

## 4. FM: the one loss we can actually take back

Measured 2026-09-21/22, and the strongest single result behind this spec.

* The **Si4708 tuner is physically present and answering on Walkman One**: `DEVICEID 0x1242`,
  matching Wampy's reference dump. The driver `radio-si4708icx.ko` loads and creates `/dev/radio0`.
* W1 ships the **mock** `libTunerPlayerService.so`: 79,916 B / 288 dynamic symbols against stock
  1.02's 96,308 B / 346 — **58 missing, none added**, and the missing set is precisely the hardware
  layer. Stock's copy has `ioctl` and `__open_2` undefined; the mock does not need them.
* **The swap links.** Stock's version wants `libConfigurationService.so`, which W1 carries at the
  same 194,608 B and which exports all 9 config symbols it imports; across the board, **all 44
  Sony-namespace undefined symbols in stock's library resolve** against W1's own
  `libpstcore`/`libcxxrt`/`libConfigurationService`.

Two traps, both documented by Wampy and both solved there:

1. On a power press the player sets power state to `mem`, which kills the radio → hold a **wakeup
   source**.
2. A service flips ALSA `numid=26 'analog input device'` from `tuner` back to `off` on a timer →
   **poll it**, roughly 1 Hz. `amixer sevents` / ALSA's `monitor.c` see nothing here, because that
   mixer is driven by `ioctl`, not filesystem events.

### The cheaper route: Cinder may not need the library at all

Cinder's FM is already built, and **it already drives the chip directly**.
`cinder-audio/src/tuner_shim.cpp` carries a `regmon` layer over
`/proc/regmon/Si4708icx/{target,value}` that reads `DEVICEID` to prove the chip is there, **writes
`CHANNEL` with the `TUNE` bit and waits on `STATUS_RSSI.STC` to tune**, drives
`POWERCFG.{SEEK,SEEKUP,SKMODE}` for the chip's own hardware seek, reads the channel plan out of
`SYSCONFIG2` rather than assuming it, and already names `POWERCFG.ENABLE` and `POWERCFG.DMUTE`.
None of that goes through `libTunerPlayerService.so`.

What still does is exactly **three vtable calls** in `cinder_tuner_start()`:

```
route(true);                 // 'analog input device' = tuner   — a MIXER control, independent
T_Open                       // <- TunerPlayerService
T_SetFrequency               // <- TunerPlayerService   (regmon already does this)
T_Play                       // <- TunerPlayerService
AudioIn A_Play               // <- AudioInPlayerService, a DIFFERENT service
```

`AudioInPlayerService` is not the mock — it is a separate service, and `hagoromo28` starts it on
Walkman One alongside the stub tuner. The mixer route is a plain ALSA control. So of the five steps,
**only the three middle ones touch the missing library, and one of them is already implemented
twice.**

That suggests a `regmon`-only bring-up, used as a FALLBACK when `TunerPlayerService` is the stub:

1. `route(true)` — unchanged;
2. `POWERCFG` ← `ENABLE | DMUTE` (replaces `T_Open`; the constants are already in the file);
3. tune via `CHANNEL | TUNE`, wait `STC` (replaces `T_SetFrequency` — **already written**);
4. `AudioIn A_Play` — unchanged.

**If that holds, FM on Walkman One needs no library swap and no redistribution question — just
`insmod radio-si4708icx.ko` and a fallback path.** It is also the better design on stock, because it
removes a dependency on two Sony primitives the file already documents as broken
(`GetSignalLevel` returns a constant, `StartAutoTuning` is a 48-byte stub).

**Deliberately not implemented tonight.** It is a handful of register writes, but it is in the
boot-critical audio shim, on a path whose own header records that a wrong argument
(`AudioIn Play("tuner")`) **rebooted the device**. Register pokes written blind and shipped
untested is the wrong trade; this wants a device session where each step is checked as it lands.

### Needs the device

Everything in §4 is static analysis plus one register dump. Nothing has been played through it.
In order, cheapest first:

1. Does `POWERCFG ← ENABLE|DMUTE` + `CHANNEL|TUNE` via `regmon` produce a carrier on Walkman One
   with no Sony tuner service at all? (Read `STATUS_RSSI` back — no audio needed to answer it.)
2. Does `AudioIn A_Play` + `analog input device = tuner` then carry that to the jack?
3. Only if 1 or 2 fails: swap stock 1.02's `libTunerPlayerService.so` in and retry. Linkage is
   proven; loading is not.
4. The two power traps — the `mem` transition on a power press, and the timer that flips the mixer
   back to `off`.

---

## 5. What we keep that W1 loses

The honest answer is **all of it, by construction** — because the losses are consequences of the
model swap, and we do not swap.

| W1 loses | Cinder One | Why |
|---|---|---|
| FM Radio | **Kept**, with a library swap and our own screen | §4 |
| VPT Surround | **Kept** | Model-gated in Sony's UI only; the DSP enums are catalogued |
| ClearAudio+ | **Kept** | Same |
| Language Study | **Kept** if wanted — decision 6 | Same |
| Line-out | **Kept** | Same |
| Noise cancelling | **Kept**, because we never remove it | W1 deletes the NC tables and the DNC module; we simply do not. Note NC is inert without Sony NC headphones regardless |

---

## 6. Where the payload comes from — the licensing question

W1's `.mod` tree is **MrWalkman's redistributable**, not ours, and the external tunings are the
paid part of that product. The parts Cinder One wants divide cleanly:

* **From Sony's own stock firmware** — the audio HAL, `dacdat`, `libTunerPlayerService.so`, the
  gain tables, the analyser params. These already exist on the user's own player, or in the stock
  `.UPG` they can download from Sony. **Cinder should derive these from the device or from a
  user-supplied stock package at install time, never ship them.** `cinder-signature.sh` already
  works exactly this way: it patches the library that is on the device.
* **From Walkman One** — the external tunings. Not ours to redistribute, and Tier 3 anyway.

This is not a side note. It decides the shape of the installer: **Cinder One is a set of
transformations applied to files the user already has**, not a payload we carry.

---

## 7. What it costs, honestly

* Tiers 0–1 are a modest amount of work on top of components that already exist, and carry roughly
  the risk Cinder already carries.
* Tier 3 is a different category. `3.bin` goes to `mmcblk0p7` — **the bootloader**. wbrt does not
  restore the preloader, and a bad uboot is below every escape the project has. The payoff is a
  sound-signature nuance. That trade looks bad, and §3's "read and report only" middle ground
  captures most of the value for none of the risk.
* The genuine new surface is FM: a service to drive, a screen to draw, and two power behaviours to
  fight. That is real work, but it is ordinary application work, not firmware work.

---

## 8. Needs the device

1. Swap stock's `libTunerPlayerService.so` onto W1 and confirm `TunerPlayerService` starts with the
   real implementation (linkage is proven statically; loading is not).
2. Confirm tuner audio reaches the jack with `analog input device = tuner` — and that this does not
   repeat the reboot seen when `Play("tuner")` was called directly.
3. Measure the wakeup-source and 1 Hz poll workarounds.
4. Confirm the NC tables and DNC module are intact on a non-W1 install (the §5 claim is a file-level
   diff, not a listening test).
5. Anything in Tier 2 or Tier 3 — untouched so far, deliberately.
