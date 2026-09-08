# The DSD path — native, real, and unused

Measured 2026-09-08, jack to a PC input. Prompted by "fully use the hardware": card 0 publishes
`dsdenc` (hw:0,2) and `cxd3778gf-dsd-out` (hw:0,3) and nothing in Cinder or, apparently, in Sony's
player has ever opened them.

## The claim this replaces

`docs/AUDIT_2026-08-16.md` §sid_4105 says DSD files "play — `psk::FileUtil::GetFormatFromFilename`
maps `dsf`/`dff`. Only the DoP-vs-PCM knob is missing, and **this hardware converts to PCM
regardless**." The last clause was read from code, never measured. It is wrong.

## What the driver accepts

`hw:0,3`, probed by opening it:

| format | verdict |
|---|---|
| `DSD_U8` | **accepted** |
| `DSD_U16_LE` | **accepted** |
| `DSD_U32_LE` | refused — `set_params:1233: Sample format non available` |
| `S16_LE`, `S32_LE` | accepted (it takes PCM too) |

Rates above 192000 are refused by **aplay's own argument parser** (`main:593: bad speed value
352800`), not by the driver. That does not matter: `DSD_U16_LE` at 176400 frames/s x 16 bits is
**2 822 400 bit/s — DSD64 exactly**, so a standard DSD64 stream reaches the hardware through a rate
aplay will accept.

## What the codec does with it

A real DSD64 bitstream was generated for this test — 1 kHz, second-order delta-sigma with clamped
integrators, packed oldest-bit-in-MSB — and verified by demodulating it back to PCM before it went
anywhere near the device (1000.0 Hz, THD 0.30 % through a crude boxcar decimator, so genuinely a
tone and not noise). Played to `hw:0,3`, read while the stream was open:

```
DSD_ENABLE   = 0x01      <- the codec's DSD block is ENGAGED
SMS_DSD_CTRL0 = 0x80
SMS_DSD_CTRL1 = 0x0F
HPOUT2_CTRL1 = 0x0F      <- S-Master SE output up, as for PCM
pcm3: format: DSD_U16_LE  rate: 176400  channels: 2
```

`DSD_ENABLE` is `0x00` during ordinary PCM playback. **So this hardware has a native DSD path, it
works, and a 1-bit stream is carried as 1-bit.** The "converts to PCM regardless" claim is retired.

## Why it is not usable yet

**It has its own volume curve, and most of the range is dead.** Sweeping `master volume` with the
DSD stream open:

| master volume | 20 | 60 | 100 | 120 |
|---|---|---|---|---|
| `PHV_L` | 0x04 | 0x04 | 0x94 | 0xE4 |

Against the PCM path's 0x50/0x7C/0xCC/0xE4 over the same range. The DSD path is at 4 of 228 — near
silence — for everything below volume 100, which is why the first capture recovered no tone at
−42 dBFS. This is a separate table (`ov_dsd_*`, written to `/proc/icx_audio_cxd3778gf_data/ovt_dsd`
by `cinder-voltable`), and `RE_walkmanone_extract.md` records that the WM1A ships a **different DSD
curve** (`ov_dsd_127x` differs, where `ov_dsd_1291 == ov_dsd_1280`).

**`dsd remastering mute` will not clear.** `amixer cset numid=24 0` is accepted and reads back `on`,
so something in the driver holds it — plausibly that the DSD *encoder* (`hw:0,2`, `dsdenc`) has to
be running for the remastering path to unmute, and this test drove only the output device.

## What it would take

1. The DSD volume curve, measured the way the PCM curves were (`RE_headphone_amp_modes.md` §2) —
   acoustically at the jack, not by reading PHV, since PHV was already shown not to be the whole
   volume path. Then decide between stock and the WM1A DSD table.
2. Work out what holds `dsd remastering mute`, most likely `hw:0,2`.
3. A real `.dsf` on the device. **There is none** — `find /contents -iname '*.dsf' -o -iname '*.dff'`
   is empty — so nothing has ever exercised this end to end, and any shipped support would be
   untested against a real file.
4. Then the actual question: does Sony's PlayerService route a `.dsf` to `hw:0,3` (native) or to
   `hw:0,0` (converted)? That needs a DSD file to play through the normal path, and it is the test
   that decides whether native DSD is a feature Cinder would be **adding** or merely **keeping**.

## Status

Native DSD is confirmed present and working at the hardware level. Not shipped, not wired into
Cinder, and it should not be until step 4 above is answered — the interesting claim is not "the
chip can do DSD" but "Sony throws it away and Cinder need not", and that is still unproven.
Device restored: volume 54, `dsd remastering mute` on, test files removed.
