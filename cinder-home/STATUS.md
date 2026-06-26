# Cinder — status & flash/verify guide (2026-06-25)

This is the hand-off after the autonomous integration session. It tells you **what works**,
**how to flash and verify it**, **how to tune the input keymap**, and **what still needs the
device** (RE follow-ups). Read the "Flash it" section first.

---

## TL;DR — what changed this session

1. **Root-caused & fixed the boot crash.** cinder-home was crashing in
   `CuiAppModule::OnInitialize` (`[this+0x18]=0x12`). Cause: `easel_abi.hpp` declared the
   `ApplicationBase`/`CuiAppModule` device classes with **no storage**, so `new`/stack
   under-allocated and the device ctor overflowed memory. Fixed by reserving the real
   footprint (sizes read from the Ghidra decompiles) + `static_assert`. **cinder-home now
   constructs cleanly** — proven under qemu against the device's own libs.
2. **Two offline gates** (no device needed), wired into `build.sh`:
   - `tools/preflight_qemu.sh` — constructs the real device objects under qemu with guard
     canaries; catches std::function-ABI / ctor-signature / object-SIZE regressions before you
     ever flash.
   - `tools/pack_upg.sh` — reproducible `.UPG` packer (refreshes `dist/`).
3. **The UI is now data-driven & daily-usable** (all verified by rendering PNGs offline):
   real library browsing with **windowed scrolling**, a **volume HUD**, an **interactive EQ**,
   **Bluetooth on/off**, a **built-in scrobbler**, and a **complete input/now-playing pump**.

## What works (once flashed)

- Boots as the Home app (replaces the stock Qt UI) and completes the easel handshake.
- Lock screen → unlock → **Now Playing** showing the **real current track** (resolved from the
  library DB by URI).
- **Transport**: play/pause, next/prev track, next/prev **album** (shuffle-by-album primitive).
- **Library browse**: Songs/Albums/Artists/Playlists, real data, **scrolls** (thousands of rows),
  grouped album headers, distinct per-album art (hashed gradient until real thumbnails decode).
- **Volume HUD** on Vol±, **night mode** toggle, **EQ** band editing, **Bluetooth** toggle.
- **Scrobbler**: writes `/contents/.scrobbler.log` (Audioscrobbler/1.1) as you listen.
- **Safety nets unchanged**: bad-boot counter auto-reverts to stock after 3 failed boots;
  USB-connected-at-launch runs stock; `/contents/cinderhome_off` disables; uninstaller restores.

## What does NOT work yet (needs the device + Ghidra — see "RE follow-ups")

- **Starting a NEW selection from the library** (tap a track/album to play it). The transport
  works on whatever is *already* queued, but jumping to an arbitrary track needs
  `PlayController::SetTrackSequence` RE'd. `Select` on a library row is currently a no-op.
- **Volume keys driving the hardware** (the HUD shows, but the level may need SoundService).
- **Now Playing progress bar** (position/duration; PlayStatus layout not RE'd → shows 0).
- **EQ/Sound effects reaching the DSP** (the UI edits them; wiring to libSoundServiceFw pending).

---

## Flash it

From `/home/sony/sony`, with the Walkman plugged in:

```bash
tools/flash.sh --push cinder-home/dist/cinder-home          # push the binary to /contents
tools/flash.sh cinder-home/dist/cinder_home_install.upg     # install (repoints the Home app)
# UNPLUG, power on, let it boot. (USB-connected-at-launch deliberately runs STOCK for recovery.)
```

To read the log afterward, plug back in (it boots stock when USB-connected) and:

```bash
tools/flash.sh --cat cinderhome.log
```

To revert at any time:

```bash
tools/flash.sh cinder-home/dist/cinder_home_uninstall.upg   # restores the stock .appcfg
# or, no PC: create /contents/cinderhome_off over USB-MSC and reboot
# or worst case: wbrt restore (Windows)
```

## Verify it

`cinderhome.log` should now progress **past** the old crash point and show the lifecycle
running. Look for, in order:

```
[cinder-home] main: start
[cinder-home] main: constructing CuiAppModule
[cinder-home] main: calling app.run()
[N| …|EASL|LifeCycleManager.cc:91] start ToInitialize
[cinder-home] app:OnForeground            <-- NEW: we now get here (was crashing before this)
[cinder-home] bring_up: cinder_render_init
[cinder-home] bring_up: cinder_db_open(/db/MTPDB.dat)
[cinder-home] cinder-ffi: library loaded — NNN tracks, MM albums, KK artists
[cinder-home] bring_up: cinder_scrobble_open(/contents/.scrobbler.log)
[cinder-home] bring_up: cinder_audio_init
[cinder-home] bring_up: DONE (renderer ready)
[cinder-home] input: opened P /dev/input/event* node(s)
```

On the **panel** you should see the Cinder UI painting (Now Playing). If the screen paints but
buttons do nothing, that's the **keymap** (next section), not a crash.

If it crashes, the log ends with `*** FATAL SIGNAL : PC=0x… ***` + a backtrace + `/proc/self/maps`
— paste that; the PC minus the library's map base gives the exact function (same method that
found the sizing bug).

## Tune the input keymap (likely needed on first boot)

The NW-A50 buttons are GPIO keys with **device-specific raw codes**. cinder-home ships sensible
Linux defaults, but they probably won't match. To calibrate without a rebuild:

1. Get a shell (adb, or stock + a terminal) and run `getevent -lt /dev/input/event*`, pressing
   each physical button; note the **device** and the **decimal key code** for each.
2. Create `/contents/cinder_keymap.conf` (over USB-MSC) with one `rawcode button` per line, where
   `button` is: `0`=Up `1`=Down `2`=Left `3`=Right `4`=Select `5`=Back `6`=Option `7`=Play
   `8`=Home `9`=VolUp `10`=VolDown `11`=Power. Example:
   ```
   # rawcode  button
   115 9      # vol+  -> VolUp
   114 10     # vol-  -> VolDown
   163 3      # FF    -> Right (next track on Now Playing)
   165 2      # REW   -> Left  (prev track)
   164 7      # play  -> Play
   ```
3. Reboot. The pump logs `input: applied /contents/cinder_keymap.conf overrides`.

---

## RE follow-ups (device-gated; ordered by daily-use impact)

These are the only things between "browsable + transport works" and "fully replaces stock".
Each needs the device and a Ghidra pass on the named library. See the project memory for detail.

1. **`PlayController::SetTrackSequence(shared_ptr<TrackSequence>)`** in
   `libPlayerServiceClient.so` — how to build a TrackSequence so `Select` on a library row
   actually starts that track/album. **Highest impact.** Wire into
   `cinder-audio/src/player_shim.cpp` + `cinder_audio_play_object(id)`, then carry out
   `CINDER_ACT_PLAY_INDEX` in `cinder-home/src/main.cpp`.
2. **Volume** — find the volume set/get path (SoundService or system); wire `CINDER_ACT_VOLUP/DOWN`.
3. **`PlayStatus` field offsets** (`GetCurrentStatus`) in `libPlayerServiceClient.so` — playstate
   / current-ms / total-ms, for a real progress bar + accurate scrobble timing. Then pass real
   `progress`/`playing` into `cinder_set_now_playing_uri`.
4. **EQ/DSEE → DSP** via `libSoundServiceFw.so` — make `Action::EqChanged` + the Sound screen
   toggles reach the hardware.

## Where things live

| Piece | Path |
|---|---|
| easel shell + pump + input/keymap | `cinder-home/src/main.cpp` |
| device-class ABI (sized!) | `cinder-home/src/easel_abi.hpp` |
| build + gates | `cinder-home/build.sh`, `cinder-home/tools/{preflight_qemu,pack_upg}.sh` |
| flashable artifacts | `cinder-home/dist/` |
| Rust UI (screens, nav, model, overlay) | `player/cinder-ui/src/` |
| C-ABI render/input/scrobble bridge | `player/cinder-ffi/src/` (`lib.rs`, `scrobble.rs`) |
| library DB reader | `player/cinder-db/src/lib.rs` |
| playback control shim | `cinder-audio/src/` |
| host PNG preview | `cd player && cargo run -p cinder-host` → `player/out/*.png` |
