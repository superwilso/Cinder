# The codec "tone table" is a hardware EQ

Decoded 2026-09-29 from Sony's tables and the stock A50 kernel; the helper's tables were checked on a
Walkman One player the same day. This replaces the guess in `docs/PLAN_sound_2026-09-29.md` that
`tct` is Sony's Tone Control.

## 1. Summary

`/proc/icx_audio_cxd3778gf_data/tct` is **not** Sony's Tone Control. That runs in software, in
SoundServiceFw. `tct` is the CXD3778GF's own digital EQ: five biquads, applied to both channels, in
the chip, after all of Sony's software effects.

Sony uses it only to correct its own noise-cancelling headphones (NW500N, NW750N, NC31). For every
other pair it loads the identity, so on an ordinary headphone the EQ is on and flat. That makes
it a free, zero-CPU EQ Cinder can fill: **DAC EQ** (Sound ▸ Advanced).

`dgt` is unrelated to playback: it holds input gains (§5).

## 2. Table layout

2 888 bytes: nine blocks of 320 bytes, then an 8-byte trailer.

| block | amp | headphones |
|---|---|---|
| 0 | — | none plugged in |
| 1 | linear (`normal`) | ordinary |
| 2, 3, 4 | linear | NW500N, NW750N, NC31 |
| 5 | S-Master (`smaster-se`/`-btl`) | ordinary |
| 6, 7, 8 | S-Master | NW500N, NW750N, NC31 |

A block is 64 words of 5 bytes. Words 0–31 serve the 44.1 kHz family of rates and words 32–63 the
48 kHz family. Each half holds five biquads, `b0 b1 b2 a1 a2` (25 words), then seven unused words
that are zero in every table.

A word is a signed 40-bit big-endian fixed-point number, Q3.37: `0x20_0000_0000` is 1.0. The
feedback terms are stored **negated**: `y = b0·x + b1·x1 + b2·x2 + a1·y1 + a2·y2`. An identity
biquad is therefore `20 00 00 00 00` followed by 24 zero bytes.

The trailer is the same checksum every codec table carries (Wampy's `checksum()`): a 32-bit sum of
the body bytes, then an XOR of each byte shifted left by `(i % 4) * 8`, both little-endian. Checked
against the real `tc_1291`, `tc_127x` and `tc_1280`.

### What Sony ships

| file | model | blocks 0, 1, 5 | NC blocks |
|---|---|---|---|
| `tc_1291.tbl` | NW-A50 | identity | filled |
| `tc_127x.tbl` = `tc_1280.tbl` | NW-WM1A (and ZX300's `tc_1288`) | identity | filled |

The A50 and WM1A tables differ **only in the NW500N blocks**. So Cinder's `tone-w1`/`tone-wm1a`
keys change nothing with any other headphones. They stay for NW500N owners.

## 3. How the kernel loads it

Traced in the stock A50 kernel (`adjust_tone_control.part.23`):

1. It picks the block from the amp setting, the headphone type (a 5-pin jack reads as type 3) and
   the NC type.
2. It writes registers `0x30`, `0x17` and `0x70` to prepare the load.
3. It writes the block's 320 bytes as 8 × 40 bytes: the address goes through register `0x71`, the
   data through `0x72`.
4. It turns the EQ back on (`0x30` bit 1).

It runs when a table is written (`apply_table_change(2)`), on every output, amp,
headphone-type and jack change, on init, and on resume. So a table written once stays in force for
every later change until a reboot, when Sony's own boot loads the stock file again.

### Proc nodes, as found on the device

`tct` accepts a whole 2 888-byte table. Reading it back (`cat` the whole node, never a partial read —
see the codec-table mutex note) returns the 2 880-byte body without the trailer, and the kernel also
dumps it to `dmesg`. There are per-block nodes beside it: `tct_nh`, `tct_ng`, `tct_sg` and the
`tct_n*`/`tct_s*` NC ones. Cinder does not use them.

## 4. DAC EQ

`cinder-home/src/codec_eq.h` builds a table; `cinder-voltable eq G1 G2 G3 G4 G5` (setuid root)
applies it. The arguments are five whole numbers. The helper never takes bytes or paths.

| band | type | frequency | Q |
|---|---|---|---|
| 1 | low shelf | 100 Hz | 0.707 |
| 2 | peak | 400 Hz | 0.9 |
| 3 | peak | 1.5 kHz | 0.9 |
| 4 | peak | 4 kHz | 0.9 |
| 5 | high shelf | 10 kHz | 0.707 |

- Coefficients come from the RBJ Audio EQ Cookbook, for 44.1 kHz and 48 kHz, one per block half.
- Gains are half-dB steps from −24 to +12 (−12 dB to +6 dB). The helper rejects anything outside
  that range (rc 2), so the UI clamps to it first.
- **A boost never raises the level.** If any band boosts, the first biquad's feed-forward terms are
  scaled so the cascade peaks at 0 dB (checked from 20 Hz to 20 kHz at 1/24 octave). Nobody has
  measured how much headroom the codec path after the EQ has, so none is assumed.
- A zero gain writes the exact identity biquad, so a flat EQ reproduces Sony's table and trailer
  byte for byte.
- Q3.37 conversion saturates instead of wrapping. A wrapped coefficient is an unstable filter.
- Only blocks 1 and 5 change, with the same EQ in both. The NC blocks are kept as the source had
  them.
- The source is the A50's `tc_1291`, or the WM1A's `tc_127x`/`tc_1280` when that is what the
  player carries (Walkman One has only these).

Source Direct holds the EQ flat. The shell (`apply_dac_eq()` in `main.cpp`) re-applies it at startup
and on every change to the EQ, Source Direct or a settings reset. It skips a curve that is already
loaded, and a flat curve when nothing has been loaded yet.

### Tests

`cinder-home/tools/codeceq_selftest.cpp` runs in `build.sh` and CI:

- the trailer against a hand-computed vector;
- flat reproduces the source exactly;
- only blocks 1 and 5 change;
- cuts and peaks land at their frequencies, in both rate halves;
- a boost peaks at 0 dB or below;
- all 243 extreme settings × 2 rates are stable and survive the round trip through Q3.37;
- clamping and saturation.

### On the device, 2026-09-29 (Walkman One, no headphones plugged in)

| step | result |
|---|---|
| read `tct` at boot | the WM1A table's body |
| `eq 0 0 0 0 0`, read back | identical to the boot table |
| `eq -24 0 0 0 0`, read back | only blocks 1 and 5 differ |
| `eq 0 0 0 0 0` again | identical to the boot table |

`tone-stock` returned 4 on this player, as expected: Walkman One carries no `tc_1291`.

**Not yet known:** whether the EQ is audible, and whether a reload clicks. Both need headphones or a
capture at the jack (`tools/measure_output.py`).

## 5. `dgt`

`dgt` holds input gains, `{pga, adc}` for each of five inputs: none, tuner (+6), mic, line
(−8, stored `0xf8`), direct mic. It does not touch playback.
