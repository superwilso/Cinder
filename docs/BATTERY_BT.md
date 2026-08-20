# Battery during Bluetooth playback — method, and the first finding

*2026-08-20. Written before any optimisation, because the last time this project optimised from a
guess ("the visualiser must be the expensive part") the guess was wrong by two orders of magnitude.*

## What is already done

* **The analyzer stops when the panel is dark.** `viz_analyzer_tick` gates on `g_screen_on`, so the
  per-frame IPC into Sony's `AudioAnalyzerService` is not paid with the screen off — which is how a
  Bluetooth session is normally spent.
* **The audio pump backs off when dark:** 20 ms lit → 250 ms dark (100 ms for a grace window after
  a transport press, so a button still feels immediate).
* **Idle draw was measured cable-out at 99.84% @ 598 MHz, 321 ctxt/s, cinder-home 0.65% of a core**
  (memory: `reference_power_measurement`). There is nothing left in the idle path.

## Measuring it — `tools/btpower.sh`

```
tools/btpower.sh start bt      # opening sample; then UNPLUG, play over Bluetooth, screen off
tools/btpower.sh report bt     # replug; closing sample + the deltas
```

Two one-shot samples of **cumulative** counters, so the sampling is the only intrusion and the
window is however long you left it. Run the same length three times — `bt`, `jack`, `idle` — and
compare; a single run tells you almost nothing.

Why it is built this way:

* **adb wakes the core.** The device sits at 598 MHz idle and reads 1.3 GHz the moment a shell
  attaches, and a cable pins the gauge to "charging" so the battery level says nothing. Hence:
  cable out for the window, and cumulative counters rather than instantaneous ones.
* **No daemon.** The first version backgrounded a sampler on the device with `nohup`; adb kills the
  process group when its shell exits, so the closing sample never ran and the file came back with
  an opening block and nothing else.
* **Process names come from `cmdline`, not `comm`.** Sony starts its services under `logwrapper` —
  30 processes on this device report the comm `(logwrapper)` — so matching on comm finds
  `cinder-home` and none of the audio or Bluetooth services.

It reports: CPU busy % and seconds, average clock and the time-in-state histogram, context
switches/s, per-process CPU for cinder-home and every `hagodaemon`, battery capacity/voltage (with
a %/hour extrapolation when it moved), which ALSA substreams were open, and a set of CXD3778GF
registers at both ends of the window.

## The first finding, and the open question

With **nothing playing, no PCM open, and nothing in the jack**, the codec is not asleep:

```
SYSTEM        0x03      OSC_ON     0x10     OSC_SEL   0x01     OSC_EN   0x10
BLK_ON0       0x0F      SD_ENABLE  0x05     PLUG_DET  0x10
PHV_L/PHV_R   0xD8      PHV_CTRL0  0x80     HPOUT2_CTRL1 0x0F  DNC1_START 0x50
```

Oscillators enabled, four block-enables set in `BLK_ON0`, the serial-data path enabled, both
headphone attenuators loaded and the DNC engine's start register non-zero — on a chip that has
nothing to render.

**The question this raises for Bluetooth:** LDAC audio never touches the CXD3778GF — it is encoded
by Sony's `BtTransmitterService` and leaves through the MTK radio (`analysis/E_usbdac_ldac/`). If
these same bits are still set during a Bluetooth session, the device is clocking and biasing a DAC,
a headphone amp and a noise-cancelling engine for an output nobody is listening to, for the whole
session. On a 3.5 mm session they are exactly right and must not be touched.

**Do not change a register before the A/B says so.** The order is:

1. `btpower.sh` runs for `idle`, `jack` and `bt`, same length, cable out.
2. Compare the codec block across the three. If `bt` matches `jack` rather than `idle`, the
   hypothesis holds and the size of the prize is the `jack`−`idle` CPU/voltage gap.
3. Only then look at what turns them off, and how Sony's own stack behaves when it switches routes
   — the driver may own these bits, in which case the lever is a route call, not a register poke.

**Never write `/proc/regmon/<chip>/value`** while chasing this. Selecting a register through
`target` is a read; writing `value` changes the audio hardware under the running player, and the
codec is the one part of this device with no software recovery path.
