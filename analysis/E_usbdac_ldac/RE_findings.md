# USB-DAC → LDAC: reverse-engineering findings (2026-06-22)

Host-side RE of the stock NW-A50 v1.02 binaries (`artifacts/rootfs_mnt/vendor/sony/lib/`),
after device probing proved: no audio-stack mutex, USB-DAC = ALSA capture→playback, LDAC
transmit = NOT ALSA, block = app-policy only. This document maps how LDAC transmit actually
works so we can drive it for USB-DAC input.

## On-device update (2026-07-25, live adb) — ALSA topology + card-index correction

Direct adb (device `10459A05194859`) while NOT in UAC mode confirmed:
- **`/proc/asound/cards` has exactly ONE card: `0 [sonysoccard]`** (the built-in cxd3778gf
  codec). Its playback PCMs: dev0 `cxd3778gf-hires-out`, dev1 `cxd3778gf-standard`, dev2
  `dsdenc`; it also exposes a capture substream `card0/pcm1c` (idle/closed).
- **There is NO `card4`.** The USB-DAC (UAC gadget) capture card is registered *dynamically*
  by the kernel **only while the gadget is in UAC mode** (`setprop sys.sony.config uac`,
  functions `audio_func,adb`, PID 0x0B8C), and it gets the next FREE index — **not guaranteed
  to be 4.** So the earlier `hw:4,0` in this doc / README / `capture.c` was fragile.
- **FIX (landed):** `ldac-bridge/src/capture.c` now has `capture_find_dev()` — it scans
  `/proc/asound/cards` for the first capture-capable card whose id is NOT `sonysoccard` and
  returns its `hw:C,D`. `main.c` calls it instead of hardcoding `hw:4,0` (env `LDAC_CAP_DEV`
  overrides; falls back to `hw:4,0` only if discovery finds nothing). Bridge still builds clean.

**Still device-gated (needs a UAC-mode session, ideally with a PC feeding audio + LDAC
headphones):** (1) the UAC capture card's actual index + whether `snd_pcm_open` returns
`-EBUSY` (stock UAC service contention — TEST.md unknown #2); (2) whether `SetCurrentSource`
opens the BtTransmitter server socket (TEST.md unknown #1). Both are unchanged by the above;
the card-discovery fix just removes the wrong-index failure mode before we get there.

## TL;DR — the transmit pipeline

```
   <audio source>                          ipcmw::ipcsocket                  MTK BT
  ┌──────────────┐   OMX capture   ┌──────────────────────┐  PcmDataParam  ┌──────────────┐
  │ Recorder /   │ ──────────────► │ AudioInRecorder       │ ─────socket──► │ BtTransmitter │ ─► LDAC
  │ AudioIn (OMX)│  CAPTURER.PCM   │ writes PCM to socket  │   (write())    │ Service       │   headphones
  └──────────────┘                 └──────────────────────┘                └──────────────┘
```

- **Producer:** `libRecorderService.so` / `libAudioInRecorderServiceClient.so` — captures
  PCM via OMX (`OMX.SONY.CAPTURER.PCM`) and writes it to a local socket using
  `pst::ipcmw::ipcsocket::PcmDataParam` framing.
- **Consumer:** `libBtTransmitterService.so` — reads PCM from the socket, encodes (LDAC/
  aptX/SBC), hands it to the MTK BT stack via `libBtCompIf.so`.
- **SoundServiceFw is NOT in the BT-transmit path.** Its renderer only targets *wired*
  `OutputDevice`s (`headphone`/`speaker`/`wmport`/`None`); there is no BT-transmit output
  device and no a2dp-source ALSA HAL. (It *does* know `a2dpsnksingletrack` = BT *receive*.)

## BtTransmitterService — the PCM entry point (the answer to the original RE question)

`libBtTransmitterService.so` — ARM ELF32, **stripped** (only 2 exported dynsyms, both
factories). It is a Sony "binder" IPC service (`pst::services::binder::*`: ServiceBase,
TransactionParam, ServiceManager, OnTransact dispatch). Imports `socket`, `socketpair`,
`__open_2`, `write`, `memcpy`, `pthread_create` — **no mmap/ashmem/ion/ALSA/OMX**, so PCM
moves over a **socket fd**, not shared memory.

PCM-feed contract (from strings + import set):
1. `NotifyOpenAudio()` — opens the audio socket, spawns the read thread.
2. `GetSocketName()` — returns the socket name the producer connects to.
3. `NotifyPcmPreferredSize(uint16_t)` — negotiates read chunk size
   (log: `Change read pcm size:%u`, guard `Over read pcm size MAX.`).
4. Producer `write()`s PCM frames to the socket; service reads + encodes
   (`Data Send Error`, `Close socket.`, `BtPlayControl(%d), pushed(%d)`).
5. `NotifyCloseAudio()` — tears down (`Close socket.`).

Built on `pst::ipcmw::ipcsocket` (`BtTransmitterExHal::OnEvent(ipcsocket::EventParam&)`).

### Socket mechanism (Ghidra-decompiled, FUN_00026c38)
The audio socket is an **abstract-namespace AF_UNIX SOCK_STREAM** socket, and the
**service is the SERVER**:
```c
socketpair(AF_UNIX,SOCK_STREAM,0,&fds);       // a control socketpair (object+0x28)
fd = socket(AF_UNIX,SOCK_STREAM,0);
addr.sun_path[0] = '\0';                       // leading NUL  -> abstract namespace
memcpy(&addr + 3, name, strlen(name));         // name copied after the NUL (offset 3)
addr.sun_family = AF_UNIX;                      // =1
setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
bind(fd, &addr, 0x6e);                          // 0x6e = 110 = sizeof(sockaddr_un)
listen(fd, 1);                                  // accepts ONE producer
// spawns a thread: accept() then recv(acceptedfd, buf, preferred_size, 0) in a loop
```
- `name` is a `std::string` member of the BtTransmitterExHal object (SSO: inline at
  `obj+1`, or heap ptr at `obj+8`), returned by `GetSocketName()`. Max len < 0x6d (108,
  the sun_path size). We don't need the literal — call `GetSocketName()` at runtime.
- The read thread `recv()`s PCM in chunks of `param_3` = the `NotifyPcmPreferredSize` value.
- **Producer contract:** `connect()` an AF_UNIX SOCK_STREAM socket to abstract name
  `"\0"+GetSocketName()`, then `write()` raw interleaved PCM in preferred-size chunks.

### Full BtTransmitterService control API (transaction methods, via the Client proxy)
Exported factories: `BtTransmitterServiceClientFactory::CreateInstance()`,
`BtTransmitterServiceFactory::CreateInstance()`. Methods (string table):
- Codec/quality: `SetLdac(bool)`, `SetLdacSoundQuality(BtLdacSoundQuality)`,
  `SetAptxHD`, `SetAptxClassic`, `SetSbcSoundQuality`, `SetEnableLowLatency(bool)`,
  `GetCapabilities(vector<BtA2dpConfiguration>&)`
  (config descriptor: `codec / channel / frequency / bit_per_sample`, each 1 byte).
- Audio pipe: `NotifyOpenAudio()`, `NotifyCloseAudio()`, `NotifyPcmPreferredSize(uint16_t)`,
  `GetSocketName()`.
- Source/state: `SetCurrentSource(bool)`, `SetCurrentTrack(vector<uint8_t>)`,
  `SetCurrentPlayStatus(BtPlayStatus,uint32,uint32)`, `SetCurrentVolume(uint8)`.
- Connection: `RequestConnection`, `RequestDisconnection`, `RequestStartConnectWait`,
  `RequestStopConnectWait`, `RequestLastDeviceConnection`, `Get*ConnectionStatus`.
- AVRCP (not needed for our path): injects media keys via `/dev/uinput`
  (`Send play key`, `Send absolute volute(%u)`, `createUinput`).

## Recorder / AudioIn — the PCM producer (the template to copy)

`libRecorderService.so` (3.3 MB) + `libAudioInRecorderServiceClient.so`. References
`BtTransmitterServiceClient` and `GetSocketName` (it is the thing that connects + feeds).
- OMX capture components: `OMX.SONY.CAPTURER.PCM`, `OMX.SONY.DECODER.DSDTOPCM`,
  `OMX.SONY.DEMUX.LPCM`. `CaptureThread.cc`, `GapRecorder_open/record/stop/updateSetting`,
  `GapAudioCapturerOMXCmp`.
- Wire format: `checkPcmParam(OMX_AUDIO_PARAM_PCMMODETYPE*, const ipcmw::ipcsocket::PcmDataParam*)`
  — validates the OMX capture PCM params against the socket's `PcmDataParam`. Also writes raw
  PCM to file (`/sdcard/wmmedia.%Y-%m-%d.%H%M%S.pcm`) for the record-to-file feature.
- `WMPortConnectionObserver` (`IsRecordable()`, `OnDeviceEnabledChanged(Device, Enabled)`)
  — can capture from the **WM-PORT**, i.e. the connector where USB-DAC audio arrives.
- Client API: `AudioInRecorderService::GetInstance() / SetUp() / GetRecordController() /
  GetController() / Suspend() / Resume() / TearDown() / Terminate() / GetMaxAmplitude()`.

## SoundServiceFw routing model (for context; not the BT-transmit path)

`libSoundServiceFw.so` (3.3 MB, full symbols). Track model: `CreateTrack(TrackType, TrackParam)`,
`SetSourceTypeTrack(SourceType)`, `OpenModulesIn(TrackType)` → `IFilterIn` + `IRendererIn`.
Renderer: `RendererDmpMaster::SwitchOutputDevice(OutputDevice)`,
`RendererDmpBase::GetAudioHalFor/SetOutputDevice(OutputDevice)`. `OutputDevice` values seen:
`headphone`, `speaker`, `wmport`, `None`. HAL getters: `AdlerAlsa` (wired/cxd3778gf),
`UacAlsaSingleTrack` (USB-DAC in), `A2dpSnkSingleTrack` (BT receive), `DualTrackMixAlsa`.
→ This is the *wired/USB-DAC/BT-receive* render engine. BT *transmit* is the separate
Recorder→BtTransmitter pipe above.

## Implementation strategies for USB-DAC → LDAC

Both reuse existing services; neither needs a kernel module or boot-image repack.

**Strategy A — reuse Recorder → BtTransmitter (preferred).** USB-DAC input is already a
capture (ALSA card4 / WM-PORT). Drive `AudioInRecorderService` to capture that source and
let its existing path forward PCM (`ipcsocket`/`PcmDataParam`) to `BtTransmitterService`
(after `NotifyOpenAudio`/`SetLdac`). This is exactly what stock does for BT transmit, with
the USB capture as the source — minimal new code. Open Qs: does the recorder expose
USB-DAC/WM-PORT as a selectable capture source, and what controller call selects it.

**Strategy B — our own producer.** A small daemon: open card4 capture (ALSA) →
`BtTransmitterServiceClient`: `NotifyOpenAudio()` → `GetSocketName()` → connect → `SetLdac`/
`SetLdacSoundQuality` → `write()` PCM (chunk = `NotifyPcmPreferredSize`, format per
`GetCapabilities`/`PcmDataParam`) → `NotifyCloseAudio()` on stop. Self-contained but
duplicates framing; needs the exact `PcmDataParam` layout. Needs the C++/libc++ ABI shim
(links libpstcore/libBtCompIf) — same toolchain boundary as baseline §5.4.

In both, the replacement player must NOT run the stock `disconnectMsgOverlay` / BT-disconnect
on USB-DAC entry (keep the LDAC link up), per CLAUDE.md H5 Step 1.

## App-policy block (HgrmMediaPlayerApp) — what a replacement player must omit
The mutual-exclusion is enforced entirely in the app (`/vendor/sony/bin/HgrmMediaPlayerApp`):
- On USB-DAC entry it shows `qrc:/window/UsbDacDeviceWindow.qml` with `id:disconnectMsgOverlay`,
  and the `1DisconnectView/Component/Model/WindowViewModel` + `GoToBluetoothSetting` flow.
- It actively tears the link down via **`IBtTransmitterService::RequestDisconnection()`**
  (strings: `BT RequestDisconnection`, `RequestDisconnection() invoke`,
  `IBtTransmitterService::RequestDisconnection() error`).
→ Our player simply does NOT gate USB-DAC on BT state and does NOT call `RequestDisconnection`.
The control plane the app DOES own (and we must reproduce): BT connect + codec
(`SetLdac`/`SetLdacSoundQuality`) — the app drives these; it does NOT feed the audio socket.

## Recorder internals (libRecorderService.so) — status
Fully stripped (Ghidra: `FUN_`/`DAT_`, PC-relative string refs not auto-resolved → low decompile
yield without manual literal-pool fixups). Confirmed it is the BT-transmit PCM producer (it +
`libRecorderServiceClient.so` are the only things referencing `GetSocketName`/`NotifyOpenAudio`)
and it captures via OMX (`OMX.SONY.CAPTURER.PCM`) and can capture WM-PORT
(`WMPortConnectionObserver::IsRecordable`). The exact "select USB-DAC/WM-PORT as source + transmit
to BT" trigger was not extracted — Strategy A remains plausible but unverified.

## DECISION: Strategy B (own producer daemon) — and why
Strategy B is the predictable path because the consumer contract is fully known (above) and
self-contained; Strategy A needs the recorder's stripped controller internals which are costly to
recover and still route through the same C++/binder layer. Plan:
1. **Data plane (high confidence, toolchain-independent C):** open the USB-DAC capture
   `hw:4,0` (card4/pcm0c, 44100 S32_LE 2ch) via tinyalsa/ioctls; connect an `AF_UNIX` `SOCK_STREAM`
   socket to abstract name `"\0"+GetSocketName()`; `write()` PCM in `NotifyPcmPreferredSize` chunks.
2. **Control plane (the hard part, needs on-device iteration):** instantiate the client via the
   exported `BtTransmitterServiceClientFactory::CreateInstance()`, then call
   `NotifyOpenAudio`/`GetSocketName`/`SetLdac`/`SetLdacSoundQuality`/`NotifyPcmPreferredSize`. These
   are virtual/binder methods (only the factory is exported) → a small C++ shim declaring the
   interface in vtable order (from Ghidra) and calling through it.
3. **Trigger/policy:** force USB-DAC gadget mode while BT stays connected (bypassing the app gate),
   or run the daemon when both states are present; never call `RequestDisconnection`.

### TOOLCHAIN BOUNDARY (critical build constraint)
The Cinder UI binary is **armv7 musl, static** — but anything linking Sony's services must match
the device ABI: **arm-linux-gnueabihf (glibc) + libc++** (the libs NEED `libc.so.6`,
`ld-linux-armhf.so.3`, `libc++.so.1`, `libcxxrt.so.1`). So the LDAC bridge is a *separate*
glibc/libc++ dynamic binary, linked against the device's `.so`s — NOT the musl toolchain.
Needs `g++-arm-linux-gnueabihf` (apt) + link stubs copied from `artifacts/rootfs_mnt`.

## Next RE targets
1. `pst::ipcmw::ipcsocket::PcmDataParam` layout (rate/format/channels/size) — find the
   defining lib (grep sony libs for `PcmDataParam` / `ipcsocket`). Needed for both strategies.
2. `AudioInRecorderService` record-controller API: how a capture **source** is selected
   (is USB-DAC / WM-PORT a selectable input?) — decides whether Strategy A is turn-key.
3. `libBtCompIf.so` — the BtTransmitter→MTK glue (sanity only; not on the hot path).
4. `HgrmMediaPlayerApp` — the exact USB-DAC-entry code that shows `disconnectMsgOverlay` +
   calls BT disconnect (so the Cinder player can simply omit it).

## Key files
- `artifacts/rootfs_mnt/vendor/sony/lib/libBtTransmitterService.so` (consumer)
- `.../libRecorderService.so`, `.../libAudioInRecorderServiceClient.so` (producer)
- `.../libSoundServiceFw.so` (wired/USB-DAC/BT-receive render engine)
- `.../libBtCompIf.so` (BT → MTK glue)
- `amixer` + `aplay` exist on-device at `/bin` (useful for a Strategy-B test harness)

---

## 2026-07-29 — Q1 ANSWERED ON DEVICE: the USB-DAC→LDAC data path is open

`cinder-probe --ldac`, headphones connected (`GetBtStatus` 3, `GetAvSrcConnectionStatus` 1):

```text
ldac: SetLdac(true) … SetLdacSoundQuality(Auto) … SetCurrentSource(true) …
ldac: GetSocketName(std::string&) …
ldac: Q1 socket name = 'pst::services::bttransmitterservice' (len 35, pump ticks 3491)
ldac: Q1 PASS — connected to the transmitter's audio socket
```

**Both halves of Q1 pass.** The control plane opens the transmitter's audio socket, and a client can
connect to it. TEST.md's third outcome ("control-plane assumption wrong → redo Ghidra") is ruled out.
Two separate bugs had to be fixed to get here, and both were in OUR code, not the RE.

### Bug 1 — `GetSocketName` was called with its arguments swapped

The probe treated it as returning a `std::string` **by value**: `fn(sret_buf, this)`. It does not.
The library states its own prototype in `.rodata`:

```text
void pst::services::BtTransmitterService::GetSocketName(pst::base::string &)
```

Void return, string by **reference** — so the old call passed a 12-byte stack array as `this`. That
is why it "threw", with libcxxrt reporting *"Fatal error during phase 1 unwinding"* and the process
dying at `PC=0`. The previous conclusion drawn from that — *"this call throws unless a BT link is
up"* — was wrong and has been struck from the notes.

`pst::base::string` is a **typedef, not a class**: the mangled form `N3pst4base6stringE` appears in no
symbol anywhere in the vendor tree, while the marshaller's own PLT entry is
`TransactionParam::GetStr(std::__1::basic_string<char, ...>&)`. It is plain libc++ `std::string`, and
`cinder-home`/`cinder-probe` compile against the libc++ 3.9.0 headers that match the device runtime —
so a real `std::string` is ABI-correct and there is nothing to hand-decode. (Same conclusion for
`pst::base::vector<T>` = `std::__1::vector<T>`, which is what task #22 needs.)

### Bug 2 — abstract socket `addrlen` IS part of the name

With the name finally in hand, `connect()` returned **ECONNREFUSED** — against a socket
`/proc/net/unix` plainly showed as listening (flags `00010000` = `SO_ACCEPTCON`).

An abstract AF_UNIX address is a **byte string** of length `addrlen - offsetof(sun_path)`, compared
exactly, trailing NULs included. `BtTransmitterService` binds with the **full `sockaddr_un`,
addrlen 110**, so its real name is the 35-character string followed by 72 NULs. Sizing `addrlen` to
`strlen()` asks the kernel for a *different* name.

```c
socklen_t len = sizeof a;   /* 110 — NOT offsetof(sun_path) + 1 + strlen(name) */
```

`/proc/net/unix` hides this: it prints the name up to the first NUL and pads the column, so the entry
looks like an exact match and the failure reads as "the server hasn't opened it yet" — which sends
you off hunting the wrong trigger. `od -c` on that line is what shows it:

```console
$ adb shell cat /proc/net/unix | grep -a bttransmitter | sed 's/.*@//' | od -c
0000000   p s t : : s e r v i c e s : : b t t r a n s m i t t e r s e r v i c e
0000043  \0  \0 …                                     # 107 bytes total = 110 - 2 - 1
```

**Rule:** ECONNREFUSED against a socket listed with `SO_ACCEPTCON` is an addrlen/name mismatch, not a
missing listener.

### What is still open

**Q2 — capture contention — remains unanswered**, and cannot be answered from an adb shell: it needs
the gadget in `uac` mode with a PC actually feeding audio, and entering `uac` changes the USB identity
(idProduct `0B8C`), which drops adb. So Q2 is a UI-driven test from inside cinder-home, with the
answer read out of `/contents/cinderhome.log` afterwards.

The standalone `ldac-bridge` daemon is **retired** as a delivery vehicle. Its own banner already said
why — it starts no `pst::core::Framework`, so nothing pumps the looper and every client call returns
uninitialised stack. cinder-home is an easel app with a live framework and an already-working
`BtTransmitterServiceClient`, so the pipeline belongs there. `ldac-bridge/src/` stays as the reference
implementation of the socket writer and the ALSA capture loop.

---

## 2026-08-11 — the USB gadget has TWO owners, and that is the USB-DAC/MSC bug

Reported: *"still have issues with USB mass storage mode, and I'm not sure USB-DAC mode has ever
output audio."* Both turn out to be the same defect, and it is not in Cinder's logic — it is that
**two independent subsystems configure the USB gadget and neither knows about the other.**

### The two owners

| owner | mechanism | what it sets | what it does NOT set |
|---|---|---|---|
| **init** (`/init.usbcfg.rc`) | `on property:sys.sony.config=adb\|uac\|msc` | `functions`, `idVendor`, `idProduct` (hardcoded `0B8B`/`0B8C`/`0B8D`), **`f_mass_storage/lun/file`**, and starts/stops `mount_msc1`/`unmount_msc1` | — |
| **`UsbMgrServiceFw`** (hagodaemon, `UsbMgrImplWmport`) | `SetUsbFunction(UsbFunction)` → `UpdateUsbFunction()` → `SetUac()`/`SetMsc()` | `idVendor`, `idProduct` (from `DmpFeature::GetFeatureUsbVid/PidUac\|Msc`), `functions`, `MaxPower` | **`lun/file`, the `/contents` unmount, and the property** |

Read live on device, and the disagreement is plain — `cinder-probe --usbmgr`, 2026-08-11:

```
service  GetUsbFunction = 2  (MSC)      <- what UsbMgrServiceFw believes
gadget   functions=mass_storage,adb  enable=1  054c:0ca0
msc      lun/file=<empty>   (sys.usb.msc1=/emmc@contents)
init     sys.sony.config=adb   sys.usb.state=adb    <- what init believes
init.svc.mount_msc1 = stopped ; /contents still mounted rw device-side
```

**The service says MSC, init says adb, and the medium was never attached.** `idProduct=0ca0`
matches neither `0B8B`/`0B8C`/`0B8D`, confirming the service — not init — wrote the gadget last.
This is no longer an inference from the disassembly; it is the measured state of the unit.

`0ca0` comes from `DmpFeature`, so **`UsbMgrServiceFw` wrote the gadget last** and it believes the
function is MSC. Meanwhile init's `adb` branch had already cleared `lun/file`.

### Why mass storage is broken

MSC needs four things, and they are split across the two owners:

1. `umount /contents` — `unmount_msc1`, **init only**
2. `lun/file = /emmc@contents` — **init only**
3. gadget descriptor `functions=mass_storage,adb` + VID/PID — either owner
4. on exit: clear `lun/file`, `mount_msc1` to remount `/contents` — **init only**

Only (3) is currently happening. The host therefore enumerates a **USB Mass Storage device with no
medium** — a drive that appears and then reports no disk. That is exactly the reported symptom, and
no amount of retrying in Cinder fixes it, because Cinder is driving the half that doesn't own the
medium.

Verified from the init scripts on device:

```
service mount_msc1   /system/bin/mount_partition contents
service unmount_msc1 /system/bin/umount /contents
```

Both `disabled` + `oneshot`, i.e. reachable **only** from init's property triggers.

### Why USB-DAC has probably never output audio

Same mechanism, worse consequence. Cinder sets `sys.sony.config=uac` through its setuid helper, so
init writes `functions=audio_func,adb`, PID `0B8C`. But `UsbMgrServiceFw` still holds
`UsbFunction=Msc`, and **any** of its own triggers — cable insert, `OnDeviceConnectedChanged`,
`OnDeviceEnabledChanged`, `Resume()`, `ReconfigureUsbOtgMode()` — calls `UpdateUsbConfig()` →
`UpdateUsbFunction()` and **rewrites the gadget back to mass storage**. The PC then never
enumerates a sound card, or enumerates one that vanishes.

`UpdateUsbFunction()` also `ExecuteCommand`s a **stop of adbd** around the switch (string
`"!!! failed to stop adbd"` on the failure path), so the whole reconfiguration is disruptive and
racy against init doing the same thing from the property.

### The fix: use the owner Sony uses

`UsbMgrServiceFwClient::SetUsbFunction` is a real IPC method, and the whole client vtable came out
clean — this library keeps `R_ARM_ABS32` relocations that name every slot, so nothing is inferred:

| slot | method |
|---|---|
| 0/1 | dtors |
| 2 | `GetServiceName() const` |
| **3** | **`SetUsbFunction(const ReqMsg_SetUsbFunction&, RspMsg_SetUsbFunction&)`** |
| **4** | **`GetUsbFunction(const ReqMsg_GetUsbFunction&, RspMsg_GetUsbFunction&)`** |
| 5/6 | `Set`/`GetUsbOtgMode` |
| 7/8 | `Set`/`GetPowerSuppliedModeFromUacHost` |
| 9 | `GetCurrentPowerSuppliedMode` |
| 10/11 | `Set`/`GetAdbEnabled` |
| 12/13 | `AddListener` / `RemoveListener` |
| 14 | `GetName() const` (15 = `0xfffffffc`, the secondary-vtable marker) |

`AddListener` sits immediately after the last service method for the **fourth** service in a row
(BtCommon 30, BtTransmitter 39, UacPlayer 6, UsbMgr 12). Treat that as a law of this codebase.

**Note the calling convention differs from the Bt clients.** These take Req/Rsp *message structs*,
not bare scalars. Both are trivially small, read straight out of the marshalling:

* `SizeOfReqMsg_SetUsbFunction` → returns **4**
* `WriteReqMsg_SetUsbFunction` → one `TransactionParam::Alloc(4)`, then copies **one word from
  offset 0** of the ReqMsg
* `SizeOfRspMsg_SetUsbFunction` → returns **4**
* `SizeOfRspMsg_GetUsbFunction` → returns **8** — the GET reply is *two* words

```c
struct ReqMsg_SetUsbFunction { uint32_t function; };                  /* 4 */
struct RspMsg_SetUsbFunction { uint32_t result;   };                  /* 4 */
struct RspMsg_GetUsbFunction { uint32_t result; uint32_t function; }; /* 8 — value at OFFSET 4 */
```

> The offset-4 detail was caught by the device, not the disassembly. The first `--usbmgr` run
> printed `rsp 0 2 0 0` with the gadget sitting in mass storage; reading `rsp[0]` would have
> reported "function 0 (??)" forever. The size functions then confirmed it (8 vs 4). **Print the
> raw words on a first bring-up** — a getter that reports a plausible zero is the worst failure
> mode on this platform.

**The enum values are read from the dispatch switch**, not guessed —
`UsbMgrImplWmport::UpdateUsbFunction()` at `0x10f80`:

```
r0 = [r4+0x2c]        ; the requested UsbFunction
cmp r0, #2  -> SetMsc()
cmp r0, #1  -> SetUac()
else        -> log "invalid"
```

**`UsbFunction: 1 = UAC (USB-DAC), 2 = MSC.`**

`SetUac()` and `SetMsc()` are structurally identical — `GetFeatureUsbVid{Uac,Msc}` → write
`idVendor`, `GetFeatureUsbPid{Uac,Msc}` → write `idProduct`, write `functions`, write `MaxPower`.
**Neither touches `lun/file`**, which is what makes the split above load-bearing:

> **Switching to MSC needs BOTH owners.** `SetUsbFunction(2)` for the descriptor so `UsbMgr` stops
> fighting, and the init property for the unmount + `lun/file` handoff. Switching to UAC needs
> `SetUsbFunction(1)` so the gadget is not reverted underneath us.

### And a second, independent USB-DAC bug: `Start()` is called too early

`UsbDeviceAudioPlayerServiceClient`'s vtable also came out clean, and it **confirms Cinder's
existing slot guesses** (they were right):

| slot | method |
|---|---|
| 2 | `GetServiceName() const` |
| 3 | `GetStatus(stream_info_t&)` |
| **4** | **`Start(stream_info_t&)`** |
| **5** | **`Stop()`** — *takes no argument; Cinder passes one. Harmless under AAPCS, still wrong.* |
| 6/7 | `AddListener` / `RemoveListener` |
| 8 | `GetName() const` |

The important part is the **direction** of `Start`'s parameter: the ref is non-const, i.e. an
**out** param. The service does not take the format from us — `UsbAudioStreamMonitor` learns it from
a hotplug socket (`UacInitHotplugSock`, `RecvUACEvent`, `ParseStreamInfo(const std::string&,
stream_info_t&)`) and only then can `UsbAudioPlayerCore::StartPlaying` open
`OpenInhal` (UAC capture) + `OpenExhal` (an AudioTrack to the 3.5 mm chain).

Cinder calls `Start()` **once, at the moment the user toggles USB-DAC** — before any PC is
streaming. At that instant there is no valid stream_info, so `StartPlaying` has nothing to open,
and **nothing ever retries**. That alone is sufficient to explain "recognised in audio, no output".

The service publishes exactly the event needed, and the dispatcher gives its index without
ambiguity. `UsbDeviceAudioPlayerServiceListenerProxy::OnChangedFormatBase` at `0x23148`:

```
r0 = [r4+0x24]     ; the user listener object
r2 = sp+16         ; a 12-byte struct (three words unpacked from the transaction)
r1 = [r0+0]        ; its vptr
r3 = [r1+8]        ; vptr[2]
r1 = sp+28         ; one further word
blx r3
```

**`OnChangedFormat` is listener slot 2**, called as `(this, const uint32_t&, const struct{u32,u32,u32}&)`.
Five `TransactionParam::Get(4)` calls feed it; the first return is discarded by the service itself.

So the correct shape is: register a listener at DAC entry, and call `Start()` **when the format
arrives**, not when the user flips the switch.

### Confirmed healthy on device (so these are not the problem)

* `/sys/class/android_usb/android0/f_audio_func/` exists and is populated:
  `f_valid=1 f_allow=1 f_start=180 f_thresh=50 f_plus=1 f_minus=1`.
  The UAC gadget function is compiled in and initialised.
* The service is running: `hagodaemon UsbHostConnectionService UsbDeviceConnectionService
  UsbDeviceAudioPlayerService … capabilities=1,12 nice=-10` (pid 311).
* Only card 0 (`sonysoccard`) exists while not in UAC mode, which is expected — the UAC capture
  card only appears once the gadget is in `audio_func` **and** the host is streaming.

### Order of work this implies

1. `cinder-probe --usbmgr` — read `GetUsbFunction` (slot 4) first. Read-only, settles whether the
   service really believes MSC while the UI says otherwise.
2. Switch via `SetUsbFunction` (slot 3) instead of the property alone; keep the property for the
   MSC medium handoff only.
3. Register the `OnChangedFormat` listener and move `Start()` behind it.
4. Only then is USB-DAC → LDAC a data-path problem rather than a control-plane one.

### `stream_info_t`, from the service's own formatter

`Utils.cc` in `libUsbDeviceAudioPlayerService.so` carries the printer, so the field set is read
rather than guessed:

```
--- stream_info_t: %s ---
action: kActionStop | kActionPlay
format: %s          -> kFormatNone | kFormatPCM | kFormatDSD | kFormatDOP   (0,1,2,3 in .rodata order)
freq: %u Hz
bitwidth: %u bits
```

Four fields, which is exactly the four values `OnChangedFormatBase` unpacks (five
`TransactionParam::Get(4)` calls, the first return discarded by the service). It stores them at
`sp+16, +20, +24` and `sp+28`, then calls the listener with `r1 = sp+28` (one word) and
`r2 = sp+16` (three words) — so implement the callback as

```c
virtual void OnChangedFormat(const uint32_t& a, const uint32_t (&b)[3]) { ... }
```

and read no further than that; anything beyond is not written by the dispatcher.

`kFormatDOP` and the `LibDsdToPcmConv_*` / `LibDsdCrossFade_*` imports confirm the service handles
DSD-over-PCM from the host, and `snd_pcm_readi` confirms the input half is an ordinary ALSA
**capture** from the UAC card — which is what a USB-DAC → LDAC bridge would tap.

Incidental: `FileWriter.cc` will `mkdir` and dump to **`/contents/UDAP`** — a debug capture path
that exists in the shipped service. Untested, but it is a free way to prove PCM is arriving if the
audible path ever stays silent.

### Bring-up note: the restore child must leave the session (2026-08-11)

`cinder-probe --usbmgr uac` forks a child that puts the previous `UsbFunction` back after a window,
so a switch made over adb can always be undone. The first version of that child **stayed in the
parent's process group and session** — and switching to UAC re-enumerates the gadget, which kills
adbd, which SIGHUPs the group. The restore child died with the shell it existed to rescue, and the
device sat in UAC with no adb until it was power-cycled from the Windows side.

This is the boot-escape ladder's rule (*an escape must depend on strictly less than the thing it
rescues*) reappearing one layer down. The child now does, **before anything else**:

```c
signal(SIGHUP, SIG_IGN); signal(SIGINT, SIG_IGN); signal(SIGTERM, SIG_IGN);
setsid();
```

Anything that reconfigures the gadget, the radio, or the display over adb needs the same treatment.

---

## Round k (2026-08-11) — the gate, and the third owner

Two rounds of on-device testing ended the same way: the gadget switched, the PC enumerated a sound
card (`card4 [UAC2Gadget]`, `04-00: UAC2 PCM : capture 1`), the `OnChangedFormat` listener
registered with `rc=0` — and then `GetStatus format=0` for the entire session. Calling `Start()`
harder was never going to fix it, because the service was not being told anything at all.

### How the service learns a format (static RE of `libUsbDeviceAudioPlayerService.so`)

It does **not** poll, and it does **not** learn from ALSA. The chain is three links:

1. **connmgr says the device is enabled.**
   `UsbAudioConnectionMonitor::Open` (`0x1e1d0`) calls
   `funcarch::connmgr::ConnMgrService::GetDeviceStatus(Device = 7, DeviceStatus&)` and registers a
   `DeviceListener`. Every change lands in `UsbAudioPlayerCore::NotifyChangeConnectionStatus`
   (`0x16c44`), and that function is the gate:

   ```
   if (status == 1)  UsbAudioStreamMonitor::Open();      // opens the socket, starts the thread
   else              ClearStreamInfo(); StopPlaying();   // and tears it all down
   ```

   Nothing else opens the stream monitor.

2. **The stream monitor's socket.** `UsbAudioStreamMonitor::UacInitHotplugSock` (`0x1ebf4`),
   instruction for instruction:

   ```c
   socket(AF_NETLINK /*16*/, SOCK_DGRAM /*2*/, 24);            // proto 24 — MTK/Sony private
   setsockopt(fd, SOL_SOCKET, SO_RCVBUFFORCE /*33*/, &2048, 4);
   bind(fd, &(struct sockaddr_nl){ .nl_family = 16, .nl_pid = getpid(), .nl_groups = 1 }, 12);
   ```

3. **The kernel sends the format.** `RecvUACEvent` (`0x1ef00`) `recvmsg()`s into a 2048-byte
   buffer, **skips the first 16 bytes** (the `nlmsghdr`), splits the rest on `'\n'`/`'\r'`, and
   feeds each line to `ParseStreamInfo`, which matches `ACTION=` (`STOP`/`PLAY`/`NONE`), `FORMAT=`,
   `FREQ=` (table `32000 … 11289600`) and `BITWIDTH=`.

`UsbAudioPlayerInhal` then opens **`hw:4,0`** — hardcoded — for capture, and drives
`/sys/class/android_usb/android0/f_audio_func/{f_allow,f_valid,f_start,f_thresh,f_plus,f_minus}`
for its feedback control.

### `funcarch::connmgr::ConnMgrService` (libConnMgrService.so)

Stateless — the ctor at `0x60bc` only touches the stack guard; every method re-fetches the client
with `Framework::GetServiceClient("ConnMgrServiceFw")` (a 16-char name) and calls it by vtable
index. Slot 3 = `GetDeviceStatus`, slot 4 = `GetUsbHostSuspended`. Reply is
`{ uint32 result; DeviceStatus }`, and `DeviceStatus` is 8 bytes — one `vst1.8 {d16}` out of the
reply at offset 4 — i.e. `{ uint32 enabled; uint32 connected; }`.

`Device` enum, from `ConnMgrServiceFw`'s own dump strings, **confirmed on device** (index 12 is the
`Invalid` terminator and `GetDeviceStatus` rejects it with `rc=1`; index 6 read enabled with the
gadget at `mass_storage,adb`):

| # | name | # | name | # | name |
|---|---|---|---|---|---|
| 0 | LineIn | 4 | UacDevice | 8 | AvrcpTg |
| 1 | BtlHeadphone | 5 | A2dpSink | 9 | HostCable |
| 2 | SeHeadphone | 6 | MscHost | 10 | SdCard0 |
| 3 | LineOut | 7 | **UacHost** ← the gate | 11 | SdCard1 |
|   |  |   |  | 12 | Invalid (sentinel) |

### Measured: every link was dark (`cinder-probe --uacgate`, 2026-08-11)

Gadget at `mass_storage,adb`, `sys.sony.config=adb`, `lun/file` empty:

```
socket(AF_NETLINK, SOCK_DGRAM, 24) failed: Protocol not supported
device  6 (MscHost )    enabled=1 connected=1
device  7 (UacHost )    enabled=0 connected=0     <-- the gate
GetUsbHostSuspended = 0
f_audio_func f_allow=1 f_valid=1 f_start=180 f_thresh=50 f_plus=1 f_minus=1
```

Netlink protocol 24 **does not exist** unless the UAC function is loaded, so the socket open is
itself a clean yes/no on the gadget's real state. And `f_valid=1` while the gadget is in mass
storage proves those sysfs nodes are persistent configuration, not a streaming indicator — do not
read them as one.

### The third owner: `UsbDeviceConnectionService::SetDeviceType`

`libUsbDeviceConnectionService.so` exports the client factory
`_ZN3pst8services39UsbDeviceConnectionServiceClientFactory14CreateInstanceEv`. Vtable, recovered
from `R_ARM_ABS32` relocations (no inference):

| slot | method |
|---|---|
| 2/3 | `~UsbDeviceConnectionServiceClient` D2 / D0 |
| 4 | `GetServiceName() const` |
| **5** | **`SetDeviceType(const device_type_t&)`** |
| 6 | `GetConnectionStatus()` |
| 7 | `GetUsbSuspend()` |
| 8 | `DisableConnection(const bool&)` |
| 9 | `SetOtgType(const otg_type_t&)` |
| 10 | `AddListener(IServiceListener*, const std::string&)` |
| 11 | `RemoveListener(IServiceListener*)` |

`AddListener`/`RemoveListener` are the last two again — that is now six services in a row.
`SetDeviceType` takes the enum **by const reference** (a pointer to one word), not the Req/Rsp
pattern.

`device_type_t`, from the `cmp #1 / #2 / #3` dispatch in `UsbDeviceConnectionMonitor::SetDeviceType`
(`0xab98`), with the branch targets resolved through their Thumb→ARM veneers to PLT entries:

| value | handler | what it actually does |
|---|---|---|
| 1 | `SetDeviceTypeAdb` (`0xb0ed`) | `stop adbd` → ids → `start adbd` → **`start mount_msc1`** |
| 2 | `SetDeviceTypeMsc(bool)` (`0xac5d`) | **`start unmount_msc1`** → `stop adbd` → read `sys.usb.msc1` and write it into `f_mass_storage/lun/file` → `functions=mass_storage,adb` + ids → `start adbd` |
| 3 | `SetDeviceTypeUac` (`0xaed5`) | `stop adbd` → read `sys.usb.vid` → `functions=audio_func,adb` + ids + `enable` → `start adbd` → **`start mount_msc1`** |

Two bugs collapse into that table:

* **Mass storage.** `unmount_msc1` plus writing `sys.usb.msc1` (`/emmc@contents`) into `lun/file`
  **is** the medium, and nothing outside this service does it. `UsbMgrServiceFw::SetUsbFunction`
  sets the descriptor only — hence "a drive with no disk", every time.
* **USB-DAC silence.** The gadget rewrite re-enumerates in a way `UsbDeviceConnectionMonitor`'s own
  uevent thread is watching (`NotifyUEventMessage` → `UpdateStatus` → `NotifyUacConnectEvnet`),
  which `ConnMgrServiceFw` republishes as device 7 enabled — link 1 of the chain above. Switch the
  gadget any other way and the connect event never fires, the socket is never opened, and the
  service reports `kFormatNone` forever no matter how often `Start()` is called.

So the ownership picture is not two writers but **three**, and only the third is complete:

| owner | descriptor | medium (`lun/file`, mount) | connect event |
|---|---|---|---|
| init, via `sys.sony.config` | yes | yes | no |
| `UsbMgrServiceFw::SetUsbFunction` | yes | **no** | **no** |
| `UsbDeviceConnectionService::SetDeviceType` | yes | yes | **yes** |

No root required for any of it: `start mount_msc1` runs inside the service.

`cinder-home` now calls `SetUsbFunction` (so the mode manager's belief stays in sync and its resume
trigger cannot revert us) and then `SetDeviceType` **last**, so the complete owner's writes win.
USB-DAC off goes to `Adb`, not `Msc` — `Msc` unmounts `/contents` and would take the music library
out from under the player. Mass storage stays a separate, explicit user action.

Probes: `cinder-probe --uacgate [secs] [delay] [engage]` (read-only by default; `engage=1` does the
switch itself behind an armed restore child) and `cinder-probe --usbdt uac|msc|adb [restore_secs]`
for the raw switch.

### First on-device run of SetDeviceType: a no-op, then a reboot (2026-08-11)

`cinder-probe --uacgate 30 6 1` (engage mode, restore child armed) produced the cleanest possible
negative result:

```
usbdt: SetDeviceType(3 = Uac) rc=0
usbmgr: gadget   functions=mass_storage,adb enable=1 054c:0ca0     <-- unchanged
device  7 (UacHost )    enabled=0 connected=0                      <-- unchanged
socket(AF_NETLINK, SOCK_DGRAM, 24) failed: Protocol not supported  <-- unchanged
```

Sampled every 5 s for 30 s: **nothing moved**. The gadget composition, the ids, connmgr's device
table and the netlink protocol were all exactly as before. Then, a couple of minutes later, the
**device rebooted** — recovering onto Cinder by itself, with `/contents/uacgate2.log` intact, so the
whole run is on record. The pre-reboot logcat is not: the ring only holds the new boot.

Two corrections fall out of that run:

* **`rc` is meaningless.** The client proxy's return was read as an `int`; the tail of
  `SetDeviceType` (`0xdd2a`…) does no such thing. Treat `rc=0` as "we do not know", not "OK".
  (`funcarch::ConnMgrService::GetDeviceStatus` genuinely does return 0 = OK — different function,
  different convention. Do not generalise either one.)
* **hagoromo8 is running and does host the service** (`ps`: pid 282,
  `hagodaemon UsbHostConnectionService UsbDeviceConnectionService UsbDeviceAudioPlayerService`),
  so "service absent" is ruled out.

What the boot log then showed is the more interesting half:

```
UMGR|UsbMgrImplWmport.cc:525] UsbFunction [Msc] is set
UDCS|UsbDeviceConnectionMonitor.cc:544] Detect change connection status : 1
UDCS|UsbDeviceConnectionService.cc:125] Fires OnConnectEvent : 2
```

The monitor fires the **generic** `OnConnectEvent`, not `OnUacConnectEvent`/`OnMscConnectEvent`, and
`UsbMgrServiceFw` separately announces `UsbFunction [Msc]`. That points at a cheaper mechanism than
re-driving the whole gadget: the glue most likely combines "a connection event happened" with "what
does UsbMgrServiceFw say the function is" to decide device 6 vs device 7. If so, the earlier rounds
were nearly right — `SetUsbFunction(Uac)` did stick — and the only missing piece is that **no
connect event re-fires after the function changes**, so connmgr never re-evaluates.

That makes `UsbDeviceConnectionServiceClient::DisableConnection(const bool&)` (vtable slot 8) the
next thing to try: `true` then `false` should drop and re-raise the connection and force the glue to
re-decide, without rewriting the gadget at all. Next test, not yet run.

Until that is understood, `cinder-home` does **not** call `SetDeviceType` — a Home-app toggle that
can reboot the player is worse than one that does nothing. The code and the vtable stay, gated on
`/contents/cinder-usbdt.on`; `cinder-probe --usbdt` remains the place to experiment.

Also added as a consequence: **`cinder-msc usb-rescue`**. Every writer of this gadget does
`enable 0 -> ids -> functions -> enable 1`, so any of them dying in the middle leaves `enable=0`
and *nothing* on the host bus — no adb, no MSC, no UAC. The rescue re-drives init's `adb` block and,
failing that, writes `enable 1` directly; it runs once at cinder-home startup and after every USB
mode change, and is a no-op when the gadget is healthy.

---

## Round l (2026-08-11) — FuncMode: the gate was never the gadget

The `DisableConnection` hypothesis at the end of round k was wrong, and so was the premise under it.
Following the glue one library further answers the whole question, and the answer is not on the USB
side at all.

### The chain, end to end

`libConnMgrServiceFw.so` is the missing link — it is the only thing in the rootfs besides the
service itself that references `OnUacConnectEvent` on the *device* interface. Two of its classes
matter, and Sony has their names crossed over: `UsbHostListener` listens to
`IUsbDeviceConnectionService` (we are the device, the PC is the host), while `UsbDevListener`
listens to `IUsbHostConnectionService` (something is plugged into us). We want the former.

`UsbHostListener`'s vtable (`0x26450`, recovered from the R_ARM_ABS32 relocations) shows it
overrides only **two** slots:

| slot | method |
|---|---|
| 4 | `UsbHostListener::OnConnectEvent(connection_status_t const&)` |
| 5 | `IUsbDeviceConnectionService::IServiceListener::OnUacConnectEvent` — **base no-op** |
| 6 | `IUsbDeviceConnectionService::IServiceListener::OnMscConnectEvent` — **base no-op** |
| 7 | `UsbHostListener::OnChangeUsbSuspend(usbsuspend_status_t const&)` |

So connmgr never hears `OnUacConnectEvent` at all. It hears the generic event and then decides for
itself, in two functions:

```
ConnGlueUsbHost::CnvConnected(Device, int status, bool& out)          @0x19ed0
    dev == 7 (UacHost) : out = (status == 4 || status == 2)
    dev == 6 (MscHost) : out = ((status & ~1) == 2)        i.e. status 2 or 3
    otherwise          : out = false

ConnGlueUsbHost::CnvStatus(Device, bool connected, FuncMode, DeviceStatus& out)   @0x19f24
    if (!connected)     out = { 2, 0 };
    else if (dev != 7)  out = { 2, 1 };
    else                out = { 2, (FuncMode == 1) };
```

and `UsbAudioConnectionMonitor::Open` (@0x1e1d0) does `GetDeviceStatus(Device{7}, st)` — `movs r0,#7`,
so device 7 is confirmed by disassembly, not inference — and believes it only when the word it reads
back is `1`.

**Device 7 is an AND, and we only ever had one input.** The connect event was arriving; `FuncMode`
was never 1, so `CnvStatus` threw the event away. That is why `SetUsbFunction`, `sys.sony.config`
and `SetDeviceType` all produced nothing: none of them is on this path.

### connection_status_t, finally pinned

`UsbDeviceConnectionMonitor::UpdateStatus(int)` (@0xbb88) is the whole decision, and it is driven by
`NotifyUEventMessage` — a kernel uevent, not an API call. `this+0x1c` is the stored device type,
`this+0x08` the cached status, `this+0x04` the service. Reading `[svc_vtbl + 68/72/76]` against the
service vtable at `0x13640` (the stored pointer is past the 8-byte RTTI header, so `+68` is slot 19)
names them exactly:

| raw uevent | stored device type | call | status |
|---|---|---|---|
| 1 | 1 Adb | `NotifyConnectEvnet` (slot 21) | 2 |
| 1 | 2 Msc | `property_get` + `WriteUsbSetting` + `NotifyMscConnectEvnet` (19) | 3 |
| 1 | 3 Uac | `NotifyUacConnectEvnet` (slot 20) | 4 |
| 2 | (CheckDeviceTypeMsc) | `NotifyMscConnectEvnet` | 5 |
| 0 | previous 2 / 3 / 4 | the matching Notify\*, argument 1 | 1 |

So `connection_status_t` = 1 disconnected, 2 Adb, 3 Msc, 4 Uac, 5 Msc-configured. This predicts the
boot log line `UDCS|UsbDeviceConnectionService.cc:125] Fires OnConnectEvent : 2` exactly — device
type Adb, cable present — which is the independent check that the vtable offsets are right.

### FuncMode

`funcarch::GetName(FuncMode const&)` (@0x7e00, `libFuncMgrServiceFw.so`) builds a
`std::map<FuncMode,const char*>` inline: eight `strd {key, ptr}` pairs into a 64-byte stack array,
keys 0..7, against the contiguous `.rodata` run at `0xba69`. Read off the binary, not guessed:

| 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| MediaPlay | **UsbDac** | A2dpSink | Fm | DirectRec | Dmr | Dms | Initial |

`Invalid` is the ninth string but not a key — `GetCurrentFuncMode` returns `9` of its own accord
when the binder call fails, and `GetName` falls back to it. Two independent confirmations that
`UsbDac == 1`: the string order, and `CnvStatus`'s `cmp r1, #1`.

`FuncMgrServiceServiceImpl::EnterFuncMode` (@0x7fb4) is what stock runs when the user picks USB-DAC
in Settings, and it is three calls under one mutex:

```
mutex.lock()
if (mode == current) return                              ← early-out, so calling twice is harmless
FireRequireExitFuncMode(current)                         ← listeners may veto
usbmgr::UsbMgrService::SetUsbFunction(...)               ← the only step we were doing
connmgr::ConnMgrService::SetDeviceHandleRules(...)       ← publishes device 7
pathmgr::PathMgrService::SetPath(...)                    ← the audio routing path
log "FuncMode [%s] transition is completed"
mutex.unlock()
```

`SetPath` is worth staring at: even a perfect gadget and a perfect connect event would still have
left the audio unrouted, because nothing we called ever touched `PathMgrService`.

The client is `pst::services::funcarch::funcmgr::FuncMgrService` in `libFuncMgrService.so` and it is
the easy kind — plain exported methods, no factory, no vtable index to guess, and **stateless** in
exactly the way `funcarch::connmgr::ConnMgrService` is: the ctor at `0x5d84` only reads the stack
guard, and every method re-fetches the client through
`Framework::GetServiceClient("FuncMgrServiceFw")`. A dummy `this` is legitimate.

```
_ZN3pst8services8funcarch7funcmgr14FuncMgrService18GetCurrentFuncModeEv     -> int  (9 on failure)
_ZN3pst8services8funcarch7funcmgr14FuncMgrService13EnterFuncModeERKNS1_8FuncModeE   -> bool
_ZN3pst8services8funcarch7funcmgr14FuncMgrService15SetBootFuncModeERKNS1_8FuncModeE -> bool
```

### Measured on device (2026-08-11, `cinder-probe --funcmode`, read-only)

```
funcmode: current = 0 (MediaPlay)
device  6 (MscHost )  enabled=1 connected=1
device  7 (UacHost )  enabled=0 connected=0     <-- the gate
device 10 (SdCard0 )  enabled=1 connected=1
gadget  functions=mass_storage,adb enable=1 054c:0ca0
```

`FuncMode = 0` with a healthy gadget and adb up. The static read predicted precisely this. Rounds
d through k were all measuring the same closed valve from the wrong side.

### What landed

* `cinder-probe --funcmode` — read-only with no argument (prints FuncMode, all 13 connmgr devices,
  the gadget, the `f_audio_func` nodes and `/proc/asound`), and `--funcmode <n> [restore] [watch]`
  to make the transition. The engage form self-detaches (`fork` + `setsid` + `SIG_IGN`) before it
  touches anything, because `SetUsbFunction` re-enumerates the gadget, adbd bounces, and a probe
  that is still a child of adbd would be SIGHUP'd mid-experiment — taking its own restore with it.
  A second detached child re-enters `MediaPlay` on a timer regardless of what happens to the parent:
  same rule as the boot ladder, the escape depends on less than the thing it rescues.
* `usb_enter_func_mode()` in `cinder-home`, via `dlopen` rather than a `DT_NEEDED` — the
  libNfcService rule; nothing on this path runs at boot, so it must not be able to break boot.
  Wired into `apply_usb_dac` **before** the property write, for the same reason `SetUsbFunction`
  goes there: `EnterFuncMode` installs the service's audio-only descriptor internally, and letting
  init's `audio_func,adb` land afterwards is what keeps adb alive through DAC mode.
* Opt-in for now, on `/contents/cinder-funcmode.on`. The *gate* is confirmed on hardware;
  `EnterFuncMode` itself has not been executed on hardware yet, and the last unverified service call
  in this file preceded a reboot. One clean `--funcmode 1` run retires the marker — delete the
  `::access` line, nothing else changes.

Corrected from round k: the closing hypothesis there — that `DisableConnection(true/false)` would
re-fire the connect event and make the glue re-decide — would not have worked. The glue would have
re-decided with `FuncMode` still 0 and reached the same answer.

### CONFIRMED ON HARDWARE — `cinder-probe --funcmode 1 120 45`, 2026-08-11

One run, every link moved together and moved back:

| | before | after `EnterFuncMode(1)` | after `EnterFuncMode(0)` |
|---|---|---|---|
| `GetCurrentFuncMode` | 0 MediaPlay | **1 UsbDac** | 0 MediaPlay |
| connmgr device 7 (UacHost) | enabled=0 connected=0 | **enabled=1 connected=1** | enabled=0 connected=0 |
| connmgr device 6 (MscHost) | 1/1 | 0/1 | 1/1 |
| gadget | `mass_storage,adb` `054c:0ca0` | **`audio_func` `054c:0b8c`** | `mass_storage` `054c:0ca0` |
| `socket(AF_NETLINK,SOCK_DGRAM,24)` | ENOPROTOOPT | **bound** | — |
| `/proc/asound` | card0 only | **card4 pcm0c present** | — |

Stable across all eight 5 s beats of the 45 s window. `hw:4,0` — the capture device
`UsbAudioPlayerInhal` hardcodes — exists in UsbDac mode and in no other mode. Four rounds of
"USB-DAC enumerates but is silent" are explained and the control plane is closed.

Three things the run taught that the disassembly did not:

* **`EnterFuncMode`'s bool is not a result.** It returned `false` for the switch *and* for the
  restore, on the run where all five readings above prove both worked. Exactly the same trap as
  `UsbDeviceConnectionServiceClient::SetDeviceType`'s `rc`. Judge these calls by read-back, never by
  return value; `funcmode_enter` now calls `GetCurrentFuncMode` and reports that instead.
* **Neither mode's descriptor carries adb.** UsbDac is bare `audio_func`, MediaPlay is bare
  `mass_storage` — so a probe that switches and stops there leaves the device with *no adb at all*,
  recoverable only by a reboot, which is what happened on the first run. `funcmode_recompose_adb`
  (re-drives init's `sys.sony.config adb`, the same lever as `cinder-msc usb-rescue`) now runs after
  every restore, in the in-process path and in the detached child. cinder-home never had this
  problem: `apply_usb_dac` already writes the property after, which is why the ordering there is
  FuncMode → SetUsbFunction → property and not any other way round.
* **Zero netlink FORMAT events, and that is the environment, not the chain.** The player was
  attached to WSL through usbipd for adb, so Windows never enumerated it as a sound card and nothing
  was ever streaming into it. The socket existed and had a reader; there was simply no host. Closing
  the last link needs the player plugged into a PC *without* the passthrough — at which point the
  events land on a socket we can already open, and `hw:4,0` is already there to capture from.

`cinder-home` calls `EnterFuncMode` unconditionally as of this round — the marker file
`/contents/cinder-funcmode.on` is retired.

### END TO END — a PC streaming into it, 2026-08-11

New build installed, `usbipd detach` so Windows could enumerate the sound card, USB-DAC toggled from
the player's own screen, audio played from the PC. Audio came out. From `/contents/cinderhome.log`:

```
usb-dac: released Music track for the DAC (ClosePlayer rc=0)
usb-dac: EnterFuncMode
func-mode: FuncMgrService ready
func-mode: EnterFuncMode(1 = UsbDac) [returned false — not meaningful]
usb-dac: SetUsbFunction
usb-dac: engage -> cinder-msc dac-on rc=0
usb-dac: OnChangedFormat listener rc=0 (registered)
usb-dac: Start() -> ... format=0 ...            <- kFormatNone, nothing streaming yet
usb-dac: waiting for the host — t=1s .. t=116s  <- the heartbeat, doing its job
usb-dac: host changed the stream format — (re)opening the render path
usb-dac: Start() -> { 1, 44100, 32 }
```

Every link of the chain, in order, on real hardware. Device state while streaming:
`functions=audio_func,adb`, `sys.sony.config=uac`, `idProduct=0b8c`, and
`/proc/asound/cards` showing `4 [UAC2Gadget]: UAC2_Gadget`. **adb survived the whole session** —
that is the FuncMode → SetUsbFunction → property ordering paying off exactly as intended.

**And it exposed a decoding bug that had been in the tree for rounds.** The first live payload
printed as `action=1 format=44100 freq=32 bits=0`, which is self-evidently wrong — 44100 is a rate,
not a format. `IUsbDeviceAudioPlayerService::stream_info_t` is **three words and has no `action`
field**; from the tail of `UsbAudioPlayerCore::GetStreamInfo` (@0x16ab0), the function that fills the
struct that goes over the wire:

```
str  r3, [r9, #0]      ; stream_type_t  — mapped from the internal stream_format_t (1->1, 2->3, 3->2)
ldr  r0, [r8, #72]  ->  str  r0, [r9, #4]      ; freq
ldrb r0, [r8, #76]  ->  str  r0, [r9, #8]      ; bitwidth (a byte, widened to a word)
```

Three words — which is also why the listener is `OnChangedFormat(const u32&, const u32(&)[3])`. Our
reader had invented a leading `action`, so every label was shifted by one **and the kFormatNone test
was reading the frequency**. It only ever behaved because a live stream has a nonzero rate and a
stopped one has zero, so the bug was invisible until a real host produced a real payload. Fixed in
both readers (`uac_render` and the `uac_poll_status` backstop); the listener deliberately still
decodes nothing, so there is exactly one decoder to get wrong.

Correctly read, the live stream was **format 1 (PCM), 44100 Hz, 32-bit** — which is precisely the
`S32_LE / 44100 / stereo` the capture thread already hardcodes. That assumption is now measured
rather than assumed.

Task #16 (USB-DAC mode working) is **done**. What remains for #23 is the LDAC leg only: capture
`hw:4,0` and feed the transmitter, with the input side no longer in question.

---

## Round m (2026-08-11) — the LDAC leg: an ALSA capture opened too early is poisoned for good

First on-device run of the bridge with a real host. The log, in full:

```
ldac: capture device hw:4,0
ldac: streaming
ldac: capture not ready (Input/output error) — waiting for the host to start streaming
ldac: readi -> File descriptor in bad state
ldac: stopped after 0 frames (0 s)
```

Five lines, and every one of them matters.

**The hard part had already succeeded.** `snd_pcm_open("hw:4,0", CAPTURE)` returned **0, not
`-EBUSY`** — Sony's `UsbDeviceAudioPlayerService` does *not* hold the gadget's capture substream
exclusively, so the whole feature is viable. That was the one thing that could have killed it.

**What actually failed is ordering.** The UAC capture card appears the moment the gadget enters UAC
mode. That is *minutes* before the user has picked the Walkman as the PC's output device and pressed
play. The bridge opened the PCM at card-appearance time, into an endpoint with no stream behind it:

- first `snd_pcm_readi` → `-EIO`
- every `snd_pcm_readi` after that → **`-EBADFD`** ("file descriptor in bad state"), *permanently*,
  no matter how many times `snd_pcm_prepare` is called.

`-EBADFD` is `SND_PCM_STATE_DISCONNECTED`/`OPEN` leaking out as an errno: the PCM object is in a
state from which `prepare()` cannot move it. **Retrying harder cannot work** — and retrying harder
was the previous round's fix, which is why it did not. The object was poisoned at open.

### The gate: GetStatus, not Start

`UsbDeviceAudioPlayerServiceClient::GetStatus(stream_info_t&)` (slot 3) is a **pure read** of the
same three words `Start()` fills in, and — the whole point — it does **not** take the capture PCM.
So it can answer "is the host streaming yet?" without being self-defeating, which `Start()` cannot.
`format == 0` (`kFormatNone`) means no host. Wait on that, *then* open, once, into a stream that
exists.

**Who is allowed to make that call matters.** Every `pst::services::*` client is asynchronous and
its replies land on the framework looper; `g_uac_client` is built and driven by cinder-home's render
thread. Having the bridge thread call `GetStatus` on that same client would put two threads in one
client's transaction state — the exact shape of the bug that cost weeks on PlayerService. So the
render loop's existing 1 Hz `uac_poll_status` **publishes** the format word to a `sig_atomic_t`, and
the bridge only ever reads it. One writer, one reader, one word. (`uac_poll_status` used to bail out
early when the BT route was active, on the grounds that there was no local render path to open —
true, but it is precisely the bridging case that now needs the poll, so that early-out moved down.)

### Session loop, not one shot

A PC pausing, switching output device, or sleeping ends the *stream*, not the session. The bridge
now loops: wait for a format → open → pump → close → wait again. Previously the first stream ending
killed the thread, so the bridge worked at most once per USB-DAC toggle. Exit conditions are
distinguished — a closed transmitter socket or a user toggle ends the thread; a stopped host or a
wrong-state PCM only ends the current session (reopens capped at 8).

Recovery inside the pump is likewise bounded: `-EPIPE`/`-ESTRPIPE`/`-EIO` get `prepare()` + 20 ms,
but after ~5 s of that we ask GetStatus whether the host is even still there, and after ~15 s with a
live host we conclude the PCM is poisoned and reopen rather than spinning on it forever.

Result of the retest: **the gate works and the capture runs.** Log: `host is streaming (format=1)
— opening hw:4,0` → `after set_params state=2 (PREPARED)` → `snd_pcm_start -> ok` → `after start
state=3 (RUNNING)` → `streaming`, and then no "no capture data" line ever appeared, which means the
1 s wait kept returning ready and `readi` kept delivering frames. **The explicit `snd_pcm_start` is
what moved it** — the stream sat in PREPARED and the library's auto-start-on-read never fired on
this gadget driver.

### The reference dump — Sony's geometry, for the record

Taken from the working DAC→jack session (owner_pid 372 = hagoromo8), against ours:

| | Sony | Cinder |
|---|---|---|
| access / format / channels / rate | RW_INTERLEAVED / S32_LE / 2 / 44100 | **identical** |
| `period_size` | 441 (10 ms) | 882 (20 ms) |
| `buffer_size` | 22050 (500 ms) | 4410 (100 ms) |

So the format was never wrong, only the buffering — and since the capture now delivers with our
geometry, that difference is *not* the bug. Left alone deliberately: Sony's 500 ms buffer is more
overrun-tolerant, but a 125 ms period would make our reads bursty, and changing what now works to
chase a hypothetical is how the last three rounds went.

---

## Round n (2026-08-11) — the bridge's first audio killed the Home app: SIGPIPE

The same session ended:

```
[cinder-home] ldac: streaming
cinderhome-launch: cinder-home CRASHED rc=141 after 419s (respawn 1, 0 consecutive fast)
```

**rc=141 = 128 + 13 = SIGPIPE.** No fault handler line, no backtrace — nothing faulted. The bridge
read real frames, `write(2)` them to the transmitter socket, the service closed its end, and the
default disposition of SIGPIPE terminated the process. The `if (errno == EPIPE)` branch sitting
right there could never run: the process was dead before `write` returned.

That is a far worse bug than the feature it was hiding. cinder-home is the Home app, appmgr does not
respawn the launcher, and the respawn attempt then hung in the easel lifecycle
(`condition_variable::wait` under `onPostInitialize`) and exited 42 — so the launcher handed back to
appmgr and **the device was left with no Home app at all**, showing a frozen frame.

Fixes, both in:

- `signal(SIGPIPE, SIG_IGN)` in `install_diagnostics()`. Any long-lived process that writes to fds it
  does not own needs this, and this one is the Home app.
- `send(fd, …, MSG_NOSIGNAL)` instead of `write()` in the pump, as a local backstop.
- A `SOCKET_GONE` session end now reconnects (bounded to 3) instead of ending the bridge — the
  transmitter closing at the end of a stream is normal.
- `FIRST CAPTURE READ ok — N frames` is logged once per session, so "did any audio move?" stops
  being an inference from the absence of other lines.

What is still unknown is **why** the transmitter closed. The next log answers it: the EPIPE line now
carries the frame count, which separates "closed on the very first write" (we are feeding it
something it rejects) from "closed after seconds of audio" (a stream/state problem at the A2DP end).

Status: built and installed, awaiting the retest.

---

## Round o (2026-08-11) — the socket was never a PCM pipe, and feeding it rebooted the device

The retest ended with the **whole device rebooting** after 5–10 s of audio. The log did not survive
it (`/contents/cinderhome.log` is truncated at boot), so the answer came from the binary instead.

`GetSocketName` returns `pst::services::bttransmitterservice`, and `/proc/net/unix` shows it as the
only socket of its kind on the device — no other `pst::services::*` endpoint exists, so it is not
the generic service transport it looks like. Disassembling the accept loop behind it
(`libBtTransmitterService.so`, bind @0x16d04 addrlen 110, `listen(fd, 1)`, accept @0x16e68) shows
what it actually speaks:

```
recv 4 bytes            -> message type
recv 4 bytes            -> payload length
operator new[](length)
recv length bytes       -> payload
dispatch on type: 0 => length must be 0
                  1 => length must be 28   (copied to a 28-byte struct)
                  2 => length must be 12   (copied to a 12-byte struct)
                  anything else => close the connection
```

**It is a framed control channel with a 12-byte maximum payload.** Round 1 recorded it as the PCM
pipe — `NotifyOpenAudio() → GetSocketName() → connect → write() PCM in NotifyPcmPreferredSize
chunks` — and that reading was wrong in two ways at once. `NotifyOpenAudio`, `NotifyCloseAudio`,
`NotifyPcmPreferredSize` and `NotifyChangeVolume` are the *inbound* half of the interface (the BT
middleware telling the service what happened; `NotifyChangeVolume` is the one we already receive as
a listener callback), not client-callable RPCs. And `NotifyPcmPreferredSize`'s own code
(@0xa4f4: log `"Change read pcm size:%u"`, then `cmp #8192` → `"Over read pcm size MAX."`) is the
service being *told* a chunk size, not announcing one.

So what the bridge did was write raw PCM into a control socket. The service read one audio sample as
a message type and the next as a length:

- a type that is not 0/1/2 takes the error path and closes the connection — **that is the EPIPE of
  round n**, and it explains why the transmitter "closed the socket" the instant real audio moved;
- a length word taken from PCM reaches `operator new[]` as an arbitrary 32-bit value. An allocation
  of hundreds of megabytes inside a core service throws `bad_alloc`, the service dies, and the
  device reboots — **which is exactly what was observed.**

The bridge is therefore **disabled in `ldac_start()`**, with the frame layout recorded at the call
site. It could take the player down at any time and no amount of tuning our end changes that.

### Where the audio actually goes

`/proc/net/unix` names it:

```
/tmp/bt.a2dp.stream      SOCK_DGRAM   <- the A2DP PCM path (MTK stack)
/tmp/bt.int.adp  /tmp/bt.ext.adp  /tmp/bt.app.gap
```

`BtTransmitterExHal::WriteSilent()` is the service keeping that stream fed when there is no source,
which is the shape of the thing we need to displace. Bridging USB-DAC to LDAC means getting PCM into
`/tmp/bt.a2dp.stream` (or into the HAL that owns it), not into the control channel — and the next
round should start by finding which process holds that socket and what a datagram on it looks like,
**before** anything writes to it.

### What survives from the last two rounds

Not nothing. The USB side is now genuinely solved and measured:

- the capture gate works (`GetStatus` format word, published by the render thread);
- `hw:4,0` opens for us — Sony does **not** hold it exclusively;
- an explicit `snd_pcm_start` is required (the stream sits in PREPARED otherwise, and the
  auto-start-on-read convention does not fire on this gadget);
- with that, `snd_pcm_readi` delivers real frames at S32_LE/44100/stereo.

Every one of those is a prerequisite for any future bridge, whatever it writes to. What was wrong
was only the destination.

---

## Round p — the control channel *is* the PCM pipe, after a handshake

Round o was right that raw PCM into `@pst::services::bttransmitterservice` reboots the device, and
right about the frame grammar. It was wrong about the conclusion. The socket is not "control only".
**The accepted connection becomes the A2DP PCM source** the moment a well-formed type‑1 frame
arrives. We crashed because we skipped the handshake, not because we picked the wrong socket.

### Where the audio really flows

Read bottom-up, all inside `hagodaemon` (PID 334 — `BtCommonService BtTransmitterService
BtBleCommonService BtBleRemoteService BtPlayerService`):

```
client socket  --PCM-->  BtTransmitterExHal stream thread
                         -> bt::BtAvSrcComponentIf::SendData(uint16 len, uint8* pcm)   [libBtCompIf]
                         -> BtMwAvSrcRequestSendData                                    [libBtMw]
                         -> btmtk_a2dp_send_audio_stream_data  (encodes: LDAC/SBC/aptX) [blueangel]
                         -> btmtk_a2dp_send_audio_encoded_stream_data
                            sendto("/tmp/bt.a2dp.stream", 1340 bytes, MSG id 601)
                         -> mtkbt (PID 146, fd 4 = the bound end)  -> air
```

`/tmp/bt.a2dp.stream` is therefore **not** a destination for us: it is the MTK stack's internal
message channel, carrying fixed 1340-byte datagrams of *already encoded* frames (msg id 601 at +0,
16-bit `1` at +28, `0x520` at +30, payload length u16 at +34, timestamp u32 at +36, payload from
+40, max 1300 B). Writing there would mean re-implementing MTK's framing *and* bypassing AVDTP
state. `libbluetooth.blueangel.so` links `libldacBTBC.so`, `libsbc_enc.so` and the two aptX blobs —
**the LDAC encoder runs in hagodaemon, in software, on PCM we supply.**

### `BtTransmitterExHal`, recovered

Object layout (from `WriteSilent` @0xa348 and the stream thread @0xa714, `libBtTransmitterService.so`):

| offset | meaning |
|---|---|
| +4     | `bt::BtAvSrcComponentIf*` — `SendData` is vtable slot at +0x2c |
| +12    | the PCM reader — `Read(buf, size, &got)` at vtable slot +8 |
| +0x2c  | `streaming` flag |
| +0x2d  | `silent stop` flag |
| +0x44  | last accepted sound-status byte |
| +0x54  | `u16 pcm size` — what `NotifyPcmPreferredSize` sets, capped at 8192 |
| +0x56  | PCM buffer, 8192 B |
| +0x2056| silence buffer, 8192 B |

`WriteSilent()` is just `memset(silence,0,8192)` then `while (!stop) SendData(pcm_size, silence)` —
it keeps the A2DP stream alive with zeros while nothing is playing. The stream thread is the same
loop with real data:

```c
while (this->streaming) {
    if (reader->Read(pcm_buf, this->pcm_size, &got)) continue;   // vtable slot +8
    if (got == 0) break;
    src->SendData((uint16)got, pcm_buf);                          // vtable slot +0x2c
}
```

### The handshake

`OnEvent(ipcmw::ipcsocket::EventParam& p)` @0x9fc0. The server fills `p` as

```
p+0    event kind (0 = message received)
p+4    the accepted connection object   <- the server hands its connection over here
p+8    message type: 0 | 1 | 2
p+12   payload, 28 bytes for type 1 / 12 bytes for type 2
```

and the type‑1 path ends with the move that settles the question (@0xa166):

```
r1 = p[4];  p[4] = 0;            // take the connection
old = this->[12];
this->[12] = r1;                 // it IS the PCM reader
if (old) old->release();
this->[0x2c] = 1;                // streaming = true
... pthread_create(stream thread)
```

Payload fields the handler actually reads: `+4` = channel count (1 stays 1, anything else becomes
2), `+20` = a `u8` flag, `+24` = **sample rate in Hz**, checked against the negotiated frequency
through this table (`.rodata` @0x1c130, indexed by `freq_enum - 1`):

```
1 -> 44100   2 -> 48000   3 -> --   4 -> 88200   5..7 -> --   8 -> 96000
```

which is exactly `IBtTransmitterService::BtSoundFrequency`. Before any of that, the handler requires
`(avsrc_status & ~1) == 4`, so the headphones must already be connected and the source ready.
Fields `+0`, `+8`, `+12`, `+16` are not touched by this handler — still unidentified.

### What this means for the bridge

The path is: connect → send **one** `[u32 type=1][u32 len=28][28-byte payload]` frame → then write
raw PCM in `pcm_size` chunks on the same fd. Sony's own code does the LDAC encoding and the AVDTP
bookkeeping; we only supply samples, which is exactly what the USB capture already produces.

The danger is unchanged and must be respected: **anything that is not a well-formed frame while the
connection is still in frame-parsing mode reaches `operator new[]` with an arbitrary 32-bit length
inside a core service, and the device reboots.** So the next step is a `cinder-probe` mode that
sends the type‑1 frame *and nothing else*, and we read the service's log to see whether it was
accepted, before a single PCM byte follows it.

### Round p, confirmed on device the same evening

`cinder-probe --btopen`, three runs, no reboot (uptime ran 92 → 223 s straight through):

```
btopen: socket 'pst::services::bttransmitterservice'
btopen: connected
btopen: sending type=1 len=28 chans=2 rate=44100
btopen: ACCEPTED — connection still open 3 s after the handshake
btopen: wrote 1764000 of 1764000 bytes
```

Three independent confirmations, in increasing order of strength:

1. **Accepted.** The service kept the connection instead of closing it, so the payload passed the
   `(avsrc_status & ~1) == 4` check and the channel/rate fields were understood.
2. **Real-time drain.** 10 s of audio (1764000 B) took 9.8 s of wall clock to write. The writer is
   paced by the reader, and the reader consumes at exactly 44100 x 2 x 2 B/s — a live A2DP stream,
   not a socket buffer swallowing bytes. That arithmetic also **fixes the wire format at S16_LE**:
   4 bytes per frame, not 8.
3. **Audible.** A 440 Hz sine came out of the headphones. A wrong format would have been noise.

So the bridge is: connect → handshake → raw S16_LE PCM. The capture side already produces S32_LE,
so `ldac_pump` takes the top 16 bits of each sample (the gadget's low half is padding on a
24-in-32 container) and writes 4 bytes per frame instead of 8.

`ldac_start()` is re-enabled, with `ldac_handshake()` gating every session and refusing to send
audio on a connection the service did not accept.
