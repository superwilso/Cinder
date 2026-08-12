# Install — build, flash, and iterate on Cinder for the NW-A55

End-to-end guide: from a fresh Linux box to a device running Cinder, then the
fast adb iteration loop. If you only read one section, read [§2 First-time
install](#2-first-time-install) once and [§4 Iterate via adb](#4-iterate-via-adb)
forever after.

> **Read [`RECOVERY.md`](RECOVERY.md) before flashing anything.** This device
> has no public DFU/EDL recovery path — a bad flash means a full `wbrt` eMMC
> restore. Cinder's safety model (bad-boot counter, crash supervisor, escape
> ladder ordered so each rung depends on strictly less than the one it rescues)
> exists because of a real brick during development.

---

## 0. What you're installing

Cinder is **not an Android APK**. It's a cross-compiled ARM hard-float Linux
binary (a C++ "shell" + Rust UI/FFI staticlib) that runs in place of Sony's
stock Qt player on the NW-A55's embedded Linux, while keeping Sony's audio
services (DSP, codecs, LDAC, EQ) intact underneath via binder IPC.

Two install paths exist, for two different situations:

| Path | When | How |
|---|---|---|
| **First-time install** | Cinder is not yet on the device | USB-MSC + Sony `.UPG` firmware updater (`tools/flash.sh`) |
| **Iteration** | Cinder dev channel is already running | adb push + atomic swap (`tools/cinder-install.sh`) |

There is no overlap. adb does not exist on a stock device — the **dev channel
of cinder-home enables adbd at boot** (`main.cpp` `deferred_up`), so adb only
becomes available *after* the first install. The stable channel never enables
adb.

---

## 1. Prerequisites

### 1.1 Host environment

Cinder is built on **WSL2 Ubuntu** (per `CLAUDE.md`). A bare-metal Linux box
works too. macOS is not supported (the cross toolchain setup assumes Linux).

Required packages:

```bash
sudo apt update
sudo apt install -y \
    git binwalk dtc file readelf strings nm qemu-arm-static \
    cargo rustup clang clang-18 lld llvm \
    python3 python3-pip \
    android-tools-adb \
    libssl-dev pkg-config \
    gcc-arm-linux-gnueabihf \
    exfat-fuse exfatprogs
```

Plus the musl cross-compiler for the static setuid helpers:

```bash
# Option A: distro package
sudo apt install -y musl-tools arm-linux-musleabihf-gcc

# Option B (if distro package unavailable): musl.cc prebuilt
mkdir -p ~/toolchains
cd ~/toolchains
curl -fLO https://musl.cc/arm-linux-musleabihf-cross.tgz
tar xf arm-linux-musleabihf-cross.tgz
# Put on PATH: export PATH="$HOME/toolchains/arm-linux-musleabihf-cross/bin:$PATH"
```

### 1.2 The two toolchains `build.sh` checks for

`cinder-home/build.sh` will refuse to start without both of these. They take
~10 minutes to set up the first time and are the most common reason a fresh
clone won't build.

#### 1.2.1 libc++ 3.9.0 headers (device ABI match)

The device runs libcxx-3.9.0 (clang 3.9, Chromium-OS, 2016). Its
`std::function` layout (24B, functor ptr @+16) differs from modern libc++ —
building with libc++18 **corrupts the `CuiAppModule` callbacks → hang in
`OnInitialize` on device**. You MUST use the 3.9.0 headers for the C++ shell:

```bash
mkdir -p ~/toolchains
cd ~/toolchains
curl -fLO https://releases.llvm.org/3.9.0/libcxx-3.9.0.src.tar.xz
tar xf libcxx-3.9.0.src.tar.xz
# -> ~/toolchains/libcxx-3.9.0.src/include
```

The default path `build.sh` looks for is `$LIBCXX_V1` →
`~/toolchains/libcxx-3.9.0.src/include`. Override with:

```bash
export LIBCXX_V1=/path/to/libcxx-3.9.0.src/include
```

#### 1.2.2 glibc-2.23 sysroot (xenial armhf)

The device is glibc 2.23 (2016). The host's cross-gcc is glibc 2.39, which
emits `GLIBC_2.28..2.34` symbol refs the device's `ld-2.23` **refuses**. The
fix is a glibc-2.23 sysroot built from Ubuntu 16.04 "xenial" armhf `.debs`:

```bash
mkdir -p ~/toolchains/xenial-armhf-sysroot
cd ~/toolchains/xenial-armhf-sysroot
B=http://ports.ubuntu.com/ubuntu-ports/pool/main/g/glibc
curl -fLO $B/libc6-dev_2.23-0ubuntu11.3_armhf.deb
curl -fLO $B/libc6_2.23-0ubuntu11.3_armhf.deb
for d in *.deb; do dpkg-deb -x "$d" sysroot; done
# -> ~/toolchains/xenial-armhf-sysroot/sysroot
```

The default path `build.sh` looks for is `$DEVSYS` →
`~/toolchains/xenial-armhf-sysroot/sysroot`. Override with:

```bash
export DEVSYS=/path/to/sysroot
```

### 1.3 Rust target

```bash
rustup target add arm-unknown-linux-gnueabihf
```

### 1.4 Rockbox tooling (`upgtool`, `scsitool`)

Required only for the first-time install (the `.UPG` packer and the SCSI
backchannel that triggers the Sony updater). Built by the project's own
analysis pipeline:

```bash
make phase1    # builds upgtool + scsitool under artifacts/
```

This also requires the firmware-analysis dependencies listed in `CLAUDE.md`
Part B. If you only want to iterate via adb (Cinder is already installed),
you can skip this step.

### 1.5 Verify the toolchain

```bash
# All four should resolve:
[ -f "$LIBCXX_V1/functional" ] && echo "libc++ 3.9.0 OK" || echo "MISSING: libc++ 3.9.0"
[ -d "$DEVSYS/usr/lib/arm-linux-gnueabihf" ] && echo "glibc 2.23 sysroot OK" || echo "MISSING: sysroot"
rustup target list --installed | grep -q arm-unknown-linux-gnueabihf && echo "Rust target OK" || echo "MISSING: rust target"
command -v arm-linux-musleabihf-gcc >/dev/null && echo "musl cross OK" || echo "MISSING: musl cross"
```

---

## 2. First-time install

> **Brick warning.** This path writes to the device's firmware-update slot.
> `exec_file.sh` clears the upgrade flag before running, so a failed payload
> does not boot-loop — but a wedged payload still requires `wbrt` eMMC restore.
> Take a `wbrt` backup first (see `RECOVERY.md` step 3) and never overwrite it.

### 2.1 Build

```bash
# From the repo root:
bash cinder-home/build.sh dev
bash cinder-home/tools/pack_upg.sh dev
```

Use **`dev`** for the first install — it enables adb at boot, which you need
for iteration. `stable` exists for daily-use builds with no adb.

`build.sh` runs six stages: Rust `cinder-ffi` staticlib → C++ shell
(`-stdlib=libc++ -fno-rtti`) → glibc-2.23 compat shim → link against xenial
2.23 crt + device shared libs → GLIBC ≤2.23 ceiling gate + guard self-test +
qemu construction preflight + 44-case launcher recovery matrix → static setuid
helpers (`cinder-umount`, `cinder-gpunode`, `cinder-power`, `cinder-msc`).
Everything lands in `cinder-home/dist/dev/`.

### 2.2 Put the device in USB-MSC mode

On the stock Walkman UI: **Settings → USB Mode → Mass Storage** (not MTP, not
ADB — those don't expose the block device).

Then plug the device into the host. On WSL2 you also need `usbipd` to pass
the USB device through from Windows:

```powershell
# Admin PowerShell on Windows
usbipd list                       # find the Walkman BUSID (PID 054C:0CA0 = MSC mode)
usbipd bind --busid <BUSID>
usbipd attach --wsl --busid <BUSID>
```

Verify from WSL:

```bash
lsblk | grep -i sony              # should see /dev/sdX
```

### 2.3 Push the binaries

The `.UPG` payload does NOT contain the `cinder-home` binary — it expects the
binary and the setuid helpers to already be staged at the root of the device's
user partition (`/contents` on the device, mounted as the FAT partition you
see over USB).

```bash
# Stage every binary the installer needs. Each one is a separate push.
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-home
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-umount
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-power
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-msc
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-gpunode   # dev-only helper
```

`flash.sh --push` mounts the device's FAT partition, copies the file to its
root, syncs, and unmounts. If you skip a helper, the installer silently
degrades (no `cinder-umount` → USB-MSC cannot unmount `/contents` as uid 100;
no `cinder-gpunode` → the GPU path can never be enabled).

### 2.4 Flash the install payload

```bash
sudo tools/flash.sh cinder-home/dist/dev/cinder_home_install.upg
```

This copies the `.UPG` to the device root as `NW_WM_FW.UPG`, asks for
confirmation, then fires `scsitool do_fw_upgrade`. The Walkman:

1. Reboots into the Sony UPDATER
2. Runs `exec_file.sh` (clears the fw-upgrade flag — brick-safe)
3. Runs `install_cinderhome.sh` as root (atomic copy, repoints
   `HgrmMediaPlayerApp.appcfg`, installs the launcher)
4. Reboots into Cinder

The device drops off USB during the upgrade — that's expected.

### 2.5 Verify the first boot

Wait ~20 s after the reboot (cinder-home's deferred adb bring-up takes a
moment), then:

```bash
adb devices                       # expect one Sony VID 054C device
adb shell ps | grep cinder        # cinder-home should be running
adb shell tail /contents/cinderhome.log
```

If the screen paints and the log is free of `GUARD` recoveries, you're in.
If it boot-loops or lands on stock, see [§6 Troubleshooting](#6-troubleshooting).

### 2.6 First-boot escape hatch (important)

**Boot with the USB cable connected → stock.** This is rung 0 of the escape
ladder in `RECOVERY.md` and works no matter how broken the filesystem is. The
cost is that charging at boot also lands on stock — disable that for
cable-heavy dev with `/data/cinder/cable_escape_off` (or
`/contents/cinderhome_cable_off`).

For the first install, **boot with the cable UNPLUGGED** so Cinder actually
runs.

---

## 3. Build channels

`build.sh` builds two channels from one tree. They never clobber each other
(artifacts land in `dist/<channel>/`).

| Channel | Use | adb | Notes |
|---|---|---|---|
| `dev` (default for iteration) | Active development | **Enabled at boot** | Adds a visible "CINDER DEV" marker; `cinder-gpunode` ships (dev-only, setuid-root, GPU node permissions) |
| `stable` | Daily use | **Never** | No adb, no gpunode, no dev marker |

```bash
bash cinder-home/build.sh dev        # dev channel
bash cinder-home/build.sh stable     # stable channel
bash cinder-home/tools/pack_upg.sh dev     # pack the .UPG payloads
```

### 3.1 Build warnings you can ignore

Two warnings show up on every clean build. Neither indicates a problem:

```
clang++-18: warning: argument unused during compilation: '-stdlib=libc++'
cinder-home/src/main.cpp:4389:13: warning: unused function 'prop_equals'
```

- **`-stdlib=libc++` unused**: `build.sh` passes this flag to the `-c`
  compile-only step where it has no effect (it's a linker flag). The libc++
  headers are already selected explicitly via `-nostdinc++ -isystem
  "$LIBCXX_V1"`. Vestigial; harmless.
- **`prop_equals` unused**: leftover from an earlier refactor (commit
  `bf15b26`) that removed the only caller. Dead code, harmless. Delete the
  function (around `main.cpp:4389`) if it bothers you.

### 3.2 Build without a device

The Rust UI and a host-side simulator can be built with no cross toolchain
and no device:

```bash
cd player
cargo build --release -p cinder-host    # renders every screen to PNG
cargo build --release -p cinder-sim --bin device    # 480x800 window, real navigator
```

Useful for UI iteration without the device bring-up cost.

---

## 4. Iterate via adb

Once the dev channel is installed and running, this is your normal loop.
**Seconds per iteration, no USB-MSC, no `.UPG` reflash.**

### 4.1 The one-command installer

```bash
./tools/cinder-install.sh
```

This is the recommended way to iterate. It does, in order:

1. Build the dev channel (`bash cinder-home/build.sh dev`)
2. Push `cinder-home` to `/data/local/tmp/cinder-home.new` (ext4, safe from
   MSC — never use `/contents` for adb pushes, see [§5 MSC trap](#5-the-msc-mode-trap))
3. Back up the current binary to `/data/cinder/cinder-home.last` (for
   `--rollback`)
4. Arm `/data/cinder/no_respawn` (the project's documented kill switch —
   prevents the crash supervisor from respawning)
5. Kill the running `cinder-home` cleanly (launcher stays alive as appmgr's
   direct child, so appmgr does NOT reboot)
6. `mount -o remount,rw /system` (works now that cinder-home isn't holding
   files open — falls back to `mount -t ext4 -o rw,remount /emmc@android
   /system` if plain remount fails, matching the installer pattern)
7. Atomic swap: `cp` to `.tmp` → `cmp` → `mv` (a torn write leaves the old
   binary intact)
8. `mount -o remount,ro /system` + `sync` + `reboot`

Full options:

```bash
./tools/cinder-install.sh                # build dev + install + reboot (default)
./tools/cinder-install.sh --no-build     # skip build, install existing dist/dev/
./tools/cinder-install.sh --stable       # use stable channel (NO adb next boot!)
./tools/cinder-install.sh --full         # also push + chmod 4755 the setuid helpers
./tools/cinder-install.sh --rollback     # restore previous binary from /data/cinder/
./tools/cinder-install.sh --logs         # tail /contents/cinderhome.log
./tools/cinder-install.sh --status       # device + install health check
./tools/cinder-install.sh -h             # help
```

### 4.2 Manual adb swap (what the installer does, expanded)

If you want to see exactly what's happening, or the installer fails and you
need to debug:

```bash
# 1. Build
bash cinder-home/build.sh dev

# 2. Push to /data/local/tmp (NOT /contents — see §5)
adb push cinder-home/dist/dev/cinder-home /data/local/tmp/cinder-home.new

# 3. Arm no-respawn + kill cinder-home (releases /system)
adb shell 'touch /data/cinder/no_respawn && kill $(pidof cinder-home)'
sleep 2

# 4. Atomic swap
adb shell 'mount -o remount,rw /system && \
           cp /data/local/tmp/cinder-home.new /system/vendor/unknown321/bin/cinder-home.tmp && \
           cmp /data/local/tmp/cinder-home.new /system/vendor/unknown321/bin/cinder-home.tmp && \
           mv /system/vendor/unknown321/bin/cinder-home.tmp /system/vendor/unknown321/bin/cinder-home && \
           sync && mount -o remount,ro /system'

# 5. Disarm no-respawn + reboot
adb shell 'rm /data/cinder/no_respawn && reboot'
```

### 4.3 Why `--no-build` exists

If you're debugging a build failure, you can push a known-good binary without
rebuilding:

```bash
./tools/cinder-install.sh --no-build
```

### 4.4 When to use `--full`

Use `--full` when you've changed any of the setuid helpers
(`cinder-umount`, `cinder-power`, `cinder-msc`, `cinder-gpunode`). It pushes
each helper, atomically swaps it in, and restores the `4755 root:root`
permissions. Without `--full`, helpers stay at whatever's on the device —
they only change when you do a full `.UPG` reinstall.

### 4.5 adb setup gotchas

- **No auth prompt on this device.** `adb devices` will never show
  "unauthorized" — the old adbd has no auth. You get a root shell directly.
- **Driver install on Windows.** If `adb devices` is empty from native
  Windows: Device Manager → the Walkman's ADB interface may need the generic
  "Android ADB Interface" driver (Google USB driver, "have disk" install).
- **WSL2 + adb.** Two options (see `docs/adb_setup.md` §1):
  - **Option A (recommended):** Talk to the Windows adb server from WSL.
    `export ADB_SERVER_SOCKET=tcp:$(ip route show default | awk '{print $3}'):5037`
  - **Option B:** `usbipd attach --wsl --busid <BUSID>` for native USB in
    WSL. Re-run after every replug/reboot, or use `--auto-attach`.

### 4.6 Logs and inspection

```bash
./tools/cinder-install.sh --logs                        # tail the boot log
adb shell tail -f /tmp/cinder_msc.log                   # while USB-MSC mode is active
adb shell ps | grep cinder                              # is it running?
adb pull /db/MTPDB.dat artifacts/device_pull/MTPDB.dat  # real library DB
adb shell 'cat /proc/asound/card0/pcm*p/sub0/status'    # which ALSA device is live
adb shell getprop | grep -iE 'usb|sony'                 # gadget mode, platform props
adb push cinder-home/dist/dev/cinder-probe /contents/   # diagnostic binary
adb shell '/contents/cinder-probe --discover'           # PlayStatus offset map
```

---

## 5. The MSC mode trap

> **Never toggle USB-MSC mode while iterating via adb.**

This is the single most common way to lose work on this project. The sequence
that bites you:

1. `adb push foo /contents/foo` — file lands on the vfat partition
2. You enable MSC mode (Settings → USB Mode → Mass Storage, or from the
   Cinder settings menu)
3. The device **unmounts `/contents` from its own filesystem** and exports
   the underlying block device to the PC as a USB drive
4. MSC mode's LUN setup can write the backing file / reformat the exported
   view. If the vfat unmount was unclean (documented failure mode — see
   `cinder-home/STATUS.md` lines 361–366), the just-pushed file gets lost
5. You disable MSC mode. `/contents` remounts. The file is gone.

**The rule:** `/contents` is for the one-time first install (§2) and for the
user-facing "transfer music from PC" feature. Once you have adb up, stay in
adb. Push to `/data/local/tmp` instead — it's ext4, journaled, never touched
by MSC.

If you need to push a file the user-facing way (e.g. a config file at the
root of `/contents`), use the Cinder UI's own USB-MSC feature, push from the
PC, eject cleanly, then return to adb.

---

## 6. Troubleshooting

### 6.1 `mount: Device or resource busy` on `/system` remount

**Cause:** the running `cinder-home` has files open on `/system` (libraries,
the binary itself), so the kernel refuses the remount.

**Fix:** kill cinder-home first, with the no-respawn flag armed so the crash
supervisor doesn't respawn it:

```bash
adb shell 'touch /data/cinder/no_respawn && kill $(pidof cinder-home)'
sleep 2
# Now the remount will succeed.
```

This is exactly what `./tools/cinder-install.sh` does automatically.

### 6.2 `cp: can't stat '/contents/cinder-home.new': No such file or directory`

**Cause:** you pushed to `/contents` and then toggled MSC mode. See [§5 The
MSC mode trap](#5-the-msc-mode-trap).

**Fix:** push to `/data/local/tmp` instead. Use `./tools/cinder-install.sh`,
which does this by default.

### 6.3 Build fails: `ERR: libc++ 3.9.0 headers not at $LIBCXX_V1`

You haven't set up the libc++ 3.9.0 headers. See [§1.2.1](#121-libc-390-headers-device-abi-match).

### 6.4 Build fails: `ERR: glibc-2.23 sysroot not at $DEVSYS`

You haven't set up the xenial sysroot. See [§1.2.2](#122-glibc-223-sysroot-xenial-armhf).

### 6.5 Build fails: `ERROR: arm-linux-musleabihf-gcc not found`

You need the musl cross-compiler for the static setuid helpers. See [§1.1](#11-host-environment).

### 6.6 Build succeeds but device hangs in `OnInitialize`

You built with the wrong libc++ headers (probably modern libc++18 instead of
3.9.0). The `CuiAppModule` callback ABI is corrupted. Re-check `$LIBCXX_V1`
points at the 3.9.0 tree and rebuild.

### 6.7 Build succeeds but device refuses to boot the binary

Check the GLIBC ceiling. `build.sh` runs `gate_glibc` which fails the build
if the binary needs `GLIBC_2.24+`, but if you bypassed it:

```bash
arm-linux-gnueabihf-readelf -V cinder-home/dist/dev/cinder-home | grep GLIBC
```

The device is glibc 2.23 — any `GLIBC_2.24+` symbol will fail to load.
Usually means the xenial sysroot wasn't actually used (re-check `$DEVSYS`).

### 6.8 cinder-home installed but boots to stock

Possible causes, in order of likelihood:

1. **USB cable was plugged in at boot.** Rung 0 of the escape ladder → stock.
   Unplug and reboot.
2. **Bad-boot counter hit `MAXBAD=4`.** Cinder crashed 4 times in a row and
   the counter reverted to stock. Check `/contents/cinderhome.log` for crash
   patterns. Clear with `tools/flash.sh --clear-latch` (over MSC).
3. **`/data/cinder/no_respawn` is armed.** Remove it: `adb shell rm
   /data/cinder/no_respawn` (if adb is up) or delete via USB-MSC.

### 6.9 adb devices is empty

1. Is the dev channel actually installed? adb only exists once cinder-home
   dev is running. `adb shell getprop sys.usb.state` should say
   `mass_storage,adb` or `mtp,adb`.
2. If on Windows: driver issue. See [§4.5](#45-adb-setup-gotchas).
3. If on WSL2: did you `usbipd attach --wsl --busid <BUSID>` after the last
   replug/reboot?
4. Wait ~20 s after boot — cinder-home's deferred adb bring-up isn't
   instant.

### 6.10 The install UPG fails to apply

The Sony updater's ambient shell utilities are unreliable (documented in
`install_cinderhome.sh` header — a bare `wc -c` returned 0, causing a
false-abort on a good copy). The installer works around this by using
`/xbin/busybox` for every op. If you're seeing install failures, check:

```bash
# Re-attach in MSC mode and read the install log:
sudo tools/flash.sh --log
```

---

## 7. Recovery

Cinder's safety model is documented in [`RECOVERY.md`](RECOVERY.md). The
short version, in escalation order (each rung depends on strictly less than
the one below it):

| # | Escape | Depends on |
|---|--------|-----------|
| 0 | Boot with USB cable connected → stock | Nothing |
| 1 | Bad-boot counter hits `MAXBAD=4` → stock | A writable `/data` (ext4) |
| 2 | `/contents/cinderhome_off` over USB-MSC → stock | A mountable `/contents` + a PC |
| 3 | `/contents/cinderhome_clear` → clears the latch, retries | Same |
| 4 | Flash `cinder_home_uninstall.upg` | The Sony updater boots |
| 5 | `wbrt` eMMC restore | Nothing — but it wipes `/contents` |

**Rung 0 is the important one.** Plug the cable in, power on, and you land
on stock — no matter how broken the filesystem is.

### 7.1 Rollback to the previous binary

If a single iteration is bad but the device is still bootable (Cinder runs
but crashes, or you don't like the new behavior):

```bash
./tools/cinder-install.sh --rollback
```

This restores `/data/cinder/cinder-home.last` (which `cinder-install.sh`
saves before every swap) and reboots. Only one level of undo — if the
rollback is also bad, fall through the ladder above.

### 7.2 Full uninstall

```bash
sudo tools/flash.sh uninstall
```

Removes the wrapper entirely and restores the original scrobbler. See
[`UNINSTALL.md`](UNINSTALL.md) for details.

### 7.3 wbrt eMMC restore (last resort)

Native Windows only. Tool: `github.com/unknown321/wbrt/releases`. Driver:
MediaTek VCOM (`VID_0E8D&PID_2000`). Full procedure in `RECOVERY.md` step 3.

**Take a fresh `wbrt` Backup before you Restore** — a restore rewrites the
whole eMMC, and `/contents` (music library, playlists, `.scrobbler.log`) is
inside it. Save to a **new filename**; never overwrite the known-good image.

---

## 8. Quick reference

```bash
# First-time install (Cinder not yet on device):
bash cinder-home/build.sh dev
bash cinder-home/tools/pack_upg.sh dev
# → put device in MSC mode, attach via usbipd if WSL2
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-home
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-umount
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-power
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-msc
sudo tools/flash.sh --push cinder-home/dist/dev/cinder-gpunode
sudo tools/flash.sh cinder-home/dist/dev/cinder_home_install.upg
# → unplug USB, boot, wait 20s

# Iteration (Cinder dev already running):
./tools/cinder-install.sh                    # build + push + swap + reboot
./tools/cinder-install.sh --logs             # tail boot log
./tools/cinder-install.sh --status           # health check
./tools/cinder-install.sh --rollback         # restore previous binary

# Recovery:
# Boot with USB plugged in → stock (rung 0, always works)
sudo tools/flash.sh --clear-latch            # clear bad-boot latch (rung 3)
sudo tools/flash.sh uninstall                # full uninstall (rung 4)
# wbrt eMMC restore                          # last resort (rung 5)
```

---

## See also

- [`README.md`](README.md) — project overview, why, repo layout
- [`RECOVERY.md`](RECOVERY.md) — full recovery runbook, escape ladder, crash supervisor
- [`UNINSTALL.md`](UNINSTALL.md) — uninstall procedure
- [`CLAUDE.md`](CLAUDE.md) — full environment setup, host pipeline, device procedure writeup
- [`docs/adb_setup.md`](docs/adb_setup.md) — adb setup details (Windows + WSL2)
- [`docs/FLASH_NEXT.md`](docs/FLASH_NEXT.md) — run sheet for the next hardware session
- [`cinder-home/STATUS.md`](cinder-home/STATUS.md) — feature status matrix, what works / what doesn't
- [`cinder-home/build.sh`](cinder-home/build.sh) — the build script itself (heavily commented)
- [`tools/flash.sh`](tools/flash.sh) — the MSC + UPG flasher (heavily commented)
- [`tools/cinder-install.sh`](tools/cinder-install.sh) — the adb installer (heavily commented)
