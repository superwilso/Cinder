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
