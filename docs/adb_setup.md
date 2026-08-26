# adb on the NW-A55 — fast iteration + RE access

The Cinder **dev channel** enables adb at boot (`build.sh dev` → `cinder_home_install.upg`).
The binary adds `adb` to the USB gadget composite and starts `adbd`, guarded and best-effort
(`main.cpp` `deferred_up`), touching no boot-critical files. The **stable** channel never
enables adb. With adb up, the flash→unplug→read-log loop shrinks to seconds, and the device
becomes directly inspectable for RE.

## 1. One-time setup

### Windows side (recommended first — no passthrough games)

1. Install platform-tools: `winget install Google.PlatformTools` (or unzip from
   developer.android.com). `adb version` in PowerShell to confirm.
2. Flash the dev channel (`tools/flash.sh install` from WSL, or the usual UPG route).
3. Reboot the device with USB connected, wait ~20 s for cinder-home's deferred bring-up,
   then:
   ```powershell
   adb devices        # expect one device (Sony VID 054C); "unauthorized" won't appear —
                      # this old adbd has no auth prompt
   adb shell          # root shell (the device runs everything as root)
   ```
4. If `adb devices` is empty: Device Manager → the Walkman's ADB interface may need the
   generic "Android ADB Interface" driver (Google USB driver, "have disk" install). MTP
   still works alongside — the composite is `mtp,adb` / `mass_storage,adb`.

### WSL2 side (two options)

**Option A — talk to the Windows adb server (zero drivers in WSL, preferred):**
```bash
sudo apt install -y android-tools-adb            # client only
# Point the WSL client at the Windows host's adb server (start it on Windows once: adb start-server)
export ADB_SERVER_SOCKET=tcp:$(ip route show default | awk '{print $3}'):5037
adb devices
```
Caveat: the Windows server must allow remote connections: start it as
`adb -a nodaemon server` (or `adb kill-server; adb -a start-server`) on Windows, and allow
port 5037 on the Windows firewall for the WSL subnet. Add the `export` to `~/.bashrc`.

**Option B — usbipd passthrough (native USB in WSL; also what flash.sh MSC mode uses):**
```powershell
# Windows admin PowerShell
winget install usbipd
usbipd list                    # find the Walkman BUSID (VID 054C)
usbipd bind --busid <BUSID>
usbipd attach --wsl --busid <BUSID>
```
```bash
# WSL
lsusb          # Sony device present
adb devices
```
Re-run `usbipd attach` after every replug/reboot (or `usbipd attach --wsl --auto-attach`).
Note: while attached to WSL, Windows loses the device (MTP/adb there) until `usbipd detach`.

## 2. Fast iteration loop (no UPG reflash)

The installed binary lives at `/system/vendor/unknown321/bin/cinder-home`; appmgr launches
it via `/system/vendor/unknown321/bin/cinderhome-launch.sh` under the name
`HgrmMediaPlayerApp`. To swap in a fresh build:

```bash
bash cinder-home/build.sh dev
adb push cinder-home/dist/dev/cinder-home /contents/cinder-home.new
adb shell 'mount -o remount,rw /system &&
           cp /contents/cinder-home.new /system/vendor/unknown321/bin/cinder-home.tmp &&
           cmp /contents/cinder-home.new /system/vendor/unknown321/bin/cinder-home.tmp &&
           mv /system/vendor/unknown321/bin/cinder-home.tmp /system/vendor/unknown321/bin/cinder-home &&
           sync && mount -o remount,ro /system && reboot'
```
Atomic (temp → verify → mv), same pattern as the installer. **Reboot, don't kill**: the
launcher's bad-boot counter treats a dying cinder-home as a crash — NEVER `kill -9` the
running app to "restart" it (standing rule), reboot instead.

Logs, live:
```bash
adb shell tail -f /contents/cinderhome.log      # our stdout/stderr redirect
adb shell 'tail -f /tmp/cinder_msc.log'         # while USB-MSC mode is active
```

First-boot health check after a swap: screen paints, `adb shell ps | grep cinder`, log free
of `GUARD` recoveries.

## 3. RE access the log-file loop can't give you

```bash
# Pull the REAL library DB (verifies cinder-db's schema + the images/art table shapes):
adb pull /db/MTPDB.dat artifacts/device_pull/MTPDB.dat

# Pull any Sony lib fresh from the running device (vs our extracted rootfs):
adb pull /system/vendor/sony/lib/libPlayerServiceClientUtil.so artifacts/device_pull/

# Observe the stock services live:
adb shell getprop | grep -iE 'usb|sony'         # gadget mode, platform props
adb shell 'cat /sys/class/android_usb/android0/functions'
adb shell 'cat /proc/asound/card0/pcm*p/sub0/status'   # which ALSA device is RUNNING
adb shell ps | grep hagoromo                    # the service host processes

# PlayStatus offset mapping (cinder-probe --discover) without a flash cycle:
adb push cinder-home/dist/dev/cinder-probe /contents/
adb shell 'LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/lib:/usr/lib \
  /system/vendor/unknown321/bin/cinder-probe --discover'   # NOT /contents — that is noexec
```

Pulling `/db/MTPDB.dat` is the highest-value first step: it closes the album-art open
question (does `images.bmpfile` point at real files, or is art at `value`+`dataoffset`?)
with `sqlite3 MTPDB.dat 'SELECT bmpfile,value,dataoffset,datasize FROM images LIMIT 5'`.

## 4. Safety notes

- adb shell is root with no auth on this device — leave the **stable** channel (no adb) on
  it when you're not actively developing.
- `/system` is normally read-only; always remount ro after writing, always `sync`.
- Don't `adb push` directly over the installed binary while it's running (text file busy /
  torn write) — the temp→cmp→mv pattern above avoids both.
- The wbrt eMMC backup remains the recovery of last resort; adb changes nothing there.
