# Cinder as a full firmware replacement — four builds, and what can go in them

**Draft for selection, 2026-09-21.** Nothing here is decided. It exists so the owner can tick
things and so we are demonstrably on the same page before any of it is built.

Supersedes nothing yet. When it is agreed it should replace the scope section of
[`VISION.md`](../VISION.md) and absorb [`PLAN_walkman_one_parity.md`](PLAN_walkman_one_parity.md).

---

## 1. The four builds

Two independent axes: **what firmware is underneath**, and **what the UI looks like**.

|  | **Cinder UI**<br>(amber on near-black, Cinder's own) | **Stock-like UI**<br>(Sony's look, Cinder's guts) |
|---|---|---|
| **Stock firmware**<br>Sony 1.02, unmodified | **① Cinder Stock**<br>*What exists today.* | **② Stock Stock**<br>Looks like Sony, every Cinder gain underneath |
| **Walkman-class firmware**<br>W1, or our own | **③ Walkman Cinder** | **④ Walkman Stock** |

All four are the **same codebase**. They differ by a theme choice and a firmware target, not by a
fork. That is the whole point of writing it as a 2×2: if either axis turns into a fork, the matrix
has failed.

### What each axis actually costs

**The UI axis is cheap and well-bounded.** Cinder's UI is already a separate Rust crate
(`player/cinder-ui`) that is pure render + navigation state, no I/O, with a host-side harness that
renders every screen to PNG without a device. A "stock-like" build is a **theme**: palette, type
scale, row metrics, and the hairline-under-title chrome. The measurements are already recovered and
written down — `analysis/ui_assets/UI_DESIGN_SPEC.md` in the Sony-files repo. This is ordinary
work with a fast feedback loop and no device risk.

**The firmware axis is where the difficulty is,** and tonight proved it rather than predicted it.

---

## 2. What tonight established

Cinder was installed onto this Walkman One player and **the player boot-looped.** Recovery is by
wbrt, in progress at the time of writing.

What is known:

* The install itself was clean — `.appcfg` repointed, all nine binaries placed with their setuid
  bits, `.appcfg.real` backed up, launcher written.
* **The loop survived a cable-in boot**, which is rung 0 of the escape ladder and needs no
  filesystem. A loop that outlives rung 0 is happening *before or beneath* the launcher.
* The first hypothesis was an ABI mismatch: `cinder-home` is built against stock 1.02's Sony
  libraries, Walkman One is 3.02, and a Home app that fails appmgr's Foreground handshake gets the
  device rebooted by design. **That hypothesis is dead.** It was tested off-device by extracting Walkman
  One's `/vendor/sony/lib` from its system image and diffing against the catalogued 1.02 set:
  **all 13 Sony libraries `cinder-home` links against export exactly the same symbols, and
  `libeaselcore`, `libeaselcui`, `libpstcore` and `libappmgrservice` are byte-identical.** The
  appmgr/easel handshake path is literally the same code on 3.02 as on 1.02. Cinder did not fail
  because of a library mismatch.
* **What actually looped it is not known.** Across the whole `/vendor/sony/lib` set, 65 libraries
  are byte-identical and 18 differ — and of the ones Cinder links, only **`libEffectCtrlDmp.so`**
  and **`libMediaStoreServiceClient.so`** differ, both with identical exports and different code.
  A behavioural difference there (or in the `hagodaemon`-hosted services behind them —
  `libPlayerService`, `libSoundServiceFw`, `libMediaStoreService` all differ) would crash
  `cinder-home` after it registers, and appmgr reboots a Home app that does not hold Foreground.
  That is the new leading hypothesis and **the log settles it, not more reasoning**.
* **Wampy runs fine on Walkman One.** It is an *interface addon*: it leaves Sony's Home app in
  place and draws over it, so it never has to satisfy appmgr. Cinder replaces the Home app and
  therefore must. That architectural difference is the single biggest reason Wampy's device support
  matrix is wide and Cinder's is one unit.

**Consequence for this document:** builds ③ and ④ are not "① with a different base". They need a
firmware-compatibility story that does not exist yet. Builds ① and ② do not.

---

## 3. The idea that makes "our own Walkman One" worth doing

The owner's list of what Walkman One **removes** relative to stock:

> FM Radio · VPT Surround · Language Study · ClearAudio+ · Noise cancelling · Line-out

**Those losses come from the model swap, not from the audio changes.** Walkman One makes the player
identify as an NW-WM1Z/DMP-Z1 (`fpi`), and those models have no FM tuner and a different feature
set, so Sony's own app hides the A50 features that the new identity does not have. Noise cancelling
is a harder loss: W1 also physically drops the NC tables (`ncgain_*`, `ambgain*`) and stops loading
`cxd3778gf_dnc_core.ko`.

| loss | why | proven? |
|---|---|---|
| Noise cancelling, Ambient Sound | W1 drops the NC/ambient tables and the DNC kernel module | **Proven** — file-level diff of the two system images |
| FM Radio | The WM1Z identity has no FM hardware, so the feature is gated off | Inferred from the mechanism + owner's report |
| VPT, ClearAudio+, Language Study, Line-out | Model-gated UI on the new identity | Inferred, same |

Now put that beside what Walkman One's audio gains actually **are** — all measured, all in
`analysis/RE_walkmanone_extract.md`:

| W1 audio gain | mechanism | needs the model swap? |
|---|---|---|
| "Plus mode" / sound signature | swap `libaudiohal-adleralsa.so` (3 bytes: ALSA device + CPU floor) | **No** — plain file swap. Cinder already does it (`cinder-signature.sh`) |
| Gain modes (normal / lower) | copy `ov_127x*.tbl` then `dacdat ovt` | **No** — `dacdat` applies at runtime through `/proc/icx_audio_cxd3778gf_data/` |
| Full DAC reprogramming | `dacdat auto BBDMP2_linux …` | **No** — the *stock* `dacdat` binary already accepts `BBDMP2_linux`, and is byte-identical to W1's |
| Region / volume limiter | NVP `rflcountry`/`rflsku` + `dacdat limiter_*` | **No** — an NVP field write, independent of `fpi` |
| **External tunings** (Bright / Neutral&Warm / WM1Z) | **`dd` a blob onto NVRAM `p3` and uboot `p7`** | **No — and this is new** (§4) |

> **So "our own Walkman One" is not a smaller Walkman One. It is a strictly larger one:** every
> audio gain, and none of the feature losses, because there is no reason for us to swap the model
> identity at all. Working name in this document: **Cinder One**.

---

## 4. The external tunings are reachable after all — correction

`analysis/RE_walkmanone_extract.md` has said since 2026-08-17 that the external tuning packages are
**"CLOSED, negative"**: undecryptable behind an unknown key. The decryption half of that still
stands — but the conclusion drawn from it was wrong, and Wampy shows why.

Wampy applies Walkman One sound signatures **on the device, with no PC**, like this
(`src/w1/w1.cpp`):

```
upgtool-linux-arm5 -w -m nw-wm1a -z 2 -z 3 -e -o <workdir> <tuning>/Data/Device/NW_WM_FW.UPG
dd if=<workdir>/2.bin of=/dev/block/mmcblk0p3      # NVRAM
dd if=<workdir>/3.bin of=/dev/block/mmcblk0p7      # uboot
```

Three things fall out of that:

1. **The key is `nw-wm1a`** — the same key the player carries, and the same one already tried here.
   The only obstacle is the **header signature check**, which unknown321's `-w` flag ignores. The
   payload decrypts fine.
2. **The tuning is an NVRAM + bootloader write.** Independently confirmed on the player tonight:
   live NVRAM md5 is byte-identical to `/opt2/stock/nv_bk`, and Walkman One's own boot script
   decides "is a tuning applied?" with exactly `diff /opt2/stock/nv_bk /dev/block/mmcblk0p3`.
   The earlier guess that it lived in NVP was wrong.
3. **Nothing in that route needs `fpi`, the model swap, or Sony's updater.** The model swap exists
   only so Sony's *Windows* updater will accept the package. If we extract the payload ourselves,
   the identity is irrelevant.

**Status: not yet extracted here.** Our `upgtool` build has `-f/--force` but it does not skip the
signature compare, and patching that compare out was blocked as a security weakening — see §8.

**And a large red flag on the other side of it:** `3.bin` goes to **`mmcblk0p7`, uboot**. Writing
the bootloader is the single most brick-prone operation anywhere in this project, and wbrt's
coverage **does not include the preloader**. Any Cinder feature built on this must be opt-in,
wbrt-gated, and must never run unattended.

---

## 5. The feature matrix — tick what you want

Legend — **feasibility**: 🟢 have it / straightforward · 🟡 real work, known route · 🔴 open
question or hazard · ⚫ not reachable.

### 5A. Playback and library

| Feature | Stock | W1 | Wampy | Cinder now | Feasible | Notes |
|---|---|---|---|---|---|---|
| Library browse (album/artist/genre/folder/year) | ✅ | ✅ | via Sony | ✅ | 🟢 | |
| Play queue + Up Next | ❌ | ❌ | ❌ | ✅ | 🟢 | Cinder-only; stock has no queue at all |
| Shelf | ❌ | ❌ | ❌ | ✅ | 🟢 | |
| Playlists (.m3u8) | ✅ | ✅ | — | ✅ | 🟢 | |
| Library search | ❌ | ❌ | ❌ | ✅ (new) | 🟡 | untested on a player |
| Lyrics (.lrc) | ❌ | ❌ | ❌ | ✅ (new) | 🟡 | untested on a player |
| CUE sheets | ❌ | ❌ | ✅ | ❌ | 🟡 | Wampy has it; ordinary work |
| Bookmarks / export | ❌ | ❌ | ✅ | ❌ | 🟡 | |
| ATRAC, region-free | partial | ✅ | ✅ | ❌ | 🟡 | |
| SensMe channels | ✅ | ✅ | reads them | ⚙️ off | 🟢 | Solved: Flint tags, the device computes. Needs channel screens |
| Per-song audio settings | ❌ | ❌ | ✅ | ❌ | 🟡 | Wampy's `EQ/Song` |

### 5B. Sound

| Feature | Stock | W1 | Wampy | Cinder now | Feasible | Notes |
|---|---|---|---|---|---|---|
| 10-band EQ | ✅ | ✅ | ✅ | ✅ | 🟢 | |
| 6-band EQ | ✅ | ✅ | ✅ | ❌ | 🔴 | Writes arrive, filter does not engage — measured null |
| **Clear Bass** | ✅ | ✅ | ✅ | ❌ | 🟡 | Band 0 of the six-band. Filter fully decoded → can be recreated on the ten-band even if the six-band never engages |
| Tone control | ✅ | ✅ | ✅ | partial | 🟡 | |
| DSEE HX | ✅ | ✅ | ✅ | ✅ | 🟢 | |
| DC Phase Linearizer | ✅ | ✅ | ✅ | ✅ | 🟢 | |
| Dynamic Normalizer | ✅ | ✅ | ✅ | partial | 🟡 | |
| Vinyl Processor | ✅ | ✅ | ✅ | partial | 🟡 | |
| **VPT Surround** | ✅ | ❌ | ✅ | partial | 🟡 | W1 loses it; Cinder One keeps it |
| **ClearAudio+** | ✅ | ❌ | ✅ | ❌ | 🟡 | same |
| DSP applied to Bluetooth | ❌ | ❌ | ? | goal | 🔴 | Long-standing stretch goal |
| Volume-table swap (WM1A curve) | ❌ | ✅ | ✅ | ✅ | 🟢 | `cinder-voltable`; needs W1's `gain_l` tables present |
| Sound signature (HAL + CPU floor) | ❌ | ✅ | ✅ | ✅ | 🟢 | `cinder-signature.sh` reproduces it byte-for-byte |
| **External tunings** | ❌ | ✅ | ✅ | ❌ | 🔴 | §4. Route known; writes uboot; needs extraction |
| Region / volume-cap change | ❌ | ✅ | ✅ | ❌ | 🟡 | NVP field write. Hearing-safety decision, not just a feature |
| **Noise cancelling** | ✅ | ❌ | ✅ | ❌ | 🔴 | Inert without Sony NC headphones — not a software limit |
| Ambient Sound Mode | ✅ | ❌ | ✅ | ❌ | 🔴 | same |
| **Line-out** | ✅ | ❌ | ? | ❌ | 🟡 | Cinder One keeps it by not swapping the model |
| High-gain output | ✅ | ✅ | ✅ | ❌ | 🟡 | Codec surface already mapped |

### 5C. Radio, USB, Bluetooth

| Feature | Stock | W1 | Wampy | Cinder now | Feasible | Notes |
|---|---|---|---|---|---|---|
| **FM radio** | ✅ | ❌ | ✅ (needs W1) | ✅ | 🟢 | Cinder's has a real signal meter + fast scan |
| FM recording | ❌ | ❌ | ✅ | ❌ | 🟡 | |
| Extended FM band (76–108) | ❌ | ❌ | ✅ | ❌ | 🟡 | |
| USB-DAC in | ✅ | ✅ | ✅ | 🔴 gated | 🔴 | Needs `FuncMode==1`; recovered, not shipped |
| Low-latency USB-DAC (llusbdac) | ❌ | ❌ | ✅ | ❌ | 🟡 | Third-party module, MIT |
| **USB-DAC in + LDAC out together** | ❌ | ❌ | ❌ | ✅ headline | 🟡 | Cinder-only |
| LDAC / codec selection | ✅ | ✅ | ✅ | ✅ | 🟢 | |
| Bluetooth receiver mode | ✅ | ✅ | — | ❌ | 🟡 | Named as not-implemented in the README |
| BT remote (RMT-NWS20) | region | ✅ | ✅ | ❌ | 🟡 | W1's `REM` |
| USB-MSC | ✅ | ✅ | — | ✅ | 🟢 | |

### 5D. System, UI, platform

| Feature | Stock | W1 | Wampy | Cinder now | Feasible | Notes |
|---|---|---|---|---|---|---|
| Stock-like UI skin | ✅ | ✅ | — | ❌ | 🟡 | **Build ② / ④.** Spec already written |
| Winamp / cassette / clock skins | ❌ | ❌ | ✅ | ❌ | 🟡 | Wampy's signature feature |
| Custom palette from a text file | ❌ | ❌ | ✅ | ✅ | 🟢 | |
| Home icon colour | ❌ | ✅ | ✅ | ❌ | 🟢 | It is just `nvpflag clv` — free |
| Scrobbler | ❌ | ❌ | ✅ addon | ✅ built-in | 🟢 | |
| Clock set on device | ❌ | ❌ | ✅ | ✅ | 🟢 | `cinder-clock` |
| **Language Study** | ✅ | ❌ | — | ❌ | 🟡 | Reimplementation; low priority unless wanted |
| Faster boot / better battery | — | — | — | goal | 🔴 | Unmeasured against stock |
| Night mode + backlight dim | ❌ | ❌ | — | partial | 🟡 | Palette done, backlight dim outstanding |
| W1 settings as on-device rows | ❌ | text file | ✅ | ❌ | 🟢 | Wampy already does this; highest-value "easier access" win |
| Deep idle / suspend | ❌ | ❌ | — | ✅ achieved | 🟡 | USB gadget won't re-enumerate after resume |

---

## 6. What has to be true before ③ and ④ are possible

In order. Each one is cheap relative to the one after it.

| # | Question | How it gets answered |
|---|---|---|
| 1 | **Why did it loop?** | Get `/contents/cinderhome.log` and `/data/cinder/*` off the player after wbrt. If the launcher wrote breadcrumbs, the loop is above it; if it wrote nothing, it is beneath it |
| 2 | **Does `cinder-home` even start on 3.02?** | Run it from a shell with the stock app still Home — it will fail to take Foreground, but a link error, a missing symbol or a SIGSEGV shows up immediately and cannot loop the device |
| 3 | ~~**Which Sony libraries differ 1.02 → 3.02?**~~ **DONE — 0 symbol differences; easel/appmgr byte-identical.** Remaining: why `libEffectCtrlDmp` / `libMediaStoreServiceClient` differ in code | Off-device, done |
| 4 | **Can appmgr be satisfied at all on 3.02?** | Only after 2 and 3 |
| 5 | **Do we need per-firmware builds, or one binary with runtime detection?** | Falls out of 3 |

Until (1) and (2) are answered, **no further install attempt on Walkman One.** The cheap
experiments are all off-device or shell-only.

---

## 7. Decisions I need from you

1. **Which of the four builds are actually wanted**, and in what order? My recommendation: **② first**
   (stock-like UI on stock firmware — all upside, no firmware risk, and it makes ④ almost free
   later), then ①/② share everything, then ③/④ once §6 is answered.
2. **Cinder One — yes or no?** Building our own firmware layer is a real expansion of scope. The
   payoff is "W1's audio with none of W1's losses". The cost is that writing NVRAM and uboot enters
   the project.
3. **Do you want the external tunings at all**, given `3.bin` writes the bootloader? A defensible
   middle: support *reading and reporting* which tuning is applied, support the HAL/gain/dacdat
   half, and leave the uboot write out.
4. **Region / volume-cap unlocking — in or out?** It is reachable. It changes what a volume step
   does to your ears. I would keep it behind an explicit, documented opt-in or leave it out.
5. **Wampy overlap.** Skins, per-song EQ, CUE sheets, FM recording and llusbdac are all things
   Wampy already does well, on more devices. Worth deciding which we duplicate and which we simply
   recommend Wampy for.
6. **Language Study, ATRAC, BT receiver mode** — genuinely wanted, or listed for completeness?

---

## 8. One blocked step

Extracting the external tunings needs an `upgtool` that tolerates a header-signature mismatch —
exactly what unknown321's `-w` flag does. Patching our local copy of Rockbox's `upg.c` to skip that
compare was refused by the sandbox as a security weakening, and I did not route around it.

Three clean ways forward, your call:

* grant that one patch explicitly, or
* use unknown321's prebuilt `upgtool-linux-arm5` (it lives in his `nw-installer` repo, not in the
  Wampy checkout we have), or
* leave the tunings unextracted and keep §4 as analysis only.
