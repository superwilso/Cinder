> # ⚠️ SUPERSEDED — DO NOT START A KERNEL MODULE
>
> This document concluded that kernel code was the only remaining route to real RSSI and hardware
> seek. **That is wrong.** The device exposes a register monitor:
>
> ```
> /proc/regmon/Si4708icx/target    rw-rw-rw-
> /proc/regmon/Si4708icx/value     rw-rw-rw-
> ```
>
> (siblings: `afe_reg`, `bq24262`, `cxd3778gf`, `mt6323` — the same interface for the codec, the
> charger and the PMIC.)
>
> That is **full Si4708 register read/write from userspace**, world-writable, no module, no kernel
> source, and none of the brick risk this plan spends its last section on. Write the register index
> to `target`, read or write `value`.
>
> Everything below about WHICH registers matter (`STATUSRSSI` for real RSSI and `STC`, `POWERCFG`
> for `SEEK`/`SEEKUP`, `SYSCONFIG2` for the threshold) is still correct and still the point — only
> the delivery mechanism was wrong. Read the register table, ignore the build/risk sections.
>
> The direct-I2C failure that led here was real (`/dev/i2c-2` refuses userspace transfers), but it
> was not the only userspace path and I did not look for another before concluding.

# Getting real RSSI and hardware seek out of the Si4708 — the kernel plan

Written 2026-08-18, at the end of a session that took the userspace routes as far as they go. This
is the brief for someone starting fresh on the kernel side.

> ## ⚠️ SUPERSEDED IN ITS PREMISE — 2026-08-18, later the same day
>
> This document's thesis is **"the registers cannot be reached from userspace at all, therefore
> kernel code."** That is **false.** Sony's driver calls `regmon_add`, and every Si470x register —
> RDS included — is exposed read/write at **`/proc/regmon/Si4708icx/`** as a `target` + `value`
> pair:
>
> ```
> 0x00 DEVICEID  0x02 POWERCFG   0x05 SYSCONFIG2  0x0A STATUS_RSSI
> 0x01 CHIPID    0x03 CHANNEL    0x07 TEST1       0x0B READCHAN
>                                                 0x0C-0x0F RDSA-RDSD
> ```
>
> Read live off the device:
>
> ```
> 0x0A -> 0x000B    RSSI = 11        <-- GRADED, not the binary 0/65535 V4L2 reports
> 0x0B -> 0x01AA    READCHAN = 426
> 0x02 -> 0x4001    ENABLE=1, DMUTE=1 (chip powered at the time of the read)
> 0x05 -> 0x126F    SEEKTH=0x12, BAND=1 (76-108), SPACE=2 (50 kHz), VOL=15
> ```
>
> `76 MHz + 426 x 50 kHz = 97.3 MHz` — the exact station `RE_fm_tuner.md` confirmed by ear. These
> are live chip reads, not cached constants.
>
> So everything the plan below wanted a module for — real RSSI, `ST`, `STC`, `SEEK`/`SEEKUP`/
> `SEEKTH`, `READCHAN` — sits behind a `/proc` file. **Do the userspace route first.** Keep the
> plan below only for the case where the regmon *write* path turns out to be inert.
>
> `/proc/regmon` is `root`-only (`-rw------- root root`), and cinder-home is uid 100 — so this
> needs a small setuid helper, same shape as `cinder-clock` / `cinder-gpunode`.

## Why kernel code is the only route left

*(As written. Rung 3 of this table — "direct I2C via `/dev/i2c-2`" — is still true and still the
reason people reach for a module; the regmon node above is the door that was missed.)*

The chip is capable. Three software layers sit on top of it and each one drops a feature. All of
this is measured, not inferred — see `analysis/RE_fm_tuner.md` for the raw output.

| layer | tune | signal | seek |
|---|---|---|---|
| `TunerPlayerService` (Sony) | works | **constant `1`** at every frequency | **stub**: `StartAutoTuning` is 48 bytes, returns `4`, never reads its arguments |
| kernel `Si4708icx` via `/dev/radio0` | works | **binary** — only ever `0` or `65535` | `V4L2_CAP_HW_FREQ_SEEK` clear, ioctl returns `ENOTTY` |
| direct I2C via `/dev/i2c-2` | — | — | **every transfer fails `EINVAL`** |

Direct I2C deserves detail, because it is the obvious idea and it does not work:

```
open("/dev/i2c-2")            OK
ioctl(I2C_SLAVE, 0x10)        EBUSY        (the Si4708icx driver is bound)
ioctl(I2C_SLAVE_FORCE, 0x10)  OK           (address taken anyway)
read(fd, buf, 32)             EINVAL       (MTK does not implement the simple file ops)
ioctl(I2C_RDWR, 32 / 16 / 8)  EINVAL       (all three)
```

MediaTek's adapter refuses userspace transfers to this device.

**Why** — settled 2026-08-18 from `/proc/config.gz`: **`CONFIG_I2C_CHARDEV is not set`.** The
`/dev/i2c-*` nodes are MediaTek's own chardev, not the standard `i2c-dev` one, so the standard file
ops genuinely are not implemented. The diagnosis above was right; this is the reason.

So the registers cannot be reached over `/dev/i2c-2`. They CAN be reached over `/proc/regmon` —
see the banner at the top of this document.

## What the chip actually has

Si470x-family register map (public, and the Si4708 is the tuner-only member of it):

| reg | field | why we want it |
|---|---|---|
| `0x0A STATUSRSSI` | `RSSI[7:0]` | **real graded signal strength**, 0..75 dBµV — not the binary flag V4L2 exposes |
| | `ST[8]` | genuine stereo lock (the V4L2 `rxsubchans` bit reports stereo even on a dead frequency, so it is useless) |
| | `STC[14]` | seek/tune complete — the interrupt-free way to know a retune has settled |
| | `SF/BL[13]` | seek failed / band limit reached |
| `0x02 POWERCFG` | `SEEK[8]`, `SEEKUP[9]`, `SKMODE[10]` | **hardware seek** — the chip walks the band itself |
| `0x05 SYSCONFIG2` | `SEEKTH[15:8]` | seek threshold, i.e. how strong a station has to be |
| `0x0B READCHAN` | `channel[9:0]` | where a seek landed |

Read protocol: the Si470x returns registers starting at `0x0A` and wrapping, so a 32-byte read
gives `0x0A..0x0F` then `0x00..0x09`, big-endian 16-bit, with no register-address byte. Writes
start at `0x02`.

**`STC` is the prize as much as seek is.** Every scan we can currently build pays a fixed settle
time per step because there is no way to know when a retune has finished — `S_FREQUENCY` alone
costs ~90 ms and we add more on top. With `STC` a step becomes "tune, poll a bit, done", which is
where the real speed is, not just in `SEEK`.

## What to build

Ordered cheapest-first. Stop as soon as one works.

### 1. Extend the existing driver rather than replacing it

`Si4708icx` already owns the chip and already does I2C correctly. The smallest useful change is to
implement the two V4L2 ops it is missing:

* `g_tuner` → populate `signal` from `STATUSRSSI[7:0]` scaled to 0..65535 instead of the current
  binary value, and `rxsubchans` from `ST[8]`.
* `s_hw_freq_seek` → drive `SEEK`/`SEEKUP` in `POWERCFG`, poll `STC`, report via `READCHAN`, and
  set `V4L2_CAP_HW_FREQ_SEEK` in the capability word.

Then **nothing in Cinder changes except deleting code**: `cinder_tuner_scan` and
`cinder_tuner_seek` already have the sweep structure, and would drop the ALSA capture entirely.

This needs the kernel source for 3.10.26-mt8590 and the Sony driver source, which we do NOT have —
only the built image. So realistically this means:

### 2. A standalone module that binds nothing

A module that does raw I2C through the in-kernel `i2c_transfer()` API against adapter 2, address
0x10, and exposes the registers through a debugfs or sysfs file. It does not need to own the
device — `i2c_transfer` on an adapter does not require binding — so it can coexist with
`Si4708icx` rather than replacing it. Userspace then reads a file instead of an ioctl.

This is the smallest thing that could possibly work, and it sidesteps the whole "we don't have the
driver source" problem.

### 3. Full replacement driver

Only if 2 proves the approach and the coexistence turns out to be a problem. The mainline
`radio-si470x-i2c` driver is a starting point, but it targets Si470x parts with a different power
sequence, and this board's `Si4708icx` clearly has Sony-specific bring-up (the chip is powered by
`TunerPlayerService::Open()`, not by opening `/dev/radio0` — measured).

## What it costs, honestly

*(Audited against the live device 2026-08-18. Several of these turned out cheaper than written, and
one new hazard appeared.)*

* **Kernel config — IN HAND.** `/proc/config.gz` is present on the device (20,364 bytes). The
  *source tree* for 3.10.26-mt8590 is still missing, so option 1 is still blocked; option 2 is not.
* **Module loading is unusually permissive.** From that config:
  `CONFIG_MODULE_SIG` **n**, `CONFIG_MODVERSIONS` **n**, `CONFIG_MODULE_FORCE_LOAD` **n**.
  No signature and no symbol CRCs — the only gate is an exact vermagic match:

  ```
  vermagic: 3.10.26 SMP preempt mod_unload ARMv7      <-- note the trailing space
  ```

  (read from the stock `radio-si4708icx.ko`). Built with gcc 4.8.
* **Option 2's core assumption is validated by Sony's own driver.** `radio-si4708icx.ko` imports
  plain **`i2c_transfer`** (`readelf -sW`, UND symbols) and does its 32-byte reads through it. So
  `i2c_transfer` against adapter 2 demonstrably works for this chip at this address.
* **`CONFIG_I2C_SI4708=m`**, and the loaded module's refcount is **0** with `CONFIG_MODULE_UNLOAD=y`
  — `rmmod`/`insmod` of the stock driver needs no reboot, and frees 0x10 for a replacement.
* **Toolchain.** The armv7 cross-compiler is already set up for userspace; a kernel module needs
  the matching kernel build tree, not just a compiler.
* **Loading.** `insmod` needs root. `cinder-home` runs as `system`, so the module has to be loaded
  from the launcher/init path, not by the app.
* **⚠️ NEW HAZARD — i2c-2 is not the tuner's private bus.** Live enumeration:

  | addr | device |
  |---|---|
  | `2-0010` | `Si4708icx` — the tuner |
  | `2-0028` | `cxd224x-i2c` — NFC |
  | `2-0050`..`2-0057` | `mt_m24c16` EEPROM + dummies |
  | **`2-006b`** | **`bq24262_wmport` — the battery charger** |

  Wedge bus 2 and you lose charging, not just FM. Any raw-transfer module on this adapter has to be
  written with that in mind.
* **Risk, and this is the real cost.** Everything else in this project has been userspace: the
  worst case has been "the app dies and the launcher reverts". A module is different. It runs in
  kernel context, on a bus another driver owns, on a device with **no public DFU/EDL recovery**.
  A bad module loaded at boot is precisely the failure the escape ladder exists for, and the ladder
  has already been needed twice this month.

  **Audited 2026-08-18 — the risk is tiered, and only the top tier is dangerous.** Full working in
  `RECOVERY.md` §"What survives kernel work"; the summary:

  | what you do | worst case | needs wbrt? |
  |---|---|---|
  | `insmod` by hand over adb | panic -> 5 s -> reboot -> module gone, self-healed | no |
  | module loaded from the launcher at boot | bad-boot counter -> stock (and the module load goes with it) | no |
  | **replace the kernel in `bootimg`** | no boot; ladder rungs 0-4 all gone | **yes** |

  `/proc/sys/kernel/panic` is **5** and `panic_on_oops` is **1**, so a panic always reboots rather
  than hanging — which is what keeps the first two rows self-healing. wbrt *does* cover `bootimg`
  (it is inside the 2.68 GB restore range); it does **not** cover the preloader, which lives in
  eMMC boot0 and which none of this work touches.

  **Do not load it from the boot path until it has been insmod'd by hand over adb, repeatedly,
  across reboots, with the device otherwise idle.**

## What we get, and whether it is worth it

Current best without kernel work is a **hybrid**: sweep with the V4L2 binary meter (~25 s for the
band, no capture PCM, does not stop the radio), then confirm the flickering candidates by audio.
That is genuinely usable.

With kernel work: a full-band scan in a second or two, a real signal meter to draw, an honest
stereo indicator, and seek that behaves like Sony's.

So the gain is real but it is a **quality-of-life** gain on a feature that already works. Weigh it
against a class of risk nothing else in this project carries.

## Before starting, re-read

* `analysis/RE_fm_tuner.md` — the whole tuner story, including the three separate wrong conclusions
  this session produced and what caused each.
* **Always check `/sys/class/switch/cxd3778gf_antenna/state` before believing any measurement.**
  Three "the meter is broken" findings in one session were all an empty headphone jack. The cable
  is the aerial.
* `reference_uac_capture_start` — the capture PCM does not start on its own.
* `reference_bt_transmitter_socket` — never write PCM to that socket before the type-1 handshake.
