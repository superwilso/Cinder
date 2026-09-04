# RE — the DAC never sleeps, and what else is awake at idle

**Date:** 2026-09-03 · **Device:** NW-A55, firmware 1.02, Cinder as Home app
**Status:** codec finding measured and the mechanism verified on hardware; the cinder-home wiring
is built and tested but **device-unverified** (installing it needs a reboot).

Prompted by the user's suspicion that "some of the SoCs on the device, like the DAC, don't go to
sleep." That is correct for the DAC, and the survey below says which other chips are and are not.

## The idle state that started it

Conditions: screen off (backlight node reads 0), Bluetooth off (`bt_on=0`), **nothing in the
headphone jack** (`jack status se = 0`), no PCM substream open anywhere — every
`/proc/asound/card0/pcm*/sub*/status` read `closed`. The device had been sitting like this for
minutes, and CPU-wise it was behaving beautifully: 99.89 % of the window at the minimum 598 MHz,
cinder-home using 0.36 % of a core.

The codec was wide awake:

```
standby             = off
deep early suspend  = off
headphone amp       = smaster-se

DEVICE_ID=0x2B  BLK_ON0=0x0F  CLK_EN0=0x17  CLK_EN1=0x4A  CLK_EN2=0x60
CODEC_EN=0x82   CPCTL1=0x84   DAMP_REF=0x01 HPOUT2_CTRL1=0x0F
OSC_ON=0x10     OSC_EN=0x10   SW_XRST0=0xFF SW_XRST1=0xFF
```

Blocks enabled, three clock-enable registers non-zero, the oscillator running, everything out of
reset, the charge pump control bits set, and the S-Master single-ended output path selected.
`HPOUT3_*` (the balanced/BTL path) was all zeros, so only the SE amp was up — driving a jack with
nothing plugged into it.

## Why

The driver is not missing the feature. `/proc/kallsyms` lists the whole power surface:

```
cxd3778gf_suspend / cxd3778gf_resume
cxd3778gf_early_suspend / cxd3778gf_late_resume
cxd3778gf_put_standby / cxd3778gf_get_standby
cxd3778gf_put_deep_early_suspend / cxd3778gf_get_deep_early_suspend
cxd3778gf_put_standby_control / cxd3778gf_get_standby_control
```

The `_control` pairs are ALSA kcontrol callbacks, and both controls are exposed to userspace:
`numid=35 'standby'` and `numid=36 'deep early suspend'`, both BOOLEAN, both readable and writable
on `/dev/snd/controlC0`.

Two routes into standby therefore exist, and neither is taken:

1. **The early-suspend hook never fires.** `cxd3778gf_early_suspend` is registered with the kernel's
   early-suspend chain, which on this system is only driven from a `/sys/power/state` write. Nothing
   writes it — `autosleep` is `off` — so the chain never runs. Cinder blanks the panel by writing
   the backlight node and calling Sony's `display_backlight(0)`; neither touches early suspend.
2. **Nothing sets the control.** The audio path *clears* standby when a PCM opens, but nothing ever
   sets it again. So the codec wakes for the first sound after boot and stays awake until power-off.

## Standby is real, not a flag

Writing the control drops the chip off the I2C bus entirely — regmon cannot read a single register:

```
codec: set_standby(1) -> 0
codec: asleep    DEVICE_ID=UNREADABLE BLK_ON0=UNREADABLE CLK_EN0=UNREADABLE …
```

That is the measurement that matters. `standby` reporting "on" would only be a flag; DEVICE_ID
becoming unreadable is the chip actually powered down. **This is why the probe prints registers
rather than the control** — verify by the second, never the first.

Waking restores every register exactly:

```
codec: set_standby(0) -> 0
codec: awake     DEVICE_ID=0x2B BLK_ON0=0x0F CLK_EN0=0x17 … CPCTL1=0x84 HPOUT2_CTRL1=0x0F
```

## The safety property the fix rests on

**Opening a PCM clears standby by itself.** Verified directly: standby on → codec dead on I2C →
`aplay -f S16_LE -r 44100 -c 2 -d 1 /dev/zero` → afterwards `standby = off` and every register back
to its pre-standby value.

So setting standby while idle cannot break playback. The worst case is the wake latency of the next
track, which the driver already pays on the first play after every boot.

## It does NOT explain the deep-idle block

Tested rather than assumed, because the temptation to join these two findings is obvious and it is
wrong. With the codec held in standby for 30 s:

```
codec awake,   20 s: dpidle_cnt[0]=0   by_vtg 553085 -> 557906   (~241/s)
codec standby, 30 s: dpidle_cnt[0]=0   by_vtg 558191 -> 565379   (~240/s)
```

`dpidle_cnt` stays at zero and `by_vtg` climbs at an identical rate. The SoC's deep idle is blocked
by something else entirely — so as a *measurement* these are two independent problems, and treating
them as one would have been wrong.

**RESOLVED 2026-09-04, and they share a root cause after all** — see `analysis/RE_early_suspend.md`.
`by_vtg` is not a voltage check despite the name: `dpidle_handler` gates on
`mt_cpufreq_earlysuspend_status_get()`, i.e. on the kernel's early-suspend flag, which nothing on
this system ever sets. `cxd3778gf_i2c_probe` is also a registrant on that same early-suspend chain.
So the standby fix below is treating a **symptom**: the root cause is that Cinder's screen-off never
tells the kernel the device went idle. The fix below is still worth having — it is narrow, proven on
hardware, and independent of whether the early-suspend question is ever resolved — but it is not the
whole story, and this section originally implied there was nothing linking the two.

## The other chips — survey

`/proc/regmon` exposes five: `cxd3778gf`, `Si4708icx`, `afe_reg`, `mt6323`, `bq24262`.
Selecting a register through `target` is a read; **`value` is never written** (SECURITY.md).

| Chip | State at idle | Verdict |
|---|---|---|
| **CXD3778GF** (DAC/amp) | awake, amp + charge pump up | **the finding** — fixed below |
| **Si4708icx** (FM tuner) | `POWERCFG=0x4000` — ENABLE bit clear, `STATUS_RSSI=0` | **already powered down.** Fine |
| **afe_reg** (MTK audio front-end) | `AFE_DAC_CON0=0x01` (AFE_ON) with no PCM open | **not a power finding** — every audio clock group is already gated; likely a stale bit, see below |
| **mt6323** (PMIC) | `ANALDO_CON1=0x8C00`, `CON2=0xC201`, `CON7=0x10`, `CON8=0x04`, `DIGLDO_CON0/2=0xC001`, rest 0 | raw data only — bit meanings not established, no claim made |
| **bq24262** (charger) | active | expected, on the cable |

Also checked and **not** a finding: `green`/`LED_GREEN_*` at 255 is the charge indicator, explained
by the device being on the cable. `lcd-backlight` is 0. No PCM open. No userspace wakelocks
(`/sys/power/wake_lock` empty).

**The AFE looked like the same finding. It is not — measured 2026-09-04.** `AFE_DAC_CON0` bit 0 is
the global AFE enable and it is set with nothing playing, which does have the same *shape* as the
codec finding. But the early-suspend work (`analysis/RE_early_suspend.md`) settled it as a side
effect: with the deep-idle gate open, the only clock blocking deep idle was
`dpidle_block_mask[CG_PERI0]=0x400` (USB0). **Every audio clock group — CG_AUDIO, CG_AUDIO0,
CG_AUDIO4 — read zero**, i.e. already gated as far as the idle path is concerned.

So the likeliest reading is a **stale register bit in a block whose clock is already off**, not a
powered AFE. That is a materially different thing from the codec, which was provably powered: it
answered on I2C, with the amp path selected and the charge pump up.

Still not fully closed — a set bit in a gated block should be confirmed against the audio clock
states in `/proc/clkmgr/clk_test` — but it is no longer a candidate for wasted power, and the
"same shape as the codec" framing above was too strong.

## The fix

`cinder_codec_set_standby()` drives the control directly over `/dev/snd/controlC0` with two ioctls —
no libasound, because the binary that gains the dependency would be the Home app, and a Home app
that cannot start is a device recovered by hand. Controls are addressed **by name**, not by numid:
numid is an artefact of driver registration order and would silently address the wrong control on a
firmware that registers a different set.

No setuid helper is needed. `/dev/snd/controlC0` is owned by `system`, and cinder-home runs as uid
100, which *is* `system` on this device.

cinder-home puts the codec into standby after **30 s** of screen-off and not-playing, and takes it
out on the transition back. The grace period stops a pause between tracks from cycling the amp. It
deliberately does not sleep the codec during Bluetooth playback — BT does not use the codec, so
there is a further saving there, but `g_playing` is true and a conservative first cut is worth more.

## What is verified and what is not

- **Verified on hardware:** the idle state, that standby powers the chip down, that waking restores
  it exactly, that a PCM open clears standby by itself, and that the shim drives all of this
  correctly through `cinder-probe --codec`.
- **NOT verified:** the cinder-home wiring, because installing it needs a reboot. It builds, the
  46-case recovery matrix and 446 Rust tests pass, and the mechanism underneath it is proven.
- **NOT measurable here:** the actual power saved. This unit has no `current_now`, only
  `voltage_now`, and a USB cable pins the gauge at Full — so milliamps cannot be measured at all,
  and idle draw must be measured cable-out. The case for the change rests on what the registers say
  is powered, not on a battery figure.

## Related

- `cinder-audio/include/cinder_codec.h`, `cinder-audio/src/codec_shim.cpp`
- `cinder-home/src/probe.cpp` `--codec` — the device test
- `analysis/RE_volume_pop.md` — the same codec, the PHV analogue attenuator
- `analysis/RE_storagemgr.md` — the other 2026-09-03 finding, unrelated
