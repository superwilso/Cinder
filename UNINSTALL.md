Three levels, pick by how permanent you want it.

## 1. Just boot to stock (reversible, no uninstall)

Any of these — the device still has Cinder installed, it just doesn't launch:

```bash
adb shell 'touch /data/cinder/off; sync'     # stock every boot until you remove it
adb shell 'rm -f /data/cinder/off'           # undo
```

Or with no shell at all: **plug the USB cable in and power on** — cable-at-boot is the escape that needs no filesystem. Or drop an empty `/contents/cinderhome_off` over USB-MSC. Or Settings ▸ Boot to stock inside Cinder (fires once, next boot returns to Cinder).

## 2. Real uninstall over adb

The whole install is one repointed `.appcfg` plus files under `/system/vendor/unknown321/bin`. Restoring the `.appcfg` *is* the uninstall.

**Verify the stock backup exists before touching anything** — it is the only copy of Sony's launch config:

```bash
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg.real'
```

Expect four lines with `command: HgrmMediaPlayerApp`. If that comes back empty, **stop** — go to option 3 instead.

Then:

```bash
adb shell 'mount -o remount,rw /system'

# restore stock launch config (this alone returns the device to stock)
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg.real > /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg; sync'
adb shell 'cat /system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg'   # confirm it reads command: HgrmMediaPlayerApp

# remove the binaries and helpers
adb shell 'rm -f /system/vendor/unknown321/bin/cinder-home \
                 /system/vendor/unknown321/bin/cinderhome-launch.sh \
                 /system/vendor/unknown321/bin/cinder-umount \
                 /system/vendor/unknown321/bin/cinder-gpunode \
                 /system/vendor/unknown321/bin/cinder-power \
                 /system/vendor/unknown321/bin/cinder-msc \
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

Note this needs the **dev** channel build, which is what's on the device now (that's how we've been pushing all session). Stock channel has no adb.

## 3. Flash the uninstaller UPG

Works even when the device won't boot far enough for adb, because the updater runs before any of our code:

```bash
tools/flash.sh /mnt/c/Users/ABDPa/Downloads/cinder_uninstall.upg
```

One caveat I can't resolve from memory: `RECOVERY.md` calls this file `cinder_uninstall.upg` in the flash example but `cinder_home_uninstall.upg` in the ladder table. Check which one actually exists in Downloads before flashing.

**Recommendation:** option 2. It's exact, reversible (reinstall is three `--push`es plus the install UPG), and doesn't depend on the updater.