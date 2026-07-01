# Cinder — Roadmap & remaining-work audit

Forward-looking companion to [`STATUS.md`](STATUS.md) (which is the *current-state* feature matrix).
This is **what's left and in what order**, written so the next working session — especially the
first one with the device — is a straight line, not a guessing game.

Last audited: 2026-06-30.

## Audit summary — where we are
Per the STATUS.md matrix: the player is daily-usable and **all genuinely-offline work is done**.
What remains is **device-gated**: it needs real values from the hardware (control names, byte
offsets, keycodes, ALSA topology) that the **discovery probe** captures in one run. Each remaining
item is therefore either (a) **scaffolded** — activates with a config drop, no rebuild — or
(b) **needs code wired** from the captured data (a dev rebuild). Nothing is blocked on design.

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
- **Play a selected track / album** — the biggest functional gap (`Select` on a library row is a
  no-op). Needs `PlayController::SetTrackSequence` + the `NodeTrackSequence<UriInfo>` JSON shape,
  captured live (strace PlayerService / the discovery PlayController dump). RE'd in principle; the
  exact node construction must be confirmed on device (object sizing + JSON).
- **Seek-accurate progress** — replace the play-clock *estimate* with the real position. The
  discovery PlayStatus hex dump reveals the position/duration int offsets; then read them in
  `player_shim` and push via a `cinder_set_position`. (Estimate is fine meanwhile.)

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
