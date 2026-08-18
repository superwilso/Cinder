# FM tuner — the full callable surface

Recovered 2026-08-17 from `vendor/sony/lib/libTunerPlayerService.so`. Nothing here has been called
yet; this is the map that makes #66 an implementation task rather than a research one.

**Why it matters:** `player/cinder-ui/src/fm.rs` is an 81-line placeholder that draws a hardcoded
88.6 MHz, and there are **zero** tuner calls anywhere in the shell. The code says so itself —
`// tuner not wired — claim nothing`, and the menu row carries an empty subtitle for the same
reason. So "we never tested FM" is not an oversight: there has never been anything to test.

## The hardware

`TunerPlayerImplSi4708.cc` in the library's `.rodata` names the part: a **Silicon Labs Si4708**
FM receiver. `/bin/load_sony_driver` has an FM branch gated on the destination code held in NVP
(`nvpflag -x shp`), and this unit is destination **E** (Europe), which has the tuner fitted.

## Getting a client

Same shape as every other Sony service Cinder already drives:

```
_ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv   @ 0xf338
_ZN3pst8services25TunerPlayerServiceFactory14CreateInstanceEv         @ 0xa70c
```

Exported, so a replacement player can instantiate the client from outside. The service itself is
hosted in `hagoromo28` alongside `AudioInPlayerService`.

## The client vtable — RECOVERED

`_ZTVN3pst8services24TunerPlayerServiceClientE` @ `0x1789c`. The slots are filled by `R_ARM_ABS32`
relocations rather than stored in the file, so `objdump -s` shows zeros — read them with
`readelf -rW --demangle` and map the relocation offsets back to slot indices instead.

| slot | method |
|---|---|
| 0, 1 | `~TunerPlayerServiceClient()` |
| 2 | `GetServiceName() const` |
| 3 | `GetTunerState()` |
| 4 | `Open()` |
| 5 | `Close()` |
| 6 | `Play()` |
| 7 | `Stop()` |
| 8 | `GetMuteMode(MuteMode&)` |
| 9 | `SetMuteMode(const MuteMode&)` |
| 10 | `GetStereoMode(StereoMode&)` |
| 11 | `SetStereoMode(const StereoMode&)` |
| 12 | `GetStereoState(StereoMode&)` |
| 13 | `GetBandwidth(uint&, uint&, uint&)` |
| 14 | `GetSoftBandwidth(uint&, uint&, uint&)` |
| 15 | `SetSoftBandwidth(const uint&, const uint&, const uint&)` |
| 16 | `GetFrequency(uint&)` |
| 17 | `SetFrequency(const uint&)` |
| 18 | `GetSenseMode(SenseMode&)` |
| 19 | `SetSenseMode(const SenseMode&)` |
| 20 | `GetSignalLevel(SignalLevel&)` |
| 21 | `StartAutoTuning(const uint&, const bool&, const uint&)` |
| 22 | `StopAutoTuning()` |
| 23 | `IsRunningAutoTuning()` |
| 24 | `SetDeviceSettings(const DeviceSettings&)` |
| 25 | `SetSoundSettings(const SoundSettings&)` |
| 26 | `AddListener(IServiceListener*, const std::string&)` |
| 27 | `RemoveListener(IServiceListener*)` |
| 28 | `GetName() const` |
| 31–33 | non-virtual thunks (second base) |

`AddListener`/`RemoveListener` sit immediately before `GetName`, matching the pattern already
established for `BtCommonServiceClient` — see `reference_pst_listener_abi`.

## The listener

`TunerPlayerServiceListenerProxy` exists with the usual `…Base` dispatch helpers, and the client
registers through `ServiceClientBase::AddListenerBase<ITunerPlayerService::IServiceListener,
TunerPlayerServiceListenerProxy>`. Callbacks:

| callback | carries |
|---|---|
| `OnChangedTunerDeviceState` | tuner powered/ready |
| `OnChangedSignalLevel` | live RSSI — drives a signal meter |
| `OnChangedStereoState` | mono/stereo lock indicator |
| `OnChangedAutoTuningInfo` | seek progress and result |
| `OnReceivedPs` | **RDS Programme Service name** — the station's text ID |

Pass a RAW pointer with the right vtable; the client builds the proxy itself.

## What is NOT yet known

* **Enum values.** `MuteMode`, `StereoMode`, `SenseMode`, `SignalLevel` and the `TunerState`
  return are all unrecovered. Same situation as `VptMode` was, and the same rule applies: an
  echoed read-back does NOT bound an enum on this device. `GetStereoState`/`GetSignalLevel` are
  genuine reads of hardware, though, so those two can be settled by tuning to a known-strong
  station and a known-dead frequency and comparing.
* **`StartAutoTuning(const uint&, const bool&, const uint&)`** — three arguments, presumably start
  frequency, direction, and step or threshold. Needs a probe.
* **`DeviceSettings` / `SoundSettings` layouts.** Both take a struct by const reference; neither is
  needed for basic tune-and-listen.
* **Where the audio goes.** The tuner is a separate source; whether it lands on the same ALSA path
  as file playback or needs `SoundServiceFw::CreateTrack` with a different `TrackType` is unknown.
  This is the one that decides how much work wiring it really is.

## FIRST CONTACT — `cinder-probe --fm`, 2026-08-17

The client constructs and the whole lifecycle works on the first try:

```
fm: client 0xb8ed2870
fm: GetTunerState (before Open) = 0
fm: Open() rc=0
fm: after Open state=1  GetFrequency=90200  signal=1
fm: Play() rc=0
fm: Stop()   fm: Close() rc=0
```

**Settled:**

* **`TunerState` is 0 = closed, 1 = open.** Read before and after `Open()`; it moves, so it is a
  real state and not an echo.
* **`SetFrequency` takes kHz**, and — unusually for this device — the range is **VALIDATED**:

  ```
  SetFrequency 98000     -> reads 98000     (accepted)
  SetFrequency 9800      -> reads 98000     (REJECTED, previous value kept)
  SetFrequency 98000000  -> reads 98000     (REJECTED, previous value kept)
  ```

  That is the first setter found on this hardware that refuses an out-of-range value instead of
  storing it. Compare `VptMode`, `SelectUsingEq` and `VinylizerType`, all of which happily kept
  whatever integer they were handed.
* **The device already held 90200** — 90.2 MHz, a real station frequency, left by the stock player.
* `Open`, `Play`, `Stop`, `Close` all return 0.

**Not settled: `GetSignalLevel` reads a constant 1 across the entire 87.5–108.0 MHz sweep**, with
and without `Play()`. Three candidate explanations, now down to one: the unit is confirmed kHz, and
`Play()` made no difference — so it is almost certainly **no aerial**. On this hardware the
headphone cable IS the FM antenna, and the jack was empty for both runs.

**Re-run with headphones plugged in** and the sweep becomes a real station scan. That single run
would also settle `StereoMode` (compare a strong local station against a dead frequency) and give
`SignalLevel` a scale.

## AUDIO CONFIRMED — measured, 2026-08-18

`GetSignalLevel` turned out to be useless for this: with the aerial in, **203 of 206 frequencies
across 87.5–108.0 MHz return 1**. It is not an RSSI scale, so it cannot drive a scan — Sony's own
UI must seek with `StartAutoTuning` rather than by reading levels.

**The tuner audio does NOT pass through ALSA.** Holding a station and diffing the PCM table against
a baseline: no new PCM opened, only the music already on `pcm4p`, and it was still there after the
tuner closed. The Si4708 feeds the CXD3778GF's analogue path directly. That answers the "where does
the audio go" question above — and it means no amount of software introspection can confirm FM
audio. The only instrument is the analogue output itself.

So it was measured that way, by recording the headphone jack on a PC (`tools/measure_output.py`,
Windows ffmpeg backend), with a control to separate signal from the music's own variation:

| capture pair | RMS delta |
|---|---|
| music → music (control, same interval, nothing changed) | **-0.47 dB** |
| music → music + FM tuned to 90.2 MHz | **+7.43 dB** |

Music-to-music RMS is stable to half a dB; the FM hold adds 7.4 dB. **FM audio is present at the
output.** (Per-BAND deltas are worthless in this test — the control swings up to ±16 dB per band
because the programme material moves. Only RMS is stable enough to compare.)

**And FM MIXES with playback rather than replacing it.** The music kept running on `pcm4p`
throughout and the levels added. Whatever wires the FM screen has to stop playback itself on entry;
`Play()` on the tuner is not exclusive.

## FM OVER BLUETOOTH — possible, and the machinery already exists

Asked 2026-08-18: with a cable in (needed as the aerial), can FM go out over Bluetooth?

The obstacle looked fatal — FM audio never becomes PCM in the SoC, which is why no ALSA device
opens when the tuner plays. A2DP needs PCM to encode. But the codec can digitise it:

```
numid=26  'analog input device'  ENUMERATED, 5 items:
    0 'off'    1 'tuner'    2 'mic'    3 'line'    4 'directmic'
                 ^^^^^^^
/proc/asound/card0/pcm1c   stream: CAPTURE   id: cxd3778gf-standard DAI_CXD3778GF_STD-1
```

Verified on device: setting `numid=26` to **1 (tuner)** takes and reads back, and `capture mute`
(numid=15) is already off. So the chain is:

```
Si4708 --analogue--> CXD3778GF ADC --> hw:0,1 (capture) --> LDAC/SBC encode --> BtTransmitterService
```

**Cinder already has every piece of the right-hand side.** The USB-DAC to LDAC bridge captures a
PCM device and pushes frames to `BtTransmitterService`; this is the same bridge with a different
source (`hw:0,1` instead of the UAC gadget's capture). Reuse it — do NOT write a second PCM path:
`reference_bt_transmitter_socket` records that writing PCM to that fd before the type-1 handshake
REBOOTS the device, measured twice.

**The aerial and the output are independent**, which is what makes the question worth answering:
the cable only has to be plugged in, not listened to. Cable in the jack for reception, audio to the
headphones over LDAC.

**Not proven end to end.** Nobody has yet captured a frame from `hw:0,1` with the tuner routed in.
The next step is exactly that — open it, confirm non-silent PCM, and only then wire it to the
bridge. Expect the UAC capture lessons to apply (`reference_uac_capture_start`: the device needs an
explicit `snd_pcm_start` and must not be opened before the source is live).

## FM WORKS — confirmed by ear 2026-08-18, 97.3 MHz

The scanner picked 97.3 with no human input (hf 0.087 vs a 0.451 baseline); holding it produced
audible, listenable radio. Whole chain proven: aerial -> Si4708 -> codec ADC -> hw:0,1 -> AudioIn
track -> headphones.

## THE AUDIO PATH — SOLVED (2026-08-18)

Tuning the chip makes no sound on its own. `hagoromo28` hosts two services and both are needed:
`TunerPlayerService` (the Si4708) and **`AudioInPlayerService`**, whose internals give the design
away — `AudioInPlayerInhal::{SetupAlsa,Open,StartRead,Read}` and
`AudioInPlayerExhal::CreateAudioTrack()`, with exactly one device literal in the library: **`hw:0,1`**.

So Sony's FM path is: **Si4708 → codec ADC → `hw:0,1` capture → CreateAudioTrack → output.**
FM DOES become PCM. (An earlier note here said it never does — that was true only of the
tuner-alone path.)

**THE ORDER MATTERS, and this is the whole trick:**

```
1. amixer -c0 cset numid=26 1      # 'analog input device' = tuner  <-- FIRST
2. Tuner:   Open() -> SetFrequency(kHz) -> Play()
3. AudioIn: Play()                 # rc=0, PlayerState 0 -> 2
```

With step 1 missing, `AudioInPlayerService::Play()` returns **rc=1** and the audio path never
opens. With it, **rc=0 and the state moves 0 → 2**, and the measured output rises from a flat
**-59.8 dBFS to -46.5 dBFS**. The ADC must have a source selected before the capture side starts.

`Play()` with no argument builds an EMPTY `std::string` (disassembled at `0xabf8`) and hands it to
`Play(const std::string&)`, so the no-arg form is `Play("")` — and it works once the route is set.
The names both `libAudioInPlayerService` and `libSoundServiceFw` carry are `music`, `beep`, `hdmi`,
`hfp`, `mic`, `mrmcloop`. **`"tuner"` is not one of them** — that was a guess, and see the hazard
note below for what it cost.

## STILL NO STATION — and the evidence says aerial, not software

With the path open and working, there is still no reception:

* audio at a known station frequency vs a dead one: **-47.65 dBFS / 7.04 dB variance** versus
  **-46.55 dBFS / 7.18 dB variance** — indistinguishable
* `GetSignalLevel` reads 1 at 203 of 206 frequencies
* **Sony's own `StartAutoTuning` finds nothing**: returns within 100 ms with
  `IsRunningAutoTuning()==0` and the frequency back at the start value, seeking both up and down
* `SetSenseMode` accepts 0/1/2 and reads back, so sensitivity is not stuck

The antenna switch reads 1, but that only means **something is in the jack** — it does not mean the
aerial works. The far end of the cable was plugged into a PC recording input for the whole session,
which presents a near-short to ground and would kill FM reception while still asserting the switch.

**The next step is physical, not software: unplug the cable's far end from the PC, leave it hanging
from the Walkman, and re-run `cinder-probe --fm seek 87500`.** If it lands on a real frequency, the
tuner works and only the UI is left. If it still finds nothing with a free aerial, the problem is
reception, not Cinder.

The probe is now self-contained — it sets `numid=26` itself, reports the antenna switch, and
restores the mixer on exit.

## ⚠️ HAZARD — `AudioInPlayerService::Play(string)` REBOOTED THE DEVICE (2026-08-18)

**Do not repeat this without a safety plan.** It cost a manual recovery from stock.

Tuning the chip is not enough to hear anything. `hagoromo28` hosts BOTH `TunerPlayerService` (the
Si4708) and **`AudioInPlayerService`** (the audio path for analogue sources), and with only the
tuner played the output sat at a flat **-59.8 dBFS** at every frequency — amp noise, no radio.

`AudioInPlayerServiceClient` (factory `…33AudioInPlayerServiceClientFactory14CreateInstanceEv`
@ 0xdd30, vtable @ 0x13884):

| slot | method | result |
|---|---|---|
| 3 | `Play()` | **rc=1 — rejected** |
| 4 | `Play(const std::string&)` | **rc=0 — accepted** with `"tuner"` |
| 5 | `Stop()` | — |
| 6 | `GetPlayerState()` | stayed `0` throughout |

Calling slot 4 with `"tuner"` moved the measured output from **-59.8 to -48.7 dBFS (+11 dB)**, so
something in the audio path genuinely opened. But immediately afterwards the TUNER client's own
getters started returning **-1** (`signal=-1 stereoState=-1`) — the tuner client was already
broken — and shortly after that **the device rebooted and came up on stock firmware** (the launcher
bad-boot counter, rung 1, doing its job).

**What is NOT yet known**, and what makes this dangerous to retry as-is:

* whether `"tuner"` is even the right string — it was a guess, and a wrong argument here is exactly
  the shape of the `pst` faults recorded in `reference_pst_containers`
* whether `GetPlayerState()` staying 0 means the play never really started
* whether the fault was the call, the interaction with the still-open tuner client, or teardown
  (the probe called `AudioIn::Stop()` then `Tuner::Stop/Close` then `_exit`)

**Before touching it again:** find the string Sony passes. `HgrmMediaPlayerApp` and
`libAudioInPlayerService.so` should contain the literal — recover it by RE rather than by guessing
on hardware, because the failure mode here is a reboot into stock and a hand recovery, not an error
code.

## SETTLED — `StartAutoTuning` IS A STUB. The chip's seek is not implemented.

`TunerPlayerImpl::StartAutoTuning` @0xcd14 is 48 bytes, and all of it is the stack-protector
epilogue:

```
cd1a-cd22   load + stash the stack canary
cd24-cd2e   reload, compare
cd30-cd36   ittt eq / moveq r0,#4 / pop     <- normal path: RETURN 4
cd38        blx __stack_chk_fail            <- canary mismatch
```

**The three arguments are never read and nothing else is called.** It unconditionally returns
Result 4 (evidently "not supported"). Registering a listener (which succeeds, rc=0) changes
nothing, because there is no seek to report on.

So this is not a calling-convention problem and never was. Sony did not implement the Si4708's
search hardware on this model — which also explains `GetSignalLevel` returning a constant 1 across
the band: the tuning interface is exposed, the SEARCH interface is not.

**Consequence: the audio-measuring scanner is the only mechanism available**, not a workaround for
a mis-call. Sony's own UI cannot scan faster on this hardware either. ~140 ms per 100 kHz step is
close to the floor: retune, settle, capture enough samples to compute the ratio.

(The earlier note here speculated the opposite — that the async/listener shape meant the call had
never been given a fair test. Registering a listener was worth doing and settled it in the other
direction.)

## HOW FAR DOWN CAN WE REACH? All three userspace layers, measured

Every layer between us and the Si4708 drops a feature. This is the complete picture as of
2026-08-18, and it is why the audio-measuring scanner still exists.

| layer | tune | signal | seek |
|---|---|---|---|
| `TunerPlayerService` | yes | **constant 1** everywhere | **stub** — 48 bytes, returns 4, args unread |
| kernel `Si4708icx` via `/dev/radio0` | yes | **binary** 0 or 65535, nothing between | `HW_FREQ_SEEK` cap bit CLEAR, ioctl ENOTTY |
| direct I2C via `/dev/i2c-2` | — | — | **no transfer works at all** |

**Direct I2C is closed.** `/dev/i2c-2` opens; `I2C_SLAVE 0x10` returns EBUSY (the driver is bound)
and `I2C_SLAVE_FORCE` takes it — but then every read fails:

```
read(32)                 -> EINVAL   (MTK does not implement the simple file ops)
I2C_RDWR 32 / 16 / 8 B   -> EINVAL   (all three)
```

So MediaTek's adapter refuses userspace transfers to this device. The chip has a proper 8-bit RSSI
in `STATUSRSSI` and SEEK bits in `POWERCFG`; we simply cannot reach them from userspace.

**The only remaining route to real RSSI and hardware seek is KERNEL CODE** — a module (or a patch
to `Si4708icx`) built against 3.10.26-mt8590. That is a real project with real brick risk: it
touches a bus a bound driver owns, and a bad module at boot is the failure mode the launcher's
escape ladder exists for. Not something to attempt casually.

**What the V4L2 meter is still good for.** It is binary, but it is INSTANT and it discriminates
perfectly on a strong carrier (65535 vs 0, repeatable). Across the full band it agrees with the
audio scanner on the strong stations and flickers on marginal ones (97.3 present in one sweep,
absent the next; 91.3 and 106.2 likewise). So it is a good FIRST PASS and a bad final answer:
sweep with V4L2 to get candidates in ~25 s, then confirm the flickering ones by audio.

**The aerial dominates everything.** Three separate "the meter is broken" conclusions during this
session were all one cause: nothing in the headphone jack. `signal` reads 0 at every frequency with
no aerial, exactly as it would for a dead band. ALWAYS check
`/sys/class/switch/cxd3778gf_antenna/state` before believing any tuner measurement.

## ✅ /dev/radio0 GIVES AN INSTANT (BINARY) SIGNAL METER (2026-08-18)

Sony's search interface is a stub, but the kernel driver underneath is not. `cinder-probe --fm v4l2`:

```
driver='si4708 icx dirv'  card='Si4708icx'  caps=0x00050000
TUNER=1 RADIO=1 HW_FREQ_SEEK=0 RDS=0
tuner 'FM' cap=0x411 CAP_LOW=1 range 76.0..108.0 MHz

STATION  97.3 MHz -> reads 97.3  signal=65535/65535  stereo=1
DEAD    104.3 MHz -> reads 104.3 signal=    0/65535  stereo=1
STATION  97.3 MHz -> reads 97.3  signal=65535/65535  stereo=1
HW_FREQ_SEEK up rc=-1 (Inappropriate ioctl for device)
```

**`VIDIOC_G_TUNER` is a REAL meter** — full scale on a carrier, zero on a dead frequency, read
instantly. That is exactly what `TunerPlayerService::GetSignalLevel` refused to provide (constant 1
everywhere), and it makes the audio-measuring scanner obsolete.

**`VIDIOC_S_HW_FREQ_SEEK` is NOT implemented** (the capability bit is clear and the ioctl returns
ENOTTY), so the chip's hardware seek is unavailable through this driver too. It no longer matters:
a scan becomes tune → settle → read a register, with no audio capture at all. Expect ~30-50 ms per
step against the 140 ms the audio scanner needs, and no need to own the capture PCM — which also
means **a scan no longer has to stop the radio**.

Units: `CAP_LOW` is SET, so the `v4l2_frequency.frequency` field is in 1/16 kHz (62.5 Hz) steps.
Range reads 76.0-108.0 MHz (the Japanese band edge, wider than the European 87.5).

`stereo=1` was reported at BOTH frequencies, so `rxsubchans` looks unreliable here — do not build a
stereo indicator on it without checking further.

### What to build next

Replace the scanner's metric: keep the sweep structure in `cinder_tuner_scan`, drop the ALSA
capture entirely, and score each step with `VIDIOC_G_TUNER.signal`. The live seek in
`cinder_tuner_seek` should use the same. Both then stop competing with `AudioInPlayerService` for
`hw:0,1`, which removes the stop-audio-to-scan constraint.

## (superseded) The chip is reachable directly — `/dev/radio0`

Sony's `StartAutoTuning` is a stub, but the chip underneath is not hidden:

```
/dev/radio0                     crw-rw-rw-  system system  81,1     "Silicon Labs. FM Tuner"
/sys/bus/i2c/drivers/Si4708icx  bound at i2c 2-0010
/dev/i2c-2                      crw-rw----  system system
```

`/dev/radio0` is a **V4L2 radio device**, and it is world-writable. The standard V4L2 radio ioctls
are exactly the two things Sony did not expose:

| ioctl | gives |
|---|---|
| `VIDIOC_G_TUNER` | `v4l2_tuner.signal` — a REAL RSSI (0..65535), and `rxsubchans` for stereo lock |
| `VIDIOC_S_HW_FREQ_SEEK` | the chip's **hardware seek** — what StartAutoTuning was stubbed out of |
| `VIDIOC_S_FREQUENCY` / `G_FREQUENCY` | tune (units per `v4l2_tuner.capability & V4L2_TUNER_CAP_LOW`) |

If `Si4708icx` implements `s_hw_freq_seek`, an INSTANT scanner and a real signal meter are both
available, bypassing TunerPlayerService for SEARCH while still using it (plus AudioInPlayerService)
for AUDIO. The Si470x family has SEEK/SEEKUP/SKMODE in POWERCFG, a seek threshold in SYSCONFIG2 and
STC/SF/RSSI in STATUSRSSI, so the hardware certainly supports it — the only question is how much of
that the driver wired up.

**First step is cheap and read-only:** open `/dev/radio0`, `VIDIOC_QUERYCAP`, then `VIDIOC_G_TUNER`
at a known station and at a dead frequency. If `signal` differs, the meter is real and the
audio-measuring scanner can be retired. `/dev/i2c-2` is the fallback if the driver turns out to be
tune-only — the Si4708 register map is public.

Note the ownership question this raises: TunerPlayerService almost certainly holds the V4L2 device
while it is playing, so a search may have to happen with the tuner stopped, exactly as the audio
scanner already does.

## THE SCANNER — `cinder-probe --fm scan <start_kHz> [end_kHz]`

Neither Sony primitive can find a station on this hardware, and both were checked against a station
the user could hear:

* `GetSignalLevel` returns **1 at every frequency** with the aerial free (203 of 206 even before).
* `StartAutoTuning` returns inside **100 ms** with `IsRunningAutoTuning()==0`, frequency back at the
  start value, seeking up AND down.

So the scan measures the AUDIO, captured on-device from `hw:0,1` (the codec ADC with
`analog input device` = tuner). The discriminator is spectral, not level — hiss is often LOUDER
than a locked carrier, so ranking by loudness finds nothing:

```
hf = mean(|x[n] - x[n-1]|) / mean(|x[n]|)      first-difference high-pass proxy
```

White noise sits near 1.4; programme material sits far below. **Low hf = station.**

MEASURED 2026-08-18, full band, median hf **0.451** (the no-station baseline):

| frequency | hf | level |
|---|---|---|
| **97.3** | 0.087 | -9.6 dBFS |
| 97.4 | 0.117 | -23.8 |
| 107.9 | 0.170 | -25.3 |
| 100.9 | 0.180 | -26.0 |
| 100.0 | 0.190 | -14.0 |
| 107.8 | 0.198 | -11.6 |

Carriers **cluster across adjacent 100 kHz steps** (97.2/97.3/97.4, 100.0/100.1, 107.7/107.8/107.9),
which is what a real transmitter looks like at 100 kHz resolution and is good evidence the metric is
measuring reception rather than noise. In 88.0-89.5 only **89.0** beats baseline, and weakly
(hf 0.378 vs 0.472, -31 dBFS) — a fringe signal, not the strong ones at 97/100/108.

**AudioInPlayerService must not be playing during a scan** — it owns `hw:0,1` and the open fails
with EBUSY. Scan, then play.

`snd_pcm_start` and `snd_pcm_drop` had to be added to `ldac-bridge/include/alsa/asoundlib.h`: the
capture PCM does not start on its own (the UAC lesson), and the buffer must be dropped between
retunes or each frequency is scored on the previous one's audio.

## Suggested order

1. `cinder-probe --fm`: construct the client, `Open()`, `GetTunerState()`, `SetFrequency()` a
   local station, `Play()`, read `GetSignalLevel` and `GetStereoState`. That answers "does audio
   come out" and settles the two readable enums in one session.
2. Only then decide whether to build the screen or delete it. A screen that draws a fake frequency
   is the same class of lie stripped out of Sound and Advanced twice.

## THE CHIP IS UNLOCKED — `/proc/regmon/Si4708icx`, 2026-08-18

`PLAN_fm_kernel.md` argued that real RSSI and hardware seek needed a kernel module, because
"the registers cannot be reached from userspace at all". **That was wrong.** Sony's driver calls
`regmon_add` (visible in its UND symbols), and the kernel exposes a generic register-monitor at
`/proc/regmon/` with one directory per registered device:

```
/proc/regmon/{Si4708icx, afe_reg, bq24262, cxd3778gf, mt6323}
/proc/regmon/Si4708icx/{target, value}      -rw------- root root
```

`target` **reads** as the register-name table and **writes** to select a register; `value` reads and
writes that register over I2C. The whole Si470x map is there:

```
0x00 DEVICEID  0x02 POWERCFG   0x04 SYSCONFIG1  0x06 SYSCONFIG3  0x08 TEST2       0x0A STATUS_RSSI  0x0C-0x0F RDSA-RDSD
0x01 CHIPID    0x03 CHANNEL    0x05 SYSCONFIG2  0x07 TEST1       0x09 BOOTCONFIG  0x0B READCHAN
```

Root-only, and cinder-home is uid 100 — so shipping this needs a setuid helper, same shape as
`cinder-clock` / `cinder-gpunode`.

### Stock register state (device idle, tuned to 97.3)

| reg | value | decode |
|---|---|---|
| `0x00 DEVICEID` | `0x1242` | Silicon Labs, MFGID 0x242 |
| `0x01 CHIPID` | `0x1093` | REV 4, firmware 19 (powered) |
| `0x02 POWERCFG` | `0x4001` | `ENABLE=1`, `DMUTE=1`, no SEEK |
| `0x03 CHANNEL` | `0x01AA` | chan 426 |
| `0x04 SYSCONFIG1` | `0x08AA` | `DE=1` (50 µs, Europe ✓), **`RDS=0` — disabled** |
| `0x05 SYSCONFIG2` | `0x126F` | `SEEKTH=0x12`(18), `BAND=01`(76–108), `SPACE=10`(50 kHz), `VOL=0xF` |
| `0x06 SYSCONFIG3` | `0x1000` | **`SKSNR=0`, `SKCNT=0` — seek quality gates disabled** |
| `0x0A STATUS_RSSI` | `0x000B` | `RSSI=11`, `ST=0`, `STC=0` |
| `0x0B READCHAN` | `0x01AA` | 426 |

`76 MHz + 426 × 50 kHz = 97.3 MHz` — the station this doc confirmed by ear. Live reads, not cache:
RSSI wanders 9–14 between reads at the same frequency.

### Writes work — tune + `STC` proven

```
write CHANNEL = 0x811C     (TUNE=1, chan 284)
  STATUS  0x5007  -> STC=1
  READCHAN 0x011C -> chan 284 = 76000 + 284*50 = 90.2 MHz   ✓ landed
write CHANNEL = 0x011C     (clear TUNE)
  STATUS  0x1007  -> STC=0   ✓ handshake completes on demand
```

**`STC` settles within one extra register read.** The 90 ms-per-step settle tax that every
Cinder scan has paid does not exist here.

### Real RSSI — the full band, 206 channels

Sweep = write `CHANNEL|TUNE`, poll `STC`, read `STATUS_RSSI`. 35 s for the whole band, and that is
*entirely* shell-spawn overhead (2 `echo` + `cat` per register op); the chip is ready immediately.

Noise floor 5–6, with genuine 3-channel-wide carriers on top:

| MHz | RSSI |
|---|---|
| 98.2 / 98.3 / 98.4 | **14** |
| 107.7 / 107.8 / 107.9 | 11 / 12 / 12 |
| 106.1 / 106.2 / 106.3 | 10 / 11 / 11 |
| 90.9 / 91.0 | 11 / 11 |
| 105.3 / 105.4 / 105.5 | 9 / 10 / 10 |
| 92.1 – 92.8 | 10 |
| 97.3 / 97.4 | 9 / 9 |

This is the graded meter that `GetSignalLevel` (constant `1` at 203 of 206 frequencies) and the
V4L2 `signal` field (binary `0`/`65535`) both failed to give.

### Hardware seek works

`POWERCFG` `SEEK[8]` / `SEEKUP[9]` / `SKMODE[10]`, poll `STC`, read `READCHAN`, clear `SEEK`.

**Why it appeared broken before:** stock `SEEKTH` is **18**, and no station here exceeds **14**.
The threshold sits above the entire band, so seek walks to the band limit and raises `SF/BL`.
That — not a missing capability — is why Sony's `StartAutoTuning` finds nothing.

Only the top byte of `SYSCONFIG2` needs touching, so `BAND`/`SPACE` stay as the driver set them
and **the driver's frequency mapping never desyncs**:

| settings | result | time |
|---|---|---|
| `SEEKTH=8`, `SKSNR=4`, `SKCNT=8` (AN230 "recommended") | 98.3, 106.2 | 7 s |
| `SEEKTH=6`, `SKSNR=1`, `SKCNT=1` (permissive) | 97.3, 98.3, 105.4, 105.45, 106.2 | 13 s |

Again, seconds are shell overhead — the chip's own seek is ~30–100 status polls per station.

### RDS is absent in HARDWARE — do not chase it

`SYSCONFIG1` bit 12 sets and reads back (`0x18AA`), but `RDSS` never syncs and `RDSA`–`RDSD` stay
zero after 300 polls at both 98.3 (RSSI 14) and 106.2. **The Si4708 is the non-RDS member of the
family — Si4709 is the one with RDS.** `TunerPlayerServiceListener::OnReceivedPs` exists because
the service is shared across models, not because this part can deliver it. Don't re-investigate.

### The design this implies

Split by what each side is actually good at, and nothing desyncs:

* **Tune / audio path** — keep using `TunerPlayerService` + `AudioInPlayerService` exactly as the
  section above establishes (`numid=26` first, then Tuner `Open`/`SetFrequency`/`Play`, then
  AudioIn `Play()`). Sony owns the power sequence and the ADC route; don't fight it.
* **Signal meter, scan, seek** — `/proc/regmon/Si4708icx` via a setuid helper. Read `STATUS_RSSI`
  for the meter, `SEEK` for seek, `READCHAN` for where it landed — then hand the resulting
  frequency back to `SetFrequency` so the service and the chip agree.
* Write only `POWERCFG` (SEEK bits) and the **top byte** of `SYSCONFIG2` (`SEEKTH`), plus
  `SYSCONFIG3` quality gates. Never `BAND`/`SPACE` — those define the chip↔driver channel mapping.
  Never `TEST1`/`BOOTCONFIG`.

Restore-on-exit values for a probe: `POWERCFG=0x4001`, `SYSCONFIG1=0x08AA`, `SYSCONFIG2=0x126F`,
`SYSCONFIG3=0x1000`. Verified: after all of the above the driver is `Live`, refcount intact,
`/dev/radio0` present, no I2C errors in `dmesg`.

**Zero kernel code. Zero boot-path risk. The escape ladder is untouched.**

## AS BUILT — what shipped, and the three things the hardware corrected

Wired into Cinder 2026-08-18: `cinder-fm` (setuid, chmods the two regmon nodes), a register layer
in `cinder-audio/src/tuner_shim.cpp`, a real meter on the FM screen, and `cinder-probe --fm regmon`
as the on-device test for the shipping code path rather than a probe-local re-implementation.

Measured through the shipping path, not through a shell loop:

```
fm-regmon: cinder_tuner_hw()=1
fm-regmon: start rc=0  signal=15  stereo=0
[cinder-tuner] hw scan 87500-108000 kHz: floor=8 cut=11 -> 8 station(s)
fm-regmon: scan found 8 station(s) in 9876 ms
[cinder-tuner] hw seek from 87500 dir +1 -> 91700 kHz
fm-regmon: hw seek up from 87.5 -> 91.7 MHz in 2751 ms
```

**1. The scan is ~10 s, not ~1 s.** The shell measurement above was dominated by process spawns,
and reading it as the chip's speed was wrong. Through compiled code a step costs ~45 ms, which is
the Si4708's own tune-and-settle time; 206 of those is ~9.5 s and no amount of code removes it.
Still a 10x improvement on the ~90 s audio scan, and the docs and the UI both now say ten seconds.

**2. Hardware seek stops on 50 kHz HALF-steps.** The driver runs the chip at `SPACE=50 kHz`, so
seek landed on **91.45 MHz** — the shoulder of a carrier, not a carrier, because European
broadcasts sit on the 100 kHz raster. Which neighbour is the real one is not deducible from the
offset, so the shim now measures the RSSI of both and keeps the stronger (two tunes, ~90 ms). After
that, results land on the raster: 91.7 MHz.

**3. `GetFrequency` returns 0 right after a start.** Reproduced on both runs
(`start rc=0 freq=0 kHz`). The UI *clamps* whatever it is handed into the band, so a 0 does not
show as "unknown" — it drags the dial to 87.5 MHz and leaves it there. Every read-back in the shell
now goes through `fm_report_actual()`, which reports only a frequency that actually read.

**The RSSI scale moves with the aerial.** Floor 5-6 and carriers 9-14 with the cable's far end in a
PC; floor 8 and carriers up to 15 with it hanging free. So no fixed calibration is honest — the UI
saturates at 18 and marks its floor, and each scan republishes the measured floor as the seek
threshold, which is what makes seek work at all.

**What the register path removed, beyond speed:** the audio seek borrowed `hw:0,1` from
`AudioInPlayerService`, so the radio went SILENT for the duration and the shell's 45 s guard firing
mid-seek would leave the audio path stopped. Hardware seek touches no PCM at all — the radio stays
audible while it sweeps, and that failure mode is gone.

## FOUR DEVICE QUESTIONS SETTLED — 2026-08-18, autonomous session

All four were open items in `cinder-home/ROADMAP.md`; three of them turned out not to be problems
on this configuration, and the fourth is a real one that the BT-out button was silently ignoring.

### 1. Stereo is UNREACHABLE at these signal levels — stop chasing it

`ST` had read 0 at every station in every session, and the obvious suspicion was configuration.
It is not. Tested on the strongest local carrier (98.3 MHz, RSSI 15) with `MONO` (POWERCFG[13])
confirmed clear, stepping `SYSCONFIG1` `BLNDADJ[7:6]` through **all four** values:

```
BLNDADJ=0 -> RSSI=15  ST=0        BLNDADJ=2 -> RSSI=15  ST=0   (Sony's setting)
BLNDADJ=1 -> RSSI=15  ST=0        BLNDADJ=3 -> RSSI=14  ST=0
```

The datasheet explains it: `BLNDADJ=10` — the value Sony ships — is the **most sensitive** option,
starting the stereo blend at 19 dBµV. Our best carrier reads **15**. The signal is below the point
where the chip will even begin to blend, on the most permissive setting the part offers. Nothing in
software reaches this; it is an aerial and reception limit. The `ST` indicator stays in the UI
because it costs nothing and would light on a strong signal — but do not expect it here.

### 2. The "Sony service re-disables the ADC mux on a timer" trap does NOT reproduce

Set `numid=26` to `tuner` and polled it every 5 s for 90 s: it held at `1` throughout, no reset.
Wampy hit that trap with Sony's own player running and owning the mux; with Cinder as Home and the
stock Qt app frozen, nothing is there to reset it. No re-assert timer is needed in the shim.

### 3. The "device suspends to `mem` without a wake_lock" trap does NOT reproduce either

```
/sys/power/autosleep = off        /sys/power/wake_lock = (empty, no locks held)
dmesg suspend entries this boot = 0     uptime 7782 s, idle 6102 s, backlight = 0
```

Opportunistic suspend is **off** on this device, and it had gone 2 h 10 min — screen dark, no
wake_locks held, cinder-home running as Home — without a single suspend. There is no reason to add
a `/sys/power/wake_lock` for FM. (Caveat: measured with adb attached, though nothing held a lock.)

### 4. ⚠️ REAL PROBLEM — `AudioInPlayerService` never releases `hw:0,1`

The BT-out design is "stop the local audio path, and the bridge takes `hw:0,1`". Tested for the
first time with `cinder-probe --fm btcap`, which captures with the mux routed to the tuner, again
with it OFF as a control, and a third time routed:

```
[cinder-tuner] hw:0,1 open=-16 set_params=-1   (-16 = EBUSY)   x3
fm-btcap: RMS  routed=-1  MUX-OFF(control)=-1  routed-again=-1
```

`Stop()` does not give the PCM back:

```
/proc/asound/card0/pcm1c/sub0/status
  state: RUNNING   owner_pid: 337   trigger_time: 909.5   tstamp: 8159.5
/proc/337/cmdline
  hagodaemon AudioInPlayerService TunerPlayerService userAndGroup=system,system nice=-10 sub_sm
```

hagoromo28 opened the capture at **t=909 s** — hours before this session, i.e. left over from the
earlier FM work — and it is still **RUNNING** with its tstamp still advancing at t=8159 s. Every
`snd_pcm_open("hw:0,1")` since returns `-EBUSY`.

**Two readings, and they are not yet distinguished.** Either (a) `AudioInPlayerService` simply never
closes the PCM once opened, in which case the BT-out design needs the bridge to take `hw:0,1`
*before* AudioIn ever does; or (b) the service is **wedged** in a leftover state from the earlier
session — note that `Play()` now returns `rc=0` but leaves `GetPlayerState()` at **0**, where a
healthy start moves it 0 → 2 — in which case a restart of hagoromo28 clears it and the design is
fine. The discriminating test is to restart `hagoromo28` and re-run `--fm btcap`; that was not done
here because killing a Sony service unprompted is not a safe autonomous act on this device.

**What shipped in the meantime:** `cinder_tuner_capture_rms(ms)` measures the bridge's source, and
`fm_btout_fn` now calls it before starting the bridge. If the PCM cannot be opened the button
refuses, says why, and puts the radio back on the jack — instead of lighting up and transmitting
nothing, which is what it would have done. The control in `--fm btcap` matters for the same reason:
a capture that opens and returns all-silence looks identical to one that works.

## RESOLVED — `hw:0,1` was WEDGED, not permanently held (fresh boot, 2026-08-18)

The section above left two readings open and said the discriminating test was a `hagoromo28`
restart. A reboot settled it for free, and **the answer is reading (2): the service was wedged.**

On a fresh boot, before anything touches the tuner:

```
/proc/asound/card0/pcm1c/sub0/status  ->  closed        (and no process holds the fd)
```

So `AudioInPlayerService` does **not** grab the capture PCM at boot and never let go. The
`RUNNING since t=909 s` seen earlier was leftover state from that session's FM work.

Then `cinder-probe --fm btcap` on the clean PCM — the same run that failed with `-EBUSY` three
times an hour earlier:

```
[cinder-tuner] capture rms(500 ms) = 2.8
[cinder-tuner] capture rms(500 ms) = 0.0     <-- mux OFF, the control
[cinder-tuner] capture rms(500 ms) = 8.5
fm-btcap: RMS  routed=2  MUX-OFF(control)=0  routed-again=8
VERDICT — the route is what carries it; hw:0,1 is a real FM source
```

**The control is exactly 0.0 and both routed captures are not.** That is the proof the FM → Bluetooth
design needed and had never had: `hw:0,1` carries FM, and it carries it *because of* the
`analog input device` route rather than incidentally. (Absolute levels are low here because the
aerial was reading `signal=0` on that frequency at the time; the separation is the result, not the
magnitude.)

And afterwards:

```
pcm1c after the probe exited  ->  closed
```

**Released cleanly.** So the normal lifecycle is fine.

### What this changes

* **FM → Bluetooth is NOT blocked.** The earlier entry said it was; it is not.
* The failure mode is real but is a **wedge**, not a design limit: once `AudioInPlayerService` is in
  that state it holds `hw:0,1` `RUNNING` for the rest of the boot and every open returns `-EBUSY`.
  What wedges it is still unknown — the suspect is the `Play()` that returns `rc=0` while
  `GetPlayerState()` stays `0`, where a healthy start moves 0 → 2.
* **A reboot is the recovery**, and no Sony service needs killing to get out of it.
* `cinder_tuner_capture_rms()` and the `fm_btout_fn` gate stay exactly as they are — they are now
  the detector for the wedge rather than for a permanent condition, which is a better reason to have
  them: the button refuses with a reason on the boot where it cannot work, instead of transmitting
  silence.

## THE WEDGE, IDENTIFIED — an exit that skips Stop() (2026-08-18)

Reproduced deliberately, and the rule is simple:

```
three clean start/stop cycles      -> pcm1c closed every time
SIGKILL the client while playing   -> pcm1c RUNNING, and it stays RUNNING for the rest of the boot
```

`AudioInPlayerService` opens the capture on `Play()` and closes it only on `Stop()`. **It does not
notice its client dying.** So any exit that skips `Stop()` — a crash, a `kill -9`, the installer's
own swap script killing cinder-home — leaks `hw:0,1` until reboot, and every later open is `-EBUSY`.

**Shipped:** `fm_release_capture()` is called from every DELIBERATE exit path. It is deliberately
NOT called from the fault handler: that path's contract is to die fast so appmgr reboots and the
bad-boot counter reverts, and a Sony IPC call from a signal handler after a SIGSEGV can hang rather
than return — trading a leaked PCM for a hung Home app is a bad trade. A hard crash therefore still
leaks it, and the recovery is the reboot that is already happening.

**FM → Bluetooth needs no more code.** `fmbt_thread` already opens `hw:0,1`, sets the ADC format,
connects the transmitter socket and pumps. It has never been run with a headphone connected — that,
not construction, is what is left.
