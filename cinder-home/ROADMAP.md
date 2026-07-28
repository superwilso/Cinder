# Cinder — Roadmap & remaining-work audit

Forward-looking companion to [`STATUS.md`](STATUS.md) (which is the *current-state* feature matrix).
This is **what's left and in what order**, written so the next working session — especially the
first one with the device — is a straight line, not a guessing game.

> **What stands between this tree and a device the owner can rely on:**
> [`../docs/PRODUCTION_READINESS.md`](../docs/PRODUCTION_READINESS.md) (2026-07-27). Short version:
> **25 commits have landed since the last hardware-verified one** (`eb07f7f`), several of them on
> the boot path; the headline LDAC feature has never been executed; and shuffle/repeat still draw
> as real while doing nothing.

Last audited: **2026-07-26** (full project audit — [`../docs/AUDIT_2026-07-26.md`](../docs/AUDIT_2026-07-26.md)).
Prior: 2026-07-25, 2026-06-30. Three commits have landed since the 07-03 round; the tree is clean
and every offline gate passes (71 host tests, 18-case launcher matrix, GLIBC ≤2.23, qemu preflight).

## Audit summary — where we are
The player is daily-usable and **all genuinely-offline work is done**. The Option-B IPC
reverse-engineering is complete and realized in code: PlayerService transport + now-playing
(`analysis/G_player_ipc/`) and the SQLite MediaStore library/metadata (`analysis/H_mediastore/`)
are both implemented (`cinder-audio` drives `PlayerService`; `cinder-db` reads `/db/MTPDB.dat`;
play-by-index is wired).

**The discovery session that used to head this roadmap has happened** — twice (2026-07-25 and
2026-07-26) — and it answered what it was meant to answer: volume is `amixer -c0 'master volume'`
0..120 (now the built-in default, no conf needed), backlight is `/sys/class/leds/lcd-backlight`,
keys are plain keyboard codes (play=28, next=106, prev=105), touch is himax on `event1` with raw
range x[0..959] y[0..1599], the real MTPDB was pulled and `cinder-db` calibrated against it, the
USB-MSC failure was root-caused to a uid-100 unmount (fixed with the `cinder-umount` setuid
helper), and the GPU/EGL hang was root-caused to four root-only device nodes (fixed with
`cinder-gpunode`, opt-in). What those sessions did **not** produce: a PlayStatus dump with music
actually playing (so the position/duration offsets are still unmapped), and any LDAC validation.

**So the shape of the remaining work has changed.** It is no longer "gather data" — it is
**verify a large batch of unverified code, then execute the headline feature.** The 07-26 brick
was recovered with wbrt, so Cinder is not currently installed and the next flash carries the whole
07-26 batch at once (type scale, fonts, GPU path, screenshot, escape ladder, gpunode).

> ### ⇒ THE next action (2026-07-26): probe, reinstall, then run the LDAC test.
> Step 3 is the one that matters most — goal #3 is the reason this project exists and it has
> **never been run end to end**, despite the RE being complete and the bridge building. It runs
> under **stock** firmware, so it does not depend on steps 1–2 at all and can be done first if the
> device is in a stock state.

## The next device session — critical path (do in this order)
1. **Probe first — zero boot risk.** `cinder-probe` (and `cinder-probe --discover`) over adb, plus
   one `CINDER_GPU=1` run as uid 100 after granting the nodes with `cinder-gpunode`. No easel
   lifecycle → it cannot affect boot, and it de-risks the whole unverified batch.
   **Capture a PlayStatus dump with music actually playing** — the 07-25 dump was all zeros
   because nothing was playing, which is why seek-accurate progress is still blocked.
2. **Reinstall Cinder** (`dist/dev/`): push **three** binaries — `cinder-home`, `cinder-umount`,
   `cinder-gpunode` — then flash the install `.UPG` and boot **with the cable out** (a cable at
   boot is itself the escape to stock). Confirm: paint, library load, counter cleared, and eyeball
   the new type scale + non-Latin rendering. See STATUS.md STEP 2.
3. **Run `ldac-bridge/TEST.md`** — the headline feature, still 0% validated. Its two unknowns
   (does `SetCurrentSource(true)` open the server socket; is `hw:4,0` capture `-EBUSY`) each have a
   documented next step in the three-outcome table. Runs under stock; independent of steps 1–2.
4. **Validate, then flash `dist/stable/`** for daily use (no adb, lean).

The probe-first gradient (STATUS.md STEP 1/2) applies more strongly than on any previous flash —
never repoint the `.appcfg` before the probe run looks clean.

## Prioritized backlog

### P0 — verify on device (code already shipped; NO rebuild)
| Item | Verify by | State |
|---|---|---|
| **The 07-26 batch** (type scale, font fallback, escape ladder, screenshot) | one clean dev boot + eyeball; `test_launcher.sh` already covers the ladder offline | code-complete, **never run on hardware** |
| **USB-DAC → LDAC** (goal #3, the headline) | `ldac-bridge/TEST.md` under stock | builds; **never executed** |
| **Play-by-index** (tap a track/album → plays) | tap a Songs row on device | wired 07-03, qemu-preflighted, unverified |
| **Touch navigation** (primary nav — no d-pad) | confirm taps land after the +2 type-scale/row-height pass | implemented ✓ (re-check after resize) |
| **Volume** (Vol± → hardware) | one audible Vol± press; defaults are baked in, conf only if wrong | **default is the discovered control** ✓ |
| **Transport-button codes** | press each button; real NW-A50 codes are already the defaults | defaults from wampy `glfw.patch` ✓ |
| **GPU present path** | `cinder-gpunode` + `CINDER_GPU=1` under `cinder-probe`, then the flag | opt-in, default OFF ✓ |
| **Night backlight level** | tune via `cinder_backlight.conf` if the auto-detected level is wrong | scaffolded ✓ |

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
- **Remaining inert controls** (deliberate, tracked): FM / BT Receiver / Pairing screens (backends
  not RE'd, P2 below); the EQ footer's **"Save Sound Preset"** — the EQ already persists
  automatically on every change, so this button has nothing to do; either reword it or give it a
  real named-preset store; the Now Playing heart (no "liked" store yet).
- **Seek-accurate progress** — prefer the **`PlayEventListener`** route
  (`OnPlayTimeUpdated(cur,total)` @slot+0xc, already mapped in `analysis/G_player_ipc/`): it is
  event-driven, battery-efficient, and needs **no new RE**, unlike the byte-offset route which is
  still blocked (the 07-25 PlayStatus dump was all zeros because nothing was playing). The
  play-clock estimate is fine meanwhile.
- **Repo hygiene** — 18 build outputs are tracked, including two ~5 MB `.unstripped` ELFs that
  churn on every build (`.git` is 116 MB). The `dist/` artifacts arguably belong (they are what
  gets flashed); the `.unstripped` pair and loose `*.o` files are pure churn.

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
    four nodes — no argv, no environment, fixed path list, `lstat`-guarded so a planted symlink is
    rejected. Safe test path = run the helper, stop cinder-home, run `cinder-probe` with
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
- **The unverified 07-26 batch** — the single biggest risk. Type scale, font fallback, GPU path,
  screenshot hooks, the rewritten escape ladder and `cinder-gpunode` all reach hardware for the
  first time on the same flash, so a misbehaving boot has a whole day of changes as its bisect
  surface. Probe first; the escape ladder's rung 0 (cable at boot) depends on nothing and is the
  backstop. *(Resolved since the last audit: adb enumerated and was driven directly from the host
  on 07-25, and `amixer` is confirmed present — it produced the discovery dump.)*
- **`/contents` is fragile** — vfat, no journal, and it is the partition handed to the PC for
  USB-MSC. Repeated auto-MSC cycling corrupted it on 07-26 and that is what bricked the device.
  Cinder state now lives on `/data` (ext4); avoid gratuitous MSC cycling during dev sessions.
- **deferred_up timeout budget** — many guarded calls (db/audio/battery/EQ/sound/storage/discovery/
  adb/analyzer); fast on a healthy device, but if one Sony service is wedged the boot UI freezes for
  that call's budget before the guard recovers. Watch the logs; trim budgets if needed.
- **PlayStatus `_opaque[256]`** is a generous reserve (real ≈124 B); if a future fw enlarges it,
  re-confirm before reading new offsets.

## Recent bug audit (2026-06-30 session)
- **Fixed:** the sleep-timer countdown was coupled to the position-estimate clock anchor
  (`last_pos` reset on track change), making it drift slightly long; decoupled (anchor is now
  touched only by `clock_tick`).
- **Verified clean:** action-code space is consistent across FFI / `cinder.h` / `carry_out` (no
  collisions, all handled, code 9 intentionally unused); the new C++ (volume/backlight/discovery)
  is all guarded + bounded with no panic/overflow risk; no compiler warnings beyond the benign
  `-stdlib` one; GLIBC ≤2.23 + guard self-test + qemu preflight all pass on both channels.
