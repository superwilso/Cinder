# The whole Sony DSP surface — what exists, what Cinder wires, and every enum

Recovered 2026-08-17. Method, in order of confidence:

1. **Symbols** — `EffectCtrlDmp`'s 54 exported methods, from `HgrmMediaPlayerApp`'s dynamic
   references. Gives the API exactly; gives the enum *names* but not their values.
2. **Qt translation catalogues** — `vendor/sony/translations/HgrmMediaPlayerApp_en_US.qm`, read with
   `strings -a -e b` (UTF-16BE; the default 8-bit pass finds nothing, which is why earlier sweeps of
   the libraries came up empty). These carry the option labels **in catalogue order**, which is the
   order Sony's own pickers draw them, which is almost certainly the enum order.
3. **Device read-back** — `cinder-probe --vpt` with the Framework pump running. Reads what stock
   actually left behind, which corroborates the ordering at one point per enum.

> **The read-back does NOT bound an enum.** With the pump running, every value 0..7 echoed back for
> both `VptMode` and `DcPhaseFilterType` — the service stores whatever int it is handed. An echo
> proves the call path, not the feature. Only listening settles the top of the range. Same lesson as
> the high-gain finding: a write landing is not evidence it does anything.

## The enums

Order is catalogue order. The "stock had" column is a real value read off the device, and in both
cases it is the LAST or near-last member, which is decent evidence the list is complete.

| enum | values (0-based) | stock had |
|---|---|---|
| `VptMode` | Studio, Club, Concert Hall, Matrix | **1** (Club) |
| `DcPhaseFilterType` | Type A LOW, Type A STANDARD, Type A HIGH, Type B LOW, Type B STANDARD, Type B HIGH | **5** (Type B HIGH) |
| `DseeHxCustomMode` | Standard, Female Vocal, Male Vocal, Percussion, Strings | — |
| `SetVinylizerType(unsigned)` | Standard, Turntable Resonance, Arm Resonance, Surface Noise | — |
| `ToneType` | BASS, MIDDLE, TREBLE | — |
| `Eq6BandPreset` | Bright, Excited, Mellow, Relaxed, Vocal, Custom 1, Custom 2 | — |
| `UserPresetNo` | Custom 1, Custom 2 | — |
| `EqType` (`SetSelectUsingEq`) | Equalizer / Tone Control — mutually exclusive | 10-band selected |

On the last one the manual text is explicit, and it matters for the UI: *"You can switch between the
equalizer and tone control at will, because their settings are saved separately."* They are not two
views of one control; they are two controls with one selector.

## What Cinder wires today

13 shim entry points against Sony's 54 methods:

`set_dsee_hx`, `set_vinylizer`, `set_vpt`, `set_vpt_mode`*, `get_vpt_mode`*, `set_dc_phase`,
`set_dc_phase_type`*, `get_dc_phase_type`*, `set_dynamic_normalizer`, `set_clearaudio_plus`,
`set_eq` (10-band), `set_bt_audio_effect`, `set_bypass`.  (* added 2026-08-17, not yet in the UI.)

## The gap — everything Sony has that Cinder does not

| feature | Sony API | note |
|---|---|---|
| **VPT mode** | `SetVptMode` / `GetVptMode` | 4 rooms. Cinder renders VPT as a bool; the row's own subtitle already promises "Studio / Club / Concert Hall". |
| **DC Phase type** | `SetDcPhaseFilterType` / `Get…` | 6 types, same story — the row says "Analog-amp low-frequency phase response" and offers on/off. |
| **DSEE HX Custom** | `SetDseeHxCustom`, `SetDseeHxCustomMode`, `Get…` | 5 modes, source-material specific. |
| **DSEE AI** | `SetDseeAi`, `IsDseeAiOn` | Present in the API. Whether the A50 has the hardware is UNVERIFIED — treat like high gain until heard. |
| **Source Direct** | `SetSourceDirect`, `IsSourceDirectOn` | Bypasses the chain for the purest path. Distinct from Cinder's A/B bypass, which uses `DisableSoundEffects`. |
| **Tone Control** | `SetToneControl`, `SetToneValue(ToneType,int)`, `SetToneCenterFreq(ToneType, ToneCenterFreq)`, getters | 3 bands, each with an adjustable CENTRE FREQUENCY. A whole second tone system, alternative to the EQ. |
| **Clear Phase** | `SetClearPhaseHeadphone` / `Speaker` / `Wmport` | Headphone is the relevant one; Speaker/WMPORT describe hardware the A55 does not have (cf. `smaster btl`). |
| **EQ 6-band + presets** | `SetEq6Band`, `SetEq6BandPreset`, `SetEq6BandValue` | Sony's named presets (Bright/Excited/Mellow/Relaxed/Vocal) live on the SIX-band, not the ten. Cinder only drives the ten. |
| **User presets** | `SaveUserPreset` / `LoadUserPreset(UserPresetNo)` | Custom 1 / Custom 2 — Sony's own "two saved setups", and the natural backing store for Cinder's A/B. |
| **Vinylizer type** | `SetVinylizerType(unsigned)` / `Get…` | 4 characters; Cinder has on/off. |

## How to settle the ranges by ear

`cinder-probe --vpt <n> [secs]` holds a mode with the pump running and re-asserts it once a second
(cinder-home's `apply_sound_fn` will otherwise overwrite it). It must HOLD rather than return: the
effect belongs to the probe's own `EffectCtrlDmp` client, so exiting tears the setting down with it.
Default hold is 30 s. With no argument it sweeps every value with a 3 s dwell and restores what it
found.

## Gotchas

- Catalogues are **UTF-16BE**. `strings` without `-e b` returns nothing and makes the strings look
  absent from the firmware entirely.
- `GetVptMode` and friends need the **Framework pump**; without it they return a plausible, constant
  `0` that reads exactly like "the service rejected the write". See `reference_pst_ipc_pump`.
- The probe exits with a cosmetic `FATAL SIGNAL PC=0` during teardown, after the work and the
  restore have both completed.

---

## 2026-09-30 — the count, redone, and a second class (R5)

The "13 against 54" above is the 2026-08-17 state and went stale within weeks: the Advanced screen
(2026-08-17 to 08-23) wired most of the gap. Counted again from the binaries, host only:

| | methods |
|---|---|
| `EffectCtrlDmp` exports, excluding ctor/dtor and the internal `Update…` family (`nm -DC --defined-only`) | **60** |
| …of which `HgrmMediaPlayerApp` imports | **55** |
| wrapped by `cinder-audio/src/effect_shim.cpp` | **55** (the app's 55 less the four Clear Phase Speaker / WM-PORT calls, plus the four `…dB` getters the app does not import) |
| reached by `cinder-home` before R5 | **22**, all setters |
| reached by `cinder-home` after R5 | **41**: the same 22 plus 19 getters, read after a sound profile is applied (`main.cpp`, `fx_verify_fn`) |

Not wrapped, on purpose: `SetClearPhaseSpeaker` / `SetClearPhaseWmport` and their two `Is…On`
(hardware this player lacks), `GetPresetSettings` (takes an `EffectSettingsDmp*` whose layout is
unrecovered — an out-struct of unknown size is exactly the call that smashes a stack).

Wrapped but not driven by `cinder-home`, each for a reason that is a finding, not an omission:

* **The 6-band EQ** (`SetEq6Band`, `SetEq6BandPreset`, `SetEq6BandValue` and their getters) —
  measured to have no effect at the jack (`RE_clear_bass.md`, 2026-09-17).
* **Tone Control centre frequencies** (`Set`/`GetToneCenterFreq`) — the ordinals echo 0..7 with no
  frequency behind them, and no view-model in the stock player was found to set them (the only
  references in `HgrmMediaPlayerApp` are the wrapper `dmpapp::SoundEffect::SetToneCenterFreq` and
  its log strings; there is no `tone…Freq` property, signal or QML binding). So Sony ships the
  defaults, and so does Cinder.
* **Sony's user presets** (`SaveUserPreset` / `LoadUserPreset`) — each stored preset holds
  `SelectUsingEq = 1`, the 6-band, so loading one takes Cinder's EQ out of the path. Cinder's own
  profiles (R5) are the replacement.

### `SoundServiceSettingsDmp` — the class nobody had looked at

`HgrmMediaPlayerApp` imports a second sound class, `pst::services::sound::SoundServiceSettingsDmp`
(`libSoundServiceSettingsDmp.so`): six setter/getter pairs. Parameter keys, from the library's
strings: `dsd_conv_filter_type`, `dsd_conv_gain_mode`, `builtin_dsd_process_mode`,
`uac_dsd_output_mode`, `lpcm_playback_mode`, `hp model` (and `se_hp_dsd_native_enabled`, which no
exported method names).

Read from the disassembly (`cinder-audio/src/sound_settings_abi.hpp` has the addresses):

* The object is **4 bytes**: the ctor does `operator new(1)` and stores the pointer.
* A setter builds `<key>=<to_string(unsigned)>` and calls
  `SoundServiceSettings::SetParams(const std::string&)`, returning its result.
* A getter calls `GetParams(key, std::string&)` and returns `std::stoi` of the reply, or 0 if
  `GetParams` failed — so 0 is ambiguous, and **`std::stoi` on an empty reply throws with nothing
  in the library to catch it**. The shim wraps every call in `try`/`catch (...)`.
* `SetHeadphoneModel` also constructs an `ncasm::NcAsmService` and calls its `SetParams`. Not
  wrapped.

Catalogue labels that go with them (`strings -a -e b`, UTF-16BE): *Filtering* — "Slow Roll-Off",
"Sharp Roll-Off"; *Gain* — "0 dB", "-3 dB"; "Play DSD in Native Format"; "USB Output for DSD";
"DSD Remastering". **Which label is which value is not recovered**, and what `lpcm_playback_mode`
selects is not known (the renderer has `RendererDmpMaster::UpdateLpcmPlaybackMode`).

Status: 11 of the 12 methods wrapped (`cinder_sound_settings.h`), linked into `cinder-probe` only,
read by `cinder-probe --soundsettings`, **every signature unverified on device**
(`docs/DEVICE_CHECKLIST.md` 26.7). No screen offers them: the values are unknown, and there is no
DSD file on the reference player to hear a difference with (`RE_dsd_path.md`).
