# The volume-step pop — cause found, mitigation shipped but not enabled

Reported 2026-08-18: *"a small noticeable pop sound when volume is changed when using wired
headphones"*, and then the detail that turned out to be the whole clue — *"only up to volume 100
not beyond"*.

## Where this stands (read this first)

**The cause is known and measured.** Every volume step moves the CXD3778GF's analogue headphone
attenuator (`0x49 PHV_L` / `0x4B PHV_R`) directly, and the curve it follows comes from the output
volume table the codec driver loads at boot. The A50's stock table (`ov_1291.tbl`) has two dead
zones and coarsens toward the top — 4 attenuator counts per step between volume 80 and 100, which
is exactly where the pop was loudest, and nothing at all above 100, which is exactly why it stopped
there. Full working in the SOLVED section below.

**A mitigation is written, built and installed — and has never once run on hardware.** Sony ships a
better table on every stock device (`ov_127x.tbl`, the NW-WM1A's own: monotonic, no dead zones,
finer steps at the top). `cinder-home/src/cinder-voltable.c` installs it, `build.sh` step 6g builds
it, `install_cinderhome.sh` section 1f3 installs it, and the launcher applies it on every boot
because `load_sony_driver` re-applies the stock table each time. But:

* the `voltable` component in `cinder-home/deploy/components.conf` **defaults to `stock`**, and
* `/contents/cinder_voltable.conf` did not exist on the device, and
* until 2026-08-26 the launcher installed on the device predated the voltable block entirely —
  `tools/cinder-install.sh` pushed the binary and the helpers but never refreshed the launcher, so
  the device ran a 2026-08-12 launcher against a 2026-08-26 app. See the comment on `LAUNCHER_SRC`
  in that script.

The launcher is current now, so the switch is one file away: write `wm1a` to
`/contents/cinder_voltable.conf` and reboot. Nothing has been switched, because changing the table
changes what every volume step does and the user has to be told, not surprised.

**Whether it is audibly gone is still unmeasured.** As of 2026-08-26 the reporter says they have not
been hearing it recently and will judge over the next few daily drives — on the STOCK curve, which
is the one that has been running all along. That is a useful observation and it is not a
measurement; the analogue rig (`tools/measure_output.py`) is still what would settle it.

## The history that led there

Everything below is the investigation in the order it happened, including two wrong turns worth
keeping: the metric that measured loudness and called it a pop, and the gain-staging theory that
was plausible and false.

## The measurement

Headphone jack recorded on a PC (`tools/measure_output.py`, Windows ffmpeg backend) while stepping
the volume one press per second with music playing: 60→67, then 103→110.

**The obvious metric gives the wrong answer.** Raw slew — the largest sample-to-sample jump in a
window — put every transient in the *above*-100 half:

```
transients >4x median raw slew:  72   ALL of them above volume 100
```

That is an artefact, not a result. The above-100 half is simply LOUDER, and louder programme
material has more slew. Measuring the pop requires normalising by the local signal level:

```
slew_normalised = max|x[n] - x[n-1]| / rms(window)      per 10 ms window
```

Outliers (>3x the median normalised slew) that land ON a volume-step boundary (±120 ms):

| | on-step transients |
|---|---|
| **below volume 100** | **26** |
| **above volume 100** | **1** |

That reproduces the report exactly. The pop is real, it is tied to the step itself, and it
effectively stops above 100.

## What it is NOT

**Not the shell.** `apply_volume` writes exactly one control, `master volume` (numid=10), through
one `amixer` invocation. Nothing else is touched on a volume change.

**Not a gain-stage switch.** The first theory was that something re-arranges the gain staging at
step 100 — plausible, and wrong. Reading every other gain control after each step across the whole
range:

```
vol =  0  20  40  60  80  90  95  99 100 101 105 110 120
master gain (numid=13)   30  30  30  30  30  30  30  30  30  30  30  30  30
HWGAIN      (numid=46)  0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0 0,0
smaster     (numid=28)    0   0   0   0   0   0   0   0   0   0   0   0   0
smaster se  (numid=29)    0   0   0   0   0   0   0   0   0   0   0   0   0
```

Nothing moves, at any step, let alone at 100. There is no ALSA control changing state.

**Not audible with the output idle.** The same stepping with nothing playing produced zero
transients above +10 dB over a -39.8 dBFS floor. A real negative, but a narrow one: with no stream
the output amp is idle, so this only says the pop needs an active signal path — which is consistent
with what a listener hears.

## What it therefore is

The volume step is an attenuator change inside the CXD3778GF, and what each step DOES is decided by
the output-volume table loaded at boot (`ov_1291.tbl` on this unit, via
`dacdat auto $PRODDEV` in `/bin/load_sony_driver` — see `reference_walkmanone_extract`).

A click that appears per-step below a fixed point and not above it is what a table crossing a
range boundary on each step looks like: below the boundary the attenuator is being re-ranged as
well as re-set; above it, one register moves smoothly.

## The experiment that would settle it

**Swap the table and repeat the measurement.** Both curves are already on the device:

```sh
dacdat ovt /system/usr/share/audio_dac/ov_127x.tbl   # the NW-WM1A curve
dacdat ovt /system/usr/share/audio_dac/ov_1291.tbl   # the A50's own (default)
```

Then re-run the normalised-slew capture across 95..105.

* **pop moves to a different step, or disappears** → it is the TABLE, not the silicon. That makes
  it fixable by shipping a different table, and turns this into a product decision rather than a
  hardware limitation.
* **pop stays at exactly the same step** → it is the codec's own attenuator topology, and the only
  mitigations left are software: ramp through intermediate steps instead of jumping, or mute for a
  few milliseconds across the change. Both are cheap and both cost responsiveness.

Reverting is a reboot either way — `load_sony_driver` reloads the 1291 set on every boot, so a
table swap cannot be left behind by accident.

> Volume-table changes alter what every step does. Headphones off the head, volume at minimum, and
> raise one step at a time — step N on a new table is not the loudness step N was on the old one.

## Reproduction recipe

```sh
# music playing, cable from the headphone jack to a PC input
python3 tools/measure_output.py --seconds 21 --label "pop baseline"    # while stepping
```

Step the volume one press per second across the range under test, then analyse with normalised
slew rather than raw level or raw slew. The analysis is a dozen lines of Python; the important part
is dividing by the local RMS, because the whole first attempt at this measured loudness and called
it a pop.

## SOLVED — it is the PHV analogue attenuator, and the curve is a Sony table (2026-08-18)

The pop was never findable from the shell or the mixer because the thing that moves is **inside the
codec**. `/proc/regmon/cxd3778gf` (see `reference_fm_regmon` for how that door opened) makes it
directly observable: register `0x49 PHV_L` / `0x4B PHV_R` is the analogue headphone attenuator, and
it is what every volume step actually changes.

Measured against `numid=10 'master volume'`, 1 s settle per point so nothing is caught mid-ramp:

```
vol    0   20   40   60   80   90  100  110  120
PHV    4   80  100  100  148  188  228  228  228
```

**Two dead zones, both real and both reproducible:**

* **40 → 60 does nothing.** PHV sits at 100 across a fifth of the scale.
* **100 → 120 does nothing.** PHV pins at 228 and never moves again.

So **40 of the 120 UI steps are inert on the wired output**, and the second dead zone is exactly the
"26 pops below 100, 1 above" asymmetry this document opened with: above 100 there is only one pop
because above 100 *nothing changes*. Below it, every step is a fresh analogue gain change — and the
steps get bigger toward the top (1 count per step at vol 20-40, **4 per step at 80-100**), which is
where the pops were worst.

### The curve is a loadable table, and Sony ships a better one

That shape comes from the output volume table `load_sony_driver` installs at boot — `ov_1291.tbl`,
chosen because `ro.product.device` is `BBDMP5_linux`. `dacdat ovt <file>` swaps it live through
`/proc/icx_audio_cxd3778gf_data/ovt`. All three tables are already on the device
(`analysis/RE_walkmanone_extract.md`). A/B, same sweep, same instrument:

| vol | 0 | 20 | 40 | 60 | 80 | 90 | 100 | 110 | 120 |
|---|---|---|---|---|---|---|---|---|---|
| `ov_1291` — stock A50 | 4 | 80 | 100 | **100** | 148 | 188 | 228 | **228** | **228** |
| `ov_1280` — Walkman One's BBDMP2 | 4 | 80 | 100 | **100** | 148 | 188 | 228 | **228** | **228** |
| `ov_127x` — **NW-WM1A** | 4 | 44 | 84 | 124 | 164 | 184 | 204 | 224 | 228 |

**Walkman One's table is behaviourally identical to stock.** The files differ by md5 but produce the
same attenuator values at every point sampled — so whatever the model swap buys, it is not the
volume curve. That is worth knowing before anyone pays for it.

**The WM1A table is straightforwardly better on this hardware:**

* **no dead zones** — monotonic across the whole 0..120 range;
* **the full scale is usable**, where stock wastes 40 steps;
* **smaller steps at the top** (204 → 224 → 228 against stock's 4-per-step run to 228), which is
  precisely where the pop is loudest.

### What this does NOT settle

Whether the pop is *audibly* gone. PHV moving in smaller increments should mean a smaller
discontinuity, but "smaller" is an inference — confirming it needs the analogue rig
(`tools/measure_output.py`) or ears, and neither was available here. The mechanism and the curve are
measured; the audible result is not.

Also note the WM1A curve is **quieter at the same number** through the mid range (vol 20: 44 vs 80).
Maximum output is unchanged (both reach 228), but a habitual volume setting will sound different, so
this is a change to announce rather than to slip in.

The remaining suspects for the pop's *character* — `SMS_SFTRMP` (soft ramp, reads **0 = disabled**)
and the driver's `fade_amount` / `timed_mute_ms` parameters — are untested and are the next thing to
try if a table swap is not enough.

Two of those three are free to try and need no helper. Confirmed present on device 2026-08-26:

```
/sys/module/snd_soc_cxd3778gf/parameters/{fade_amount, monvol_wait_ms, timed_mute_ms}
```

`SMS_SFTRMP` is the third and is a codec register, which means a WRITE to
`/proc/regmon/cxd3778gf/value`. That is the one operation this project has ruled out standing: the
codec is the part with no software recovery path. Trying it needs an explicit decision, not a
convenient moment.
