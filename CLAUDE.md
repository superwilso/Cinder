# NW-A55 Project — Setup & Next Steps (v1.4)

**Purpose:** A single onboarding document that takes you from a stock Windows PC to a
working analysis environment, runs the existing host-side pipeline, and then adds the
device-side procedure for when the NW-A55 is in hand.

**Relationship to existing repo:** The repo already has a 7-phase host-side pipeline
(`Makefile`, `phases/phase1`–`phase7`, `CLAUDE.md`, `docs/`). This document does **not**
replace those — it wraps them with (a) Windows/WSL environment setup the phases assume but
don't provide, and (b) a new device-side procedure ("Phase 8") that the host-side phases
explicitly defer.

**v1.4 status note:** The MT8590 SoC is confirmed (`docs/baseline_v1.4.md` §4.1, citing
`unknown321/wbrt` README and Wampy `MAKING_OF.md`). Phase 3 is a *confirmation* step, not
*discovery* — `phases/phase3_soc_id.sh` now checks for MT8590-specific markers in the
extracted firmware and reports them as evidence.

---

## Part A — Why Windows + WSL2 (and what stays native)

The analysis pipeline is Linux bash + Linux tools (`binwalk`, `dtc`, `qemu-arm-static`,
loop-mounting ext4, cross-compilers). That all runs in **WSL2**. But several operations
*must* run on **native Windows** because they talk to the device over USB with
Windows-only drivers:

| Task | Where it runs | Why |
|---|---|---|
| Phases 1–7 (firmware analysis, cross-compile) | **WSL2 (Ubuntu)** | Linux toolchain; loop-mount; qemu |
| Reading/editing/decrypting `.UPG` (`upgtool`) | **WSL2** | Rockbox tool builds on Linux |
| Installing a `.UPG` to the device (Sony updater) | **Native Windows** | `SoftwareUpdateTool.exe` is Windows-only |
| Installing Walkman One / Wampy / scrobbler | **Native Windows** | Their installers are `.exe` |
| Full eMMC backup/restore (`wbrt`) | **Native Windows** | Needs MediaTek USB VCOM driver (`VID_0E8D`) |
| Low-level MediaTek access (`mtkclient`, SP Flash Tool) | **Native Windows** (or WSL2 + usbipd) | MTK preloader/BROM USB driver |
| `adb`/`scsitool` device shell | **Either** (WSL2 needs usbipd-win for USB passthrough) | adb works in both; raw USB needs passthrough |

**Rule of thumb:** *analyze in WSL, touch the device from Windows.* The one exception is
`adb` shell access, which can be made to work from WSL2 with `usbipd-win` (Part F).

---

## Part B — Environment setup

### B1. Install WSL2 + Ubuntu (native Windows, PowerShell as admin)

```powershell
wsl --install -d Ubuntu-24.04
wsl --set-default-version 2
# reboot when prompted, then launch "Ubuntu" from Start menu to create your user
```

Verify you're on WSL2 (not 1):

```powershell
wsl -l -v   # VERSION column must say 2
```

### B2. Install host dependencies (inside WSL Ubuntu)

The repo's `make check-deps` looks for: `git binwalk dtc file readelf strings nm
qemu-arm-static cargo rustup clang`. Install them:

```bash
sudo apt update
sudo apt install -y \
    git build-essential \
    binwalk device-tree-compiler binutils file \
    qemu-user-static \
    clang lld llvm \
    python3 python3-pip \
    android-tools-adb \
    libssl-dev pkg-config

# Rust + the ARM musl target the project cross-compiles to
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add armv7-unknown-linux-musleabihf
```

Optional but recommended for deeper RE:

```bash
# unblob is a stronger alternative to binwalk for some sectors
pip3 install --user unblob
# Ghidra (for HgrmMediaPlayerApp / libSoundServiceFw decompilation) — install via:
sudo apt install -y default-jdk
# then download Ghidra from github.com/NationalSecurityAgency/ghidra/releases and unzip
```

A musl cross-compiler for the C shim (Phase 6 looks for `arm-linux-musleabihf-gcc`):

```bash
# Easiest: grab a prebuilt toolchain from musl.cc
cd ~ && wget https://musl.cc/arm-linux-musleabihf-cross.tgz
tar xf arm-linux-musleabihf-cross.tgz
echo 'export PATH="$HOME/arm-linux-musleabihf-cross/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

Then confirm:

```bash
cd /path/to/repo
make check-deps     # should now report all green
```

### B3. Where to keep the project

Clone the repo **inside the WSL filesystem** (`~/projects/...`), not under `/mnt/c/...`.
Loop-mounting ext4 and fast `find`/`grep` over the rootfs are dramatically slower on the
Windows-mounted drive. You can still open the files from Windows via `\\wsl$\Ubuntu\home\...`.

### B4. Native Windows tools to install (download, don't run yet)

Install these on the Windows side so they're ready when the device arrives:

1. **MediaTek USB VCOM driver** — required for `wbrt` and any MTK tool. The device enumerates
   as `VID_0E8D`. (`wbrt`'s README has the driver-cleanup steps if Windows grabs the wrong one.)
2. **`wbrt`** (Walkman Backup/Restore Tool) — `github.com/unknown321/wbrt/releases`. **This is
   your brick insurance.** Full eMMC backup before any write.
3. **Walkman One installer** — `mrwalkman.com` (NW-A50 series page). The recommended baseline FW.
4. **Wampy installer `.exe`** — `github.com/unknown321/wampy/releases/latest`.
5. **scrobbler installer** — `github.com/unknown321/scrobbler/releases` (for log-format reference).
6. **(Optional) `mtkclient`** — `github.com/bkerler/mtkclient`, only if you later need
   BROM/preloader-level eMMC access. Python-based; needs `usbdk` driver on Windows.

---

## Part C — Repos to clone and what to read in each

Phase 1 (`make phase1`) already clones the first five into `artifacts/repos/`. This table is
the reading guide — which file in each repo answers which project question.

| Repo | Cloned by | Read this first | Answers |
|---|---|---|---|
| `unknown321/wampy` | phase1 | `MAKING_OF.md`, then the other `MAKING_OF_*.md`, `ALSA.md`, `kernel.md`, `installer/run.sh`, `src/Controller.cpp` | The entire userspace map: init hooks, LD_PRELOAD points, view-model surface, filter chain, ALSA topology, boot-image gotcha, `.UPG` installer pattern |
| `unknown321/scrobbler` | phase1 | `README.md`, `INSTALL.md`, `playerevents`/`audioscrobbler`/`daemon` source | `.scrobbler.log` format and path (root of internal storage), the track-state IPC, the "beeps off" gotcha, adb-vs-scsitool install |
| `unknown321/wbrt` | phase1 | `README.md` | **MT8590 confirmation**, MediaTek VID/PID, backup/restore procedure |
| `Rockbox/rockbox` (sparse: `utils/nwztools`) | phase1 | `upgtools/` (build `upgtool`), `database/nvp/` (KAS + NVP slot map), `scsitool/` | `.UPG` pack/unpack, per-model KAS (64 bytes), NVP slot definitions, SCSI backchannel |
| `roobscoob/SonyWalkmanFirmwarePatcher` | phase1 | `upgDocs.md` | The clearest one-page V2 `.UPG` format + crypto spec |
| `zhangboyang/llusbdac` | **add manually** | `README`, the module source | Low-latency USB-DAC kernel module precedent — central to the USB-DAC+LDAC question |

Add the one missing repo to your Phase 1 clone list (or clone by hand):

```bash
git clone --depth=1 https://github.com/zhangboyang/llusbdac \
    artifacts/repos/llusbdac
```

**Reading order for a new engineer:** wbrt README (5 min, confirms platform) → roobscoob
`upgDocs.md` (15 min, the file format) → all Wampy `MAKING_OF_*.md` (one afternoon, the
whole device) → Rockbox `nwztools` source (as needed for KAS/NVP/SCSI).

---

## Part D — Firmware acquisition & extraction (host-side, in WSL)

This is the existing pipeline. Run it in order. Each phase gates on the previous via a
sentinel file.

### D1. Acquire the two firmware images (manual download — Sony needs a browser)

- **Stock A55 `.UPG`**: Sony support downloads → "NW-A55 firmware" → `NW_WM_FW.UPG` (v1.02 or
  latest). Place at `artifacts/stock/NW_WM_FW.UPG`.
- **Walkman One `.UPG`**: `mrwalkman.com` NW-A50 page. Place at
  `artifacts/walkmanone/WalkmanOne.UPG`.

### D2. Run the pipeline

```bash
make check-deps          # all green from Part B
make phase1              # clone repos, build upgtool, unpack both .UPG files
make phase2              # binwalk every sector; extract boot image; loop-mount rootfs
                         #   (phase2 may print a sudo helper: sudo bash artifacts/mount_rootfs.sh)
make phase3              # SoC: now a CONFIRMATION step — expect to see mt8590 markers
make phase4              # rootfs deep-dive 4a–4h (init flow, toolchain, view-models,
                         #   SoundServiceFw symbols, USB-DAC routing, hold switch, 2038)
make phase5              # diff stock vs Walkman One — the "what W1 changes" recipe
make phase6              # cross-compile hello-walkman (armv7 musl) + C shim; qemu-test
make phase7              # .UPG repack round-trip — proves modify→repack→unpack is sound
```

### What each phase answers for your four feature questions

| Your question | Phase that informs it | Output file |
|---|---|---|
| Full UI replacement, auto-boot | phase4b (init flow → HgrmMediaPlayerApp service) | `analysis/4b_init_flow.txt` |
| Keep all audio features | phase4c (Sony clang version), phase4e (SoundServiceFw symbols) | `analysis/4c_*`, `analysis/4e_*` |
| Queue / shuffle-by-album | (none host-side — pure app logic; see baseline §5.12) | n/a |
| USB-DAC + LDAC out | phase4f (routing verdict from strings) — **narrows but cannot close**; needs device | `analysis/4f_usb_dac_routing.txt` |

The USB-DAC question is the one host-side analysis can only *narrow*. Phase 4f greps
`HgrmMediaPlayerApp`, `libSoundServiceFw.so`, and `llusbdac.ko` for routing/exclusivity
strings to guess which of the three candidates enforces the block — but the definitive
answer comes from Phase 8 (device-side).

---

## Part E — Device-side procedure ("Phase 8") — when the NW-A55 arrives

The host-side phases are explicitly "no device required." This is the missing piece. Do these
in order; the ordering is a safety gradient (back up first, read before write, reversible
before irreversible).

### E0. FIRST, before anything: full backup (native Windows)

1. Connect the A55, let Windows install the MediaTek VCOM driver (`VID_0E8D`).
2. Run **`wbrt`**, choose Backup. This dumps the full eMMC including NVP, serial, and factory
   settings. **Store this backup somewhere safe and redundant.** It is the only thing that can
   recover a brick on this device (there is no public USB DFU/EDL recovery for the audio SoC).
3. Note: a `wbrt` backup is **device-specific** — never restore one device's backup to another;
   it overwrites the serial and factory calibration with no recovery.

### E1. Establish the baseline firmware (native Windows)

1. Install **Walkman One** via its installer. This is the project's known-good baseline
   (everything downstream is measured against it, and Wampy/scrobbler are best-tested on it).
2. Install **Wampy** on top. This gives you a known-working modded environment to probe from,
   and a working `.UPG` install path you can imitate.

### E2. Get a shell

Two routes; try adb first.

**Route 1 — adb (preferred).** Per the scrobbler INSTALL.md, adb is present on some
configurations. From WSL2 you need USB passthrough (Part F), or just run adb from native
Windows:

```bash
adb devices                 # confirm the A55 shows up
adb shell                   # interactive shell on the device
```

**Route 2 — scsitool (no adb).** Build Rockbox `scsitool-nwz` (in WSL) and use the USB-MSC
SCSI backchannel to read NVP and device info. Use this for NVP work even if adb is available.

### E3. Confirm the SoC and platform (closes/confirms OQ1)

On the device shell:

```bash
cat /proc/cpuinfo                              # CPU cores, features
cat /sys/firmware/devicetree/base/compatible   # SoC compatible string (expect mt8590)
getprop ro.board.platform                       # expect: mt8590  (if property service present)
getprop ro.boot.console                         # expect: ttyMT1  (MediaTek UART prefix)
dmesg | head -100                               # kernel boot log — machine name, SoC init
```

This confirms the v1.4 finding (MT8590) on your actual unit and records the exact sub-variant.

### E4. Map the ALSA topology in each mode (the USB-DAC answer — closes OQ2)

This is the single most important device-side investigation. Run the **same commands in each
audio mode** and diff the output:

```bash
# Run this block in EACH of these modes:
#   (a) normal file playback to 3.5mm
#   (b) USB-DAC mode active
#   (c) Bluetooth output (LDAC) active
#   (d) Bluetooth receiver mode active
cat /proc/asound/cards          # which cards exist
aplay -l                        # playback PCM devices (expect sonysoccard, 6 devices)
aplay -L                        # PCM device names incl. plugins
arecord -l                      # CAPTURE devices — KEY: is anything capturable?
amixer scontrols                # mixer controls / routing switches
cat /proc/asound/card0/pcm*/sub*/status   # which substreams are RUNNING
ls -l /dev/snd/                 # device nodes present
```

**What the result tells you:**
- If a **capture or loopback device appears during USB-DAC mode**, you can route USB-PCM-in to
  the LDAC encoder → the feature is achievable in software.
- If **no capture device ever appears and `llusbdac.ko` outputs only to `cxd3778gf-*`**, the
  routing is fixed at the kernel-module level → you'd need a custom `llusbdac.ko` build.
- If the **BT sink simply isn't present while USB-DAC is active**, check whether it's BlueZ
  state (E5) or app policy (E6).

### E5. Observe what changes when you toggle USB-DAC (find the enforcement layer)

```bash
# While toggling USB-DAC mode on the device, watch what the player touches:
strace -f -p $(pgrep HgrmMediaPlayerApp) 2>&1 | grep -iE 'bluez|dbus|snd|pcm|usb|dac'
# In a second shell, watch BlueZ state:
dbus-monitor --system 2>/dev/null | grep -i bluez     # (BlueZ 4 is old; may need hcitool/hciconfig)
hciconfig -a                                          # BT adapter up/down across mode switch
cat /sys/class/android_usb/android0/functions 2>/dev/null  # USB gadget config (MSC/MTP/DAC)
```

Maps directly onto the three candidates in baseline §5.10:
- player calls `disconnect`/`bluez` on USB-DAC entry → **Candidate 1 (app policy)** → easiest fix
- a `libSoundServiceFw` source/sink enum flips → **Candidate 2** → call routing APIs carefully
- `llusbdac.ko` binds a fixed ALSA sink → **Candidate 3** → custom kernel module

### E6. Dump and characterize NVP (read-only first)

```bash
# Via scsitool from the host (read-only — do NOT write yet):
./scsitool-nwz <dev> dump_nvp        # read every reachable NVP node
# Confirm: destination code, SPS flag, KAS node (kas, 64 bytes), upgrade flag
```

Back up the raw NVP partition before any write. Only after a verified backup should you
attempt any `scsitool` write (destination/region is reversible; the U-Boot password slot is
**not** — never write it).

### E7. First code on the device

1. Build `hello-walkman` (Phase 6 output) into a `.UPG` using `upgtool` + a Rockbox
   `nwztools/scripts/exec_file.sh` template.
2. Install it the same way Wampy installs (its `installer/run.sh` is the template).
3. Verify it runs at boot and logs. Include a **bad-boot counter** (Wampy's pattern) so a
   crash auto-reverts after N failed boots.
4. Only after that works: build the `init.rc` change that swaps `HgrmMediaPlayerApp`'s service
   entry for your binary (full UI replacement). Test with the bad-boot counter as the safety net.

---

## Part F — (Optional) USB device access from WSL2

If you want `adb`/`mtkclient` to reach the device from inside WSL rather than native Windows,
use `usbipd-win`:

```powershell
# Native Windows, admin PowerShell:
winget install usbipd
usbipd list                       # find the A55's BUSID
usbipd bind --busid <BUSID>
usbipd attach --wsl --busid <BUSID>
```

```bash
# Inside WSL:
lsusb                             # should now show VID 0e8d (MediaTek) or the adb interface
adb devices
```

For raw MediaTek BROM/preloader work, native Windows + `usbdk` is usually less fiddly than WSL
passthrough. Keep `wbrt` and SP Flash Tool on the Windows side.

---

## Part G — Suggested milestone order (the whole project on one page)

1. **Environment** (Part B) — WSL2 + deps green on `make check-deps`. *Half a day.*
2. **Read** (Part C) — wbrt README → roobscoob upgDocs → Wampy MAKING_OFs. *One day.*
3. **Host pipeline** (Part D) — `make phase1`…`phase7`. *One to two days.* Produces the
   confirmed SoC, init map, toolchain version, SoundServiceFw symbols, the W1 diff recipe, a
   proven `.UPG` round-trip, and a cross-compiled hello-world.
4. **Update stale docs** — flip `CLAUDE.md` and `phase3` to "MT8590 confirmed."
5. **Device arrives → backup** (E0) — `wbrt` full dump. *Non-negotiable, first.*
6. **Baseline FW** (E1) — Walkman One + Wampy.
7. **Shell + SoC confirm** (E2–E3).
8. **The USB-DAC investigation** (E4–E5) — this is the make-or-break for the headline feature.
   Decide here whether USB-DAC→LDAC is "free," "needs kernel module," or "blocked."
9. **NVP characterization** (E6) — read-only, then careful backups before any write.
10. **First code on device** (E7) — hello-world service → bad-boot counter → full player
    swap. Now you're building the actual replacement player.

**Features recap against this plan:**
- Full UI replacement + auto-boot: unlocked at step 10. *High confidence.*
- Keep all audio features: depends on step 3 (clang version) + step 10 (shim). *High confidence.*
- Queue + shuffle-by-album: pure app logic in the replacement player; no blocker. *High
  confidence.* (Both genuinely absent from stock — baseline §5.12.)
- USB-DAC in + LDAC out: resolved at step 8. *Uncertain until then* — do not promise it as a
  feature before E4/E5. **Host-side analysis (Part H) has narrowed this to Candidate 1
  (app-policy enforcement) with high confidence.**

---

## Part H — Host-side findings (2026-05-26 run, stock NW-A50 v1.02)

This section captures the concrete findings from the first end-to-end pipeline run on the
stock firmware (`NW-A50_V1_02.exe` → `NW_WM_FW.UPG`, 112 MB). It supersedes the speculation
in `docs/baseline_v1.4.md` §5.10 with on-firmware evidence.

### H1. Pipeline state

All seven host-side phases ran on stock firmware. Walkman One unpacked successfully as a
cross-reference but is **not** the project's baseline (user runs stock).

| Phase | Status | Key output |
|---|---|---|
| 1 | ✓ | `upgtool` built; stock + W1 unpacked with model `nw-a50` (KAS confirmed) |
| 2 | ✓ | Boot image (sector 2) = Android `bootimg` container (kernel + ramdisk); rootfs (sector 6) = 800 MB ext4 |
| 3 | ✓ | MT8590 **confirmed on firmware**: `ro.board.platform=mt8590` in `/build.prop`, plus `mt8590` strings in 9 `libMtkOmx*`/`libmtk_drvb` libraries. OQ1 closed on-firmware (not just by `wbrt` citation). |
| 4 | ✓ | Full rootfs deep-dive — see H3 below. The single most important output. |
| 5 | partial | Stock vs W1 sectors are renumbered (stock rootfs = sector 6, W1 = sector 7); a useful diff requires file-level comparison of extracted rootfs trees, not sector files. |
| 6 | ✓ | Rust hello-walkman cross-built for `armv7-unknown-linux-musleabihf`, 506 KB static, runs under `qemu-arm-static`. C shim toolchain (`arm-linux-musleabihf-gcc`) verified. |
| 7 | ✓ | UPG round-trip: 8/9 sectors byte-identical, sector 6 differs only because phase1 saved it `-z`-decompressed (ext4) vs the round-trip's compressed `fwpup` form — the **mechanism is sound**. |

### H2. Extraction-tool notes (so the next person doesn't re-discover these)

- **Sony's `NW-A50_V1_02.exe`** is a proprietary "Packman" self-extractor — 7z/binwalk/unblob
  cannot extract the inner `NW_WM_FW.UPG`. The reliable path is to run the `.exe` on Windows
  and copy the `.UPG` out of `%TEMP%` before clicking past the first dialog.
- **The `1_StockRevert_Walkman_One_A50.exe`** in the Walkman One bundle contains a `.UPG`
  named `NW_WM_FW.UPG`, but **it is not the Sony stock firmware** — it's a custom revert
  payload built by MrWalkman that fails KAS validation against `nw-a50` (the StockRevert
  procedure runs the genuine `2_NW-A50_V1.02.exe` afterwards to actually flash stock).
- **`upgtool` build** requires `libcrypto++-dev` (`mg.cpp` brute-force search, unused for
  known KAS but a hard build dep).
- **`upgtool` flags** — the original phase scripts referenced `-x` and `-d`; the actual
  flags are `-e` (extract) / `-c` (create), with `-m <model>` mandatory and `-o <prefix>`
  for output. `-z <idx>` decompresses sector `<idx>` from `fwpup` framing.
- **Mounting the rootfs** doesn't require sudo: `7z x 6.bin -o<dir>` reads ext4 natively.
  Phase 2 was updated to do this automatically and detect sectors by `file` output rather
  than filename pattern.

### H3. USB-DAC → LDAC: the verdict (Candidate 1, high confidence)

The `baseline_v1.4.md §5.10` open question — *where is USB-DAC-and-BT mutual exclusion
enforced?* — narrows to **Candidate 1: app-policy enforcement in `HgrmMediaPlayerApp`**.
The evidence:

**1. The kernel-level candidate is dead for stock.**
`llusbdac.ko` does not exist in the stock rootfs at all. It is a Wampy/Walkman One add-on.
Stock USB-DAC mode is handled by `libUsbDeviceAudioPlayerService.so` (Sony-built) which
writes PCM to ALSA via `libaudiohal-uacalsasingletrack.so`. So Candidate 3 (kernel-fixed
sink) is ruled out for stock.

**2. The USB mode manager is not the gate.**
`libUsbMgrServiceFw.so` (the service deciding MSC/MTP/ADB/UAC) contains **zero** references
to Bluetooth, BT, disconnect, or disable. It only manages USB power supply mode from a UAC
host. So Candidate 2b (USB-mode manager forcing BT teardown) is ruled out.

**3. The audio framework supports concurrent tracks of different types.**
`libSoundServiceFw.so` logs *"Cannot create multiple tracks that have **same type**"* — i.e.
the constraint is on duplicate types, not on coexistence of different types. The
`SoundServiceImpl::CreateTrack(TrackType)` and `OpenModulesIn(TrackType)` API explicitly
parameterizes by type. Two evidence-grade indicators that this is real multi-track
infrastructure, not a vestige:
- `libaudiohal-dualtrackmixalsa.so` exists — a HAL plugin that **mixes two tracks** into a
  single ALSA sink.
- The full HAL plugin catalogue (`vendor/sony/lib/libaudiohal-*.so`):
  `a2dpsnksingletrack` (BT-receive sink, *Walkman as BT speaker*),
  `adleralsa` (CXD3778GF wrapper — references `/sys/module/snd_soc_cxd3778gf/parameters`),
  `analyzer`, `dualtrackmixalsa`, `genericalsa`, `listener`,
  `uacalsasingletrack` (USB-DAC mode → ALSA).

**4. The BT transmit path is a complete, callable service.**
`libBtTransmitterService.so` exposes:
- `SetCurrentSource(bool)` — declare which logical source is active
- `SetLdac(bool)` — enable/disable LDAC codec
- `SetLdacSoundQuality(BtLdacSoundQuality)` — Auto / 990 / 660 / 330 kbps
- `NotifyOpenAudio()` / `NotifyCloseAudio()` — open/close the streaming pipe
- `NotifyPcmPreferredSize(uint16_t)` — chunk size negotiation
- `GetCapabilities(vector<BtA2dpConfiguration>)` — query peer capabilities

The exported class factories `BtTransmitterServiceFactory::CreateInstance` and
`BtTransmitterServiceClientFactory::CreateInstance` mean a replacement player can
instantiate this service from outside.

**5. The block is in the app UI flow.**
`HgrmMediaPlayerApp` contains the embedded QML strings:
`disconnectMsgOverlay`, `1DisconnectView`, `1DisconnectComponent`, `1DisconnectModel`,
`1DisconnectWindowViewModel`, `GoToBluetoothSetting`, `usbDacDeviceWindow`.
These describe an overlay dialog shown when the user enters USB-DAC mode that prompts them
to disconnect Bluetooth, with a button that navigates to the Bluetooth settings page. The
exclusivity is therefore implemented as a UI screen plus an explicit BT-disconnect call,
not as a runtime constraint of the underlying services.

### H4. Architecture map (concrete services and binaries)

Sony bundles many services into a generic `hagodaemon` (Hagoromo daemon) host process. From
`init.hagoromo.rc` (extracted from the boot-image ramdisk, sector 2), the services
relevant to the USB-DAC→LDAC project are:

| `hagoromoN` | Services hosted | Relevance |
|---|---|---|
| `hagoromo8` | `UsbHostConnectionService`, `UsbDeviceConnectionService`, `UsbDeviceAudioPlayerService` | **USB-DAC input side** — receives PCM from USB-gadget UAC |
| `hagoromo11` | `SoundServiceFw` | **Central audio routing** — `SetSourceTypeTrack`, `CreateTrack(TrackType)` |
| `hagoromo22` | `UsbMgrServiceFw` | USB mode selector — confirmed *not* a BT gate |
| `hagoromo24` | `WiredHpServiceFw` | 3.5 mm headphone sink (current USB-DAC output target) |
| `hagoromo27` | `BtCommonService`, `BtTransmitterService`, `BtBleCommonService`, `BtBleRemoteService`, `BtPlayerService` | **LDAC output side** — encodes PCM and transmits via BlueZ |
| `hagoromo28` | `AudioInPlayerService`, `TunerPlayerService` | Audio input (line-in?) and FM tuner |

USB mode switching is driven by `setprop sys.sony.config <mode>` (per
`init.usbcfg.rc`); modes seen: `adb`, `uac` (USB Audio Class — i.e. USB DAC), `msc` (mass
storage), `root`/`unroot`. The `uac` path sets USB function = `audio_func,adb` and USB
product ID = `0x0B8C`. None of these toggles touch Bluetooth.

`HgrmMediaPlayerApp` links against `libc++.so.1` + `libcxxrt.so.1` — confirming the
clang/LLVM + libc++ ABI, which is what `baseline_v1.4.md §5.4` flagged as the toolchain
boundary for the C++ shim. Default ALSA via `etc/asound.conf` = `hw:0,4` (the
`cxd3778gf-icx-lowpower` low-power playback device per Wampy `ALSA.md`).

### H5. Concrete implementation plan for USB-DAC → LDAC

The goal is now an engineering problem with a clear shape, not a research one. The
replacement player needs to:

**Step 1 — Don't enforce the block.**
In the replacement player's QML/UI, do not show `disconnectMsgOverlay` when the user enters
USB-DAC mode, and do not call any BT-disconnect path. Sony's UI flow does both; ours
shouldn't. This step alone, without anything else, may already give a partial result on
device — worth verifying empirically (Phase E4/E5) before building more.

**Step 2 — Bridge USB-DAC PCM into the BT transmit pipeline.**
Two viable approaches:

*Approach A — drive the existing services directly.*
- Instantiate `BtTransmitterServiceClient` via its factory.
- Call `NotifyOpenAudio()`, `SetLdac(true)`, `SetLdacSoundQuality(...)`.
- Tap PCM from `UsbDeviceAudioPlayerService` (either via its existing IPC, or by replacing
  `libaudiohal-uacalsasingletrack.so` with a build that writes to a different sink).
- Push frames using whatever `pst::audiohal::AudioHalOutA2dpSnk`'s inverse looks like for
  the source direction. This needs Ghidra time on `libBtTransmitterService.so` to find the
  PCM-write entry point — symbols visible so far are notification-only.
- This is the **clean** approach but requires the C++ ABI shim (clang/libc++) per §5.4.

*Approach B — ALSA loopback bridge.*
- Build/insmod `snd_aloop.ko` (kernel ALSA loopback module — would need a build matching
  the device kernel, 3.10.26-mt8590).
- Reconfigure `etc/asound.conf` so the UAC path writes to `hw:Loopback,0`.
- Run a small userspace daemon that reads `hw:Loopback,1` and writes to BlueZ A2DP via a
  socket interface (or via the BlueZ-ALSA bridge if shipped/buildable on this device).
- This sidesteps the C++ ABI issue (the bridge is pure C/Rust + ALSA + sockets) at the cost
  of a kernel module rebuild and an extra process in the audio path (~ few ms latency).

Approach A is architecturally cleaner; Approach B is more conservative and avoids touching
Sony's closed C++ surface. Pick after the device is in hand (Part E) — `strace` on
`UsbDeviceAudioPlayerService` and `BtTransmitterService` during stock USB-DAC mode will
make the choice obvious.

**Step 3 — Latency expectation setting.**
Stock USB-DAC mode already has noticeable latency; LDAC adds ~150-200 ms. Stacking them
makes the feature **unusable for video/gaming** but fine for music listening, which is the
only sensible use case. The replacement player's UI for this mode should not advertise it
as video-capable.

### H6. Open questions for Phase E (device-side, when device is in hand)

These are the questions host-side analysis cannot answer. Run these on the device after
the Phase E0 wbrt backup:

1. Does `BtTransmitterService` accept `NotifyOpenAudio()` while `UsbDeviceAudioPlayerService`
   is also open? (i.e. is there any runtime mutex inside `SoundServiceFw` we missed?)
2. What's the actual ALSA output device that `libaudiohal-uacalsasingletrack` writes to —
   confirm `hw:0,4` via `cat /proc/asound/card0/pcm*/sub*/status` during stock USB-DAC
   playback.
3. Does the BT A2DP source path have an ALSA-side entry point we can write PCM into (would
   make Approach B trivial), or does it pull PCM from `SoundServiceFw` over IPC only?
4. Confirm the `strace` finding from `baseline_v1.4.md §5.10`: does
   `HgrmMediaPlayerApp` actually call `disconnect`/`bluez` paths when entering USB-DAC mode,
   or is it purely the UI overlay that asks the user to do it?

Once these four are answered, the implementation is mechanical.
