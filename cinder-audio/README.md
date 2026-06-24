# cinder-audio — playback control via Sony's PlayerService (C ABI shim)

A thin C++ shim that wraps Sony's **exported** PlayerService client
(`libPlayerServiceClient.so`) and exposes a flat **C ABI** (`include/cinder_audio.h`) so the
rest of Cinder (Rust `cinder-ffi` / the `cinder-home` C++ shell) drives playback without any
libc++ types crossing the boundary. This is the playback half of Option B (the library/
metadata half is the pure-Rust `cinder-db`; the UI is `cinder-ui`).

## What's verified (ground-truthed 2026-06-24)
Symbols + signatures demangled from the extracted `libPlayerServiceClient.so`, and
`src/player_shim.cpp` compiles (cross g++) emitting undefined refs that match them exactly:

- `PlayerService::GetInstance()` → `getPlayController(char const*)` → `Connect(PlayEventListener*)`
  — **`Connect(NULL)` is valid → poll mode**, so we do NOT need to implement the listener vtable.
- Transport `ChangePlayState(playstate_t)` (0/1/2 valid; 3–6 rejected by the wrapper).
- `NextTrack()` / `PrevTrack(PrevTrackOption const*)`.
- `NextGroup()` / `PrevGroup(PrevGroupOption const*)` — **album-level skip = the shuffle-by-album
  primitive** the project wants.
- `SeekTime(media_origin_t, int ms)`.
- `GetCurrentStatus(PlayStatus&)` — one-shot now-playing snapshot (poll path).
- `SetTrackSequence(shared_ptr<TrackSequence> const&)` — load the queue (queue model TBD).

All PlayController/PlayerService methods are **non-virtual exported members** (called directly
by symbol) — unlike `easel::ApplicationBase`, there is **no vtable to reproduce**, so this shim
is lower-risk than `cinder-home`.

## Files
- `include/cinder_audio.h` — the C ABI: init/shutdown, play/pause/stop, next/prev track,
  next/prev group, seek_ms, current_uri.
- `src/playerservice_abi.hpp` — hand-written declarations matching the device mangling.
- `src/player_shim.cpp` — the implementation. **Skeleton** (ABI-correct, not yet compiled with
  libc++ / linked / run).

## Open items (need RE or device)
1. **PlayStatus field layout.** It's a plain struct with NO exported accessors. Only the track
   **URI is known (libc++ `std::string` @ +0x6c)**. The **playstate / current-ms / total-ms**
   offsets need a Ghidra pass on `GetCurrentStatus()` / `OnPlayStatusUpdated()`. Until then
   `cinder_audio_current_uri` reads only the URI (and that offset read must be validated on
   device first — wrong offset = UB).
2. **Enum calibration.** Confirm `playstate_t` 0/1/2 ↔ stop/play/pause and `media_origin_t`
   begin/current on device.
3. **TrackSequence build** for `SetTrackSequence` (queue / shuffle permutation) — see
   `../analysis/G_player_ipc/RE_findings.md` (NodeTrackSequence model).

## Build (same blockers as cinder-home)
Needs **clang `-stdlib=libc++`** + the device `libc++.so.1`/`libcxxrt.so.1` (not in the extracted
rootfs — pull from device) and `-L …/vendor/sony/lib -lPlayerServiceClient`. The shim's std types
mangle `std::__1::` under libc++ (matching the device); cross g++ (libstdc++) only structure-checks.
