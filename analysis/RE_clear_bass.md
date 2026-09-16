# Clear Bass — where it lives on the NW-ZX100 (2026-09-16)

Question: can Cinder take Clear Bass from the ZX100 the way it took the Walkman One patch?

Input: `NW-ZX100_V1_11.exe` (the owner's download). The updater unpacks itself to
`%TEMP%\pft*.tmp\Data\Device\NW_WM_FW.UPG` while its first window is open; `upgtool -m nw-zx100 -e`
opens it (3 files: install script, kernel, 180 MB rootfs tar.gz). Kept under `artifacts/zx100/`
(ignored; Sony's).

## Readings

* **Different platform.** The ZX100 is Sony's older "genesys" firmware on a Renesas EMMA Mobile EV0
  (`/devel/usr/local/bin/SpiderApp`, modules `em_ave.ko`, `inter_dsp.ko`), not the MT8590 /
  SoundServiceFw platform the A50 shares with the A30, A40, ZX300 and WM1.
* **Clear Bass is the sixth equalizer slider.** SpiderApp's settings keys are
  `EQUALIZER_CUSTOM{1,2}_{00_40,01_00,02_50,06_30,16_00}_KHZ` plus `EQUALIZER_CUSTOM{1,2}_BASS`; the
  label resource calls it `ClearBass` (tid-0605). It is not a separate effect or a preset.
* **The filter runs on the DSP core, not the ARM.** SpiderApp only forwards levels
  (`DalSharedMemOmf_setEq`, `_setEqEffect`, `_setDseeHx`, `_setVpt`, `_setDn`, `_setAlc`, …) into shared
  memory. The processing is in `/devel/usr/lib/omf/dspfw/renderer/omf_dsp_manager.em-ev0`
  (3.0 MB, header `OmfSpxCore 2015/06/08 v5.6`, then one `CODE` section): SPX DSP machine code with
  no symbols and no strings, for a core Ghidra has no processor module for.

## Verdict

Not portable from this firmware. There is no ARM code to lift, and the DSP binary would have to be
disassembled by hand before its coefficients could even be found. Measuring it would need a ZX100.

The realistic source is a model on the A50's own platform whose equalizer had a Clear Bass slider
(A30/A40 or ZX300 — upgtool has confirmed keys for all three): if its SoundServiceFw DSP libraries
carry a Clear Bass stage, it is ARM code of the same ABI Cinder already calls. The A50's libraries
and catalogues have no Clear Bass symbol or label (docs/PLAN_2026-09-14.md), so until then the
answer stays "a bass shelf through Tone Control or the EQ".
