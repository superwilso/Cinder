# Cinder — Roadmap & remaining-work audit

Forward-looking companion to [`STATUS.md`](STATUS.md) (which is the *current-state* feature matrix).
This is **what's left and in what order**, written so the next working session — especially the
first one with the device — is a straight line, not a guessing game.

> **What stands between this tree and a device the owner can rely on:**
> [`../docs/PRODUCTION_READINESS.md`](../docs/PRODUCTION_READINESS.md). Short version: **33 commits
> have landed since the last hardware-verified one** (`eb07f7f`), several of them on the boot path;
> the headline LDAC feature has never been executed; and shuffle/repeat still draw as real while
> doing nothing.

Last audited: **2026-07-28** (this pass). Prior: 2026-07-26
([`../docs/AUDIT_2026-07-26.md`](../docs/AUDIT_2026-07-26.md)), 2026-07-25, 2026-06-30.
**34 commits have landed since the 07-26 audit** — the file below had drifted badly enough to be
misleading in places, which is why this pass exists. The tree is clean and every offline gate
passes: **157 host tests**, the **24-case** launcher recovery matrix, GLIBC ≤2.23, qemu preflight,
both channels packed.

## Audit summary — where we are

**Playback works.** That is the headline change since the last audit and it happened on hardware
(2026-07-27): position advancing 1000 ms/s, listener callbacks once a second with real position and
duration, `ALSA pcm4p` RUNNING, and the PCM device that opens is `hw:0,4` =
`cxd3778gf-icx-lowpower` — so 3.5 mm already takes the low-power hardware S-Master path rather than
the CPU. Three bugs had to fall, and the first was the important one: **nothing drove
`pst::core::Framework`'s event looper**, so every PlayerService out-param was uninitialised stack
and the service logged nothing at all. Wampy's `pstserver` drives the same loop the same way. The
other two: SoundService's single "Music" track leaks into `hagodaemon` if a process exits without
`ClosePlayer`, and `SetTrackSequence` leaves the OMX graph at Idle where Idle → Executing is
illegal (so `play_tracks` now goes Pause → Play).

The Option-B IPC reverse-engineering is complete and realized in code: PlayerService transport +
now-playing (`analysis/G_player_ipc/`) and the SQLite MediaStore library/metadata
(`analysis/H_mediastore/`) are both implemented.

**The device state is not recorded, and the next session should assume nothing.** Cinder was
installed and booting during the 2026-07-27 session (a mid-session bad boot reverted to stock and
came back to Cinder, which is the escape ladder working). Which build is on it was never written
down, and **33 commits have landed since the last hardware-verified one** (`eb07f7f`). Reflash
rather than reason about it.

**So the shape of the remaining work is unchanged in kind but larger in size.** It is still *verify
a large batch of unverified code, then execute the headline feature* — the batch is now everything
from 07-26 through 07-28 rather than one day's work. What no session has yet produced: a PlayStatus
dump with music actually playing, any LDAC validation, and a single `cinder-probe --analyzer` run.

> ### ⇒ THE next action (2026-07-28): LDAC test, probe, reflash — in that order.
> The LDAC test is first because it is the reason this project exists, it has **never been run end
> to end**, and it runs under **stock** firmware — so it carries no boot risk and depends on nothing
> else here. Everything after it is verification of code that is already written.

## The next device session — critical path (do in this order)
1. **Run `ldac-bridge/TEST.md`** — the headline feature, still 0% validated. Runs under stock, so it
   is independent of everything below and carries no boot risk. Its two unknowns (does
   `SetCurrentSource(true)` open the server socket; is the USB-DAC capture `-EBUSY`) each have a
   documented next step in the three-outcome table.
2. **Probe — still zero boot risk.** `cinder-probe`, `--discover`, `--pump`, and `--analyzer` over
   adb. No easel lifecycle → it cannot affect boot, and it de-risks the whole unverified batch.
   Two specific captures matter:
   - **A PlayStatus dump with music actually playing.** Every previous dump was all zeros because
     nothing was, which is why the byte offsets are still unmapped.
   - **`--analyzer`, which has never been run once.** As of 2026-07-28 it should finally produce
     frames: Cinder was never calling `SetPassband`, and the service reports nothing until it is
     told which bands to analyse. If it still emits nothing, the visualiser has a second cause.
3. **Reflash `dist/dev/`** and boot **with the cable out** (a cable at boot is itself the escape to
   stock). Push `cinder-home` and `cinder-umount`; `cinder-gpunode` is dev-only now and is only
   needed if you intend to re-test the GPU path, which measures slower than software. Confirm:
   paint, library load, counter cleared, and eyeball the type scale and non-Latin rendering.
4. **Soak it.** Nothing has ever run for hours. Memory growth, log growth within one long boot, and
   the art cache's first build across a 304-album library are all unmeasured — as are boot time and
   battery against stock, which are goal #1's entire claim.
5. **Validate, then flash `dist/stable/`** for daily use (no adb, lean, and no setuid GPU helper).

The probe-first gradient (STATUS.md STEP 1/2) applies more strongly than on any previous flash —
never repoint the `.appcfg` before the probe run looks clean.

## Prioritized backlog

### P0 — verify on device (code already shipped; NO rebuild)
| Item | Verify by | State |
|---|---|---|
| **USB-DAC → LDAC** (goal #3, the headline) | `ldac-bridge/TEST.md` under stock | builds; **never executed** |
| **The 07-26 → 07-28 batch** — type scale, font fallback, escape ladder, screenshot, the pager, accents, idle screen-off, brightness, boot-to-stock, A–Z rail, the render optimisation | one clean dev boot + eyeball; `test_launcher.sh` covers the ladder offline | code-complete, **33 commits deep, never run on hardware** |
| **Analyzer emits frames** — `SetPassband` was missing until 07-28 | `cinder-probe --analyzer`; **never run once** | the likely root cause is fixed; unverified |
| **Play-by-index** (tap a track/album → plays) | tap a Songs row on device | wired 07-03, qemu-preflighted, unverified |
| **Drag-to-seek** — `media_origin_t::Begin == 0` is the last unverified value in that path | drag the rail, confirm it lands where dropped | wired 07-27, unverified |
| **`duration_raw` is milliseconds** | the diagnostic in `1ccb7bc` settles it on the next boot | assumed |
| **Idle screen-off wakes reliably** | blank it, wake by touch and by Power | a failed wake is indistinguishable from a dead device |
| **Touch navigation** (primary nav — no d-pad) | confirm taps land after the type-scale/row-height pass | implemented ✓ |
| **Volume** (Vol± → hardware) | one audible Vol± press; defaults are baked in | **default is the discovered control** ✓ |
| **Transport-button codes** | press each button; real NW-A50 codes are the defaults | defaults from wampy `glfw.patch` ✓ |
| **Backlight / brightness at boot** | the row cycles 5 levels and survives a reboot | wired 07-27, unverified |
| **GPU present path** | dev channel only now; `cinder-gpunode` + `CINDER_GPU=1` under `cinder-probe` | opt-in, default OFF, **measured slower** ✓ |

### P1 — offline work that no longer needs the device
- ~~**Playlists**~~ — **DONE 2026-07-26.** Schema RE'd offline against the pulled DB
  (`artifacts/MTPDB_dev.dat`): no playlist table; playlists are containers in a second object tree
  with `object_type = 3` membership rows referencing tracks. `Db::playlists()` /
  `Db::playlist_tracks()` added, wired through `build_library`, and a playlist row now plays the
  whole list in saved order via the existing pending-play channel (no new FFI symbol, no C++
  change). Schema + the two traps (decoy `.m3u8` rows, 96% orphaned entries) are written up in
  `analysis/H_mediastore/RE_findings.md`. Device-verify rides with play-by-index.
- ~~**The four "Shuffle …" bands are inert**~~ — **DONE 2026-07-26**, along with a full
  touch-input sweep that found six more drawn-but-inert or mis-hit controls (EQ preset pills
  selected the *wrong* preset; the EQ raise/lower split was 55px off the drawn zero line; Up Next
  rows weren't tappable; a tap anywhere on the USB-DAC screen engaged the headline feature). All
  fixed with shared render/hit-test geometry + regression tests — see
  [`../docs/AUDIT_2026-07-26.md`](../docs/AUDIT_2026-07-26.md) §F6b.
- ~~**Seek-accurate progress**~~ — **DONE 2026-07-27** via the `PlayEventListener` route, as this
  entry recommended. `onPlayTimeUpdated(cur_ms, total_ms)` fires about once a second; cinder-ffi
  interpolates between updates and re-anchors on each one, so the bar follows seeks, mid-track
  starts and wrong tag durations — none of which the old local play-clock could. Drag-to-seek rides
  on it. The byte-offset route is no longer needed for this and stays blocked.
- ~~**The Now Playing heart**~~ — **DONE 2026-07-27.** Liked songs are a real store
  (`cinder_liked.conf`, object ids) with a TSV export.
- ~~**Shuffle and repeat on Now Playing**~~ — **DONE 2026-07-28.** Shuffle reorders the queue
  Cinder builds (tapped track first); repeat is two real states wired to `SetOneTrackMode`, since
  repeat-**all** has no known primitive. Device-verify: does `OneTrackMode::On == 1` actually repeat,
  and is setting it live on an in-use sequence safe. **Repeat-all** stays open — the shape would be
  to detect end-of-queue and re-issue the sequence, which needs one device session to observe what
  the play state actually does when a queue runs out.
- **Remaining inert controls** (deliberate, tracked): FM / BT Receiver / Pairing (backends not wired, P2 below),
  Settings ▸ Database, and the EQ footer's **"Save Sound Preset"** — the EQ already persists on
  every change, so that button has nothing to do; reword it or give it a real named-preset store.
- **Repo hygiene — now materially worse, and worth doing.** `.git` has grown from 116 MB to
  **418 MB**. Two ~5 MB `.unstripped` ELFs are tracked and rewritten on every single build, as are
  the stripped binaries beside them; that is the churn. The `dist/` artifacts arguably belong (they
  are what gets flashed) and `.crt223/*.o` are *inputs*, not outputs — they are the glibc-2.23 crt
  files the toolchain needs, and must stay. Untracking the `.unstripped` pair and the loose build
  outputs stops the growth; shrinking what is already there needs a history rewrite, which is a
  separate decision.

### Reference — device-verify items (detail; tracked in the P0 table above)
- **GPU present path (EGL/GLES2 on Mali)** — **DONE in code 2026-07-26; needs device verify.**
  cinder-ffi's frame present was a software framebuffer blit (mmap + 3× memcpy + `FBIOPUT` force-
  flip, no vsync). It is now EGL + GLES2: upload the software-rasterized `Canvas` to one RGBA
  texture, draw a full-screen quad, `eglSwapBuffers` (Mali fbdev does the page-flip + vsync
  internally). Driver = `libMali_linux.so` (Mali-450 r0p0, glibc build); linked `-l:libMali_linux.so`.
  Rasterization stays on CPU — this offloads *presentation* + gives vsync pacing, and is the
  foundation for GPU transitions/scaling later. **Safety:** `GlPresenter::open` returns Err on any
  EGL failure → falls back to the software framebuffer, so no black-screen risk; `CINDER_GPU=0`
  forces software. **Device-gated unknown:** whether uid-100/CapEff-0 cinder-home may open the Mali/
  M4U device nodes (Sony's Home app does GPU, so likely yes). If EGL init fails in `cinderhome.log`,
  the software path still renders — check the log line "GPU present path active" vs "GPU init failed".
  Code: `player/cinder-ffi/src/gpu.rs`.
  - **DEVICE RESULT 2026-07-26:** root `egl_test` proved the Mali stack works (65fps vsync, EGL 1.4),
    but uid-100 cinder HANGS in EGL init — it's blocked on four ROOT-ONLY device nodes (`/dev/ion`,
    `/dev/mtkfb_vsync`, `/dev/mtk_disp`, `/dev/sw_sync`); it can only open the `system`-owned `mali`
    + `fb0` (hence software works). The hang wedged the boot → bad-boot revert. **Fix shipped:** GPU
    is now **opt-in, default OFF** via `/contents/cinder_gpu_on` (or `CINDER_GPU=1`); the default
    binary never touches the GPU. **The enabling helper now exists:** `cinder-gpunode`
    (`src/cinder-gpunode.c`, setuid-root, built + staged in `dist/`) `chmod 0666`s exactly those
    four nodes — no argv, no environment, fixed path list. *(2026-07-28: it was `lstat`-guarded and
    that guard did not work — `chmod()` re-resolves the path and follows symlinks, so it was a
    textbook setuid TOCTOU. It is now `O_PATH|O_NOFOLLOW` + `fstat` + a chmod through
    `/proc/self/fd`, which binds the check and the change to one inode. It also no longer ships on
    the stable channel: a setuid-root binary that world-opens graphics nodes, in service of a path
    that is default-off and 4.7× slower, does not belong on the daily-use build.)*
    Safe test path = run the helper, stop cinder-home, run `cinder-probe` with
    `CINDER_GPU=1` as uid 100 (no lifecycle → no boot-counter risk), reboot to restore.
    **Security trade-off:** 0666 on those nodes world-opens graphics memory and display control —
    acceptable on a single-user player, but it is a real loosening of kernel device permissions.
    See memory `reference_gpu_mali_stack`.
  - **DEVICE RESULT 2026-07-26 (bench):** GPU present measures WORSE than software in every
    config (ms/present, `cinder-probe --bench gpu`): poke+interval-1 **45.6**, poke+interval-0
    **55.5**, no-poke **24.0** but the panel never updates (mtkfb needs the FBIOPUT poke; swap
    alone reaches no glass). Software present: **9.6**. The cost is `FBIOPUT_VSCREENINFO`
    contending with the Mali pipeline, not vsync stacking (interval 0 made it worse). GPU path
    stays opt-in — worth revisiting only with a pan-display or real MTK overlay path.
    **Superseding fix, same day:** the **present thread** (`cinder-ffi/src/present.rs`, ON by
    default, escape `/contents/cinder_nothread`) overlaps raster with present — measured
    **9.55 ms/frame pipelined ≈ 105 fps ceiling** on the software path (pump caps at 60). The
    escape-ladder contract survives: submit blocks on a wedged present thread so the per-frame
    `alarm(8)` still fires, and "first frame painted" health now gates on
    `cinder_frames_presented()` (completed presents, not submissions).
- **Play a selected track / album** — **WIRED 2026-07-03 (was the biggest gap); needs device
  verify only.** `Action::PlayIndex` → `cinder-db::album_context` → `cinder_audio_play_tracks`
  builds the JSON Node-tree (`{"uri","format","children"}`), maps formats via Sony's
  `psk::FileUtil::GetFormatFromFilename`, constructs `NodeTrackSequence<UriInfo>`, then
  `SetTrackSequence` + `ChangePlayState(Play)`. qemu-preflighted (the whole JSON→Node chain is
  built under guard canaries). Remaining: confirm playback starts on the unit. *(Queue-honoring —
  `SetTrackSequence` for the user swipe-queue — rides the same code, gated on device verify.)*

### P2 — device-gated, lower priority
- **Bluetooth radio on/off** — UI toggle exists; wire `BtTransmitterService` (SetCurrentSource/SetLdac).
- **Volume must become route-aware BEFORE BT ships.** `apply_volume()` writes
  `amixer -c0 'master volume'`, a CXD3778GF codec register — and the BT transmit path never touches
  that codec (decode → we `write()` raw PCM into the `GetSocketName` AF_UNIX socket → the MTK BT
  chip). So the volume keys would silently do **nothing** on Bluetooth. Sony has a layer Cinder does
  not: `pst::services::volume::VolumeService::SetVolume(unsigned)` + `VolumeCondition` (the route) +
  `AvlsCondition` (regional loudness cap); one volume goes in and the service picks the DAC register
  or `BtTransmitterService::SetCurrentVolume(uint8_t)` (AVRCP Absolute Volume, 7-bit → 0..127).
  Ship this with the BT client, not after — the first BT build without it looks broken.
  - Granularity, since it comes up: 128 AVRCP steps is already *finer* than the wired 0..120. The
    coarseness users feel is the headphones quantising 0..127 into their own 16 or 32 internal
    steps. Cinder is the **PCM producer** for the BT pipe, so a digital pre-scale before the socket
    write gives finer steps than any protocol limit (half-steps included) — cheap for a
    fraction-of-a-dB trim, expensive if used for the whole range, because it spends bit depth.
    If the sink reports no absolute-volume support (`IsSupportedAbsoluteVolume()`), Sony injects
    AVRCP VOLUME_UP/DOWN through `/dev/uinput` and the step size is entirely the headphones' —
    there the pre-scale is the only lever. Detail: `../docs/PRODUCTION_READINESS.md` §B4.
- **BT transmit codec — live apply.** The selector (LDAC/aptX HD/aptX/SBC + LDAC quality), the
  device-wide preference, and its persistence (`cinder_settings.conf` + `cinder_bt.conf`) are **DONE
  in the UI/shell**. Remaining = the live `BtTransmitterService` apply (SetLdac/SetAptxHD/SetSbc +
  SetLdacSoundQuality) via the C++ BT client shim (same boundary as `ldac-bridge`).
- **USB-DAC → LDAC** (the headline) — the **UI/toggle/engage-signalling are DONE**: the USB-DAC
  screen routes input to 3.5 mm + BT/LDAC, the shell starts the bridge (`/contents/ldac_on`) + UAC
  `setprop`, and it never disconnects BT. Remaining = on-device: the `ldac-bridge` daemon capturing
  card4 → the LDAC socket, the E4/E5 ALSA confirm, and validating the UAC switch live. See
  `ldac-bridge/TEST.md` + `analysis/RE_playerservice_sound.md §5`.
- **USB-mode switch** (enter MSC) — wired to **Settings ▸ USB mode** (`setprop sys.sony.config msc`,
  guarded; disruptive — validate live).
- **FM radio / BT receiver / Pairing** screens — Sony tuner/BT services (currently static).
- **FM radio, and FM → Bluetooth** — designed end to end 2026-07-28, none of it built. Full write-up
  with the evidence: [`../docs/COMPARISON_cinder_wampy_sony.md`](../docs/COMPARISON_cinder_wampy_sony.md).
  Short version: the Si4708 is a V4L2 *control* device with no ALSA symbols, so its audio is analog
  into the codec's `'analog input device'` mux (item #1 is `tuner`) → ADC → **`hw:0,1`**
  (`/dev/snd/pcmC0D1c`, a real capture PCM that wampy already records from on hardware). From there
  it is PCM in the SoC and the existing `ldac-bridge` shape applies — the capture scanner currently
  *skips* card 0 on purpose, so FM is a flag rather than a rewrite. The 3.5 mm cable is the
  **antenna**, not the audio path, so a bare extension lead works while audio leaves over BT.
  Order: needs the BT client (above) first. Three traps wampy hit first are documented there — a
  Sony service re-disables the mux on a timer, ALSA control events cannot be watched with
  `poll`/`epoll` on this device, and the power state drops to `mem` without a `/sys/power/wake_lock`.
- **Database rebuild** — triggers the Sony MTP re-indexer (complex).
- ~~**Manual Brightness slider**~~ — **DONE 2026-07-27.** The Settings row cycles 5 levels as a
  percentage of the node's own `max_brightness`, is persisted and applied at boot, and level 1 is
  15% rather than 0 so the screen you would use to undo it stays readable. `cinder_backlight.conf`'s
  `day=` overrides it (and, since 2026-07-28, actually does).

### P3 — calibration / polish
- **Analyzer: verify it now emits frames.** `cinder-probe --analyzer` has still never been run. It
  matters more than it did: 2026-07-28 found that Cinder never called `SetPassband`, and the service
  reports nothing until it is told which bands to analyse — very likely the whole reason no frame
  has ever been seen. The twelve stock passbands are set now, the values are mapped in dB rather
  than linearly, and 12 bands interpolate up to 36 bars instead of stepping. All of that is
  unverified on hardware. (The old note here said to "flip `analyzer=1`" — the analyzer defaults ON
  since 2026-07-27 and that file now only turns it OFF.)
- **VPT / DC-Phase mode** enums (Studio/Club, Standard/Low) — on/off works; the specific mode is TBD.
- **Volume direction** — some mixer controls are attenuation (lower = louder); confirm + invert if so.
- **Now Playing sleep badge** already done; consider an auto-night-by-clock option later.

## Open risks to watch (next device session)
- **The unverified 07-26 → 07-28 batch** — the single biggest risk, and it has grown from one day
  to three. **33 commits since the last hardware-verified one**, and several are on the BOOT PATH:
  brightness applied at boot, the idle screen-off timer, the render-loop rate change, the analyzer's
  demand-start, auto-MSC gating, the dark-panel paint skip, and the art bake. A misbehaving boot has
  three days of changes as its bisect surface. Probe first; the escape ladder's rung 0 (cable at
  boot) depends on nothing and is the backstop, and Settings ▸ Boot to stock is now a cable-free
  rung above it.
- **A Rust panic is a reboot.** `panic = "abort"`, so any panic kills the process, appmgr calls
  `android_reboot`, and the bad-boot counter takes a life — four of them revert to stock. The panic
  message does reach `cinderhome.log` via the launcher's stderr redirect, but there is no hook
  recording which screen and state it happened in. Every new arithmetic and indexing site was swept
  on 2026-07-28; the sweep is not a substitute for the hook.
- **`/contents` is fragile** — vfat, no journal, and it is the partition handed to the PC for
  USB-MSC. Repeated auto-MSC cycling corrupted it on 07-26 and that is what bricked the device.
  Cinder state now lives on `/data` (ext4); avoid gratuitous MSC cycling during dev sessions.
- **deferred_up timeout budget** — many guarded calls (db/audio/battery/EQ/sound/storage/discovery/
  adb/analyzer); fast on a healthy device, but if one Sony service is wedged the boot UI freezes for
  that call's budget before the guard recovers. Watch the logs; trim budgets if needed.
- **PlayStatus `_opaque[256]`** is a generous reserve (real ≈124 B); if a future fw enlarges it,
  re-confirm before reading new offsets.

## Recent bug audits

### 2026-07-28 — brick sweep, render profiling, and a comparison against wampy/stock
- **The album art was the whole cost of a frame, not the visualiser.** Measured, not guessed
  (`cargo test -p cinder-ui --release --test render_bench -- --ignored --nocapture`): the visualiser
  cost ~30 µs and the art behind it ~8,000. The gradient recomputed a float divide and a `sqrt` per
  pixel across 230,400 pixels, every frame; `draw_image` blitted pixel-by-pixel through a
  bounds-checked `put`. Now a 512-entry ramp table, a highlight that only computes inside its own
  disc, a gradient baked once per track into the slot a decoded cover would occupy, and row-slice
  blits. **~430 µs a frame whatever the track has, from 1,080 (with art) and 8,300 (without)** —
  roughly 16 ms → 6 ms on device. The frame is now present-bound, not raster-bound.
- **This closes the GPU question rather than reopening it.** Software present is 9.6 ms and the
  present thread overlaps it with the raster, so the ceiling is ~104 fps against a 60 fps pump. The
  Mali path measures 45.6 ms/present. Cheaper raster cannot change that.
- **Fixed, all brick-adjacent:** a 1.5 MB scratch `Canvas` allocation introduced by the optimisation
  itself (the exact size whose churn already caused one on-device allocator abort); per-frame
  `format!`/`to_uppercase()` on the audio pages; a per-frame `clock_gettime` in `viz_decay`; a page
  swipe using `y < BOT` where it needed a range, which would have turned every ABS_Y-less contact
  into a page turn instead of a track skip; an empty-vector index in `cinder_bench`; and an
  unresolved track URI redrawing its gradient every frame.
- **Comparison against wampy and stock found three real defects** — see
  [`../docs/COMPARISON_cinder_wampy_sony.md`](../docs/COMPARISON_cinder_wampy_sony.md). Cinder never
  called `SetPassband`, so the analyzer had nothing to report; the spectrum was mapped linearly when
  the data spans three decades; and 12 bands were bucket-averaged into 36 bars. It also confirmed
  Cinder's volume is **correct as-is** — the perceptual curve lives in region-selected DAC gain
  tables below the mixer, so writing the raw 0..120 step is exactly what stock does.
- **`cinder-gpunode` setuid TOCTOU** fixed, and the binary removed from the stable channel.

### 2026-07-27 — playback, and a dead-UI sweep
- **Playback fixed on hardware** (the Framework pump — see the audit summary above).
- **Fabricated state removed:** a hardcoded `WH-1000XM5` shown as a connected BT device in three
  places, mock Menu subtitles read as fact, and invented Settings values on rows that did nothing.
- **Fixed:** the clock and battery were hardcoded literals on 14 of 16 screens; a wall charger could
  hand the library to it as USB mass storage; playback started on the wrong track (index shift);
  truncated URIs were queued as valid; five separate fixed-frame-rate assumptions; `redirect_fds`
  silently leaving the log holding `/contents`; 16-bit PNGs decoding to noise and palette PNGs never
  appearing at all.

### 2026-06-30
- **Fixed:** the sleep-timer countdown was coupled to the position-estimate clock anchor
  (`last_pos` reset on track change), making it drift slightly long; decoupled (anchor is now
  touched only by `clock_tick`).
- **Verified clean:** action-code space is consistent across FFI / `cinder.h` / `carry_out` (no
  collisions, all handled, code 9 intentionally unused); the new C++ (volume/backlight/discovery)
  is all guarded + bounded with no panic/overflow risk; no compiler warnings beyond the benign
  `-stdlib` one; GLIBC ≤2.23 + guard self-test + qemu preflight all pass on both channels.
