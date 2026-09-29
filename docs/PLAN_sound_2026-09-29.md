# Sound settings: plan (2026-09-29)

What this plan works from:

- the Walkman One deep dive ([`analysis/RE_walkmanone_installers.md`](../analysis/RE_walkmanone_installers.md)),
- the bench survey ([`analysis/RE_headphone_amp_modes.md`](../analysis/RE_headphone_amp_modes.md)),
- Wampy's jack measurements,
- the tables read live off a Walkman One player on 2026-09-29 (`artifacts/sound_2026-09-29/`, git-ignored).

## Where things stand

| Lever | Changes the signal? | In Cinder |
|---|---|---|
| Regional table (`_cew`) | Yes: quieter on CEW2/KR3 players | Lifted by default since 2026-09-29 (`voltable=region` keeps it) |
| Volume curve (stock / WM1A) | Yes: step mapping. Stock has 20 dead steps; WM1A is even, same ceiling | `voltable=wm1a` (works on W1 since 2026-09-29) |
| Headphone amp (S-Master / linear) | Yes, into a line input: +2.85 dB, half the THD, flat | Sound ▸ Advanced, opt-in, **untested on device** |
| Sony DSP (EQ10, Tone Control, DSEE, DC Phase, Vinyl, ClearAudio+, VPT) | Yes | Sound and Advanced screens |
| Tone table (`tct`) = the codec's own 5-band EQ | Sony's tables: only with its NC headphones (A50 and WM1A differ only in the NW500N blocks). Filled by Cinder: yes, in the chip | **DAC EQ** (Sound ▸ Advanced), experimental; `tone-*` keys inert with other headphones |
| W1 signatures, Plus modes, DAC mode, the tuning package | No (Wampy's measurements; the tuning's bootloader is inert on this board) | Plus modes via `signature` (not persistent on W1) |

**Is Walkman One doing its job on an A50?** Partly. Its sound signatures and tuning packages do
nothing measurable on this hardware. Its "Higher" gain mode is really the WM1A curve, which is
*quieter* at volume 100 and reaches the same ceiling. Its real effects are all table loads: the
region cap and the curve. W1's tone table changes nothing without Sony NC headphones (§3). Cinder
can make every one of those, and can tune them beyond what W1 ships.

## What the live read gave us

Every table the codec driver holds, read whole from `/proc/icx_audio_cxd3778gf_data/`:
`ovt` (volume), `ovt_dsd`, `tct` (tone), `dgt` (device gain) and nine `tct_*` variants for Sony's
noise-cancelling headphones.

- **The live read only returns the table body.** It has the file's size minus the 8-byte trailer.
- **Live tables on the W1 reference player:** `ovt` = A50 plain `ov_1291`, `ovt_dsd` = A50 plain,
  `tct` = W1's WM1A `tc_127x`.
- **`dgt` is 10 bytes:** `00 00 06 00 | 00 00 f8 00 | 00 00`. That is `0x060000`, `0xF80000`, `0`,
  the same values as the kernel's `cxd3778gf_device_gain_table`. `dacdat dgt FILE` loads it.
  **Decoded 2026-09-29: input gains** (`{pga, adc}` per input: tuner +6, line −8). Not playback.
- **The linear amp setting persists across codec standby and wake.** It resets at reboot.

> **Safety: never read part of one of these nodes.** The driver takes the codec's `global_mutex`
> on the first read and releases it only when a read reaches the end of the table. A `head -c`
> leaked it on 2026-09-29 and every codec operation hung until a forced reboot. Use `cat` on the
> whole node, or don't read it.

## Work, in order

### 1. Linear amp: listening test (device, ~20 min)

1. Install the build with the new row. Put headphones in and set the volume low.
2. Toggle Sound ▸ Advanced ▸ Linear headphone amp with music playing, and listen for a pop.
3. Check `cinderhome.log` for `hp amp:` lines. Count how often something resets it (plug-in,
   track start, Bluetooth handover). That settles whether the once-a-second re-check is needed.
4. A/B by ear at matched loudness: linear is about 3 dB louder, so turn it down 3 dB first (6–12
   steps, depending on where on the curve you are).
5. Battery: one full-drain run each way at a fixed volume. Only the runtime ratio is trustworthy on
   this device.

Decision afterwards: keep it opt-in, make it the default, or remove it.

### 2. The volume curve (device, ~10 min + decision)

The WM1A curve is the one clear improvement: no dead steps, 0.25 dB steps at the top, same ceiling.

- **Decision needed:** make `wm1a` the default? A familiar number sounds different: quieter at
  most steps (5.5 dB at 100), about 2 dB louder around 70, the same at 80 and 120.
- **Our own curve instead.** The table format is known: 2 × 27 × 121 × 13 bytes plus the trailer,
  laid out in Wampy's `cxd3778gf_table.h`. We can generate an even curve in dB, e.g. −60 → 0 dB in
  0.5 dB steps, from the codes in the WM1A table. The trailer is the table's own sum/xor. Gate:
  `cinder-probe --volcurve`, then the jack rig, before anyone hears it.

### 3. DAC EQ (built 2026-09-29; needs ears)

Decoded offline: `tct` is not Sony's Tone Control but the codec's own five-biquad EQ, flat for every
headphone except Sony's NC models. `dgt` is input gain. Full write-up:
[`analysis/RE_codec_tone_table.md`](../analysis/RE_codec_tone_table.md).

Built on that: Sound ▸ Advanced ▸ **DAC EQ**, five fixed bands, −12 to +6 dB, boosts paid for by
lowering the rest, flat under Source Direct. `cinder-voltable eq` builds the table.

- **Done on the device (W1, 2026-09-29):** tables accepted and read back exactly; flat = stock bytes.
- **Left:**
  1. Listen with headphones: bass −12 should be obvious. Listen for a click when a band moves.
  2. Jack rig: sweep or pink noise, capture each band at −12 and +6, confirm the frequencies.
  3. Then decide: keep "Experimental", or drop the label.

### 4. Plus modes on W1 (small)

`cinder-signature.sh` patches the live HAL, and W1 copies its own over it on every boot. To make
it stick on W1, patch the W1 source copy (`/system/etc/.mod/adler/normal_nt/`) as well. It's low
value, since nothing about it measures at the jack.

### Not doing

- **S-Master high gain:** measured inert on this model (2026-08-17); the row was cut.
- **Tuning packages, bootloader, NVRAM:** inert on the A50.
- **Direct codec register writes:** `/proc/regmon/*/value` stays forbidden. Every lever above goes
  through ALSA controls, `dacdat` table loads or Sony's services.
