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

## Suggested order

1. `cinder-probe --fm`: construct the client, `Open()`, `GetTunerState()`, `SetFrequency()` a
   local station, `Play()`, read `GetSignalLevel` and `GetStereoState`. That answers "does audio
   come out" and settles the two readable enums in one session.
2. Only then decide whether to build the screen or delete it. A screen that draws a fake frequency
   is the same class of lie stripped out of Sound and Advanced twice.
