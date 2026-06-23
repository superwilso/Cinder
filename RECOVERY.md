# NW-A55 Cinder — recovery runbook

If a flashed build misbehaves, work **top to bottom** — each step is more invasive than
the last. Most cases now self-heal at step 0.

## The guardrails that are built in (as of the safe `cinder_install.upg`)
- **Auto-revert (bad-boot counter).** The boot wrapper bumps `/contents/cinder_bootcount`
  on each Cinder-attempt boot. If Cinder reboots the device **3 times within 60 s**, it
  creates `/contents/cinder_off` + `/contents/cinder_DISABLED_badboot` and the **stock UI
  comes back on its own** (~2 min, no PC needed). A boot that survives 60 s clears the count.
- **SIGSTOP, not kill.** The stock Qt app is frozen (kept alive), so init's watchdog never
  forces a reboot. *(A true kill caused the 2026-06-23 boot loop — do not reintroduce it
  without first neutralising the watchdog.)*
- **Missing-binary guard.** If `cinder-device` isn't present+executable, the wrapper leaves
  the stock UI untouched (never freezes stock with nothing to show).
- **Verified copies.** Install uses `cat >` + `-s` checks; it won't write the wrapper unless
  the binary copied and the scrobbler backup (`scrobbler.real`) both verified.

## Step 0 — let the auto-revert do its job
If the screen is glitching/rebooting, **leave it alone for ~2 minutes.** After 3 bad boots
it auto-disables and stock returns. Confirm later via USB-MSC: `tools/flash.sh --ls` shows
`cinder_DISABLED_badboot`. To re-enable Cinder, delete `cinder_off`, `cinder_bootcount`,
and `cinder_DISABLED_badboot`, then reboot.

## Step 1 — manual escape hatch
If it's stable enough to mount as USB-MSC:
```bash
: > /tmp/cinder_off; : > /tmp/ldac_off
tools/flash.sh --push /tmp/cinder_off    # stock UI on next boot
tools/flash.sh --push /tmp/ldac_off      # stop the LDAC bridge supervisor
```

## Step 2 — flash the uninstaller
Removes the wrapper entirely and restores the original scrobbler:
```bash
tools/flash.sh /mnt/c/Users/ABDPa/Downloads/cinder_uninstall.upg
```
The next boot enters the Sony updater (which never runs the wrapper), so this breaks a loop.

## Step 3 — wbrt eMMC restore (guaranteed; native Windows)
This is brick-insurance and works even in a hard boot loop, because it catches the device in
**MediaTek mode at the very start of boot**, before any of our code runs.

- Backup file (this device, 2026-06-18, ~2.88 GB): `C:\Users\ABDPa\OneDrive\Desktop\walkman_backup.20260618_185658.bin`
- Tool: `C:\Users\ABDPa\Downloads\walkman-backup-restore-tool.v1.0.9.exe`
- Driver: `C:\Users\ABDPa\WalkmanBackupRestoreDriver\installer_x64.exe` (device = `VID_0E8D&PID_2000`)

Steps: run the driver installer → run wbrt → **Restore** → pick the backup → get the device
into MediaTek mode (below) → let it finish, don't disconnect.

### Getting a stubborn looping device into MediaTek mode (hard-won notes)
- **Detach usbipd first.** If the device is bound to WSL it's invisible to Windows/wbrt:
  `usbipd detach --busid <X>` and `usbipd unbind --busid <X>`; close any `--auto-attach` window.
  (Symptom: Device Manager shows *nothing* while it's plugged in and cycling.)
- **It won't power off because USB power reboots it — unplug the cable first**, then hold POWER
  ~15–20 s; keep holding *through* the dark screen (releasing at the dark moment lets it reboot).
- **Catching the preloader without powering off:** with wbrt at "waiting for device" and the
  correct driver, the ~1–2 s MediaTek preloader window at the start of every boot can be caught
  mid-loop.
- **Guaranteed:** leave it **unplugged and looping until the battery dies** (constant rebooting
  drains it in ~1–2 h). Once off, in wbrt start Restore → **hold VOL− and plug USB in while
  holding** → it enters BROM and holds there → restore.
- **Driver errors** ("Unrecognized device driver"/"Open failed"): `pnputil /enum-drivers`, then
  `pnputil -f -d oemNN.inf` for each `VID_0E8D&PID_2000` entry, reboot Windows, retry.
