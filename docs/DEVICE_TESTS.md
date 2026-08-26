# Device test backlog — things only ears or a hand can settle

Everything that could be measured from the host or over adb has been. What is left here needs you
holding the device. Ordered by payoff. Each item says what to do, what a PASS looks like, and what
to do if it fails.

Last updated 2026-08-17.

> **For the full device-gated list — not just the ear tests — see
> [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md).** It consolidates this file, `ROADMAP.md`'s P0
> table, `STATUS.md`'s running unverified notes, `ldac-bridge/TEST.md` and `BATTERY_BT.md` into one
> ordered run sheet, with the safety rules at the top. This file stays the place for the procedures
> and the results.

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

## 12. Volume-change POP on wired headphones, only below volume 100

**Full write-up: [`analysis/RE_volume_pop.md`](../analysis/RE_volume_pop.md).** Confirmed by
measurement 2026-08-18; cause narrowed to the output-volume table; one experiment left to settle it
(swap `ov_127x` for `ov_1291` and re-measure).

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

### Investigated 2026-08-18 — what it is NOT

**The shell is not doing it.** `apply_volume` writes exactly one control, `master volume`
(numid=10). Nothing else.

**The driver is not switching gain stages either.** Stepping volume across 0..120 and reading every
other gain control after each step:

```
vol=  0 .. 120   master_gain=30  HWGAIN=0,0  smaster=0  se=0     (constant throughout)
```

`master gain` (numid=13), `HWGAIN` (numid=46) and both S-Master gain modes (numid=28/29) never
move. So the "gain staging re-arranged at step 100" theory is DEAD — there is no visible control
changing at any step, let alone at 100.

**Not captured with the output idle.** Recording the headphone jack while stepping 60→66 and
104→110, one step per second, with nothing playing: zero transients above +10 dB over a -39.8 dBFS
floor. That is a real negative but a narrow one — with no stream the output amp is idle, and a
volume write into an idle amp is not the event being reported.

### CONFIRMED BY MEASUREMENT 2026-08-18

Captured the headphone jack while stepping volume one press per second, 60→67 and 103→110, with
music playing. The right metric is slew NORMALISED BY LOCAL LEVEL — raw slew is useless here,
because louder audio has more slew and the above-100 half is simply louder, which produced a
completely inverted first answer.

Outliers (>3× median normalised slew) landing ON a step boundary:

| | on-step transients |
|---|---|
| below volume 100 | **26** |
| above volume 100 | **1** |

That is the reported behaviour, reproduced objectively. Combined with every mixer control being
constant across 0..120, the pop is the CXD3778GF's own attenuator changing, driven by the
`ov_*.tbl` output-volume table — not by anything the shell or the ALSA control layer does.

### What is left

The pop is inside the codec: the volume STEP itself is an attenuator change in the CXD3778GF, and
the `ov_*.tbl` output-volume table decides what each step does. A click that stops above a fixed
step is consistent with the table crossing a range boundary per step below it and not above.

**To reproduce properly:** play a steady quiet tone (not music — music masks it), capture, and step
one press at a time across 95..105. A tone makes a click obvious in a way programme material does
not. `tools/measure_output.py --wav` will analyse the capture.

**Worth trying once reproduced:** load the WM1A curve (`dacdat ovt ov_127x.tbl`) and repeat. If the
pop moves to a different step or disappears, it is the table and not the silicon — which would also
make it fixable by shipping a different table.

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
  log, repeat — 24 track changes. `cinder-probe --pump` can drive playback, so it is automatable.
  (**Not `--play`** — that mode is deliberately the framework-DEAD control in the A/B and cannot
  connect; see the 2026-08-25 results below.)
  Parked instead: this only labels a picker for a control **Sony's own A50 UI does not expose at
  all**, and the Tone screen ships without it rather than with numbers Cinder invented.
- **External tuning blobs** (#63) — the 192 KB nested `NW_WM_FW.UPG` under `/etc/.mod/tunings/`
  is still unpacked. Decides whether the full Walkman One signature is reachable without flashing.

## 13. Bluetooth reconnect — the radio does it now, one case left to prove

**Pushed and waiting for a reboot** (`cinder-home` md5 `cbdbcbbd…`). Two calls the app never made
are now wired: the service's own retry thread, and the connectable window that lets a headphone's
power-on land. Both were verified as primitives against the live radio on 2026-08-19 — the retry
mode arms, the service pages on its own interval, and the count Cinder ships (20) is accepted.

**What is still unproven is the case that matters to you:** whether a headphone switched on near
the player actually connects on its own, without touching the Walkman.

```sh
# 1. with the headphones OFF, disconnect so the test starts from a known state
cinder-probe --btlink drop
# 2. open the window, then SWITCH THE HEADPHONES ON inside it
cinder-probe --btlink wait 90 keep
```

A link inside those 90 s is the whole "reconnect is slow" complaint answered. Nothing happening is
also a result — it means the accept path needs more than this call, and the local ladder stays the
mechanism. Either way `--btlink status` afterwards shows where the radio ended up.

Then the same thing through the app rather than the probe: reboot, connect the headphones, switch
them off, and switch them back on a minute later without touching the player. The log should show
`bt: SetConnectRetryMode(true, 5, 20) rc=1` and `bt: RequestStartConnectWait() rc=0` at the drop.

> One caveat worth knowing while testing: the retry mode is **service state that survives the app**.
> If a test leaves it armed, the radio keeps paging every ~15 s until the count runs out, whatever
> the UI says. `cinder-probe --btlink retry off` clears it.

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

---

## RESULTS 2026-08-25 — the Phase-0 probe sweep, and a build that did not build

A device session driven entirely over adb: no flash, no reboot, nothing written to `/system` except
`cinder-probe` itself. Phase 0 of [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) is now mostly closed.
The session ended when the device stopped enumerating on USB (see "How this session ended").

### 0. The gate that was supposed to catch this, and didn't

**`build.sh dev` FAILED at HEAD.** The tree did not compile for the device:

```
src/main.cpp:99:47: error: use of undeclared identifier 'uc_'
   99 |     ucontext_t* uc = static_cast<ucontext_t*>(uc_);
```

Introduced by `f2f41a8` ("Deep sweep: … -Werror turned on"), which silenced an unused-parameter
warning by commenting the name out — `void* /*uc_*/` — while the `#if defined(__arm__)` body below
still used it. **Host builds skip that block, so CI stayed green and only the ARM cross-compile
broke.** `probe.cpp:117` had the identical defect. Both now name the parameter and `(void)uc_` it on
non-ARM.

This is exactly the hole gate 0.1 exists to cover ("a green CI says nothing about whether the thing
links for the device"), and it is the second time an ARM-only path has been broken by a host-only
check. After the fix: ARM link OK, GLIBC ceiling OK (`2.4…2.18`, under 2.23), qemu construction
preflight PASS, launcher escape matrix 46/0, harness all scenarios PASS. **0.1 and 0.2 PASS.**

### 0a — LDAC control plane: **Q1 PASS**, first validation of the headline feature

`cinder-probe --ldac`, with the framework started and pumped:

```
ldac: SetLdac(true) / SetLdacSoundQuality(Auto) / SetCurrentSource(true)
ldac: Q1 socket name = 'pst::services::bttransmitterservice' (len 35, pump ticks 7)
ldac: Q1 PASS — connected to the transmitter's audio socket
```

A real name and a real connection, not uninitialised stack. `TEST.md`'s first question is answered:
**the control plane does make `BtTransmitterService` open its audio socket.** The source was
released again at the end of the run.

**Q2 INCONCLUSIVE** — no capture PCM exists to open (`Invalid value for card` on every candidate).
That needs the gadget in UAC mode with a PC actually feeding audio, and per the FuncMode notes
entering USB-DAC drops adb, so it is a hands-on item.

### 0b — analyzer: **PASS**

Frames flow. `cinder_analyzer_start` rc=0, then with audio playing alongside it:

```
analyzer: frames=4  bands=12  vals[0..7]= 156336 1141720 1457742 870771 124308 195243 1224925 1704183
analyzer: frames=25 bands=12  vals[0..7]= 59387 691385 330639 1083123 165325 114236 695333 191591
```

**12 bands, and the values are LINEAR** (roughly 0 … 1.7e6), not dBFS — worth knowing before
`spectrum::from_bands` is trusted to auto-detect. Frame rate came in under the 20 Hz requested
(~6/s observed); not investigated.

### 0c — PlayStatus with music actually playing: **PASS**, via `--pump` (not `--play`)

```
pump: audio_init=0  IsConnected=1
SetTrackSequence OK; prepare ChangePlayState(1) rc=0
ChangePlayState(Play=2) rc=0
pump:   ALSA pcm4p = state: RUNNING
pump: t+5s ticks=2616 events=10 pos=5000/268333 uri(95)=…/01 - American Football - Never Meant.flac
```

Position advances in real time, the listener fires, and the ALSA device really is RUNNING. The whole
RE'd chain (JSON → Node → NodeTrackSequence → SetTrackSequence → ChangePlayState) is verified live.

**Note for anyone reading the old procedure: `--play` cannot work and is not supposed to.** It is
the deliberate framework-dead half of the A/B — it calls `cinder_audio_init` with no
`StartForApplication` and no pump, so `Connect` returns uninitialised stack every time
(`rc=-1229155576`, `rc=-1094638144`, …) and logcat shows the service never saw a transaction.
`--pump` is the mode that plays.

**This also settles 3d.** `duration_raw = 268333` for a 4:28 track. **It is milliseconds.**

### 0d — `--discover`: root-caused, and the long-standing note was wrong

The dump ran with a live link (`audio_init=0 IsConnected=1`) and **the PlayStatus block still came
back 128 zero bytes.** Two separate defects, both now fixed:

1. **`--discover` never pumped.** It called `cinder_audio_init("cinder")` with no framework and no
   pump, so the Connect reply was never dispatched and the link was never actually up. Every dump
   before today was taken over a dead connection. The recorded explanation — *"every previous dump
   was all zeros because nothing was playing"* — **was wrong**; nothing was ever connected.
2. **`PlayStatus` is per-controller.** `cinder_audio_dump_status` reads *this process's* controller,
   which reports what this client was told to play — not what the device is playing. Running a
   player alongside it in another shell (or letting the Home app play) dumps zeros no matter how
   healthy the link is. **The dumping process has to own the playback.**

`--discover` now starts the framework, pumps, waits for `IsConnected`, and takes media paths of its
own, which it plays before capturing:

```
cinder-probe --discover /contents/cinder_discovery.txt /contents/MUSIC/…/01 - … .flac
```

**Built and pushed but NOT re-run** — the device left the bus first. Re-running it is the one
outstanding piece of Phase 0.

### 0e — the MediaStore half that could be settled off-device: **recovered**

`libMediaStoreServiceClient.so` exports the scanner as a concrete class, so **no vtable slots have
to be guessed at all** (checklist rule 4 is satisfied outright):

```
pst::services::mediascanner::MediaScanner::MediaScanner(IMediaStoreService*)
pst::services::mediascanner::MediaScanner::Scan(IMediaScannerListener*, pst::mediascanner::language_t)
pst::services::mediascanner::MediaScanner::ScanFile(IMediaScannerListener*, std::string const&, language_t)
pst::services::mediascanner::MediaScanner::Cancel()
pst::services::mediascanner::MediaScanner::OnProgress(int, int) / OnFinished(status_t)
pst::services::mediastore::MediaStoreService::GetInstance() / GetMediaStoreClient()
```

`MediaStoreService` is hosted by `hagoromo9`, sharing a process with `PlayerService`
(`init.hagoromo.rc:89`). **`strace` is present on the device at `/system/xbin/strace`**, so the
second half — is a rescan app-driven or mount-driven — is runnable, but it needs a real USB-MSC
disconnect, and MSC takes over the gadget and kills adb. Hands-on item.

### 0f — the notes vs the device: **the notes hold**

`--userpreset`, unchanged from the RE record:

```
LIVE (entry)  … sel=2
slot 0/1/2    dsee=0 vpt=0/1 dc=0/5 norm=0 vinyl=0/7 tone=1 tv=0,0,0 eq6p=0 sel=1
slot 3,4      EffectCtrlDmp.cc !!! unknown UserPresetNo
```

Three real slots, every one holding `SelectUsingEq=1` (the six-band), 3+ out of range. The probe put
the chain back exactly as found (`sel=2` on re-read), so it is safe to run. `--inpath 2`, with music
playing, in the service's own log:

```
Eq10band::UpdateProcCond … isproc is 1
EqTone::UpdateProcCond   … isproc is 0
```

**Cinder's 10-band EQ is in the signal path.** `--btwho` ran clean and read `GetBtStatus=7` (off),
nothing connected — consistent, but the with-a-peer half still needs headphones.

One observation not in the notes: under selector 2, **`Eq6band` also reports `isproc is 1`**, and
`FilterChain` lists `eq6band` and `eq10band` as separate filters. Both report `no desired value,
skip`. Not necessarily a contradiction of the alternatives model, but it is not what the model
predicts either, and it is worth a look before the six-band decision in §10 is treated as final.

### Two more probe bugs found and fixed

* **`--fx` exited 42 after a clean run**, with a `PC=0x00000000` backtrace. Same teardown fault
  `--tone` already documents: the pump thread is still inside `libpstcore` while static destructors
  unwind through Sony's vtables. `--fx` and `--discover` now `_exit()` like `--tone` and `--eq` do.
  `--fx` now exits 0. (`--userpreset` and `--btwho` were already clean at exit 0.)

### New: `cinder-probe --transport` — Phase 3, by measurement instead of by eye

Four Phase-3 items are the same shape (an RE'd value nothing ever confirmed) and are all visible in
the position/URI the listener already reports, so none of them needs a finger on the glass:

| Item | What the mode does | PASS |
|---|---|---|
| 3a play-by-index | `play_tracks({A,B}, start=1)` | the SECOND path is what plays |
| 3c drag-to-seek | `seek_ms_origin(0, 60000)` | lands at ~60 s, not at now+60 s (that would mean 0 is Current, not Begin) |
| 3e repeat-one | repeat on, park 6 s from the end | the SAME uri restarts near zero |
| 3f queue end | repeat off, single track, park 6 s from the end | records state/playing at the boundary — an observation, per the checklist |

```
cinder-probe --transport <trackA> <trackB>      # both >70 s
```

**Built and pushed; NOT yet run** — the device left the bus before it could. This is the first thing
to run when it is back.

### How this session ended

The device stopped enumerating on USB after the `--ldac` run (which itself exited 0 and released the
source). It disappeared from Windows entirely — present only under usbipd's *Persisted* list, not
*Connected* — so this was not a WSL passthrough drop. A reboot and a fired auto power-off both look
like this from the host side and cannot be told apart without the device in hand;
`/contents/cinderhome.log` will say which, and if auto power-off is configured then this is **2D.4
firing on an idle device** and is a pass, not a fault. Read that log before assuming anything broke.

---

## RESULTS 2026-08-26 — the Phase-3 transport four, and the enum that made repeat-one dead

Run with `cinder-probe --transport <trackA> <trackB>`, two FLACs over 70 s, no flash. Everything
below is measured; nothing is inferred from the harness.

### 3a — play-by-index: PASS

`play_tracks({A,B}, start=1)` played **B**. `pos=1000/318333 uri=05 - … Lost in the Flood.flac`.
The start index selects the track; the primitive under tap-a-row is correct.

### 3c — `media_origin_t::Begin == 0`: PASS

`seek rc=0 before=1000 after=63000/318333`, twice, reproducibly. Origin 0 is **BEGIN**, absolute —
a seek to 60 s lands at 60 s regardless of where the head was. Drag-to-seek lands where it is
dropped.

**But the first two attempts read INCONCLUSIVE, and the reason is worth keeping.** The probe drove
`cinder_audio_seek_ms_origin()`, which is the RAW call, while the track was streaming. The engine
refuses that: `MediaEnginePlayer.cc:221 SeekTime(): Bad parameter. ignored`, once per attempt, in
logcat. Position moved 1000 → 4000 purely because playback carried on during the call — no seek
happened at all, and `rc=0` said nothing, because `SeekTime` is `void`.

This is the same trap `player_shim.cpp:497` already documents: the engine will not seek while it is
streaming, so `cinder_audio_seek_ms()` — the shipping path — brackets the call in a transport-level
pause/resume. **`cinder_audio_seek_ms_origin()` does not.** The probe now pauses around it
(`seek_origin_paused`), and the answer fell out immediately. The shipping path was never broken;
only the diagnostic was.

### 3e — repeat-one: PASS, but only after fixing the enum

`OneTrackMode::On` was **1**, an assumption written into `playerservice_abi.hpp` and flagged there
as unverified. It is wrong. The device says:

```
repeatsweep: v=0 -> stopped at end
repeatsweep: v=1 -> stopped at end
repeatsweep: v=2 t+6s pos=318333/318333 playing=1
             v=2 t+7s pos=1000/318333   playing=1
repeatsweep: RESULT OneTrackMode::On == 2
```

`On = 2`. Values 0 and 1 both run the track to the end and stop; 2 wraps cleanly, same URI, still
playing. With the enum corrected, 3e passes through the ordinary shipping path.

**Repeat-one had therefore never worked, and nothing could have told us.** It was applied at the
right moment — before `SetTrackSequence`, on both the sticky and the live path — with the wrong
value, and `SetOneTrackMode` is `void`, so the service silently did nothing. Only 0 (off) and 2 (on)
are established; what 1 means is unknown, and 3+ should not be assumed unused.

Found with `cinder-probe --repeatsweep <track> [value]`, added for this. It drives the value through
a diagnostic override (`cinder_audio_set_one_track_raw`) applied where the sequence is built, so
each candidate is tested on the path that actually reaches the service. Nothing in the app calls it.

### 3f — end of queue: the observation

Repeat-one off, single-track sequence, parked 6 s from the end:

```
t+5s pos=303000/304173 playing=1 state=128
t+6s pos=304173/304173 playing=1 state=1
t+8s pos=304173/304173 playing=0 state=1
```

At the boundary **position pins at duration and `playing` goes 1 → 0**; it does not reset, and the
URI does not change. That pair — `playing == 0 && pos >= tot` — is the signal a repeat-all would
have to watch for and override. `state` also moves (128 → 1) but is not a clean flag: an earlier run
read `state=1` mid-track, so 128 is not simply "playing" and should not be used as the trigger.

### An audio-path wedge worth knowing about

After roughly a dozen probe sessions in a row, the **first** play of each new session began failing:

```
GapOMXCmp.c:320 onEventError cmp = OMX.SONY.REN.AUDIO, Error = [8000100b]
GapOMXCmp.c:347 GAP_E_UNSUPPORT_FORMAT
```

Position sticks at ~72 ms, `GetCurrentStatus` returns 0/0, and the renderer walks
`Executing → Idle → Loaded`. It is not format-specific (a different album fails identically), not a
settle-time problem (60 s idle did not clear it), and not caused by the enum change (3a never sets
repeat). Later plays *within the same session* work — 3e passed on every one of these runs — so it
is the first sequence after a fresh connect that dies.

Not chased further, because the results above were already in hand and clearing it means a reboot,
which lands the device on stock. Flagged rather than fixed: if `cinder-home` can hit this on its
first play after a service restart, a user would see a track that will not start.

### 0d — the PlayStatus offset map: DONE

`--discover` with a media path, on a fresh boot, with the app not holding the output. The dump is
no longer zeros, and every field falls out against a known track (2000 ms into a 304173 ms FLAC,
44.1 kHz / 16-bit / stereo):

| Offset | Bytes | Value | Field |
|---|---|---|---|
| `+0x00` | `02 00 00 00` | 2 | playstate — 2 while playing, matching `ChangePlayState(Play=2)` |
| `+0x44` | `ff ff ff ff` | -1 | unknown sentinel |
| `+0x48` | `d0 07 00 00` | 2000 | **position, ms** |
| `+0x4c` | `2d a4 04 00` | 304173 | **duration, ms** |
| `+0x5c` | `02 00 00 00` | 2 | channels |
| `+0x60` | `10 00 00 00` | 16 | bits per sample |
| `+0x64` | `44 ac 00 00` | 44100 | sample rate |
| `+0x68` | `80 88 15 00` | 1411200 | bitrate (44100 x 16 x 2 — derived, not from the file) |
| `+0x6c` | `81 …` | — | URI `std::string` (already confirmed; unchanged) |

Nothing shipping depends on these yet: `cinder_audio_position()` reads the listener callbacks, not
the struct. What the map unblocks is the format badge (rate/depth/channels are right there) and a
better play-state source — `cinder_audio_play_state()` currently returns the LISTENER's state int,
which its own comment calls uncalibrated and which was observed as both 1 and 128 while playing.
`PlayStatus+0x00` was a clean 2 throughout. Not changed here; flagged for the next pass.

### Why the earlier dumps were zeros, and it was never "nothing was playing"

The renderer was failing to start, and the error it reports is a red herring:

```
DmcAndroidAudioRendererCmp.c:691  Failed WMX_AudioOutput::Open() (0x80001009)
DmcAndroidAudioRendererCmp.c:1513 Failed emptyThisBufferForAudio() (0x8000100b)
GapOMXCmp.c:347                   GAP_E_UNSUPPORT_FORMAT
```

`0x80001009` is `OMX_ErrorHardware` and comes FIRST: the audio output would not open. `0x8000100b`
is `OMX_ErrorStreamCorrupt`, which Sony surfaces as `GAP_E_UNSUPPORT_FORMAT` — so the log blames the
file when the real failure is one line up and is about the device. Symptom: position sticks at
~72 ms, `GetCurrentStatus` returns 0/0, renderer walks `Executing -> Idle -> Loaded`.

The cause is ordinary contention: **`cinder-home` was running and owning the audio output.** Not a
format problem (a different album failed identically), not settle time (60 s idle did not clear it),
not the OneTrackMode change (3a never sets repeat). It cleared completely on the next boot, and the
same `--discover` that had dumped zeros dumped the full map.

**So: a probe that plays audio needs the Home app not to be holding the output.** Run playback
probes on a fresh boot before touching the app, or expect the first play of the session to die with
a message that points at the file.

### Two invocation gotchas that cost time

- **`/contents` is mounted `noexec`.** `install.md` and `docs/adb_setup.md` both tell you to run
  `adb shell '/contents/cinder-probe …'`; that returns `permission denied` no matter what the mode
  bits say. Use `/system/vendor/unknown321/bin/cinder-probe`, or push to `/tmp` (tmpfs, exec, 32 MB)
  for a build that is not installed yet.
- **`LD_LIBRARY_PATH` is not optional** even from `/tmp`: without it the loader cannot find
  `libPlayerServiceClient.so`.
