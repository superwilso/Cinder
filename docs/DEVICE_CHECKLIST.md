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
| 0.1 | **`cinder-home/build.sh [stable\|dev]` passes** | This is the only gate that does the ARM link, the **GLIBC ≤ 2.23 ceiling** and the **qemu construction preflight**. CI deliberately does not carry the cross toolchain, so a green CI says nothing about whether the thing links for the device. **This gate earned its keep on 2026-08-25**: the tree did not compile for ARM at all — `f2f41a8` silenced an unused-parameter warning by commenting out a name that the `#if defined(__arm__)` body still used, which only host builds can skip. Note `build.sh` is not executable in a fresh checkout; run it as `bash build.sh dev`. |
| 0.2 | **`cinder-home/harness/run.sh` passes** | Thirteen scenarios, ~8 s — the strongest offline check of the app's *behaviour*, and what the fixes below were written against. **`build.sh` now runs it**, so 0.1 covers this; run it alone when iterating. |
| 0.3 | **`tools/release.sh`** if flashing a release | Verifies the committed `dist/` payload byte-for-byte against a fresh build before it will tag. |
| 0.4 | Escape ladder intact | Bad-boot counter → auto-revert → crash supervisor → kill switch → `wbrt` restore. `cinder-home/tools/test_launcher.sh` covers it offline and **now runs in CI**, so a green tick already says this. Run it by hand only if you changed the launcher. *(46 cases as a normal user, 45 as root — one case uses `chmod`, which does not bind uid 0, and skips itself there.)* |

---

## Phase 0 — zero boot risk (probe and adb only)

Nothing here loads the easel lifecycle, so none of it can affect whether the device boots.

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 0a | ~~**`ldac-bridge/TEST.md`** — the headline feature, **0% validated**~~ **Q1 PASS 2026-08-25** | `cinder-probe --ldac` (no flash, no stock boot needed) | `GetSocketName` = `pst::services::bttransmitterservice`, socket connected | **Q2 still open**: no capture PCM exists until the gadget is in UAC mode with a PC feeding audio, and that drops adb — hands-on |
| 0b | ~~**`cinder-probe --analyzer`** — has never been run once~~ **PASS 2026-08-25** | With music playing (`--pump` alongside) | Frames arrive | Recorded: **12 bands, LINEAR values ~0…1.7e6**, not dBFS |
| 0c | ~~**A `PlayStatus` dump with music actually playing**~~ **PASS 2026-08-25** | `cinder-probe --pump` | Position advances, `ALSA pcm4p = RUNNING`, listener fires | Also settled **3d**: `duration_raw` is **milliseconds**. Note `--play` is the framework-DEAD control and cannot connect — use `--pump` |
| 0d | **`cinder-probe --discover`** — **ROOT-CAUSED, fix built, NOT yet re-run** | `--discover <report> <media paths…>` | Non-zero PlayStatus bytes | Two bugs, both fixed: it never pumped (so it was never connected — the "all zeros because nothing was playing" note was **wrong**), and `PlayStatus` is **per-controller**, so the dumping process must own the playback |
| 0e | **MediaStore re-scan probe** (§1a of the 08-23 audit) — **first half DONE 2026-08-25** | ~~Recover the `MediaStoreClient` vtable~~; then `strace -f` the `hagodaemon` hosting MediaStoreService across a stock USB-MSC disconnect | ~~The slot map~~, **and** whether a scan is app-driven at all | **No vtable needed**: `MediaScanner` is exported concretely — ctor takes `IMediaStoreService*`, plus `Scan()`/`ScanFile()`/`Cancel()`. `strace` is on the device at `/system/xbin/strace`; the MSC half still needs a real disconnect, which kills adb |
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
| 1a | It paints | A frame reaches the glass; the boot animation does not stay latched |
| 1b | Library loads | Album/artist/song counts look right for a 304-album library |
| 1c | **Bad-boot counter cleared** | `healthy: bad-boot counter cleared` in `/contents/cinderhome.log` within ~10 s of first paint |
| 1d | Type scale and non-Latin rendering | Eyeball; nothing clipped, no tofu |
| 1e | Touch navigation lands where drawn | Taps hit the row you aimed at. *(Narrower than it was: the harness now covers the decode — a contact becomes a tap, a drag becomes a drag and a fling, and the raw codes map to the right buttons. What is left here is the panel's ACTUAL coordinate range, which the harness invents, and whether the drawn geometry matches the hit test.)* |
| 1f | Vol± reaches the hardware | One audible press. *(The ramp curve, its stop-on-release and its stuck-key dead-man are covered off-device; that the mixer write is audible is not.)* |
| 1g | Transport buttons | Each does what it says |
| 1h | Idle screen-off, and **it wakes** | Blank it; wake by touch **and** by Power. A failed wake is indistinguishable from a dead device |

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

### 2B. Sound (PR #3, merged)

| # | Item | Do | PASS |
|---|---|---|---|
| 2B.1 | **The DSP reconcile with NO settings file** | Delete `/contents/cinder_settings.conf`, boot, play | The chain is what the UI draws — not what the stock player left. In particular the EQ is audible |
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
| 2D.1 | **A stalled bring-up is survivable** — *the highest-risk item in this file* | Rename `/db/MTPDB.dat`, boot | The UI is **usable** (touch and the Power button respond), the panel sleeps on the idle timeout, and nothing crashes. Then put the DB back | The input arm is **reasoned from call sites and the shims' null checks, not observed.** If the UI is dead or it crashes, that arm is wrong and should be reverted to housekeeping-only |
| 2D.2 | **…and it stays quiet** | Leave it in that state ten minutes | `/contents/cinderhome.log` grows by a handful of lines, not thousands | Before: 62 lines/second |
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
| 3a | **Play-by-index** — tap a track or album and it plays that one | The tapped row plays. *(`--transport` checks the primitive: `play_tracks({A,B}, start=1)` must play B.)* |
| 3b | **Playlists** — a playlist row plays the whole list in saved order, plain and shuffled | Both bands work; PLAY is not shuffle |
| 3c | **Drag-to-seek** — `media_origin_t::Begin == 0` is the last unverified value in that path | It lands where you dropped it. *(`--transport`: seek to 60 s must land at ~60 s, not now+60 s.)* |
| 3d | ~~**`duration_raw` is milliseconds**~~ **CONFIRMED 2026-08-25** | `268333` for a 4:28 track. It is milliseconds. |
| 3e | **Repeat-one** — does `OneTrackMode::On == 1` actually repeat, and is setting it live on an in-use sequence safe | Yes to both. *(`--transport`: park 6 s from the end and see if the same URI restarts.)* |
| 3f | **Repeat-all** — no known primitive; needs one session watching what the play state does when a queue runs out | An observation, not a pass. *(`--transport` records state/playing at the boundary.)* |
| 3g | **Backlight / brightness** — five levels, survives a reboot | Both |
| 3h | **The 07-26 → 07-28 batch** — escape ladder, screenshot, the pager, accents, A–Z rail, the render optimisation | One clean dev boot and an eyeball |
| 3i | **GPU/EGL present path** — dev channel only, opt-in, **measured slower** | Only if you intend to re-test it |

---

## Phase 4 — measurements nobody has taken

| # | Item | Why it matters |
|---|---|---|
| 4a | **The codec question — the biggest open item in the project.** Is the CXD3778GF still clocked and biased through a Bluetooth session, for an output nobody is listening to? | The A/B is specified in [`BATTERY_BT.md`](BATTERY_BT.md). **Read-only** — see rule 2 |
| 4b | **A soak.** Nothing has ever run for hours — **first data points 2026-08-25, both good** | Memory growth, log growth within one long boot, and the art cache's first build across 304 albums are all unmeasured. Measured so far, over one ~42 min boot: RSS **39996 → 40008 KB** in 737 s (VSZ flat at 144488) and the log **did not grow at all** over 261 s idle with the screen off (10580 B at both samples). Nothing like a leak, but this is tens of minutes, not hours, and mostly idle |
| 4c | **Boot time and battery life against stock** | This is goal #1's entire claim, and it has never been measured |
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

## Recording results

Append findings to [`DEVICE_TESTS.md`](DEVICE_TESTS.md) in the style of its "RESULTS 2026-08-17"
section — what was run, what happened, and what it settles — and update the status column of
whatever table above the item came from. An item that passes should stop being on this list; an item
that fails should gain a root cause, not just a retry.
