# cinder-audio — Sony-service C-ABI shims (playback, effects, analyzer, power)

Thin C++ shims that wrap Sony's **exported** service clients and expose flat **C ABIs** so the rest
of Cinder (Rust `cinder-ffi` / the `cinder-home` C++ shell) drives the device without any libc++
types crossing the boundary. All four shims are **compiled clang `-stdlib=libc++`, linked into
`cinder-home`, and pass the GLIBC-2.23 gate + qemu construction preflight** (see `../cinder-home`).

> Feature status (functional / partial / stationary) is in **`../cinder-home/STATUS.md`**; the RE
> detail (symbols, offsets, sizes) is in **`../analysis/RE_playerservice_sound.md`**.

## The shims

| Shim | C ABI header | Wraps | Status |
|---|---|---|---|
| `player_shim.cpp` | `cinder_audio.h` | `libPlayerServiceClient.so` (PlayerService / PlayController) | ✅ transport + now-playing URI; ◐ progress; ▢ play-by-index |
| `effect_shim.cpp` | `cinder_effects.h` | `libEffectCtrlDmp.so` (EffectCtrlDmp) | ✅ EQ + all Sound toggles + A/B bypass |
| `analyzer_shim.cpp` | `cinder_analyzer.h` | `libAudioAnalyzerServiceClient.so` (AudioAnalyzerService) | ✅ built, **default-OFF**, device-validate first |
| `power_shim.cpp` | `cinder_power.h` | `libPowerMgrServiceClient.so` (PowerMgrServiceClient) | ✅ battery care (Itawari) on/off |

**SAFETY model (all shims):** every entry point is a Sony-service call, so the shell invokes it
behind its crash+hang guard (`run_guarded`). Anything we **construct** (`new EffectCtrlDmp`,
`new PowerMgrServiceClient`) reserves ≥ the RE-confirmed device object size with a `static_assert`
(the heap-overflow brick care); factory-returned pointers (`GetInstance`/`getPlayController`) are
Sony-allocated and need no sizing. PlayerService/Effect/Power methods are **non-virtual exported
members** (called by symbol — no vtable to reproduce); the analyzer is the one exception (a faithful
`IEventListener` vtable, RE-verified slot order `[~dtor, del-dtor, OnLevelUpdate, OnSpectrumUpdate]`,
dlopen-resolved).

### player_shim (`cinder_audio.h`) — ✅ built & linked
- `GetInstance()` → `getPlayController(char const*)` → `Connect(NULL)` = **poll mode** (no listener vtable).
- Transport: `ChangePlayState(playstate_t)`, `NextTrack`/`PrevTrack`, `NextGroup`/`PrevGroup`
  (**album skip = shuffle-by-album primitive**), `SeekTime`.
- `GetCurrentStatus(PlayStatus&)` → `cinder_audio_current_uri` reads the track URI (libc++
  `std::string` @ **+0x6c**, RE-confirmed; `PlayStatus` reserves `_opaque[256]` ≫ the ~124 B
  `ConverPlayStatus` write-extent — no stack smash). The string is explicitly destructed each poll
  (the freed-heap fix).

### effect_shim (`cinder_effects.h`) — ✅ EQ + Sound + A/B
`set_eq` (10-band), `set_dsee_hx`, `set_vpt`, `set_dc_phase`, `set_dynamic_normalizer`,
`set_vinylizer`, `set_clearaudio_plus`, `set_bt_audio_effect`, and `set_bypass` (A/B compare =
`DisableSoundEffects` / `ReenableSoundEffects`). `EffectCtrlDmp` ≈ 8 B, reserve `0x10` + static_assert.

### analyzer_shim (`cinder_analyzer.h`) — ✅ built, default-OFF
Real audio-reactive visualiser source: registers an `IEventListener`, forwards `OnSpectrumUpdate`
(Sony's already-FFT'd bands) → `cinder_set_spectrum`. **dlopen-based** (a missing `.so` just disables
it — never a link dependency of the boot binary; the analyzer thread self-masks SIGALRM so it can't
steal the shell watchdog). Gated by `/contents/cinder_viz.conf: analyzer=1`; validate with
`cinder-probe --analyzer` before enabling.

### power_shim (`cinder_power.h`) — ✅ battery care
`get/set_battery_care` → `PowerMgrServiceClient::IsItawariChargingEnabled` /
`EnableItawariCharging`. Itawari = Sony "considerate" charging (caps ~90%). Object 8 B, reserve `0x10`.

## Open items (still need device / Ghidra)
1. **PlayStatus position/duration** offsets (only the URI @ +0x6c is mapped). The progress bar is
   currently a live play-through *estimate* (DB duration + a local play-clock in cinder-ffi); RE'ing
   the real position offset would make it seek-accurate.
2. **`SetTrackSequence` / NodeTrackSequence** — play an arbitrary selected track/album (the no-op
   `Select` on a library row); see `../analysis/RE_playerservice_sound.md`.
3. **Enum calibration on device**: `playstate_t` 0/1/2 ↔ stop/play/pause; analyzer `mode_t`
   (LEVEL/SPECTRUM); VPT/DC-Phase mode/type values; effect band/gain ranges.

## Build
Driven by `../cinder-home/build.sh [stable|dev]` (clang `-stdlib=libc++` + the device
`libc++.so.1`/`libcxxrt.so.1` — already in the repo at `../analysis/ramdisk/lib`, no device pull —
and `-L …/vendor/sony/lib` linking the service libs). Not built standalone.
