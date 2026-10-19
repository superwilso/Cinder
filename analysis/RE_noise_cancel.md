# Noise cancelling: how stock does it, and why Walkman One cannot

**Date:** 2026-10-19. Offline RE of stock 1.02's `libNcServiceFw.so` (Ghidra, `tools/re_fn.sh`), the
kernel image (`artifacts/walkmanone/re/kernel`) and `cxd3778gf_dnc_core.ko`, then read-only and
forced tests on the owner's player (Walkman One 3.02) with no NC earphones in. Started by issue #27
(IER-NW500N, "noise cancelling does nothing").

## Summary

| Question | Answer |
|---|---|
| What does Sony's NC service do? | Writes four codec controls by item name. Nothing else reaches the hardware. |
| How does the player know NC earphones are in? | The codec reads the plug: `jack status se` = `5pin`. |
| Does it know which model? | **No.** The model is a user setting on stock (`HpModel`, DmpConfig). |
| Why the rc2 probe wrote nothing | It waited for `cxd3778gf_ucom` to change. That switch is the codec microcontroller's health, not the plug. |
| Why NC cannot work on Walkman One | W1's `load_sony_driver` never loads `cxd3778gf_dnc_core.ko`, and W1 deleted the gain tables and the ambient filter. |
| Can Cinder fix it? | Yes, except ambient mode: load the module, rebuild the gain tables, write the controls. Forced on with no earphones, the codec reported `nc` (2026-10-19). |

## 1. The service (`libNcServiceFw.so`, `pst::services::ncasm`)

`NcServiceImpl::UpdateValues` → `MakeDriverParams` → `UpdateDriver`, which writes, by name
(`AmixerUtils::SetStr` / `SetInt`):

| Control | Values | From |
|---|---|---|
| `noise cancel headphone type` | `nw750n` `nc31` `nw500n` `other` | `HeadphoneModel` {0 NW750N, 1 NW500N, 2 NC31, 3 OTHERS}, a saved setting |
| `noise cancel mode` | `off` `airplane` `train` `office` `ainc` `ambient1..3` | NC on + environment, or ambient on + environment. Sony's map never uses `airplane` |
| `user noise cancel gain` | INTEGER 0..30 | NcGain, Sony's range −15..+15 (`GetMin/MaxNcGain`) |
| `user ambient gain` | INTEGER 0..30 | AsmGain |

`MakeCapability`: NC for models 0–2, ambient (ASM) for NW500N only. `MakeValidity` logs each input:
the model, `5pin hp` (ConnMgrService's device info, `NcMonitor+0x81`), single-ended output
(`PathMgrService`, output device 2), FuncMode, and the driver.

`NcMonitor::GetUcomState` reads `/sys/class/switch/cxd3778gf_ucom/state`. Value 1 feeds the
emergency message (`EmgMsg` "HungUp"), not the plug. In the kernel the only direct setter is
`cxd3778gf_ext_force_shutdown` (sets 1). It reads 0 on every player we have seen.

## 2. The kernel

* **The plug.** `get_jack_status_se` powers the mic bias, reads the two mic ADCs (registers 14/15)
  and calls a plug 5-pin when both read 13..231. `report_jack_status_se` prints
  `hp(3pin|4pin|5pin) detect`. The ALSA control returns the cached value under the codec mutex.
  Items: `none 3pin 4pin 5pin antenna`. Its put handler does nothing.
* **The model.** Only `cxd3778gf_put_headphone_type_control` sets `present+0x70`. Nothing
  detects it. (The 2026-08-18 note that it was auto-detected was wrong.)
* **The judge.** `cxd3778gf_dnc_module_judge` (in the .ko, through `dnc_interface`) engages when
  output device = headphone, `jack status se` = 5pin (or `nc ignore jack state` on), the type is not
  `other`, and the mode is 1–4 (NC) or 5–7 (ambient, which also needs analog input off). Without
  the module, `dnc_interface` is NULL and the judge is always 0.
* **Gain.** `set_user_nc_gain(0..30)` writes DNC registers 0x90–0x93 from a table filled by
  `/proc/icx_audio_dnc_data/u_ncgain_*`. With no table loaded the table is zeros.

## 3. What Walkman One removed

Stock's `/system/bin/load_sony_driver`:

```
insmod /system/lib/modules/cxd3778gf_dnc_core.ko          <- W1: removed
dacdat cnw500n|cnw750n|cnc31n <airplane/train/office/through x 44.1/48>   <- kept
dacdat cnw500a ambient480_nw500n.bin x3                   <- W1: removed, file deleted
dacdat idata iram.bin; dacdat ainc_500|750|31; dacdat limiter_500|750|31  <- kept
dacdat u_ncgain_500|750|31; dacdat ambgain                <- W1: removed, files deleted
```

The `.ko` is still on the player. With the module absent, the kept `dacdat` lines have no
`/proc/icx_audio_dnc_data` to write to and fail silently. On the owner's player nothing had ever
written the controls: type `nw750n`, NC gain 0.

## 4. The gain tables are a formula

Each `ncgain_*.tbl` / `ambgain.tbl` is 70 bytes: 31 big-endian u16, then the little-endian byte sum
of those 62 bytes, then the little-endian XOR of the 62 bytes zero-padded to 64 and read as u32. The
module's `check_data` rejects a one-bit change (`sum error`, verified).

The 31 values are `centre × 10^((k−15)·step/20)`, within 1 code of Sony's. The step is 0.3 dB for
NC and 8/30 dB for ambient. Centres:

| Table | plain | `_cew` |
|---|---|---|
| ncgain 500n, 750n | 2106 | 3966 |
| ncgain 500s, 750s | 2082 | 3744 |
| ncgain 31n | 4106 | 7645 |
| ncgain 31s | 4059 | 7218 |
| ambgain | 5733 | 10313 |

`dacdat` picks `_cew` for `shp` 0x00000103 and 0x00000203. The ambient filter
(`ambient480_nw500n.bin`, 160 bytes of coefficients) is not a curve and cannot be rebuilt.

## 5. What Cinder does (v0.3.17-rc3)

* `cinder-voltable nc-load` (setuid): returns at once if `/proc/icx_audio_dnc_data` exists (stock).
  Otherwise it loads the module with `init_module`, re-runs Sony's kept `dacdat` NC lines, and
  writes the seven tables, computed. Tested on the owner's W1 player from a clean module state:
  rc 0 and no `check_data` errors.
* cinder-home's `nc_tick` (1 Hz): `jack status se` = 5pin → reports the plug to the UI. While
  the plug is in, it writes the type, gain 15 (Sony's 0) and the mode whenever the choice changes
  and after every plug-in. The first non-off mode runs `nc-load`. While NC is on, the codec is kept
  out of standby and the SoC out of RAM suspend.
* UI: a Noise cancelling row at the top of Sound, only while the plug is in, opening a screen with
  the mode (Off, Automatic `ainc`, Bus or train, Office, Ambient sound) and the earphone model.
  Ambient is offered only for the IER-NW500N and only where Sony's ambient filter exists (stock).

**Forced test, no earphones (2026-10-19, owner's W1 player):** with `nc ignore jack state` on, type
`nw500n` and gain 15, mode `ainc` → `noise cancel status` = `nc`; `train` → `nc`; `ambient1` →
`ambient` (before the gain tables existed). Everything was set back afterwards.

**Not verified:** cancellation by ear (needs the earphones); whether the linear amp or the DAC EQ
interacts with NC (the DAC EQ writes the ordinary-headphone tone blocks; the codec may use the
NW500N blocks instead with that type set); whether a stock player's own NC service later rewrites
Cinder's values (it writes only when its own settings change).
