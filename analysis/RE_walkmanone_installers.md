# Walkman One installers, StockRevert and external tunings — unpacked

2026-09-29. Source: MrWalkman's `Walkman_One_A50_(23_09)` bundle (installer, StockRevert, three
external tunings). Unpacked files live in `artifacts/walkmanone/re/` (git-ignored).

This supersedes **Layer 3** of [`RE_walkmanone_extract.md`](RE_walkmanone_extract.md), which said
the tuning packages could not be opened. They can, and they are not what that section guessed.

## How to open them

W1 seals its own packages with Sony's nw-wm1a KAS, but with the **AES passkey and IV each changed by
one character** (`…488c`→`…488e`, `…543a`→`…543b`). unknown321's
[nw-installer](https://github.com/unknown321/nw-installer) carries a patched Rockbox `upgtool` with
a `-w` flag for exactly this:

```
cd artifacts/repos/nw-installer/tools/upgtool/upgtools && make     # host build, needs libcrypto++-dev
upgtool -w -m nw-wm1a -e -z 2 -z 3 -o out/ NW_WM_FW.UPG            # tunings, StockRevert
upgtool    -m nw-a50  -e -o out/ WalkmanOne.UPG                   # the installer itself: stock A50 key
```

Every W1 package except the installer shares one decrypted header signature (`GC..yZ.$..+.~..#`).
Cinder's own on-device UPG (built by `upgtool -w`) carries the same one, which is why the player
accepts both.

## The Windows side is all Sony

| File | What it is |
|---|---|
| `Walkman_One_A50_(23.09).exe` | RAR5 SFX: Sony `FirmwareUpdateTool.exe` + `WmFwUpdater.dll` + `SWUpdate.xml` + `NW_WM_FW.UPG` (= `artifacts/walkmanone/WalkmanOne.UPG`, md5 `967461f0…`) |
| `1_StockRevert_Walkman_One_A50.exe` | Same SFX shape, 125 MB UPG |
| `2_NW-A50_V1_02.exe` | Sony's own signed updater (not repackaged) |
| `*_external_tuning/FirmwareUpdateTool.exe` | Same Sony tool, loose files next to it |

`FirmwareUpdateTool.exe` (`cba37351…`) and `WmFwUpdater.dll` (`30e3cae7…`) are byte-identical in
every package. Only `SWUpdate.xml` and the UPG change. The XML picks which USB product string the
tool will talk to: the installer targets `NW-A50Series`; StockRevert and the tunings target
`NW-WM1Z`/`NW-WM1A` (plus `DMP-Z1` for StockRevert), which is what a W1 player reports after the
model swap. `SWPackageWritePathName` is `\NW_WM_FW.UPG` in all of them: the tool copies the UPG to
the drive root and asks the player to reboot into the updater.

## What each package's script does

All three scripts are MrWalkman's "Update script v3.5" and use Sony's `fwpup -z` to inflate one
UPG entry straight onto a block device.

**Installer (`WalkmanOne.UPG`, stock A50 key).** Writes eight images: `p8` boot, `p9` recovery,
`p10`, `p12`, `p14` + `p15` (same image to both), `p19` system (838 MB), `p21`, `p25`. It does not
touch NVRAM (`p3`), NVP (`p22`) or the bootloader (`p7`). The first W1 boot does the rest from
`/sbin/boot_complete.sh`, including one-time backups `/opt2/stock/conf_bk` (`p22`) and
`/opt2/stock/nv_bk` (`p3`).

**StockRevert.** Writes stock images to `p8 p9 p10 p12 p14 p15 p19 p21` (not `p25`), then
`conf_bk > p22`, `nv_bk > p3`, deletes `/opt2/stock`, `/opt2/sig`, `/opt2/boot_count`, and
reboots. It does **not** restore `p7`.

**External tuning (all three).**

```sh
CSIG=`cat /opt2/sig`
[ "$CSIG" = "wm1z" ] || { log "not set to [WM1Z] … Aborting"; exit 0; }
fwpup -z -f /contents/NW_WM_FW.UPG -2 /dev/block/mmcblk0p3    # NVRAM, 5 MB
fwpup -z -f /contents/NW_WM_FW.UPG -3 /dev/block/mmcblk0p7    # uboot (LK), 384 KB
rm /contents/NW_WM_FW.UPG
```

Every line it logs goes to `/contents/CFW/boot_log.txt`. So a successful run and an abort both
leave a trace there.

## What a tuning actually is

| | WM1Z | Bright | Neutral & Warm |
|---|---|---|---|
| NVRAM image md5 (= W1's `TMD5`) | `ccb29dd2…` | `d7d08780…` | `d7d08780…` (same file as Bright) |
| Bootloader | LK built for **BBDMP2** (NW-WM1 series) | LK built for **BBDMP4** | same as Bright |

**What the bootloader changes, traced through the kernel (2026-09-29).** W1 runs the stock A50
kernel byte for byte (`md5 e861027b…`, same as `analysis/boot_image`), so any model-specific
behaviour has to come from command-line values. Every kernel load of those values was found by
scanning for `movw`/`movt` pairs against `/proc/kallsyms` (`artifacts/walkmanone/re/kernel/dis.py`):

| value | who reads it | effect here |
|---|---|---|
| `icx_bid2`, `icx_bid3` | `icx_check_bid`, PMIC late init, eMMC/SD tuning, DSI clock, NFC, `cxd3778gf_setup_platform` | The WM1Z LK doesn't pass them, so they stay −1 and `icx_check_bid` **reads the board ID from the hardware itself**: dmesg `[BID] invalid bid` then `bid2=0, bid3=0`. The A50's own LK passes the same hardware values, so nothing differs. |
| `icx_modelid` | `bq24262_wmport_probe` only (the WM-PORT charger) | Comes from NVP `mid`, not from the LK |
| `icx_sysinfo` | `cxd3778gf_reset`/`unreset` test bit `0x20`; backlight, touch, SD | Comes from NVP `syi`. Bit `0x20` is clear in stock (`0x1c`) and W1 (`0x1b`) alike |
| `icx_ship` | no kernel reader; `libConfigurationService` reads it from `/proc/cmdline` and falls back to NVP | Region, and NVP has it anyway |

In the codec driver, `bid3 == 4` sets platform flags `0x71`, anything else `0x59`, but the only
bits the driver tests are `0x1` and `0x2`, set the same way in both. `board_type` is fixed at 1 in
this kernel. With `bid3 != 4` the PMIC init prints `power off unnecessary LDO` and switches off
VGP1–3, VCAM_IO and VEMC_3V3, and that happens under either LK on this board.

**So on an A50 the swapped LK changes nothing the kernel or the codec sees.** What reaches them is
NVP identity, which W1 writes itself (`conf_a/b/c`, `nvpflag mid`), plus one userspace property,
`ro.boot.bid3=1`.

**The NVRAM image is only a fingerprint.** It is a whole MTK NVRAM partition (BT address, Wi-Fi
and AUXADC calibration files, MTK's generic audio-parameter names), taken from one source device.
Against **this player's own stock NVRAM** (`/opt2/stock/nv_bk`, `bc41b677…`) WM1Z's differs in
1,102 bytes: the file table's checksums (twice) and one 580-byte `BT_Addr` record that stock doesn't
carry. The audio-parameter records are identical. Bright/Neutral's differs from it only in the
checksums and 66 bytes at `0x20202`, with no `BT_Addr` record.
Its only job is to make `md5(p3)` equal `TMD5`, the gate `boot_complete.sh` checks.

**Bluetooth side effect, unverified:** the WM1Z record holds another player's address
(`8c:57:9b:8e:8a:f1`); this player's is `cc:98:8b:7d:af:1d`. `/data/nvram/APCFG/APRDEB/BT_Addr` is
rewritten on every boot, so which one would win after a tuning wasn't tested.

**NVP identity, for reference.** W1's `conf_a` (Warm/WM1Z) against this player's stock NVP: `mid`
WM1Z `0x21000008` "128G", `fpi` "NW-WM1Z", `kas` the nw-wm1a key, `shp` `0x0306`, `syi` `0x1b`
(stock `0x1c`), and `ser` **`5018758`** in place of this player's `5194859`. So every W1 player gets
the same serial number. `conf_b/c` (Bright/Neutral) are a different source NVP again. Zone layouts
move between images; the per-node dump from `/dev/icx_nvp/*` and Rockbox's `nvp_index_94b5fc` name
table are in `artifacts/walkmanone/re/nvp/`.

## What the gate unlocks, read from `boot_complete.sh`

When `md5(p3) == TMD5`:

- Hold + `PMD` choose the HAL: `normal`, `pv1` or `pv2`.
- `GMD` picks `gain_n` or `gain_l`.
- `DIM` picks the full `dacdat` init or `auto` only.
- `COL` sets the UI colour via `nvpflag clv`.

When it doesn't, the script loads `normal_nt` HAL + `normal_gain` + `normal_dac`.

`normal` and `normal_nt` HAL are the same file (`c8de2a65…`), so with `PMD=1`, `GMD=0`, `DIM=0`,
`COL=0` both branches load identical audio files. Only the spectrum-analyser parameter set differs.
**On such a player, re-applying the NVRAM half changes nothing you can hear.** It only matters for
Plus mode, lower gain, the "different" DAC init, or a UI colour.

## A persistence nobody documents

Neither StockRevert nor Sony's stock A50 updater writes `p7`. The stock UPG's
`SOURCE_FILES` are `boot.img secro.img logo.bin tz.img system.img cm4.bin` plus the NVP background;
`lk.bin` is handled by the script but not shipped. So **a player that ever had a tuning applied
keeps that model's bootloader through a full revert to stock.** On the owner's player the kernel
command line still carries `androidboot.bid3=1` (BBDMP2 LK) after W1 was reinstalled, while NVRAM
was back to `nv_bk`.

`wbrt` **does** cover `p7`: its backup runs from user-area offset 0 through `uboot`, `bootimg`,
`nvp` and `android` (see the memory note on wbrt coverage). A `wbrt` backup taken before any tuning
therefore holds the A50's own LK.

## What is audible, and where Cinder stands

Wampy's jack measurements (REW, line-in, 28 runs) found three things that change the signal: the
**CEW2 and KR3 regions** and **gain mode 1** (the WM1A curve). Plus modes, sound signatures and DAC
mode measured the same. Everything that measures different is a runtime table load, with no flash:

| W1 piece | where it lives | Cinder |
|---|---|---|
| Region cap lifted | W1 puts the plain A50 table in both `ov_127x` and `ov_127x_cew` | `voltable=stock`, the default since 2026-09-29; `region` keeps the cap |
| Gain mode 1 = WM1A curve | `/system/etc/.mod/gain/gain_l/ov_127x.tbl` (`39a60adc…`) | `voltable=wm1a`. It didn't work on W1 until 2026-09-29: W1 overwrites Sony's `ov_127x.tbl` with the A50 curve every boot, and the helper went by file name. It now checks content and finds `gain_l`. Verified with `--volcurve` |
| Tone table | W1 ships the WM1A/ZX300 `tc_127x` (`f678cb93…`) in place of `tc_1291` | `tone-wm1a` / `tone-w1` (same bytes) |
| Plus modes (HAL) | 3 bytes in `libaudiohal-adleralsa.so` | `signature` component. **Gap on W1:** W1's boot script copies its own HAL over the live one every boot, so Cinder's patch doesn't survive a reboot there. Only latency and the CPU floor change, and neither measures at the jack |
| Tuning (NVRAM + LK) | `p3`, `p7` | Not needed. The LK is inert on this board, and the NVRAM is a checksum token |

**Nothing audible needs the bootloader, NVRAM or NVP.**

## Open

1. **Why the 2026-09-28 WM1Z run did nothing.** The updater boot lasted 4.5 s (`last_kmsg`), the
   UPG was gone afterwards, NVRAM stayed `bc41b677…`, and `boot_log.txt` has no line from the
   tuning script (it always logs, even to abort). So Sony's updater stopped before running the
   script. Not pursued, since re-applying changes nothing audible.
2. The Plus-mode gap on W1, if anyone wants it: point `cinder-signature.sh` at W1's source copy
   (`/system/etc/.mod/adler/normal_nt/`) as well as the live library.
