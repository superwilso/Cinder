# Cinder — battery, performance and sound audit on Walkman One, 2026-09-28

*Against `9128329` plus this change. Every device number below was read off the owner's NW-A55
running **Walkman One 3.02** over adb on this date — the first power audit on W1; the earlier ones
(`AUDIT_BATTERY_PERF_2026-09-05.md`, `analysis/RE_early_suspend.md`) ran on stock 1.02. Method:
cumulative counters (`/proc/stat`, per-thread `stat` + `sched`, `/proc/interrupts`, `time_in_state`,
`/sys/power/{idle,dpidle}_state`) read at both ends of a window inside one `adb shell`, because adb
itself wakes the core and an instantaneous read measures the reader. There is no fuel gauge on this
device, so no claim here is in milliamps.*

## Summary

| # | Finding | State |
|---|---|---|
| **P1** | With the screen off, the display pipeline **kept scanning out at 60 Hz** — 260 IRQs/s and 135 kernel-thread wakeups/s — because a dark panel is only a backlight at 0. Stage-1 early suspend (which powers the LCM, the display engine and the DRAM clock down) existed but shipped **off**. | **Fixed: on by default.** Device-verified. |
| **P2** | A cable pulled **during** stage 1 is never processed by the USB port (its VBUS thread is parked by the same chain), so the USB0 clock stays on and **deep idle never happens**: `dpidle_cnt` 0 for 150 s off-cable. | **Fixed.** Device-verified: 0 → 8,224 at ~36/s. |
| **P3** | After a USB-MSC session that added nothing, the rescan campaign ran all 12 rounds: **13 full library rebuilds and ~60 MB of snapshot writes in two minutes**, 3,355 tracks every time. | **Fixed.** Device-verified: 1 scan, 0 rebuilds, done in 30 s. |
| **P4** | After USB-MSC (and USB-DAC, FM), the hardware Play button did nothing until a song was tapped. | **Fixed.** Device-verified (second attempt — the first cut was caught by the owner's test). |
| **P5** | Power-to-light: the stage-1 exit waited up to a 1 Hz tick, then sat behind a 270 ms touch-controller IPC. | **Fixed:** exit is the first thing the wake path does. |
| P6 | Walkman One's scheduler tunables (`sched_latency/min_granularity/wakeup_granularity` = 0.1 ms) cost ~3% of the audio pipeline's CPU and ~8% of context switches. | **Fixed (follow-up):** `cinder-power sched` resets them at startup. |
| P7 | The governor spends 17% of screen-off playback at 1300 MHz (1.30 V) for a ~13% load. | **Fixed (follow-up):** `cinder-power cap` from the first dark second; stage 1 pins the same step after 60 s. |
| S1 | Playback is bit-perfect at native rate for CD audio, on the codec's **low-power** PCM. | Confirmed, nothing to do. |
| S2 | The Walkman One **WM1Z tuning is not applied** on this player (W1's own boot log). | Cause found (Part C). The 09-28 flash did nothing. With `PMD=1`, `GMD=0` and `DIM=0` the tuned path loads the same audio files anyway ([RE](../analysis/RE_walkmanone_installers.md)). |

Nothing below changes sound. Every lever that could (the S-Master gain mode, W1's CPU floor, the
signature HAL) is either unmeasurable here or already measured flat by others — see Part C.

---

## Part A — Where the battery goes on W1

### A1. Idle, screen off, nothing playing (awake, the old default)

90 s, cable in: 1.2% of a core, **364 ctxt/s, 353 IRQ/s**, 98% of the time at 598 MHz. The largest
wake sources are not Cinder:

| source | rate |
|---|---|
| `mtk_disp` IRQs 184 + 188 | 195 + 65 /s |
| `rdma0_update_kt`, `disp_config_upd` | 69 + 65 wakeups/s |
| `WMPortService` (two threads, 10 Hz polls) | 20 /s |
| cinder-home | 5.3 /s, 0.29% of a core |

The display rows are the pipeline scanning out a frame nobody can see.

### A2. Playing CD FLAC down the jack, screen off, awake

90 s: **12.6% of a core, 549 ctxt/s**, and **16.7% of the time at 1300 MHz**. SoundServiceFw 9.9%
(the DSP chain: this player runs ClearAudio+ and the normalizer), the decoder process 3.3%, the
display pipeline the same 135 wakeups/s as idle. The two audio threads take 23 and 46 involuntary
preemptions a second (see P6).

### A3. The same, in stage 1

60 s, same track type: **IRQs 383 → 90/s, ctxt 549 → 318/s**, both display threads gone, PCM
`RUNNING` throughout, `hw_ptr` advancing 44,118 frames/s against 44,100, no xrun in logcat. The CPU is
held at 1040 MHz by `mt_cpufreq_early_suspend`, and that is not a cost:

```
/proc/cpufreq/cpufreq_ptpod_freq_volt
  1300000 kHz  1300 mV
  1196000      1200
  1040000      1150
   747500      1150
   598000      1150
```

598, 747.5 and 1040 MHz share one voltage, so a fixed 1040 costs no more per cycle than 598, and
the governor's bursts to 1300 (≈28% more per cycle, V²) stop. The audio pipeline measured 8.8% of
a core in stage 1 against 13.2% awake (different track, same format).

Deep idle still does not happen while a stream is open — the audio path holds a clock (`by_clk`),
as the 2026-09-06 stock run found. What stage 1 buys during playback is the display, the DRAM
clock and the voltage step.

### A4. Off-cable, idle, stage 1 — P2

First run (build without the P2 fix), stage 1 entered on the cable and then unplugged:

* `by_vtg` frozen at 80,929 — the gate opened as designed;
* IRQs ~430 → 52/s, ctxt ~460 → 145/s;
* **`dpidle_cnt` 0 for the whole 150 s**, `by_clk` climbing, `dpidle_block_mask[CG_PERI0] =
  0x00000400` = USB0.

`dmesg` has no `Disconnect USB` / `PHY off` for the unplug at all — only the `Connect USB` on the
replug. Handler 9 of the chain is `bq24262_wmport_early_suspend`: the WM-PORT VBUS thread is parked,
so the port never learns the cable left and keeps its clock. The charger's sysfs node (`usb/online`)
does see it. On 2026-09-04 (stock) the cable was out *before* stage 1 began, which is why that run
reached deep idle and this one did not.

Fix: on a 1 → 0 edge of `usb/online` during stage 1, write `on` (late resume runs the port's
handler), wait ~15 s, re-enter. Late resume leaves the backlight at 0 (checked by hand: `on`, read
the node, `mem`), so the panel stays dark. Re-run with the fix:

```
 uptime usb  state  dpidle_cnt  per s   peri0
    509  1   mem          0      0.0    0x00000400
    515  0   off         38      7.0    0x00000000   <- stepped out, port released
    531  0   mem         84      8.6    0x00000000   <- back in
    536  0   mem        262     33.1
    ...
    756  0   mem       8176     35.6
    762  1   mem       8224      8.9    0x00000400   <- replugged
```

## Part B — The changes

All in `cinder-home/src/main.cpp` unless named.

1. **Stage 1 on by default** (`socsusp::threshold_s`): absent `/contents/cinder_suspend_s` = 60 s;
   `0` turns it off; `/contents/cinder_no_suspend` still stops everything, checked every tick.
   Undecided until `/contents` is really mounted (reads `/proc/mounts`), so an explicit `0` can
   never be missed in the boot-time MSC window. Stage 2 (suspend to RAM) stays opt-in.
2. **Stage 1 during jack playback, by default** — opt out with `/contents/cinder_no_suspend_playing`
   (re-read every tick). Never on Bluetooth: `wmt_dev_early_suspend` is handler 0 and no A2DP run
   has been done.
3. **Back-off on a refused `/sys/power/state` write** (1 min × 3 → 1 h), found by the harness's
   `autooff-idle` scenario the moment the default flipped: it logged once a second.
4. **Leave stage 1 immediately on wake** (`soc_suspend_leave_now`), before the touch IPC.
5. **Cable pulled during stage 1** → step out ~15 s (P2).
6. **Library content signature** (`cinder_db::content_signature`, FFI
   `cinder_db_content_signature`): every row of `object_body` and the lookup tables, plus an
   aggregate of `object_ext_int`. The housekeeping watcher rebuilds only when it moves, and the
   rescan campaign now distinguishes *content moved* (scan again), *file moved, content not* (a scan
   is still writing — wait) and *neither* (settled). Evidence that it holds still: this boot's copy
   of `MTPDB.dat` and the file after 13 scans differ in **21 bytes, all in the SQLite header and
   `sqlite_master`** — Sony's scanner rebuilds its indexes on every scan and changes no row. Cost:
   ~13 ms on the host for 6,536 rows; it runs only when the file's stat moves.
7. **`cinder_resume_rearm`** (cinder-ffi): after the player is closed and re-opened (MSC, DAC, FM
   exits) the live Up Next and position are armed as a pending resume, snapshotted at once —
   `cinder_db_open` drops `last_track`, so a rebuild deferred to the press found nothing (the
   first cut's bug).

**Gates:** 660 Rust tests (+2), all harness scenarios (+6: `library-touched`, `suspend-default`,
`suspend-off-file`, `suspend-unplug`, `suspend-unplug-back`, and an assertion in `msc-cycle`),
C/C++ syntax clean. Each new scenario was checked to FAIL with its fix removed.

## Part C — Sound

* **Bit-perfect for CD audio.** A 16/44.1 FLAC plays as `S16_LE`, 44,100 Hz, on
  `pcm4p` = `cxd3778gf-icx-lowpower` (period 5,513 frames, buffer 44,104 = 1 s). W1's "normal" HAL
  is byte-identical to stock's and already picks the low-power path; `pv1`/`pv2` only change which
  PCM opens and the CPU floor, and Wampy's REW measurements found the same signal from all of them.
* **Nothing in P1–P7 touches the audio path.** Stage 1 leaves an open stream alone (measured above);
  the 1 s ALSA buffer makes scheduler granularity irrelevant to what reaches the DAC.
* **The WM1Z tuning is not in effect on this player.** `/contents/CFW/boot_log.txt`, boot #45:
  *"The [WM1Z] tuning was not applied to the device … Normal (no tuning) mode initialized."*
  **Why:** W1's gate is `md5(/dev/block/mmcblk0p3) == TMD5` (`ccb29dd2…` for WM1Z), a constant
  for every player. The live NVRAM is byte-identical to W1's own stock backup
  (`/opt2/stock/nv_bk`, both `bc41b677…`): the boot log shows W1 was reinstalled (boot counter back
  to #1), the install writes `nv_bk` back, and the tuning package was never re-applied. It was
  applied once, and boots #2–6 of the first install report it.
  **What the gate unlocks** (`/sbin/boot_complete.sh`): the Plus-mode HAL (`pv2` here: Hold up,
  `PMD=0`), the analyser parameters, `GMD`/`DIM`/`COL`. Untuned, W1 loads `normal_nt`, whose HAL is
  byte-identical to `normal` and to stock's. `pv2` opens `hw:0,4` and pins **1300 MHz** during
  playback, the opposite of P7. Wampy's REW run found the same signal at the jack from every
  signature and Plus mode.
  **How to apply:** W1's supported route is
  `/contents/CFW/External_Tunings/WM1Z_external_tuning/FirmwareUpdateTool.exe` on Windows. Its
  package writes NVRAM and, per Wampy's `w1.cpp`, the bootloader (`mmcblk0p7`). Cinder does not do
  this itself.
* **Open lead, unmeasured:** with ClearAudio+ and the normalizer on, the stream still leaves the HAL
  as `S16_LE`, i.e. the DSP output is requantised to 16 bits. Whether Sony dithers, and whether the
  HAL can be made to open 24/32-bit for processed audio, has not been looked at.

## Part D — Open, in order

1. **Stage 1 under Bluetooth playback.** Needs headphones and a listen: `wmt_dev_early_suspend`
   (the combo chip) runs first in the chain. If clean, drop the "never on Bluetooth" rule — it is the
   other half of screen-off listening.
2. ~~A root boot hook for P6 and P7~~ — done without one, see Part E.
3. **`/data` is at 4 MB free** on the owner's player: four rollback binaries (`cinder-home.prev`,
   `.r1`, `.r2`, `.r3`) are ~18 MB of its 35 MB.
4. **Wampy's `scrobbler` still runs** as root beside Cinder (0.07% of a core, ~5 wakeups/s, 8.5 MB).
   Cinder already stands down so plays are not logged twice; its uninstaller removes the process.
5. The first seek after one boot took 1.76 s in `pause` (`seek 461000 ms: pause=1762ms`); later
   boots were 49–143 ms. Seen once; not chased.

## Part E — Follow-up the same evening: P6 and P7 shipped

No new root hook was needed: `cinder-power` was already setuid root, so it gained three verbs whose
values are all hard-coded (`src/cinder-power.c`):

| verb | writes |
|---|---|
| `sched` | `sched_latency_ns` 6000000, `sched_min_granularity_ns` 750000, `sched_wakeup_granularity_ns` 1000000 (kernel defaults for one online CPU; the values of the P6 A/B) |
| `cap` | `scaling_max_freq` 1040000 |
| `uncap` | `scaling_max_freq` = `cpuinfo_max_freq`, read back |

cinder-home runs `sched` once per boot and `cap`/`uncap` from the 1 Hz tick: capped while the screen
is dark on the jack (the same scope as stage 1), uncapped within a tick of it lighting. Not on
Bluetooth, since LDAC encode load at 1040 has not been measured. `/contents/cinder_no_cpu_tune`
stops `sched` and `cap` (never `uncap`, so the file cannot strand the clock). Three helper failures
end the attempts for the session.

**On the device, as root:** all three verbs rc 0, and the nodes read back as written. Under a busy
loop while capped, `scaling_cur_freq` stayed at 1040000 and the 1300 MHz row of `time_in_state`
did not move (5705 before and after). Unknown verbs and no argument give rc 2.
**Harness:** `cpu-cap` (sched once; one cap after the idle blank; uncap after Power; capped
again on the next blank) and `cpu-tune-off`; 59 scenarios green.
**Device, build 4 (2026-09-28 reboot):** `cpu: scheduler slices…` at 11.2 s, the three nodes read
6000000 / 750000 / 1000000; `cpu: screen dark on the jack -> max 1040 MHz` at 19.3 s, and
`scaling_max_freq` reads 1040000. Still to see: the uncap line on a screen wake.
