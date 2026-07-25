# PlayerService IPC — RE findings (playback control + now-playing + queue)

How Cinder drives playback and reads live now-playing state, for Option B. **Best news:
this is a clean, *exported* C++ client API** — unlike BtTransmitter (manual vtable), we just
link `libPlayerServiceClient.so` + `libPlayerServiceClientUtil.so` and call the methods. The
client classes marshal to the `IPlayerService` binder proxy internally.

## Bootstrap
```cpp
auto* svc  = pst::services::playerservice::PlayerService::GetInstance();   // singleton (new 0x24)
auto  ctrl = svc->getPlayController("<name>");   // -> shared_ptr<PlayController> (0x4c), registered in svc
ctrl->Connect(listener_or_null);                 // AddListener + subscribe(proxy v+0x1c); stores listener@+0x30
```
A `PlayController` holds: the `IPlayerService` binder proxy @+0x14, the `PlayerService` @+0x10,
a session handle @+0x34, the listener @+0x30. Every method = `(*(proxy_vtbl[off]))(proxy,
&req, &resp)` internally — we don't replicate it, we call the exported wrapper.

## Control API (all exported, call directly)
| Method | Effect | Notes |
|---|---|---|
| `ChangePlayState(playstate_t)` | play / pause / stop | enum: 0,1,2 are the real states; **3–6 are rejected** by the wrapper (`if (s-3u<4) noop`). Confirm exact 0/1/2 ↔ stop/play/pause on device. |
| `NextTrack()` / `PrevTrack(PrevTrackOption*)` | track skip | proxy v+0x38 / next slot |
| `NextGroup()` / `PrevGroup(PrevGroupOption*)` | **album/group skip** | this is the shuffle-/queue-by-album primitive |
| `SeekTime(media_origin_t, int ms)` | seek | origin enum (begin/cur); proxy v+0x48 |
| `SetTrackSequence(shared_ptr<TrackSequence>)` | load the queue | see queue model below |
| `SetTrackSequenceWithDpcMode(seq, dpc_mode_t)` | load queue + DPC | DPC = digital-pure/cross? |
| `Suspend()` / `Resume()` / `ClosePlayer()` | pause engine / teardown | |
| `SetPlaySpeed` / `SetPartialMode` / `SetSilenceEnabled` | A-B / speed / gapless tweaks | |
| `GetCurrentStatus(PlayStatus&)` | **one-shot now-playing snapshot** | the poll path (no listener needed) |
| `HasSetList(bool*)` | is a queue loaded | |

## The queue model (`libPlayerServiceClientUtil.so`)
- `util::NodeTrackSequence<util::UriInfo>` is the concrete `TrackSequence`: a **tree of
  `Node<UriInfo>`** (each `UriInfo` = a file URI). Build the tree (group nodes = albums, leaf
  nodes = tracks), wrap in `NodeTrackSequence(unique_ptr<Node>, startIdx, onUpdate_fn)`, hand to
  `SetTrackSequence`.
- `Node<UriInfo>`: ctor `(UriInfo, int)`, `InsertChildAmongOriginalPosition`, `GetItemCount`,
  permutation helpers (`SetupPermutation`/`AdjustPermutationForInsert`) → **this is where shuffle
  lives** (a permutation over the node's children; group-level permutation = shuffle-by-album).
- `NodeTrackSequence::SetOneTrackMode(OneTrackMode)` = repeat-one; `MoveTo(int)`,
  `GetCurrentIndexes(int*,int*,uint*)` = current group/track/flags.
- `psk::FileUtil::CloneUriNode(...)` builds nodes from files.

## Live now-playing data
Two paths:
- **Poll:** `GetCurrentStatus(PlayStatus&)` each second/frame. It calls proxy v+0x28 to fill an
  `IPlayerService::PlayStatus` then `ConverPlayStatus` → `pst::playservice::PlayStatus`.
- **Push:** implement `pst::playservice::PlayEventListener` (abstract; not exported) and pass it
  to `Connect`. The controller forwards `OnPlayStatusUpdated(uint, PlayStatus&)` and
  **`OnPlayTimeUpdated(int cur, int total)`** (the position/duration in ms) to it. Position
  almost certainly comes from `OnPlayTimeUpdated`, so for a live progress bar we either implement
  the listener or find a position field in PlayStatus.

### `PlayStatus` layout (from `ConverPlayStatus`, IPlayerService::PlayStatus → pst::)
Flat struct: a run of u32/u64 numeric fields + **one `std::string` @ pst+0x6c (from I+0x30)** —
the track URI/path — + a trailing bool @ pst+0x78. Field copy map (src I-offset → dst pst-offset):
`0→0, 4→8, 8→0xc, 0xc→0x14, 0x10→0x18, 0x14→0x30, 0x18→0x38, 0x1c→0x3c, 0x20→0x44(8), 0x28→0x4c(8),
0x30→0x6c(string), 0x3c→0x54(8), 0x44→0x5c(8), 0x4c→0x64, 0x50→0x68, 0x54→0x78(bool)`. So PlayStatus
carries: state + several indices/ids + codec/format numerics + 64-bit fields (likely durations /
sizes) + the URI string. Exact semantics per field = a small follow-up (decompile the server's
PlayStatus fill, or read fields empirically on device). **Title/artist/album/art are NOT obviously
in here** beyond the URI → they come from the library DB by URI/key ⇒ that's the MediaStore RE next.

## What this unlocks (goals 2 & 3)
- **Real transport** (play/pause/seek/next/prev) → wire to the Now Playing controls + hardware keys.
- **Real now-playing** (state + position + the playing track's URI) → feed `cinder_set_now_playing`.
- **Queue + shuffle-by-album** → `NodeTrackSequence` (group permutation) + `NextGroup/PrevGroup`.

## Integration shape
A thin **C++ `cinder-audio`** shim (libc++, links `libPlayerServiceClient*`), exposed to the
Rust UI through the same C-FFI pattern as `cinder-ffi`:
- `audio_play()/pause()/stop()` → `ChangePlayState(...)`; `audio_next()/prev()` → `NextTrack/PrevTrack`;
  `audio_seek(ms)` → `SeekTime`; `audio_next_album()` → `NextGroup`.
- A 1 Hz poll of `GetCurrentStatus` (or the listener) → push title/position/state into
  `cinder_set_now_playing(...)`. Title/artist/art resolved via MediaStore (next RE).

## Open items
- Exact `playstate_t` values (0/1/2 ↔ stop/play/pause) and `media_origin_t`.
- PlayStatus field semantics (which offset = position, duration, codec, samplerate, bitdepth).
- Whether to poll vs. implement `PlayEventListener` (push) — need its vtable if push.
- The URI→metadata lookup ⇒ **MediaStoreService RE (`libMediaStoreServiceClient.so`)** = next.

## Artifacts
`player.c` (Ghidra decomp, this dir). `libPlayerServiceClient.so` imported in
`artifacts/ghidra_appmgr`. Reuse `DecompileByName.java`.
