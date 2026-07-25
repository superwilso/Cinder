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

## Status: BUILDS + packaged. Two unknowns confirmed only on-device (see TEST.md).

- **Control plane — DONE.** The real `BtTransmitterServiceClient` vtable indices are
  baked into `btclient.c` (SetCurrentSource=12, SetLdacSoundQuality=18, SetLdac=20,
  GetSocketName=29; extracted via `analysis/E_usbdac_ldac/ghidra/DumpVtable.java`).
  KEY RE finding: the audio socket is opened **server-internally**, triggered by the
  `SetLdac`/`SetCurrentSource` path — there is no client `NotifyOpenAudio`. So `main.c`
  does `SetLdac(true)` → `SetLdacSoundQuality` → `SetCurrentSource(true)` →
  `GetSocketName()` → connect (with retry, since the open is async).
- **Builds clean** (`build.sh`): armhf glibc-dynamic, ~10 KB stripped, NEEDED =
  `libasound.so` + `libBtTransmitterService.so` + libc. No libasound2-dev required —
  a minimal ALSA shim (`include/alsa/asoundlib.h`) is used if the dev package is absent.

Two things only hardware can confirm — TEST.md drives both with logging:
1. **Does `SetCurrentSource` actually open the server socket** (so `connect()` works)?
2. **Capture contention** — stock UAC (`UsbDeviceAudioPlayerService`) owns `card4/pcm0c`,
   so `snd_pcm_open` may return `-EBUSY`. If so, stop/redirect that service or replace
   `libaudiohal-uacalsasingletrack.so` to tee PCM into our socket.

Policy side (Option B): Cinder replaces `HgrmMediaPlayerApp`, so the stock
`disconnectMsgOverlay` / `RequestDisconnection` gate is simply gone — but for the first
test under stock, use the reverse-order trick (USB-DAC first, then connect BT). See TEST.md §2.

## Build
```bash
./build.sh        # → cinder-ldac-bridge (armhf, dynamic). No sudo/apt needed.
```

## Deploy / test
Packaged via the same `exec_file` hook as Cinder — see `deploy/` (`install_ldac.sh`,
`uninstall_ldac.sh`, `ldac-run.sh` supervisor) and **TEST.md** for the full procedure.
Quick version: stage the binary, flash `ldac_install.upg`, reboot, then control via
files — `touch /contents/ldac_on` to start, `rm` it to stop, read `/contents/ldac.log`.
