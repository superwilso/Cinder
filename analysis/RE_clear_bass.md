# Clear Bass — found in the A50's own firmware (2026-09-16)

**Short version.** Clear Bass was never missing from the NW-A50. It is **band 0 of Sony's six-band
EQ**, whose complete Walkman implementation ships inside `libSoundServiceFw.so` as `CB_6bandEQ_*`
("Clear Bass 6-band EQ"). The stock A50 UI never showed it — it shows the 10-band EQ and Tone
Control — but the service runs it, and `EffectCtrlDmp` exposes it. Device reading: band 0 accepts
+10 under the Custom 1 preset and the six-band reports `isproc is 1` next to Cinder's 10-band EQ.
**Still unheard** — the listening check is the one open item before it becomes a Cinder setting.

The first pass of this note (earlier the same day) said Clear Bass was not portable. That was about
the ZX100's copy, which runs on a DSP nothing can disassemble; it was right about the ZX100 and
wrong to stop there. `docs/PLAN_2026-09-14.md` said "not in this firmware" for the same reason —
nobody searched for `6band` — and is corrected.

Decompilations, decoded coefficient tables and Ghidra scripts for everything below are in the
Sony-files repository (`cinder-sony-analysis/analysis/clear_bass/`). Scripts that are Cinder's own:
`analysis/clear_bass/ghidra/*.java`, `analysis/clear_bass/find_biquads.py`.

## 1. How to turn it on (the API Cinder already links)

`EffectCtrlDmp` (libEffectCtrlDmp.so; `cinder-audio/src/effect_shim.cpp` already wraps these):

| call | meaning |
|---|---|
| `SetEq6Band(bool)` / `IsEq6BandOn()` | the six-band's own on/off (stock default: on) |
| `SetEq6BandPreset(Eq6BandPreset)` | 0 = flat, 1..8 Sony's curves, **9 / 10 = Custom 1 / 2** — the only slots that keep band writes (`--eq6custom`, 2026-08) |
| `SetEq6BandValue(Eq6Band, int)` | **band 0 = Clear Bass**, 1..5 = 400 Hz, 1 k, 2.5 k, 6.3 k, 16 k; −10..+10, whole dB (`GetEq6BandValuedB(0)` read 10.0) |
| `SetSelectUsingEq(EqType)` | 1 six-band, 2 ten-band (Cinder), 3 tone; the six-band still processed under 2 |

Config keys (libDmpConfig.so): `SS_EQ6_ONOFF`, `SS_EQ6_PRESET`, `SS_EQ6_BAND1..6`,
`SS_EQ6_CUSTOM{1,2}_BAND1..6`.

Probe: `cinder-probe --clearbass <level> [secs] [selector]` selects Custom 1, writes band 0, holds,
and restores everything (selector last). It needs the launcher's library path when run by hand:
`LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib`.

**Reading, 2026-09-16 (silent — nothing was playing):** entry `selector=2 Eq6On=1 preset=0`;
`band0 10 -> reads 10 (10.0 dB), preset 9`; the service logged
`effect param eq6band,band=0,value=10`, `Eq6band::UpdateProcCond … isproc is 1` and
`Eq10band::UpdateProcCond … isproc is 1` (EqTone 0); restored `selector=2 Eq6On=1 preset=0`.

## 2. The algorithm (A50, `libSoundServiceFw.so`, exported symbols)

`CB_6bandEQ_eq(state, params, n)`:

1. Sample-rate index: 44.1 k → 0, 48 k → 1, 88.2 k → 2, 96 k → 3, 176.4 k → 4, anything else
   (192 k) → 5.
2. **Clear Bass (band 0, level L at state+0x130):** `CB_6bandEQ_geq_2ch` with coefficients at
   `CB_6bandEQ_geq_coef + fs*0x1f8 + (10 − L)*0xc` → a per-band limiter `CB_6bandEQ_alc` → added
   to the dry signal; then a SECOND `geq` stage at `+0xfc` on that result, added again.
3. **Bands 1..5:** for each, `CB_6bandEQ_iir2order_flt_2ch` with `CB_6bandEQ_iir_coef +
   band*0x1a4 + (10 − L)*0x14 + fs*0x834` → limiter → added.
4. `CB_6bandEQ_core` then ramps (`_ramp`) and applies a channel-separation stage (`_chsep`, table
   `CB_6bandEQ_chsep_coef`).

Every band is a PARALLEL band-pass added to the signal: out = x + BP(x). Level 0 rows are all zero.

**`CB_6bandEQ_iir_coef`** (12,600 bytes, float32): 6 rates × 5 bands × 21 levels × 5 coefficients
`[b0, 0, −b0, a1, a2]` with y = b0·x − b0·x₂ + a1·y₁ + a2·y₂. Checked: 400 Hz at +10 has
b0 / ((1 − r²)/2) = 2.162, so the band's peak is 1 + 2.162 = 3.162 = **exactly +10 dB**; at
44.1 kHz its poles sit at 387 Hz.

**`CB_6bandEQ_geq_coef`** (3,024 bytes, float32): 6 rates × 2 stages × 21 levels × `(g, c1, c2)`.
The NEON routine doubles all three on load (`vmov.f32 s3, #2.0`; `vmul s0..s2, s3`), so
y = 2g·(x − x₂) + 2c1·y₁ + 2c2·y₂. At 44.1 kHz, level +10: stage 1 is a boost resonator at
**≈ 47 Hz** (peak ≈ +10.7 dB on its own); stage 2 is a **negative** band around **≈ 100 Hz** —
deep bass up, the mid-bass that muddies it down, which is what "clear" plausibly means. Stage 2's
centre moves with the level (≈ 173 Hz on the cut side). Full decoded tables: the Sony-files repo,
`CB_6bandEQ_geq_coef.csv` / `CB_6bandEQ_iir_coef.csv`.

**Presets** (identical table in libEffectCtrlDmp.so @0x1c8c8 and libSoundServiceFw.so @0x19b580),
band order [CB, 400, 1k, 2.5k, 6.3k, 16k]: `[2,0,−3,3,3,0]`, `[0,3,3,0,0,−3]`, `[2,0,0,−3,0,3]`,
`[6,0,3,0,−3,6]`, `[3,0,0,6,0,0]`, `[3,−3,0,3,0,6]`, `[10,0,0,0,6,9]`, `[0,1,2,−1,−2,0]`. Names
(catalogue): Bright, Excited, Mellow, Relaxed, Vocal, Custom 1, Custom 2 — the mapping of names to
rows is not settled.

**Volume-dependent limiter:** `kEq6bandVolTable{A2dp, LineOut, Se…, BtlGain…, …Cew}` — per output
and region, the A50's form of the ZX100's `ct_cb` tables below. `Eq6band::UpdateProcCond` needs the
on/off flag, a preset other than 0, and a field (this+0x178) equal to 1.

## 3. The ZX100's copy (why the first pass stopped)

`NW-ZX100_V1_11.exe` → `%TEMP%\pft*.tmp\Data\Device\NW_WM_FW.UPG` → `upgtool -m nw-zx100 -e`.
Sony's older "genesys" platform on a Renesas EMMA Mobile EV0 (`/devel/usr/local/bin/SpiderApp`).

* **Same six-band EQ.** Settings keys run `EQUALIZER_CUSTOM1_BASS`, then `_00_40_KHZ` … `_16_00_KHZ`
  (the key table at 0x95e7d4). `setEqualizer` packs six int16 gains into three words. Of its
  eight fixed presets, four equal the A50's rows, two differ only in the Clear Bass slot (3 vs 2),
  and the last two are different curves (`[6,0,0,0,0,0]`, `[0,0,0,0,3,10]`).
* **The filter runs on a DSP.** `DalSharedMemOmf` writes the gains into shared memory (enable at
  +0x16, gains at +0x20..+0x2b, one ALC threshold at +0x0a, six band thresholds at +0x34..+0x3f,
  then a commit); `omf_dsp_manager.em-ev0` (header `OmfSpxCore`) holds the code. The kernel driver
  names the core: `description=EMXX SPXK7 DSP Driver`, Renesas. No disassembler exists for SPXK7.
  A numeric scan (`find_biquads.py`) found no coefficient tables in its data — the DSP designs them.
* **Clear Bass thresholds per volume step.** `SpiderApp` sends `threshold ct_alc` and
  `threshold ct_cb` on every volume change. The `ct_alc` tables are `/usr/local/share/audio/
  cxd3774gf/alc` (7 × 32 int16 + checksum; the kernel's `cxd3774gf_alc_table`, 0x1c0 bytes); the
  `ct_cb` tables are DERIVED from them in `DalAlcTable` by `cb = ceil(alc · (20/−18) ·
  log10(10^−0.875 − 10^−0.9))`, i.e. ≈ 2.3636 × alc (pow/log10/ceil confirmed through the PLT).
* **Not Clear Bass:** the codec driver's `adjust_tone_control` writes 5 biquads × 16 bytes per
  headphone type into the CXD3774GF's own digital EQ from `/usr/local/share/audio/cxd3774gf/tct`
  (2 rate families × 6 tables × 5 × 16 = 960 bytes). `au_param_t {clear_stereo, bass_gain,
  trbl_gain, bass_cf, trbl_cf}` is the codec's tone control. Headphone compensation, not the EQ.

## 4. What is left

1. **Listen.** Music playing: `cinder-probe --clearbass 10 20` (selector untouched, so Cinder's
   10-band stays in the path), then `--clearbass 10 20 1`. Expected: deep bass up, mid-bass
   slightly down. If nothing is audible under selector 2, the this+0x178 field is the selector.
2. Then a Cinder setting: a Clear Bass row (−10..+10) on the Sound screen, driven through
   Custom 1's band 0 with the other five bands flat, re-asserted like the rest of the chain.
   Sony's own implementation, limiter included — no port, no shim.
3. Map the preset names to rows (the stock QML never drew the six-band; Media Go / Music Center
   or an A40 UI would).
