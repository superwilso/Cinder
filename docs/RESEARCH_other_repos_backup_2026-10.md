# Other repos worth knowing, and a universal backup tool

Research note, 2026-10-25. Everything here comes from reading each repo's README page, not its
code, and nothing was run on a device. Treat claims as leads until someone checks them.

**Licence rule** (see `CLAUDE.md`): take facts from other projects, never code or tuning data,
unless the licence below says otherwise and the file is credited.

## Repos

| Repo | Licence | What it gives Cinder |
|---|---|---|
| [`neoncloud/walkman-tuning-guide`](https://github.com/neoncloud/walkman-tuning-guide) | CC BY-NC-SA 4.0 | Turns AutoEq/PEQ corrections into CXD3778GF tone tables: five cascaded biquads per chunk, 40-bit Q37 coefficients, one half per sample-rate family (44.1 / 48 kHz). Reports the tone DSP clock as about 176.4 / 192 kHz, not 4x the input rate. Lists the A50 as tested, with direct writes through a stock proc node. **Facts only**: share-alike does not fit PolyForm. Feeds `PLAN_sound_2026-09-29.md` (tone table decode). |
| [`unknown321/wbrt`](https://github.com/unknown321/wbrt) | not stated on the page | Windows 10+ GUI backup/restore for the MT8590 family: NW-A30/40/50, ZX300, WM1A/Z, DMP-Z1. Device VID_0E8D, PID_2000. No cross-model or cross-device restore checks. Page does not describe the protocol. |
| [`unknown321/mediatek_flash_tool`](https://github.com/unknown321/mediatek_flash_tool) | MIT | Command-line dump/flash over the MediaTek bootloader, described as for MT8590 Walkmans. C. Appears to be the MT8590 adaptation of [`saleemrashid/mediatek_flash_tool`](https://github.com/saleemrashid/mediatek_flash_tool) (MIT; libusb, Argp; sends a Download Agent to the Preloader; arbitrary address and length, no scatter file; eMMC only; upstream tested only on MT6580). The fork relationship is inferred, not checked. |
| [`bkerler/mtkclient`](https://github.com/bkerler/mtkclient) | GPL-3.0 | General MediaTek exploit and flash tool, Windows (usbdk) and Linux. The README does not list MT8590. Useful as a reference for BROM entry, not a dependency. |
| [`zhangboyang/llusbdac`](https://github.com/zhangboyang/llusbdac) | GPLv2 | Low-latency USB DAC kernel module for the ZX300A (about 50 ms, PCM only, behaves as Direct Source). Only a precedent for the USB-DAC work; keep any GPL code out of Cinder's tree. |
| [`unknown321/battery-charge-fix`](https://github.com/unknown321/battery-charge-fix) | GPLv3 | Fixes the charge indicator on an A50 after a third-party battery swap. Not needed, but shows what the platform allows. |
| [`unknown321/nw-installer`](https://github.com/unknown321/nw-installer) | not checked | C, tagged walkman/walkmanone, no description on the listing. Worth reading next, since installers are Cinder's job too. |
| Head-Fi [custom firmware thread](https://head-fi.org/threads/sony-walkman-custom-firmware-non-android.943661/page-545) | n/a | Real-user reports on the A50 series. Good for symptoms, not for specs. |

## A universal backup tool

**Today:** wbrt is the only recovery path (`RECOVERY.md`), and it is Windows-only and a closed
loop: one device, one backup, restore to the same unit. The installer already runs on Windows and
Linux, so Linux and macOS users have to borrow a Windows machine before their first install.

**Goal:** one Cinder-owned tool that backs up and verifies the eMMC on Windows, Linux and macOS,
and refuses to restore the wrong image.

### Design sketch

1. **Transport.** Reimplement the MediaTek preloader/DA handshake in Rust (the installer is
   already dependency-free Rust), using the MIT `mediatek_flash_tool` as the reference for
   protocol facts and crediting it. Raw USB through `libusb` on Linux/macOS and WinUSB on
   Windows. First milestone is **read-only**.
2. **Scope.** Dump the same regions wbrt covers (the 2.68 GB window in `RECOVERY.md`), plus a
   full-eMMC mode if the Download Agent allows it, so the preloader in boot0 is also covered.
3. **Safety, the part wbrt lacks.** Write a small header with each backup: model, serial hash,
   firmware version, region map, SHA-256 per region. On restore, refuse a different model
   outright, refuse a different serial unless the user types a confirmation, and verify hashes
   before the first write. Never write NVP or the serial region from a foreign image.
4. **Verify mode.** Read back and compare after a backup, so "backup done" means "backup readable".
5. **Wbrt compatibility.** Import and export wbrt's backup layout so existing backups stay usable.
   Needs the format, which the README does not give.
6. **Integration.** Installer step 1 becomes "back up now" with the new tool, falling back to
   pointing at wbrt.

### Open questions (all need hardware)

- Does the upstream Download Agent flow work on an MT8590, or does wbrt use a different path?
  Read wbrt's source or capture its USB traffic.
- Which Download Agent file is used, and may it be redistributed?
- How does the A55 enter the preloader (key combination, cable state), and is that stable?
- Does a read-only dump match wbrt's backup byte for byte? That is the acceptance test.
- Licence of wbrt (not stated on the page), before reusing anything beyond facts.

### Order

1. Read-only dump on one A55, compared against a wbrt backup. No writes anywhere.
2. Backup header and verify mode.
3. Restore, tested only on a spare unit with a wbrt backup in hand.
4. Linux and macOS builds; installer integration.

Nothing here is committed to; it is a place to start from.
