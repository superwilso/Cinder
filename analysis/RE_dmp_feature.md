# `libDmpFeature.so`: the per-build feature switches

Read 2026-10-11 from disassembly of both copies (`FIRMWARE_MAP.md` for the two trees). Every other
Sony library asks this one what the model can do. Most answers are constants compiled into the
build, which is the whole difference between the stock A50 library and the one Walkman One ships.

## Constants

| `DmpFeature::` | stock A50 1.02 | Walkman One | what it gates |
|---|---|---|---|
| `IsFeaturedNoiseCancel` | 1 | 0 | noise cancelling (NcService, the NC screens) |
| `IsFeaturedSourceDirect` | 0 | 1 | Direct Source |
| `IsFeaturedClearAudioPlus` | 1 | 0 | ClearAudio+ |
| `IsFeaturedSelectUsingEq` | 0 | 1 | choice between the 10-band EQ and Tone Control |
| `GetFeatureFixedUsingEq` | 1 | -1 | the EQ type when there is no choice |
| `IsFeaturedDsdProcNative` | 0 | 1 | native DSD on the built-in output |
| `IsFeaturedDsd256ProcNative` | 0 | 1 | native DSD256 |
| `IsFeaturedUsbdacOutDsdNative` | 1 | 1 | native DSD over USB output |
| `IsFeaturedLpcmDsdRemaster` | 0 | 0 | DSD Remastering |
| `IsFeaturedWmportOutAnalog` | 1 | 0 | analogue out on the WM-PORT |
| `GetNumOfLpcmMaxPlayableFs` | 8 | 8 | highest PCM rate accepted, in multiples of 44.1/48 kHz |
| `GetNumOfLpcmMaxNativePlayableFs` | 4 | 8 | highest PCM rate played without down-conversion |
| `GetNumOfExternalStorage` | 1 | 1 | SD slots |
| `IsFeaturedVolAvls` | 1 | 1 | AVLS |

So Walkman One's feature set is a WM1-class profile: no noise cancelling, Direct Source, native
DSD to 256, PCM native to 384 kHz, the 10-band EQ.

**Battery consequence.** Native DSD goes out on `hw:0,2`, where the output library switches deep
idle off for the whole stream and raises the CPU floor to 1040 MHz
(`RE_sound_service.md` section 4). On the stock A50 build DSD is converted to PCM and takes the
low-power output. A DSD library costs more battery on Walkman One by design.

## Switches read at run time

| `DmpFeature::` | source | rule |
|---|---|---|
| `IsFeaturedSeHpDsdNative` | `persist.sys.sony.icx.sehp.dsd` | true when the property is set |
| `IsFeaturedAnalyzerTuning` | `persist.sys.sony.icx.anls.tune` | true when the property is set |
| `IsFeaturedMqaRender` | `persist.sys.sony.icx.mqarender` | false when the property is set (rendering is on by default) |
| `IsModel(Model)` | the model name in NVP | one of `NW-WM1A`, `NW-WM1Z`, `NW-A30Series`, `NW-ZX300`, `NW-ZX300A`, `NW-A40Series`, `NW-A50Series`, `DMP-Z1`; 8 is "invalid" |
| `IsShip(Ship)` | the destination in NVP | 8 is "invalid"; selects the functions below |
| `IsFeaturedVolAlert`, `IsFeaturedVolStrategyEu`, `IsFeaturedBtlGain2Step`, `IsFeaturedAtrac`, `IsFeaturedVideo` | `IsShip` | by destination |
| `GetFeatureVolume…Init`, `…AvlsThresh`, `…VolAlertThresh` | `IsShip` | by destination; the defaults seen are 68 (normal gain), 58 (single-ended high gain), 46 (balanced high gain) |
| `GetFeatureVolAlertIntervalShortageInMin` | `ConfigurationService` | a stored integer |
| `IsBundledHp(Hp)` | NVP | which headphones shipped in the box; 4 is "invalid" |
| `GetFeatureUsbVidMsc/Uac`, `GetFeatureUsbPidMsc/Uac` | model | VID `054C`; the PID by model |
| `GetDseeHxCustomHpDcfgFilename` | model | the DSEE HX configuration file for the bundled headphones |

The PID strings in the library, in table order: `0BC0`, `0BC1`, `0BBF`, `0C70`, `0C71`, `0CA0`,
`0CA1`, `0B8C`, `0B8D`. The A50 pair is `0B8D` (storage) and `0B8C` (USB audio), as seen on the
player. Which of the others belongs to which model is not read, nor which destination value is
which region.
