# Audit — what the DEVICE gives vs what SONY presents

*2026-08-18. Prompted by the FM result: Sony's `TunerPlayerService` could not measure a signal or
seek, and the chip could do both — the capability was there all along, one layer down. This audit
asks the same question of every other subsystem, on the live device rather than from the source.*

## The pattern, and why it keeps paying

Sony's userspace services are a **product surface**, not a hardware surface. They expose what the
stock UI needed and nothing else, and where the stock UI never shipped a feature the service is
often a stub that still returns 0. Underneath, the drivers Sony wrote are generous — they publish
raw register access, full ALSA control sets, and real device nodes.

Already exploited: the clock (no Sony setter exists at all → `settimeofday` + RTC ioctl), power off
(`PowerMgr` hangs on a shutdown barrier → `reboot(2)`), the GPU nodes, USB-MSC's LUN, and now FM.
Each time the shape was identical: **Sony's API is lossy or stubbed; the kernel is not.**

## Access cost — read this before planning anything below

Three tiers, and the tier changes the work far more than the feature does.

| tier | what | cost |
|---|---|---|
| **free** | already `0666` or `system:system`, and cinder-home runs as uid 100 = `system` | none |
| **helper** | `root`-only node; needs a `chmod` helper like `cinder-fm`/`cinder-gpunode` | ~80 lines, one component |
| **RE** | a protocol or binary interface with no documentation | a session |

## A. The CXD3778GF codec — 210 registers, live *(tier: helper)*

`/proc/regmon/cxd3778gf/` exposes the **entire Sony audio codec**, read and write, by name. This is
strictly bigger than the FM find. Selected registers, verbatim from the device's own table:

| register | why it matters |
|---|---|
| `0x42-0x47 HPRM_CTRL0/1, HPRM_MEAS, HPRM_DATA2/1/0` | **headphone impedance measurement** — the chip measures the load. Nothing in Cinder or Sony's UI shows it, and it is exactly what decides a safe gain mode |
| `0x25 CODEC_PLAYVOL`, `0x36 CODEC_CS_VOL` | the digital playback gain the volume steps actually move |
| `0x49/0x4B PHV_L / PHV_R`, `0x4C-0x4F PHV_CTRL` | the analogue headphone attenuator — the *second* of the two attenuators the BT-vs-3.5 mm split turns on |
| `0xD8 SMS_SFTRMP`, `0xC2 SMS_NS_PMUTE` | **S-Master soft ramp and mute** — the first place to look for the volume-step pop (`analysis/RE_volume_pop.md`: 26 pops below vol 100, cause never found, and NOT the shell or any mixer control) |
| `0x07 PLUG_DET`, `0xF0 BUT_TH`, `0xFC-0xFE INT0/1/2` | plug detection, **headphone-remote button threshold**, and the interrupt status bits behind both |
| `0x80-0xDB DNC*` (~90 registers) | the **digital noise-cancelling engine** — `cxd3778gf_dnc_core.ko` is loaded on this device right now |
| `0x62 LINEOUT_VOL`, `0x64-0x66 DAMP_VOL_CTRL`, `0x67-0x6F HPOUT2/3_CTRL` | every output stage's gain |

**Highest-value single item:** `HPRM_*`. An impedance reading turns "high gain mode" from a setting
the user guesses at into one the device can recommend.

## B. ALSA — Cinder uses 5 of 51 controls *(tier: FREE)*

`amixer -c0 controls` lists **51**. Cinder touches five: `numid=10` master volume, `26` analog input
device, `28/29/30` S-Master gain modes. Read live, with their current values:

| numid | control | value | what it gives us |
|---|---|---|---|
| **1, 2** | `noise cancel mode` / `status` | 0 / 0 | **noise cancelling — a whole Sony feature Cinder does not have at all** (zero mentions in the source) |
| **3, 7** | `user noise cancel gain` / `user ambient gain` | 15 / 15 | NC strength and ambient-sound level |
| **31** | `noise cancel headphone type` | 3 | which NC headphone is fitted |
| **33, 34** | `jack status se` / `jack status btl` | **1** / 0 | **real jack detection** — se reads 1 with headphones in. Pause-on-unplug, and a truthful FM aerial check |
| 8 | `nc ignore jack state` | off | |
| 13 | `master gain` | 30 | a *second* gain stage, separate from `numid=10` |
| 11, 12 | `l/r balance volume` | 0 / 0 | the hardware balance pair |
| 18-21 | `timed mute`, `std/icx/dsd timed mute` | 0 | mute ramps — the pop again |
| 27, 32, 37 | `headphone amp`, `jack mode`, `headphone detect mode` | 1, 0, 2 | output routing |
| 35, 36 | `standby`, `deep early suspend` | off | power states |
| 40 | `playback latency` | 0 | |
| 9, 25 | `sound effect`, `output device` | on, 1 | |

This tier costs **nothing** — `amixer` is already how Cinder drives volume.

## C. Bluetooth — the stack's own sockets are already ours *(tier: free surface, RE protocol)*

This is where the suspicion was right, and the access is better than expected. Everything below is
**`system:system`**, and cinder-home runs as uid 100 = `system`, so **no helper is needed at all**:

```
/dev/stpbt      crw-rw---- system system   192,0    the raw MTK BT HCI transport
/dev/stpwmtA    crw-rw---- system system   200,0    the combo-chip control channel
/tmp/bt.app.gap srwx------ system system            GAP — discovery, connection, link state
/tmp/bt.int.adp / bt.ext.adp                        adapter protocol, internal + external
/tmp/bt.a2dp.stream                                 the A2DP PCM pipe already known
```

Sony's `BtCommonService` presents a **5-state enum** and little else — no link RSSI, no link
quality, no negotiated-codec truth, no peer battery (`reference_bt_no_peer_battery`), and the MTK
stack logs nothing so failures are judged by side effects (`reference_bt_radio_wedge`). HCI has all
of it: `Read RSSI`, `Read Link Quality`, `Read Transmit Power Level`, `Read Remote Version`, and
`Read Remote Extended Features` — the last of which would settle what a headphone *actually*
supports instead of trusting Sony's enum.

Sony also ships the tools: `/system/bin/hci_cmd` (`hci_cmd XX XX XX`, or `-f FILE`), `bt_drv`, and
`btut` (633 KB — MTK's BT test harness). `hci_cmd` talks over `/tmp/hcicmd_socket`, which does not
exist while Sony's stack owns the transport, so it is not a drop-in — but it is a working reference
for the framing.

**Caveat, stated plainly:** the MTK adapter protocol on those sockets is undocumented and Sony's
services hold the transport. This is the one item in this audit that is a genuine RE session rather
than an afternoon. It is also the one with the biggest payoff, because *every* BT limitation
recorded in this project so far is a limitation of Sony's presentation layer, not of the radio.

## D. Charger and PMIC registers *(tier: helper)*

`/proc/regmon/bq24262/` — `STATUS`, `CONTROL`, `BATTERY_VOLTAGE`, `BATTERY_CURRENT`, `VIN_MINSYS`,
`SAFETY`. Read live: `0x02 = 0x78` decodes to a 4.10 V charge target, matching
`power_supply/battery/voltage_now = 4085101`. `/proc/regmon/mt6323/` exposes the PMIC's whole
`CHR_CON0..N` block.

Cinder currently reads `capacity` and `status` from `/sys/class/power_supply/battery` — which is all
sysfs offers here: **no `current_now`, no `temp`, no cycle count**. The charger IC has the current
and the termination voltage. the power measurements in `docs/` had to infer draw from
cumulative counters; this is the direct reading.

## E. CPU floor — free, and it is half of Walkman One's paid feature *(tier: FREE)*

```
/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq   -rw-rw-rw-   <-- already world-writable
scaling_cur_freq = 1300000    governor = hotplug
available = 1300000 1196000 1040000 747500 598000
```

`analysis/RE_walkmanone_extract.md` found that Walkman One's "sound signature" is a three-byte patch
choosing an ALSA device and **a CPU clock floor**. The floor half needs no patch and no helper —
the node is already `0666`. Cinder ships `cinder-signature.sh` for the ALSA half; the clock half is
a file write.

## F. Thermal *(tier: FREE)*

```
thermal_zone0 mtktscpu  = 33.5 °C     thermal_zone1 mtktspmic = 39.4 °C
thermal_zone2 mtktsabb  = 33.5 °C
```

World-readable. Nothing in Cinder reads them. Relevant to the battery-care feature and to charging.

## G. Headset detection *(tier: FREE)*

`/sys/class/switch/` — `cxd3778gf_h2w` = **2** (headset type, not just present/absent),
`cxd3778gf_antenna` = 1 (already used, for the FM aerial), `cxd3778gf_ucom` = 0, plus `otg_state`,
`usb0_suspend_state`, `usb_audio`. With `numid=33` this gives two independent ways to know a cable
is in, and one of them tells you *what kind*.

## H. Codec module parameters *(tier: mixed — one is free)*

```
/sys/module/snd_soc_cxd3778gf/parameters/
  timed_mute_ms   -rw-rw-rw-   <-- FREE, world-writable
  fade_amount     -rw-r--r--   = 1
  monvol_wait_ms  -rw-r--r--   = 150
```

A driver-level **fade amount** and **timed mute** are precisely the shape of knob that the unsolved
volume-step pop would respond to, and `timed_mute_ms` can be experimented with today at zero cost.

## Ranked — what I would actually do

1. **Noise cancelling (B).** Free tier, a complete Sony feature Cinder does not have, six ALSA
   controls, and the kernel module is already loaded. Best ratio in the audit by a wide margin.
2. **Jack detect → pause on unplug (B/G).** Free, small, and the kind of thing whose absence is felt
   every time it happens.
3. **The volume pop (A/H).** A months-old unsolved bug with two new suspects — `SMS_SFTRMP` in the
   codec and `timed_mute_ms` in the driver — one of which is free to test.
4. **CPU floor (E).** Free. Half of a feature people pay for.
5. **Headphone impedance (A).** Needs a helper, but turns a guessed setting into a measured one.
6. **Battery detail: current, charge target, temperature (D/F).** Half free, half helper.
7. **Bluetooth over the stack's own sockets (C).** Biggest payoff, biggest cost. Do it deliberately,
   not opportunistically — and start by watching `/tmp/bt.app.gap` during a known connect.

## What is NOT there — so nobody re-investigates

* **RDS.** The register exists; the part does not have the decoder. Si4708 is the non-RDS member of
  the family (Si4709 is the one with it). Confirmed by measurement, not inference.
* **Peer battery over Bluetooth.** Already settled (`reference_bt_no_peer_battery`) and nothing in
  this audit changes it — the radio can report *our* battery outward, not read theirs.
* **A Sony clock setter.** A sweep of every library's demangled prototypes found none; the kernel
  route Cinder already uses is the only one.
* **`/dev/i2c-2` for anything.** MediaTek does not implement the standard chardev file ops
  (`CONFIG_I2C_CHARDEV is not set`), and the bus carries the battery charger and NFC besides. Use
  `regmon`, which is what it is for.
