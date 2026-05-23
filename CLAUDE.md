# NW-A55 Project — Setup & Next Steps (v1.4)

**Purpose:** A single onboarding document that takes you from a stock Windows PC to a
working analysis environment, runs the existing host-side pipeline, and then adds the
device-side procedure for when the NW-A55 is in hand.

**Relationship to existing repo:** The repo already has a 7-phase host-side pipeline
(`Makefile`, `phases/phase1`–`phase7`, `CLAUDE.md`, `docs/`). This document does **not**
replace those — it wraps them with (a) Windows/WSL environment setup the phases assume but
don't provide, and (b) a new device-side procedure ("Phase 8") that the host-side phases
explicitly defer.

**v1.4 status note:** Two items in the existing repo carry stale framing and should be
updated to match the v1.4 findings:
- `CLAUDE.md` "Critical Corrections" table says *"SoC is MT8590 → Unknown."* This is now
  **reversed**: the MT8590 is confirmed (see `docs/baseline_v1.4.md` §4.1). Phase 3 changes
  from *discovery* to *confirmation*.
- `phases/phase3_soc_id.sh` writes "SoC: UNRESOLVED" as its default verdict. Update its
  expected-result text to "confirm MT8590" rather than "identify unknown SoC."

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
  feature before E4/E5.
