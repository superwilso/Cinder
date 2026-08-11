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

---

## 2026-07-29 (night) — the container calls are open: `pst::base::vector`/`string` are libc++

Three unknowns closed, all verified on device with `cinder-probe --btinfo`. Together they unblock the
pairing screen, which was the last thing gating tasks #17 and #18.

### `pst::base::vector<T>` is `std::__1::vector<T>`; `pst::base::string` is `std::string`

Both are **typedefs, not classes**. Three independent lines of evidence:

1. The mangled forms `N3pst4base6stringE` and `N3pst4base6vectorI…E` appear in **no symbol anywhere**
   in the vendor lib tree. A real class would mangle as itself.
2. The marshaller's own PLT entry is
   `TransactionParam::GetStr(std::__1::basic_string<char, char_traits<char>, allocator<char>>&)`.
3. The `push_back` loop inside `GetConnectInformation` touches exactly three pointers at +0/+4/+8 —
   libc++'s `{__begin_, __end_, __cap_}`.

`cinder-home` and `cinder-probe` compile against the **libc++ 3.9.0 headers that match the device
runtime**, so real `std::vector` / `std::string` can be passed straight across. There is nothing to
emulate, and the "container out-params are too dangerous to touch" blocker is gone.

### `GetConnectInformation` — it was never a struct

```c
bool GetConnectInformation(pst::base::vector<uint8_t>& addr, pst::base::string& name)
```

**Two** out-params, by reference. Recovered from the prologue (`sl = r1`, `r8 = r2`) plus what each
register is used for: `r8` goes to `GetStr`, while `sl` is walked as `{begin,end,cap}` and grown one
byte at a time by a `Get(1)` loop counted by a preceding `Get(4)` — a MAC address being pushed back.

This retires the old note about "a struct containing a std::string at an unrecovered offset". The two
crashes at an **identical** fault address were not a bad buffer: they were a **missing second
argument**, so the `push_back` wrote through whatever happened to follow the one pointer we passed.
Device check now returns `rc=0` cleanly with empty out-params when nothing is connected.

### `BtPairedDeviceInformation` — 48-byte stride

| off | type | meaning |
|---|---|---|
| +0 | `std::vector<uint8_t>` | address — `end-begin == 6`, the MAC |
| +12 | `uint32` | Bluetooth Class of Device (`0x240404` = A/V headset) |
| +16 | `std::vector<uint8_t>` | 16 bytes — link key / UUID; the UI does not need it |
| +28 | `std::string` | device name |
| +40 | 2 × `uint8` | flags (both `1` for a normally-paired device) |
| +42 | — | padding to 48 |

Typed read on device:

```text
btinfo: typed rc=1 count=2
  [0] AC:80:0A:56:A9:91  'WH-1000XM4'      cod=0x240404 key=16B flags=1,1
  [1] 3C:B0:ED:3B:73:BA  'CMF Buds Pro 2'  cod=0x240404 key=16B flags=1,1
```

The single strongest confirmation of the ABI is in that output: the 10-character name arrived as a
libc++ **SSO** string (`0x14 >> 1 == 10`, characters inline) and the 14-character one as a **long**
string (`__cap_ = 0x11` with the long bit set, `__size_ = 0x0e`, heap pointer). Both representations
decoded correctly through the same declaration, which a merely-plausible layout would not manage.

### `BtLdacSoundQuality` — the value goes through intact

`SetLdacSoundQuality(0..3)` makes the service log `BtTransmitterService.cc:445  ldac quality:0/1/2/3`
— exactly what was sent. Cinder's UI index therefore reaches the wire unchanged. What this does *not*
prove is that `0` semantically means *Auto*; that still rests on Cinder's list mirroring Sony's own
menu order, and being wrong there would mislabel rows rather than misbehave.

### Also confirmed: `dlopen("libasound.so")` works

`/lib/libasound.so` does **not** exist on the device; the library is at `/system/lib/libasound.so`,
and the launcher exports an `LD_LIBRARY_PATH` that includes `/system/lib`, so the bare SONAME
resolves. This matters because cinder-home resolves libasound **lazily and deliberately** — a
`DT_NEEDED` entry on the Home app would turn a missing audio library into a device that boots to
nothing, so the LDAC bridge must degrade to "unavailable" instead.

---

## Round 2026-07-30 — the paired-device path (Devices screen), and where pairing stops

### Every signature the pairing flow needs, straight from `.rodata`

`strings -a vendor/sony/lib/libBtCommonService.so | grep '^virtual '` again — no decompiler needed.
Note that **every one of these takes its arguments by const reference**, including the scalars, and
that the containers are the `pst::base` typedefs (= the libc++ ones, established last round):

| slot | signature |
|---|---|
| 6 | `bool SetDiscoverableMode(const bool &)` |
| 7 | `bool Pairing(const pst::base::vector<uint8_t> &)` |
| 8 | `bool CancelPairing()` |
| 9 | `bool SetNumericComparison(const pst::base::vector<uint8_t> &, const bool &)` |
| 10 | `bool SetPasskey(const pst::base::vector<uint8_t> &, const pst::base::string &)` |
| 11 | `bool CancelPasskey(const pst::base::vector<uint8_t> &)` |
| 14 | `bool SetSearchMode(const bool &, const uint16_t &)` |
| 15 | `bool DeleteLinkkey(const pst::base::vector<uint8_t> &)` |
| 16 | `bool DeleteLinkkeys(const pst::base::vector<pst::base::vector<uint8_t> > &)` |
| 20 | `bool GetPairedDeviceInfo(pst::base::vector<BtPairedDeviceInformation> &)` |
| 21 | `bool GetPairedDeviceInfo(pst::base::vector<BtPairedDeviceInformation> &, const IBtCommonService::BtDeviceListProfile &)` |
| 25 | `bool GetRssi()` |
| 28 | `bool RequestSspReply(const pst::base::vector<uint8_t> &, const IBtCommonService::SspVariant &, const bool &, const uint32_t &)` |

And on the transmitter side, the call that connects **one named device** rather than "whatever was
last used" — the distinction the Devices screen is built on:

```
virtual bool pst::services::BtTransmitterService::RequestConnection(const pst::base::vector<uint8_t> &)
virtual bool pst::services::BtTransmitterService::RequestCancelConnection()
virtual bool pst::services::BtTransmitterService::RequestStartConnectWait()
virtual bool pst::services::BtTransmitterService::RequestStopConnectWait()
```

`RequestLastDeviceConnection` (slot 7) takes nothing, `RequestConnection` (slot 6) takes the BD
address. Cinder now uses the first for the radio toggle and the second for a row tap.

### What is wired (cinder-home, 2026-07-30)

`refresh_bt_paired()` reads slot 20 into a `std::vector<BtPairedDeviceInformation>`, marks the row
whose address equals the one `GetConnectInformation` reports, and pushes names + CoD-derived labels
into the UI. The **addresses never enter the UI** — it holds a row index, the shell holds a parallel
address vector, and the list is re-read after every action so the two cannot drift. `DeleteLinkkey`
sits behind a two-tap confirm for the obvious reason: nothing in Cinder can undo it.

One ordering rule carried over from the radio toggle: `apply_bt_codec()` runs **before**
`RequestConnection`, because A2DP negotiates the codec during connection setup.

### Where pairing stops: there is no listener implementation anywhere in Cinder

Starting a scan is trivial (`SetSearchMode(true, timeout)`), but the results are **pushed**, not
pollable:

```
BtCommonServiceListener::OnNotifySearchedDevice     <- scan results
BtCommonServiceListener::OnNotifyPairingComplete
BtCommonServiceListener::OnNotifyNumericComparison  <- the dialogs pairing.rs already draws
BtCommonServiceListener::OnNotifyPasskey
BtCommonServiceListener::OnNotifySspRequest
BtCommonServiceListener::OnNotifyServiceUuids
BtCommonServiceListener::OnNotifyRssi
BtCommonServiceListener::OnNotifyAclStateChanged
BtCommonServiceListener::OnNotifyBtStatus
BtCommonServiceListener::OnNotifyError
```

There is no `Get`-style call for "devices seen so far", so **implementing a Sony listener vtable is
the single blocker** for both scan-and-pair and NFC tap-to-pair (`FireOnBluetoothOob` is also a
listener-side callback). Cinder has never done this: the player path hands `Connect()` a `NULL`
`PlayEventListener` and polls instead, which is why nothing in the tree can be copied as a template.
That makes it one contained RE task — recover the listener's vtable order and the client's
`AddListener` signature — and not a series of them, since every method above is already located.

Until it exists, the Devices screen states the limit in its footer rather than drawing a scanner
that would spin forever. An inert control teaches the user it doesn't work; a scanner that never
finds anything teaches them the radio is broken.

### Measured 2026-07-30: a `const vector<uint8_t>&` IN-param marshals correctly

Everything proven before this went one way — the service filling containers we handed it.
`RequestConnection`/`DeleteLinkkey`/`Pairing` go the other way, so `cinder-probe --btconnect <row>`
was added to test the safe one of the three (a connect is reversible; `DeleteLinkkey` destroys a
pairing this firmware's Cinder cannot recreate). Result:

```
[cinder-probe] btconnect: RequestConnection(AC:80:0A:56:A9:91) …
[cinder-probe] btconnect: RequestConnection rc=1
```

and, from the service side, the address **echoed back byte for byte**:

```
I/hagodaemon: [BT] BtTransmitterService.cc:229] RequestConnection [ac:80:0a:56:a9:91]
```

That is the proof — not `rc=1`, which only says the stub returned true. Since `DeleteLinkkey`,
`Pairing`, `SetNumericComparison` and `CancelPasskey` all take the same `const vector<uint8_t>&`, this
closes the marshalling question for the whole pairing family in one shot.

### Two side facts worth having

**`SetRfOnOff(true)` reconnects the last device by itself.** Powering the radio up produced, with no
further calls from us:

```
BtTransmitterService.cc:951] AVSRC status change to (1)
BtTransmitterService.cc:980] AVRCP status change to (1)
BtCommonService.cc:523]      BT status change to (2)   then   (3)
```

So `RequestLastDeviceConnection` in Cinder's toggle path is belt-and-braces rather than the thing that
makes reconnect work. Keep it — an explicit request costs nothing and covers the case where the stack
declines to do it — but do not read a successful reconnect as evidence that call did anything.

**`GetBtStatus` has more values than the three we knew.** Alongside 7 = off, 2 = on/idle,
3 = connected, a `RequestConnection` in flight produced `BT status change to (6)` — a transient during
connection setup. Anything that treats "not 2 and not 3" as *off* (Cinder's `bt_radio_up`) will
therefore read a connecting radio as off for a moment. Harmless where it is used today (the route poll
re-reads every 3 s) but it is the kind of thing that becomes a bug in a retry loop.

---

## Round 2026-07-30b — the listener ABI (task #24), recovered

**Result: implementing a Sony listener does NOT require implementing `IBinderObject`.** The client
library builds the binder proxy itself and only needs a raw pointer to an object whose vtable has the
notification methods in the right slots. That is the difference between a day of binder work and an
afternoon of writing one C++ class.

### How the mechanism is put together

Nothing about it is Bluetooth-specific — the machinery lives in `libpstcore.so`:

```
pst::services::binder::ServiceClientBase::AddListener(shared_ptr<IBinderObject>&, shared_ptr<IBinderObject>&)
pst::services::binder::ServiceClientBase::RemoveListener(shared_ptr<IBinderObject>&, unsigned)
pst::services::binder::ServiceClientBase::ServiceListenerProxyBase::{ctor,AddNotifyJob,Disable}
pst::services::binder::ServiceBase::NotifyListeners(unsigned id, TransactionParam&, bool,
                                                    const string&, bool(*)(const string&, const string&))
```

`libBtCommonService.so` imports all of them, so the same pattern should hold for every `pst` service
(NFC and PlayerService included — worth checking before assuming).

### `BtCommonServiceClient`: two more vtable slots than we had mapped

The client vtable was recovered from `.data.rel.ro` (word 127 onwards, `vaddr 0x1bab4`). Slots 0/1 are
the two destructors, slots 3–29 are the 27 service methods already documented, and then:

| slot | method |
|---|---|
| **30** | `int AddListener(IBtCommonServiceListener* listener, const std::string& name)` |
| **31** | `int RemoveListener(unsigned id)` |

Both were read from the disassembly, not guessed. `AddListener` (0xd540 → 0xd590):

1. `ServiceClientBase::GetService()`; if the service is absent → **return 4**.
2. if `listener == nullptr` → **return 1**.
3. `operator new(0x34)` — the proxy. `ServiceListenerProxyBase` ctor, then
   `[proxy+0x00] = vtable`, `[proxy+0x04] = the client`, **`[proxy+0x24] = listener` (raw pointer)**,
   `[proxy+0x28] = std::string(name)` (copy ctor). 0x34 = 52 bytes accounts for exactly that layout,
   which is the cross-check that +0x24 holds a bare pointer and not a `shared_ptr`.
4. `operator new(0x10)` for the shared_ptr control block, then
   `ServiceClientBase::AddListener(service, proxy)` and **return its value — the listener id**.

`RemoveListener` (0xd658) mirrors it: 4 if no service, 1 if `id == 0`, else
`ServiceClientBase::RemoveListener(service, id)`.

### `IBtCommonServiceListener` vtable — the whole map

Recovered by decoding the PIC string references (Thumb-2 `ldr rX,[pc,#imm]` + `add rX, pc`) to the 16
`BtCommonServiceListener::OnNotify*` log strings, then reading the vtable dispatch that follows each
one. Every case has the same shape, e.g. for SearchedDevice:

```
cb3a: ldr.w r0, [r8, #0x24]   ; the listener we registered
cb42: ldr   r1, [r0]          ; its vptr
cb44: ldr   r4, [r1, #0x18]   ; vtable[6]
cb48: blx   r4
```

| slot | method | slot | method |
|---|---|---|---|
| 0, 1 | destructors | 10 | `OnNotifyUpdateOSInfo` |
| 2 | `OnNotifyBtStatus` | 11 | `OnNotifyRssi` |
| 3 | `OnNotifyNumericComparison` | 12 | `OnNotifyStartSwitchDevice` |
| 4 | `OnNotifyPairingComplete` | 13 | `OnNotifyAclStateChanged` |
| 5 | `OnNotifyPasskey` | 14 | `OnNotifySspRequest` |
| 6 | **`OnNotifySearchedDevice`** | 15 | `OnNotifyServiceUuids` |
| 7 | `OnNotifyDisconnectEnd` | 16 | `OnNotifyServiceResume` |
| 8 | `OnNotifyCoexistenceBtWifiRatio` | 17 | `OnNotifyError` |
| 9 | `OnNotifyUpdateSupportProfile` | | |

Contiguous 2..17, in `.rodata` string order, no gaps — which is itself corroboration, since a wrong
anchor would produce a scattered map.

### The one signature needed first, read by hand

`OnNotifySearchedDevice` — slot 6, three arguments, all by const reference (the house style):

```cpp
void OnNotifySearchedDevice(const std::vector<uint8_t>& addr,   // r1 = sp+0x20
                            const uint32_t&            cod,    // r2 = sp+0x1c
                            const std::string&         name);  // r3 = sp+0x10
```

Read off how the handler *builds* those stack objects rather than from any declaration: `sp+0x20` is a
three-word vector header zeroed and then grown by a `Get(1)` push_back loop counted by a preceding
`Get(4)` (the MAC), `sp+0x1c` takes a single `Get(4)`, and `sp+0x10` is filled by `GetStr`. The middle
value is *probably* class-of-device (it would match the 0x240404 seen in the paired list) — log it on
the first run rather than trusting that.

The remaining 15 signatures were NOT recovered this round. A per-case unpack scan produced sequences
that bleed across function boundaries, so anything it printed is unusable; each case needs the same
by-hand read as above. Do them when a feature needs them, starting with `OnNotifySspRequest`,
`OnNotifyNumericComparison`, `OnNotifyPasskey` and `OnNotifyPairingComplete` for the pairing dialogs.

### Open question before writing the code

`AddListener` takes a **name string**, stored in the proxy, and the notify side is
`NotifyListeners(id, param, bool, const string&, bool(*filter)(const string&, const string&))` — i.e.
that string is a **filter key** compared against each listener's registered name. Registering with the
wrong key would mean a listener that never fires while looking perfectly healthy. The empty string is
the obvious candidate for "match everything"; the alternative is the app/module name. Try `""` first
and confirm with a real scan, and treat "no callbacks" as a key mismatch rather than a broken vtable.

### Reproducing any of this

Three things made it tractable and are worth reusing:

1. **The libs are Thumb-2, not ARM.** Exported symbol addresses are odd (`0xc28d`), which is the
   giveaway. Disassembling with `--triple=armv7` yields plausible-looking garbage; use `thumbv7a`.
2. **PIC string references** are `ldr rX, [pc, #imm]` + `add rX, pc`; llvm-objdump annotates the
   literal's address, so target = `word(literal) + addr(add) + 4`. Decoding that gave exact xrefs for
   all 16 strings.
3. **PLT entry → symbol** is positional: entry *i* is at `plt + 20 + 12*i` and corresponds to
   `.rel.plt` entry *i*. Do this before reading any call, or you will attribute a call to the wrong
   function — the first pass here mis-identified `AddListener` as an unrelated stub.

---

## Round 2026-07-30c — the listener ABI, PROVEN on hardware (`cinder-probe --btscan`)

The round-b recovery was static analysis. This is the device confirming it, and it corrected two
things static analysis got wrong.

```
btscan: radio status=3
btscan: AddListener(key='') -> 0
btscan: SetSearchMode(true, 6) rc=1
btscan: callback BtStatus                      <-- a notification arriving on OUR vtable
btscan: RemoveListener(0xb6ffd508 /* &listener */) rc=0
btscan: post-remove callbacks 1 -> 1
btscan: re-registered (rc=0) same toggle: 1 -> 3
btscan: *** RemoveListener CONFIRMED — the unsigned IS the listener pointer ***
```

**Correction 1 — `AddListener` returns 0 on SUCCESS.** Round b read the disassembly's fall-through
return as "the listener id" (because `RemoveListener` takes an `unsigned` and rejects 0). Wrong: the
call returns 0 and the listener demonstrably works. 1 and 4 remain the documented failures.

**Correction 2 — `RemoveListener`'s `unsigned` is the LISTENER POINTER**, which follows once there is
no id to pass. Cast the object's address and hand it over.

**The negative control is the part worth copying.** Silence after a remove proves nothing on its own —
the stimulus might simply have changed no state. So: remove → repeat the identical stimulus (0 new
callbacks) → re-register → repeat it again (2 new callbacks). Only the pair of results is evidence.
On a stack with no failure channel (see the MTK notes above) this is the only honest way to test a
teardown path.

**Confirmed as recovered:** the filter key `""` works; the slot map is right (`OnNotifyBtStatus` at
slot 2 fired, named correctly, which a one-off error would have shown as the wrong name printing);
`SetSearchMode(const bool&, const uint16_t&)` returns 1 on acceptance.

**Still unknown:** the second `SetSearchMode` argument's units (30 is used as "seconds" and behaves
sensibly), and `GetRssi()` returned 0 with no `OnNotifyRssi` — so either it needs a connected device
argument or that reply path differs. Neither blocks discovery.

### What shipped on top of it

`cinder-home` now registers `CinderBtListener` (16 virtuals, slots 2..17, **static storage** because
the proxy holds a raw pointer), starts/stops discovery from a SCAN button on the Devices screen, and
pairs with a FOUND row via `Pairing` (slot 7). Callbacks land on the framework looper, so they only
append to a mutex-guarded vector and set a flag; the render loop is what pushes it to the UI. A
`OnNotifyPairingComplete` re-reads the paired list and ends the scan.

The four prompt callbacks (`NumericComparison`, `Passkey`, `SspRequest`, `PairingComplete`'s arguments)
are declared but take unnamed word-sized parameters that are **never dereferenced** — ABI-safe on armhf
and enough to log that a device is waiting on a prompt Cinder cannot show yet. That is the remaining
gap, and the Devices screen says so in its footer rather than failing silently.

### Measured 2026-07-30d: `OnNotifyPairingComplete` fires BEFORE the pairing table is updated

First real pairing on the device worked, but looked broken. The log is unambiguous:

```
bt-scan: Pairing(row 0) rc=1
bt-scan: pairing complete — refreshing the paired list, scan off
bt-paired: 1 device(s)          <-- the read right after the callback: still the OLD count
bt-scan: Pairing(row 0) rc=1    <-- so the user tapped PAIR again
bt-paired: 2 device(s)          <-- and THAT is what made it appear
```

So `OnNotifyPairingComplete` is not a promise that `GetPairedDeviceInfo` will report the new link key
yet. **Do not refresh once on the callback.** Cinder now schedules re-reads (~700 ms apart, up to 8)
and stops the moment the address it just paired shows up, then gives up quietly rather than inventing
a row.

Same shape as the pattern noted earlier in this file: on this stack a call returning cleanly is not
evidence that the state behind it has moved. Poll for the side effect.

Two display bugs in the same pass, both of which made a working pairing look worse than it was:

* **An already-paired device kept appearing in the scan results**, still offering "TAP TO PAIR" — a
  scan reports paired devices like any other. They are now filtered out of the FOUND list. That
  filtering is also why the shell keeps a *second* address list in UI-row order: the index the UI hands
  back addresses the FILTERED list, and reusing the raw scan list would pair with the wrong device
  whenever anything was hidden.
* **A name arriving after the address never reached the screen.** Devices are reported repeatedly and
  often nameless on the first report; the code kept the better name but only marked the list dirty for
  *new* devices, so a row that started as "(unnamed)" stayed that way for the whole scan.

## Round 2026-07-30e — the four prompt callbacks, read by hand

Same method as `OnNotifySearchedDevice`: find the handler's prologue, read the `TransactionParam`
unpack sequence, then read which stack objects the vtable call actually passes. The slot index in each
call site double-checks the map (`ldr r4, [r1, #N]` → N/4).

| listener slot | signature | verified by |
|---|---|---|
| 3 `OnNotifyNumericComparison` | `(const vector<uint8_t>& addr, const uint32_t&, const uint32_t&, const string& name)` | `[r1,#0xc]`; args r1=sp+0x30 (push_back loop), r2=sp+0x2c, r3=sp+0x28, **[sp]=sp+0x18** (GetStr) |
| 4 `OnNotifyPairingComplete` | `(const vector<uint8_t>& addr, const uint8_t& result, …)` | `[r1,#0x10]`; r1=sp+0x48, r2=r7-0x31 (a single byte), r3=sp+0x10. **No `GetStr` in this handler**, so the third argument is not a string — left undecoded because Cinder only needs the fact that it fired |
| 5 `OnNotifyPasskey` | `(const vector<uint8_t>& addr, const uint32_t& passkey, const string& name)` | `[r1,#0x14]`; r1=sp+0x20, r2=sp+0x1c, r3=sp+0x10 |
| 14 `OnNotifySspRequest` | `(const vector<uint8_t>& addr, const string& name, const uint32_t&, const uint32_t&, const uint32_t&)` | `[r1,#0x38]`; r1=sp+0x30, r2=sp+0x20 (GetStr), r3=sp+0x1c, **[sp]=sp+0x18, [sp+4]=sp+0x14** |

Note the four-and-five-argument cases spill onto the STACK (`str.w r8,[sp]` / `strd r2,r1,[sp]`), which
is why the earlier placeholder declarations took three word-sized parameters and never dereferenced
anything — a placeholder that guessed at arity would have read rubbish.

**Two values are still uninterpreted, deliberately:**

* `NumericComparison` hands over **two** 32-bit words and nothing says which is the six digits the peer
  displays. Cinder logs both and shows the one that looks like a 6-digit code. Being wrong here shows
  the wrong number; it cannot corrupt anything, because the reply is a yes/no.
* `SspRequest`'s three words map onto
  `RequestSspReply(const vector<uint8_t>&, const SspVariant&, const bool&, const uint32_t&)`, but
  `SspVariant`'s enumerators are not decoded. So Cinder **echoes back the words it received** rather
  than interpreting them — the one choice that cannot be wrong about an undecoded enum.

### What shipped

A modal panel on the Devices screen: device name, the code in large digits, YES, PAIR / CANCEL. A
`Passkey` prompt is display-only (that code is for the *other* device's user to type), so it offers a
single DISMISS which sends `CancelPairing`, and a unit test pins that a passkey panel can never report
Confirm. The prompt is modal in the tap handler too — while the radio is blocked waiting for an answer,
nothing else on the screen is reachable.

---

## Round 2026-07-30f — NFC tap-to-pair: the ABI, and the NFC controller powering up on demand

`libNfcService.so` has **no `virtual …` prototype strings** (it is a much smaller library and logs
differently), so this one came entirely from the vtables and the disassembly. The generic listener
pattern from round b held, which is the main structural result: *registration sits immediately after
the last service method on every `pst` client*, not at a fixed slot.

### `NfcServiceClient` vtable (`.data.rel.ro` word 83, vaddr 0xab4c)

| slot | method | slot | method |
|---|---|---|---|
| 0, 1 | destructors | 7 | `Stop` |
| 3, 4 | `Open` (two overloads) | 8 | `Close` |
| 5, 6 | `Start` (two overloads) | 9 | `GetCurrentMode` |
| | | **10** | **`AddListener`** |
| | | **11** | **`RemoveListener`** |

Slot 13 is `-4`, the start of a secondary base's vtable — same shape as `BtCommonServiceClient`.

An alloc histogram over the stubs (base = 3 × `TransactionParam::Alloc`) says `Open` slot 4, `Stop`,
`Close` and `GetCurrentMode` marshal **no** arguments, `Start` slot 5 marshals **one**, and `Start`
slot 6 marshals **two**. That is what picked the calls used below.

### `NfcServiceListener` vtable

| slot | method |
|---|---|
| 2 | `OnBluetoothOob` |
| 3 | `OnUnknownTag` |
| 4 | `OnHostCardEmulation` |

`OnBluetoothOob` takes **one** argument (`ldr r2,[r1,#0x8]` = slot 2, `r1 = sp+0x10`): a pointer to a
struct the client fills from the transaction. The filler (0x4dd8) unpacks, in order, a `Get(4)` count
followed by a `Get(1)` push_back loop into a vector at **+0x00**, then a `Get(4)` stored at **+0x0C**,
then more fields from +0x10 (strings — the caller destroys one at +0x1C).

So the struct starts `{ vector<uint8_t> addr; uint32 cod; … }`. **Only that prefix is read.** Declaring
the unverified tail would risk a crash for no benefit, since the address is the one field tap-to-pair
needs — and the notification's own name already tells us it is a Bluetooth OOB record.

### Measured on device (`cinder-probe --nfc`)

```
nfc: client=0xb724ec10
nfc: AddListener -> 0            (same convention as BtCommon: 0 = registered)
nfc: GetCurrentMode (before) = 0
nfc: Open() slot4 rc=0
nfc: Start(0) slot5 rc=1
nfc: GetCurrentMode (after) = 0
```

`GetCurrentMode` did not move, which on its own would be discouraging — but logcat shows the calls
reaching the service and **the NFC controller coming up**:

```
D/NfcAdaptation: NfcAdaptation::Initialize: enter
I/BrcmNfcNfa:    NFA_Init () / nfa_rw_init () / nfa_ce_init () / NFA_Enable ()
E/NfcNciHal:     prmFileOpen Unable to open updatefile /vendor/firmware/cxd225x_firmware.bin
```

So `Open`/`Start` work, and this is another instance of the rule that keeps recurring on this device:
judge by side effects, not by return values. (The missing `cxd225x_firmware.bin` is a patch-update
file; the stack proceeds without it. Worth remembering if reads turn out flaky.)

**Not yet proven: a tag callback.** Nothing was tapped during that run, so `OnBluetoothOob` firing —
and therefore the struct-prefix read — is still unverified. Cinder is **not** wired to NFC until it is:
`libNfcService` is linked into `cinder-probe` only, and `readelf -d cinder-home | grep NfcService`
returns nothing, deliberately. The Home app does not take a dependency on a path whose payload read has
never executed.

Next step is one command with a phone against the rear panel:
`cinder-probe --nfc 30`.

---

## Round g (2026-08-10) — the negotiated codec, and Sony's OTHER Bluetooth mode

### The codec the UI shows was never the codec in use

Everything the Bluetooth screen displays is the user's **preference** — `SetLdac` / `SetAptxHD` /
`SetAptxClassic`, applied before `RequestConnection`. But A2DP **negotiates**, so a sink that
cannot do LDAC silently lands on SBC while the UI still says "LDAC". Nothing ever asked what was
actually agreed.

`BtTransmitterServiceClient` **slot 26** is the answer:

```
virtual void GetSoundStatus(BtSoundCodec&, BtSoundFrequency&, BtSoundChannel&, bool&)
```

Four OUT params, **all scalars** (three enums + a bool). That is what makes it safe to call blind,
unlike `GetCapabilities` (takes a `pst::base::vector`) or `GetConnectInformation` (holds a
`std::string`) — the call that crashed twice before the container layout was recovered.

Exercised on device via `cinder-probe --btwho`, radio OFF: all four out-params were **written**
(0xDEAD sentinels came back as 0), so the ABI is right. The enumerators are NOT yet mapped — with
nothing connected every field is 0, and `0` is therefore "none/unset" rather than SBC. The service's
own log line is the key to the map:

```
codec:0x%02x channel:0x%02x frequency:0x%02x bit_per_sample:0x%02x
```

so one run with a known headphone connected settles it. **Do not guess the enumerators** — same
rule as `BtLdacSoundQuality`.

### Sony's alternative Bluetooth mode = RECEIVER (the Walkman as an A2DP SINK)

`BtTransmitterService` sends audio OUT to headphones. **`BtPlayerService` is the mirror image**: a
phone streams TO the Walkman, and the CXD3778GF DAC/amp drives whatever is in the 3.5 mm jack. The
HAL half is `libaudiohal-a2dpsnksingletrack.so` ("a2dp **snk**"), catalogued back in CLAUDE.md §H3
as "BT-receive sink, Walkman as BT speaker".

It is worth having: the amp is the expensive part of this device and a phone has nothing like it.
Note `SetLDAC` on the **sink** side — this can RECEIVE LDAC, which few receivers do.

**`BtPlayerServiceClient` vtable**, recovered from `_ZTVN3pst8services21BtPlayerServiceClientE` at
`0x313dc`. `.data.rel.ro` is relocated at load, so the file words are all zero — the slot map comes
from the `R_ARM_ABS32` relocation entries covering that range, not from the raw bytes. (Reading the
bytes gives 34 zeros and looks like a dead end.)

| slot | method | slot | method |
|---|---|---|---|
| 4 | `GetServiceName` | 19 | `GetPlayStatus` |
| 5 | `GetAvSnkConnectionStatus` | 20 | `SendCommand(BtPlayControl&, bool&)` |
| 6 | `GetAvrcpConnectionStatus` | 21 | `SendCommand(uint8&, bool&)` |
| 7 | `GetConnectInformation` | 22 | `SetAAC` |
| 8 | `RequestConnection` | 23 | `SetLDAC` |
| 9 | `RequestConnectionWithRoleSwitch` | 24 | `SetLDACBufferControl` |
| 10 | `RequestLastDeviceConnection` | 25 | `SetDriftControl` |
| 11 | `RequestDisconnection` | 26 | `GetTrackCodec` |
| 12 | `RequestCancelConnection` | 27 | `GetTrackFreq` |
| 13 | `RequestStartConnectWait` | 28 | `GetTrackChannel` |
| 14 | `RequestStopConnectWait` | 29 | `GetTrackScmst` |
| 15 | `StartSound` | 30 | `GetBitrate` |
| 16 | `StopSound` | **31** | **`AddListener`** |
| 17 | `SetCurrentVolume` | **32** | **`RemoveListener`** |
| 18 | `GetMedia(BtMetadataType&)` | 33 | `GetName` |

Third independent confirmation of the general rule: **registration sits immediately after the last
service method**, not at a fixed slot (BtCommon 30/31, Nfc 10/11, BtPlayer 31/32).

Listener slots, in declaration order from the exported names: `OnNotifyAvSnkConnectionStatus`,
`OnNotifyAvrcpConnectionStatus`, `OnNotifyConnectInformation`, `OnNotifyReceiveMedia`,
`OnNotifyPlayStatus`, `OnNotifyTrackNumber`, `OnNotifyVolumeDown`, `OnNotifyVolumeUp`,
`OnNotifyChangeVolume`, `OnNotifyRemoteVersion`, `OnNotifySoundStatus`, `OnNotifyBitrate`,
`OnNotifyError`, `OnNotifyAudioSetting`, `OnNotifyReceiveMediaComplete`, `OnNotifyAudioState`,
`OnNotifyRegisterForAbsVolume`.

`GetTrackCodec` / `GetTrackFreq` / `GetTrackChannel` / `GetBitrate` are **no-arg scalar returns** —
the receive-side equivalent of `GetSoundStatus`, and just as safe to call.

**Not yet proven: an actual sink connection.** `cinder-probe --btrx <secs>` is written and linked
(into the PROBE ONLY — `readelf -d cinder-home | grep -c BtPlayerService` = 0, same rule as
NfcService: no `DT_NEEDED` on the Home app for a path nothing has exercised). It powers the radio
if needed, `AddListener`, `RequestStartConnectWait` + `SetDiscoverableMode(true)`, polls the track
codec/freq/bitrate while it waits, then **puts everything back** — StopSound, disconnect,
StopConnectWait, discoverable off, and the radio off again if it was the one that powered it.

Next step is one run with a phone: `cinder-probe --btrx 40`, pair the Walkman from the phone's
Bluetooth list, play something. If `OnNotifyReceiveMedia` / `GetBitrate` come back non-zero, the
sink path is live and the Receiver screen can be wired to it.

---

## Round h (2026-08-11) — Sony's "Use Enhanced Mode", and why the buds beep

The user remembered a Sony setting that removed their CMF Buds' volume-change feedback beep, and
said explicitly it was **not** the BT Receiver mode of round g. It is this, and the firmware names
it outright.

`vendor/sony/bin/HgrmMediaPlayerApp`, on the Bluetooth Setting screen (title msg `230002`, the same
screen as "Wireless Playback Quality" `230009` and Auto-Connect):

```
property bool is_absolute_volume_on   // AbsoluteVolume
signal absoluteVolumeOnOffToggled()
property string absoluteVolumeTitile     : qsTr("230077")
property string absoluteVolumedescription: qsTr("230079")
BT SetAbsoluteVolume[%d]        // dmpapp::BtTransmitter
```

`vendor/sony/translations/HgrmMediaPlayerApp_en_US.qm` (parsed: sections are tagged `0x42` Hashes /
`0x69` Messages / `0x88` NumerusRules, translations are UTF-16BE):

| id | English |
|---|---|
| `230077` | **Use Enhanced Mode** |
| `230079` | Select this check box\nif you cannot change the volume. |

So "Enhanced Mode" is the **AVRCP absolute-volume switch**, nothing else.
`vendor/sony/lib/libBtTransmitterService.so` shows the whole state machine in three log strings
that live adjacent to `SetCurrentVolume`:

```
Not control absolute volume mode     <- GetControlAbsoluteVolume() == false  (the checkbox is OFF)
Not support absolute volume          <- IsSupportedAbsoluteVolume() == false (the SINK can't)
Send absolute volute(%u)             <- both true: the level is transmitted
```

**Why the beep.** With the preference off, `SetCurrentVolume` transmits nothing, so a volume press
has to go out as `SetVolumeUp`/`SetVolumeDown` — AVRCP passthrough `VOLUME_UP`/`VOLUME_DOWN` key
events. A sink that treats those as its own button press answers each one with its own feedback
tone. With the preference on, the player sends the level and the sink just adopts it silently.

**What Cinder had wrong.** `bt_abs_volume_supported()` gated only on `IsSupportedAbsoluteVolume`
(slot 33) and never touched the preference. Sony's service checks the preference *itself* before
transmitting, so on a device where stock had last left the box unticked, Cinder's absolute path was
a silent no-op and every volume step fell through to the beeping one. Fixed: the Bluetooth screen
now carries the switch, `bt_apply_enhanced_mode()` pushes it at boot, on the user toggle, and on
every reconnect (the radio does not carry it across a link), and `bt_use_absolute_volume()` requires
both halves.

Client vtable slots used (already tabled in round f): 30 `IsAvrcpTgVolumeSupported`,
31 `SetControlAbsoluteVolume`, 32 `GetControlAbsoluteVolume`, 33 `IsSupportedAbsoluteVolume`,
34 `SetCurrentVolume`. `cinder-probe --btwho` now prints 30/32/33 alongside the negotiated codec.

Not the cause, ruled out on the way: `SoundEnhancementSetting`
(`selectBtReceiverPrioritySetting`, msgs `230087` "Sound Quality Preferred" / `230088` "Connection
Preferred") is the **Receiver** playback-quality screen, a different feature entirely.

---

## Round i (2026-08-11) — DisplayService, and the standby power picture

Measured on device, screen dark, idle, not playing (30 s windows, `/proc/stat` + per-thread
`/proc/<tid>/status`):

| | CPU | context switches |
|---|---|---|
| whole system | 1.17% of one core | ~354/s |
| `cinder-home` (all 10 threads) | 0.25% of one core | 20.9/s |
| `disp_ovl_engine_rdma0_update_kthread` + `_DISP_ConfigUpdateKThread` | ~0.47% | **~230/s** |

CPU frequency residency is healthy: 2997 of 3009 jiffies at the **minimum** 598 MHz (governor
`hotplug`; steps 1300/1196/1040/747.5/598 MHz). Earlier reads showing a pinned 1.3 GHz were the
observer effect — `adb shell` + `cat` wakes the core. Use `cpufreq/stats/time_in_state`, which is
cumulative and immune to it.

Playing, screen dark: system 37.5% of a core, 604 ctxt/s, of which `SoundServiceFw` is 33.8% and
`hagodaemon MediaStoreService PlayerService` 5.05%. **cinder-home is 0.43%** — about 1% of the
total. The decoder dominates and always will.

### The display pipeline is not addressable from here

`libDisplayService.so` has the real panel switch. Vtable recovered from the `R_ARM_ABS32`
relocations covering `.data.rel.ro` (the words on disk are zero — same technique as round g):

| slot | method |
|---|---|
| 3 | `SetOneLedBrightness(const uint32_t&, const uint32_t&)` |
| 4 | `SetMultiLedBrightness(const vector<uint32_t>&, const uint32_t&)` |
| 5 | `SetMultiLedPacket(const vector<led_packet_t>&)` |
| 6 | `SetLedBlink(const uint32_t&, ×4)` |
| 7 | `SetLedSpecialPattern(const uint32_t&, const vector<uint32_t>&, const string&)` |
| **8** | **`SetLCDValidate(const bool&)`** |
| 9 | `SetLCDValidateGradually(const bool&)` |
| 10 | `GetLCDValidate(bool&)` |
| 11 | `SetLCDBacklightBrightness(const uint32_t&)` |
| 12 | `GetLCDBacklightBrightness(uint32_t&)` |
| **13** | **`SetTouchPanelValidate(const bool&)`** |
| 14 | `GetTouchPanelValidate(bool&)` |
| 15 | `SetDimmer(const bool&, const uint32_t&)` |
| 16 | `GetName() const` (primary vtable ends here; slot 17 is the `0xfffffffc` secondary marker) |

`cinder-probe --disp` (read-only) with the backlight already at 0 reported
`GetLCDValidate=1 GetTouchPanelValidate=1 backlight=255` — **the panel and the touch controller are
fully powered during screen-off**, and DisplayService still believes the backlight is 255 because
Cinder writes the sysfs node behind its back.

`cinder-probe --dispoff` proved the call works and reverses: a second client read
`GetLCDValidate=0 GetTouchPanelValidate=0` mid-window, and 1/1 afterwards. **But it does not quiet
the two MTK kernel threads** — +5 jiffies/10 s invalidated vs +7/15 s baseline, i.e. identical.
`echo 4 > /sys/class/graphics/fb0/blank` is worse than useless: rc=0, `fb0/state` stays 0, same CPU.
So those 230 ctxt/s are not reachable from userspace on this firmware; treat the number as the
floor, not as a bug to fix.

### What WAS reachable

`SetTouchPanelValidate` is the one that matters. `touch_set_sleep()` has been logging
*"no touch sleep node found"* on every screen toggle since 2026-07-02 — Wampy's two himax paths do
not exist on this unit and neither does any `sleep` attribute on the i2c bus — which means the
capacitive panel has **never** stopped scanning with the screen off. It is now driven through
DisplayService slot 13, reached by `dlopen` rather than a link (a `DT_NEEDED` on the Home app for a
path this thin is the `libNfcService` boot-to-nothing rule), and only from the **Power-button**
blank, never the idle blank — the idle blank must stay wakeable by touch. Re-validated on every
wake and at `input_open()`, so a stuck "invalid" cannot outlive one Power press or one reboot.

---

## Round j (2026-08-11) — the sink's own volume, and why the bar read mute while audio played

Round h found the *preference* (`SetControlAbsoluteVolume`, "Use Enhanced Mode"). This round is the
consequence: with absolute volume in play, **Cinder's volume number stopped being the truth**. The
sink owns its level, and there is no getter for it — only a notification.

### The bug, stated precisely

Cinder kept an absolute `0..CINDER_BT_VOL_MAX` counter and assumed the sink followed it. That
assumption holds only while `SetCurrentVolume` is actually being transmitted. Three separate things
break it, and all three were observed on the CMF Buds Pro 2:

1. **The preference is off** → `SetCurrentVolume` transmits nothing (round h) and every press
   silently degrades to an AVRCP passthrough step.
2. **The sink refuses absolute volume mid-session** (below) → same outcome, but intermittently.
3. **The user turns the volume up on the headphones themselves** → nothing tells Cinder at all.

Once the counter and the sink drift apart, the UI shows one thing and the ears hear another. The
report — *"I have it on mute but it has audio"* — is exactly that drift, and it is not fixable by
being more careful with the counter. The sink has to be asked.

### `IsSupportedAbsoluteVolume` (slot 33) is NOT a stable property

This is the finding that cost the most time, because caching it is wrong in **both** directions:

| what was cached | why it looked right | how it failed on device |
|---|---|---|
| the first answer | one IPC read per session | `GetBtStatus` reaches 3 *before* the sink's AVRCP capabilities are readable, so the first ask reliably answers **no** — and the whole session then used the step path |
| only a `true` ("a YES can't become a NO on one link") | AVRCP capability is negotiated at connect | **measured 1, then 0, on the same unbroken link**, `IsAvrcpTgVolumeSupported` moving with it. Cinder held the stale YES, `SetCurrentVolume` was refused on every press, the step fallback never ran → UI moved, headphones didn't |

Support is a property of the **AVRCP session**, which renegotiates underneath a connection that
never drops. So it is read on every press, and even a `true` is treated as provisional:
`SetCurrentVolume`'s return is checked, and a `false` falls through to a step in the same press.

> **Rule this generalises to:** on this stack, a capability read is a *reading*, not a fact. Judge
> by side effects — the same rule that BT radio state (`GetBtStatus`) already taught us.

### The volume notification — recovered and proven

Read with `cinder-probe --btvollisten <secs>`, which registers a listener whose every slot logs its
index, then moving the volume by hand.

| what | index | evidence |
|---|---|---|
| `BtTransmitterServiceClient::AddListener` | **client slot 39** | immediately after the last service method (38 `SetEnableLowLatency`) — the same placement BtCommon (30) and Nfc both use |
| `RemoveListener` | client slot 40 | the pair is always the last two |
| `OnNotifyChangeVolume(const uint8_t&)` | **listener slot 10** | instrumented every slot; only 10 fired, carrying **19 → 15 → 19 → 15** against two down and two up presses |

Two facts fall straight out of those numbers:

* **Sony's relative step is 4 units** of AVRCP's 0..127 — i.e. stock's own volume granularity over
  Bluetooth is ~32 steps, not the 30 Cinder happened to use.
* The buds were sitting at **15/127 ≈ 12%** while Cinder's bar read near the bottom of a 0..30
  scale. Same direction, different scale, drifting — the reported symptom.

The listener object must be **static**: `AddListener` stores a raw, unowned pointer (round 30b).
The callback arrives on the framework looper, so it stores one byte and the render thread applies
it — `0..127 → 0..N` with `(v * N + 63) / 127`, rounded rather than truncated so the top of the
sink's scale reaches the top of ours instead of stopping one step short.

The eight callbacks above slot 10 are placeholders that **never dereference their arguments**.
Their real signatures carry `pst::base` containers, and a wrong guess there is a crash rather than
a no-op; all they exist to do is put `OnNotifyChangeVolume` at index 10.

### The first-connect transient

There is no getter, so until the sink volunteers a notification the bar is still last session's
persisted belief. Reported as *"3 was mute until I went to mute, then it worked properly"* — going
to mute forced a real change, the sink reported, and the bar corrected itself.

Fixed by provoking the first report at connect: one `SetVolumeUp` (17), 150 ms, one `SetVolumeDown`
(16). Net zero to what the user hears, and the sink reports its real level within a second. Only
needed on the step path — when absolute volume is accepted, the push at connect already establishes
where the level landed.

### Volume granularity: 30 → 64

With the sink reporting in units of 127 there is no reason to quantise to 30. `BT_VOL_MAX` is now
**64**, so one press moves ~2 AVRCP units instead of ~4.2 — finer than stock's own 4-unit step.
Persisted under a new key `bt_volume64`; the legacy `bt_volume` is read once and rescaled, so an
existing install doesn't wake up at half volume.

### Slot map delta (BtTransmitterServiceClient)

| slot | method | how known |
|---|---|---|
| 16 | `SetVolumeDown()` | in use |
| 17 | `SetVolumeUp()` | in use |
| 31 | `SetControlAbsoluteVolume(const bool&)` | round h |
| 32 | `GetControlAbsoluteVolume()` | round h |
| 33 | `IsSupportedAbsoluteVolume()` | round h; **flaps — see above** |
| 34 | `SetCurrentVolume(const uint8_t&)` | proven this round: return value distinguishes transmitted from refused |
| 38 | `SetEnableLowLatency(const bool&)` | last service method |
| **39** | **`AddListener(listener*, const string& key)`** | this round |
| **40** | **`RemoveListener(...)`** | this round (position, not exercised) |

A note on method, since it burned an hour: an early slot scan produced no service-side log and I
read that as "34 is not `SetCurrentVolume`". It proved nothing — hagodaemon's file log had stopped
writing twelve minutes earlier. **Check that the log is live before treating its silence as
evidence.**

### Addendum — the BT stack's idle-connected CPU cost is the link, not us (task #31)

Round i left an open number: with the CMF Buds connected and **nothing playing**, `mtkbt` 6.15% +
`BtCommonService` 5.50% + `btif_rxd` 1.80% ≈ **13.5% of the single online core**. The question was
whether that is the intrinsic cost of an A2DP link or something Cinder keeps busy — the obvious
suspect being the 3 s BT route poll.

Answered with the control measurement: **radio OFF, playback running**, 161.7 s cumulative window,
per-thread `utime+stime` deltas (`adb shell` wakes the core, so instantaneous reads are useless —
see the power-measurement rule).

| process | %core, radio OFF | %core, connected-idle (round i) |
|---|---|---|
| `mtkbt` (all threads, incl. `btif_rxd`) | **0.00%** | 7.95% |
| `hagodaemon` hosting `BtCommonService`/`BtTransmitterService`/`BtBle*`/`BtPlayerService` | **0.01%** | 5.50% |
| system busy | 35.77% (SoundServiceFw decoding) | — |
| system ctxt | 735/s (screen on) | — |

**The BT stack is genuinely asleep with the radio down.** So the 13.5% is link-attributable, and
the only remaining question is how much of it Cinder causes. That is now bounded by arithmetic
rather than argument:

* In steady state Cinder's *entire* BT traffic is **one zero-arg `GetBtStatus` per 3 s** —
  `refresh_bt_route()` early-returns before touching anything else once the route is unchanged and
  the device name has resolved.
* The 0.01% above is ~16 ms of service CPU across the ~11 polls that fired at the radio-off 15 s
  cadence — **~1.5 ms per round trip**.
* At the connected 3 s cadence that is ~0.5 ms/s ≈ **0.05% of a core**, i.e. about **1%** of
  `BtCommonService`'s 5.50%.

**Verdict: not ours, and not reachable from Cinder.** The other ~99% is Sony's service and the MTK
stack servicing the link itself. Dropping the poll to 10 s would save ~0.035% — noise. Task closed.

One genuine follow-up, filed rather than done: now that the volume listener is registered,
`OnNotifyAvSrcConnectionStatus` (listener slot 2) would deliver connect/disconnect **as events**,
so the 3 s poll could be retired for the connected case entirely rather than merely tuned. Worth
doing for the architecture, not for the power.

---

## Round 2026-08-11 — NFC tap-to-pair fires, and why it never did before

**`Start(0)` was never a valid call.** Round f called it, read `rc=1` as ambiguous, saw the NFC
controller appear in logcat, and concluded "Open/Start work". Only the first half was true.
`NfcService::Start` (libNfcService.so @0x7a40) decompiles to:

```c
lock();
if (state == 3) return 3;                      // already started
ret = 1;                                       // the DEFAULT is failure
if      (mode == 1) nf = 1;
else if (mode == 2) nf = 2;
else if (mode == 3) nf = 0;
else goto out;                                 // mode 0 falls straight here, ret stays 1
puts("calling NF_start2()...");
NF_start2(handle, id, nf);
state = 2; ret = 0;                            // 0 = SUCCESS
out: unlock(); return (uint8_t)ret;
```

So the valid arguments are **1, 2, 3 only**, the return is **0 = ok / 1 = rejected / 3 = already
started**, and mode 0 exits before `NF_start2` is ever reached. The controller coming up in logcat
was `Open`'s `NF_initialize`, which is why the wrong reading survived: a real side effect, produced
by the *other* call.

**Mode 1 reads tags.** Measured with `cinder-probe --nfc 30`:

```
nfc: Open() slot4 rc=0
nfc: Start(1) rc=0 (ok) -> GetCurrentMode=1
nfc: *** BLUETOOTH OOB TAG — addr=AC:80:0A:56:A9:91 (6 bytes) ***
```

A WH-1000XM4 held against the rear panel produced a callback within seconds — the first time
`OnBluetoothOob` has ever fired.

### The OOB payload, recovered rather than guessed

```
+00: 30 05 90 b3 36 05 90 b3 38 05 90 b3 | 04 04 24 00
+10: 50 05 90 b3 60 05 90 b3 60 05 90 b3 | 14 57 48 2d
+20: 31 30 30 30 58 4d 34 00 00 00 00 00 | 80 93 bf b6
```

| offset | type | value |
|---|---|---|
| +0x00 | `std::vector<uint8_t>` | the BD address — begin/end 6 bytes apart, `AC:80:0A:56:A9:91` |
| +0x0c | `uint32` | `0x240404` = class-of-device, and 0x240404 *is* the headphone CoD |
| +0x10 | `std::vector<uint8_t>` | begin 0xb3900550, end 0xb3900560 — a **16-byte** block, the OOB pairing material |
| +0x1c | `std::string` | `"WH-1000XM4"` — SSO form, length byte `0x14` = 10 << 1 |

Round f's read of the prefix (`{vector<uint8_t> addr; uint32 cod; …}`) was correct as far as it
went. The two fields it could not name are a second vector and the device name.

### Wired into cinder-home

The `libNfcService` rule said no dependency until the payload read had executed. It has, so
tap-to-pair is now in the Home app — still by `dlopen`, never a `DT_NEEDED` (`readelf -d
cinder-home | grep -i nfc` is empty, and must stay that way). The reader is armed whenever the
radio is on and stopped when it goes off. `OnBluetoothOob` runs on the framework looper, so it only
copies the address and name under a mutex; the render thread calls `Pairing(addr)` — the same call
the Devices screen's FOUND rows already use. The 16-byte block at +0x10 is read by nobody: nothing
needs it, and reading a field with no use is risk without benefit.
