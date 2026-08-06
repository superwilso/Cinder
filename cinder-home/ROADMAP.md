# Cinder — Roadmap & remaining-work audit

Forward-looking companion to [`STATUS.md`](STATUS.md) (which is the *current-state* feature matrix).
This is **what's left and in what order**, written so the next working session — especially the
first one with the device — is a straight line, not a guessing game.

Last audited: **2026-07-25** (full project audit). Prior: 2026-06-30. STATUS.md is current to
**2026-08-05 (eleventh round — bug + usability audit)**; the device-session critical path below is
unchanged by that round, which was entirely offline.

> **2026-08-05 — what the audit round changed, and what it adds to the device session.**
> Fixes: rewind/seek (◁ semantics + a scrubbable progress rail), the user queue made playable, six
> Shelf defects, BT volume re-assert on reconnect, and the render loop's constant-load "gets hot"
> floor. Plus a UI-scale slider. Full detail in STATUS.md's eleventh-round TL;DR. Host tests: 95
> green (was 71).
>
> **Three new things to verify in the SAME device session** (all cheap, none blocking):
> - **Seek**: confirm `PlayController::SeekTime(Begin, ms)` actually moves playback (the rail and ◁
>   both route through it; it was never called before, so it is RE'd but unexercised).
> - **BT link detection**: `cinderhome.log` prints `bt: link detection = …` at first poll. If it
>   says `NONE`, capture `ls /sys/class/bluetooth` and `hcitool con` so the detector can be taught
>   the right path. Everything else degrades cleanly (the screen says "link state unavailable").
> - **Framebuffer pages**: the blit now writes only the displayed page (yoffset is pinned to 0 on
>   every flip, so pages 1–2 were never scanned) — ~3× less memory traffic per frame. If the panel
>   tears or flickers, `touch /contents/cinder_fb_allpages` restores the old behaviour; the mode is
>   logged at fb open.


## Audit summary — where we are
Per the STATUS.md matrix: the player is daily-usable and **all genuinely-offline work is done**
(host tests green: 63 UI + 22 FFI + 8 DB + 2 font; qemu preflight passes; both channels' `.UPG`s
packed in `dist/` — note the packed artifacts predate the 2026-08-05 round and need a rebuild). The Option-B IPC reverse-engineering that used to be "next" is **complete and realized
in code**: PlayerService transport + now-playing (`analysis/G_player_ipc/`) and the SQLite
MediaStore library/metadata (`analysis/H_mediastore/`) are both implemented (`cinder-audio` drives
`PlayerService`; `cinder-db` reads `/db/MTPDB.dat`; **play-by-index is wired**, not a gap anymore).

What remains is **device-gated**: it needs real values from the hardware (control names, byte
offsets, keycodes, ALSA topology) that the **discovery probe** captures in one run. Each remaining
item is therefore either (a) **scaffolded** — activates with a config drop, no rebuild — or
(b) **needs code wired** from the captured data (a dev rebuild). Nothing is blocked on design.

> ### ⇒ THE single next action (2026-07-25): run one device session.
> Everything below funnels to this. Flash `dist/dev/` once, capture the discovery dump, drop 2–3
> config files, verify. That one session unblocks volume, keymap, seek-accurate progress, the
> LDAC-bridge validation, album-art schema, and playlists — in a single pass. See the critical
> path immediately below; nothing here needs more offline design or RE.

## The device session — critical path (do in this order)
1. **Flash `dist/dev/` once.** Brings up adb, auto-writes `/contents/cinder_discovery.txt`, and logs
   raw key codes to `cinderhome.log` as you press buttons.
2. **Gather data:** play a track (for the PlayStatus dump), press each physical button (keycodes),
   then `adb pull /contents/cinder_discovery.txt`. Or run `cinder-probe --discover` for the full
   isolated capture incl. the 12 s keymap window.
3. **Config drops — activate, no rebuild (P0):**
   - `cinder_volume.conf`  ← amixer master control name + range (or CXD3778GF sysfs) → Vol± work.
   - `cinder_keymap.conf`  ← the real GPIO keycodes from the dev keycode log → buttons map correctly.
   - `cinder_backlight.conf` (optional) ← tune the night level / node (auto-detected otherwise).
4. **Wire from data — needs a dev rebuild (P1):** play-by-index + seek-accurate progress (below).
5. **Validate, then flash `dist/stable/`** for daily use (no adb, lean).

The bad-boot counter + probe-first gradient (STATUS.md STEP 1/2) still apply — never repoint the
`.appcfg` before the probe run looks clean.

## Prioritized backlog

### P0 — confirm/activate on device (data from discovery; NO code rebuild)
| Item | Confirm/activate with | State |
|---|---|---|
| **Touch navigation** (the primary nav — no d-pad) | confirm taps land + the `EVIOCGABS` x/y range is right (discovery dumps it); nudge per-screen coords only if off | implemented ✓ (device-calibration) |
| **Volume** (Vol± → hardware) | `cinder_volume.conf` (control/path + range) | scaffolded ✓ |
| **Transport-button codes** (Play/Prev/Next/Vol/Power) | `cinder_keymap.conf` (codes from the dev keycode log) | mechanism exists ✓ |
| **Night backlight level** | `cinder_backlight.conf` (auto-detected; tune `night`) | scaffolded ✓ |

### P1 — wire code from device data (a dev rebuild; I do this with the data in hand)
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
    binary never touches the GPU. **To enable GPU:** a setuid-root helper (like `cinder-umount`) that
    `chmod 0666`s those four nodes before EGL init. Safe test path = grant perms, stop cinder-home,
    run `cinder-probe` with `CINDER_GPU=1` as uid 100 (no lifecycle → no boot-counter risk), reboot
    to restore. See memory `reference_gpu_mali_stack`.
- **Play a selected track / album** — **WIRED 2026-07-03 (was the biggest gap); needs device
  verify only.** `Action::PlayIndex` → `cinder-db::album_context` → `cinder_audio_play_tracks`
  builds the JSON Node-tree (`{"uri","format","children"}`), maps formats via Sony's
  `psk::FileUtil::GetFormatFromFilename`, constructs `NodeTrackSequence<UriInfo>`, then
  `SetTrackSequence` + `ChangePlayState(Play)`. qemu-preflighted (the whole JSON→Node chain is
  built under guard canaries). Remaining: confirm playback starts on the unit. *(Queue-honoring —
  `SetTrackSequence` for the user swipe-queue — rides the same code, gated on device verify.)*
- **Seek-accurate progress** — replace the play-clock *estimate* with the real position. The
  discovery PlayStatus hex dump reveals the position/duration int offsets (only URI @ +0x6c mapped
  so far); then read them in `player_shim` and push via `cinder_set_position`. Alternatively
  implement the `PlayEventListener` (`OnPlayTimeUpdated(cur,total)` @slot+0xc, mapped in
  `analysis/G_player_ipc/`) for event-driven, battery-efficient updates. (Estimate is fine meanwhile.)

### P2 — device-gated, lower priority
- **Bluetooth radio on/off** — UI toggle exists; wire `BtTransmitterService` (SetCurrentSource/SetLdac).
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
- **Database rebuild** — triggers the Sony MTP re-indexer (complex).
- **Manual Brightness slider** (the static Settings row) — the backlight node is now known; make the
  row interactive and reuse `set_backlight`.

### P3 — calibration / polish
- **Analyzer `mode_t`** (LEVEL vs SPECTRUM) for the real audio-reactive visualiser — `cinder-probe
  --analyzer` confirms, then flip `/contents/cinder_viz.conf: analyzer=1`.
- **VPT / DC-Phase mode** enums (Studio/Club, Standard/Low) — on/off works; the specific mode is TBD.
- **Volume direction** — some mixer controls are attenuation (lower = louder); confirm + invert if so.
- **Now Playing sleep badge** already done; consider an auto-night-by-clock option later.

## Open risks to watch (first device session)
- **adb-enable** (`setprop sys.usb.config mtp,adb` + `ctl.start adbd`) is best-effort — if adb doesn't
  appear on the first dev flash, tweak the one `std::system(...)` in `main.cpp` (`#ifdef CINDER_DEV`).
- **amixer presence** — if absent, use the sysfs backends for volume/backlight (configs support both).
- **deferred_up timeout budget** — many guarded calls (db/audio/battery/EQ/sound/storage/discovery/
  adb/analyzer); fast on a healthy device, but if one Sony service is wedged the boot UI freezes for
  that call's budget before the guard recovers. Watch the logs; trim budgets if needed.
- **PlayStatus `_opaque[256]`** is a generous reserve (real ≈124 B); if a future fw enlarges it,
  re-confirm before reading new offsets.

## Recent bug audit (this session)
- **Fixed:** the sleep-timer countdown was coupled to the position-estimate clock anchor
  (`last_pos` reset on track change), making it drift slightly long; decoupled (anchor is now
  touched only by `clock_tick`).
- **Verified clean:** action-code space is consistent across FFI / `cinder.h` / `carry_out` (no
  collisions, all handled, code 9 intentionally unused); the new C++ (volume/backlight/discovery)
  is all guarded + bounded with no panic/overflow risk; no compiler warnings beyond the benign
  `-stdlib` one; GLIBC ≤2.23 + guard self-test + qemu preflight all pass on both channels.
