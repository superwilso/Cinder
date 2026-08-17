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
