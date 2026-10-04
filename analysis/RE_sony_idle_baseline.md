# What Sony's firmware does when idle, and what Cinder should aim for

**Date:** 2026-10-04. Offline RE (stock rootfs + kernel image) plus the same day's off-cable probe
(`tools/idle_probe.sh`). Nothing here was written to the device.

## The one-line finding

**Sony's idle baseline is suspend-to-RAM; Cinder's is deep idle.** Stock writes `mem` to
`/sys/power/state` and lets the kernel go all the way down whenever no wakelock is held. Cinder
writes the same `mem` but holds its own wakelock, so it stops at early suspend (stage 1) and the
SoC only ever reaches deep idle, waking about 30 times a second for other processes' timers.

## Sony's path, from the binaries

`vendor/sony/lib/libPowerService.so` (strings; the functions are not exported):

| What | Evidence |
|---|---|
| Suspend is a plain state write | `mem > /sys/power/state`, `on > /sys/power/state`, `PowerService::SystemSuspend` / `SystemResume` |
| Wakelocks are the interlock | `%s > /sys/power/wake_lock`, `powerservice_wake_lock`, `service_wake_lock`, `AcquireServiceWakeLock` |
| Every service is told first | `ServiceManager::NotifyEarlySuspend`, `NotifySuspend`, `NotifyLateResume`, `NotifyResume` — the `OnEarlySuspend(bool&)` / `OnSuspend(bool&)` each service implements |
| It reads why it woke | `icx_pm_helper/spm_r12`, `eint_sta`, `boot_powerkey`, `resume_lock_cancel`; `GetResumeFactor`; `kColdSleep` / `kWarmSleep` |
| The codec is switched by name | `AmixerUtils::SetInt`, `Turn off deep early suspend`, `Failed to turn on deep early suspend!` |

`HgrmMediaPlayerApp` (strings): `reqSuspend()` / `resSuspend(bool)` — each audio source is asked and
answers before the app suspends (`AudioSource [%s] sends resSuspend(%d)`), and it takes
`Acquire USB WakeLock` / `Acquire AC IN WakeLock`. So on stock:

* **paused, off the cable** -> every source agrees -> `mem` with no lock -> **RAM suspend**;
* **playing** -> the audio path holds a lock -> early suspend only (what Cinder's stage 1 is);
* **on a cable or charger** -> Sony holds a lock on purpose -> never RAM-suspends.

### "deep early suspend" is a codec switch

Kernel, `cxd3778gf_early_suspend` @0xc063568c: it reads `deep_early_suspend` (0xc0f863a0, the ALSA
control numid 36). Zero: the handler records the state and returns — the codec stays powered. Non-zero:
it flushes the driver's work and, if no stream is active (three flags in `present`), calls
`suspend_core` — the same full power-down `cxd3778gf_suspend` does. So Sony powers the codec down at
*early* suspend by setting that control first. Cinder gets the same result by writing `standby`
30 s after the screen goes off (`RE_codec_power.md`); the kernel's version has the stream check
built in and needs no timer.

## Measured the same day (Walkman One, Cinder, stage 1, off the cable)

| | on the cable | off the cable |
|---|---|---|
| Deep idle entries | 0 (USB0 clock blocks it) | about 30 a second |
| Interrupts / s | about 300 | 84 (window was 127 s out + 77 s in) |
| `AD_WHPLL_CK` | 480 MHz | **0** — it is the USB PHY clock, not the Bluetooth chip |
| PLLs on | ARM, MAIN, UNIV, (AUD when playing) | ARM, MAIN, UNIV |
| Still waking the CPU | | Sony `WMPortService` 20/s, `khubd_poll` 9/s, scrobbler 3/s |

Those wakers are the difference between deep idle and RAM suspend: suspended, user space is frozen
and nothing runs until a wake source fires.

## What to aim for, and with what

1. **Paused and off the cable: RAM suspend (Cinder's stage 2), as Sony does.** It exists behind
   `/contents/cinder_ram_suspend` and resumed correctly on 2026-09-04 (`resume_count` 0 -> 5). What
   it needs before it can be a default is Sony's own interlocks, all of which are now known: a lock
   while USB or the charger is in, a lock while any source is audible (jack, Bluetooth, receiver,
   soundscape, FM), a lock while a Bluetooth connect is pending, and reading `spm_r12` on resume.
   Wake sources are the power key (PMIC, EINT 150) and the RTC (`RE_kernel_power.md`).
2. **Playing: deep idle is vetoed, not blocked by a clock.** Measured 2026-10-04 with a stream
   running: the only counter that moves is `dpidle_block_cnt[by_oth]` (70/s in stage 1, 400/s with
   the screen on), it stops the moment the PCM closes, and `by_clk` / `by_cpu` / `by_vtg` do not
   move at all — `dpidle_handler` returns at its first test and never reaches the clock check.
   With `dpidle_mode=0` that first test is the callback registered through
   `register_dpidle_key_status_func`; the only registrant anywhere is Sony's `wm_key.ko`
   (`cdev_icx_key_is_active`, true when either of two driver state words is 1; they are written by
   its ADC key work queue and its wake interrupt). No key interrupt fires during playback and no
   process holds `/dev/icx_key`, so WHAT makes it true under a stream is not established. The
   2026-09-06 note that "a clock blocks it" was a reading of the summary line, not of the counters.
   Not a lever today: behind the veto the clock check has never run under a stream, and deep idle
   stops the PLLs the audio path needs, which is the likely reason Sony wants it vetoed.
3. **`deep early suspend` instead of the 30 s standby timer** — same power state, one control set
   once, the stream check done by the driver. A simplification, not a saving.
4. **Not levers:** the Bluetooth chip's "PLL" (it was USB), the CPU governor (already at the floor),
   the audio front end's `AFE_ON` (not in deep idle's condition set).

## Not established

* Sony's standby current, or Cinder's: this unit has no current sensor. A number needs
  `tools/battery_track.sh` over hours.
* Whether Bluetooth with a link up survives RAM suspend. Sony's sources answer `resSuspend(false)`
  while connected, presumably; not read.
* The order of operations inside `PowerService::SuspendInternal` (not disassembled).
