# Device test backlog — things only ears or a hand can settle

Everything that could be measured from the host or over adb has been. What is left here needs you
holding the device. Ordered by payoff. Each item says what to do, what a PASS looks like, and what
to do if it fails.

Last updated 2026-08-17.

---

## 1. The EQ was never in the signal path — confirm the fix

**This is the big one.** Sony's Equalizer, 6-band EQ and Tone Control are ALTERNATIVES;
`SetSelectUsingEq` decides which is in the path, and nothing in Cinder had ever called it. The
device sat on `1` = the **6-band EQ**, which Cinder does not even expose. Every band the EQ screen
has written since June was stored by the sound service and never applied.

Settled on device without ears, by reading the sound service's own log — `UpdateProcCond … isproc
is N` is the service saying whether that effect is actually processing:

```
EqType 0 -> nothing      2 -> the 10-band Equalizer   (what Cinder drives)
       1 -> the 6-band   3 -> Tone Control
```

`cinder-home` now selects 2 (or 3 when Tone Control is on) every time it applies the chain.

**Test.** Play something familiar. Open Equalizer, drag the bottom three bands to +10 dB and the top
three to −10 dB.

- **PASS:** unmistakable. Bass huge, treble gone.
- **FAIL:** if it is still inaudible, run `cinder-probe --inpath 2` with music playing and check
  `logcat | grep isproc` — `Eq10band … isproc is 1` means the selector took and the problem is
  further down the chain (ClearAudio+ or Source Direct, both of which override it).

Then set the EQ back to whatever you actually like.

---

## 2. Tone Control — the new screen

Sound ▸ Advanced ▸ **Adjust bands**. Three bands, ±10 dB, tap above or below the zero line.

Units are measured, not assumed: raw ±20 = ±10 dB, and BASS/MIDDLE/TREBLE are ordinals 0/1/2
(the service logs `eqtone,type=N` as each is written).

**Test.** Turn Tone Control ON in Advanced, then open Adjust bands and swing BASS to +10.

- **PASS:** audible, and the Equalizer screen's curve stops mattering while Tone Control is on —
  that is the point of the selector, they are alternatives.
- Turn Tone Control off again and confirm the EQ comes back.

**Known gap, deliberate:** no centre-frequency picker. `SetToneCenterFreq` echoes 0..7 but has no
dB twin and no recovered frequency list, so a picker would be showing numbers Cinder invented. To
settle it: play music, run `cinder-probe --tone`, then `logcat | grep "FREQ = \["` — the DSP prints
the real Hz when the effect is configured. Not worth a session on its own.

---

## 3. VPT room / DC Phase filter / DSEE HX Custom / Vinyl character — the labels

All four are wired and land on the device (verified with `cinder-probe --fx`), but the LABELS come
from Sony's translation catalogue in catalogue order. Catalogue order is almost certainly enum
order — it has been every other time — but nothing proves it.

**Test.** With music playing, cycle each and listen:

| Row | Cinder's labels, in order |
|---|---|
| VPT (Surround) | Studio · Club · Concert Hall · Matrix |
| DC Phase Linearizer | 6 filter types |
| DSEE HX Custom | Standard · Female Vocal · Male Vocal · Percussion · Strings |
| Vinyl Character | Standard · Turntable · Arm Resonance · Surface Noise |

- **PASS:** the differences are in the right direction — "Concert Hall" bigger than "Studio",
  "Female Vocal" doing something to vocals.
- **FAIL:** if the order feels shuffled, note which sounds like which and the lists get reordered.
  Nothing else changes.

Do this AFTER item 1, or you will be listening to effects that ClearAudio+ or the selector is
hiding.

---

## 4. Walkman One's sound signature — needs headphones OFF

`cinder-signature.sh` reproduces MrWalkman's paid "sound signature" byte-for-byte on device without
flashing. It is three bytes: which ALSA PCM device the stream opens, and the CPU clock floor.

```sh
cinder-signature.sh status          # what is live now
cinder-signature.sh set pv2         # or pv1 / clock / hw1 / hw2
cinder-signature.sh revert          # back to stock
```

Needs a reboot to take effect (the HAL is loaded once). Safe: it verifies the live md5 before
touching anything, keeps a pristine `.stock` copy, and rebuilds every variant from that backup.

**Test.** Same track, same volume, A/B `stock` vs `pv2`. Expect subtle — it is a PCM-device and
clock-floor change, not an EQ.

---

## 5. `dacdat` volume tables — DO THIS ONE DELIBERATELY

> **Headphones OFF and volume at MINIMUM before you run this.** Loading a different output-volume
> table changes what every volume step does, and `VOL_LIMIT 0` removes the EU output cap. This is
> the one item in this file that can hurt.

Walkman One's ZX300 tables (`ov_1280`, `ov_127x`, `tc_1280`, `tc_127x`) are already staged in
`/system/usr/share/audio_dac/` and are **inert** — `load_sony_driver` still runs
`dacdat auto $PRODDEV` with `ro.product.device` = `BBDMP5_linux`, so boot keeps loading the A50's
own 1291 set.

```
dacdat ovt FILE                 # output volume table
dacdat tct FILE                 # tone control table
dacdat auto MODEL VOL_LIMIT     # MODEL: BBDMP2_linux|BBDMP3_linux|BBDMP5_linux
                                # VOL_LIMIT: 0 | 10
```

The stock binary already accepts `BBDMP2_linux` — Walkman One's model — and it is byte-identical to
W1's. Reverting is a reboot (the boot script reloads the 1291 tables every time).

---

## RESULTS 2026-08-17 — items 1, 2, 3 PASS

EQ audibly works, Tone Control works, the EQ comes back when Tone Control is switched off, and the
VPT rooms sound like their labels. The selector fix is confirmed by ear (`SelectUsingEq=2` now).

Two things the service's own log gave up while you ran the probes:

**a. The out-of-range behaviour has a name.** `EffectCtrlDmp.cc:677 !!! unknown gain value` fires
exactly four times per sweep — for -30, -24, +24, +30. So the ±20 clamp is the service's own table
boundary, and the UI clamp is in the right place.

**b. The 6-band EQ IS writable — presets 9 and 10 are the Custom slots.** CONFIRMED
(`cinder-probe --eq6custom`): writing 6/-6 to bands 0 and 1 is rejected under presets 0..8 and
0..8 only, and STICKS under 9 and 10. Preset 11 is out of range. So the enum is **0..10**, the
first nine are Sony's fixed named curves, and the last two are Custom 1 / Custom 2 — both flat
today because nothing has ever written them.

Original evidence:
`EffectCtrlDmp.cc:534 !!! cannot set value except for UserCustom preset` is the service saying the
band writes were rejected because presets 0/6/7 are not the Custom slots. And `!!! unknown preset.
use fallback` fires for 11..15 but NOT for 9 and 10 — so **the enum is 0..10, eleven slots**, and
9/10 are almost certainly Custom 1 / Custom 2: both read flat because nothing has ever written them.
Next probe run should set preset 9, write bands, and confirm they stick. That would make Sony's
6-band editable rather than preset-only, and settle the name mapping at the same time (eleven
ordinals, and the catalogue's seven names plus Off plus two Customs is nine — so two more names are
still missing from the list I read).

---

## 7. UI: sliders now drag, and the stutter is MEASURED — RETEST

Was: every slider tap-only, one tap per step; balance draggable but slow; audio stuttering while
adjusting the EQ.

**The stutter had a number behind it** (`cinder-probe --fxtime`, 32 reps each):

```
get_eq_band        median      1 us      <- reads are free
is_vpt_on          median      1 us
set_eq_band        median  10304 us      <- 10 ms, and the SAME for an unchanged value
set_tone_value     median   6956 us
set_vpt_mode       median   2981 us
set_dsee_hx        median   1056 us
```

A UI frame is 16 ms. The shell was re-applying the whole chain on every change — ten EQ bands plus
~fifteen effect calls, **103 ms, six frames**, per motion event. Three fixes, all in now:

1. **Drag.** `cinder_scrub_to` takes y as well as x, so a VERTICAL slider can be scrubbed. The band
   is captured at finger-down and never re-picked, so a diagonal sweep cannot rewrite bands you
   were only passing over.
2. **Write only what moved.** The shell caches what it last told the sound service. Note line 3 of
   the table: writing an unchanged value costs the full 10 ms, so the service does not short-
   circuit and the caller has to.
3. **Throttle during a drag** — at most one apply per 60 ms, because even one 10 ms write per frame
   is 60% of the budget. Safe because the release always re-emits the action, so the final value is
   never the one that got dropped.

**Retest:** drag each EQ band top to bottom with music playing. Expect the knob under your finger,
no steppiness, and no audible break. Then the same on Tone Control, and the balance slider.

## 8. USB mass storage could not be turned off — FIXED, RETEST

Cause: leaving mass storage cleared `g_msc_active`, and the very next watcher tick saw the PC host
still connected and auto-entered again two ticks later. The modal was not reappearing — it was
being **re-entered, once every couple of seconds, forever**.

Fix: leaving by hand while the cable is still in latches "the user said no", and auto-MSC stays off
until the data host goes away. Entering by hand clears the latch, because asking for mass storage
is changing your mind.

**Retest:** plug into the PC, let mass storage come up, press Turn Off. Expect it to stay off with
the cable still in. Unplug and replug — it should offer mass storage again.

---

## 9. Sony's saved setups — measured, and the A/B plan is OFF

`cinder-probe --userpreset` reads Sony's `SaveUserPreset`/`LoadUserPreset` slots without writing
them (it loads each into the live chain, records what came back, and puts the chain back). Result:

```
slot 0,1,2   vpt mode=1  dc type=5  vinyl type=7  tone=1  SelectUsingEq=1
slot 3,4     everything 0, SelectUsingEq=0
```

Three real slots — matching the catalogue's "Saved Sound Settings 1/2/3" — and 3+ is out of range.

**This kills the idea of backing Cinder's A/B onto them**, which was the remaining half of #62:

- every stored slot holds `SelectUsingEq = 1`, the SIX-band, which Cinder does not expose. Loading
  one would silently take Cinder's EQ back out of the signal path — the exact bug just fixed.
- loading an out-of-range slot **zeroes the whole chain**, selector included, with no error.
- the slots carry state Cinder does not model (6-band preset, tone centre frequencies), so a
  round trip through them would lose or invent settings.

Cinder's A/B already works, lives in Cinder's own settings file, and is covered by tests. The only
thing Sony's slots would add is "the stock UI sees the same setups", on a device that boots Cinder
as Home. Not worth the failure modes.

**What DID come out of it:** the tone-system selector is no longer cached in the shell. It is now
re-asserted on every apply, exactly like `SetBtAudioSoundEffect`, because these slots prove
something on this device can move it back to the 6-band behind Cinder's back.

---

## 11. LIVE NOW: the NW-WM1A volume curve is loaded

Ran 2026-08-17 with a track playing and volume at 0:

```sh
dacdat ovt /system/usr/share/audio_dac/ov_127x.tbl   # rc=0
```

That file is md5 `39a60adc…`, which is **byte-identical to the NW-WM1A's own `ov_127x.tbl`** (and
to the ZX300's `ov_1288`). The A50's own curve is `ov_1291.tbl`, md5 `bb5ccae7…`. Both were already
on the device from the earlier staging pass.

**It is NOT the "WM1Z sound signature".** It is the WM1A's output-volume table: a mapping from each
of the 120 volume steps to an attenuation. It changes how loud a given step is. Tonality lives in
the external tuning, which is the encrypted blob in §9 and is not reachable.

**Raise the volume ONE STEP AT A TIME from 0.** A different table means step N is not the same
loudness it was before.

Revert, either:

```sh
dacdat ovt /system/usr/share/audio_dac/ov_1291.tbl   # back to the A50 curve
```

or just reboot — `load_sony_driver` runs `dacdat auto $PRODDEV` every boot and reloads the 1291 set,
so this change does not survive a restart and cannot be left behind by accident.

There is no read-back: `dacdat` reports rc only. Judge by level.

---

## 12. Volume-change POP on wired headphones, only below volume 100 (REPORTED 2026-08-18)

A small but audible pop each time the volume changes, on the 3.5 mm output. **It stops above
volume 100** — steps 100..120 are silent, steps below pop.

That threshold is the interesting part and probably names the cause. The volume scale is 0..120 and
the output path has more than one gain stage:

* `numid=10 'master volume'` 0..120 — the table-driven attenuator (`dacdat` output-volume tables)
* `numid=13 'master gain'` 0..30 — a coarse gain
* `numid=28/29 'headphone smaster (se) gain mode'` — normal/high, the CXD3778GF's own output stage

A pop that disappears past a fixed step is the signature of **gain staging being re-arranged at
that boundary** — below it something switches per step and clicks; above it the same step only
moves one attenuator. Worth reading `master gain` and the smaster gain mode across the range and
finding what changes at 100; if a discrete control is being toggled per step, the fix is to stop
toggling it, or to ramp rather than jump.

Not yet investigated. Reproduce with wired headphones and step the volume one press at a time.

---

## 10. Still open, lower priority

- **Bluetooth scan-and-pair UI** (#25) — the listener ABI is recovered, the screen is not built.
  Pairing works today via the stock flow; this is convenience.
- **Backlight true-off** (#55) — parked. Needs a Hold-switch failsafe first, or a dark screen with
  no way back is a brick you have to wait out.
- **6-band EQ — editable, but deliberately NOT wired.** Presets 9 and 10 are the writable Custom
  slots, so Cinder *could* offer a six-band editor. It should not: only ONE tone system is in the
  path at a time (that is what `SelectUsingEq` decides), so a 6-band editor would compete with the
  10-band Cinder already drives and lose — fewer bands, same range, and a preset name mapping that
  is still a guess. The measurement is recorded so the decision is a decision rather than a gap.
  If you ever boot the stock UI, listing its Equalizer preset names in order settles 1..8 in
  thirty seconds.

- **Tone Control centre frequencies in Hz — attempted with music playing, blocker now exact.**
  `cinder-probe --tonefreq` sweeps all 8 ordinals on all 3 bands with Tone Control on and EqType 3
  selected. The service accepts every write (`eqtone,type=N,centerfreq=F` for all 24) and reports
  the effect as live — `EqTone::UpdateProcCond … isproc is 1` — but then says:

  ```
  EqTone::UpdateProcCond(bool, bool):379 no desired value, skip
  ```

  and **`SetEffectParam` never runs**, so the DSP never prints the Hz. Zero `FS = [` lines across
  the whole sweep, and `--inpath 3` reproduces it. So a live parameter write only STAGES the value;
  the coefficients are computed at TRACK-CONFIGURE time (`EqTone::Config(TrackParam&)` →
  `SetEffectParam`), which is the one path that logs.

  Recipe to finish it, if it is ever worth the churn: set an ordinal, force a track change, read the
  log, repeat — 24 track changes. `cinder-probe --play` can drive playback, so it is automatable.
  Parked instead: this only labels a picker for a control **Sony's own A50 UI does not expose at
  all**, and the Tone screen ships without it rather than with numbers Cinder invented.
- **External tuning blobs** (#63) — the 192 KB nested `NW_WM_FW.UPG` under `/etc/.mod/tunings/`
  is still unpacked. Decides whether the full Walkman One signature is reachable without flashing.

---

## Running a probe

```sh
adb shell
export LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib
/system/vendor/unknown321/bin/cinder-probe --fx        # whole chain state + the three gates
/system/vendor/unknown321/bin/cinder-probe --tone      # tone + 6-band units, ranges, preset curves
/system/vendor/unknown321/bin/cinder-probe --inpath 2  # which tone system a given EqType selects
```

`--fx` first, always: ClearAudio+, Source Direct and `BtAudioSoundEffect` each make everything else
inaudible while still reading back exactly as written.
