# Other repos worth knowing, and a universal backup tool

Research note, 2026-10-25. Everything here comes from each repo's README and LICENSE file, not its code, and nothing was run
on a device. Claims were re-checked against those files on 2026-10-25; anything still unconfirmed says so.

**Licence rule** (see `CLAUDE.md`): take facts from other projects, never code or tuning data,
unless the licence below says otherwise and the file is credited.

## Repos

| Repo | Licence | What it gives Cinder |
|---|---|---|
| [`neoncloud/walkman-tuning-guide`](https://github.com/neoncloud/walkman-tuning-guide) | CC BY-NC-SA 4.0 | Turns AutoEq/PEQ corrections into CXD3778GF tone tables: five cascaded biquads per chunk, 40-bit Q37 coefficients, one half per sample-rate family (44.1 / 48 kHz). Reports the tone DSP clock as about 176.4 / 192 kHz, not 4x the input rate. Says that on tested A50 devices the stock proc node can apply a table directly; the only node it names is `/proc/icx_audio_cxd3778gf_data/tct`, in a backup command. **Facts only**: share-alike does not fit PolyForm. Cinder has already decoded `tct` as the codec's five-biquad EQ and ships Sound ▸ Advanced ▸ **DAC EQ** (`analysis/RE_codec_tone_table.md`, `PLAN_sound_2026-09-29.md` §3), so this is a cross-check of our decode and the clock finding, plus a source of ideas for PEQ import and finer bands. It does not give us the table. |
| [`unknown321/wbrt`](https://github.com/unknown321/wbrt) | GPLv3 with the Commons Clause (no selling the software or services built on it) | Windows 10+ GUI backup/restore for the MT8590 family: NW-A30/40/50, ZX300, WM1A/Z, DMP-Z1. Device VID_0E8D, PID_2000. No cross-model or cross-device restore checks. README does not describe the protocol. Its `Makefile` shows what it is built from: a Windows installer (MinGW-w64 in Docker, NSIS) that bundles a **prebuilt `flash_tool.exe` from unknown321's `mediatek_flash_tool` v0.1.7**, a **Download Agent (`DA.bin`) taken from the mtkclient 1.9 release**, and a driver installer built with **libwdi** for the VID_0E8D&PID_2000 device. So the protocol work lives in the MIT flash tool, not in wbrt. Only the build recipe and README were read; wbrt's own backup logic was deliberately not studied (see below). |
| [`unknown321/mediatek_flash_tool`](https://github.com/unknown321/mediatek_flash_tool) | MIT | Command-line dump/flash over the MediaTek bootloader. README: a fork of [`saleemrashid/mediatek_flash_tool`](https://github.com/saleemrashid/mediatek_flash_tool) "with modifications for MT8590 Walkmans - NW-A30/40/50, ZX300, WM1A, WM1Z, DMP-Z1", "only tested on MT8590, with Download Agent from SP Flash Tool" (examples use `MTK_AllInOne_DA_5.2136.bin`), eMMC devices only. Upstream (MIT, libusb, Argp) sends a Download Agent to the Preloader and takes an arbitrary address and length with no scatter file; it was tested only on the MT6580. Copyright lines: Saleem Rashid 2020, unknown321 2025. Implementation language not confirmed. |
| [`bkerler/mtkclient`](https://github.com/bkerler/mtkclient) | GPL-3.0 | General MediaTek exploit and flash tool, Windows (usbdk) and Linux. LICENSE is GPLv3. The README's chip list does not mention MT8590 (it lists V6 chips such as MT6781 and MT8985), though the flash tool above links to it for the DA file. Useful as a reference for BROM entry, not a dependency. |
| [`zhangboyang/llusbdac`](https://github.com/zhangboyang/llusbdac) | GPLv2 | Low-latency USB DAC kernel module for the ZX300A (about 50 ms, PCM only, behaves as Direct Source). Only a precedent for the USB-DAC work; keep any GPL code out of Cinder's tree. |
| [`unknown321/battery-charge-fix`](https://github.com/unknown321/battery-charge-fix) | GPL-3.0 | Per its description, fixes the charging indicator on an NW-A50 after a knock-off battery; the README says it only ignores the temperature warning. A50 only. Not needed. |
| [`unknown321/nw-installer`](https://github.com/unknown321/nw-installer) | GPLv3 with the Commons Clause (its LICENSE applies the Commons Clause to GPLv3, as wbrt's does) | README: "Generic service installer for Walkman NW-A50Series". Packs your files and a `run.sh` into `userdata.tar.gz` and builds `.UPG` installers with `make` and Docker, plus a Windows installer target. Overlaps what `cinder-installer` does; read for facts, not code. |
| Head-Fi [custom firmware thread](https://head-fi.org/threads/sony-walkman-custom-firmware-non-android.943661/page-545) | n/a | **Not verified.** The page returned a Head-Fi security checkpoint when fetched, so its contents were not read. A web search snippet suggested users post A50 findings there. Check it by hand before relying on it. |

## Staying clean of wbrt

wbrt is GPLv3 with the Commons Clause. Cinder is PolyForm Noncommercial, so none of wbrt's code,
scripts or installer logic goes into Cinder (`CLAUDE.md` licence rule). What was read, and what was not:

- **Read:** README, LICENSE, `Makefile`, the top-level file list. Used only to learn what it is made of.
- **Not read:** the installer script and whatever drives the backup (which regions, in what order,
  with what flags). Do not read it before the tool is built; if it has to be read later, record that
  here and have someone other than the author of the code do the comparison.
- **Where Cinder's tool takes its facts instead:** the MIT `mediatek_flash_tool` (credited,
  copyright lines kept), the region map already in `RECOVERY.md`, and measurements on a real A55.
- **Not wbrt's to give:** wbrt contributes nothing the flash tool does not. The backup is a flash-tool
  dump of regions; the value of a Cinder tool is the header, the checks and the verify step, none of
  which wbrt has.

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

### Step 1 result (2026-10-26): read-only dump works

`rescue/` (`cinder-rescue`, Rust, `rusb` with vendored libusb) ran on Windows against the A55 and
read wbrt's whole range, 2,885,156,864 bytes from user-area offset 0, in 21 minutes (2.2 MiB/s),
every 1 MiB packet checksum-verified. It has no write command.

- **Byte-for-byte check.** Every partition the running system does not write (`mbr` through
  `android`, `cm4`, `nvp`: 23 regions) has the same MD5 as the player's own `dd` of it. The ones
  that differ are mounted and written on the boot after the dump (`cache`, `var`, `db`, `usrdata`,
  `contents`), or are `option1-3`, whose only changes are ext4 mount bookkeeping (mount count
  289 → 290, mount time).
- **wbrt's backup is the same format**: a raw image of the user area from offset 0. Our dump and the
  owner's July wbrt backup agree on every partition nothing has rewritten since, at the same
  offsets, so an existing wbrt backup can be a restore source as it is, once restore exists. `bootimg`
  differs from July because the July image carried Wampy's init hooks; today's is byte-identical to
  Sony's stock 1.02 boot image.
- **Entry needs the buttons.** Player off, hold Play + Volume Down, plug the cable in. A normal
  reboot with the cable in never offered download mode (Windows saw no 0E8D device). Once the
  Download Agent is loaded the player stays in it until the host restarts it (watchdog command) or
  power is held for about 10 s.
- **Device facts read:** chip `0x8590`, hardware `0xcb01`, software `0x0102`; the DA entry used is the
  MT8590 one in `MTK_AllInOne_DA_v3.3001` (wbrt's `DA.bin`); eMMC CID
  `15010043 4B544134 320202B2 EACA8553`, whose serial matches the kernel's `emmc_boot_serial`.
- **Transport.** The first run, reading 1 MiB per USB transfer, stopped at 120 MiB with a pipe
  error; the second, reading 512 bytes per transfer like the flash tool, finished. One sample
  each, so the transfer size is the suspect, not proven. Speed was the same both ways.
- **Driver.** Windows used the WinUSB driver wbrt had already installed for VID_0E8D&PID_2000.

### Step 2 (2026-10-26): manifest and verify

Every dump now writes `FILE.manifest` beside it: chip, eMMC CID, the range, and a SHA-256 for each
of the 38 user-area partitions (the A50-series table from `/proc/dumchar_info`, clipped to the
range), hashed from the bytes as they arrived over USB. The dump then re-reads its own file
against that manifest, so "backup done" means "backup readable". `cinder-rescue verify FILE` does
the same later, and for a backup without a manifest (a wbrt one) it writes one from the file.

Both check the partition layout first: the MBR plus MediaTek's EBR chain (the MBR holds p2-p4 and
points at the first EBR; each EBR holds three partitions relative to itself and links the next
relative to the first) must put p2-p29 exactly where the table says, or the image is refused as
not an A50-series player's. Checked on the owner's dump and July wbrt backup: both pass, the
manifests agree with `sha256sum`, and the partitions whose hashes differ between the two are
exactly the ones found by the byte comparison above.

### Step 3 (2026-10-26): restore

`cinder-rescue restore --from BACKUP --parts bootimg[,...]` writes partitions back from the
player's own backup with the DA's SDMMC_WRITE_DATA (1 MiB packets, each with an additive checksum,
each answered with 0x69). It writes only `bootimg`, `recovery`, `android`, `usrdata`, `cache`,
`var` and `db`: the partitions a boot loop can break. The partition table, bootloader, `nvram`,
`nvp`, `pro_info`, `seccfg`, `tee` and `contents` are never written, and boot0 (the preloader)
is outside anything it can address. It refuses unless the chosen partitions still hash to the
backup's manifest, the backup's and the player's partition tables are both A50-series, and the
player's chip and eMMC CID equal the manifest's; a wbrt backup has no CID, so it cannot be
restored from directly. Each partition is read first and left alone if it already matches;
every write is read back and must hash to the manifest's value. `--dry-run` stops before writing.

First real write, on the owner's A55: `bootimg` rewritten with the identical bytes it already
held (`--even-if-same`), 16 MiB in 2 s (about 8 MiB/s, four times the read speed), read back
matching, and the player booted normally afterwards.

Then a restore that really changed the partition: `cache` (28 MiB, mounted read-write, so it
differs after every boot) put back from the afternoon's dump, written in 3 s and read back
matching. The player booted, ext4 replayed the restored image's journal on mount ("recovery
complete", as for any image of a filesystem that was in use), and all 13 files in `/cache` were
byte-identical to before.

Not yet done: the larger partitions (`android` is 800 MB: about 2 minutes to write, 6 to read
back), and a run on Linux or macOS.

### Open questions (all need hardware)

- Settled from wbrt's `Makefile`: it uses the same route, the flash tool plus a DA from mtkclient 1.9.
  Still untested by us on an A55.
- Download Agent: wbrt ships one taken from mtkclient (GPLv3); the flash tool's README uses an SP
  Flash Tool file. May Cinder redistribute either? Probably not, so the tool may need to fetch it
  or ask the user to supply it. The mtkclient `Loader` directory holds several DA binaries and shows no licence or origin notice for them; the SP Flash Tool file belongs to MediaTek. Neither is cleared. Check each file's terms.
- Driver: wbrt installs a WinUSB-style driver for VID_0E8D&PID_2000 using libwdi (LGPL v3 per its README). Whether a
  Cinder tool can reuse that or must ask the user to install a driver is open.
- How does the A55 enter the preloader (key combination, cable state), and is that stable?
- Does a read-only dump match wbrt's backup byte for byte? That is the acceptance test.

### Order

1. Read-only dump on one A55, compared against a wbrt backup. No writes anywhere.
2. Backup header and verify mode.
3. Restore, tested only on a spare unit with a wbrt backup in hand.
4. Linux and macOS builds; installer integration.

Nothing here is committed to; it is a place to start from.
