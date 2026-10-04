# Device verification checklist

**What this is.** One ordered list of everything that can only be settled with the NW-A55 in hand,
consolidated from the places it was scattered: `cinder-home/ROADMAP.md`'s P0 table, `STATUS.md`'s
running "device-unverified" notes, `docs/DEVICE_TESTS.md` (the ear tests), `ldac-bridge/TEST.md`,
`docs/BATTERY_BT.md` and the audits. Where a detailed procedure already exists this file links to it
rather than restating it.

**Written 2026-08-24, after a working day of off-device work that produced fourteen fixes across two
PRs and touched nothing on hardware.** That is the point: the list below is what that work owes.

Ordered by **safety gradient** first (things that cannot affect boot, then things that can) and by
payoff within each phase. Every item says what to do, what a PASS looks like, and what to do if it
fails.

---

## Rules that do not bend

These are not preferences. Each one is written from something that already went wrong.

1. **A current `wbrt` backup exists before any write.** It is the only thing that recovers a brick on
   this device — there is no public USB DFU/EDL path for the audio SoC. A backup is
   device-specific: never restore one unit's dump to another, it overwrites the serial and the
   factory calibration with no recovery.
2. **Never write `/proc/regmon/<chip>/value.`** Reading the codec's registers is free; writing one
   changes the audio hardware under the running player, and the codec is the one part of this device
   with no software recovery path.
3. **Never write to the `BtTransmitterService` PCM fd without the handshake.** PCM sent while the
   connection is parsing frames is read as a type and a length, and a garbage length reaches
   `operator new[]` inside a core Sony service. That rebooted the device twice on 2026-08-11.
4. **Do not guess vtable slot indices** into Sony services. Recover them, or leave the feature off.
5. **Boot with the cable OUT.** A cable at boot is itself an escape route to stock; using it up on an
   ordinary boot means it is not there when a boot goes wrong. For a **cable-heavy session** —
   flash, reboot, flash, reboot, which otherwise lands on stock every time — take the opt-out
   explicitly and give it back at the end:
   ```sh
   sudo tools/flash.sh --cable-off     # cable at boot no longer escapes to stock
   sudo tools/flash.sh --cable-on      # PUT IT BACK when the session ends
   ```
   It is a **loan, not a setting** (`cinder-install.sh` treats its `/data` twin the same way):
   while it is set, rung 0 — the one escape that needs no filesystem, no shell and no working
   counter — is gone, and you will not notice until the boot you needed it. Rung 1 (the bad-boot
   counter, MAXBAD=4) still covers a build that will not start.
6. **Probe before repointing `.appcfg`.** The probe path has no easel lifecycle, so it cannot affect
   boot. Nothing that can affect boot happens until a probe run looks clean.

---

## Before the device is touched at all

| # | Check | Why it is here and not in CI |
|---|---|---|
| 0.1 | **`cinder-home/build.sh [stable\|dev]` passes** | This is the only gate that does the ARM link, the **GLIBC ≤ 2.23 ceiling** and the **qemu construction preflight**. CI deliberately does not carry the cross toolchain, so a green CI says nothing about whether the thing links for the device. **This gate earned its keep on 2026-08-25**: the tree did not compile for ARM at all — `a1f91f2` silenced an unused-parameter warning by commenting out a name that the `#if defined(__arm__)` body still used, which only host builds can skip. Note `build.sh` is not executable in a fresh checkout; run it as `bash build.sh dev`. |
| 0.2 | **`cinder-home/harness/run.sh` passes** | Thirteen scenarios, ~8 s — the strongest offline check of the app's *behaviour*, and what the fixes below were written against. **`build.sh` now runs it**, so 0.1 covers this; run it alone when iterating. |
| 0.3 | **`tools/release.sh`** if flashing a release | Verifies the committed `dist/` payload byte-for-byte against a fresh build before it will tag. |
| 0.4 | Escape ladder intact | Bad-boot counter → auto-revert → crash supervisor → kill switch → `wbrt` restore. `cinder-home/tools/test_launcher.sh` covers it offline and **now runs in CI**, so a green tick already says this. Run it by hand only if you changed the launcher. *(55 cases as a normal user; one of them uses `chmod`, which does not bind uid 0, so it skips itself when the suite is run as root.)* |

---

## Phase 0 — zero boot risk (probe and adb only)

Nothing here loads the easel lifecycle, so none of it can affect whether the device boots.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 0a | ~~**`ldac-bridge/TEST.md`** — the headline feature, **0% validated**~~ **Q1 PASS 2026-08-25** | `cinder-probe --ldac` (no flash, no stock boot needed) | `GetSocketName` = `pst::services::bttransmitterservice`, socket connected | **Q2 still open**: no capture PCM exists until the gadget is in UAC mode with a PC feeding audio, and that drops adb — hands-on |
| 0b | ~~**`cinder-probe --analyzer`** — has never been run once~~ **PASS 2026-08-25** | With music playing (`--pump` alongside) | Frames arrive | Recorded: **12 bands, LINEAR values ~0…1.7e6**, not dBFS |
| 0c | ~~**A `PlayStatus` dump with music actually playing**~~ **PASS 2026-08-25** | `cinder-probe --pump` | Position advances, `ALSA pcm4p = RUNNING`, listener fires | Also settled **3d**: `duration_raw` is **milliseconds**. Note `--play` is the framework-DEAD control and cannot connect — use `--pump` |
| 0d | ~~**`cinder-probe --discover`**~~ **PASS 2026-08-26** | `--discover <report> <media path>` on a FRESH BOOT | Non-zero PlayStatus bytes | **Done — full offset map recovered**: position `+0x48`, duration `+0x4c`, channels `+0x5c`, bits `+0x60`, rate `+0x64`, bitrate `+0x68`, playstate `+0x00`. The zeros were never "nothing was playing": `cinder-home` held the audio output, so `WMX_AudioOutput::Open()` failed `OMX_ErrorHardware` and the log blamed the file (`GAP_E_UNSUPPORT_FORMAT`). Run playback probes before touching the app. |
| 0e | **MediaStore re-scan probe** (§1a of the 08-23 audit) — **first half DONE 2026-08-25** | ~~Recover the `MediaStoreClient` vtable~~; then `strace -f` the `hagodaemon` hosting MediaStoreService across a stock USB-MSC disconnect | ~~The slot map~~, **and** whether a scan is app-driven at all | **No vtable needed**: `MediaScanner` is exported concretely — ctor takes `IMediaStoreService*`, plus `Scan()`/`ScanFile()`/`Cancel()`. `strace` is on the device at `/system/xbin/strace`; the MSC half still needs a real disconnect, which kills adb |

> **USB-MSC itself is confirmed working on device (2026-08-26).** What 0e still needs is not MSC but the `strace` ACROSS a disconnect, which kills adb — so it has to be started detached, survive the disconnect, and be read back afterwards.
| 0f | ~~**`--btwho`, `--inpath 2`, `--userpreset`**~~ **PASS 2026-08-25 (BT half pending a peer)** | With a peer linked and music playing | Consistent with the RE notes | `--userpreset` and `--inpath 2` match the notes exactly and the chain is restored on exit. `--btwho` read `GetBtStatus=7` (off) with nothing connected — the with-a-peer half needs headphones. **One anomaly:** under selector 2, `Eq6band` also reports `isproc is 1` — see the 08-25 results |

> **On 0e, and why the `strace` half is the important half.** §1a of the 08-23 audit raises a
> possibility worth settling before any IPC is written: the scan may not be app-driven at all. If
> `MediaStoreService` watches the volume being mounted, then Cinder's problem is that `cinder-msc`
> does the unmount and remount **itself** (`mount(2)` directly, because uid 100 cannot use init's
> path) and no Sony service is in the loop at the moment the volume changes. **The fix would then be
> in `cinder-msc`, not in a vtable call** — much cheaper and much safer.
>
> I could not narrow this off-device: `artifacts/` is gitignored and empty in a fresh clone, so the
> extracted rootfs and the full `init*.rc` are not here (`analysis/4a_init_system.txt` and
> `4b_init_flow.txt` are 2 and 46 lines — summaries, not the scripts). Re-running `make phase2`
> would recover them and might answer it without the device, and that is worth an hour before the
> next session.
>
> The half that IS settled: **once the database changes, Cinder notices and reloads it, once, then
> settles.** That is the `library-changed` harness scenario. So the remaining work is the trigger
> alone, not the trigger plus the reload.

---

## Phase 1 — first boot of the new build

Flash `dist/dev/`, cable **out**.

| # | Item | PASS |
|---|---|---|
| 1a | ~~It paints~~ **PASS 2026-08-26** | A frame reaches the glass; the boot animation does not stay latched |
| 1b | ~~Library loads~~ **PASS 2026-08-26** | Album/artist/song counts look right for a 304-album library |
| 1c | ~~**Bad-boot counter cleared**~~ **PASS 2026-08-26** | `healthy: bad-boot counter cleared` in `/contents/cinderhome.log` within ~10 s of first paint |
| 1d | ~~Type scale and non-Latin rendering~~ **PASS 2026-08-26** | Eyeball; nothing clipped, no tofu |
| 1e | ~~Touch navigation lands where drawn~~ **PASS 2026-08-26** | Taps hit the row you aimed at. *(Narrower than it was: the harness now covers the decode — a contact becomes a tap, a drag becomes a drag and a fling, and the raw codes map to the right buttons. What is left here is the panel's ACTUAL coordinate range, which the harness invents, and whether the drawn geometry matches the hit test.)* |
| 1f | ~~Vol± reaches the hardware~~ **PASS 2026-08-26** | One audible press. *(The ramp curve, its stop-on-release and its stuck-key dead-man are covered off-device; that the mixer write is audible is not.)* |
| 1g | ~~Transport buttons~~ **PASS 2026-08-26** | Each does what it says |
| 1h | ~~Idle screen-off, and **it wakes**~~ **PASS 2026-08-26** | Blank it; wake by touch **and** by Power. A failed wake is indistinguishable from a dead device |

---

## Phase 2 — what this session changed (none of it has touched hardware)

Everything in this phase was written against the harness or by reading call sites. The harness
proves the app *does the thing*; it cannot prove the thing is the right thing to do to the hardware.

### 2A. Bluetooth (PR #3, merged — reasoned from call-site audits, all device-unverified)

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 2A.1 | **Auto-reconnect to a WH-1000XM4 after a reboot** | Reboot with the headphones off; then power them on | They connect without opening Settings ▸ Bluetooth | The paired table and the notification listener are now read/registered at boot; check the log for `bt-paired: N device(s)` and `bt-scan: AddListener rc=0` |
| 2A.2 | **NFC tap pairs and connects** | Tap the headphones to the NFC pad, cold | Pairs *and* links, with no Settings visit | The arm block is now wall-clock paced (10 tries, 2 s apart). Check it armed at all |
| 2A.3 | **The Bluetooth switch matches the radio** | Toggle it; also boot with the radio left on by stock | The switch and the radio never disagree | The reconcile needs three consecutive `GetBtStatus == 7` reads to decide "off" — see `src/bt_switch.h` |
| 2A.4 | **The Bluetooth screen names the connected device** | With a peer linked and playing | The name, not "No device connected" | The address is the signal, not the return value |
| 2A.5 | **Enhanced Mode / absolute volume** | Change volume from the headphones | The UI level follows | `bt_apply_enhanced_mode("boot")` now runs at boot |
| 2A.6 | **A Devices tap while the radio is already paging** — new 2026-09-15 — **PASS once 2026-09-15** (dev build flashed that day) | Bluetooth ▸ Devices: tap the headphones' row while a reconnect is under way (headphones off, tap, then switch them on) | The tap logs `bt-paired: row N: a connect is already on the air — asking for this device the moment it ends`; the spinner stays on the row and more taps send nothing; the link follows within seconds. Seen: one tap at 65.68 s held behind the reconnect's page, linked at 70.61 s | Before the fix, 23 of 24 taps were refused `RequestConnection rc=0` and dropped. A `bt-reconnect: retry … rc=0` about a second before the link is the link already forming, not a failure |

### 2B. Sound (PR #3, merged)

| # | Item | Do | PASS |
|---|---|---|---|
| 2B.1 | ~~**The DSP reconcile with NO settings file**~~ **PASS 2026-08-26 (machine half)** | Settings file moved aside, booted: `apply EQ` 9.014 s and `apply sound chain` 9.225 s both ran from defaults, library loaded 3438/337/198. **The audible half needs ears** and was not tested |
| 2B.2 | **Source Direct really bypasses everything** | Turn it on with a big EQ curve set | The EQ stops being audible, and the UI warns it is bypassed |
| 2B.3 | **DSEE AI — the open question** | A/B by ear on a lossy file | *Unknown.* It is labelled UNVERIFIED, not removed, because nobody has measured it inert — unlike high gain, which was |
| 2B.4 | Tone Control vs the 10-band EQ | Switch between them | Exactly one is audible; `isproc is 1` for the selected one |

### 2C. Bluetooth battery work (PR #4 — the numbers are measured off-device, the milliamps are not)

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 2C.1 | **Pause-on-disconnect is still instant** | Walk out of range / power the headphones off mid-track | Playback pauses within a second | **This is the thing the polling work was not allowed to break.** The relaxed intervals are gated on `g_bt_listener_on`; if `AddListener` failed, every interval returns to its old value. Check the log for the registration |
| 2C.2 | **Track changes still show up** | Let an album play through | Now Playing follows every boundary | The URI round trip is now gated on a duration change or a backwards position jump, with a 30 s backstop |
| 2C.3 | **The codec readout is still right** | Connect LDAC, then something SBC-only | The negotiated codec updates | Polled at 60 s now, with event bypasses |
| 2C.4 | **`tools/btpower.sh` before/after** | `start bt` → unplug → play with the screen off → `report bt`. Same window length, three times, against `jack` and `idle` | A number | **No battery saving is claimed yet.** The work reduction is arithmetic; nobody has measured the milliamps |

### 2D. The seven "work on a timer that never stops" fixes (all found and fixed off-device today)

These are the ones with the least hardware exposure and the most reasoning behind them. See
[`AUDIT_2026-08-24_stalled_bringup.md`](AUDIT_2026-08-24_stalled_bringup.md).

| # | Item | Do | PASS | Notes |
|---|---|---|---|---|
| 2D.1 | ~~**A stalled bring-up is survivable**~~ **PASS 2026-08-26 (machine half)** | Ran: DB renamed away, cable escape borrowed so adb stayed up | First paint 3.455 s, the 25 s guard around the DB open unwinds cleanly, process alive and painting 11+ min, RSS 28028 -> 26532 KB. **Still needs a finger:** nobody touched the panel while the DB was missing, so the input arm remains reasoned, not observed |
| 2D.2 | ~~**…and it stays quiet**~~ **FAILED then FIXED + re-measured 2026-08-26** | It did not: **61,086 bytes / 10 min**. Two throttles one layer apart and only the top one existed — the shell's `retry_log` line printed once, but `cinder-ffi`'s own db-open error printed unthrottled inside the 1 Hz retry. Latched it, and gave the retry a 1 s -> 30 s back-off. Now **1,082 bytes / 10 min (56x less)**, 1 db-open line, 2 retry lines. `stalled-bringup` in the harness asserted the old 1 Hz and was updated |
| 2D.3 | **Log volume in normal use** | Play for an hour, screen on | The log grows by tens of lines, not tens of thousands | Every line is an `fflush` to vfat |
| 2D.4 | **Auto power-off fires, and its guards hold** | Set it to 5 min. Test idle (fires), playing (does not), on a charger (does not) | Exactly that | The five-minute back-off only matters when the helper *fails*, which should not happen on a healthy install |
| 2D.5 | **The `cinder-power` helper still works** | Power off from the UI | It powers off | If the log says "helper missing or setuid bit lost", the install lost a `chmod 4755` — that is the real bug, not the back-off |
| 2D.6 | **USB-MSC round trip** | Cable in, copy a file, cable out | The drive appears with a medium; `/contents` remounts; the app carries on | If the LUN comes up empty, the ladder now backs off ten seconds instead of blocking the render thread — the UI should stay responsive while it is wedged |
| 2D.7 | **Headphone unplug still pauses** | Pull the jack mid-track | Pauses within a second | Measured 496 ms off-device |

---

## Phase 3 — the older batch, still unverified

From `ROADMAP.md`'s P0 table. Thirty-three commits deep at the time it was written; nothing since has
changed their status.

> **3a, 3c, 3e and 3f no longer need a finger on the glass.** They are all visible in the
> position/URI the listener already reports, so `cinder-probe --transport <trackA> <trackB>` settles
> all four by measurement — built and pushed 2026-08-25 but **not yet run** (the device left the
> bus first). Run it before doing any of these by hand.

| # | Item | PASS |
|---|---|---|
| 3a | ~~**Play-by-index**~~ **PASS 2026-08-26** | `play_tracks({A,B}, start=1)` played B (`pos=1000/318333`). The primitive under tap-a-row is correct. |
| 3b | ~~**Playlists** — a playlist row plays the whole list in saved order, plain and shuffled~~ **PASS 2026-08-26** | Both bands work; PLAY is not shuffle |
| 3c | ~~**Drag-to-seek** — `media_origin_t::Begin == 0`~~ **PASS 2026-08-26** | `before=1000 after=63000/318333`, twice. Origin 0 is BEGIN (absolute). **Note:** the RAW `cinder_audio_seek_ms_origin()` is a no-op while streaming (`SeekTime(): Bad parameter. ignored`) — the shipping `cinder_audio_seek_ms()` pauses first and was always fine; the probe did not, which is what made the first two runs read INCONCLUSIVE. |
| 3d | ~~**`duration_raw` is milliseconds**~~ **CONFIRMED 2026-08-25** | `268333` for a 4:28 track. It is milliseconds. |
| 3e | ~~**Repeat-one**~~ **FIXED + PASS 2026-08-26** | **`OneTrackMode::On` is 2, not 1.** 0 and 1 both stop at the end; 2 wraps (`318333/318333 -> 1000`, still playing, same URI). Repeat-one had never worked — right moment, wrong value, and `SetOneTrackMode` is `void` so nothing complained. Swept with `cinder-probe --repeatsweep`. |
| 3f | ~~**Repeat-all** — what the play state does when a queue runs out~~ **OBSERVED 2026-08-26** | At the boundary position pins at duration and `playing` goes 1 -> 0; URI unchanged, no reset. `playing == 0 && pos >= tot` is the repeat-all trigger. Do **not** use `state` (128 -> 1 here, but 1 also appears mid-track). |
| 3g | **Backlight / brightness** — ~~five levels~~ **PASS**; **BACKLIGHT OFF was broken — FIXED 2026-08-26** | Levels 1–5 and reboot-persistence confirmed on device. The 6th stop did nothing: `cinder_get_brightness()` ended `.clamp(1, 5)`, so the UI's level 0 reached the shell as 1 and the shell's `if (lvl == 0)` fully-off branch was dead code. Now clamps 0..5. Safe because 0 is never persisted (`brightness_restore`) and Hold/Power restores it. **Re-test the OFF stop.** |
| 3h | ~~**The 07-26 → 07-28 batch** — escape ladder, screenshot, the pager, accents, A–Z rail, the render optimisation~~ **PASS 2026-08-26** | One clean dev boot and an eyeball |
| 3i | **GPU/EGL present path** — dev channel only, opt-in, **measured slower** | Only if you intend to re-test it |

---

## Phase 4 — measurements nobody has taken

| # | Item | Why it matters |
|---|---|---|
| 4a | ~~**The codec question**~~ **ANSWERED 2026-08-26 — hypothesis does NOT hold** | Measured on a live LDAC link, playing. The driver already powers the headphone stage down for Bluetooth (`PHV_L/R` `0xE4`->`0x00`, `HPOUT2_CTRL1` `0x0F`->`0x00`), so `bt` is strictly BELOW `jack`, not equal to it. What stays on (`OSC`, `BLK_ON0 0x0F`, `SD_ENABLE 0x05`, `DNC1_START 0x50`) is on in **idle** too — an idle cost, not a Bluetooth one — and the no-jack idle column (taken with the jack EMPTY) shows the headphone amp biased with nothing plugged in at all, so Bluetooth is the CHEAPEST state at the codec. Connected and playing are identical at the codec. No register written. See [`BATTERY_BT.md`](BATTERY_BT.md); the power figure is still unmeasured |
| 4b | **A soak.** Nothing has ever run for hours — **first data points 2026-08-25, both good** | Memory growth, log growth within one long boot, and the art cache's first build across 304 albums are all unmeasured. Measured so far, over one ~42 min boot: RSS **39996 → 40008 KB** in 737 s (VSZ flat at 144488) and the log **did not grow at all** over 261 s idle with the screen off (10580 B at both samples). Nothing like a leak, but this is tens of minutes, not hours, and mostly idle |
| 4c | **Boot time and battery life against stock** — **boot half MEASURED 2026-08-26; stock comparison and drain still open** | This is goal #1's entire claim, and it has never been measured. The battery half now runs itself: `tools/battery_track.sh start` leaves an on-device sampler logging one line a minute to `/data/cinder/battery_track.tsv` while the player is USED normally, and `tools/battery_track.sh report` splits it into %/h per state (idle/playing x screen on/off), excluding charging. Leave it a day or two, then read it. **Boot time, Cinder half: 13.2 s from kernel boot to first pixel** — two boots agreeing (13.31 s and 13.16 s), from `/proc/<pid>/stat` starttime against `render_driver: first frame painted`. That EXCLUDES the preloader, so it is not a stopwatch number, and **the comparison against stock needs a human** because stock has no adb. The battery half has 123 samples but all of them on the cable — no discharging intervals yet. |
| 4d | **`dacdat` volume tables** — do this one deliberately | [`DEVICE_TESTS.md` §5](DEVICE_TESTS.md) |
| 4e | **Volume-change POP below volume 100** | [`DEVICE_TESTS.md` §12](DEVICE_TESTS.md) |

---

## Phase 5 — ear tests

These need ears, not instruments. All of them live in [`DEVICE_TESTS.md`](DEVICE_TESTS.md) with full
procedures: the EQ signal path (§1), Tone Control (§2), VPT / DC Phase / DSEE HX Custom / Vinyl
character labels (§3), Walkman One's sound signature with headphones off (§4), Sony's saved setups
(§9), the NW-WM1A volume curve (§11).

---

## What the harness cannot tell us, and therefore what this list cannot skip

The off-device harness boots the real `main.cpp` against fakes. It is worth being explicit about the
shape of its blind spots, because "the harness passes" has already been mistaken for "this works".

* **The ABI.** The fakes answer the way `analysis/` says the services answer. Where those notes are
  wrong, the harness is confidently wrong with them. Phase 0f exists for this.
* **The ARM link, the GLIBC 2.23 ceiling, the libc++ ABI.** Nothing off-device compiles for the
  target. That is what item 0.1 is for.
* **Whether a write did anything.** `system()` and `popen()` are recording stubs there, so every
  setuid helper "fails". The failure paths are therefore well tested and the success paths are not
  tested at all — 2D.5 and 2D.6 exist because of this.
* **`alarm()` and the guard budgets.** The virtual clock covers sleeping, not signals, so
  `run_guarded`'s timeouts and the construction watchdog are measured in real seconds and cannot be
  exercised cheaply.
* **UI input — partly closed, 2026-08-24.** The harness now presents `/dev/input/event*` as real
  FIFOs, so the path from a raw evdev code to `cinder_input`/`cinder_tap` is covered. Two things
  are still device-only: the panel's **actual** coordinate range (the harness reports 0..480/0..800
  so its numbers are the navigator's numbers, which is convenient and untrue), and what the
  navigator then *does* — that is Rust, stubbed here, and covered instead by `cinder-ui`'s 404
  tests.
* **`dlopen`ed services** — NFC, the display service, the USB manager. Off-device they take their
  degraded branch, so 2A.2 in particular has had no automated exercise at all.
* **`cinder-audio`'s shims** — 2,500 lines that are the entire IPC surface to PlayerService,
  EffectCtrlDmp, the tuner and the power manager. They sit behind the harness's stub boundary and
  have **no tests of any kind** (`SHORTCOMINGS.md` §A2).
* **Audio.** Obviously. Nothing off-device can hear anything.

---

## Open from 2026-09-07/08 — measured code shipped, verification outstanding

These are all device-gated and all have the code already installed. None of them is a guess: each
names the exact evidence that would settle it.

| # | Check | How | What decides it |
|---|---|---|---|
| 5.1 | **Volume limit actually caps** | Settings ▸ Volume limit → `SAFE LEVEL`, then hold volume up | Stops at **63/120**, not 120; `OFF` gives the headroom straight back. AVLS was measured to enforce (`--avls enforce`: asked 91, got 63) but it enforces inside `VolumeAdlerOut::SetVolume`, which Cinder never calls — so this verifies Cinder's OWN clamp in `apply_volume`, not Sony's |
| 5.2 | **The BT fine-volume stutter is gone** | Bluetooth ▸ Audio quality ▸ Fine volume on, then press volume | No click per press. The cause was `bt_trim_tick` calling `fx_cache_drop()`, which forced the cold path and re-asserted `SetEq10Band(true)` — re-instantiating the DSP filter mid-stream |
| 5.3 | **Bluetooth reads `MIN`, not `MUTE`** | Turn the BT volume to the bottom | `MIN`. AVRCP 0 is the sink's floor, and no firmware lever can mute an A2DP sink — Sony's own `VolumeA2dpOut::SetVolume` is a stub (`analysis/RE_volume_service.md`) |
| 5.4 | **DSEE HX / AI is not another high gain** | Play a **lossy** file (MP3/AAC — on lossless there is nothing above the cutoff to restore), then `/tmp/pv --dseemeter` | Top three bands move on, bottom three do not, and the third pass returns. A null result is the same evidence high gain was cut on — measure before claiming it works. Supersedes 2B.3 |
| 5.5 | **A battery number, at last** | `tools/battery_track.sh start 60`, then **unplug** and use it for hours; `tools/battery_track.sh report` | Any discharge interval at all. Every sample ever recorded is `Charging`/`Full` on the cable, which is why 2C.4 still says no saving is claimed |

Repository decision, not a device test: **the history rewrite** in
[`HISTORY_REWRITE.md`](HISTORY_REWRITE.md). It takes Sony's files and the superseded `dist/` build
output out of every commit, has been rehearsed on a scratch mirror, and force-pushes, which cannot
be undone — so it waits for the owner. Sony's files can move to a repository of their own first
(rehearsed; commands in the same document). Until then `.githooks/pre-push` still warns when a
non-release push carries `dist/stable`.

---

## Open from 2026-09-11 — the power-key escape, the band and the release

11.5, 11.6 (the Bluetooth queue) and 11.12 (palettes) **passed on 2026-09-11**, and the shuffle band
passed with one change asked for, which is 11.7 now. 11.1 is answered — the kernel does log a press
made during boot, through `kpd:` lines rather than the ones it named. **11.2 passed at 23:40 on the
fixed launcher**, after the first version was found unable to fire at all. Results:
[`DEVICE_TESTS.md`](DEVICE_TESTS.md) "RESULTS 2026-09-11 (evening)".

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 11.3 | **A quiet boot never goes to stock** | Five boots, power pressed only to turn it on — held briefly, then held for ~3 s | Cinder every time | A false positive is the exact trap the cable escape set. `dmesg \| grep -a "Power Key generate"` for a stray `pressed=1` at ≥ 2 s. The ~0.43 s `pressed=1` is the PMIC's boot-time reading and never counts |
| 11.4 | **Only after 11.2 and 11.3 pass:** decide the cable escape | Keep, narrow to a PC connection, or remove | — | The cable escape stays exactly as it is until then |
| 11.7 | **NEW PLAYLIST slides away with the band** | Library ▸ Playlists, with enough playlists to scroll: scroll down, then up mid-list | NEW PLAYLIST goes up under the tabs with the band, the rows meet the tab strip, and both come back on the way up — tappable the moment it is back | A tap on its old spot while hidden must open a playlist or nothing, never the keyboard |
| 11.8 | **Power key after a guard recovery** | Hard to force. If `GUARD RECOVERED` ever appears again: press power, touch the screen | The UI stays responsive (screen may stay lit); log carries the fault record after a forced restart | A frozen UI = a fifth unguarded Sony call; the fault record now survives to say which |
| 11.9 | **The release installer, end to end** — what a stranger will run. **PASS 2026-10-01, owner-reported: "all of the installers are working."** RE-OPENED 2026-09-12: the Windows handoff was replaced that day (it no longer launches Sony's updater; it sends the vendor SCSI command itself and asks for administrator), so a pass recorded before that date does not cover the path a stranger now runs | From the GitHub release, on Windows: `cinder-installer-windows-x64.exe` → Install, **cable left in** → boots into Cinder → restart with the cable still in → Sony's player (the post-install pass is spent) → Update, cable in → boots into Cinder → Uninstall → Install again | Each step lands; the window names the state the player is really in; the first boot after each install or update logs `cinderhome-launch: cable escape stood down for this boot` and the restart after it does not; music, playlists and settings survive all four | `cinder_home_install.log` in the drive root says which step. A revert by the device's sanity gate must read as reverted, not as a tick |
| 11.10 | ~~**USB-DAC → LDAC, the whole path** — the headline, never run end to end~~ **PASS 2026-09-12, owner-reported; LOG CAPTURED 2026-09-15 (v0.3.7 dev build)**: headphones linked on LDAC (`bt-sound: codec:0x02`), `cinder-msc: dac uac OK`, the host streamed 44.1 kHz / 32-bit, `ldac: handshake accepted`, capture `S32_LE` stereo RUNNING, `FIRST CAPTURE READ ok`, then `session ended after 8497664 frames (192 s at 44100 Hz)` when USB-DAC was switched off: 192.7 s carried against 193.1 s streamed, with no stall, recovery or reconnect line. `usb-dac: reclaimed the player after DAC (init rc=0)`. Still unlogged: the 3.5 mm leg with no Bluetooth link | Headphones on LDAC; Menu ▸ USB-DAC on; play audio on the PC into the Walkman | Sound in the headphones; the Bluetooth screen names LDAC | The transmit half is proven (STATUS.md, 2026-08-11), so silence points at capture: log for `snd_pcm_open` / `-EBUSY` on the UAC card |
| 11.11 | **Which model is this unit?** | Read the label on the back of the player | Record it in README ▸ Supported devices | `/contents` reads 55 GB — the 64 GB NW-A57, not the 16 GB NW-A55 the README used to name. The README now says "64 GB"; correct it if the label disagrees |
| 11.12 | **Palettes** | Copy `player/cinder-ui/palettes/slate.palette` and `paper.palette` into `cinder_palettes/` on the player's storage over USB; unplug; Settings ▸ Palette | Both appear and each repaints the UI; Paper's Accent row reads SET BY PALETTE; the choice survives a reboot; a deliberately broken file is skipped and `cinderhome.log` says why | Nothing listed: the log line `palettes: cannot read` names the error. Folder read at boot but not after USB: check Settings was re-opened, which is the rescan trigger |

---

## Open from 2026-09-13 — the community round: battery, exFAT cards, scrobbles

Came out of the first public thread (r/walkman) and the owner's own report. The scrobbler stand-down
and the new storage log lines **passed on 2026-09-13** on the reference device; the rest needs a
state the desk cannot produce. Detail: CHANGELOG `[Unreleased]`.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 13.1 | **The player powers itself off on a flat battery** — never happened before this build: the guard read `status` and the driver says `Not charging` on battery | Run it down off the cable, screen off or on | At ≤10 % a "Battery N% — charge soon" toast; at ≤3 % "Battery critical — shutting down" and the player turns off. Next boot's `bootreason` is `power_key` | Stays on at 0 %: `grep -a "power:" /contents/cinderhome.log.1`. No line at all means the guard never decided — read `usb/online` and `dc/online` on battery |
| 13.2 | **An exFAT card survives USB mode** (any card over 32 GB, or one the player formatted) | Card in; Settings ▸ USB mode; copy a file; leave USB mode | `cinder-msc: exFAT SD card remounted at /contents_ext`, then `storage: SD card mounted at /contents_ext (fuse.exfatfuse)`; the card's albums still play; Settings ▸ Storage shows the card | `exFAT SD card did not remount (mount.exfat rc=…)` names the failure. The automatic re-scan must say it was SKIPPED — a scan with the card missing is the data-loss case |
| 13.3 | **A FAT32 card still survives USB mode** (regression check — `cinder-msc` changed) | The same round trip on the reference card | `storage: SD card mounted at /contents_ext (vfat)`, and the usual post-transfer re-scan runs | `mount /contents_ext failed errno=…` from the vfat path |
| 13.4 | **Settings ▸ Storage shows the card** | Open Settings with the card in, then with it out | `… GB, SD N / M GB` with it in; internal only with it out. A card in the slot but unmounted reads `SD not mounted` | Row unchanged after a state change: look for the `storage:` line that should have refreshed it |
| 13.5 | **The reporter's library appears** (1 TB, formatted by the player; music on stock, `library loaded — 0 tracks` in Cinder while the card was mounted and readable) | Their `cinderhome.log` from a boot with this build | `library by storage — … SD N` with N > 0, and their albums on screen. At most a `[cinder-db] a track row would not read and was skipped` line, never an empty library | `track query FAILED — <reason>` names the column. `SD 0` with the card mounted and no failure line → the store really lacks its rows: Settings ▸ Database |
| 13.6 | **Does Sony's `_cew` region table change the level at the jack?** The 2026-09-04 "no EU cap" result swept only the analogue attenuator, and the two tables are identical there — they differ in the digital stages (`analysis/RE_volume_tables.md`, amended 2026-09-13). Wampy measured CEW2/KR3 units quieter | **Nothing on anyone's head.** Jack → PC line input, the tone file and settings from `RE_headphone_amp_modes.md` §2, `master volume` 60. `cinder-voltable eu`, step the volume 59 → 60 so the table is written, record; the same with `cinder-voltable stock`; reboot to restore | Levels at 1 kHz that differ by more than the rig's 0.02 dB repeatability, `eu` lower. The difference in dB is the number the RE doc is missing | Identical levels: on this output path the two digital stages cancel. Record that — Wampy's result then needs a CEW2/KR3 unit, not this one |
| 13.7 | **Lyrics show and follow the song** — new; embedded tags and `.lrc` | Play *Amy Winehouse – Love Is A Losing Game* (synced `LYRICS` tag) and *The Smiths – Is It Really So Strange* (plain CRLF tag); tap the title block on Now Playing ▸ Lyrics. Then put a timestamped `Song.lrc` beside a song with the same name and play it | Synced: a `Lyrics · Synced, N lines` row; the page highlights the sung line and keeps it in view; a drag stops the following. Plain: `Lyrics · N lines`, verse breaks kept, no highlight. A `.lrc` replaces the tag's lyrics. A song with neither has no Lyrics row | No row on a tagged song: the key must be `LYRICS`/`UNSYNCEDLYRICS` (FLAC). No row for a `.lrc`: the names must match apart from the extension (`.lrc` / `.LRC`). Garbled words: note the file's encoding |
| 13.9 | **The WM1A volume curve from a table the user brings** — new; the reference device had it copied in over adb, not by an install. **PARTIAL 2026-09-14** (reference device, Wampy installed): the install step, run as root, refused a junk `/contents/ov_127x.tbl` and installed all three tables from Wampy's copies with matching hashes; `cinder-voltable wm1a` then applied from Cinder's directory and `--volcurve` read vol 40 → 0x54 (84), vol 60 → 0x7C (124), monotonic. **The boot check failed first time**: `mkdir -p` under root's umask 077 made both directories 0700, so the uid-100 launcher logged `needs Sony's ov_127x.tbl` with the file present — fixed (explicit `chmod 755`); the next boot logged `volume curve: wm1a applied` at 25 s. (a) and (c) pass by the install step run over adb; still to do: the same through a real install, and (b), a player without Wampy. Also: a sweep while the codec is in standby prints `?` for every step; the probe now wakes it | (a) Wampy installed, nothing on the drive: install with `wm1a`. (b) A player without Wampy: put `ov_127x.tbl` and `ov_dsd_127x.tbl` from the cinder-sony-analysis release at the top of the drive, install with `wm1a`. (c) Any other file renamed `ov_127x.tbl` on the drive | Install log `volume table: installed ov_127x.tbl from … (SHA-256 matches Sony's)`; boot log `volume curve: wm1a applied`; `cinder-probe --volcurve` monotonic (vol 40 → PHV 84, vol 60 → 124). (c): `WARN: /contents/ov_127x.tbl is not Sony's ov_127x.tbl` and the stock curve | Boot log `needs Sony's ov_127x.tbl`: nothing reached `/system/vendor/unknown321/usr/share/cinder/audio_dac` — read the install log's `volume table:` and `WARN:` lines. A browser may have renamed the download (`ov_127x (1).tbl`) |
| 13.8 | **Library search, the install option** — new, off by default | Install with Library search ticked; Library ▸ the magnifier; type part of a title; tap a result. Then Update with it unticked | `deferred_up: library search enabled` in `cinderhome.log`; the keyboard is already up, the count changes on every key, the tap plays that song; after the Update the magnifier is gone | No magnifier with it ticked: the install log's `library search:` line says whether `/data/cinder/search_on` was written |
| 13.10 | **Mono (accessibility) over USB-DAC → LDAC** — new. This is the ONLY path on the device that can sum the channels; `analysis/RE_mono_audio.md` §1–5 is why. The row on Sound says so, so this test is also the test of whether that sentence is true | Sound ▸ the **MONO** button in the Balance row, then engage USB-DAC and connect LDAC headphones. Play something with a hard-panned stereo image (an early Beatles mix is the classic). Then switch MONO off and re-engage USB-DAC | `cinderhome.log` prints `ldac: mono downmix ON` when the bridge session starts, and both ears carry the whole mix. With it off, the same track pans again. The Balance slider is drawn dead while MONO is on, and its readout says BOTH CHANNELS | Nothing changed but the log says ON: the downmix is in `ldac_pump`, so it only applies to frames the BRIDGE carries — check the session actually started (`ldac: FIRST CAPTURE READ ok`). Log says off after toggling it: the flag is read once per session, so re-engage USB-DAC rather than toggling mid-stream |
| 13.12 | **Two readings that decide what a system-wide mono costs** — **DONE 2026-09-16** (libasound is NEEDED with `snd_pcm_writei` via the PLT; SoundServiceFw holds the transmitter socket; see 13.13 for what was built on them). No writes, no risk, five minutes. `analysis/RE_mono_audio.md` §7.7 is the reasoning; these are the only two facts a host cannot establish | (a) `readelf -d /system/vendor/sony/lib/libaudiohal-adleralsa.so \| grep NEEDED` and `readelf -r … \| grep snd_pcm`. (b) With LDAC playing, `ls -l /proc/*/fd 2>/dev/null \| grep -i bttransmitter` to find which `hagoromo` holds the transmitter socket | (a) tells us whether the jack shim can be an `LD_PRELOAD` interposing `snd_pcm_writei` (libasound in NEEDED, relocations present) or has to be a replacement HAL (`dlopen`'d, like cinder-home does). (b) names the process the Bluetooth shim would preload into | `readelf` absent on device: pull the `.so` over adb and read it on the host. No socket found: A2DP may not have been streaming — start playback first, and note that the name is abstract, so it appears with a leading NUL |
| 13.11 | **Mono on the 3.5 mm jack — the one unswept lever.** Superseded for Wampy players by 13.13; still the only jack route that would need no Wampy. NOT a bug to fix: §1–6 of `analysis/RE_mono_audio.md` closes every other cheap route. This item settles §8 by measurement instead of leaving it open. (§7 is the separate, larger answer to "what would a system-wide toggle take" — two LD_PRELOAD shims — and 13.12 is its first step.) | With the bench rig from `analysis/RE_headphone_amp_modes.md` (1 kHz hard-panned tone, capture at the jack), sweep `Audio I2sout Mch Config` through 0..6 with `amixer -c0`, capturing both channels at each. It is an MTK-side mixer control, not a codec register, and a mixer value does not survive a reboot — so this is reversible. Restore `5,6` afterwards | Either one value produces L and R carrying the same summed signal — in which case mono reaches the jack and the Sound row can stop qualifying itself — or none does, and §8 is closed | Output silent or mis-routed at some values: expected, that is what a sweep is for. Reboot restores it. **Do not** extend this to codec registers |
| 13.13 | **Mono on the jack and Bluetooth (`libcinder_mono.so`)** — new, needs Wampy. **PASS 2026-09-16** for the library itself: with the flag set by hand, `jack: pcm … fmt 2 — summing`, then over LDAC `bt: fd 17 handshake: 2 ch, 44100 Hz` → `bt: fd 17 — summing`, the stream stayed up and nothing rebooted. Still to see: the owner hearing it, the same through the Sound ▸ MONO button, and an install from the .UPG | Install with `mono` ticked on a Wampy player. Boot, play a hard-panned track on the jack, toggle Sound ▸ **MONO**; connect LDAC headphones and toggle again. Then reinstall with `mono` unticked | `cinderhome.log`: `mono: libcinder_mono.so is loaded in SoundServiceFw`. `/tmp/cinder_mono.log`: `mono ON`, `jack: … summing`, `bt: fd N — summing`, `mono off`. Unticked: `cinder_home_install.log` says `mono: Wampy's library restored` and Wampy still works | Log empty after playback: check its mode is `-rw-rw-rw-` (a root-owned 0600 log is how the first build hid every line). `SAFE MODE` in the log: two uncleared loads; `rm /data/cinder/mono_shim_boots` and reboot. Anything wrong: `touch /data/cinder/mono_shim_off` (processing off, Wampy still loaded) |
| 13.14 | **Clear Bass, Sony's own (six-band EQ band 0)** — new. Silent reading **PASS 2026-09-16**: under Custom 1, band 0 took +10 and read back 10.0 dB, and the service logged `Eq6band … isproc is 1` next to `Eq10band … isproc is 1` (selector 2). **Measured FAIL 2026-09-17:** no change at the jack under either selector or at −10, with a ten-band control on the same rig reading +7.9 dB (`analysis/RE_clear_bass.md` §4). Blocked on RE of what engages the six-band filter. `analysis/RE_clear_bass.md` | Play a bass-heavy track. `cinder-probe --clearbass 10 20` (selector untouched), then `--clearbass 10 20 1`, then `--clearbass -10 20`. By hand the probe needs `LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib` | +10: deep bass clearly up, mid-bass slightly leaner. −10: bass thinner. Each hold ends with `clearbass: restored selector=…` and the sound returns to what it was | Audible only under selector 1: `Eq6band::UpdateProcCond`'s this+0x178 field is the selector, and a Cinder Clear Bass row would have to choose between it and the 10-band EQ. Nothing audible under either: check `logcat` for `effect param eq6band,band=0` — if absent, the write never reached the service |

## Open from 2026-09-18 — SensMe channels, and the battery gauge

Came out of `dde1ec0` and [`AUDIT_2026-09-18.md`](AUDIT_2026-09-18.md). **Everything here is
device-unverified**: the host side is proven by 599 tests and 48 harness scenarios, and none of that
can see a scanner, a cell or a screen. 14.1–14.3 need a PC with a tagger on it; 14.4–14.6 need
only time on battery, or none at all.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 14.1 | **SensMe channels, end to end** — new, opt-in (`sensme`, off by default). The tag → akey path is already proven twice (`analysis/RE_sensme_musiccenter.md` §9 FLAC, §10 MP3); what has never run is Cinder reading those rows and playing a channel | Install with **SensMe channels** ticked. Tag a dozen tracks with `flint sync --apply` (or Music Center), copy them to `/contents/MUSIC`, unplug, let the scanner run. Menu ▸ **SensMe** | `cinderhome.log`: `deferred_up: sensme channels enabled` and `cinder-ffi: sensme: N analysed tracks in M channels`. The list shows those channels with counts; a channel opens onto its tracks; PLAY plays the channel and the next track is another member, not that track's album | The screen says "No analysed tracks": the scanner found no tag. Check `object_ext_int` akey 50 for one of the objects (`tools/` has the MTPDB pull) — rows present but no channels means the akey names differ on this firmware, which `Db::sensme` logs |
| 14.2 | **Which channel is which** — the id → name table is **inferred** from the order of Sony's own channel thumbnails and nothing else (audit C2) | With the same tracks on **stock** (boot with the cable in), open Sony's SensMe screen and note which channel a known track appears in. Its bitmask is in MTPDB akey 50: bit `id + 1` | The name stock shows matches `cinder_db::SENSME_CHANNELS[id]` for every track checked | Any mismatch: correct that one array — it is the only place in Cinder that knows a channel's name — and say in the commit which track settled it |
| 14.3 | **The interop claim: Music Center and Flint are the same to Cinder** | Tag one copy of a track with Flint and another with Music Center; put both on the player; let the scanner run; pull `MTPDB.dat` | Both objects carry the same akeys 50/52-56/60 and land in the same channels. (Music Center's may also carry 51/58/59 — that is expected and must change nothing) | Different channels for the same audio: compare the two tags with `flint inspect`. A difference in the CHUNKS is the interesting case; a difference in tag SIZE is not |
| 14.4 | **How far this cell's gauge moves without the charge moving** — the owner's 22% → 58% arrived seconds after the cable went in, and the fix (slew-limiting) was reasoned from a charging log, not from either case in the wild (audit B1) | `adb shell 'tail -f /contents/cinderhome.log'`. (a) Off the cable, play loudly with the screen on for a few minutes. (b) Then plug the charger in and watch the same lines | Lines of the form `battery: sysfs says N%, reporting M% (voltage_now … uV, charger in/out) — a jump / load sag on a voltage-derived gauge`. In (a) the raw value falls and recovers when playback stops; in (b) it jumps up the moment the cable lands. The status bar moves by a point at a time throughout both | No such line and the bar still jumps: the divergence never reached 5 points — record the numbers anyway, they calibrate the slew rate |
| 14.5 | **A transient dip cannot power the player off** — the harness pins it (`batt-dip`), the device never has | During 14.4, watch the reported level and keep playing | The player stays on, and the level never steps more than one point per 10 s | It powers off with real charge left: `grep -a "power:" /contents/cinderhome.log`, and note what the raw value was doing at the time |
| 14.6 | **The Battery row stops calling an unplugged charger a fault** — reported 2026-09-18: `FAULT 2` with nothing plugged in (audit B3a) | Settings ▸ Device with the cable out, then with it in | Out: `ON BATTERY`, and the raw register footer unchanged. In: `CHARGING` (or `READY`/`CHARGE DONE`), and a real fault code still shows as `FAULT n`. The log carries `charger reports fault code N with no input attached` once | Still `FAULT n` with the cable out: the charger-detect nodes disagreed with the cable — read `usb/online` and `dc/online` |

## Open from 2026-09-21 — shuffle and the queue, from the 09-06 audit

[`AUDIT_2026-09-06_queue_playback.md`](AUDIT_2026-09-06_queue_playback.md) found and fixed twelve
defects, and its own verification table ends "**nothing here is device-verified**". Three of the
twelve (its §3, §8, §9) are arguments about a `Render` the host cannot construct — read and
reasoned, never measured. This section is that list turned into things to do, so the work stops
living only inside an audit nobody re-reads. None of it needs a flash: any build from v0.3.9 on
carries all twelve fixes.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 15.1 | **Shuffle on a paused player stays silent** (audit §3) — the fix is FFI-side and the host has no `Render` to prove it with | Boot. Do NOT press ▶. Tap the shuffle icon on Now Playing | Nothing is audible, and `cinderhome.log` carries neither `queue: apply now` nor `play_tracks` | Music starts: the not-playing branch is flushing the queue again — `cinder_shuffle_set` in `lib.rs`, and note whether a resume position existed |
| 15.2 | **Repeat-all restarts a queue-edited album at track 1** (audit §6) | Repeat-all on. Play an album, swipe-queue one track from the middle, let it run to the end | The lap starts at **track 1**, and the log says `repeat-all: queue ended — restarting it from the first track` exactly once | It restarts mid-album: the sequence was truncated by the edit, so the lap is re-issuing what was left rather than the album |
| 15.3 | **Repeat-all does not fire on a pause near a track end** (audit §7) | Repeat-all on, mid-album. Pause about a second before a track ends; wait ten seconds | Nothing happens — no lap, no skip. The log has no `repeat-all` line | A lap fires: the end-of-queue test is reading a pause as an ended track (`g_playing` is intent, not state) |
| 15.4 | **A queue edit made before the first ▶ survives** (audit §9) | Boot. Before pressing ▶ at all, swipe-queue a song. Then press ▶ | It plays after the resumed track, and Up Next shows it throughout | It is lost: the edit was owed against a sequence that did not exist yet, and the resume replaced it |
| 15.5 | **What the play state does when a queue simply runs out** — the one thing repeat-all's end detection is built on, and it has never been watched (`cinder-home/ROADMAP.md`) | Repeat-all OFF. Play the last track of a short queue to its end, with `adb shell tail -f /contents/cinderhome.log` running | Record what `cinder_audio_is_playing()` and the position out-parameters do at the boundary: whether the player stops, holds the last frame, or reports the track again | Nothing in the log: raise the playback-poll logging for one session rather than guessing — this item exists to be observed, not to pass |


## Open from 2026-09-21 — installing on a model-swapped player (Walkman One)

> **Owner report, 2026-10-01:** Walkman One is tested and running with Cinder on the owner's current player, and
> every installer works. That answers what this section exists to ask — whether a W1 player can
> take Cinder at all. The rows below stay as the record of how it was checked.

An NW-A55 running Walkman One reports the **NW-WM1A** KAS (`nvpstr kas` →
`e8d171a5…26c`) and `mid: 128G`, so every NW-A50-sealed `.UPG` is refused by Sony's updater without
a word — measured 2026-09-21, full write-up in
[`analysis/RE_walkmanone_extract.md`](../analysis/RE_walkmanone_extract.md). A WM1A-sealed package now
builds (`cinder-home/tools/pack_upg.sh dev nw-wm1a`) and round-trips off-device. Nothing below has
touched hardware.

**This needs a flash**, and the player already runs a modified firmware — take a wbrt backup before
16.2 and do not run it on a player you cannot restore.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 16.1 | ~~**The key really is what decides it**~~ **SETTLED 2026-09-21 (second session)** | — | The KAS names the key, not a family: on Walkman One `kas` = `nw-wm1a` while `fpi` = `NW-WM1Z`. **Both are rewritten, to different models**, and `fpi` is what Sony's own packages check. **All three of W1's NVP images (`conf_a/b/c`) carry the same KAS**, so one `nw-wm1a` package covers every Walkman One A50 install | — |
| 16.2 | **A WM1A-sealed package installs on a Walkman One player** | Stage the binaries, then flash `cinder-home/dist/dev/cinder_home_install.nw-wm1a.upg`. Cable in through the reboot | `/contents/cinder_home_install.log` **and** `/contents/cinderhome.log` both exist afterwards, `/system/vendor/unknown321/bin` is populated, and the player boots Cinder | Still stock and still no `cinderhome.log`: the key is not the only gate — capture `.appcfg`, the uptime, and whether the updater screen appeared at all |
| 16.4 | **Walkman One's settings processor still runs with Cinder as the Home app** | After 16.2: `adb shell tail -40 /contents/CFW/boot_log.txt` | A new boot entry appears, the eight settings are read, and the mode/gain/DAC lines say "initialized" — **not** "The [WM1Z] tuning was not applied" | If the log does not advance at all, `/sbin/boot_complete.sh` is not running and every audio claim in `docs/PLAN_walkman_one_parity.md` §3 is void. If it advances but falls back to "Normal (no tuning)", the `TMD5`/NVRAM guard tripped — something wrote `mmcblk0p3` |
| 16.5 | **Clear Bass, with the stock UI as the working reference** | Cable OUT. Set Clear Bass from Walkman One's own Sound Settings, then compare the service log and effect state against what `cinder-probe --clearbass` writes | The difference names what `Eq6band::UpdateProcCond` is waiting for — the `no desired value, skip` branch | Falls back to recreating the curve on Cinder's ten-band, which already measures +7.9 dB (`analysis/RE_clear_bass.md` §4, `docs/PLAN_walkman_one_parity.md` §4) |
| 16.3 | **Cinder on top of Walkman One actually works**, not just installs | After 16.2: play a track, check the volume curve line in `cinderhome.log`, and toggle the sound signature | Audio plays and the log names the curve it loaded; W1's own DAC tables are already resident, so a mismatch shows up as a curve load failure, not silence | Note which component failed — W1 ships a different `/system/usr/share/audio_dac/` set, so `voltable` and `signature` are the two expected to disagree |

> **Update 2026-09-21 (second session).** The package is now **staged and verified on the player**:
> `/contents/NW_WM_FW.UPG` is byte-identical to `cinder-home/dist/dev/cinder_home_install.nw-wm1a.upg`
> (md5 `b6674bff…`), the staged `cinder-home` matches `dist/dev/cinder-home`, and the launcher ships
> inside the payload rather than beside it. `/contents/cinderhome.log` and
> `/system/vendor/unknown321/bin` are both still absent, so nothing has run. **16.2 is prepared and
> unrun.** Walkman One keeps `/opt2/stock/conf_bk` (the player's original NVP) and `nv_bk` (its
> original NVRAM) — that restores *identity*, not partitions, and is not a substitute for a wbrt
> backup.

### Not in scope of the above

The installer does **not** read the player's key and cannot warn about a mismatch; it reports success
because the staging half did succeed. Doing it over MSC means reading NVP off the raw device rather
than over adb. Logged here so the gap is not rediscovered as a bug.


## Recording results

Append findings to [`DEVICE_TESTS.md`](DEVICE_TESTS.md) in the style of its "RESULTS 2026-08-17"
section — what was run, what happened, and what it settles — and update the status column of
whatever table above the item came from. An item that passes should stop being on this list; an item
that fails should gain a root cause, not just a retry.

---

## How a pass gets recorded here

Two kinds of PASS appear above, and the difference matters more as more of them accumulate.

**Log-backed.** A date and a captured artefact: a `cinderhome.log` excerpt, a `cinder-probe` dump,
a `dmesg` line, a measurement. Everything struck through before 2026-09-11 is this kind, and each
one names what was seen.

**Owner-reported.** The owner ran it and it worked, with nothing captured. `11.10` on 2026-09-12 is
this kind. It is recorded because the person who ran it is the one who built it and there is no
reason to doubt it — and it is labelled, because a feature this project has described for months as
"0% validated" should not silently become "verified" on a sentence, and because the *detail* a log
would have carried is what the next debugging session needs: which LDAC quality was negotiated,
whether the capture opened first time, what the Bluetooth screen named.

If you have the player and five minutes, re-run 11.10 with `adb shell 'tail -f /contents/cinderhome.log'`
running and paste the excerpt into `cinder-home/STATUS.md`. That upgrades the row and costs nothing.


## 17 — After the 2026-09-21 Walkman One boot loop

> ## RESOLVED — 2026-09-21, later the same day. Cinder runs on Walkman One 3.02.
>
> `ps` on a W1 boot: `system 830 495 /system/vendor/unknown321/bin/cinder-home`. The full easel
> handshake completes (`ToInitialize → ToPostInitialize → ToActivate → OnForeground`),
> `/data/cinder/bootcount` reads `0` — cinder-home's own "painted and proved healthy" signal — the
> cable pass is spent, and 77 KB of `cinderhome.log` has no errors in it.
>
> **The cause was ours, not Walkman One's.** `install_cinderhome.sh` created `/data/cinder` with
> `mkdir -p` as root under `umask 077` → **`0700 root:root`**. The launcher and cinder-home run as
> **uid 100**, which on this player *is* the user `system` (`/etc/passwd`: `system:…:100:100:`),
> because appmgr's service line `hagoromo2` is `user system` and the Home app inherits it. Deleting a
> file needs write permission on the **directory**, so the launcher could not spend
> `cable_pass_once` — `CABLE_PASS_SPENT` stayed `0` and **the rung-0 cable escape fired on every
> boot**, `exec`ing Sony's player. It could not write `bootcount` or its breadcrumb either, and
> `/contents` is not mounted that early, so the other breadcrumb path failed too.
>
> **Why that read as a boot loop, and then as "the launcher never ran".** The resulting state — Sony's
> app up, no `bootcount`, no breadcrumb, an unspent cable pass — is indistinguishable from *appmgr
> never exec'd the launcher*. That conclusion was drawn twice before the permission bits settled it.
> **A launcher that ran and took an escape looks exactly like one that never ran, unless it can write
> somewhere.** A probe script that logs only to `/data/cinder` or `/contents` proves nothing at boot;
> use `/var/log` (init makes it `0777`) or `/tmp`.
>
> **Two hypotheses tested and killed** — record them so they are not retried: the ABI theory (dead, see
> below), and `.appcfg.real` sitting inside appmgr's `readdir_r` scan directory
> `/system/vendor/sony/bin` declaring the same `name:`. The backup was moved to
> `/system/vendor/unknown321/` and the boot retried: **no change**. appmgr does honour an absolute
> `command:`.
>
> **Correction to the rule recorded at the bottom of this section.** On this session's live install
> `data_is_mounted()` did **not** false-negative — the log read `state: /data (/emmc@usrdata) mounted`
> and the cable pass was written normally. What *did* bite is the installer's tail: `umount /data`
> succeeded on the live system, leaving `/data` unmounted on a running player until
> `/system/bin/mount_partition usrdata` put it back. That is the live-system hazard to fix, not the
> sentinel.
>
> **What made the diagnosis possible**, and what to keep using: `ADB=1` in
> `/contents/CFW/settings.txt` (persists adb across W1 boots), `icx_syslog` started by hand
> (`setprop ctl.start icx_syslog` — W1 never starts it), and
> [`cinder-home/deploy/cinder-guard.sh`](../cinder-home/deploy/cinder-guard.sh), which sits below
> appmgr and made each attempt survivable. Mechanism and measurements:
> [`analysis/RE_walkmanone_extract.md`](../analysis/RE_walkmanone_extract.md), third session.


**What happened.** Cinder was installed onto a player running Walkman One 3.02 by running
`install_cinderhome.sh` directly as root over adb (the Sony updater refused the package — see
below). The install itself was clean: `.appcfg` repointed, all nine binaries placed with their
setuid bits, `.appcfg.real` backed up, launcher written. On reboot **the player boot-looped, and
the loop survived a cable-in boot** — rung 0 of the escape ladder, which needs no filesystem.
Recovered with wbrt.

**A loop that outlives rung 0 is happening before or beneath the launcher.** The first hypothesis — that
`cinder-home` is built against stock 1.02 libraries and fails appmgr's handshake on 3.02 — **was
tested off-device the same evening and is dead**: all 13 Sony libraries Cinder links against export
identical symbols, and `libeaselcore`/`libeaselcui`/`libpstcore`/`libappmgrservice` are
byte-identical between the two firmwares. **The cause is currently unknown**; see
[`VISION_four_builds.md`](VISION_four_builds.md) §2. 17.1 is now the step that matters.

**Two separate findings from the same attempt, worth keeping:**

* The Sony updater **did** run and rejected the package explicitly. `nvp zr 26 4` (NVP node 26,
  *FWUP result*) read `45 55 50 47` = `EUPG` = `E_UPGFILE`, which the recovery ramdisk's
  `install_update_script/icx_start_update.sh` sets when **`fwpchk -f … -c` (md5 & signature)
  fails**.

  > **CORRECTED 2026-09-22.** This bullet used to read "a correctly `nw-wm1a`-sealed package is
  > still refused… necessary and not sufficient". **That was not measured.** The package refused
  > on 09-21 was the **`nw-a50`** one — `pack_upg.sh` only learned to take a model later the same
  > day, and `CHANGELOG.md` records the nw-wm1a package as "built and round-trip-checked, and has
  > not yet been installed on hardware". Whether ANY `.UPG` installs on Walkman One is therefore
  > **open**, not closed, and 18.1 below is the test that settles it. The `fwpchk`-key note stays
  > a hypothesis, not a finding.
* The updater can be triggered without `flash.sh`/`scsitool` and without root on the host:
  `nvpflag fup 0x70555766` over adb, then reboot. It is self-clearing (`nvpflag fup -1` on every
  failure path), so a rejected package cannot loop the updater.

> **Rule added by this session.** `install_cinderhome.sh` is written to run **inside the Sony
> updater**, where nothing else holds `/system` or `/data`. Running it on a live system works but
> its `data_is_mounted()` sentinel false-negatives (the sentinel is designed for the updater's
> ramdisk and survives on a real, already-mounted `/data`), so `DATA_MOUNTED=0` and **the cable
> pass is silently not written**. That has to be written by hand afterwards, or the first boot
> lands on stock. If direct-install becomes a supported path, that check needs a live-system branch.

> **State after recovery, 2026-09-21.** wbrt restored the player to **stock firmware, not Walkman
> One**, with USB-MSC up and **no adb** (stock ships adb off; `/system/vendor/sony/bin/AdbEnabler`
> exists but needs a shell to run, which is the chicken-and-egg). Two consequences:
>
> * **17.4 and 17.6–17.7 do not apply until Walkman One is reinstalled** — there is no
>   `/sbin/boot_complete.sh`, no `/contents/CFW/settings.txt` and no `clv` tint on stock. (When W1
>   *is* reinstalled, adb comes back for free by adding a line `ADB=1` to `CFW/settings.txt` over
>   MSC — that block runs outside the `TMD5` guard, so it works even in the "tuning not applied"
>   state. On stock there is no such hook.)
> * **This is not a setback for builds ① and ②.** Stock is Cinder's known-good target; it is where
>   the stock-like UI skin gets built and where everything except the firmware axis can proceed.
>   **17.3 is now the priority, and it needs no device at all.**
>
> Getting adb back on stock means flashing something that enables it — the dev-channel Cinder
> package self-enables adb — which needs either `tools/flash.sh` (root on the host) or the
> end-user installer. Owner's call; nothing below is blocked on it except 17.1–17.2.

### Phase A — no boot risk at all (read-only, stock firmware running)

| # | Item | Do | PASS |
|---|---|---|---|
| 17.1 | **Get the evidence off the player before anything overwrites it** | `adb pull /contents/cinderhome.log`, `adb pull /contents/cinder_home_install.log`, and `adb shell ls -la /data/cinder/` | Either the launcher wrote breadcrumbs (→ the loop is **above** the launcher, and the log names the rung) or it wrote nothing (→ the loop is **beneath** it, i.e. appmgr) |
| 17.2 | **Does `cinder-home` start on 3.02 at all?** — the decisive, loop-proof test | With the **stock app still Home**, run the binary from a shell: `adb shell 'LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib /path/to/cinder-home'`. It cannot take Foreground and will exit — that is expected | A **link error, missing symbol or SIGSEGV appears immediately**. That is the answer. This cannot loop the device because nothing repoints `.appcfg` |
| 17.3 | ~~**Which Sony libraries differ between 1.02 and 3.02?**~~ **DONE 2026-09-21, off-device: 0 symbol differences across all 13 Cinder-linked libs; easel/appmgr byte-identical. 65 of the full set identical, 18 differ — of those Cinder links, only `libEffectCtrlDmp.so` and `libMediaStoreServiceClient.so`, both same-exports/different-code** | Off-device: symbol-diff Walkman One's `/system/vendor/sony/lib` against the catalogued 1.02 set in the Sony-files repo (`firmware/nw-a50/1.02/system/symbols/`) | A list of changed/removed exports. If `easel`/appmgr-facing symbols moved, 17.2's failure is explained and the fix is a per-firmware build |
| 17.4 | **Confirm the wbrt restore put Walkman One back intact** | `adb shell 'nvpstr kas; nvpstr fpi; nvpflag -x mid; cat /opt2/sig; tail -30 /contents/CFW/boot_log.txt'` | `kas` nw-wm1a, `fpi` NW-WM1Z, `/opt2/sig` `wm1z`, and a fresh boot entry. **Note whether it still says "tuning was not applied"** — it did before the loop, so that is pre-existing, not damage |
| 17.5 | **NVRAM state, for the external-tuning question** | `adb shell 'md5sum /dev/block/mmcblk0p3'`, compare with `/opt2/stock/nv_bk` | Equal ⇒ no tuning applied. Records the baseline before any tuning work, and is exactly the test Walkman One's own boot script uses |

### Phase B — cheap, still no install

| # | Item | Do | PASS |
|---|---|---|---|
| 17.6 | **Clear Bass with the stock UI as reference** (was 16.5) | Cable **out**. Set Clear Bass in Walkman One's Sound Settings, then compare the service log and effect state against what `cinder-probe --clearbass` writes | The difference names what `Eq6band::UpdateProcCond` waits for. Do **not** run `cinder-probe` playback paths while the stock app owns the player |
| 17.7 | **Read `clv`** | `adb shell 'nvpflag -x clv'` against `COL` in `/contents/CFW/settings.txt` | They agree. Confirms Cinder can honour the icon tint for free |

### Phase C — only after 17.1–17.3 have an answer

| # | Item | PASS |
|---|---|---|
| 17.8 | A `cinder-home` that **starts** under 3.02 (however it is achieved) | 17.2 runs clean |
| 17.9 | Re-attempt the install, **cable pass written by hand**, wbrt backup current | Cinder paints |

**Do not re-attempt an install on Walkman One before 17.2 answers.** The failure mode is a boot
loop below the escape ladder, and the only way out of it is the one that was just used.

---

## Open from 2026-09-22 — can a Walkman One user install a RELEASE?

> **Owner report, 2026-10-01:** yes — the owner's own player is Walkman One running Cinder, and "all of the installers
> are working".

Cinder **runs** on Walkman One: proven twice now, and 2026-09-22 added an install onto a live W1
3.02 player over adb that came up clean with the cable in (`cable escape stood down — the pass is
spent`), plus FM through the register path, the firmware line in the install log, and the `Base`
row on Settings ▸ Device. **None of that is the path a stranger uses.** Everything below is about
the release artifacts, not the app.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 18.1 | **Does an `nw-wm1a`-sealed `.UPG` install on Walkman One at all?** The one open question that decides the shape of everything else here. See the correction above — this has never been measured | `bash cinder-home/tools/pack_upg.sh dev nw-wm1a`, stage `cinder_home_install.nw-wm1a.upg` on the drive, `nvpflag fup 0x70555766` over adb, reboot. Read the result back with `nvp zr 26 4` | The updater runs and Cinder is installed. `nvp zr 26 4` is NOT `45 55 50 47` (`EUPG`) | `EUPG` again = W1's `fwpchk` really does mutate the key, and **W1 needs a non-`.UPG` install route** (a documented adb/script path). That is a release-notes decision, not a bug |
| 18.2 | **The release packs a Walkman One package** | `tools/release.sh` calls `pack_upg.sh stable` with no model, so `dist/stable/` carries `nw-a50` only. Add the `nw-wm1a` pack (and the uninstall) once 18.1 says a package is worth shipping | Both packages in `dist/stable/`, both round-trip-checked | Gated on 18.1: if no `.UPG` installs on W1, shipping a second one is worse than useless — it looks like support that does not exist |
| 18.3 | **The end-user installer knows which player it is talking to** | `installer/src/` has no model or KAS awareness: one package, one vendor SCSI command. A W1 player silently drops it | The installer reads the player's KAS (or model) and picks the package, or says plainly that this player needs the W1 route | Silence is the whole problem here — the 09-21 session was spent on a failure that wrote nothing anywhere |
| 18.4 | **Uninstall on Walkman One** — `cinder_home_uninstall.upg` has never run there | After 18.1, uninstall and confirm W1's own player returns | W1 boots its own Home app; `.appcfg.real` restored | A player that cannot be uninstalled is not shippable to strangers |
| 18.5 | **The stable channel on W1** — everything proven so far is `dev`, which self-enables adb. Stable has no adb, and on W1 that means no diagnosis at all unless `ADB=1` is set in `/contents/CFW/settings.txt` | Install stable on W1; boot; break nothing | Cinder paints, and the install log names the firmware | The install log is the only channel left: it is why `firmware:` was added to it on 09-22 |
| 18.6 | **`cinder-guard.sh` is still not wired into any installer** — and it is W1-specific (it hooks `bootswitcher.sh`, the only persistent point upstream of appmgr). It is the sole backstop below the launcher's own counter | Decide: ship it for W1 installs, or leave it a manual step documented in the W1 route | A W1 install either installs the guard or says why it does not | Init blocks on the script that calls it, which is why it was not wired in on 09-21. That trade needs a decision, not a default |

---

## 19 — 2026-09-28 — the redesign's first two passes, and three community requests

**ALL PASS — 19.1 to 19.16, on the owner's A55, 2026-09-28** (dev build `b2a428d1`, installed over
adb, one reboot). Kept as the run sheet for the next time these screens change.

Everything here is host-tested and in the golden previews (`docs/PLAN_redesign_2026-09.md` Parts A
and B). No flash is needed: a dev build installed over adb and one reboot covers the lot. The first
two rows change what the owner sees at every boot, so they come first.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 19.1 | **The player opens on the Library** (the new default home) | Reboot | The Library is on screen after boot; Back reaches Now Playing | A player that opens on a blank Library (scan not yet in) should still land there once the library arrives — `home_pending` |
| 19.2 | **Menu ▸ START ON** | Pick each chip, reboot after *Now Playing* and after *Last screen* (leave it on an album first) | The player opens where chosen; *Last screen* reopens the album | `home_screen` / `home_last` in `cinder_settings.conf` say what was saved |
| 19.3 | **Settings ▸ Display** | Palette, Accent swatches, Night, Volume chips, Size slider, Visualiser | Each does what it did on the old Settings rows | — |
| 19.4 | **Minimal volume readout** | Display ▸ Volume: *Minimal*, press Vol± | A thin accent bar across the top, gone after the usual timeout | — |
| 19.5 | **Bluetooth ▸ THIS DEVICE** | Connect LDAC headphones | *Sound quality* shows the LDAC rate and opens the codec screen; *PAIR NEW* in the section label opens pairing | — |
| 19.6 | **Lyrics chip** | Play a song with an `.lrc` or embedded lyrics, then one without | LYRICS chip top-left on the first, none on the second; one tap opens the words | The chip is drawn from `has_lyrics()`; the words arrive with the track change |
| 19.7 | **Pull-down panel OFF (the default)** | Drag down from the status bar on the Library and on Settings | The list scrolls exactly as before; nothing opens | Any claim with the setting off is a regression: `cinder_quick_pull_begin` must return 0 |
| 19.8 | **Pull-down panel ON** | Settings ▸ Pull-down panel on. Drag down from the status bar about a finger's width | The panel opens; brightness, Bluetooth, night and sleep chips work; a tap below it or Back closes it; a drag started below the status bar still scrolls | A short pull (under 60 px) must open nothing and must not register as a status-bar tap (the Menu) |
| 19.9 | **Long names** | A headphone name over 30 characters; an album with a long artist credit | Both fit (ellipsis) instead of running off the screen | The text audit (`cinder-host --audit`) is the host check for these |
| 19.10 | **Palette picker** | Copy `slate.palette`, `paper.palette` and one broken file into `cinder_palettes/`; unplug; Display ▸ Palette | All listed with swatches; tap one and the screen repaints; *Added* puts the newest copy first; the broken file sits under SKIPPED with its reason | An *Added* order that looks random means the files' mtimes did not survive the copy — the sort falls back to name for equal times |
| 19.11 | **SensMe grid** | Menu ▸ SensMe with a tagged library | Tiles with counts; a tile opens its channel; the button shuffles all; with *Follow the time of day* on, the tile for the hour says NOW and the button plays it | A NOW tile on the wrong channel for the hour is the boundary table in `sensme::time_channel_id` — Cinder's guess, not Sony's |
| 19.12 | **Bluetooth debug log** | Bluetooth ▸ THIS DEVICE ▸ Debug log on; connect and disconnect headphones; switch it off | A toast names `cinder-bt-log-<time>.btsnoop` on the drive, and Wireshark opens it | "Could not read the Bluetooth log" = mtkbt wrote it 0600 root: `adb shell ls -l /tmp/hci_sniffer_log_*`. The fix is a setuid copy (like `cinder-clock`) or a chmod from the launcher — decide then |
| 19.13 | **Sound quality** | Bluetooth ▸ THIS DEVICE ▸ Sound quality with LDAC headphones connected; tap SBC, then LDAC, then *Connection priority* | The strip reads `<NAME> · LDAC`; each tap applies as it did from the old page (the same `BtCodecChanged`); with SBC chosen the LDAC rows go and VOLUME CONTROL moves up | A strip saying `CODEC 0x.. · ASKED FOR LDAC` while LDAC plays means the raw byte changed meaning — check `link_codec_name` |
| 19.14 | **Device screen** | Settings ▸ Device, on battery and then on the cable; scroll to the end | The strip shows the status word; the bar fills in the accent only while charging; CHARGER reads a state with the helper installed, or "Needs the battery helper" without it | — |
| 19.15 | **Help & controls** | Menu ▸ Help & controls; scroll to the end; tap *Replay the introduction*; swipe through it | One list; the replay starts at Welcome and finishing it lands back on Help | The Menu used to open the intro directly; a Menu row that still does is an old build |
| 19.16 | **Sound strip** | Sound with every effect on, at the largest text size | Two lines under the header; the second ends with the output (`3.5MM` or `BT·…`) | — |

---

## 20 — 2026-09-28 — R3's queue extras

**ALL PASS — 20.1 to 20.5, on the owner's A55, 2026-09-28** (dev build `3912e3b8`, installed over
adb, one reboot). Kept as the run sheet for the next time the queue changes.

Host-tested (unit tests, and three harness scenarios for stop-after). Needs a dev build and one
reboot. Play something first.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 20.1 | **Save Up Next** | Play an album; Up Next ▸ SAVE (NOW PLAYING heading) ▸ DONE | Toast "Saved to Playlists · N songs"; Library ▸ Playlists has it with the playing song and everything after, in order; you stay on Up Next | A playlist with 0 songs = the library DB did not resolve the rows (`cinder-ffi: Up Next saved as …` in the log says how many) |
| 20.2 | **Stop after this song** (jack) | Settings ▸ Sleep timer to END OF SONG (or the pull-down ▸ *Song*), mid-song | Now Playing shows SLEEP AT SONG END; the song plays to its end and pauses; ▶ plays the next song; the sleep row is back to OFF | A second of the NEXT song before the pause = the extrapolated pause missed and the fallback caught it (log: `stop-after: the next song began`). Note it — `STOP_AFTER_LEAD_MS` may need to grow |
| 20.3 | **Stop after this song** (Bluetooth) | As 20.2 with headphones | The same | — |
| 20.4 | **Repeat album** (jack, then BT) | Play an album from its 1st song, queue a song from another album after it (swipe right), tap repeat until ALBUM shows; skip to the album's last song | After it the album's FIRST song plays, not the queued one, and Up Next highlights it | Log `repeat album — lapping N songs` 2.5 s before the end. A click at the lap on BT is the known cost of the re-issue (the same as the queue rebuild) |
| 20.5 | **Repeat album off again** | During 20.4, tap repeat to *one* then *off* before the album's last song ends | After the last song the queued song plays — the rest of the list came back | If the album laps anyway, the re-derive on leaving album (`queue_pending`) did not fire |


---

## 21 — 2026-09-29 — DAC EQ, 32-bit FLAC, linear amp

> **Owner report, 2026-10-01:** the BT receiver works (21.7 and 21.9 were already PASS). The report did not single
> out 21.10 (Use Sony's receiver), the DAC EQ or the linear amp, so those rows say what they said.

Host-tested; the DAC EQ helper's tables were also written and read back on the W1 reference player
(`analysis/RE_codec_tone_table.md` §4). Needs a dev build and one reboot, and wired headphones for
21.1–21.3. Start with the volume low.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 21.1 | **DAC EQ is audible** | Wired headphones, music playing. Sound ▸ Advanced ▸ DAC EQ: drag BASS to −12, then TREBLE to −12 | Bass clearly thinner; then the top dulls too. Reset brings both back | Log `dac eq: … -> rc=0` but no change = the kernel did not reload the block (re-plug the headphones: that forces `adjust_tone_control`). `rc=4` = no Sony tone table on this player |
| 21.2 | **Boost lowers, never clips** | TREBLE to +6 | Treble tilts up and the rest gets quieter; no distortion on loud tracks | Distortion = the pre-gain is not reaching the codec; check the log line's curve |
| 21.3 | **Source Direct holds it flat** | With BASS at −12, Sound ▸ Advanced ▸ Source Direct on, then off | Full bass with Direct on; thin again with it off; the DAC EQ screen says "held flat" while Direct is on | A click at each change is worth noting (reload pop) — count them |
| 21.4 | **32-bit FLAC skipped** | Tap "Anything" (Sprain) in its album | Toast "Skipped a 32-bit FLAC…"; the next song plays; no AUDIO STOPPED banner. Log: `FLAC wider than 24 bits — skipped` | AUDIO STOPPED = the file reached PlayerService: `cinder-ffi` should log how many undecodable paths it loaded at startup |
| 21.5 | **32-bit FLAC alone** | Play the Sprain album on shuffle; then play the song on its own from search | Shuffle: no toast, the song never comes up. Alone: "Can't play a 32-bit FLAC…", nothing starts | — |
| 21.7 | **BT Receiver** — **PASS 2026-09-29, owner-reported (Windows PC, 48 kHz, 33 s)** | Bluetooth ▸ RECEIVER MODE › ▸ on; pair from a phone/PC; confirm the code on the Walkman; play | Sound at the jack; the page says Playing from <name> | Logcat `BtPlayerService`: AVSNK 2 waiting / 4 connected / 5 streaming. Stuck at 4 = nothing sent yet; check the PC's output device |
| 21.8 | **BT Receiver from an iPhone** | As 21.7 from iOS | Pairs | Failed 2026-09-29: "NW-A50series is not supported" before any code appeared |
| 21.9 | **BT Receiver from an iPhone, with the page cancel** (2026-09-30 build) — **PASS 2026-09-30, owner-reported: iPhone and Windows paired and played; both negotiated AAC (codec byte 0x03), snoop-confirmed** | Headphones OFF so the reconnect ladder is paging; open the Receiver page, switch on, pair from the iPhone | Pairs; log has `rx: RequestCancelConnection rc=` before `EnterFuncMode(A2dpSink)`, then `RequestStartConnectWait rc=1 … sink status 2` | `sink status 1` = the entry still lost the race: the log should then show `re-issuing connect-wait` within ~5 s. With HCI snoop on (`--btlink hci on`): EIR must list AudioSink and Write Scan Enable must be 0x03 |
| 21.10 | **Use Sony's receiver** (2026-09-30 build) | Receiver page ▸ Use Sony's receiver ▸ Restart | Stock boots straight into its BT receiver; pair the iPhone there. Restart → back in Cinder, music plays | Log: `sony-rx: stored function was N`, `SetInt(6) ok … reads back 6`. Stock opened on music instead = the resume key is not what setFuncTree reads (RE_findings "Receiver via stock"). Back in Cinder: `sony-rx: restored stock's function to N` and `FuncMode at boot = 0`; a `2` there must be followed by `EnterFuncMode(MediaPlay)`. No sound in Cinder afterwards = FuncMode was left in sink — report it |
| 21.6 | **Linear headphone amp** | `docs/PLAN_sound_2026-09-29.md` §1 | As written there | As written there |


---

## 22 — 2026-09-29 — Library views

> **Owner report, 2026-10-01:** shipped in 0.3.14 and in use on the owner's Walkman One player, reported as working
> along with most of what the 10-01 audit (C1) listed as host-tested only. Not reported row by row,
> so the rows below are still the way to say exactly what passed.

Host-tested (hit tests for every grid tile and compact row, the A-Z jump, persistence) and in the
golden previews (`library_*_grid`, `library_*_compact`). Needs a dev build and one reboot. The one
thing only the device can say is whether 96 px covers load fast enough while the grid scrolls.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 22.1 | **The view button** | Library ▸ the icon left of the ORDER caption (left of search when search is installed). Tap it on each tab | Albums and Artists: list → grid → compact → list. Songs: list ↔ compact. Playlists: list ↔ grid. A toast names each view; the first item on screen stays first | A tap that opens search or cycles ORDER instead = the zones overlap at this text size; note Settings ▸ Display ▸ Size |
| 22.2 | **Grid covers** | Albums in grid; fling down a long library, then back up | Real covers within a frame or two of each line appearing; no stall on the fling | Gradients that never turn into covers = the 96 px files are missing from the art cache (`/data/cinder/art`, `*.t96`); a stall = too many loads per frame (`GRID_LOADS_PER_FRAME`, cinder-ffi) |
| 22.3 | **Grid taps** | Tap a cover, then an album's name under its cover | Both open that album; on Artists the right-hand tile opens the artist, not shuffle | Wrong album = render and hit test disagree; screenshot it |
| 22.4 | **Kept after a reboot** | Leave Albums in grid and Songs compact; reboot | Both come back that way (`lib_views=list,grid,…` in `cinder_settings.conf`) | — |

## 23 — 2026-09-29 — Design styles (Now Playing)

> **Owner report, 2026-10-01:** shipped in 0.3.14 and in use on the owner's Walkman One player, reported as working
> along with most of what the 10-01 audit (C1) listed as host-tested only. Not reported row by row,
> so the rows below are still the way to say exactly what passed.

Host-tested: the style contract (every target ≥ 44 px, no overlaps, reachable at its middle), the
overflow audit at every UI scale, taps, scrub and page swipe per style, and persistence. Golden
previews `now_playing_nocturne_*`, `now_playing_terminal_*`. Needs a dev build and one reboot. Only
the device can say whether the inset cover costs a visible frame on a track change, and whether
the styles read at arm's length.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 23.1 | **Pick a style** | Settings ▸ Display ▸ STYLE · NOW PLAYING ▸ tap Nocturne, then Terminal, then Cinder, going back to Now Playing each time | Now Playing is redrawn in that style at once; Library and Settings stay as they were | A chip that picks the wrong style = the chip row and its hit test disagree at this text size |
| 23.2 | **Every control, per style** | In Nocturne and in Terminal: play/pause, prev, next, shuffle, repeat (all four modes), heart, the title (Track information), each toolbar slot, the band under the status bar (Menu) | Each does what its label says, first time | Note the control and the style; a miss means the layout and the draw disagree |
| 23.3 | **Seek and pages** | Drag along the rail; swipe left and right across the cover; swipe below it | The rail follows the finger and seeks on release; the cover swipe turns cover → spectrum → level; below it, a swipe skips | — |
| 23.4 | **Track change cost** | Terminal or Nocturne, cover page; skip through ten tracks with real covers | The new cover appears with the new title, no stutter in the visualiser | A visible hitch = the one-time 480 → 396/400 px resample (`art::draw_fitted`) is too slow; note it |
| 23.5 | **Night and 140%** | Night on; then Display ▸ Size at 140% | Cover dimmed; nothing runs off the panel; the times stay on screen | Screenshot it |
| 23.6 | **Kept after a reboot** | Leave Nocturne picked; reboot | Nocturne again (`style=nocturne` in `cinder_settings.conf`) | — |

## 24 — 2026-09-29 — The visualiser's own FFT (PCM tap)

> **Owner report, 2026-10-01:** shipped in 0.3.14 and in use on the owner's Walkman One player, reported as working
> along with most of what the 10-01 audit (C1) listed as host-tested only. Not reported row by row,
> so the rows below are still the way to say exactly what passed.

Host-tested: the slot header layout as read off the device, slot choice by timestamp, stereo to
mono, a window running on into the next slot, format refusal, a missing queue file, band
placement for 12–64 bands, and the Bands row. The layout was read on this player 2026-09-29
(`docs/PLAN_pcm_visualiser.md`). Needs a dev build and one reboot. Only the device can say whether
the bars are in time with what you hear.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 24.1 | **The tap draws** | Play a library track, Now Playing, visualiser on. `adb shell 'grep "pcm tap" /contents/cinderhome.log \| tail -3'` | A line every 15 s: `pcm tap — pos N ms, slot k holds A..B ms` with A ≤ N < B | No line = the tap never covered the position: send the log lines around `viz: analyzer`; the lead (`TAP_LEAD_MS`) is off |
| 24.2 | **In time** | A track with a hard kick drum; watch the lowest bars | Each kick lifts the left bars as you hear it, not before or after | Bars early or late: note by roughly how much; that is `TAP_LEAD_MS` |
| 24.3 | **Bands** | Settings ▸ Display ▸ Visualiser ▸ Bands: step through 12, 24, 36, 48, 64, going back to Now Playing each time | That many columns, every one moving, none stuck at the bottom | A column that never moves = a band with no bin; note the count |
| 24.4 | **Sony's analyzer is off** | While playing with the visualiser on: `adb shell 'cut -d" " -f14,15 /proc/$(pgrep -f "hagodaemon AudioAnalyzerService" \| tail -1)/stat; sleep 20; cut -d" " -f14,15 /proc/$(pgrep -f "hagodaemon AudioAnalyzerService" \| tail -1)/stat'` (or the owner's own before/after) | The analyzer's CPU ticks barely move (was 160 ticks in 20 s, 7.9% of a core) | Ticks still climbing = the shell is still starting it; the tap is not staying fresh |
| 24.5 | **Fallback** | Play FM radio (or USB-DAC) with the visualiser on | The visualiser still moves (Sony's analyzer, interpolated) | Flat bars = the switch-over left nothing drawing |

## 25 — 2026-10-04 — R4: ratings, play counts, smart playlists, the playlist editor

**UNVERIFIED — nothing in this section has run on a player.** Host-tested (748 player tests), in
the golden previews (`album_rated`, `artist_played`, `library_playlists_smart`,
`playlist_page_smart`, `playlist_edit*`, `view_edit*`), and driven by touch in the simulator. Needs
a dev build and one reboot. The files are in [`TRACK_DATA.md`](TRACK_DATA.md).

What only the device can say: whether the files survive a reboot and a cable, whether a write to
`/contents` is ever felt in the audio or the UI, and whether a play is counted at the right moment.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 25.1 | **Rate a song** | Play a song; tap its title on Now Playing; tap the 4th star on the Rating row | Four stars lit at once; wait 5 s, then `adb shell cat /contents/cinder_stats.tsv` shows the song's path with `4` | No file = `stats not written (…)` in `cinderhome.log` says why. A hitch in the audio at the write is worth noting, with the file's size |
| 25.2 | **Clear it** | Tap the 4th star again | No stars; after 5 s the song's line is gone (or its rating is `0` if it has plays) | — |
| 25.3 | **Survives a reboot** | Rate two songs, wait 5 s, reboot | Both ratings are back on Track information; log has `track stats: N rated or played` with N ≥ 2 | N = 0 with the file intact = the file was read before `/contents` was up. The next rating must MERGE, not replace: rate a third song and check all three lines are in the file |
| 25.4 | **A play is counted** | Play a 3-minute song past its half-way point | Within ~5 s of the half-way point the song's `plays` goes up by one and `last_played` is the player's clock | Counted at the start, or twice = `stats::Listen`; note the song's length |
| 25.5 | **A skipped song is not** | Play 10 s of a song, skip | Its count does not change | — |
| 25.6 | **Counted with scrobbling off** | With `/contents/cinder_no_scrobble` present (or the install option off), repeat 25.4 | The count still goes up; `.scrobbler.log` does not grow | — |
| 25.7 | **Album and artist pages** | Open the album of a rated song; then its artist | Album: stars top right. Artist: albums newest first, stars beside the rated album, MOST PLAYED above SONGS with the song from 25.4 | Albums in A–Z order = the year did not parse; note what the album row says |
| 25.8 | **USB hand-over** | Rate a song and at once (within 3 s) Settings ▸ USB mode | On the PC, `cinder_stats.tsv` already has the rating | Missing = the flush before `EnterUsbMsc` did not run |
| 25.9 | **A file edited on the PC** | In USB mode, add a line for another song by hand (`path<TAB>5<TAB>0<TAB>0`); turn USB off | After the library reloads, that song shows five stars | — |
| 25.10 | **Make a smart playlist** | Library ▸ Playlists ▸ SMART; Name ▸ type a name ▸ DONE; RATING 4+; SAVE | You land on its page: SMART PLAYLIST, the rated songs. `cat /contents/cinder_views.conf` has `[name]` and `rating=4` | No file = the save did not run: any tap should trigger it, so tap something and look again |
| 25.11 | **It follows the ratings** | Rate another song 5; open the smart playlist | The song is in it | — |
| 25.12 | **Play and shuffle it** | PLAY, then SHUFFLE on its page; tap a row | It plays as that list, in order / shuffled / from the row; Next stays inside the list | Plays the song's album instead = it went through `PlayIndex` |
| 25.13 | **Survives a reboot** | Reboot | The smart playlist is at the top of the Playlists tab with its diamond | Gone = `cinder_views.conf` was read before `/contents` was up; the next save must keep what is in the file |
| 25.14 | **Edit a playlist** | One of your own playlists ▸ EDIT. Drag a row by ≡ past two others; × another; UNDO; × again; DONE | Toast "Playlist saved"; the page shows the new order; the `.m3u8` has it, and a `#CINDER-EDITED:` line; the Playlists tab shows EDITED on it | The drag scrolls the list instead of lifting the row = the shell did not offer the contact to `cinder_reorder_begin` on this screen |
| 25.15 | **Long list** | In the editor of a playlist longer than the screen, hold a row and drag it to the bottom edge | The list scrolls under the row and it lands where it is dropped | — |
| 25.16 | **Back does not save** | EDIT, remove two rows, swipe Back | Toast "Changes not saved"; the playlist is unchanged | — |
| 25.17 | **A member on a missing card** | A playlist with tracks on the SD card: take the card out, reboot, EDIT, move a row, DONE; put the card back, reboot | The card's tracks are still in the playlist | Tracks lost = `Store::reorder` did not keep unresolved entries; keep the `.m3u8` before and after |
| 25.18 | **Shuffle by album** | Settings ▸ Shuffle to ALBUMS; play a playlist that spans several albums; tap shuffle on Now Playing; open Up Next | Toast "Shuffled by album"; the playing album's remaining songs come first, then whole albums, each in order. Shuffle off restores the playlist's order | — |
| 25.19 | **Shuffle by artist** | Settings ▸ Shuffle to ARTISTS; Library ▸ Songs ▸ Shuffle all songs; then Up Next ▸ MIX | Whole artists, album by album | Slow or a stall on the whole library = note the time; the deal is one pass, but the sequence hand-over is the known 512-track path |
| 25.20 | **Kept after a reboot** | Leave Shuffle on ALBUMS; reboot | Still ALBUMS (`shuffle_by=albums` in `cinder_settings.conf`) | — |

## 26 — 2026-09-30 — R5: sound profiles per output, the grouped Sound screen, effects parity

(§25 is left for the R4 work — ratings, play counts, playlists — built beside this in another
branch.)

**Nothing in this section has run on a player.** It was built host-only, with no device attached.
Host-tested: the profile data model and every route edge (734 Rust tests in the player workspace), the settings round
trip including a file from before R5 and a damaged one, the atomic settings write, the grouped
Sound list's layout, hit test and scroll at every UI scale, two harness scenarios for the shell's
half (`profile-route`, `profile-same`), goldens `sound_*`, `profiles*`, `bluetooth_*`. The C++
compiles and links for the device (ARM, glibc 2.23, libc++ 3.9). Needs a dev build and one reboot.

Every Sony call this section depends on is marked in the source as **signature from disassembly,
unverified on device**: the `fx-verify` read-back getters called from inside `cinder-home`
(26.3), and the whole of `SoundServiceSettingsDmp` (26.7).

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 26.1 | **The grouped Sound screen** | Menu ▸ Sound. Scroll to the end and back. Tap each switch; tap VPT and DC Phase through their values; drag Balance; tap MONO and CENTRE; tap Equalizer; tap Advanced | Profile row, then ENHANCE (Equalizer, DSEE HX, ClearAudio+), SPACE (VPT, DC Phase, Vinyl), LEVEL (Normalizer, Balance), then Advanced. The signal path stays put while the list scrolls under it. Every tap lands on the row under the finger; a vertical drag that starts on the Balance track moves the knob, one that starts anywhere else scrolls | A tap that lands one row off after scrolling = render and hit test disagree: note the row and roughly how far it was scrolled |
| 26.2 | **A and B are whole profiles** | On A: DSEE HX on, VPT Club, an EQ curve, Advanced ▸ Tone Control on with Bass +4. Tap **B**. Then tap **A** | B: everything at its own values (fresh: all off, Tone Control off). A: every one of those back, Advanced included. Toast `Profile B · Headphone jack`. Audible change both ways | Advanced values that do not follow = `SoundSetup` missing a field. Mono and the linear amp must NOT change with A/B — that is intended |
| 26.3 | **The read-back works from inside cinder-home** (unverified signatures) | After 26.2: `adb shell 'grep fx-verify /contents/cinderhome.log \| tail -5'` | `fx-verify(…): ok, N values read back as sent` with N ≈ 30 | `…differ:` listing nearly every value that is switched ON reading 0 = the getters get no reply in-app (the probe needs its own Framework pump for them). The profile still applied; the read-back is then removed or given a pump. A FEW values differing = a real mismatch: send the line |
| 26.4 | **Bluetooth uses its own profile** | Sound ▸ Profile row ▸ tap **Bluetooth** so it reads B. Set B up differently from A (it is not live, so: go back, tap B in the header, tune it, tap A). Connect headphones. Disconnect them | On connect: toast `Profile B for Bluetooth`, the Sound header shows B, the headphones play B's tuning. On disconnect: toast `Profile A for Headphone jack`, the jack plays A's. Log: `profile: bt connect -> profile B for Bluetooth`, then `profile: bt disconnect -> profile A for the 3.5 mm jack`, each followed by an `fx-verify` line | No `profile:` line on disconnect = `apply_profile_if_switched` is not reached on that edge. Toast but no audible change = see 26.5 |
| 26.5 | **Which effects actually run on Bluetooth** (goal 7; [`RESEARCH_bt_dsp_2026-09-30.md`](RESEARCH_bt_dsp_2026-09-30.md) §6) | Headphones connected, music playing. `adb logcat -c`; switch ONE effect on; wait 2 s; `adb logcat -d \| grep isproc`. Repeat for the 10-band EQ, Tone Control, VPT, DC Phase, Dynamic Normalizer, Clear Phase, Vinyl, DSEE HX. Then the same on the jack | Per effect, `<Class>::UpdateProcCond … isproc is 1` on switch-on. Expected from the static read: 1 for the EQ and Tone Control on both routes; the others unknown | An effect at `isproc is 0` on Bluetooth and 1 on the jack is not applied over A2DP: its row must say so on that route. Record the table of results in the research note |
| 26.6 | **USB-DAC's profile, and whether effects reach PC audio** | Profiles ▸ USB-DAC to B. Enter USB-DAC mode with a PC streaming, jack output. Then the `isproc` read of 26.5 | Toast and log line as 26.4 (`usb-dac switch`). `isproc` lines appear when an effect is toggled, and B's EQ is audible on the PC's audio | No `isproc` lines at all = Sony's chain is not in the USB-DAC path. Not a defect: the row's subtitle ("effects on PC audio unverified") becomes "Balance only", and this row is closed as measured |
| 26.7 | **`SoundServiceSettingsDmp` reads** (unverified signatures; read-only) | `adb push cinder-probe /tmp/pv`; with the launcher's `LD_LIBRARY_PATH`: `/tmp/pv --soundsettings` | Six lines `soundsettings: <name> = <n>` with n ≥ 0, then `done, pump ticks=N` with N > 0. `logcat` shows the sound service's own `GetParams` lines | `-2` = the library threw on an empty reply (its `std::stoi` is unguarded) — the shim caught it; that key has no value on this model. `-1` = the client could not be built. A hang names the row it stopped on (each is logged before it is read). Do not write these settings until the values are understood |
| 26.8 | **Kept after a reboot, and across an upgrade** | With jack = A, Bluetooth = B and both tuned: reboot with headphones off | Sound shows A with A's tuning (the boot is on the jack even if the file was last written on Bluetooth). `cinder_settings.conf` has `profile_jack=a`, `profile_bt=b`, `profile_usb=…` and `bank_adv=` / `bank_tone=` lines. No `cinder_settings.conf.tmp` left on the drive. On the FIRST boot after upgrading from 0.3.14: B has the same Advanced values A had (they were shared before), nothing sounds different | A stale `.tmp` file = the rename failed on this card's filesystem: send `ls -la /contents/cinder_settings*` |
| 26.9 | **Bluetooth ▸ Sound profile** | Bluetooth screen, THIS DEVICE | Three rows — Sound quality, Sound profile (value A or B), Debug log — all above the RECEIVER MODE link, none overlapping it; five paired devices still fit above. The row opens Sound profiles | Overlap with the footer at a large text size: screenshot it |

## 27 — 2026-10-04 — Boot to stock hands Sony's EQ back

Reported on r/walkman 2026-10-04: after Boot to stock, the stock equalizer changes nothing. Cause
(read from the code, not yet measured): Cinder sets `SetSelectUsingEq(2)` (10-band) and the sound
service keeps it; stock uses the six-band (1) and never calls the selector. Fix: `boot_to_stock()`
sends selector 1 and Source Direct off before the restart (`stock_handback_fn`, harness scenario
`stock-handback`).

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 27.1 | **Reproduce on the old build first** | On v0.3.14: Settings ▸ Boot to stock. In Sony's player, play a track on the jack and push the equalizer's bass to the top and back | No audible change (the report) | If the EQ works, the cause is something else: stop and read `adb logcat \| grep isproc` in stock before trusting the fix |
| 27.2 | **The hand-back** | On this build: Settings ▸ Boot to stock. Same EQ test in Sony's player | The equalizer is audible. `cinderhome.log` from the Cinder boot before it holds `boot-to-stock: handed the EQ selector back` | Log line present but EQ still dead: the selector is not what stock reads; `cinder-probe --fx` after returning shows `SelectUsingEq` |
| 27.3 | **Cinder takes its EQ again** | Restart from stock into Cinder; Menu ▸ Sound ▸ Equalizer, push a band | Audible; `fx-verify` (26.3) or `cinder-probe --fx` shows `SelectUsingEq=2` | Cinder's EQ dead after a stock visit: the boot apply did not re-send the selector |
| 27.4 | **Use Sony's receiver takes the same path** | Receiver ▸ Use Sony's receiver ▸ Restart (with 21.10) | The log line of 27.2 before the restart | — |
| 27.5 | **The other roads to stock** (2026-10-04: the launcher hands the EQ back) | From Cinder, with a cable in and `cable_escape_off` removed, reboot so the cable escape fires. Try Sony's EQ | The equalizer is audible. `/data/cinder/cinderhome.log` holds `stock-eq: EQ selector 2 -> 1`; `/data/cinder/eq_owned` is gone | No log line and the marker still there = the launcher on the player is older than the app (reinstall). Line present, EQ dead = the selector is not the cause: `cinder-probe --fx` in stock |
| 27.6 | **It runs once** | Reboot into stock again by the same route | No second `stock-eq:` line | A second line = the marker was written by something other than a Cinder start |
| 27.7 | **Uninstall is not covered** | Uninstall with Cinder's EQ in use, without a Boot to stock first | Sony's EQ is probably dead until a preset is picked or the player is reset: record what happens | — |

## 28 — 2026-10-04 — Soundscapes

Host-tested ([`SPEC_soundscapes.md`](SPEC_soundscapes.md) "Tests"): the generator, the shim through
`LD_PRELOAD`, the mix hook on the Bluetooth stream, the page, and the shell's line and route in the
harness. **Needs a release or dev build with the new `libcinder_mono.so`** for 28.5–28.7, and
Wampy's preload for those three at all. Start with the volume low: a soundscape at 100% on its own
is as loud as music.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 28.1 | **On its own, through the jack** | Wired headphones, nothing playing. Menu ▸ Soundscapes ▸ Rain | Rain within a second; the strip says ON ITS OWN · HEADPHONES. Log: `soundscape: playing on its own through hw:0,4 at 44100 Hz` | Strip WAITING FOR THE OUTPUT + `cannot open the jack (… busy)` = Sony's sound service still holds the PCM after a pause: wait 30 s and note whether it ever lets go. Opened but silent = the codec is muted while the player is stopped — note `amixer -c0 cget name='playback mute'` |
| 28.2 | **On its own, over Bluetooth** | Headphones linked over Bluetooth, nothing playing | Rain in the headphones; strip ON ITS OWN · BLUETOOTH. Log: `ldac: handshake accepted`, then `soundscape: playing on its own over Bluetooth` | `handshake REJECTED` = the link negotiated 48 kHz: report it (the player sends 44.1 kHz). No socket = the source did not open; send the `ldac:` lines |
| 28.3 | **Music takes the output back** | During 28.1, play a song; pause it; play again. Repeat over Bluetooth (28.2) | The song starts at once every time, with no error and no AUDIO STOPPED banner. Log: `soundscape: the output is free for the music` before each start; after the pause the soundscape comes back within a few seconds | A song that fails to start = the yield did not finish in 300 ms (`yield TIMED OUT`): that is a defect to report before anything else on this list |
| 28.4 | **What it costs** | Soundscape on its own, screen off, 10 min: `adb shell 'cat /proc/$(pgrep cinder-home)/stat \| cut -d" " -f14,15'` before and after, for Rain and for Fire | Under 5% of one core | Higher: note which sound; the self-test's host cost table says which part is heavy |
| 28.5 | **Over library music, jack** (Wampy installed) | Play a song, then Menu ▸ Soundscapes ▸ Beach, With music at 30% | The beach under the song; strip OVER THE MUSIC. `/tmp/cinder_mono.log`: `jack: soundscape mixed in (fmt 2, 2 ch, 44100 Hz)`. Volume buttons move both together | `rate never seen` in the log = the HAL commits its rate some other way: send the log. Clicks = report the sound and the track's format |
| 28.6 | **Over library music, Bluetooth** (Wampy installed) | As 28.5 over LDAC | The beach under the song in the headphones. `bt: soundscape mixed in (fmt 2, 2 ch, 44100 Hz)`; the stream stays up | A dropout or a reboot: stop, send `/tmp/cinder_mono.log` and `last_kmsg`; `touch /data/cinder/mono_shim_off` takes the hooks out |
| 28.7 | **Two levels** | During 28.5, pause: the soundscape should rise to the On its own level (through the shim if Sony keeps writing, else the shell's own player) | Quieter under music, fuller alone, with no jump at either edge | No change on pause = the line was not rewritten: `cat /tmp/cinder_ambient` before and after |
| 28.8 | **Over USB-DAC → LDAC** | USB-DAC to Bluetooth from a PC playing music, soundscape on | The soundscape under the PC's audio; strip OVER USB-DAC / RADIO | Nothing: the pump read `g_amb_want_sound` as 0 — send `soundscape:` lines |
| 28.9 | **Without Wampy (Walkman One)** | As 28.5 on a W1 player | Strip SILENT WHILE MUSIC PLAYS while the song plays; the soundscape comes back on pause | It plays over the music anyway = something else preloads the shim: interesting, report it |
| 28.10 | **Sleep timer** | Soundscape on its own, sleep timer 15 min (or Song) | When it fires the soundscape fades out and the switch reads off | — |

## 29 — 2026-10-04 — Visualisers from the decoded audio

Host-tested (`viz::signal_tests`, `vizsig` tests, golden previews `viz_signal_*`). The tap itself
is §24; this is what is now drawn from it. Needs a dev build and one reboot.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 29.1 | **Scope** | Now Playing ▸ spectrum page, style Scope, a track with a steady bass note | A waveform that stands still on a held note and changes shape with the music | A crawling trace on a steady tone = the trigger never fires (very quiet input): note the track |
| 29.2 | **Stereo field** | Style Stereo field: a mono recording, then a wide stereo one | Mono: a vertical line and the correlation bar at the right end. Wide: a cloud, the bar nearer the middle | Lying on its side = the channels are swapped or inverted somewhere: report it |
| 29.3 | **Meters** | Style Meters, a loud master | Bars near the right; peaks briefly at the end; held ticks wait 1.5 s then fall | Pinned at full scale on quiet music = the samples are mis-scaled (24-bit read as 16) |
| 29.4 | **Spectrogram** | Style Spectrogram for 10 s | Five seconds of history scrolling left; a kick drum is a bright stripe at the bottom | — |
| 29.5 | **Bands only** | FM (or USB-DAC) with Scope chosen | "Needs library playback" on the spectrum page; Spectrogram and the bar styles still draw | A blank page = the fallback text failed |
| 29.6 | **Cost** | Spectrum page, each new style, 20 s: cinder-home's `/proc/<pid>/stat` ticks | Close to Bars' | Much higher for Stereo or Spectrogram: note which |

## 30 — 2026-10-04 — The owner's decisions on R4 and R5

Host-tested (790+ player tests, golden previews `artist*`, `view_edit*`, `profiles*`, `menu*`).
Needs a dev build and one reboot.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 30.1 | **Menu** | Open the Menu | No Equalizer row; Sound ▸ Equalizer opens the equalizer | — |
| 30.2 | **Artist page** | Library ▸ Artists ▸ an artist with albums | Albums, then `SONGS · n` with SHOW ALL and no song rows. Tap the header: the list opens, the word reads HIDE. Tap again: folded. Shuffle artist still plays every track | Rows under a folded header = the fold was not applied to the hit test |
| 30.3 | **Shuffle per smart playlist** | Playlists ▸ a smart playlist ▸ EDIT ▸ SHUFFLE: Albums ▸ SAVE, then its Shuffle band | Whole albums, each in track order, in a random album order. `cinder_views.conf` holds `shuffle=albums`. With Settings chosen it follows Settings ▸ Shuffle | Songs at random with Albums chosen = the view's mode did not reach the deal; send the file |
| 30.4 | **Mono per profile** | Sound profiles ▸ ALSO PER PROFILE ▸ Mono on. Profile A: Mono on. Switch to B: Mono off. Switch back and forth | Mono follows the profile (with Wampy's shim: audible; without: the switch and `/tmp/cinder_mono` follow). With the row off again, mono is one value for both | The file `/tmp/cinder_mono` not following = `mono_flag_apply` did not run on the switch |
| 30.5 | **DAC EQ and amp per profile** | The same with DAC EQ (a different curve in A and B) and Linear amp | Log: one `dac eq:` line per switch, only when the curves differ; `hp amp:` follows within a few seconds | A `dac eq:` line on every route change with equal curves = the compare failed |
| 30.6 | **Nothing changes by default** | A player upgraded from the previous build, all three rows Off | Mono, the amp and the DAC EQ behave as before across A/B and route changes | — |

## 31 — 2026-10-04 — Idle with Bluetooth, and what stays powered behind a dark panel

Measured on the cable, Walkman One, 2026-10-04 (`/proc/clkmgr/pll_test`, `subsys_test`, `clk_test`):

| | screen dark, before stage 1 | stage 1 |
|---|---|---|
| VENCPLL (295.75 MHz, feeds the display bus) | ON | off |
| `SYS_DIS` (display power domain) and 13 display clocks | on | off |
| `SYS_CONN` (WiFi/Bluetooth chip's domain), Bluetooth switched off | on | off |
| UNIVPLL, USB0 | on (the cable) | on (the cable) |

So everything the dark panel still costs is switched off by stage 1, and stage 1 does not run in
the first 180 s of a boot, for the first 60 s of idle, or at all while playing over Bluetooth.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 31.1 | **The quiet poll** | Bluetooth on, nothing connected, screen dark for 6 minutes; `grep -c "BT route poll" ` is not logged, so instead: switch the headphones on | They connect and the UI shows them within a second or two of the link, as before. Log: `bt: link state changed (listener)` | Connects but the UI lags up to a minute = the listener did not fire for this link: report it; `btpoll` falls back to 3 s only when the listener is not registered |
| 31.2 | **Stage 1 while playing over Bluetooth** — **RUN 2026-10-04, PASS** on Walkman One with LDAC headphones, and the default since | Headphones connected and playing, screen off, listen for 5 minutes | The music does not stop or stutter. Log: `suspend: stage 1 also while playing over Bluetooth (default…)`, then `suspend: idle 5 s -> early suspend`. Measured: display domain off, 1285 -> 931 interrupts/s, link up through four entries and exits | Music stops or the link drops: the log says `the Bluetooth link went away during stage 1` and it stops for that boot. `touch /contents/cinder_no_suspend_bt` keeps it off. Still wanted: a second pair of headphones, and a long listen |
| 31.5 | **Idle blank locks after five minutes** | Settings ▸ screen-off timer 30 s. Leave it. Touch the dark screen after 1 minute, then leave it 6 minutes and touch again | First touch wakes it. After 6 minutes touch does nothing and Power wakes it. Log: `screen: dark for 5 min after the idle timeout -> locked` | Touch still wakes it after 6 minutes = the lock did not run; Power does not wake it = report the log |
| 31.6 | **Bluetooth auto off** | Settings ▸ Bluetooth auto off on. Bluetooth on, pause, screen off, leave 11 minutes | Bluetooth switch reads off; log: `bt: idle 10 min with the screen off -> radio off`. Playing over Bluetooth for 11 minutes with the screen off does NOT switch it off | Radio off mid-song = `audible` was wrong for that route: report the log |
| 31.3 | **What Bluetooth costs idle** | Bluetooth ON, nothing connected, screen dark, 6 minutes: `adb shell 'grep -E "SYS_CONN" /proc/clkmgr/subsys_test'` | Record `state(0)` or `state(1)` in stage 1. `state(1)` = the radio's domain stays powered for as long as Bluetooth is switched on, and an automatic radio-off after N minutes without a link would be worth building | — |
| 31.4 | **Framebuffer blank** (not run: needs the owner) | Screen dark, before stage 1: `echo 4 > /sys/class/graphics/fb0/blank`, read `grep VENCPLL /proc/clkmgr/pll_test`, then `echo 0 > …/blank` | VENCPLL off while blanked and the panel comes back on the next wake = the display can be switched off the moment the screen goes dark, without stage 1, which would cover Bluetooth playback too | VENCPLL stays ON = this kernel's blank does nothing; stage 1 is the only switch |

## 32 — 2026-10-04 — The hagodaemon wrapper (a preload on a player without Wampy)

Installed by hand over adb on the owner's Walkman One player; not in any package. `/proc/clkmgr`
is not involved. Measured that day: 28 services start through it, `/proc/<SoundServiceFw>/environ`
holds the `LD_PRELOAD` and no other service's does, `/tmp/cinder_mono.log` reads `loaded into
SoundServiceFw`, `audio flowed — load count cleared`, and `mono ON` / `jack: … summing` when the
flag is set. Framebuffer blank (31.4) was also run: it changes nothing on this kernel.

**Measured the same day with the headphone jack recorded by a PC** (`ffmpeg -f dshow`, stereo,
48 kHz), which settles 32.1 and 32.2 on the jack without ears:

| | off | on |
|---|---|---|
| Mono (`/tmp/cinder_mono`): level of left minus right, above 200 Hz | −48.2 dB | **−91.8 dB** (the sum unchanged at −41) |
| White noise over a song (`/tmp/cinder_ambient`): level above 14 kHz | −67.1 dB | −49.6 dB at 300/1000, −39.1 dB at 1000/1000, −68.1 dB off again |
| Stage 1 entered mid-song (72 s recorded, noise bed at full) | — | no gap of 3 ms or more |

32.3 (Bluetooth) is still nobody's measurement.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 32.1 | **Mono by ear** | Sound ▸ Mono on, a track with hard left/right panning, wired headphones | Both ears the same; off again restores the stereo picture | Log says summing and the ears disagree = the hook is on a PCM that is not the one playing |
| 32.2 | **Soundscape over music, jack** | Play a song, Menu ▸ Soundscapes ▸ Beach, With music 30% | The beach under the song; strip OVER THE MUSIC; `/tmp/cinder_mono.log`: `jack: soundscape mixed in` | Strip still says SILENT WHILE MUSIC PLAYS = Cinder did not see `/tmp/cinder_mono_shim` |
| 32.3 | **The same over Bluetooth** | 32.1 and 32.2 with headphones linked over LDAC | As on the jack; the stream stays up. Log: `bt: …` lines | A dropout or a restart: `touch /data/cinder/mono_shim_off`, reboot, report the log |
| 32.4 | **Off switch** | `touch /data/cinder/preload_off`, reboot | No `LD_PRELOAD` on SoundServiceFw, no `/tmp/cinder_mono_shim`; everything else as before | — |
| 32.5 | **Removing it** | `mount -o remount,rw /system; mv /system/vendor/sony/bin/hagodaemon.real /system/vendor/sony/bin/hagodaemon`, reboot | Stock again: `md5sum` of `hagodaemon` is Sony's | — |

## 33 — 2026-10-04 — Suspend to RAM (stage 2), Sony's idle state

Background: `analysis/RE_sony_idle_baseline.md`. Opt-in, `/contents/cinder_ram_suspend`. **Have the
player in hand:** if it does not wake, hold Power until it restarts. Keep `cable_escape_off` armed.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 33.1 | **It suspends and Power wakes it** | File present, boot, wait 4 minutes, nothing playing, Bluetooth not connected. Pull the cable, screen off, leave 3 minutes, press Power | Screen lights (it may take a second). Log: `suspend: idle and off the cable -> releasing the wakelock`, then `suspend: resumed from RAM`; `icx_pm_helper/resume_count` above 0 | No wake: hold Power to restart, delete the file over USB, report `/proc/last_kmsg` |
| 33.2 | **USB comes back** | After 33.1, plug the cable in | The PC sees the player; charging starts | Dead until a restart = the 2026-09-04 `a_idle` fault is still there: stage 2 stays opt-in |
| 33.3 | **Plugging in wakes it** | Suspended, plug the cable in without pressing anything | The charge light / PC connection appears | Nothing until Power = a suspended player on a charger does not charge: a blocker for making it default |
| 33.4 | **Never under music** | Play on the jack, screen off 3 minutes; the same over Bluetooth | Music never stops; no `releasing the wakelock` line | It stopped: delete the file, report the log |
| 33.5 | **How often it wakes by itself** | Leave it suspended 30 minutes | `resume_count` rises by a handful at most | Dozens = something is waking it; `spm_r12` names the source |

