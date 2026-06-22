# ldac-bridge — USB-DAC input → LDAC output (NW-A50)

Bridges the Walkman's USB-DAC PCM input to the Bluetooth LDAC transmitter, the
feature stock blocks purely by app policy. Architecture + evidence:
`../analysis/E_usbdac_ldac/RE_findings.md`. This is **Strategy B** (own producer
daemon driving `BtTransmitterService` directly).

## Pipeline
```
USB-DAC capture (ALSA card4/pcm0c, 44100 S32_LE 2ch)
  → ldac-bridge
    → abstract AF_UNIX socket "\0"+GetSocketName()
      → BtTransmitterService (recv → LDAC encode → MTK BT chip) → headphones
```

## Files
- `src/main.c` — orchestration + the abstract-socket writer (**complete**).
- `src/btclient.c/.h` — control plane: calls the exported factory
  `BtTransmitterServiceClientFactory::CreateInstance()` then drives the client's
  virtual methods by vtable index (manual thiscall, avoids any C++-ABI dependency).
- `src/capture.c/.h` — USB-DAC capture via the device's `libasound.so`.
- `build.sh` — cross-build (arm-linux-gnueabihf, links device `.so`s from the rootfs).

## Status: scaffold. Two pieces need on-device completion.

1. **Vtable indices (`btclient.c`, the `VIDX_*` are placeholders).** Extract the
   `BtTransmitterServiceClient` primary vtable from the Ghidra `BtTx` project (the
   client object from `CreateInstance` @0x1e840 has the `IBtTransmitterService`
   vtable at word[0]). Map each slot to its method via the per-function log strings
   (`BtTransmitterServiceClient::SetLdac`, …). Method signatures: `SetLdac(const
   bool&)`, `SetLdacSoundQuality(const enum&)`, `NotifyOpenAudio()`,
   `NotifyPcmPreferredSize(const uint16_t&)`, `GetSocketName()`→`std::string`
   (libc++ layout handled in `btclient_get_socket_name`).

2. **Capture contention (`capture.c`).** In stock USB-DAC mode the Sony UAC service
   already owns `card4/pcm0c` (routing it to `card0`); capture substreams are
   exclusive so our `snd_pcm_open` will likely return `-EBUSY`. Must stop/redirect
   that routing (e.g. stop the UAC service, or replace
   `libaudiohal-uacalsasingletrack.so` so the UAC path feeds our socket directly).

Plus, on the policy side: the replacement player (or a small mode-forcer) must enter
USB-DAC mode **without** the stock app's `disconnectMsgOverlay` / `RequestDisconnection`
so the LDAC link survives (the daemon assumes BT/LDAC is already connected).

## Build
```bash
sudo apt install -y libasound2-dev        # ALSA API headers (arch-independent)
./build.sh                                 # → cinder-ldac-bridge (armhf, dynamic)
```

## Deploy / test (once the two pieces above are done)
Install as a `/system` daemon via the same `exec_file` hook used for the probe/Cinder
(copy binary to `/contents`, installer moves it to `/system/vendor/unknown321/bin`,
wrap a boot binary or run on demand). Test loop: connect LDAC headphones, force
USB-DAC mode (bypassing the app gate), run the daemon, play from the PC, listen.
