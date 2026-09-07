# RE findings — VolumeService, and how Sony really does Bluetooth volume (offline, 2026-09-07)

Static RE (`nm -DC`, `objdump -dC`, `c++filt`) of `libVolumeService.so`, `libVolumeServiceFw.so`,
`libVolumeGlue.so` and `HgrmMediaPlayerApp`, pulled from the device on 2026-09-07.

**The question this was opened to answer:** on Bluetooth, Cinder's volume bar reaches 0, prints
`MUTE`, and audio is still audible. Cinder drives BT volume with open-loop AVRCP steps
(`SetVolumeUp`/`SetVolumeDown`) because `SetCurrentVolume` was measured inert on this firmware. A
service nobody had looked at — `pst::services::volume::VolumeService` — exports `SetVolume`,
`GetVolume` and `SetDigitalMute`, which looked like the absolute-volume path Cinder was missing.

**It is not.** On the Bluetooth route Sony's own absolute setter is a stub that touches nothing, and
Sony's framework reaches the sink through exactly the two AVRCP calls Cinder already makes. Cinder
was not missing a layer; there is no layer. Written up in full because the symbol names are
actively misleading and this is the second time this ground has been walked.

---

## 1. Architecture

`HgrmMediaPlayerApp` (Sony's stock player) imports **no Bluetooth service at all** — no
`BtTransmitterService`, no AVRCP, nothing. Its complete set of `pst::services` imports:

```
mediastore  configuration  playerservice  onetrackplayer  recorderservice
audioanalyzerservice  ncasm  sound  binder  volume  funcarch
```

Every volume operation it performs goes through one API:

```
volume::VolumeService::SetVolume(unsigned) / GetVolume()
volume::VolumeService::IncrementVolume(unsigned) / DecrementVolume(unsigned)
```

`libVolumeService.so` is a **direct-call client** — the members are exported `T` symbols with
non-virtual mangled names, not a vtable-slot proxy like `BtTransmitterServiceClient`. Each one
looks up `GetServiceFw()` and, failing that, marshals a binder transaction through
`this->[0]->vtable[10]`. So a shim can call them by declaring the mangled prototype, the same
pattern `cinder-audio/src/player_shim.cpp` already uses.

Behind the binder, `libVolumeServiceFw.so` dispatches to one of three output classes, chosen by
`funcarch::OutputDevice`:

| `OutputDevice` | class | route | where proven |
|---|---|---|---|
| 3 | `VolumeAdlerOut` (special case) | WM-PORT dock connector | `Reset` @ `0x1e844`, `cmp r0,#3` |
| 4 | `VolumeUacOut` | USB-DAC | `Reset` @ `0x1d91c`, `cmp r0,#4` |
| 5 | `VolumeA2dpOut` | **Bluetooth A2DP** | `Reset` @ `0x1cda0`, `cmp r0,#5` |
| else | `VolumeAdlerOut` | CXD3778GF codec → 3.5 mm jack | `Reset` falls through |

---

## 2. What is real and what is a stub

`VolumeOutBase` provides default implementations that **do nothing and return false**
(`SetVolume` @ `0x1c3a8`, `IncrementVolume` @ `0x1c3d8`, `GetVolume` @ `0x1c438` — each is a stack
guard, `subs`, `moveq r0,#0`, return). Only the calls a subclass overrides do anything:

| call | `VolumeAdlerOut` (jack) | `VolumeA2dpOut` (BT) | `VolumeUacOut` (USB-DAC) |
|---|---|---|---|
| `SetVolume(n)` | **real** — AVLS clamp → `SetMasterVolume` → amixer | **stub**, returns `!this->m5` | inherits base no-op |
| `GetVolume()` | **real** | **stub**, always returns `0` | inherits base no-op |
| `IncrementVolume` | real | **AVRCP `SetVolumeUp`** | inherits base no-op |
| `DecrementVolume` | real | **AVRCP `SetVolumeDown`** | inherits base no-op |
| `SetDigitalMute` | amixer write | inherits base | inherits base |

`VolumeUacOut` overrides only its constructor, destructor and `Reset` — so **USB-DAC volume is not
controlled by this service either**. On that route the host owns the level, which matches the
hardware.

---

## 3. `VolumeA2dpOut` — the Bluetooth answer

```asm
0001cf0c VolumeA2dpOut::SetVolume(unsigned):
    ldrb r0, [r0, #5]        ; this->m5
    eor  r0, r0, #1          ; return !m5
    pop                      ; ...and that is the whole function

0001cf40 VolumeA2dpOut::IncrementVolume(unsigned):
    if (this->m5)  return false;
    if (!this->m16) return true;              ; no AVRCP link: succeed, do nothing
    r0 = this->[0xd8];                        ; BtTransmitterService client
    blx [r0]->vtable[0x44/4 = 17];            ; SetVolumeUp
    return !rc;

0001cf8c VolumeA2dpOut::DecrementVolume(unsigned):
    ... blx [r0]->vtable[0x40/4 = 16];        ; SetVolumeDown

0001cfd8 VolumeA2dpOut::GetVolume():
    mutex.lock(); mutex.unlock(); return 0;   ; always zero
```

Slots **17 / 16** are `SetVolumeUp` / `SetVolumeDown` — the identical slots
`apply_bt_volume()` in `cinder-home/src/main.cpp` calls. Sony's stack does what Cinder does.

`Init()` @ `0x1cca4` is also the same two calls Cinder makes:

```asm
Framework::GetServiceClient("BtTransmitterService")   ; 20-char std::string, stored at this+0xd8
[client]->vtable[0x9c/4 = 39](listener, "")           ; AddListener — the BtAvrcpMonitorListener
```

`BtAvrcpMonitorListener::OnNotifyAvrcpConnectionStatus` @ `0x1d444` treats status `2` as connected
and re-queries client slot 30 on every connect — i.e. Sony does **not** cache the AVRCP capability
either, which independently confirms the "ASKED EVERY TIME, never cached" rule already in
`apply_bt_volume`'s comment block.

### `SetCurrentVolume` is not in this path — and the earlier claim it was is a false positive

`grep SetCurrentVolume libVolumeServiceFw.so` matches, which is what first suggested Sony had a
working absolute-volume path. The only match is **`VolumeA2dpOut::SetCurrentVolumeCondition`** — a
substring. That function (`0x1ce68`) compares an incoming `VolumeCondition` field by field against
the stored copy, memcpy's 20 bytes if it differs, and fires a change callback. It sends nothing.

Nothing in `libVolumeServiceFw.so` calls AVRCP `SetCurrentVolume`. The 2026-08-26 measurement —
three absolute writes produced no notification, four steps produced four — was right, and now has a
cause: **Sony never sends absolute volume over Bluetooth on this firmware.** It is not a setup
problem, a `VolumeCondition` problem, or a missing capability negotiation.

---

## 4. `VolumeCondition` — 20 bytes

Layout recovered from the field-by-field compare in `SetCurrentVolumeCondition` and confirmed by
the literal stores in `VolumeAdlerOut::Reset` @ `0x1e8ae`:

| offset | type | notes |
|---|---|---|
| +0x00 | u8 | flag A (set when the route's capability query returns non-zero) |
| +0x01 | u8 | flag B — **gates `SetVolume`**; `VolumeAdlerOut::SetVolume` bails when clear |
| +0x04 | s32 | value / min (`-1` on a fixed-level WM-PORT accessory) |
| +0x08 | s32 | value / min (`-1` likewise) |
| +0x0c | u8 | flag C |
| +0x0d | u8 | flag D |
| +0x10 | s32 | **max** |

`VolumeA2dpOut::Reset` selects one of two `.rodata` templates on client slot 30:

```
0x2d2e4 (slot30 != 0):  01 00 | 00000000 | 00000000 | 01 01 | 78 000000     max = 120
0x2d2f8 (slot30 == 0):  00 00 | 00000000 | 00000000 | 00 01 | 78 000000     max = 120
```

Both carry **max = 120**, the same scale as the jack's `master volume` (0..120). So the 0..120 bar
is Sony's scale on every route; on Bluetooth it is a *display* scale with no absolute setter behind
it, which is exactly the trap Cinder fell into.

---

## 5. `VolumeAdlerOut` — the jack, and the WM-PORT case

`SetVolume(v)` @ `0x1ef20`:

```
lock(this+0xd8)
if (!this->m5) bail                                  ; volume control disabled for this route
if (avls_on && avls_mode != 3 && avls_threshold < v)  ; AVLS = the volume limit
      log(line 353); v = avls_threshold               ; clamped, not refused
optional hook [this+0xa0]->vtable[6](in, out)
SetMasterVolume(v)                                   ; → amixer 'master volume' 0..120
```

So AVLS **clamps** rather than rejecting, and it is per-route state on the output object.

`Reset` @ `0x1e824` has one special case, `OutputDevice == 3`. It resolves
`Framework::GetServiceClient("WMPortService")` (the 13-char name in the disassembly) and reads the
accessory's volume type. The log strings are `Wmport: audio_volume_type_t::kFixed` and
`::kVariable`. On **kFixed** it writes:

```
condition = { A=1, B=0, min=-1, max=-1, C=1, D=1, max=108 }
```

`B = 0` disables `SetVolume` outright — correct behaviour for a fixed line-out on the dock.

**The 108 is the WM-PORT scale, not a region cap.** Worth stating plainly because a bare `108` next
to a `120` in a volume table looks exactly like the EU limit, and `reference_volume_tables` already
established (by measurement, with `--volcurve`) that the region tables give identical curves and
there is no EU cap. This is a different axis: dock accessory, not locale.

---

## 6. Digital mute, and why it cannot help Bluetooth

`VolumeService::SetDigitalMute(bool, MuteTarget)` resolves through
`VolumeOutBase::SetDigitalMute` → `VolumeAdlerOut::SetDigitalMuteToAmixer(const std::string& key,
unsigned value)`, logging `SetDigitalMute [key=%s][value=%u]`. `MuteTarget` is a `std::map` onto
ALSA control names; the strings in the library are:

```
icx timed mute    std timed mute    dsd timed mute    fader mute sdin1    fader mute sdin2
```

All are CXD3778GF codec controls. **Measured on device 2026-09-07:** with A2DP playing to a
connected sink, pulsing `playback mute` on/off four times (3 s on, 3 s off) produced no audible
interruption whatsoever. A2DP PCM leaves via the BT transmitter socket and never crosses the codec,
so no codec-side control — mute, `master volume`, or the L/R balance attenuators — can touch it.

The one source-side lever that *does* reach the A2DP stream is the DSP effect chain, because
`SetBtAudioSoundEffect(1)` puts it there (set at boot, `main.cpp:1130`). That is the 10-band EQ, and
it bottoms out at −10 dB. **There is no way to mute a Bluetooth sink from this firmware.**

---

## 7. Consequences for Cinder

1. **The Bluetooth volume path is already correct.** Open-loop AVRCP steps via slots 17/16, with
   the sink's `OnNotifyChangeVolume` as the only truth, is byte-for-byte Sony's own implementation.
   Do not replace it with `VolumeService`; there is nothing behind it on this route.
2. **`MUTE` at Bluetooth level 0 is a lie and should read `MIN`.** AVRCP 0 is the sink's floor.
   Sony's stock firmware has the same behaviour for the same reason — it cannot mute a sink either.
3. **The fine-volume vernier is justified.** It exists because one AVRCP step is 4 units of 127 with
   nothing finer available, and this RE closes the last "but maybe absolute volume works" door.
   Attenuating the EQ curve at the source is the only sub-step resolution that exists.
4. **USB-DAC volume is not a missed feature.** `VolumeUacOut` inherits the base no-ops; the host
   owns that level.
5. **AVLS is unexplored and real.** `SetAvls(bool)`, `GetAvlsThresholdValue()`,
   `GetAvlsCondition()`, `ReplyVolumeLimitAlert(ReplyParam)` — a working volume limiter on the jack
   route, clamping inside `VolumeAdlerOut::SetVolume`. Cinder has no equivalent.
6. **`SetAttenuateLrVolume(float, float)`** is a float L/R attenuator on `VolumeOutBase`, distinct
   from the `l/r balance volume` mixer controls Cinder currently drives. Not yet compared.

---

## 8. Method notes

- The libraries keep demangled prototypes and log format strings in `.rodata`; `strings | grep
  '^virtual '` gives vtable interfaces, and `nm -DC --defined-only` gives the direct-call ones.
  `VolumeA2dpOut` does not appear in a `nm --defined-only | grep -o 'volume[0-9]*[A-Za-z]*'` sweep
  because of how the class-name length prefix falls; grep the demangled output, not the mangled.
- Thumb PC-relative literals: `ldr rN,[pc,#imm]` reads from `(addr+4) & ~3 + imm`; the following
  `add rN,pc` adds `addr_of_add + 4`. Both steps are needed to resolve a `.rodata` pointer.
- **A `grep` hit on a symbol name is not a call site.** `SetCurrentVolume` matched only because
  `SetCurrentVolumeCondition` contains it, and that single false positive is what made an
  absolute-volume path look real. Disassemble the call, or check the substring, before building on
  a name.
