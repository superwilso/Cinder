# `headphone amp` — what Sony's three-item enum actually selects

Measured 2026-09-08 on the NW-A55, jack wired to a PC line input, nothing on anyone's head.

`amixer -c0 numid=27 'headphone amp'` offers `normal` / `smaster-se` / `smaster-btl`. Sony ships
`smaster-se` and never writes the control. `analysis/RE_hardware_surface.md` §2 listed it as unused
and left it there. It is not a dead control: `normal` selects a **different amplifier**, and on this
unit it measures better than the one Sony ships.

## 1. What changes in silicon

Full 210-register sweep through `/proc/regmon/cxd3778gf`, taken **while the PCM was open** (the
codec leaves the I²C bus in standby, so an idle sweep reads nothing — that is Cinder's own power fix
working, not a broken probe). Only these differ, `smaster-se` → `normal`:

| register | `smaster-se` | `normal` | reading |
|---|---|---|---|
| `BLK_ON1` `0x13` | `0x00` | `0xF0` | four analogue blocks **powered up** — the linear amp |
| `HPOUT2_CTRL1` `0x67` | `0x0F` | `0x00` | S-Master single-ended output stage **off** |
| `SMS_PWM_CTRL0` `0xd2` | `0x01` | `0x82` | class-D modulator reconfigured |
| `SMS_PWM_CTRL1` `0xd3` | `0x20` | `0x00` | " |
| `PHV_L`/`PHV_R` `0x49`/`0x4b` | `0x64` | `0x49` | analogue attenuator 100 → 73 |
| `CODEC_SDIN2VOL` `0x29` | `0xC7` | `0xC2` | digital trim |
| `DAMP_VOL_CTRL1` `0x64` | `0x13` | `0x10` | digital trim |

`HPOUT3_*` stays `0x00` in both — the BTL/balanced stage has no hardware on this model, as already
established. So `normal` is not "amp off": it powers up a separate block and shuts the class-D down.
That is the chip's conventional linear headphone amplifier. **S-Master is the class-D; `normal` is
the analogue one.**

The driver drops `PHV` by 27 steps when it switches, which is a gain-matching move — it only does
that if the linear path is meaningfully hotter.

> Correction to an earlier note in this session: a first pass read `PHV_R` at `0x4a` and reported
> `0x00000000`. `PHV_R` is `0x4b`. It tracks `PHV_L` exactly in both modes.

## 2. What comes out of the jack

Control signal: 2 s tones at −6 dBFS — 20/50/100/1k/5k/10k/15k Hz — with 0.25 s gaps, 48 kHz stereo,
5 ms raised-cosine fades. Same file every run (`aplay -D hw:0,0`), `master volume` pinned at 60,
`master gain` 30. Captured at 48 kHz on the PC. Two runs per mode.

| | `smaster-se` | `normal` |
|---|---|---|
| level @1 kHz | −28.67 / −28.62 dBFS | −25.80 / −25.80 dBFS |
| **gain difference** | — | **+2.85 dB** |
| THD @1 kHz | 0.218 % / 0.217 % | 0.113 % / 0.122 % |
| THD @100 Hz | 0.373 % / 0.485 % | 0.317 % / 0.377 % |
| THD @5 kHz | 0.052 % / 0.071 % | 0.030 % / 0.030 % |
| response tilt 20 Hz→15 kHz | −0.77 / −0.44 dB | **+0.05 / +0.04 dB** |
| idle floor, stream open | −58.8 / −58.5 dBFS | −54.6 / −58.3 dBFS |

Repeatability is 0.02 dB on level and ~0.005 % on THD, so the differences above are real and not
run-to-run scatter. The one column that is **not** a result is the idle floor: the recorder's own
floor sat at −53.7 to −60.8 dBFS across the same runs, so both modes are at or under the measurement
floor and nothing can be concluded about amplifier noise from this rig.

`normal` is flatter (a flat 0.05 dB versus a ~0.6 dB treble-up tilt), roughly **half the distortion**
at every frequency measured, and 2.85 dB louder.

## 3. What this does NOT establish

**The load.** A PC line/mic input is high impedance. A class-D output stage is designed to drive
16–32 Ω; a linear stage on the same chip may be specified for a much lighter load and sag, distort or
current-limit into real headphones. Every number above is an unloaded measurement. **It does not
predict how `normal` sounds into headphones, and it must not be shipped on the strength of it.**

**Battery.** `BLK_ON1` going `0x00` → `0xF0` powers four analogue blocks, and turning S-Master off
gives up exactly the efficiency the "battery-efficient digital amp" exists for. This device exposes
no current sense anywhere ([[reference_battery_facts]]), so the cost cannot be measured directly —
only inferred from runtime, over hours.

**Headroom.** Clipping at `master volume` 120 was not measured: the Walkman already overloads a PC
input at volume 60, so the recorder clips before the Walkman does.

**Coupling.** Whether either path is capacitor-coupled is not answerable from registers, and an
unloaded high-impedance measurement cannot see a coupling capacitor's high-pass corner at all — into
a high-Z load any sane coupling cap is flat well below 20 Hz. Establishing that needs a real load.

## 4. Persistence

`headphone amp` survives a PCM close/reopen (set to `0`, played, closed — still `0`). It is a runtime
mixer value, so it does **not** survive a reboot, and nothing in Cinder sets it. Shipping it would
mean re-asserting it at boot and on every route change, the same way the volume resync has to be.

## 5. Status

Measured, documented, **not shipped**. The device was restored to `smaster-se` / volume 54.
The next step, if it is taken, is a listening test into real headphones — which is also the only
test that can find the current-drive problem the bench rig is blind to.

---

# Audio-quality survey, same rig, 2026-09-08

With the bench rig standing, every other output-side control on card 0 was swept for anything that
changes the signal. Same control file, same capture chain, `smaster-se`, `master volume` 60.

| lever | result | verdict |
|---|---|---|
| `headphone amp` `normal` | +2.85 dB, half the THD, flat vs a 0.6 dB tilt | real — see above, needs a loaded test |
| **stock vs WM1A volume curve** | **stock has 20 fully dead steps and a 1 dB/10 near-dead zone** | **the win. Already built, not enabled** |
| `hw:0,0` vs `hw:0,4` (Walkman One "signature") | −28.64 dBFS both, THD 0.153 % vs 0.138 % | no audible difference — confirms `RE_walkmanone_extract.md` |
| native 44.1 / 96 kHz on `hw:0,0` | `rate: 44100 (44100/1)`, `rate: 96000 (96000/1)` | no forced resample. Nothing to fix |
| `sound effect` off | −14 dB level, THD 0.155 % → 0.662 % | **not an effect — it is signal path. Leave on** |
| `clock recovery` 0 → 2 | −28.64 dBFS, THD 0.155 % vs 0.153 % | inert |
| `master gain` | already 30 of 30 | nothing to gain |
| `l`/`r balance volume` | both 0 of 88 | already centred |

## The volume curve is the real finding

`cinder-voltable` and the WM1A table have been in the tree since the volume-pop work, gated on
`/contents/cinder_voltable.conf`. That file exists on this device and **contains `stock`**, so the
stock curve is what is running — the better one has been one word away since 2026-09-01. Measured acoustically at the jack (1 kHz, `hw:0,0`, `smaster-se`):

| master volume | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 100 | 110 | 120 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **stock** dBFS | — | −39.7 | −34.7 | −29.7 | −28.6 | −27.7 | −22.7 | −17.8 | **−12.8** | **−12.8** | **−12.8** |
| **wm1a** dBFS | −54.1 | −45.5 | −39.5 | −34.7 | −29.7 | −25.7 | −23.2 | −20.7 | −18.3 | −15.8 | −13.1 |

* Stock: **volume 100 to 120 is one level.** The top 20 of 120 steps change nothing at all.
* Stock: 50→70 moves 1.0 dB where the rest of the scale moves 5 dB per ten steps.
* WM1A: monotonic end to end, and 2.5 dB per ten steps across the top — 0.25 dB per step, which is
  fine control exactly where stock has none.
* Same ceiling: −12.81 vs −13.09 dBFS, a 0.28 dB difference. **The WM1A curve gives up no loudness.**
* Usable range 26.9 dB (stock) vs 41.0 dB (WM1A) — the extra is at the quiet end, where stock
  bottoms out early.

### Correction to `RE_volume_tables.md`

That document says every volume step moves the analogue attenuator, "so reading PHV back across the
range **is** the curve". It is not the whole curve. Read live at each step:

```
vol=120 PHV_L=0xE4 SDIN2VOL=0xA7      vol=60  PHV_L=0x64 SDIN2VOL=0xC7
vol=100 PHV_L=0xE4 SDIN2VOL=0xA7      vol=40  PHV_L=0x64 SDIN2VOL=0xD3
vol=80  PHV_L=0x94 SDIN2VOL=0xBB      vol=20  PHV_L=0x50 SDIN2VOL=0xD3
```

Volume is split between an analogue attenuator (`PHV_L`/`PHV_R`) and a digital one
(`CODEC_SDIN2VOL`). In the PHV flat zone at 40–60 the **digital** stage is still moving, so that zone
is not dead, only shallow — the acoustic measurement shows 1 dB across 50–70 rather than zero. The
100–120 zone is the fully dead one: both stages are pinned there, and the output level confirms it.

A PHV-only sweep therefore over-reports one dead zone and under-reports the top. `--volcurve` is
still the right silent instrument; it just needs `CODEC_SDIN2VOL` in the sweep to be the whole curve.

### To enable

```sh
adb shell 'echo wm1a > /contents/cinder_voltable.conf'
```

The launcher applies it on every boot (Sony's driver reloads the stock table each boot, so this is
not an install-time patch). `echo stock`, or removing the file, reverts. It changes what every volume
number does — at a given number the WM1A curve is quieter through the mid range — so it is a change
to tell the user about, not to slip in.

## Method notes

* Control signal: 2 s tones at −6 dBFS, 20/50/100/1k/5k/10k/15k Hz, 0.25 s gaps, 5 ms raised-cosine
  fades, 48 kHz stereo. Same file every run.
* Capture: PC line input at 48 kHz mono. **Absolute THD figures are rig-limited** — the input's own
  distortion dominates at 20 Hz (1.0–1.6 % in every run including the quietest) — so only
  *differences between runs* mean anything here. Level differences are good to about 0.02 dB.
* The codec leaves the I²C bus in standby, so every register read has to happen with the PCM open.
