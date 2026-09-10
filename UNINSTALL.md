# Uninstalling Cinder

Four ways, ordered by how much of the device has to be working for them to run. **The first one
is the answer for almost everybody.**

| | Removes Cinder | Needs | Use when |
|---|---|---|---|
| **1. The installer** | Yes, completely | A PC and a USB cable | Normal removal. This is the one to use. |
| 2. Boot to stock | No — just stops it launching | Nothing, or a USB cable | You want the stock player for one boot, or you are testing. |
| 3. adb | Yes, completely | The dev channel, adb working | You are developing and want it gone without a flash. |
| 4. Flash the uninstaller `.UPG` by hand | Yes, completely | Only the Sony updater | The device will not boot far enough for anything else. |

---

## 1. The installer (recommended)

Download the installer from the [latest release](../../releases/latest), connect the Walkman by
USB in mass-storage mode, and choose **Uninstall**. From a terminal:

```
cinder-installer --uninstall
```

It stages `cinder_home_uninstall.upg` as `NW_WM_FW.UPG` and nothing else, then hands off to the
player's own updater the same way an install does. The player restores Sony's launch config from
the backup the install made, deletes Cinder's binaries, and reboots into the stock Qt player.

**Your music, playlists, and settings on the data partition are not touched.** Neither is the
stock player itself — Cinder never modifies Sony's binary, only the `.appcfg` that says which app
to launch, and that is backed up to `.appcfg.real` on install.

It is a **no-op on a device that never had Cinder**, so running it because you are not sure is
harmless. The installer will tell you what it thinks is on the player before you confirm, read
out of the device's own install log.

Afterwards, `cinder-installer --clean` removes the staged payload files left in the drive root.

---

## 2. Just boot to stock (reversible, no uninstall)

Any of these — the device still has Cinder installed, it just doesn't launch:

```bash
adb shell 'touch /data/cinder/off; sync'     # stock every boot until you remove it
adb shell 'rm -f /data/cinder/off'           # undo
```

Or with no shell at all: **plug the USB cable in and power on** — cable-at-boot is the escape that
needs no filesystem. Or drop an empty `/contents/cinderhome_off` over USB-MSC. Or Settings ▸ Boot
to stock inside Cinder (fires once, next boot returns to Cinder).

---

## 3. Real uninstall over adb

The whole install is one repointed `.appcfg` plus files under `/system/vendor/unknown321/bin`.
Restoring the `.appcfg` *is* the uninstall.

This needs the **dev** channel, which is the only one that enables adb.

**Verify the stock backup exists before touching anything** — it is the only copy of Sony's launch
config:

```bash
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg.real'
```

Expect four lines with `command: HgrmMediaPlayerApp`. If that comes back empty, **stop** — use
method 1 or 4 instead.

Then:

```bash
adb shell 'mount -o remount,rw /system'

# restore stock launch config (this alone returns the device to stock)
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg.real > /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg; sync'
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg'   # confirm: command: HgrmMediaPlayerApp

# remove the binaries and helpers
adb shell 'rm -f /system/vendor/unknown321/bin/cinder-home \
                 /system/vendor/unknown321/bin/cinderhome-launch.sh \
                 /system/vendor/unknown321/bin/cinder-probe \
                 /system/vendor/unknown321/bin/cinder-umount \
                 /system/vendor/unknown321/bin/cinder-gpunode \
                 /system/vendor/unknown321/bin/cinder-power \
                 /system/vendor/unknown321/bin/cinder-msc \
                 /system/vendor/unknown321/bin/cinder-clock \
                 /system/vendor/unknown321/bin/cinder-fm \
                 /system/vendor/unknown321/bin/cinder-voltable \
                 /system/vendor/unknown321/bin/cinder-battery \
                 /system/vendor/unknown321/bin/ldac-run.sh'

# state, flags, logs, art cache
adb shell 'rm -rf /data/cinder'
adb shell 'rm -f /contents/cinderhome_off /contents/cinderhome_clear /contents/cinderhome_once \
                 /contents/cinderhome_DISABLED_badboot /contents/cinderhome_cable_off \
                 /contents/cinderhome_norespawn /contents/cinder_gpu_on /contents/cinder_gpu_off \
                 /contents/cinderhome.log /contents/cinderhome.log.1 /contents/ldac_off'

adb shell 'sync; umount /system'
adb reboot
```

Leave `HgrmMediaPlayerApp.appcfg.real` in place — harmless, and it's your backup if you reinstall.

**If you changed the sound signature or the volume curve**, undo those before removing the
binaries; they patch files that are *not* part of the Cinder install and survive an uninstall:

```bash
adb shell '/system/vendor/unknown321/bin/cinder-signature.sh stock'
adb shell 'echo stock > /contents/cinder_voltable.conf'
```

Method 1 and method 4 handle this for you — the uninstall package reverts both.

---

## 4. Flash the uninstaller `.UPG` by hand

Works even when the device won't boot far enough for adb, because the updater runs before any of
our code:

```bash
tools/flash.sh uninstall
```

That is the shortcut for `cinder-home/dist/dev/cinder_home_uninstall.upg`. The same package is
attached to every release as **`cinder-home-uninstall.upg`** if you are not working from a
checkout.

If the device will not enter mass-storage mode either, you are below the escape ladder and into
[`RECOVERY.md`](RECOVERY.md).

---

## What uninstalling does not undo

- **`HgrmMediaPlayerApp.appcfg.real`** stays. It is Sony's original launch config; leaving it costs
  nothing and it is what a reinstall restores from.
- **Your files.** Music, playlists (`/contents/cinder_playlists`), scrobbler logs and anything else
  you put on the data partition are left alone by every method here.
- **A stale media database.** If the library was mid-rescan when something interrupted it, that
  damage predates the uninstall and survives it — see
  [`docs/DEVICE_TESTS.md`](docs/DEVICE_TESTS.md) and restore
  `/contents/MTPDB_prescan_backup.dat`.
