# Phase E — USB-DAC → LDAC: device-side closure

Host-side analysis (CLAUDE.md Part H3) reached **high confidence that the
USB-DAC↔BT mutual-exclusion is Candidate 1: app-policy in `HgrmMediaPlayerApp`**
(a `disconnectMsgOverlay` UI flow + an explicit BT-disconnect call), *not* a
constraint of the audio stack — the services already support concurrent
different-type tracks (`libaudiohal-dualtrackmixalsa.so` literally mixes two),
and `BtTransmitterService` is a complete, externally-callable service.

This directory holds the device-side probe that **closes** the four remaining
questions (H6) before we build anything:

| # | Question | What in `probe.sh` answers it |
|---|----------|-------------------------------|
| 1 | Any runtime mutex blocking BtTransmitter + UsbDeviceAudioPlayer together? | `pcm status` in `both` mode — do a UAC capture **and** a BT/A2DP playback substream show `RUNNING` at once? |
| 2 | Which ALSA device does the UAC path write to (confirm `hw:0,4`)? | `pcm status` + `asound.conf` in `usbdac` mode |
| 3 | Does the BT source path expose an ALSA entry point we can push PCM into? | `asound pcm list` + `audiohal plugins` (a2dp source substream?) |
| 4 | Entering USB-DAC: does the player tear down BT, or just show the overlay? | `watch` mode log capture across the toggle + `mtkbt log tail` before/after |

> Note: Sony's BT is the **MediaTek stack (`/bin/mtkbt`), not BlueZ** — so H6 #4's
> "bluez" hint is really "watch `mtkbt`". Probe captures `/tmp/mtkbt.log`.

## Run sequence

`probe.sh` is **read-only** (no writes to `/system`, no service kills). Run it
once per audio condition; each run appends a labelled snapshot to
`/contents/cinder_probe.log` (the FAT user partition — readable over USB-MSC):

```sh
sh probe.sh idle        # nothing playing, no BT
sh probe.sh bt_ldac     # LDAC headphones connected + music playing
sh probe.sh usbdac      # USB-DAC mode active (plugged into a PC playing audio)
sh probe.sh both        # THE TEST: LDAC connected & playing, THEN enter USB-DAC
sh probe.sh watch 30    # live log while you flip USB-DAC on/off (answers #4)
```

Then pull `/contents/cinder_probe.log` over MSC and diff the snapshots.

## Getting it onto the device (normal boot — the foothold)

The probe needs **normal boot** with the audio stack alive; the `.UPG`/`exec_file`
pipeline runs in the UPDATER initrd (no audio) and can't host it directly.

### adb — RULED OUT (confirmed 2026-06-20)

Tested this session via usbipd→WSL (bypasses the Windows-driver problem). In
USB-DAC mode (PID `054c:0b8c`) the Walkman exposes **only two interfaces, both
USB-Audio class `01`** (`snd-usb-audio`); there is **no adb interface**
(`ff/42/01`). Mass-storage mode (`0ca0`) is storage-only. So despite
`init.usbcfg.rc` listing `audio_func,adb`, stock composes audio only — **adb is
not reachable** in any user mode on this stock+Wampy unit.

### Chosen route — no-repack `/system` wrapper hook

Sony's `init.rc` imports only from the initrd, BUT Wampy already launches its
root binaries (`scrobbler`/`wampy`/`pstserver`) from the **writable `/system`**
at boot, and the `exec_file` UPDATER can mount+write `/system` (Wampy's own
install method). So we **don't repack the boot image** at all:

- `install_probe.sh` (runs once in UPDATER): mounts `/system`, drops
  `cinder_probed.sh`, and wraps `scrobbler` reversibly
  (`scrobbler` → `scrobbler.real` + a 3-line launcher that backgrounds the
  daemon then `exec`s the original). Boot image untouched; fully reversible.
- `cinder_probed.sh` (runs every normal boot): timer-snapshots ALSA/USB/BT
  state to `/contents/cinder_probe.log` (no live channel, so it captures a
  timeline while you drive the test).
- `uninstall_probe.sh`: restores `scrobbler.real`, removes the daemon.

Recoverable via the verified wbrt eMMC backup if anything misbehaves.

### Package + flash

```sh
cd artifacts/repos/rockbox/utils/nwztools/scripts
make exec_file NWZ_TARGET=nw-a50 EXEC=install_probe.sh   UPG=cinder_probe_install.upg
make exec_file NWZ_TARGET=nw-a50 EXEC=uninstall_probe.sh UPG=cinder_probe_uninstall.upg
```
Built artifacts (also staged to the Windows Downloads folder):
`cinder_probe_install.upg`, `cinder_probe_uninstall.upg`.

To flash: rename to `NW_WM_FW.UPG` on the device root (UMS/MSC mode), then
`make do_fw_upgrade NWZ_DEV=/dev/sdX` (or `scsitool -s nw-a50 /dev/sgN
do_fw_upgrade`). Device reboots into UPDATER, runs the script (logs to
`/contents/cinder_install.log`), reboots to normal. The daemon then runs each
boot.

### Test sequence (drives the H6 answers)

With the daemon installed and the device booted normally:
1. Connect LDAC BT headphones, play music (~20 s) — captures `bt_ldac`.
2. Plug into a PC, enable **USB-DAC mode**, play from the PC (~20 s) — captures
   `usbdac`, and whether BT survived the transition (#4).
3. The headline: with LDAC still connected, stay in USB-DAC mode (~20 s) — the
   log shows whether a **UAC capture and a BT/A2DP playback substream are
   `RUNNING` simultaneously** (#1).
4. Switch to mass-storage mode, copy `/contents/cinder_probe.log` off, diff the
   ticks. Then flash `cinder_probe_uninstall.upg` to revert.

## After the probe answers H6

If #1 shows no mutex and #4 shows the block is purely the overlay/disconnect
call, the feature is "free": ship the bypass in the Cinder player (don't show the
overlay / don't call disconnect) per CLAUDE.md H5 Step 1, then bridge PCM
(Approach A: drive `BtTransmitterService` directly, or Approach B: `snd_aloop`
loopback). Pick the bridge from the `strace`/ALSA evidence the probe gathers.
