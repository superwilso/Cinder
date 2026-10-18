# Sound settings and battery, 2026-10-18

The owner asked for every sound setting (profiles, effects, the DAC, S-Master) to be battery-first
by default and still offer the options. This is the starting inventory, on the owner's NW-A55
running Walkman One 3.02. Numbers come from earlier dated runs, cited by row; nothing new was
measured today except where marked.

## 1. What costs battery, measured

| Lever | Measured | Where |
|---|---|---|
| **Custom against Direct** | 6.23 %/h against 5.54 %/h in daily use, one run each: about 11 % more drain | `tools/battery_track.sh`, runs of 2026-10-11/12 |
| Sony's sound service, per mode | 10.3 % of a core (10-band EQ + Dynamic Normalizer) against 0.6 % (Direct); about half is the EQ and its limiter | `COMPARE_cinder_sony.md` |
| **Bluetooth radio on, nothing linked, jack playback** | deep idle 0 entries in 3 min against 291 with the radio off | `COMPARE_cinder_sony.md` |
| DAC EQ | no processor time (five biquads inside the codec), jack only | `analysis/RE_codec_tone_table.md` |
| Direct sends 32-bit to the codec | halves the longest period (174 → 87 ms), so more wakes; Direct still measured cheaper overall | `analysis/RE_sound_service.md` §4 |
| Codec standby when idle, CPU cap behind a dark panel on the jack | done, default on | `AUDIT_2026-09-28_power_sound_w1.md` |

Not measured: DSEE HX, Dynamic Normalizer, Vinyl, DC Phase and Clear Bass on their own; the
linear amp against S-Master; any Bluetooth-route figure with effects on.

## 2. What does nothing on this player

| Setting | Why | Cinder since 2026-10-18 |
|---|---|---|
| VPT Surround, Clear Phase, DSEE AI | not in Walkman One's effect chain (`i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc dcphaselinear vinylizer f2i`) | drawn as "Not in Walkman One's sound library", taps ignored |
| ClearAudio+ | `DmpFeature::IsFeaturedClearAudioPlus` = 0 on W1, and the service reads it back 0 (checklist 38.3). Walkman One's player app is the A50's, so its menu still shows it | not offered (since 0.3.17) |
| S-Master high gain | measured inert on this model 2026-08-17 | row cut |
| DC Phase Linearizer | replaced by Clear Bass on this player (the chain patch) | drawn as gone |

The owner's report that toggling VPT did nothing is the first row, not ClearAudio+.

## 3. Proposals, battery first, options kept

In order of value against effort. None is built yet.

1. **Say what costs battery.** Each software effect's row says so when it is on ("runs the
   effects chain"), and the mode line says whether the chain is running or skipped. Small, UI only.
2. **EQ on the DAC by default on the jack.** The DAC EQ is free; the 10-band is about 5 % of a
   core. Offer "Equalizer: DAC (no battery cost) / 10-band (software)", DAC for new players, and a
   "fit my 10-band curve to the DAC's five bands" action so a saved curve carries over.
3. **A Battery profile.** One of the A/B profiles preset to Direct (or Custom with only the DAC
   EQ), assignable per output like any profile.
4. **Measure each effect** (section 4) so the rows in item 1 can give real numbers.
5. **Bluetooth auto off on by default** is the largest measured lever (row 3 of section 1). The
   owner asked to leave Bluetooth as it is, so this stays a recommendation.

Sound profiles: the owner says this "could use some work"; what to change is the open question.

## 4. Measurement plan (adb, cable in, about 5 minutes)

For each of: all off, 10-band EQ, DSEE HX, Dynamic Normalizer, Vinyl, Clear Bass, everything on:
set it, play a 16/44.1 FLAC on the jack for 30 s, run `tools/ssfw_cost.sh 30`. It changes what the
owner hears while it runs, so it waits for a time the player is not in use. Then one off-cable
discharge each way for the default chosen in item 2.
