# Other repos worth knowing, and a universal backup tool

Research note, 2026-10-25. Everything here comes from each repo's README and LICENSE file, not its code, and nothing was run
on a device. Claims were re-checked against those files on 2026-10-25; anything still unconfirmed says so.

**Licence rule** (see `CLAUDE.md`): take facts from other projects, never code or tuning data,
unless the licence below says otherwise and the file is credited.

## Repos

| Repo | Licence | What it gives Cinder |
|---|---|---|
| [`neoncloud/walkman-tuning-guide`](https://github.com/neoncloud/walkman-tuning-guide) | CC BY-NC-SA 4.0 | Turns AutoEq/PEQ corrections into CXD3778GF tone tables: five cascaded biquads per chunk, 40-bit Q37 coefficients, one half per sample-rate family (44.1 / 48 kHz). Reports the tone DSP clock as about 176.4 / 192 kHz, not 4x the input rate. Says that on tested A50 devices the stock proc node can apply a table directly; the only node it names is `/proc/icx_audio_cxd3778gf_data/tct`, in a backup command. **Facts only**: share-alike does not fit PolyForm. Cinder has already decoded `tct` as the codec's five-biquad EQ and ships Sound ▸ Advanced ▸ **DAC EQ** (`analysis/RE_codec_tone_table.md`, `PLAN_sound_2026-09-29.md` §3), so this is a cross-check of our decode and the clock finding, plus a source of ideas for PEQ import and finer bands. It does not give us the table. |
| [`unknown321/wbrt`](https://github.com/unknown321/wbrt) | LICENSE opens with the Commons Clause condition; underlying licence not read | Windows 10+ GUI backup/restore for the MT8590 family: NW-A30/40/50, ZX300, WM1A/Z, DMP-Z1. Device VID_0E8D, PID_2000. No cross-model or cross-device restore checks. README does not describe the protocol, language or method. |
| [`unknown321/mediatek_flash_tool`](https://github.com/unknown321/mediatek_flash_tool) | MIT | Command-line dump/flash over the MediaTek bootloader. README: a fork of [`saleemrashid/mediatek_flash_tool`](https://github.com/saleemrashid/mediatek_flash_tool) "with modifications for MT8590 Walkmans - NW-A30/40/50, ZX300, WM1A, WM1Z, DMP-Z1", "only tested on MT8590, with Download Agent from SP Flash Tool" (examples use `MTK_AllInOne_DA_5.2136.bin`), eMMC devices only. Upstream (MIT, libusb, Argp) sends a Download Agent to the Preloader and takes an arbitrary address and length with no scatter file; it was tested only on the MT6580. Copyright lines: Saleem Rashid 2020, unknown321 2025. Implementation language not confirmed. |
| [`bkerler/mtkclient`](https://github.com/bkerler/mtkclient) | GPL-3.0 | General MediaTek exploit and flash tool, Windows (usbdk) and Linux. LICENSE is GPLv3. The README's chip list does not mention MT8590 (it lists V6 chips such as MT6781 and MT8985), though the flash tool above links to it for the DA file. Useful as a reference for BROM entry, not a dependency. |
| [`zhangboyang/llusbdac`](https://github.com/zhangboyang/llusbdac) | GPLv2 | Low-latency USB DAC kernel module for the ZX300A (about 50 ms, PCM only, behaves as Direct Source). Only a precedent for the USB-DAC work; keep any GPL code out of Cinder's tree. |
| [`unknown321/battery-charge-fix`](https://github.com/unknown321/battery-charge-fix) | GPL-3.0 | Per its description, fixes the charging indicator on an NW-A50 after a knock-off battery; the README says it only ignores the temperature warning. A50 only. Not needed. |
| [`unknown321/nw-installer`](https://github.com/unknown321/nw-installer) | LICENSE opens with the Commons Clause condition; underlying licence not read | README: "Generic service installer for Walkman NW-A50Series". Packs your files and a `run.sh` into `userdata.tar.gz` and builds `.UPG` installers with `make` and Docker, plus a Windows installer target. Overlaps what `cinder-installer` does; read for facts, not code. |
| Head-Fi [custom firmware thread](https://head-fi.org/threads/sony-walkman-custom-firmware-non-android.943661/page-545) | n/a | **Not verified.** The page returned a Head-Fi security checkpoint when fetched, so its contents were not read. A web search snippet suggested users post A50 findings there. Check it by hand before relying on it. |

## A universal backup tool

**Today:** wbrt is the only recovery path (`RECOVERY.md`), and it is Windows-only and a closed
loop: one device, one backup, restore to the same unit. The installer already runs on Windows and
Linux, so Linux and macOS users have to borrow a Windows machine before their first install.

**Goal:** one Cinder-owned tool that backs up and verifies the eMMC on Windows, Linux and macOS,
and refuses to restore the wrong image.

### Design sketch

1. **Transport.** Reimplement the MediaTek preloader/DA handshake in Rust (the installer is
   already dependency-free Rust), using unknown321's MIT `mediatek_flash_tool` as the reference
   for protocol facts and crediting it. Raw USB through `libusb` on Linux/macOS and WinUSB on
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

- The flash tool's README says the Download Agent flow was tested on an MT8590, so the route
  exists. Whether wbrt uses the same one is unknown: read wbrt's source or capture its USB traffic.
- The README's example DA is `MTK_AllInOne_DA_5.2136.bin`, taken from SP Flash Tool; may we
  redistribute it? Probably not, so the tool would need the user to supply it.
- How does the A55 enter the preloader (key combination, cable state), and is that stable?
- Does a read-only dump match wbrt's backup byte for byte? That is the acceptance test.
- Wbrt's underlying licence, beyond the Commons Clause condition, before reusing anything beyond facts.

### Order

1. Read-only dump on one A55, compared against a wbrt backup. No writes anywhere.
2. Backup header and verify mode.
3. Restore, tested only on a spare unit with a wbrt backup in hand.
4. Linux and macOS builds; installer integration.

Nothing here is committed to; it is a place to start from.
