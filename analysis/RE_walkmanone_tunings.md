# What a Walkman One sound signature is made of (A50 build)

2026-11-12. Read from MrWalkman's `Walkman_One_A50_(23_09)` bundle (unpacked as in
[`RE_walkmanone_installers.md`](RE_walkmanone_installers.md)), from captures of one NW-A55 under
Walkman One and under stock 1.02, and from Sony's published kernel source for the NW-A50
(the `cxd3778gf` codec driver). Nothing here was run on a player today.

## 1. Result

The four signatures (Neutral, Warm, Bright, WM1Z) and the three "external tuning" packages change
**which device the player says it is**, and nothing that reaches the codec or the effect chain:

| what differs between signatures | where | who reads it |
|---|---|---|
| the product name: `NW-WM1Z` (Neutral, Warm, WM1Z) or `DMP-Z1` (Bright) | NVP node 33 | `libDmpFeature.so` only (section 5) |
| which donor device's NVP image is written (two donors) | NVP, whole partition | identity fields only (section 3) |
| the bootloader: built for BBDMP2 (WM1Z) or BBDMP4 (the other three) | `p7` | reads the board ID from the same ADC as the kernel (section 4) |
| an NVRAM image whose md5 the boot script checks | `p3` | the boot script, as a gate |
| the signature's name in the menus | translation files | the player app |

Every setting that does reach the sound is shared by all four signatures: the gain mode, the
region, the Plus mode and the WM1-class sound libraries.

No paid tuning for the A50 build was found. The bundle holds three packages, MrWalkman's pages for
the A50 list the same four signatures, and the boot script accepts exactly four names and two
NVRAM checksums.

## 2. What the boot script does with a signature

The script reads `SIG=0..3` from the settings file. Per signature it knows a name, a product name
to write to NVP, a model ID and the md5 it expects of the NVRAM partition:

| `SIG` | name | NVP image written | product name | model ID | expected NVRAM md5 |
|---|---|---|---|---|---|
| 0 | Neutral | `conf_c` | `NW-WM1Z` | `0x21000008` | `d7d08780…` |
| 1 | Warm | `conf_a` | `NW-WM1Z` | `0x21000008` | `d7d08780…` |
| 2 | Bright | `conf_b` | `DMP-Z1` | `0x21000008` | `d7d08780…` |
| 3 | WM1Z | `conf_a` | `NW-WM1Z` | `0x21000008` | `ccb29dd2…` |

A signature change writes the NVP image, puts the player's own NVRAM backup back, and asks for the
matching tuning package. Past that point the script uses the signature for two things only: the
folder of translation files it copies, and the md5 gate. The audio driver library, the analyzer
delay files, the gain tables and the `dacdat` calls are chosen by `PMV`/`PMD`/Hold, `GMD`, `DIM`
and `REG`, never by the signature.

## 3. The three NVP images, node by node

**Layout** (worked out from a live zone map and checked against a per-node dump of the same
player: 50 of 50 populated large nodes match). The image is 15 MiB. Area `a`'s pointer table is at
`0x2000 + 0x200·a`. Area 0 holds 32-bit sector numbers (data at `n·0x200`); every other area holds
16-bit cluster numbers (data at `n·0x2000`), one per cluster a node spans; 0 means never written.

**Diff.** `conf_b` and `conf_c` are the same donor and differ in one node: the product name.
`conf_a` is a second donor. Between the two donors:

| node | `conf_a` | `conf_b` / `conf_c` |
|---|---|---|
| 2 u-boot password, 6 board ID, 8 product code, 9 serial, 15 checker flag, 77 NVRAM init flag | donor 1 | donor 2 |
| 33 product name | `NW-WM1Z` | `DMP-Z1` / `NW-WM1Z` |
| 40 (settings block 4) | 4 bytes: the block's checksum | |
| 41 (settings block 5) | 2 bytes: `1`, `0x7f` | `0`, `0x33` |
| 63 (settings block 27) | 2 bytes inside a time stamp | |
| 67 (settings block 31) | factory trace text (a 2017 board) | factory trace text (a 2019 board) |

Model ID, destination, key, the noise-cancel parameter block (blank in all three) and every other
node are identical. Blocks 36–63 are the player's saved settings, and the script copies the
owner's own blocks back over them after a signature change, so the two bytes in block 41 do not
survive as part of a signature either.

## 4. The bootloaders

The Bright and Neutral/Warm packages carry the same two payloads byte for byte (NVRAM
`d7d08780…`, bootloader `9dd3b309…`). Only the WM1Z package differs (NVRAM `ccb29dd2…`,
bootloader `541af597…`).

`RE_walkmanone_installers.md` showed that the WM1Z bootloader passes no board ID, so the kernel
reads it from the hardware. The other bootloader does pass `icx_bid2` and `icx_bid3`, and its code
was read today (Thumb, base `0x81E00000`, the function at `0x81E1DA1C`): it takes each value from
an ADC channel and sorts it into bands, as the kernel's `icx_get_bid` does
(`drivers/power/icx_pm_helper.c`: board ID 2 on channel 5, 3 on channel 6). So both bootloaders
hand the kernel this board's own IDs. On the captured player the command line is the same under
Walkman One and under stock apart from the model ID.

Not examined: what else a bootloader built for another board sets up before the kernel starts
(clocks, PMIC rails). The kernel sets the audio rails and the codec up again itself.

## 5. Who reads the product name

`DmpFeature::IsModel` and `IsShip` are called only inside `libDmpFeature.so`; no other library or
program imports them. The functions that use the model:

| function | effect |
|---|---|
| `GetDseeHxCustomHpDcfgFilename` | which bundled-headphone file DSEE HX loads. Walkman One ships two such files and they are identical |
| `GetFeatureUsbPidMsc` / `Uac` | the USB product ID |
| `GetFeatureVolume…Init`, `…AvlsThresh`, `…VolAlertThresh` | first-boot volume, AVLS and volume-alert thresholds |

So `DMP-Z1` against `NW-WM1Z` cannot change playback with effects off, and with DSEE HX on it
selects between two identical files.

## 6. `DIM`, the "DAC initialization mode"

`DIM=0` runs `dacdat auto` and then the noise-cancel loads (`cnw500n`, `cnw750n`, `cnc31n`,
`idata`, `ainc_*`, `limiter_*`); `DIM=1` runs `dacdat auto` alone. Those extra nodes belong to
`cxd3778gf_dnc_core.ko`. Walkman One does not ship or load that module (its `lsmod` has no entry;
stock's has), and the codec driver itself registers only `ovt`, `ovt_dsd`, `dgt` and the `tct`
nodes (`cxd3778gf_table.c`). So on this build both settings load the same data.

## 7. The volume table, from Sony's source

`cxd3778gf_table.h` gives the record `RE_volume_tables.md` took from elsewhere: 13 one-byte
fields per volume step (`linein sdin1 sdin2 play lineout hpout cmx1_500 cmx2_500 cmx1_750 cmx2_750
cmx1_31 cmx2_31 hpout2_ctrl3`), 121 steps, 27 output tables, twice (sound effect off, on).

- The effect-on half is used only while an effect is on **and** the rate is 192 kHz or lower
  (`cxd3778gf_volume.c`).
- On a board of type A the single-ended table is always `SMASTER_SE_HG`; the low-gain table is
  reached only on type Z boards (`get_table_index`). The A50 is type A, so "gain" on this player
  is whatever the loaded file holds in its high-gain table.
- `hpout2_ctrl3` is `1` at every step of the tables this player uses, in all three files.

| table `SMASTER_SE_HG`, effect off | vol 0 | 20 | 40 | 60 | 80 | 100 | 120 |
|---|---|---|---|---|---|---|---|
| A50 `ov_1291` (= Walkman One `gain_n`), `hpout` | 4 | 80 | 100 | 100 | 100 | 180 | 228 |
| WM1A (= Walkman One `gain_l`), `hpout` | 4 | 44 | 84 | 124 | 164 | 204 | 228 |
| A50, `sdin2` | 239 | 239 | 239 | 219 | 199 | 179 | 167 |
| WM1A, `sdin2` | 242 | 242 | 242 | 222 | 202 | 182 | 170 |
| A50, `play` | 179 | 198 | 216 | 236 | 0 | 0 | 0 |
| WM1A, `play` | 178 | 207 | 220 | 230 | 240 | 250 | 0 |

Walkman One's `gain_n` files are the A50's own tables under the WM1 names (same md5), so with
`GMD=0` the codec gets the stock curve. The image also carries `ov_1280` (the `1280` variant): its
plain headphone tables (`SMASTER_SE_LG`, `SMASTER_SE_HG`) equal the A50's; it differs in the noise-cancel,
ambient and line-out tables.

## 8. Mixer state under both firmwares

`amixer contents` on the same player, Walkman One against stock 1.02, 51 controls: the only
differences are the three noise-cancel gains, the noise-cancel headphone type, the volume position
and one power switch. Output device, amplifier and gain-mode controls are the same.

## 9. What does differ in sound between Walkman One and stock

All of it sits in parts every signature shares:

| part | what changes |
|---|---|
| volume table | `gain_l` is the WM1A curve; `gain_n` is stock's |
| region | the plain table is put in both the plain and `_cew` slots, so the `_cew` regions lose their lower `play`/`sdin2` values |
| effect set | the WM1-class sound library: DSEE HX in place of DSEE AI, the 10-band EQ and Tone Control in place of the six-band EQ, VPT and Clear Phase; Source Direct in place of ClearAudio+ |
| Plus modes | three bytes of the jack driver library: which PCM device, and the CPU clock floor while it is open |
| DSD and high rates | the feature switches allow native DSD and PCM above 192 kHz without conversion. Native DSD on the single-ended jack also needs `persist.sys.sony.icx.sehp.dsd`, which was not set on the captured player |
| effect parameter files | identical to stock's except `DseeHxCustom_Mode_0.bin` |

With every effect off the chain is skipped whole in both builds (`RE_sound_service.md` section 3).

## 10. Two rules read from the stock sound library

**Source Direct and the volume table half.** `FilterChain::SetupFilters` (stock `0x112720`) writes
the codec's `sound effect` switch: on only when `FilterInConfig::ShouldDriverVolTblOff` is false
**and** `CheckNeeded` says an effect needs headroom (the same test that switches `heq`, `attn` and
`alc`). `ShouldDriverVolTblOff` is true under Source Direct, which the stock A50 build cannot
enter (`IsFeaturedSourceDirect` is 0). But with every effect off `CheckNeeded` is false, so the
switch goes off anyway. A stock player with all effects off therefore runs the same half of the
volume table, and skips the same chain, as Walkman One with Source Direct on. Not checked on a
player: read `sound effect` with `amixer` while a track plays.

**DSD: native or converted.** `dsdprocessrules::ResolveProcCond` (stock `0x1049c8`) takes the DSD
mode, the single-ended switch, the output device and the DSD rate (2.8, 5.6 or 11.2 MHz):

| output device | native when |
|---|---|
| 1 | the mode is 1 |
| 2, 3 | the mode is 1 and `persist.sys.sony.icx.sehp.dsd` is set |
| 4 (USB DAC out) | always |
| 5 (Bluetooth) and others | never |

`FilterInConfig::Load` (stock `0x116540`) fixes the mode at 2 unless
`DmpFeature::IsFeaturedDsdProcNative` is true, in which case it is the stored setting
(`builtin_dsd_process_mode`). The conversion filter and gain (`dsd_conv_filter_type`,
`dsd_conv_gain_mode`) are read in both cases. So the stock build always converts DSD on the jack;
Walkman One's build converts unless the mode is 1, and on outputs 2 and 3 also unless the property
is set. Which number the A50's jack is was not read.

The second flag `ResolveProcCond` takes is the Source Direct flag: with it set, a converted DSD
stream uses a different table of output rates. So Source Direct changes what rate DSD is converted
to. The two tables were not read out.

**Measured, 2026-11-12, stock A55 1.02 with Cinder.** Through `SoundServiceSettings::SetParams`
(`cinder-probe --dsdnative`), `builtin_dsd_process_mode` reads back 2 at boot. Set to 1, a
generated DSD64 `.dsf` played from the library opened PCM device 2 (`dsdenc`, `DSD_U8`) instead of
S32_LE at 176.4 kHz on device 4 (`cxd3778gf-icx-lowpower`); set back to 2, it converted again. The
setting held across track starts. Both keys were sent. `se_hp_dsd_native_enabled=1` is accepted
but reads back 0; it is still needed: after a reboot with only `builtin_dsd_process_mode=1` sent
(read back 1), the file converted, and sending the second key as well made the next start native.
So the jack is output device 2 (`SeHeadphone`) as the rule above reads, and that key's getter
reads some other store. Not yet heard at the jack.

**Native DSD and the volume curve.** In DSD mode the driver sets the DSD encoder's gain from
`ov_dsd_*.tbl` and the analogue attenuator from the `hpout` of tables 12–23 in `ov_*.tbl`
(`cxd3778gf_set_master_volume`). The A50's own file (`ov_1291`) gives DSD64 the PCM steps (80 at
20, 100 from 40). The WM1A file holds `hpout` at 4 up to step 70 and starts at 68 at step 80, for
every DSD rate. So with that curve loaded native DSD is near silent below step 80.

**The runtime setter has no feature check.** `FilterInConfig::Set` (stock `0x1156c8`) takes
`builtin_dsd_process_mode`, `se_hp_dsd_native_enabled`, `dsd_conv_gain_mode`,
`dsd_conv_filter_type`, `sourcedirect`, `btaudiosoundeffect`, `disablesoundeffects` and
`reenablesoundeffects` by name and stores them directly; only `Load` asks `DmpFeature`. These are
the keys `SoundServiceSettingsDmp` sends as `<key>=<number>` (`RE_dsp_effects_surface.md`). What
is not known: when `Load` runs again and puts the stock constants back.

## 11. Open

1. Jack measurements: distortion and noise, not only level and frequency response, across the
   gain tables and the two PCM devices. The published measurements cover frequency response.
2. What the native-DSD switches do on a stock library when answered as Walkman One's
   build answers them (`FilterInConfig::Load` reads a DSD mode from `DmpConfig` key 3 when
   `IsFeaturedDsdProcNative` is true, and fixes it at 2 otherwise).
3. The units of `sdin2`, `play` and `hpout`: the driver source is here now; not worked through.
