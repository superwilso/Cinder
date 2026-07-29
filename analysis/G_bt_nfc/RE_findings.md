# Bluetooth pairing + NFC tap-to-pair — client vtables

**Status: the API surface is fully recovered. No Ghidra needed.**

`pst::services::*Client` classes export only their FACTORY; every method is virtual, so calling one
needs its vtable INDEX. These libs are built `-fno-rtti` (no typeinfo to anchor on) and stripped
(no `_ZTV...` symbol). What *is* deterministic is the factory: it allocates the object and writes
`group_base + 8` as the vptr.

`analysis/tools/dump_vtable.py` does the whole job from binutils output:

1. disassemble `<Class>Factory::CreateInstance`, recover `group_base` through its GOT load;
2. read the slots out of `.data.rel.ro` via the R_ARM_RELATIVE relocations that fill them;
3. **name each slot from its own trace tag** — every stub opens by building a `std::string` for
   `ServiceManager::TimeMeasureHolder`, and that string is `"<Class>::<Method>"`. The names never
   left the binary.

**Validated against the only table that was already known:** it returns slot 12 =
`SetCurrentSource`, 18 = `SetLdacSoundQuality`, 20 = `SetLdac` for `BtTransmitterServiceClient` —
exactly the three indices `ldac-bridge/src/btclient.c` carries from the earlier hand RE.

Picking the right GOT load needed one refinement. "Resolves into `.data.rel.ro`" is necessary but
not sufficient — that section can abut `.dynamic`, and on `libBtCommonService.so` the stack-guard
load resolved to a `.dynamic` pointer that passed a bare range check and produced a table of
dynamic tags. Candidates are now scored by how many of the first slots point into `.text`, which a
real vtable group does and a run of dynamic tags does not. Both libs below resolve unpinned to the
bases that were originally computed by hand.

## `BtCommonServiceClient` — pairing (group base `0x1baac`, vptr `0x1bab4`)

| Slot | Method | Use |
|---|---|---|
| 3 | `GetBtStatus` | is the radio on, what is connected |
| 4 | `SetRfOnOff` | **the radio toggle** — today it flips UI state only |
| 6 | `SetDiscoverableMode` | be visible to other devices |
| 7 | `Pairing` | **pair** |
| 8 | `CancelPairing` | |
| 9 / 10 / 11 | `SetNumericComparison` / `SetPasskey` / `CancelPasskey` | the pairing dialogs `pairing.rs` already draws |
| 13 | `DisconnectAll` | |
| 14 | `SetSearchMode` | **scan for devices** |
| 15 / 16 / 17 | `DeleteLinkkey` / `DeleteLinkkeys` / `DeleteAllLinkkey` | forget device |
| 18 / 19 | `GetMyDeviceInfo` / `SetMyDeviceName` | |
| 20 / 21 | `GetPairedDeviceInfo` (two overloads) | **the paired-devices list** |
| 25 | `GetRssi` | signal strength |
| 28 | `RequestSspReply` | secure-simple-pairing confirm |
| 29 | `GetServiceUuids` | which profiles a peer offers |

Full dump: `vtable_BtCommonServiceClient.txt`.

## `NfcServiceClient` — tap to pair (group base `0xab40`, vptr `0xab48`)

| Slot | Method |
|---|---|
| 3 / 4 | `Open` (two overloads) |
| 5 / 6 | `Start` (two overloads) |
| 7 | `Stop` |
| 8 | `Close` |
| 9 | `GetCurrentMode` |
| 2, 17 | `FireOnBluetoothOob` (listener side) |

**`FireOnBluetoothOob` is the whole feature.** A tapped headphone tag carries a Bluetooth **OOB**
record; NfcService fires it at the listener with the peer's address. So tap-to-pair is not a
separate stack — it is a shortcut into the same pairing path:

```
NFC:  Open -> Start -> (listener) FireOnBluetoothOob  ->  peer address
BT:   BtCommonServiceClient::Pairing(addr)
      [-> SetNumericComparison / SetPasskey if the peer asks]
BT:   BtTransmitterServiceClient::RequestConnection   (slot 6)
```

Full dump: `vtable_NfcServiceClient.txt`.

## `BtTransmitterServiceClient` — connect/disconnect (already linked by `ldac-bridge`)

The LDAC dump also turned up the connection API, which the BT screen needs and nothing was using:

| Slot | Method |
|---|---|
| 3 / 4 / 5 | `GetAvSrcConnectionStatus` / `GetAvrcpConnectionStatus` / `GetConnectInformation` |
| 6 / 7 | `RequestConnection` / `RequestLastDeviceConnection` |
| 8 / 9 | `RequestDisconnection` / `RequestCancelConnection` |
| 10 / 11 | `RequestStartConnectWait` / `RequestStopConnectWait` |
| 13 / 14 / 15 | `SetCurrentPlayStatus` / `SetCurrentTrack` / `SetMediaAttribute` (AVRCP metadata to the headphones) |
| 16 / 17 | `SetVolumeDown` / `SetVolumeUp` |
| 18–23 | `SetLdacSoundQuality`, `SetSbcSoundQuality`, `SetLdac`, `SetAptxClassic`, `SetAptxHD`, `SetAvrcpNotification` |

So **the codec selector that STATUS.md lists as "device-gated"** is slots 20/21/22 right here, and
`SetCurrentTrack`/`SetMediaAttribute` would put real now-playing metadata on the headphones.

## Next

1. A `btclient`-style shim over `BtCommonServiceClient` (same thiscall-through-vtable pattern
   `ldac-bridge/src/btclient.c` uses), exercised first from `cinder-probe --bt` — with the
   framework + pump up, because these are `pst::services::*` clients like every other one.
2. Reach `pairing.rs` from a real `Screen::Pairing`; wire scan/pair/connect/forget.
3. NFC on top: `Open`/`Start`, listen for `FireOnBluetoothOob`, hand the address to step 1.

## Signatures (Ghidra headless)

The vtable dump gives indices and names; it does not give argument shapes, and guessing one wrong
means writing through a bogus out-param pointer. `analysis/E_usbdac_ldac/ghidra/DecompileAt.java`
decompiles **by address** for exactly this.

Two things it has to do that a normal Ghidra script does not:

- **Rebase.** Ghidra loads a shared object at an image base (0x100000), so the vaddrs the vtable
  dump prints are offsets from `currentProgram.getImageBase()`, not program addresses.
- **Split the blob.** Every stub lives inside one enormous `<Factory::CreateInstance@@Base>`
  function, so `getFunctionContaining` returns something useless. The script removes the covering
  function and creates one whose entry point is the stub. (Setting `TMode` is only needed where
  nothing is disassembled yet — where code already exists Ghidra rejects the context write, and it
  is already Thumb.)

First result, `SetRfOnOff` (slot 4):

```c
void FUN_0001d8a4(undefined4 param_1, undefined1 *param_2)   // (this, const bool&)
```

— the same `Method(this, const T&)` shape `btclient.c` already uses for
`SetLdac(void*, const bool*)`. `GetBtStatus` (slot 3) decompiles with no parameter beyond `this`,
so it returns its value rather than filling an out-param.

Full output: `decomp_BtCommonServiceClient.txt`.

## USB-DAC audio path — `UsbDeviceAudioPlayerServiceClient` (group base `0x36418`)

Measured 2026-07-29: with `sys.sony.config=uac` the PC **enumerates the sound card**, but no audio
reaches the 3.5 mm jack. The gadget is the kernel half; the render half is this service, and
nothing was starting it. Stock's app does.

| Slot | Method |
|---|---|
| 3 | `GetStatus` |
| 4 | **`Start`** — the missing call |
| 5 | `Stop` |
| 6 / 7 | `AddListener` / `RemoveListener` (exported by name here, unlike the BT libs) |

So plain USB-DAC is `Start()` on entry and `Stop()` on exit, alongside the existing
`cinder-msc dac-on|dac-off`. Far cheaper than the BT work, and it is the same
`pst::services::*` client family — so it needs the framework pump up, like everything else.

## The minimum that makes Bluetooth usable — three zero-arg calls

Confirmed on device 2026-07-29: headphones paired under stock stay paired under Cinder (the link
key is service-owned — hence `DeleteLinkkey` on `BtCommonServiceClient`), but they **do not
connect**, because nothing calls the connect. Signatures, from `DecompileAt.java` on
`libBtTransmitterService.so`:

| Slot | Method | Signature |
|---|---|---|
| 3 | `GetAvSrcConnectionStatus` | `(this)` — returns the status |
| 5 | `GetConnectInformation` | `(this, int* out)` |
| 6 | `RequestConnection` | `(this, int& device)` |
| **7** | **`RequestLastDeviceConnection`** | **`(this)` — no arguments** |
| 8 | `RequestDisconnection` | `(this)` |

**Slot 7 needs no device address.** So "reconnect my headphones" is one call, and it needs neither
a pairing UI nor `BtCommonServiceClient`. Pairing can keep happening under stock while Cinder does
connect / disconnect / status — which is most of the day-to-day value for a fraction of the work.

Order of work, cheapest first, all over clients already dumped and (for the transmitter) already
proven to construct:

1. **USB-DAC audio** — `UsbDeviceAudioPlayerServiceClient::Start` (slot 4). Makes the DAC produce
   sound; the gadget half is already done.
2. **BT connect/disconnect/status** — transmitter slots 7 / 8 / 3, all zero-arg.
3. **BT pairing UI** — `BtCommonServiceClient` (`SetSearchMode`, `Pairing`, `GetPairedDeviceInfo`,
   the passkey calls). The big one, and the only one that needs argument RE.
4. **NFC** — `Open`/`Start` + `FireOnBluetoothOob`, feeding step 3's `Pairing`.

### Device run 2026-07-29 — connect declines, and it is not the radio

Headphones paired under stock, powered on and in range. `cinder-probe --bt`:

```
bt: pump running (3466 ticks)
bt: GetBtStatus = 7                 <- radio is ON
bt: AvSrc status before = 0
bt: RequestLastDeviceConnection() …
bt: +1s..+12s status = 0            <- never changes
```

So: both clients construct, the looper is turning before any call, the radio is up, and the
connect is reachable and faults on nothing — it just declines. Ruled out: dead pump, dead radio,
wrong vtable index (a wrong slot would fault or hang, not return quietly).

Two candidates remain:

1. **`SetCurrentSource(true)` may be a precondition.** The `--ldac` path calls it and this one does
   not; the transmitter plausibly refuses to connect while it is not the current source. One extra
   call to test — do this first.
2. **The "last device" record may be empty.** The pairing was made under stock and may never have
   been *connected* under this service instance. Then the addressed form is the way in:
   `GetPairedDeviceInfo` (BtCommon slot 20/21) -> `RequestConnection` (transmitter slot 6,
   `(this, int& device)`). Needs the out-param shape decompiled — mechanical with `DecompileAt.java`.

### Device run 2026-07-29 (later) — SOLVED: the radio was wedged, and `GetBtStatus == 7` is the tell

Both candidates above are **wrong**, and it is worth saying why before the answer, because both
looked strong and each cost a device run.

- **Candidate 1 (`SetCurrentSource(true)` as a precondition) — dead.** Added it before the connect.
  No change: status stayed `0` for all 12 polled seconds.
- **Candidate 2 (empty last-device record) — dead.** logcat, service side:
  `[BT|BtTransmitterService.cc:257] last device found [AC:80:0A:56:A9:91]`. The record exists, and
  `/data/Bluetooth/devdb/dev_cache` shows that MAC sitting immediately before the name
  `WH-1000XM4`, so the connect was aimed at the right headphones the whole time.

**What it actually was.** `SetRfOnOff(false)` then `SetRfOnOff(true)` — a radio power cycle:

```text
bt: GetBtStatus = 7                          <- BEFORE: stale, and NOT a real "on"
bt: cycle: SetRfOnOff(false) …
bt: cycle: status after off = 7   <-- UNCHANGED: radio ignored the request
bt: cycle: SetRfOnOff(true) …
bt: cycle: status after on = 2               <- now a real value
bt: AvSrc status before = 1                  <- 0 on every previous run
bt: RequestLastDeviceConnection() …
bt: + 1s status = 3   <-- CHANGED
```

and on the next run, with no cycle needed: `GetBtStatus = 3`, `AvSrc status before = 3`, stable,
and the stack wrote `/data/Bluetooth/devdb/{host_cache,le}` at that moment — it had not touched
them during any of the failed runs.

**What is NOT yet established: that `3` means _connected_.** The headphones were powered on during
this run but already connected to another host, and a WH-1000XM4 can refuse a second link. So `3`
is quite possibly "connecting/attempting", stalled on a peer that declined — consistent with it
sitting at `3` indefinitely rather than advancing or timing out. What the run *does* prove is that
the state machine moves at all after the cycle, where before it was pinned at `0` through every
call. Re-run `--bt cycle` with the headphones free of any other host, and confirm audibly, before
treating `3` as connected.

**So `GetBtStatus == 7` means the stack is wedged, not "on".** It is not a value the enum uses for
a healthy radio: a healthy one reads `2` (on, idle) or `3` (connected). Treating 7 as "radio is up"
is what made the earlier runs look like a clean decline — every layer reported success and nothing
moved. Do not read `GetBtStatus != 0` as healthy.

**Why the failure was invisible.** Nothing logs. The Sony service logs the getter hit at line 257
and nothing after; MTK's stack (`mtkbt` + `libBtMw` — this is *not* BlueZ, there is no
`/sys/class/bluetooth` and no `hciconfig`) logs nothing to any logcat buffer, checked with
`-b main -b system -b radio -b events`. A connect request into a wedged middleware is silently
dropped. That is why liveness had to be established by side effect (a status transition and a file
mtime) rather than by a return value.

**Service-side connect path**, for whoever wires this into cinder-home — `FUN_000158f0`
(`RequestLastDeviceConnection` impl, file offset `0x58f0`):

```c
if (*(char *)(this + 0x6d) == '\0') {          // not already connecting
    if (GetLastDevice(0, &vec) == 1)            // vec = 6-byte MAC, logs line 257
        (**(code **)(*singleton + 0x10))(singleton, &vec);   // middleware vtable slot 4
} else {
    pst::log::Print(4, ..., 0xef, ...);         // warn @ line 239 if already connecting
}
```

`RequestLastDeviceConnection` (client stub `0xfc44`) makes **no** `TransactionParam::Set*` call
before the send, which independently confirms it is genuinely zero-argument.

**Do not call `GetConnectInformation` (slot 5) yet.** Two attempts crashed at a byte-identical
fault address inside `TransactionParam::GetStr(std::string&)` — the signature of a wrong
*out-param shape*, not a wrong slot. The stub at `0xf7b0` makes no `Set*` before the send (no
input args) and unpacks the reply as `Get, Get, GetStr, Get, Get`: it fills a **struct** holding a
`std::string` at an offset not yet recovered. An `int*` or a bare `std::string*` puts the `GetStr`
write at a garbage offset. Recover the layout first.

**Address convention, since it bit twice:** Ghidra rebases these `.so`s by `0x10000`. The
transmitter client stubs are at file offsets `0xf3f8` (GetAvSrcConnectionStatus), `0xf7b0`
(GetConnectInformation), `0xf9fc` (RequestConnection), `0xfc44` (RequestLastDeviceConnection) —
the `0x1fxxx` figures in the section above are Ghidra addresses for the same functions.

**New tooling** (`analysis/E_usbdac_ldac/ghidra/`): `DecompileStringXref.java` decompiles whatever
function references a given string, and `DecompileCallers.java` decompiles a function's callers.
Together they turn a hagodaemon log line — every Sony service method logs `<File>.cc:<line>` —
straight into the source that emitted it, which is how the connect path above was recovered. ARM
PIC keeps string addresses in PC-relative literal pools, so grep and objdump cannot follow those
references; Ghidra's reference model can.

### Correction 2026-07-29 (evening) — `GetBtStatus == 7` means the radio is OFF, not wedged

The section above concludes that 7 indicates a wedged MTK stack cured by a `SetRfOnOff(false)` /
`(true)` power cycle. **That is wrong.** Once the toggle was wired into cinder-home and could be
driven repeatedly, the device log settled it:

```text
bt: toggle ON  (GetBtStatus=7)     <- radio off
bt: radio after cycle = 2
bt: RequestLastDeviceConnection() sent
bt: toggle OFF (GetBtStatus=2)     <- radio was on
bt: toggle ON  (GetBtStatus=7)     <- reads 7 again, right after being switched OFF
```

`SetRfOnOff(false)` *produces* 7. So the enum is simply: **7 = off, 2 = on/idle, 3 = connected**,
0 = unknown/error. There was never a wedge. The power cycle "worked" only because of its `true`
leg, and the correct fix is one `SetRfOnOff(true)` whenever the status is not 2 or 3.

Why the original reading was so convincing, and the trap to avoid next time: the first probe run
only called `SetRfOnOff` when the status read `0`, so on a status of 7 it **never powered the radio
up at all** — it went straight to `RequestLastDeviceConnection` against a dead radio and watched it
vanish. Every layer reported success. The lesson is not about Bluetooth: a "quiet decline" on this
platform usually means a precondition was never met, and the first thing to check is whether the
code actually performed the setup step it claims to have performed.

**The real defect was upstream of all of it.** cinder-ffi mapped the Settings switch to
`Action::BtToggle` and then discarded it:

```rust
Action::BtToggle(_) => return None, // UI-only (RE follow-up)
```

The switch never touched hardware, which is the whole of "Bluetooth doesn't connect
automatically". Now `CINDER_ACT_BT_TOGGLE` (26) → `apply_bt_toggle()`: `SetRfOnOff(true)` when the
radio is down, then `RequestLastDeviceConnection()`, with `deferred_up` reconciling the switch
against the real status at startup.

One UI note worth keeping: the first version of `apply_bt_toggle` polled for up to 5 s on the
render/input thread. That froze the UI, the user tapped again thinking the switch had failed, and
the second tap turned Bluetooth back off — it read as "the toggle doesn't work". Anything on that
thread must stay short; the polling is now capped at ~0.9 s and the guard budget at 8 s.

---

## 2026-07-29 (late) — the libraries carry their own signatures; stop guessing arguments

Everything below replaces the decompile-and-infer workflow this document has been using. It is
faster, and unlike inference it is not a guess.

### The technique

These libraries are `-fno-rtti` and stripped, which is why the vtable dumper exists. But they are
NOT stripped of **`__PRETTY_FUNCTION__` literals** — every service method logs its own name, and the
compiler bakes the *fully demangled prototype* into `.rodata` to do it:

```console
$ strings -a libBtTransmitterService.so | grep '^virtual .*pst::services::'
virtual bool pst::services::BtTransmitterService::SetVolumeUp()
virtual bool pst::services::BtTransmitterService::SetCurrentVolume(const uint8_t &)
virtual bool pst::services::BtTransmitterService::SetLdac(const bool &)
virtual bool pst::services::BtTransmitterService::SetLdacSoundQuality(const pst::services::IBtTransmitterService::BtLdacSoundQuality &)
virtual bool pst::services::BtTransmitterService::SetCurrentPlayStatus(const pst::services::IBtTransmitterService::BtPlayStatus &, const uint32_t &, const uint32_t &)
virtual bool pst::services::BtTransmitterService::SetMediaAttribute(const pst::base::vector<AvrcpElementAttribute> &)
```

Argument count, types, constness, reference-ness, return type — all of it, for free. Note these are
the **service** (server-side) prototypes, and the client stub mirrors them; the vtable dump still
supplies the INDEX, which the strings do not.

**Do this first, before any decompiling.** `GetConnectInformation` cost two crashes and a Ghidra
session to characterise; `strings` would have shown the shape immediately.

### The marshalling cross-check (cheap, and it agrees)

Every client stub costs a **base of 3 × `TransactionParam::Alloc(4)`**. Arguments appear as
*additional* `Alloc` calls sized to the type. So the argument list is readable from a histogram
without decompiling anything:

| stub | allocs | sizes | meaning |
|---|---|---|---|
| `SetVolumeUp` / `SetVolumeDown` | 3 | 4,4,4 | **no arguments** |
| `IsSupportedAbsoluteVolume` | 3 | 4,4,4 | no arguments |
| `SetLdac`, `SetAptxHD`, `SetAptxClassic`, `SetAvrcpNotification` | 4 | 4,4,4,**1** | one `const bool&` |
| `SetCurrentVolume` | 4 | 4,4,4,**1** | one `const uint8_t&` |
| `SetLdacSoundQuality`, `SetSbcSoundQuality` | 4 | 4,4,4,**4** | one 32-bit enum ref |
| `SetCurrentPlayStatus` | 6 | 4,4,4,**4,4,4** | enum + two `uint32_t` |

Combined with the older rule — **a `GetStr` in the reply means a `std::string` is in the out-param
struct, so a zeroed scalar buffer is unsafe** — this settles call safety without running anything.

### Volume: absolute is available, and it is the right mechanism

The volume rocker did nothing on Bluetooth because Cinder's only volume backend was the ALSA
`card0 'master volume'` control — the **CXD3778GF codec master**, i.e. the 3.5 mm analogue
attenuator. Nothing in the A2DP path passes through it. The switch was never wrong; it was wired to
the other output.

`BtTransmitterServiceClient` has both mechanisms:

| slot | method | note |
|---|---|---|
| 16 / 17 | `SetVolumeDown()` / `SetVolumeUp()` | relative, one sink step, **open loop** |
| 33 | `IsSupportedAbsoluteVolume()` | ask before using 34 |
| 34 | `SetCurrentVolume(const uint8_t&)` | **absolute, 0..127 AVRCP** — preferred |
| 30 / 31 / 32 | `IsAvrcpTgVolumeSupported` / `Set` / `GetControlAbsoluteVolume` | negotiation side |

Absolute wins wherever the sink supports it: the UI level *is* the level, so a persisted level
restores exactly and a ramp cannot drift. Support is a property of the connected headphones, so the
answer is re-queried on every route change rather than cached once for the process.

**3.5 mm and Bluetooth levels are now separate all the way down** — separate UI fields, separate
persisted keys (`volume` / `bt_volume`), separate hardware paths. They have to be: they are
physically different attenuators, and sharing one number means connecting headphones silently
reassigns the jack's level and disconnecting them blasts the headphone level out the jack.

### Full client vtables

**`BtTransmitterServiceClient`** (slots 3–38; 39+ are the destructor group):

| slot | method | slot | method |
|---|---|---|---|
| 3 | `GetAvSrcConnectionStatus` | 21 | `SetAptxClassic` |
| 4 | `GetAvrcpConnectionStatus` | 22 | `SetAptxHD` |
| 5 | `GetConnectInformation` | 23 | `SetAvrcpNotification` |
| 6 | `RequestConnection` | 24 | `IsAvrcpNotification` |
| 7 | `RequestLastDeviceConnection` | 25 | `GetCapabilities` |
| 8 | `RequestDisconnection` | 26 | `GetSoundStatus` |
| 9 | `RequestCancelConnection` | 27 | `SetConnectRetryMode` |
| 10 | `RequestStartConnectWait` | 28 | `GetConnectRetryMode` |
| 11 | `RequestStopConnectWait` | 29 | `GetSocketName` |
| 12 | `SetCurrentSource` | 30 | `IsAvrcpTgVolumeSupported` |
| 13 | `SetCurrentPlayStatus` | 31 | `SetControlAbsoluteVolume` |
| 14 | `SetCurrentTrack` | 32 | `GetControlAbsoluteVolume` |
| 15 | `SetMediaAttribute` | 33 | `IsSupportedAbsoluteVolume` |
| 16 | `SetVolumeDown` | 34 | `SetCurrentVolume` |
| 17 | `SetVolumeUp` | 35 | `ChangeBatteryStatus` |
| 18 | `SetLdacSoundQuality` | 36 | `ChangePlaybackPosition` |
| 19 | `SetSbcSoundQuality` | 37 | `ChangeApplicationSetting` |
| 20 | `SetLdac` | 38 | `SetEnableLowLatency` |

**`BtCommonServiceClient`** (slots 3–29) — this is the entire pairing screen:

| slot | method | slot | method |
|---|---|---|---|
| 3 | `GetBtStatus` | 17 | `DeleteAllLinkkey` |
| 4 | `SetRfOnOff` | 18 | `GetMyDeviceInfo` |
| 5 | `SetRfOnOffEx` | 19 | `SetMyDeviceName` |
| 6 | `SetDiscoverableMode` | 20 | `GetPairedDeviceInfo` |
| 7 | `Pairing` | 21 | `GetPairedDeviceInfo` (overload, + profile filter) |
| 8 | `CancelPairing` | 22 | `SetCoexistenceBtWifiRatio` |
| 9 | `SetNumericComparison` | 23 | `GetCoexistenceBtWifiRatio` |
| 10 | `SetPasskey` | 24 | `SetiAPModelNumber` |
| 11 | `CancelPasskey` | 25 | `GetRssi` |
| 12 | `SwitchDeviceSession` | 26 | `SetHciLogEnabled` |
| 13 | `DisconnectAll` | 27 | `SetStackLogEnabled` |
| 14 | `SetSearchMode` | 28 | `RequestSspReply` |
| 15 | `DeleteLinkkey` | 29 | `GetServiceUuids` |
| 16 | `DeleteLinkkeys` | | |

### Codec selection was file-only

`write_bt_pref()` recorded the codec choice in `/contents/cinder_bt.conf` for the LDAC bridge and
never told the radio — the same defect shape as the BT switch that never called `SetRfOnOff`. Now
applied live via `SetLdac` / `SetAptxHD` / `SetAptxClassic` (three independent bools, so the
exclusive UI choice becomes "enable one, disable the others"; SBC is the A2DP baseline left when all
three are off), plus `SetLdacSoundQuality`. Applied **before** `RequestLastDeviceConnection`,
because A2DP negotiates the codec during connection setup.

### Still blocked

Everything taking or returning a **`pst::base::vector<…>`** — `GetPairedDeviceInfo`,
`GetCapabilities`, `Pairing`/`DeleteLinkkey` (MAC as `vector<uint8_t>`), `SetMediaAttribute`,
`SetCurrentTrack`. The container layout is unrecovered, and this is exactly the hazard that crashed
`GetConnectInformation` twice. The scan/pair/forget UI needs it; the no-argument calls
(`DisconnectAll`, `RequestDisconnection`, `CancelPairing`, `DeleteAllLinkkey`) do not.

`BtLdacSoundQuality`'s numeric values are also unrecovered. Cinder passes its own UI index
(0 Auto, 1 990, 2 660, 3 330 — Sony's own menu order). Safe to be wrong: it is a by-value scalar, so
the failure mode is the wrong bitrate, not memory corruption. The service logs the value it received
as `ldac quality:%d`, so cycling the row while watching logcat settles it.
