# hci_stp: a kernel HCI driver for the MT8590's Bluetooth core (Phase 4c)

Design note, 2026-11-24 (read from Sony's kernel source and firmware; nothing run yet). Plan:
[`docs/PLAN_hardware_first.md`](../docs/PLAN_hardware_first.md) Phase 4, step 6.

## Why

The Phase 4a spike puts a user process in the path of every Bluetooth packet: `btbridge` reads
`/dev/stpbt` (Sony's character driver), splits the byte stream into HCI packets and writes each to
`/dev/vhci`, and back. On LDAC that is hundreds of wake-ups a second and two extra copies each way.
A kernel driver registers an `hci_dev` and hands packets straight between STP and the Bluetooth core.

## What the kernel already offers

**MediaTek built for this.** STP, the protocol the core's firmware speaks over BTIF, has a "BlueZ"
mode (`stp_core.c`, `STP_BT_STK_IS_BLUEZ`): with `mtk_wcn_stp_set_bluez(1)`, every Bluetooth packet
STP receives goes straight to a callback registered with `mtk_wcn_stp_register_if_rx()`, instead of
into the ring `/dev/stpbt`'s `read()` drains. That callback is the receive half of a driver; the
original MediaTek `hci_stp.c` is not in Sony's tree, so ours is written from the interface alone.

**Every call is exported by the kernel image, not by a module.** Sony builds the connectivity
drivers with `WMT_EXP_HID_API_EXPORT`: `mtk_stp_wmt_soc.ko` keeps `_mtk_wcn_*` private and registers
them in a table; the public names are wrappers in the built-in `common_detect/wmt_stp_exp.c`, each
taking a trailing chip type (`CHIP_TYPE_SOC = 0`):

| Call | Use |
|---|---|
| `mtk_wcn_wmt_func_on(WMTDRV_TYPE_BT = 0, SOC)` / `_off` | power the Bluetooth core on and off (what `/dev/stpbt`'s open and close do) |
| `mtk_wcn_stp_set_bluez(1, SOC)` | receive by callback instead of the ring |
| `mtk_wcn_stp_register_if_rx(cb, SOC)` | the receive callback: `void cb(const u8* data, int size)`, one STP payload (an H4 packet, type byte first) |
| `mtk_wcn_stp_send_data(buf, len, BT_TASK_INDX = 0, SOC)` | send one H4 packet, type byte first |

The kernel has no `CONFIG_MODVERSIONS`, so a module links to these by name; the Bluetooth core is
the backports `bluetooth.ko` that `wifi/build-bluetooth.sh` already builds. **So no kernel image
change is needed**: `hci_stp.ko` loads with insmod next to `compat.ko` and `bluetooth.ko`, like the
Wi-Fi modules (the plan had it shipping with the kernel option on partition 8).

## The driver (`wifi/bt/hci_stp.c`, GPL-2.0, built by `wifi/build-bluetooth.sh`)

- **open**: `func_on`, `set_bluez(1)`, `register_if_rx(rx)`. **close**: the reverse, so Sony's stack
  finds STP as it left it.
- **setup** (once per power-on, before the core's own init): the four vendor commands btbridge sends
  today, built from the 64-byte NVRAM record `/data/BT_Addr` (read in the kernel, as Sony's
  `stp_chrdev_bt.c` reads its NVRAM file): `0xFC1A` address (record 0x00, byte-reversed), `0xFC79`
  radio (0x0C, 6), `0xFC93` TX power offset (0x1B, 3), `0xFC7A` sleep (0x12, 7). Module parameter
  `alt=1` changes the address, as `CINDER_BT_ALT` does for btbridge.
- **rx** (BTIF's receive path; treated as atomic): append to a reassembly buffer, cut whole H4
  packets by their length fields (one STP payload is normally one packet, but nothing guarantees it),
  `bt_skb_alloc(GFP_ATOMIC)`, `hci_recv_frame`. The quirk btbridge fixes stays fixed here: the
  core's Read Local Extended Features answer claims a page 2 it then refuses, so max page becomes 1.
- **send**: push the packet type byte, `mtk_wcn_stp_send_data`, free.

## Coexistence with Sony's stack

Only one owner at a time. Sony's `/dev/stpbt` open calls `set_bluez(0)` and registers its own event
callback; ours registers the BlueZ receive path. Bringing `hci0` up while Sony's `mtkbt` holds
`/dev/stpbt` would take the core's packets from it. The switch (`btsource_run.sh`, later Cinder's
setting) already turns Sony's Bluetooth off through Cinder before the own stack starts, and on the
way back brings `hci0` down (close: `set_bluez(0)`, power off) before Sony's stack starts again.

## Open, after the first load

- Whether BTIF's receive path calls `if_rx` with interrupts off (the driver assumes it may).
- The whole-chip reset WMT can do (`mtk_wcn_wmt_msgcb_reg`): Sony's driver turns it into `EIO`
  for its reader; ours should mark the device down (backports 3.17 has no `hci_reset_dev`).
- The measurement the phase exists for: BTIF interrupts and battery against Sony's stack and the
  vhci bridge, same headphones and codec.
