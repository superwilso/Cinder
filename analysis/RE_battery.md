# The battery: what this device will actually tell you, and what it will not

Measured on the unit 2026-08-26. Written because the question "can Cinder show more battery
information?" has a much shorter answer than it looks like it should, and the short answer is worth
recording so nobody spends another evening looking for a current sensor that is not there.

## The whole inventory

Four numbers from sysfs, three temperatures, and seven charger registers. That is everything.

```
/sys/class/power_supply/battery/     -r--r--r--   capacity  status  health  present  online
                                                  voltage_now
/sys/class/thermal/thermal_zone{0,1,2}/temp       mtktscpu  mtktspmic  mtktsabb
/proc/regmon/bq24262/{target,value}  -rw-------   7 registers, root only
```

The sysfs battery node and the thermal zones are **world-readable**, so cinder-home reads them at
uid 100 with no helper at all. Only the charger needs privilege.

## What is NOT there — the part that matters

**There is no fuel gauge on this platform.** No coulomb counting, no `current_now`, no
`charge_full`, no cycle count, no cell thermistor. The `battery` power-supply class exports seven
properties and five of them are booleans and strings:

```
POWER_SUPPLY_NAME=battery      POWER_SUPPLY_STATUS=Charging   POWER_SUPPLY_HEALTH=Good
POWER_SUPPLY_ONLINE=1          POWER_SUPPLY_PRESENT=1
POWER_SUPPLY_VOLTAGE_NOW=4092901                              POWER_SUPPLY_CAPACITY=99
```

Three dead ends were checked before concluding that:

* **`/sys/devices/platform/mt-auxadc/`** publishes `AUXADC_Channel_N_Offset` and `_Slope` for 16
  channels and nothing else. Those are calibration constants, not live readings — there is no node
  that returns a converted channel value.
* **`/proc/mtk_battery_cmd`** — the usual MediaTek fuel-gauge control surface — does not exist on
  this kernel. `/sys/class/hwmon` does not exist either.
* **The bq24262's `BATTERY_CURRENT` register is a SETTING, not a measurement.** It is the charge
  current the IC is configured to deliver. The part has no ADC and reports no current. The register
  name misleads; reading it as a measurement would put an invented number on screen.

So the battery tracker (`tools/battery_track.sh`) cannot be improved by reading current directly.
Inferring drain from cumulative counters, which is what it already does, is the only method
available on this hardware.

## The charger, live

`/proc/regmon/bq24262/target` reads as the register-name table and writes to select a register;
`value` reads that register over I2C. Both are `-rw------- root root`, so the app needs a helper.
The whole map, read while charging at 99%:

| reg | name | value |
|---|---|---|
| 0 | `STATUS` | `0x10` |
| 1 | `CONTROL` | `0xAC` |
| 2 | `BATTERY_VOLTAGE` | `0x78` |
| 3 | `VENDOR` | `0x46` |
| 4 | `BATTERY_CURRENT` | `0x10` |
| 5 | `VIN_MINSYS` | `0x04` |
| 6 | `SAFETY` | `0x18` |

**Only `STATUS` is decoded anywhere in Cinder, and deliberately so.** Bits 6:4 are the charge state
machine and bits 2:0 the fault code; `0x10` is therefore STAT=001 "charge in progress", fault 0.
That decode is the only one with independent corroboration — it read 001 at the same moment sysfs
`status` said `Charging`, and it is the field that would change first if charging stopped.

The other six are shown as raw hex and left undecoded. This project has no bq24262 datasheet. A
plausible-looking bit split for the input current limit or the regulation voltage would produce a
number that looks like a measurement and is not one, which is the exact failure this codebase has
cleaned out of the Sound and Advanced screens twice.

## Battery care (Itawari) — it works, and the "90%" is not what you will see

Sony's battery-care setting is `PowerMgrServiceClient::EnableItawariCharging` /
`IsItawariChargingEnabled`, wired through `cinder-audio/src/power_shim.cpp`. Every description of
it, Cinder's own Settings row included, said it caps the charge at 90%.

**Measured with care ON, from 123 samples logged by `tools/battery_track.sh` over 2.56 h on the
cable:**

```
max voltage_now across every sample     4.0932 V
max voltage_now while status == Full    4.0870 V
samples reporting Full                  53
```

and the climb, sampled by hand on the way up:

```
20:0x   cap=91   4.047 V   Charging
20:16   cap=99   4.092 V   Charging
20:56   cap=99   4.0929 V  Charging   STAT=001   (stable, six samples over 48 s)
later   cap=100  4.0840 V  Full
```

The level climbed past 90 without pausing and the charge **terminates at about 4.09 V**. A normal
full charge on this chemistry is ~4.20 V, so the cell is being held roughly 0.11 V short of full.
**The cap is real and it is working** — that is the entire point of the feature and it is what
actually slows ageing.

What it is *not* is a cap at 90% of the gauge. The gauge scales against the capped ceiling rather
than the cell's true one, so the protected state reports **100%**.

That made the old Settings row value — `ON · 90%` — a promise of a number the user was never going
to see. It says `CARE ON` now, and the Device screen's footer says what the cap actually looks like:
a voltage that tops out well below 4.2.

> **Correction, same day.** A first pass at this said the charge "plateaus at 99% and never reaches
> Full". That was drawn from a single 48-second window and was wrong — it does reach `Full` at 100%,
> 53 times in this sample set. The durable finding is the CEILING VOLTAGE, which 123 samples agree
> on, not what the gauge or the status string say at the top.
>
> Still worth doing: a full charge cycle with care OFF, to confirm the cell reaches ~4.20 V when it
> is allowed to. That would turn "4.09 is a cap" from a strong inference into a measured contrast.

## What Cinder does with all this

* **`cinder-home/src/cinder-battery.c`** — setuid-root, **read-only**. Selects each of the seven
  registers and prints them. It deliberately does NOT chmod the regmon nodes the way `cinder-fm`
  does for the tuner: widening the FM chip means a bad write detunes a radio until the next
  `Open()`, while widening this one means any local uid can reprogram a lithium battery charger's
  regulation voltage, current limit and safety timer. Nothing in Cinder ever needs to write these,
  so the helper never makes it possible. `value` is opened `O_RDONLY` and the only write it makes is
  the register selector, from a fixed compile-time list.
* **`player/cinder-ui/src/device.rs`** — Settings ▸ Device. Started as a Battery screen and grew,
  because the same trip through sysfs that answers "how full is it" also answers "how hot is it",
  "what clock is the CPU at" and "how much music space is left" — all world-readable files the app
  was already allowed to open and simply never did. Battery first (level, status verbatim, voltage,
  health, charger state, the care toggle), then die temperatures, CPU clock/governor/cores, memory,
  storage and uptime.
* **Voltage is shown to three places on purpose.** The interesting question on this device is where
  the charge tops out, and 4.09 versus 4.10 is exactly the distinction two places would lose.
* **The temperatures are labelled for what they measure** — CPU, Power IC, Analog block — not as a
  battery temperature. There is no cell thermistor exposed, and the PMIC zone reads several degrees
  above the others even at idle, so passing it off as a cell or ambient reading would be a guess
  dressed as a measurement.
* **Nothing shows a time-to-empty.** With no fuel gauge there is no honest way to compute one.
* **The expensive readings are gated on the screen being open** (`cinder_device_wants_detail`), the
  same way the spectrum analyzer is gated on the visualiser being visible. Forking the charger
  helper and walking cpufreq every few seconds for a screen nobody has open is runtime spent on
  nothing — on a device whose whole job is to play music for a long time on one charge.
