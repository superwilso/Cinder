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
