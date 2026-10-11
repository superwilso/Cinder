# Clear Bass — found in the A50's own firmware (2026-09-16)

**Short version.** Clear Bass was never missing from the NW-A50. It is **band 0 of Sony's six-band
EQ**, whose complete Walkman implementation ships inside `libSoundServiceFw.so` as `CB_6bandEQ_*`
("Clear Bass 6-band EQ"). The stock A50 UI never showed it — it shows the 10-band EQ and Tone
Control — but the service runs it, and `EffectCtrlDmp` exposes it. Device reading: band 0 accepts
+10 under the Custom 1 preset and the six-band reports `isproc is 1` next to Cinder's 10-band EQ.
**Measured 2026-09-17: it does not process when set this way** — no change at the jack under either
selector, while a ten-band change on the same rig measures +7.9 dB (§4). The data path is found; what
engages it is not.

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

## 4. Measured at the jack (2026-09-17) — no effect

**Rig.** The player's 3.5 mm output into the PC's microphone input, recorded with Windows FFmpeg
(`-f dshow`). Signal: a 29 s pink-noise FLAC played by Cinder on repeat-one (29 s so the scrobbler
never logs it). The probe switched Custom 1 band 0 between the level and 0 every 5 s for 50 s
(`cinder-probe --clearbass <level> 50 <selector> 5`); the recording was folded against a 10 s square
wave and the third-octave spectra of the "on" and "off" halves compared.

| Run | 40 Hz | 50 Hz | 63 Hz | Fit r |
|---|---|---|---|---|
| Clear Bass +10, selector 1 (six-band) | +0.6 | +0.3 | +0.2 | +0.06 |
| Clear Bass +10, selector 2 (Cinder's ten-band) | +0.8 | +0.3 | −0.1 | +0.07 |
| Clear Bass −10, selector 1 | +0.2 | +0.5 | +0.4 | — |
| **Control:** ten-band band 1 (63 Hz) +10 dB, `--eq10alt 1 20 50 5` | +4.8 | +6.9 | **+7.9** | **+0.57** |
| Clear Bass +10, selector 1, held 70 s across two loop restarts | within ±1 dB throughout | | | |

The rig sees a ten-band change clearly, so the null is real: **setting Custom 1's band 0 through
EffectCtrlDmp does not engage Clear Bass**, live or at the next stream start. The writes do arrive —
the service logs `effect param eq6band,band=0,value=10`, `selectusingeq,type=1` and
`Eq6band::UpdateProcCond … isproc is 1` — each followed by `no desired value, skip`.

Two earlier runs against music suggested +8 dB at 40-50 Hz on selector 1; a repeat showed +1.5 dB.
Music is too uneven for this; use the noise file.

## 5. What is left

1. **Why the six-band filter does not engage.** Read `Eq6band::UpdateProcCond` and the path from
   `ExecEffectParam("eq6band,band=…")` to `CB_6bandEQ_*` in `libSoundServiceFw.so`: what "desired
   value" it waits for, and whether `CB_6bandEQ_eq` is reached at all (a breakpoint-free check: the
   `--dseemeter`-style analyzer tap, or the ALC state in `CB_6bandEQ_*`'s instance). Re-measure with
   the same rig.
2. Only then a Cinder setting: a Clear Bass row (−10..+10) on the Sound screen, driven through
   Custom 1's band 0 with the other five bands flat, re-asserted like the rest of the chain.
3. Map the preset names to rows (the stock QML never drew the six-band; Media Go / Music Center
   or an A40 UI would).

## 6. On Walkman One: where it went and how to get it back (2026-10-11)

Read 2026-10-11 from Walkman One's `libSoundServiceFw.so` (`artifacts/w1_rootfs/sony/lib`, listing
in `artifacts/decomp_w1`). Nothing in this file has been run on a player. Addresses are file
offsets; add 0x10000 for the Ghidra listing. Background: [`RE_sound_service.md`](RE_sound_service.md).

### 6.1 What the module is

Clear Bass is band 0 of Sony's six-band EQ. The effect is one module, `CB_6bandEQ`, with three
parts (exported symbols of the library):

| part | symbols | job |
|---|---|---|
| bass band | `CB_6bandEQchsep_*`, `CB_6bandEQ_chsep_coef`, `CB_6bandEQ_iir2order_flt_2ch_asm`, `CB_6bandEQ_iir_coef` | splits the low band off and boosts it |
| limiter | `CB_6bandEQ_alc`, `CB_6bandEQ_ln_approx_asm` | holds the boosted band under full scale; this is why Clear Bass does not clip where a plain bass boost does |
| five-band EQ | `CB_6bandEQ_geq_*`, `CB_6bandEQ_geq_coef` | the other five bands |

The filter object that drives it is `pst::services::sound::mobile::Eq6band` (`Eq6band.cc`), named
`eq6band` in the effect chain.

### 6.2 Why it does nothing on Walkman One

The whole module is still in Walkman One's library: `EffectExecuteCB_6bandEQ` 0x159cec,
`CreateEq6band`, and `CreateFilter` 0xfd830 still knows the name `eq6band`. What changed is the
chain. The chain is a vector of eleven names built once, in the static initialiser `_INIT_19`:

    i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc dcphaselinear vinylizer f2i

`eq6band` is not in it, so no `Eq6band` object exists and every Clear Bass setting has nowhere to
land. The stock A50 list has `eq6band` in the slot after `attn`.

### 6.3 Putting it back: a one-byte patch

Each of the eleven entries is copied from a global string through its own PC-relative literal. The
name globals sit 0x10 apart (`eq6band` 0x329d90, `eq10band` 0x329da0, `eqtone` 0x329db0), so
pointing an entry at `eq6band` is a change to one literal:

| entry to give up | literal at | now | for `eq6band` |
|---|---|---|---|
| `eqtone` (Tone Control) | 0x6f910 | `ce a8 2b 00` | `ae a8 2b 00` |
| `dcphaselinear` | 0x6f918 | `be a8 2b 00` | `8e a8 2b 00` |
| `vinylizer` | 0x6f91c | `e0 a8 2b 00` | `80 a8 2b 00` |

The vector is allocated for exactly eleven entries, so an entry is replaced, not added. The
`eqtone` slot is the nearest to where stock keeps `eq6band` (after `attn`, before `alc`).

It would ship the way the sound signature does (`cinder-home/deploy/cinder-signature.sh`): verify
the live library's md5, patch a copy, verify the result, move it into place, keep the original,
take effect at the next boot. Walkman One's boot script may copy its own library over the live one
as it does for the output library; check before relying on it.

**Checked on the player 2026-10-11:** the live library is this one (md5 `845e27e9…`) and the
service prints exactly the eleven names above at start, no `eq6band`. The 2026-09-17 run in
section 4 was on stock firmware, where `eq6band` is in the chain and still measured no effect, so
**being in the chain is necessary and not known to be sufficient**. `no desired value, skip` is
not the cause: Tone Control logs the same line and works, because a live write is only staged and
the coefficients are computed when a track is configured (`docs/DEVICE_TESTS.md`).

**Not established, needs the player:**

* That the service then accepts the six-band calls. `SelectUsingEq` picks which tone system
  processes (1 six-band, 2 ten-band, 3 Tone Control); with `eq6band` in the chain, type 1 should
  give Clear Bass plus the five-band EQ **instead of** the ten-band EQ, not beside it.
* What a library with `eq6band` in place of a filter the service expects does at start-up. The
  sound service failing to start is the risk; stopping it at run time rebooted the player
  (2026-09), so the trial needs the original kept beside it and the boot guard armed.
* The CPU cost. Any enabled filter makes the whole chain run, conversion to float and back
  included (`RE_sound_service.md` section 3 there). Measure with `tools/ssfw_cost.sh`, Clear Bass on
  against everything off.

### 6.4 The cheaper alternative: bass in the codec

The codec chip has its own five-biquad EQ, which Cinder's DAC EQ already writes. A low shelf there
costs no CPU, leaves Sony's chain skipped whole, and so keeps deep idle intact. It is jack only,
and it has no limiter: the boost has to be paid for with the same number of decibels of
attenuation ahead of it, which is what Clear Bass's limiter avoids.

| | real Clear Bass (6.3) | bass shelf in the codec |
|---|---|---|
| sound | Sony's, with its limiter | a plain low shelf, level dropped by the boost |
| outputs | jack and Bluetooth | jack only |
| CPU | the whole software chain runs | none |
| risk | a patched system library | none new: DAC EQ is shipped |

### 6.5 Trial on the owner's player, 2026-10-11: it works

`cinder-home/deploy/cinder-chain.sh set clearbass` (the `dcphaselinear` entry), one reboot. The
service started and printed `… eqtone alc eq6band vinylizer f2i`. Jack into the PC microphone
input, the 29 s pink-noise track on repeat, Source direct OFF, on against off folded over a 10 s
square (`--clearbass 10 50 1 5`; control `--eq10alt 1 20 40 5`):

| run | 35-56 Hz | 56-90 Hz | 90-140 Hz | 140-300 Hz | 300-3000 Hz |
|---|---|---|---|---|---|
| control: ten-band band 1 +10 dB | +3.7 | +5.7 | +1.4 | +0.2 | +0.1 |
| **Clear Bass +10, selector 1** | **+6.9** | **+7.4** | **+4.5** | +0.5 | +0.1 |

Both folds lock to the same phase (1.25 to 1.5 s). With Source direct on the same run reads flat,
as it must: the chain is skipped. So on Walkman One the missing chain entry was the whole reason,
and Custom 1's band 0 with selector 1 drives Sony's Clear Bass. Still open: the stock-firmware
null of section 4 (Source direct was not recorded for that run), the CPU cost with it on, and
whether selector 1 silences the ten-band EQ.

### 6.6 The Cinder row, same day

Sound ▸ Advanced ▸ Clear Bass (Off, +2 … +10), drawn only when the shell finds the patched byte
(`clear_bass_in_chain` in `cinder-home/src/main.cpp`). A level above 0 writes Custom 1's band 0 and
takes the selector to the six-band; Off gives the selector back. Measured through the UI (taps sent
by `tools/dev_remote.sh`, pink noise, jack into the PC):

| | 35-56 Hz | 56-90 Hz | 90-140 Hz | 140-300 Hz | 300-3000 Hz |
|---|---|---|---|---|---|
| +10 against Off | +8.7 | +7.9 | +5.3 | -2.3 | -2.1 |

Off afterwards returned every band to within 0.3 dB of the start. The 2 dB drop above 140 Hz is
the six-band replacing the ten-band (Sony's headroom, or the ten-band's own curve leaving the
path); not separated yet.

**The six-band and the ten-band are alternatives.** Under selector 2 Clear Bass +10 measured
±0.4 dB; under selector 1 a ten-band band at +10 dB measured ±0.3 dB.

Open: the DC Phase Linearizer row on the Sound screen still draws on a patched player and now
does nothing; the level is one value for every output, not per sound profile; CPU cost unmeasured.
