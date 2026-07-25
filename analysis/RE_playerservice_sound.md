# RE findings — PlayerService / Sound / BT APIs (offline, 2026-06-26)

Static RE (objdump/nm/c++filt) of the extracted Sony libs in
`artifacts/rootfs_mnt/vendor/sony/lib/`, to unblock the device-gated Cinder features. All of
these are **non-virtual exported member functions callable by mangled symbol** from a
clang/libc++ shim (same pattern as `cinder-audio/src/player_shim.cpp`), unless noted as a
callback interface. Object SIZES must be reserved like `easel_abi.hpp` (the 2026-06-25 sizing
bug) before `new`-ing any of these.

---

## 1. PlayStatus layout — CONFIRMED (Now Playing progress + scrobble)

`PlayerService::ConverPlayStatus(IPlayerService::PlayStatus const& src, pst::playservice::PlayStatus& dst)`
@ `libPlayerServiceClient.so:0x3fc8` copies the wire struct → the client `PlayStatus` our shim
reads. Disassembly gives the **client `pst::playservice::PlayStatus` field offsets** (dst):

| dst offset | from src | type | notes |
|---|---|---|---|
| +0x00 | src+0x00 | u32 | field A |
| +0x08 | src+0x04 | u32 | field B |
| +0x0c | src+0x08 | u32 | field C |
| +0x14 | src+0x0c | u32 | field D |
| +0x18 | src+0x10 | u32 | field E |
| +0x30 | src+0x14 | u32 | field F |
| +0x38 | src+0x18 | u32 | field G |
| +0x3c | src+0x1c | u32 | field H |
| +0x44..0x53 | src+0x20 | 4×u32 | block (vld1/vst1) |
| +0x54..0x63 | src+0x3c | 4×u32 | block |
| +0x64 | src+0x4c | u32 | |
| +0x68 | src+0x50 | u32 | |
| **+0x6c** | src+0x30 | **std::string** | **URI — CONFIRMS the shim's `uri @ +0x6c`** ✓ |
| +0x78 | src+0x54 | bool | flag (`!=0 → 1`) |

**Semantics still need 1 short pass** (which u32 is playstate / current-ms / total-ms). Two ways:
- Disassemble the SERVICE side (`libSoundServiceFw`/the service that fills `IPlayerService::PlayStatus`), or
- Prefer the **listener** (next section): position/duration arrive as plain ints, no offset-guessing.

## 2. PlayEventListener vtable — MAPPED (event-driven Now Playing; battery-efficient)

`PlayController::Connect(PlayEventListener*)` accepts a listener; the `On*` handlers are thin
**forwarders** to `listener (this+0x30)->vtable[slot]`:

| listener vtable slot | offset | signature (from the forwarder) | source fn |
|---|---|---|---|
| 0,1 | +0x0,+0x4 | `~dtor` (D1/D0) | — |
| **2** | **+0x08** | `onPlayStatusUpdated(uint state, PlayStatus const&)` | `OnPlayStatusUpdated` @0x3568 |
| **3** | **+0x0c** | `onPlayTimeUpdated(int currentMs, int totalMs)` | `OnPlayTimeUpdated` @0x3618 |
| 4 | +0x10 | (unmapped — likely onError/onEndOfList) | |
| **5** | **+0x14** | `onNextTrack(change_track_mode_t)` | `OnNextTrack` @0x38c0 |
| **6** | **+0x18** | `onPrevTrack(change_track_mode_t)` | `OnPrevTrack` @0x3968 |

→ Implement a tiny `PlayEventListener` subclass (reserve its real size; provide the vtable), pass
it to `Connect()` instead of `NULL`. Then **`onPlayTimeUpdated` gives position+duration directly**
(progress bar), `onPlayStatusUpdated`/`onNextTrack` signal track changes (re-resolve now-playing).
Callbacks arrive on a binder thread → update shared state under the existing cinder-ffi mutex.

## 3. Play-by-track — REUSABLE (the big playback gap; `Action::PlayIndex`)

`SetTrackSequence(shared_ptr<TrackSequence>)` @0x3244 only ships `TrackSequence+0x4` (an int
handle) over IPC; the service then **pulls tracks by calling back** into the TrackSequence
(`PlayController_Alloc`/`AllocNext`/`OnNextTrack`/`OnPrevTrack` family). So `TrackSequence` is a
callback object — but **Sony ships a concrete one we can reuse**:

`pst::services::playerservice::util::NodeTrackSequence<UriInfo>` in **`libPlayerServiceClientUtil.so`**:
- ctor `NodeTrackSequence(unique_ptr<Node<UriInfo>>, int startIndex, function<void(UpdateReason,int)>)`
- it already implements `Alloc/AllocNext/OnNextTrack/OnPrevTrack/CreateTrack` etc.

Build the `Node<UriInfo>` tree two ways:
- **JSON (simplest):** `NodeJsonUtil<UriInfo,UriInfoPolicy>::ConvJsonStringToNode(std::string)` →
  `unique_ptr<Node<UriInfo>>`. Build a JSON playlist string (schema includes `"children"`; full
  schema = 1 short `ConvNodeToJson` disasm pass, or an on-device round-trip via `ConvNodeToJsonString`).
- **C++ API:** `Node<UriInfo>::Node(UriInfo, int id)` + `root->InsertChildAmongOriginalPosition(pos, unique_ptr<Node>, bool)`.
  Needs `UriInfo`'s layout (no exported ctor; small struct ≈ `{std::string uri; …}` — get from the
  `Node<UriInfo>::Node(UriInfo,int)` disasm).

**Recipe:** build Node tree from the chosen track URI(s) → `make_shared<NodeTrackSequence<UriInfo>>(move(node), startIdx, cb)`
→ `PlayController::SetTrackSequence(seq)` → `ChangePlayState(Play)`. Closes play-a-selected-track/album.

## 4. EQ + ALL Sound effects — COMPLETE API (`libEffectCtrlDmp.so`)

`pst::services::sound::EffectCtrlDmp` — **default ctor** `EffectCtrlDmp()` @0xdd40 (just construct it;
reserve its real size). Setters map 1:1 to the Cinder Sound + EQ screens:

| Cinder control | EffectCtrlDmp call |
|---|---|
| **10-band EQ** (our EQ screen) | `SetEq10Band(bool on)`, `SetEq10BandValue(Eq10Band band, int gain)` |
| 6-band EQ / presets | `SetEq6Band`, `SetEq6BandPreset(Eq6BandPreset)`, `SetEq6BandValue(Eq6Band,int)` |
| DSEE HX | `SetDseeHx(bool)`, `SetDseeHxCustom(bool)`, `SetDseeHxCustomMode(DseeHxCustomMode)`, `SetDseeAi(bool)` |
| VPT Surround | `SetVpt(bool)`, `SetVptMode(VptMode)` |
| Dynamic Normalizer | `SetDynamicNormalizer(bool)` |
| DC Phase Linearizer | `SetDcPhaseLinearizer(bool)`, `SetDcPhaseFilterType(DcPhaseFilterType)` |
| Vinyl Processor | `SetVinylizer(bool)`, `SetVinylizerType(uint)` |
| ClearAudio+ | `SetClearAudioPlus(bool)` |
| Tone Control | `SetToneControl(bool)` |
| **Effects → Bluetooth (goal #7!)** | **`SetBtAudioSoundEffect(bool)`** |
| disable/re-enable all | `DisableSoundEffects()`, `ReenableSoundEffects()` |

→ Build `cinder-audio/src/effect_shim.cpp` wrapping `EffectCtrlDmp` behind a C ABI; wire the EQ
screen's `Action::EqChanged` → `SetEq10BandValue`, Sound toggles → the matching setters. **Goal #7
("apply effects to Bluetooth") is a single call: `SetBtAudioSoundEffect(true)`.**
Still need: the `Eq10Band` enum (10 band indices) + gain units/range — short ctor/setter disasm,
or on-device probe.

## 5. USB-DAC → LDAC (headline feature) — PCM entry point found

`libBtPlayerService.so`:
- `BtPlayerServiceClient::SetLDAC(bool)` — enable LDAC codec
- `BtPlayerServiceClient::SetLDACBufferControl(BtLdacControl)` — buffer/quality
- `BtPlayerService::LdacWriteSound()` — **the PCM-write entry into the LDAC encoder** (the inverse
  the CLAUDE.md Part H5 "Approach A" was missing)
- `BtTransmitterServiceClientFactory::CreateInstance()` (`libBtTransmitterService.so`) — client factory

So the USB-DAC→LDAC bridge = tap USB-DAC PCM (`UsbDeviceAudioPlayerService`) → feed
`LdacWriteSound()` with LDAC enabled. Still device-gated for the PCM tap + the E4/E5 ALSA-topology
confirmation (CLAUDE.md Part H6), but the BT-side entry point is now identified.

## 6. Volume — CXD3778GF "master volume" (mechanism found)

No master-volume API in PlayerService/SoundService. `libaudiohal-adleralsa.so` (the CXD3778GF
wrapper) strings: **"master volume"**, "analog playback mute", `/sys/module/snd_soc_cxd3778gf/parameters/…`.
→ Main 3.5 mm volume is the **CXD3778GF codec master volume**, set via the **ALSA mixer control**
("master volume" on `card0`) or the sysfs parameter. Wire `Action::VolUp/VolDown` to an ALSA mixer
set (libasound `snd_mixer_*`) or a sysfs write. Exact control name / range = on-device `amixer
scontrols` (E4). (Hardware vol keys may also be handled below us — confirm with `getevent` whether
they even reach userspace.)

## 7. Visualiser — Sony's built-in spectrum analyzer (REAL audio-reactive bars)

`libAudioAnalyzerServiceClient.so` → `pst::services::audioanalyzerservice::AudioAnalyzerService`
is a **complete spectrum-analyzer service** (the stock player's spectrum screen uses it — confirmed:
`HgrmMediaPlayerApp` imports the exact same 7 symbols). Sony does the FFT itself and pushes
per-band magnitudes to a listener, so we get a real visualiser with **no FFT cost on our side**.

**Client API (all RE-confirmed exported symbols):**
- `AudioAnalyzerService::GetInstance()` → singleton (Sony-allocated → sizing rule N/A)
- `SetMode(mode_t)` — `mode_t{LEVEL=0, SPECTRUM=1}` (inferred: listener slot order Level<Spectrum +
  the `audioanalyzer_params/` file naming `delay_normal_{level,spectrum}_<rate>_<bits>.txt`;
  spectrum exists for `normal` playback only, not `dacmode`). Probe sweeps it if wrong.
- `SetUpdateRate(float)` · `SetCalcSamples(unsigned)` · `SetPassband(vector<Passband>)` (optional —
  omit to inherit the stock screen's defaults; `Passband` layout not needed for the default path)
- `Start(IEventListener*)` · `Stop()` · `Terminate()`

**The one ABI reproduced — `IEventListener` vtable.** From `libAudioAnalyzerService.so`
(`AudioAnalyzerServiceServiceImpl`'s secondary base at +4, `_ZThn4_` thunks), the listener
sub-object vtable is exactly the standard libc++ layout:
`[0]=~dtor(D1) · [1]=deleting dtor(D0) · [2]=OnLevelUpdate(vector<int>const&) · [3]=OnSpectrumUpdate(vector<int>const&)`.
A faithful C++ re-declaration (virtual dtor, then the two virtuals in declaration order) reproduces
it; **verified in the built binary** — `CinderListener` vtable @ offset-to-top 0, slot2=OnLevelUpdate,
slot3=OnSpectrumUpdate. `Start` only stores the pointer; the analyzer thread later calls
`listener->vtable[3](spectrum)`. The dtor slots are never called by Sony (we own a static listener).

**Implementation (shipped):** `cinder-audio/src/analyzer_shim.cpp` + `include/cinder_analyzer.h` —
**dlopen-based** (NOT a link dep, so a missing/renamed `.so` just disables the feature instead of
breaking cinder-home's dynamic load), forwards `OnSpectrumUpdate` → `cinder_set_spectrum()`
(cinder-ffi, `spectrum::from_bands`: resample to 36 bars + auto-gain, dB-or-linear). **Default OFF**
(gated by `/contents/cinder_viz.conf: analyzer=1`); started in `deferred_up()` behind `run_guarded`,
stopped on background/finalize. Validate on device first: `cinder-probe --analyzer` (reports frame
flow + raw band range for calibrating `from_bands`). Fallback path for a raw-PCM tap with no
analyzer (e.g. USB-DAC): `cinder_set_pcm()` (our own radix-2 FFT, `spectrum::levels`).

---

## What's now wireable vs still device-gated

| Feature | Status | Next step |
|---|---|---|
| Now Playing progress + track-change | **wireable** | implement PlayEventListener (vtable mapped) |
| Play a selected track/album (`PlayIndex`) | **wireable** | NodeTrackSequence + ConvJsonStringToNode + SetTrackSequence |
| EQ → DSP, all Sound toggles | **wireable** | effect_shim.cpp over EffectCtrlDmp |
| Effects on Bluetooth (goal #7) | **wireable** | `SetBtAudioSoundEffect(true)` |
| Volume | mechanism found | ALSA mixer "master volume" / sysfs — confirm control name on device |
| USB-DAC → LDAC (headline) | entry point found | PCM tap + `LdacWriteSound`; E4/E5 ALSA topology on device |
| Real audio-reactive visualiser | **wireable (shipped, default-OFF)** | analyzer_shim over AudioAnalyzerService; `cinder-probe --analyzer` to validate + calibrate, then `cinder_viz.conf: analyzer=1` |
| Battery care (charge limit) | **NOT wired** (UI label only) — mechanism found | `PowerMgrServiceClient::EnableItawariCharging(bool)` + `IsItawariChargingEnabled()` (§9) |

## 11. Bullet-proofing audit (2026-06-29, recursive memory/crash sweep)

Deep pass for any path that could crash/corrupt on device. Findings:

| Area | Result |
|---|---|
| **Framebuffer blit** | **BUG FIXED** — wrote `(page*H+y)*stride+copy_bytes` for all H rows×pages with no `map_len` bound; a panel geometry where `pages*H > yres_virtual` (rotated/different unit) → OOB write past the mmap. Now bounded per-row against `map_len` (clips, never overruns). Confirmed device (480×800, virtual 2400) fits exactly. |
| **PlayStatus stack object** | SAFE — `GetCurrentStatus`→`ConverPlayStatus` highest write to dst = `[r4,#120]` (URI std::string @ +0x6c..+0x77, one byte @ +120) ⇒ real ≈124 B vs our `_opaque[256]` reserve (>2×). No stack smash. |
| Rust render indexing | SAFE — library loops are `scroll..len` (empty-range-safe when stale), play uses `.get()`, eq/sound/viz/settings cursors clamped, Album `flat.get()` None handled, `set_library` resets lib cursor/scroll. |
| Canvas draw primitives | SAFE — `put`/`blend`/`fill_solid` all clamp to `[0,W)×[0,H)`; embedded-graphics clips to target. No OOB canvas writes. |
| DB (malformed MTPDB.dat) | SAFE — `build_library` uses `unwrap_or_default()` throughout; query errors propagate as `Result` (no panic) → empty/partial library. |
| C++ config/log parsing | SAFE — all `fgets(_, sizeof, _)` / `fread(_,1,sizeof-1,_)` / `strtol` with bounds; no gets/scanf/strcpy. evdev `code` bounds-checked before `g_keymap[]`. |
| Concurrency / panics | SAFE — single-threaded Rust core; analyzer thread self-masks SIGALRM; guard thread-owner check; `panic=abort` ⇒ no poison cascade. |

## 12. Dev/stable channels + adb (implemented 2026-06-29)

Two builds from ONE tree, selected by a single flag (`build.sh stable` | `build.sh dev`):
- **stable** (default): the lean player. Firmware row reads "CINDER 1.0 · RUST". No adb.
- **dev**: cargo `dev` feature flips the marker to "CINDER DEV · RUST" (so you can tell them apart
  on-device), and `-DCINDER_DEV` makes the dev binary **self-enable adb at boot** (in `deferred_up`,
  behind `run_guarded`): `setprop sys.usb.config mtp,adb` + `persist.sys.usb.config` + `start adbd`.
  Artifacts: `cinder-home/dist/{stable,dev}/` (binaries + the channel-agnostic install/uninstall .UPGs).

Why self-enable, not a ramdisk init hook: adbd IS in the stock firmware (Wampy uses `adb`; the
`sys.sony.config` USB modes include `adb`; `uac` mode = `audio_func,adb`), but the scrobbler/wampy
persistent-service pattern **modifies the boot ramdisk's init.rc** (repacks the boot image) — the
project's single biggest brick risk. The dev binary already runs at boot, so it enables adb itself:
touches NO boot-critical files, fully guarded (a failure = no adb, player runs exactly like stable),
and `persist.sys.usb.config` makes adb come up early on later boots → a **brick-recovery channel**
(`adb shell touch /contents/cinderhome_off` reverts without wbrt). The exact property mechanism is
confirmed on the first dev flash; refine the one `std::system(...)` line in main.cpp if needed.
Prereqs: MTK/adb USB driver on Windows; usbipd-win for WSL passthrough (CLAUDE.md Part F). Security:
root adb = anyone with USB gets root — fine for the dev unit; the stable channel ships without it.

## 10. Static-settings / unwired-feature audit (2026-06-29)

A sweep of every UI control for "looks live but does nothing." Status after this session:

| Control | Was | Now | Mechanism / next step |
|---|---|---|---|
| Settings ▸ Theme | wired | wired | internal |
| Settings ▸ Visualiser type/anim | wired | wired | internal |
| Settings ▸ **Battery care** | static "LIMIT 90%" | **WIRED** (On/Off) | PowerMgrServiceClient::EnableItawariCharging (§9) |
| **Sound screen** (DSEE/Vinyl/VPT/DC-Phase/Normalizer/ClearAudio+) | **display-only** (whole screen) | **WIRED** (6 On/Off toggles) | effect_shim/EffectCtrlDmp; VPT/DC-Phase *mode/type* still on/off-only (enum values TBD on device) |
| Sound screen ▸ **A/B compare** | n/a | **WIRED** (Option toggles A↔B) | EffectCtrlDmp Disable/ReenableSoundEffects → cinder_effects_set_bypass; instant DSP on/off listen test |
| EQ bands/preset | wired | wired | effect_shim |
| Volume (Vol±) | emits action, carry_out TODO | **still TODO** | CXD3778GF "master volume" ALSA mixer / sysfs — exact control name device-gated (§6, E4) |
| Play a selected track/album (PlayIndex) | emits action, carry_out TODO | **still TODO** | NodeTrackSequence<UriInfo> + ConvJsonStringToNode + SetTrackSequence — RE'd, needs player_shim impl + device test |
| Bluetooth on/off | emits BtToggle, FFI `continue` | **still UI-only** | BtTransmitterService SetCurrentSource/SetLdac (Part H4) |
| Settings ▸ USB mode | static, EnterUsbMsc TODO | **still TODO** | setprop sys.sony.config msc/uac (Part H4) |
| Settings ▸ Storage | static "12.4/16 GB" | **WIRED** (real `statvfs`) | shell `report_storage()` → `cinder_set_storage`; info row (no drill-in) |
| Up Next screen | static `data::SONGS` render | **WIRED** (real) | `nav::now_playing_queue` = current album from the library; auto-scrolls to the playing track |
| Settings ▸ Brightness / Screen-off timer | static | **still static** | backlight sysfs / appmgr power policy (device-gated) |
| Settings ▸ Database REBUILD | static | **still static** | triggers Sony MTP re-index (complex; deferred) |
| Settings ▸ Firmware / Model | static | static (honest info labels — not fake toggles) | could read real fw/NVP; low value |

The two egregious fake *features* (battery care; the entire Sound screen) are now real. The rest are
either device-gated (volume/play-index/USB/brightness need on-device validation) or honest info labels.

## 9. Battery care — Itawari charging (✅ WIRED 2026-06-29; mechanism below)

NOW IMPLEMENTED — Settings ▸ Battery care is a live On/Off toggle (power_shim →
`PowerMgrServiceClient`), state read at boot. This section keeps the RE detail. (It was previously a
static "LIMIT 90%" label.) The device support:

- `libPowerMgrServiceClient.so` → `PowerMgrServiceClient::EnableItawariCharging(bool const&)` and
  `IsItawariChargingEnabled()`. "Itawari" (いたわり, "considerate") is Sony's battery-care charging
  that caps charge at ~90% to preserve longevity.
- It is an **On/Off toggle, not a settable percentage** — confirmed by the stock app's
  `isBatteryCareOn` / `OnBatteryCareOnOffToggled` / `updateBatteryCare` (`HgrmMediaPlayerApp`).

To wire it: (1) `PowerMgrServiceClient` is **constructed** (public ctor, no factory) → the SIZING RULE
applies — RE its ctor write-extent and reserve storage like effect_abi.hpp before `new`. (2) A small
power_shim.cpp (C ABI: `cinder_power_set_battery_care(int)` / `_get`), all behind run_guarded.
(3) Make the Settings row interactive (On/Off toggle), read initial state from `IsItawariChargingEnabled`.
The label should read "On/Off" (the 90% cap is fixed in firmware), not "LIMIT 90%".

## 8. Device-object SIZES (the sizing-brick de-risk) — RE'd from ctor write extents

`new`-ing a Sony class needs `sizeof` ≥ the real object, else the device ctor overflows (the
2026-06-25 heap-overflow brick). Sizes from disassembling each ctor's highest `str [this,#off]`:

| class | how obtained | real size | reserve | notes |
|---|---|---|---|---|
| `EffectCtrlDmp` | ctor @0xdd40 writes this+0 (impl ptr), this+4 (bool) | **≈ 8 B** | 0x10 | non-polymorphic PIMPL — trivially safe |
| `NodeTrackSequence<UriInfo>` | ctor @0xbcec writes through +0xb0 (176) | **≥ 180 B** | 0x100 | reserve like CuiAppModule |
| `Node<UriInfo>` | — | n/a | — | **Sony-allocated** via `ConvJsonStringToNode` (unique_ptr) — we never size it |
| `PlayEventListener` (impl) | we subclass it | our choice | — | we control the object; just match the vtable (§2) |

So the effect path is the safest to wire first (8-byte object). For play-by-track, reserve 0x100
for `NodeTrackSequence` and let Sony allocate the `Node` tree.

**Caution (per the 2026-06-26 soft-brick):** every one of these is a Sony-service call — wire each
behind the `run_guarded` crash+hang guard in cinder-home, off the boot path, and reserve the sizes
above (`static_assert` them, like `easel_abi.hpp`). Confirm with `cinder-probe`-style isolation
before putting any on the boot path. Effect/EQ + volume + play-by-track are all invoked from UI
actions AFTER the UI is healthy, so a bug there can't brick (the guard catches it, the UI continues).
