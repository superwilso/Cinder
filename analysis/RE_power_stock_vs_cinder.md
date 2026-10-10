# Power: what Sony's firmware does, what Cinder does, and the difference

**Date:** 2026-10-10. Offline RE of the stock 1.02 rootfs with Ghidra (`libPowerService.so`,
`libPowerMgrServiceFw.so`, `libUsbMgrServiceFw.so`, `HgrmMediaPlayerApp`, and the suspend handlers
of 22 service libraries), the Walkman One kernel image (`artifacts/walkmanone/re/kernel`), and
read-only readings from the owner's player (Walkman One 3.02, Cinder dev build, cable in).
Nothing was measured in milliamps: this player has no current sensor (`RE_battery.md`).

Earlier work this builds on: `RE_sony_idle_baseline.md`, `RE_early_suspend.md`,
`RE_kernel_idle_levers.md`, `RE_codec_power.md`, `RE_usb_after_resume.md`,
`docs/AUDIT_2026-09-28_power_sound_w1.md`.

## Summary

| Question | Answer |
|---|---|
| Does stock use a power mechanism Cinder does not have? | **No.** Stock has two sleep states and both are the kernel paths Cinder already drives. |
| Do Sony's services save power in their suspend callbacks, which Cinder never triggers? | **No.** On this model they are empty or bookkeeping. |
| Where is stock ahead at idle? | **Policy.** Stock suspends to RAM 10 s after the player goes idle. Cinder does it only when opted in, and then after 65 s. |
| Where is stock ahead during playback? | **Nowhere in software.** Same early suspend, same sound service, same low-power PCM. What differs is what the owner has switched on. |
| Why did USB die after a suspend under Cinder and not under stock? | Sony's USB manager resets the port's role on resume. Under Cinder nobody tells it the role, so it does nothing. Cinder now does the same two writes itself. |

## 1. Sony's two sleep states (`libPowerService.so`)

`PowerService::SetStatus` is one switch (`FUN_00014e0c`). The states and what each does:

| State | Steps, in order | Cinder's equivalent |
|---|---|---|
| `kWarmSleep` (3) | `ServiceManager::NotifyEarlySuspend`; wait up to 40 s for every service; take `powerservice_wake_lock`; `sync` twice; write `mem` to `/sys/power/state` | Stage 1: take the `cinder` lock, write `mem` |
| `kColdSleep` (2) | `NotifySuspend`; wait up to 40 s; from warm sleep release the lock, from awake write `mem` with no lock | Stage 2: release the lock |
| `kPowerOn` (4) | write `on`; after a cold sleep also write `icx_pm_helper/resume_lock_cancel` (ends Sony's 20 s resume wakelock), then `NotifyResume` or `NotifyLateResume` | write `on`, take the lock |
| `kPowerOff` (1) | `NotifyShutdown`, the 40 s barrier, power off | `cinder-power` |

If the services do not all answer in 40 s the service logs "Power state transition is stopped.
Reboot the system..." and restarts the player (`FUN_000154a8`).

The same library switches the codec's `deep early suspend` control on and off by name, and owns
battery care (`battery/device/fully_weakly`).

## 2. The suspend callbacks do nothing for power

Cinder writes `/sys/power/state` itself, so `NotifyEarlySuspend` and `NotifySuspend` never reach
Sony's services. Every override was decompiled to see what that costs:

| Service | `OnEarlySuspend` / `OnSuspend` |
|---|---|
| SoundServiceFw | `ReadyToEarlySuspend` / `ReadyToSuspend` — on this model (`FilterChainMgrDmp`) both only call the "done" callback |
| KeyService | forwards to its key component, which has only resume-key handling (which key woke the player); nothing stops the 10 Hz `select()` poll |
| DisplayService | base class, empty (the 10 Hz LED thread keeps running) |
| BtPlayerService, BtCommonService, BtTransmitterService | a trace line, or empty |
| ConfigurationService | writes the settings file out |
| TimerService | saves its settings and arms the RTC alarm (`TimerServiceAlarm::Sleep`) |
| UsbHostConnectionService, UsbDeviceConnectionService, PathMgr, Memmon | bookkeeping |
| AudioAnalyzer, AudioIn, Tuner, Volume, WiredHp, Nc, ConnMgr, FuncMgr, StorageMgr | base class, empty |

So bypassing PowerService loses no power saving. Two side effects are worth knowing: Sony's alarm
and sleep timer are not armed across a Cinder suspend (Cinder has its own), and settings Sony's
services hold in memory are not flushed first.

## 3. Stock's idle policy (`HgrmMediaPlayerApp`, `model/power_save/`)

* `DisplayoffTimerAction` blanks the screen and asks for warm sleep.
* `AutoSleepTimerAction` holds 27 condition flags (playing, display, USB, sleep timer, and so on).
  When all are clear it starts a timer of `sleepsec` seconds, default **10** (`FUN_001f0374`; the
  `persist.sys.sony.sleepsec` property overrides it and is unset on the player). When it fires
  the app goes to its Suspend state: every audio source is asked `reqSuspend` and answers
  `resSuspend`, then cold sleep.
* The app holds a wakelock while USB or the charger is in (`Acquire USB WakeLock`,
  `Acquire AC IN WakeLock`).
* `PowerMgrServiceFw` can pin both cores (`AcquireCpuNumBaseFullLock` writes
  `cpufreq/hotplug/cpu_num_base`); the normal value is 1, which is what the player reads.

Cinder, for comparison (`socsusp`, `main.cpp`): stage 1 after 5 s; stage 2 only with
`ram_suspend` set, after a further 60 s, never in the first 180 s after start, and never with a
Bluetooth peer linked.

## 4. USB after a resume: what Sony does

`libUsbMgrServiceFw.so` has a `UsbResumeMonitor` that listens for the `icx_pm_helper` resume
uevent and for the USB supply coming online, and then runs `UsbMgrImplWmport::SetOtgDevice`:

1. write `0` to `/sys/devices/platform/mt_usb/mode`;
2. read `musb-hdrc.0/mode` until it contains `idle` (ten tries, 100 µs apart);
3. write `1` to `/sys/devices/platform/mt_usb/mode`.

In the kernel (`mt_usb_store_mode`) `1` sets the port to `b_idle`, runs `musb_id_pin_sw_work(0)`
(the full switch to device: controller stopped, clocks and PHY set up) and then
`mt_usb20_sw_connect(1)` (`musb_start`). That undoes the host switch `musb_bus_resume` makes on
every resume (`RE_usb_after_resume.md`).

The service runs under Cinder too, but it only acts when the app has told it the role
(`SetUsbOtgMode`), and Cinder never does. Cinder's helper wrote `0` and never `1`.
**Changed 2026-10-10:** `cinder-msc usb-resume` now does steps 1 and 3 first. Not yet run on a
player (checklist §33.2).

## 5. The owner's player today (cable in, screen dark, paused)

* 74 timer wakeups a second. 20 are Sony's key and LED pollers, 9 the kernel's USB hub poll,
  3 Cinder, 3 the Bluetooth receive thread. Unchanged from 2026-10-05.
* `bt_on=1`, `bt_idle_off=0`: the Bluetooth radio is on with nothing linked and never switches
  off. That keeps four supplies (VCN28, VCN33, VCN33_BT, VCN_1V8) and the CONN power domain up
  through idle and through jack playback, and it pages the last headphones every 300 s.
* `ram_suspend` is set, so this player does suspend to RAM when paused off the cable.
* The sound service runs the 10-band EQ and the DAC EQ. Sony's published battery hours are for
  every effect off.
* The 256 GB card is exFAT through a user-space FUSE driver, as on stock. Its CPU cost during
  playback has not been measured.

## 6. What would save battery, in order

1. **Suspend to RAM by default, sooner.** This is the whole idle difference from stock. It needs
   §33.2 (USB after a resume) to pass with the change in section 4, then the default flipped and
   the 60 s dwell brought towards Sony's 10 s.
2. **Bluetooth auto off by default**, or at least when nothing has ever linked this session.
   Four supplies and a power domain for a radio nobody is using.
3. **A playback breakdown off the cable** (`tools/idle_probe.sh` with music playing): CPU per
   thread for the sound service, the decoder, the FUSE driver and the Bluetooth stack, and the
   deep idle count. That says whether effects, the card or the radio is the playback cost.
4. **A discharge number** (`tools/battery_track.sh`), so each of the above has a size.
5. Small and known: the two 10 Hz pollers in Sony's services (20 wakeups a second, no switch).

Not levers, checked: Sony's suspend callbacks (section 2), the CPU governor, the audio front
end's `AFE_ON`, the "Bluetooth PLL" (it was the USB PHY).

## 7. Sony's port reset, run live (2026-10-10, later)

From a healthy port, on the owner's player, detached from adb:

```
before        role=b_peripheral state=CONFIGURED
after `0`     role=b_idle       state=DISCONNECTED     dmesg: BQWMP: PHY off
after `1`     (1 s later)       state=CONFIGURED       dmesg: BQWMP: PHY off, PHY on, high-speed config #1
```

`BQWMP: PHY on` is the line that never appeared after a resume on 2026-10-05. Still to run: the
same writes from the post-resume host state (§33.2). cinder-home now also writes `0` when the
cable leaves after the helper forced the role, as Sony's `SetOtgIdle` does. Stage 2's dwell is
10 s (was 60), Sony's own delay.

## 8. Services Cinder does not use cost no battery

60 s, per thread, cable in, stage 1, paused: 72 wakeups a second and 0.75 % of a core in total.

| Thread | Wakeups/s | What |
|---|---|---|
| `kworker/0:1` | 13.5 | `hp_dbs_timer`, the CPU governor's sampler (workqueue trace). Deferrable: it does not wake an idle core |
| `WMPortService` x2 | 20 | KeyService and the LED thread, 10 Hz each |
| `khubd_poll` | 9 | kernel, no switch |
| `rcu_preempt` | 7 | kernel |
| `btif_rxd`, BtCommonService, `6620_launcherA` | 5 | the Bluetooth radio being on |
| `file-storage` | 2 | the USB gadget (cable) |
| cinder-home, all threads | 4 | 0.3 % of a core |

Every other Sony service — UpdateService, RecorderService, RemoteCmdInjCtrlDaemon, MemmonService,
NcServiceFw, OneTrackPlayerService, AudioInPlayerService, the Bluetooth LE pair, NfcService,
StorageMgr, FuncMgr, PathMgr, ConnMgr — recorded **zero** wakeups and zero CPU. Together they hold
about 60 MB of 467 MB with 169 MB free. Stopping them saves no power, and stopping a pst service
has restarted this player before (the hang checker). Not a lever.

## 9. The GPU, and the rest of the hardware

**Stock draws everything with the GPU.** `HgrmMediaPlayerApp` is Qt 5 Quick and the only Qt
platform plugin on the player is `libqeglfs.so`: every frame is a QML scene graph rendered by
the Mali-450 through `libEGL` / `libMali_linux.so`, with the GPU's 500 MHz PLL (MMPLL) running.
Cinder draws in software straight to `fb0` and MMPLL reads off the whole time. At about 5 ms a
frame, and no frames when nothing moves, software with the GPU powered down is the cheaper
arrangement; the GPU path (`player/cinder-ffi/src/gpu.rs`, `/contents/cinder_gpu_on`) is worth
its PLL only for a full-screen visualiser running at 60 Hz, and that needs a measurement.

**The renderer was not built for this CPU.** `readelf -A` on `libcinder_ffi.a`: ARMv6, VFPv2,
Thumb-1, and not one NEON instruction; ARMv6 also has no divide instruction. That is the baseline
of Rust's `arm-unknown-linux-gnueabihf` triple. The player is a Cortex-A7 (NEON, VFPv4, hardware
divide). `player/.cargo/config.toml` now passes `-C target-cpu=cortex-a7`. The size of the win is
the `raster — frames …` line in the log: 4.8 to 6.5 ms a frame before.

**The codec has an equalizer that costs no CPU.** Its five biquads (Cinder's DAC EQ) run in the
chip. Sony's 10-band EQ and the other effects run in the sound service, which is most of the
playback CPU. A curve moved to the DAC EQ with Sony's effects off should be the cheapest way to
listen with EQ; unmeasured.

## Not established

* What the 27 auto-sleep conditions are by name, and whether a linked Bluetooth peer is one.
* Stock's draw in either sleep state, or Cinder's.
* Whether section 4's sequence gives a port that stays up after a host reset.

## How to repeat

```
G=~/ghidra/support/analyzeHeadless; R=artifacts/rootfs_mnt/vendor/sony
$G /tmp/gh pwr -import $R/lib/libPowerService.so \
   -scriptPath analysis/clear_bass/ghidra -postScript TraceByStrings.java out.txt 1 "/sys/power/state"
$G /tmp/gh svc -import $R/lib/libSoundServiceFw.so \
   -scriptPath analysis/E_usbdac_ldac/ghidra -postScript DecompileByName.java out.c "^On(Early)?Suspend$"
python3 artifacts/walkmanone/re/kernel/dis.py mt_usb_store_mode
```
