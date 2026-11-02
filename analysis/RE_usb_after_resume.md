# USB is dead after a suspend to RAM: the cause, from the kernel image

2026-10-05. Kernel: `artifacts/walkmanone/re/kernel/Image` with `ks.txt` (`dis.py <symbol>`).
Evidence from the player: `/contents/cinder_usb_resume_dmesg.txt` of the same evening
(checklist 33.1 passed, 33.2 failed).

## What happens

1. Suspend: the root hub suspends, `musb_bus_suspend` switches the ID interrupt to device and
   calls `mt_usb_disable` (`BQWMP: PHY off`).
2. Resume: the root hub resumes and `musb_bus_resume` runs

   ```
   if (icx_pm_helper_bid2 != 3) { switch_int_to_host(); musb_id_pin_sw_work(...); }
   if (usb_connect) mt_usb_connect();
   ```

   so on every board whose ID is not 3 the port comes back as a HOST: `BQWMP: PHY on` at
   resume with no cable in, role `a_idle`, `mtk_musb->power` and `is_host` set.
3. A cable goes in. The charger driver detects it (`Connect USB. bcdet=1(STD)`) and calls
   `mt_usb_connect`, which returns at its first test:

   ```
   if (!mtk_musb || !mtk_musb->is_ready || mtk_musb->is_host || mtk_musb->power) return;
   ```

   No `musb_start`, no `[MUSB] USB connect`, no pull-up. The PC never sees the player.

This kernel has no runtime PM (`usb1/power/control` does not exist), so the root hub never
suspends by itself and the port stays a host until a restart.

`cinder-msc usb-resume` could not help: writing `peripheral` to the musb `mode` node changes the
OTG state (`a_idle` -> `b_idle`) but not `is_host`; `mt_usb/mode` only acts on the value `1`
(the helper wrote `0`); and the gadget's own pull-up (`musb_pullup`) stopped the controller
(`PHY off`) and did not start it again.

## The fix in Cinder

`/sys/module/icx_pm_helper/parameters/icx_bid2` is root-writable (0 on the NW-A55). cinder-home
writes 3 through `cinder-power usbid3` just before it releases the wake lock, and the player's own
value back within a second of leaving the suspend (`resume_as_device`, main.cpp). With 3 the
resume path skips the host switch and the port stays a stopped device, which is the state a cable
plug expects.

Who else reads `icx_pm_helper_bid2` (every `movw/movt` reference in the image; no `.ko` imports
it): `DSI_PHY_clk_setting` (acts only on 4), `check_max_rate` (the USB-DAC rate limit, read at
`afunc_setup`: 0 and 2 allow 192 kHz, 1/3/4 allow 384 kHz), and three init-time functions. Nothing
reads it between the suspend and the write back, except the one test this is for.

If cinder-home dies between the two writes the ID stays 3 until the next restart: the USB-DAC
would then advertise the higher rate limit. Nothing else changes.

## Run on the player, 2026-10-05 22:20 (dev build, Walkman One)

Suspended at 253.5 s, Power woke it at 254.6 s, the cable went in, and five seconds later
`cinder-msc usb-resume` found `gadget state=CONFIGURED` and did nothing. Windows listed the player.
Entering and leaving USB mass storage by hand worked afterwards.

Left over, dev channel only: Windows saw product ID `0B8B`, not the storage-plus-adb composite the
dev build sets at boot, and after the mass-storage round trip adb showed the player `offline` until
a restart. So the gadget's mode after a resume is not the one Cinder composed, and adbd does not
survive. Next step: have `usb-resume` log `functions` and `idProduct`, then re-run the dev adb
composition after a resume.

Not checked: the write back to the player's own ID (the restart that followed reset it anyway).

## Correction, later the same evening: the port enumerates but does not stay up

The PC's own kernel log (WSL, usbip) for that run: two attaches reached `USB Mass Storage device
detected` and dropped after 3 and 6 seconds; the third got as far as `SetAddress` and then every
control transfer timed out (`urb->status -104`) until it was abandoned. Windows kept listing the
device throughout. So with the ID at 3 the port is back on the bus and answers a first
enumeration, and fails after a host reset or under transfers. `0B8B` is init's plain `adb`
configuration (init.usbcfg.rc), so that product ID was not itself a fault.

The dead storage device also left the PC's Linux side unable to `sync` (every later harness and
launcher-test run hung on it) until `wsl --shutdown`.

Next: `scratchpad/resume_probe.sh`, armed on the player, saves dmesg, `/sys/kernel/debug/musb/regdump`,
the PLL table and the gadget state at the resume and 4, 20 and 60 s after the plug. Healthy
values to compare: `Power 71`, `DevCtl 99`, `MISC 0f`.

## 2026-10-10: what Sony's firmware does about it

`libUsbMgrServiceFw.so` has a `UsbResumeMonitor` (the `icx_pm_helper` resume uevent, and the USB
supply coming online) that runs `UsbMgrImplWmport::SetOtgDevice`: write `0` to
`/sys/devices/platform/mt_usb/mode`, wait for `musb-hdrc.0/mode` to read `idle`, write `1`. In
`mt_usb_store_mode`, `1` runs `musb_id_pin_sw_work(0)` and `mt_usb20_sw_connect(1)`: the controller
stopped and started as a device. The service runs under Cinder but acts only once the app has set
the role, which Cinder never does. `cinder-msc usb-resume` wrote `0` and never `1`; it now does
Sony's two writes as its first step. Not yet run on a player. Detail:
`RE_power_stock_vs_cinder.md` section 4.

## 2026-10-26: Sony's service does it, once it is told adb is wanted

`UsbMgrImplWmport::Resume` (and the `UsbResumeMonitor` on the resume uevent and the USB supply)
runs `UpdateUsbConfig` on its own thread: re-enable the gadget, `SetOtgIdle` then `1` to
`mt_usb/mode` (device), then `SetMsc`. `SetMsc` writes `mass_storage,adb` when the service's adb
flag (`this[0x38]`) is set and bare `mass_storage` when not, and `UpdateUsbConfig` starts or stops
adbd to match. None of it waits for the app's USB role: the wanted OTG mode defaults to device.

Stock leaves the adb flag off, so on every cable event and resume the service stripped adb from the
gadget; that was the "dev adb does not come back" half. The dev build now calls
`UsbMgrServiceFwClient::SetAdbEnabled(true)` (vtable slot 10, a one-bool request) once at boot
(`usb_adb_enable_dev`, main.cpp). Run on the owner's player, stock 1.02: suspend to RAM at 209 s,
Power at 211 s, cable in at ~217 s; the gadget was already `CONFIGURED` as `mass_storage,adb`
(product 0ca0, Sony's), `cinder-msc usb-resume` found a host and did nothing, adbd running.
Checklist 33.2 passes. The stable channel has no adb and does not set the flag.

## 2026-11-02: a wall charger is a cable with no host (#52)

Reported on an NW-A55 left on a wall charger overnight: cinder-home exited `rc=42` after 45,213 s
(12.5 h), the last lines in its log were `cinder-msc: usb-resume: step 4, init's adb block:
state=DISCONNECTED — STILL DOWN` and `musb role now = b_idle`, and every boot after was held on
stock by the bad-boot latch.

What the evidence says, and what it does not:

- **`/sys/class/power_supply/usb/online` reads 1 on that charger.** The resume path only runs
  `usb-resume` when it does, and it ran. So that node does not tell a charger from a computer;
  `android_usb/android0/state` reaching `CONNECTED`/`CONFIGURED` is the only sign of a host. Not
  measured here which chargers read as `usb` rather than `ac`; a non-standard one (data lines open)
  may be the reason.
- **With no host, `usb-resume` walks every step.** Steps 0–3 wait up to 4 s each for a host, step 3
  adds 0.5 s, step 4 (`usb_rescue`) polls `getprop sys.usb.state` up to 60 times (a `popen` each)
  and then waits 4 s more, and dmesg is copied twice: over 25 s, worst case past 30 s.
- **The call ran under `run_watchdog_only("usb: resume", 20, …)`**, where an overrun is fatal by
  design (a forked child cannot be abandoned), and the fatal path latches the counter to MAXBAD even
  after a healthy start. The backtrace (`waitpid` under `system()`) fits; the WATCHDOG line naming
  the call was not in the paste.

Fixed in `src/usb_resume_gate.h` (host-tested, `tools/usbresume_selftest.cpp`): the budget is
60 s, and a run that finds no host is not repeated until `usb/online` drops, so a charger costs one
run per plug instead of one per resume. Not yet run on a player with a wall charger.
