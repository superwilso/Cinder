# The volume-step pop — measured, localised, not yet fixed

Reported 2026-08-18: *"a small noticeable pop sound when volume is changed when using wired
headphones"*, and then the detail that turned out to be the whole clue — *"only up to volume 100
not beyond"*.

Confirmed objectively the same day. This is what is known, what has been ruled out, and the one
experiment that would settle the cause.

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
