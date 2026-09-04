# RE — what the audio hardware can do that Sony's firmware never asks it to

**Date:** 2026-09-04 · **Device:** NW-A55, firmware 1.02 · **Codec:** Sony CXD3778GF
**Status:** control inventory and register map are measured/extracted. **Nothing in the "unused"
column was written to the device** — see the safety note in §2.

The premise, from the FM radio and the USB-DAC work: the firmware's feature set is a Sony product
decision, not the hardware's limit. This is the inventory for the audio path.

## 1. The register map

`/proc/regmon/<chip>/target` prints its own register-name map when read — 210 named registers for
`cxd3778gf`, 425 for `mt6323`. The codec map is also compiled into the kernel as the reg_info table
behind the symbol `cxd3778gf_customer_info` @0xc0bc7d90 (the symbol name is misleading; it is the
regmon table, a header followed by `{const char *name; u32 regnum}` pairs). Both are captured:

- `analysis/kernel/cxd3778gf_regmap.txt` — extracted from the kernel image, 210 registers
- `analysis/kernel/mt6323_regmap.txt` — read from the device, 425 registers

Blocks worth knowing about, by name:

| range | block |
|---|---|
| `0x00`–`0x0a` | device/oscillator/plug-detect/trim |
| `0x10`–`0x19` | `BLK_FREQ*`, `BLK_ON*`, `CLK_EN0..2`, `CLK_HALT`, `SW_XRST0/1` — the power/clock surface used by the standby fix |
| `0x1a`–`0x23` | serial audio ports, `DSD_ENABLE`, `CLK_OUTPUT` |
| `0x24`–`0x30` | codec volumes and sample-rate config |
| `0x36`–`0x3c` | mic volumes and preamp |
| `0x42`–`0x47` | **`HPRM_CTRL0/1`, `HPRM_MEAS`, `HPRM_DATA0..2`** — headphone-impedance measurement |
| `0x48`–`0x4f` | `PHV_*` — the analogue attenuator (see `RE_volume_pop.md`) |
| `0x51`–`0x56` | charge pump `CPCTL1..3`, dither |
| `0x62`–`0x6f` | `LINEOUT_VOL`, `DAMP_*`, `HPOUT2_*` (SE), `HPOUT3_*` (BTL/balanced) |
| `0x70`–`0x7d` | **`MEM_CTRL`/`MEM_ADDR`/`MEM_RDAT`/`MEM_INIT`/`MEM_ISTA`** — coefficient RAM |
| `0x80`–`0xdb` | `DNC*` (digital noise cancelling), `AINC_*`, **`UC_DM*`** (embedded µC data memory), `SMS_*` (S-Master) |
| `0xf0`–`0xfe` | `BUT_TH` (remote button threshold), `OCDDET_*` (overcurrent), interrupt enables/status |

## 2. ALSA controls the firmware exposes but never uses

Card 0 publishes 51 controls. Read on the device, idle:

| control | value | available | note |
|---|---|---|---|
| `headphone smaster se gain mode` | `normal` | `normal` / `high` | **never selected** — see below |
| `headphone smaster btl gain mode` | `normal` | `normal` / `high` | BTL path unused on this model |
| `headphone smaster gain mode` | `normal` | `normal` / `high` | |
| `headphone amp` | `smaster-se` | `normal` / `smaster-se` / `smaster-btl` | `HPOUT3_*` all zero — SE only |
| `output device` | `headphone` | `off` / `headphone` / `line` / `speaker` / `fixedline` | |
| `analog input device` | `off` | `off` / `tuner` / `mic` / `line` / `directmic` | `tuner` is the FM path |
| `playback latency` | `normal` | `normal` / `low` | |
| `master gain` | `30` | `0..30` | already at maximum |
| `master volume` | `63` | `0..120` | codec digital volume, driven by Sony |
| `clock recovery` | `0` | `0..2` | |
| `sound effect` | `on` | boolean | codec-internal |
| `SPDIF In` | `0` | `0..2` | MTK side |
| `Audio I2sout Mch Config` | `5,6` | `0..6` | multichannel I2S out |
| `SAMPLE_ASRC_RATE` | `48000` | `0..192000` | |

**The headline is `headphone smaster se gain mode`.** The S-Master amplifier has a high-gain
setting, the control is present and writable, and Sony ships it at `normal` and never changes it.
`cxd3778gf_put_headphone_smaster_se_gain_mode` @0xc063960c stores the value at `state+0x68` and, if
`output device == headphone`, calls the output reconfiguration path at `0xc063b2e0` — so it takes
effect live rather than only at startup.

**Deliberately not tested on hardware.** Raising headphone amplifier gain is not a reversible
config toggle in the way the codec standby fix was: the change persists until something rewrites
it, and the next time headphones go in at an unchanged volume setting the output is louder than the
user set. That is a hearing-safety question, not an engineering one, and it is the user's call to
make knowingly. What can be said from the code is that the control exists, is writable, and applies
live. What the actual gain difference is in dB is **not** established — `cxd3778gf_device_gain_table`
@0xc0bc8958 holds `{0x00060000, 0xf80000, 0x00000000}` (plausibly Q16: +6.0, −8.0, 0.0) but no code
reference to it was found; it is reached through a registered pointer, so the indexing is unknown
and the dB reading is a guess. It should not be repeated as fact.

## 3. Hardware present that the driver never touches

**Headphone impedance measurement (`HPRM_*`, `0x42`–`0x47`).** The chip has a measurement block with
a control pair, a trigger, and three data registers. The string `HPRM` appears in the kernel image
**only** inside the regmon name table — there is no code that references those registers. Read on
the device, all six are `0x00000000`. So the silicon can measure headphone impedance and this
firmware never asks it to. Sony's higher-end players use impedance to pick a gain mode; that is
presumably what this is for. Acting on it would need the measurement sequence from the datasheet,
which is not available, so this is recorded as an observation and not a lead.

**Coefficient RAM and an embedded µC (`MEM_*`, `UC_DM*`, `DNC_REQ`).** `MEM_CTRL`/`MEM_ADDR`/
`MEM_RDAT`/`MEM_INIT`/`MEM_ISTA` plus `UC_DMCNT`/`UC_DMA`/`UC_DMD_0..4` are a memory port into the
codec's noise-cancelling DSP. The driver drives these through `dnc_port_read`/`dnc_port_write` and
`cxd3778gf_register_dnc_module`, but only for noise cancelling — which is inert on this unit anyway
without Sony NC headphones (see [[reference_nc_needs_sony_hp]]).

**S-Master noise shaper and dither (`SMS_NS_*`, `SMS_DITHER_CTRL0..7`, `SMS_PWM_CTRL0/1`).** A full
tuning surface for the class-D modulator, with no ALSA control on top of it.

**Hardware beep (`BEEP0/1`, `SMS_BEEP_CTRL0/1`) and overcurrent detect (`OCDDET_DSE`, `OCDDET_DBTL`).**

## 4. Confirmed by register read, idle, nothing in the jack

```
PHV_L = 0x64  PHV_R = 0x64     analogue attenuator at maximum (100)
CPCTL1 = 0x84                  charge pump control bits set
HPOUT2_CTRL1 = 0x0F            SE output path up
HPOUT3_CTRL1 = 0x00            BTL/balanced path down — SE-only, as expected for this model
HPRM_CTRL0..HPRM_DATA0 = 0x00  impedance block idle
```

## 4b. The S-Master gain mode — measured, and it is not what it looked like

Tested 2026-09-04 with a bare plug in the jack (nothing on anyone's head, so the interlock could be
forced safely). The control is accepted and reads back `1`. **No codec register changes** — not with
an empty jack, not with a plug in so the driver treats the output as connected, and not after a PCM
open. A full sweep of registers 0x40–0xdb was byte-identical in every case, and byte-identical again
after restoring `normal`.

That is not the same as "the control does nothing", and the driver says why. The stored value is
read back at `0xc063accc` and `0xc063ad7c`, where it selects between return values (2/3, and
14–19) **together with the sample rate** — the comparisons are against `0x15888` = 88200 and
`0x2b110` = 176400. So it is a gain-index chooser, and the index is consumed by
`cxd3778gf_ext_set_gain_index`, part of a whole `cxd3778gf_ext_*` family (`ext_restore_preamp`,
`ext_enable_i2c_bus`, `ext_force_disable`) that drives an **external** preamp stage.

`/sys/bus/i2c/devices` on this unit lists only `mt_m24c16` (an EEPROM), a run of `dummy` slots and
`bq24262_wmport` (the charger). **There is no ext codec device on any I2C bus**, and `regmon`
exposes no such chip either. The most consistent reading is that the gain mode selects an index for
a stage the NW-A55 does not have fitted — which is why nothing observable moves.

Not fully closed: the ext bus may be bit-banged or hang off the codec rather than appearing under
`/sys/bus/i2c`, so "no ext device" is an absence of evidence on the buses that were checked. What
*is* solid is the negative: no register this device exposes changes, in any output state tested. An
ear test or an analogue measurement is the only thing left that could contradict it.

## 5. What this does and does not establish

**Established:** the register map, the full control inventory with current values and ranges, that
the S-Master high-gain mode is available and unused, that the impedance block has no driver code
behind it, and the idle state of the output path.

**Not established:** the dB value of any gain mode; whether `line`/`fixedline` output modes do
anything useful on hardware with no line-out pins; whether `playback latency low` has an audible or
measurable effect. All three are testable, none were tested.

## Related

- `analysis/RE_codec_power.md` — the standby fix, and the idle-power survey
- `analysis/RE_volume_pop.md` — the `PHV_*` attenuator
- `analysis/RE_kernel_power.md` — the SoC-side power framework
- `analysis/kernel/` — the extracted tables
