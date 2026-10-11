# Firmware map: every Sony component, what it does, and how far it has been read

Started 2026-10-11. This is the index for the reverse-engineering notes in this directory. One row
per component; the "read" column says how far the reading has gone, so a gap is visible as a gap.

Read levels: **full** (every exported function accounted for and the behaviour written down),
**part** (the parts a Cinder feature needed), **surface** (exports and strings only), **none**.

## Two firmwares, and which one the player runs

| | stock tree | Walkman One tree |
|---|---|---|
| path | `artifacts/rootfs_mnt` | `artifacts/w1_rootfs/sony` (the `/vendor/sony` half) |
| source image | stock NW-A50 `6.bin` | Walkman One `7.bin` (`artifacts/unpacked/walkmanone`) |
| `ro.sony.version` | 1.02 | 3.02 |
| Sony sound library has | `DseeAi` | `DseeHxCustom` |

The owner's NW-A55 runs Walkman One. It keeps the A50's player app (the same size, patched in
place, `RE_walkmanone_extract.md` section 5) but replaces 18 Sony libraries with a different build
of the same code base: its sound service carries the WM1-series DSEE HX class in place of the
A50's DSEE AI and a different effect chain (`RE_sound_service.md` section 3). Which model's
firmware those libraries were taken from is not established; `ro.sony.version` reads 3.02.
**Read the Walkman One copy when the question is what the player does today, and the stock copy
when the question is what an unmodified A50 does.**

Libraries that differ (md5, 2026-10-11): `libConfigurationService`, `libDmpConfig`,
`libDmpFeature`, `libEffectCtrlDmp`, `libInitialObject`, `libKeyListener`, `libMediaStoreService`,
`libMediaStoreServiceClient`, `libNcServiceFw`, `libOneTrackPlayerService`, `libPathMgrServiceFw`,
`libPlayerService`, `libRecorderService`, `libScreenshotShared`, `libSoundService`,
`libSoundServiceFw`, `libTunerPlayerService`, `libeaselqt`, and `bin/HgrmMediaPlayerApp`,
`bin/screenshot`. Everything else under `/vendor/sony/{lib,bin}` is byte-identical, including
every `libaudiohal-*`.

## Tools

| tool | use |
|---|---|
| `tools/re_decompile.sh <binary>...` | Ghidra headless, one project per binary, listing in `artifacts/decomp/<name>.c` (not tracked). `RE_FW=w1` writes `artifacts/decomp_w1/`. |
| `tools/re_fn.sh [-s] <lib> <regex>` | one function with its strings resolved; `-s` prints calls, strings and branches only |
| `tools/re_strings.sh <lib>` | one line per function: the strings it references |
| `tools/re_touchpoints.sh` | regenerates `FIRMWARE_touchpoints.md`: the kernel nodes, devices and properties each binary names |
| `tools/ssfw_cost.sh` | on the player: where the sound service's CPU goes under a stream |
| `tools/re_resolve.py <elf>` | the resolver the two above use: turns Ghidra's `DAT_x + 0xy` into the string, and names static `std::string` objects from their initialisers |
| `artifacts/walkmanone/re/kernel/dis.py <symbol>` | kernel function by name (run from that directory) |

Ghidra loads a zero-based library at `0x10000`, so a listing address is the file address plus
`0x10000`.

## Boot chain and kernel

| component | role | read | notes |
|---|---|---|---|
| preloader, LK | MediaTek boot chain | part | `reference` notes on `wbrt` coverage; `RE_walkmanone_extract.md` (the model swap is another model's LK) |
| kernel 3.10 (MT8590) | | part | `RE_kernel_power.md`, `RE_kernel_idle_levers.md`, `PLAN_deep_idle.md` |
| `cxd3778gf_dnc_core.ko` + built-in `snd_soc_cxd3778gf` | DAC, amp, noise-cancel core | part | `RE_codec_power.md`, `RE_codec_tone_table.md`, `RE_headphone_amp_modes.md`, `RE_volume_pop.md` |
| `radio-si4708icx.ko` | FM tuner | part | `RE_fm_tuner.md`, `PLAN_fm_kernel.md` |
| `icx_usbconn.ko`, `icx_usbscsi.ko`, `icx_carddet.ko`, `icx_nvp_emmc.ko`, `wm_key.ko`, `lcd.ko` | Sony drivers | surface | `RE_usb_after_resume.md` touches `icx_usbconn` |
| `mtk_stp_*`, `mtk_wmt_detect.ko` | combo-chip (BT, Wi-Fi) transport | surface | |
| ramdisk `init*.rc`, `bootswitcher.sh`, `boot_complete.sh` | service start-up | part | `4a_init_system.txt`, `4b_init_flow.txt` |

## The Sony service processes

All are `/system/vendor/sony/bin/hagodaemon <Service>...`, one init service each
(`init.hagoromo.rc`). The service is `lib<Service>.so`. "W1" marks a library Walkman One replaces.

| init | services | runs as | read | notes |
|---|---|---|---|---|
| hagoromo1 | (master, `daemonNum=28`) | system | surface | `F_appmgr_home/` |
| 2 | appmgrservice | system | part | `F_appmgr_home/`; launches the Home app |
| 3 | PowerService | root | part | `RE_power_stock_vs_cinder.md`, `RE_sony_idle_baseline.md` |
| 4 | EventRouter | system | surface | one call, `SendEvent`, fanned out to listeners (`OnEvent`) |
| 5 | TimerService | system + caps | surface | clock, time zone (`/usr/share/zoneinfo`, `/data/share/chrome/localtime`), alarms on `/dev/rtc0`; `SetClock`, `SetAlarmTimer`, `GetNearestAlarm` |
| 6 | WMPortService, KeyService, ConfigurationService (W1), DisplayService | system + caps | surface | the two 10 Hz pollers live here (`docs/NEXT.md` section 5 item 7). WMPortService talks to a dock over `/dev/icx_cradle` and `/dev/ttyMT1` and republishes its keys through `uinput` |
| 7 | UpdateService | system | surface | |
| 8 | UsbHostConnectionService, UsbDeviceConnectionService, UsbDeviceAudioPlayerService | system, nice -10 | part | `E_usbdac_ldac/`, `4f_usb_dac_routing.txt` |
| 9 | MediaStoreService (W1), PlayerService (W1) | system | part | `H_mediastore/`, `G_player_ipc/`, `RE_playerservice_sound.md`, `RE_32bit_flac.md`, `RE_queue_edit.md` |
| 10 | OneTrackPlayerService (W1) | system | surface | a one-file player (`GapPlayerImpl.cc`) on the same media framework as PlayerService: `PlayController` with `SetDataSource`, `Prepare`, `Start`, `Pause`, `Stop`, `Reset`, `SetLooping`, `ResetTrackType`, `GetState`. The stock app names it 21 times; Cinder does not use it |
| 11 | **SoundServiceFw** (W1) | system, nice -15 | part | `RE_sound_service.md` (in progress) |
| 12 | NcServiceFw (W1) | system + caps | surface | noise cancel; inert without Sony headphones |
| 13 | VolumeServiceFw | system | part | `RE_volume_service.md`, `RE_volume_tables.md` |
| 14 | VolumeGlue | system | surface | |
| 15 | RecorderService (W1) | system | surface | recording: `SetUp`, `GetRecordController` (`Start`), `GetMaxAmplitude`, `GetSocketName` (a PCM socket, like the Bluetooth transmitter's), `Suspend`, `Resume`, listener `OnInfo` / `OnError`. The stock app names it 65 times; Cinder does not use it |
| 16 | AudioAnalyzerService | system | part | `RE_playerservice_sound.md` sections 7 and 10 |
| 17 | ConnMgrServiceFw | system | surface | |
| 18 | FuncMgrServiceFw | system | part | the FuncMode gate (USB-DAC) |
| 19 | PathMgrServiceFw (W1) | system | surface | decides the output device and the mixer values for it: `ResolveOutputDevice`, `GetCurrentOutputDevice`, `GetSuitableMediaPlayDevice`, `ResolveMpAmixerValues`, `GetSmasterValue`, `InformDeviceEnabled`, `InformTunerState`, `IsLineOutDisconnectedInSuspending`. It logs `current device is changed [SeHeadphone]` |
| 20 | PowerMgrServiceFw | system + caps | part | `RE_power_stock_vs_cinder.md` |
| 21 | RemoteCmdInjCtrlDaemon | system | surface | no API of its own (`IDontHaveAnyApi`); listens to BtTransmitterService and WMPortService to inject remote-control commands |
| 22 | UsbMgrServiceFw | system + caps | part | `RE_usb_after_resume.md` |
| 23 | StorageMgrServiceFw | root | part | `RE_storagemgr.md` |
| 24 | WiredHpServiceFw | system + caps | surface | headphone detection from kernel uevents (`UEventViewer`, `WiredHpMonitorAdler::OnUEvent`), `GetConnected`, `GetInfo`. Event driven: no poll |
| 25 | NfcService | system | part | `G_bt_nfc/` |
| 26 | MemmonService | root | surface | memory monitor: samples `/proc/meminfo` and per-process memory on an interval into `/var/log/memmon<n>.csv`, `DropCache`, `OnMemoryShort`; idle unless a client calls `Start` (no wakeups measured 2026-10-10) |
| 27 | BtCommonService, BtTransmitterService, BtBleCommonService, BtBleRemoteService, BtPlayerService | system | part | `G_bt_nfc/`, several `reference` notes |
| 28 | AudioInPlayerService, TunerPlayerService (W1) | system, nice -10 | part | `RE_fm_tuner.md` |

## Libraries the services share

| library | role | read | notes |
|---|---|---|---|
| `libpstcore.so` | IPC framework (`pst::`), binder-like transport, service manager | part | `G_player_ipc/`; the listener ABI and pump notes |
| `libaudiohal-adleralsa.so` | the jack output (CXD3778GF over ALSA) | part | `RE_sound_service.md` section 4: it owns a power policy |
| `libaudiohal-dualtrackmixalsa.so` | two-track mixer output | surface | carries its own DSD-to-PCM converter (`LibDsdToPcmConv_*`) and Sony's `Plt*` platform helpers |
| `libaudiohal-a2dpsnksingletrack.so` | Bluetooth transmit output, despite the name | surface | writes PCM to the socket `pst::services::bttransmitterservice` (`RE_sound_service.md` section 4) |
| `libaudiohal-uacalsasingletrack.so` | USB-DAC output | part | `E_usbdac_ldac/` |
| `libaudiohal-analyzer.so`, `-listener`, `-genericalsa` | spectrum tap, error listener, generic ALSA input | surface | |
| `libEffectCtrlDmp.so` (W1), `libSoundServiceSettingsDmp.so` | client side of the sound effects and sound settings | part | `RE_dsp_effects_surface.md` |
| `libDmpFeature.so` (W1) | per-build feature switches | part | `RE_dmp_feature.md` |
| `libDmpConfig.so` (W1) | configuration helpers | surface | |
| `libFuncArchUtils.so` | sample-rate converter (`SRConvHr`), format converters, sysfs and amixer helpers | surface | |
| `libcnpysort.so`, `libMediaStoreServiceClient.so` (W1) | library database client | part | `H_mediastore/` |
| `libeasel*.so` | Sony's Qt application base | part | `F_appmgr_home/` |

## Applications and tools

| component | role | read | notes |
|---|---|---|---|
| `HgrmMediaPlayerApp` (W1) | the stock player, Qt 5 Quick | part | `4d_qt_viewmodel.txt`, `ui_assets/UI_FEATURE_MAP.md` |
| `PaSleepPrimaryAudioDevice`, `PaWakePrimaryAudioDevice` | set the codec's `standby` mixer control to 1 and 0 (`PowerAssist.cc`) | full | nothing in the rootfs or ramdisk calls them; Cinder's codec standby drives the same control (`RE_codec_power.md`) |
| `AdbEnabler`, `AdbDisabler` | set and clear `persist.sys.sony.icx.adb` | surface | |
| `hagomon` | lists the Sony service processes over `/dev/binder` and marks one that is hanging | surface | a ready-made check for a stuck service |
| `hagomach` | malloc checker for a Sony service process (`start`, `stop`, `dump`, `trap`), output in `/tmp/hagomach.<pid>` | surface | |
| `fillheap` | allocates memory until it fails, protected from the OOM killer: a memory-pressure test | surface | |
| `screenshot` (W1) | `/dev/graphics/fb0` to a picture file in `/contents` | surface | |
| `StorageFormatterDmp`, `StorageFormatterSample` | format a volume (exFAT or FAT) | surface | `RE_storagemgr.md` |
| `hdumpstate` | a bug-report dump: dmesg, logcat, mmc state, wake sources | surface | |
| `/system/bin/dacdat` | applies volume, DSD and tone tables to the codec | part | `RE_volume_tables.md`, `RE_codec_tone_table.md` |
| `/system/bin/load_sony_driver`, `nvp*`, `mtkbt`, `kisd`, `dpfd`, `fwpup`, `ffu` | drivers, NVP, Bluetooth stack, key install, updater | surface | `RE_walkmanone_installers.md` for the updater path |
| `/system/xbin/regmon`, `dsf_player`, `wplayer`, `fm_tuner`, `diag_main` | Sony diagnostics | part | `regmon` is the register tool the FM and codec work uses |

## Measured cost while a track plays (2026-10-11, Walkman One, jack, 16-bit FLAC)

Per cent of one core, from `/proc/<pid>/task/*/stat` over 15 to 30 s.

| process | Direct mode | Custom, 10-band EQ + normalizer |
|---|---|---|
| SoundServiceFw | 0.9 | 10.5 |
| MediaStoreService + PlayerService | 1.9 (decode thread 1.1) | same |
| cinder-home, screen off | 0.4 to 1.3 | same |

PlayerService's time is spread thin: no symbol in its decode thread is above 4 % of that thread,
and Walkman One's copy is stripped. Nothing there to patch. The lever is the effect chain.

## Reading order from here

1. `libSoundServiceFw.so`, Walkman One copy (in progress).
3. `libPlayerService.so`: the decoder side that feeds the sound service.
4. `libPathMgrServiceFw.so`, `libWiredHpServiceFw.so`, `libNcServiceFw.so`: output routing.
5. Every service now has at least a surface read. Deeper reads wait for a question that needs one.
