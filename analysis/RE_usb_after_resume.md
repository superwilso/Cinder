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

## Not yet proven

That the port really comes up after a resume with the ID at 3. Checklist 33.2 is the test.
