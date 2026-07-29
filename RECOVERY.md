# NW-A55 Cinder — recovery runbook

If a flashed build misbehaves, work **top to bottom** — each step is more invasive than
the last. Most cases now self-heal at step 0.

## The escape ladder (rebuilt 2026-07-26 after a wbrt-requiring brick)

Each rung depends on **less** than the one below it, so try them in this order. This ordering is
the whole design: the 2026-07-26 brick happened because every escape that existed depended on
`/contents`, and `/contents` was the thing that had failed.

| # | Escape | Depends on |
|---|--------|-----------|
| 0 | **Boot with the USB cable connected → stock.** | Nothing. No filesystem, no shell, no counter. |
| 1 | **Bad-boot counter** hits `MAXBAD=4` → stock, by itself. | A writable `/data` (ext4). |
| 2 | `/contents/cinderhome_off` over USB-MSC → stock. | A mountable `/contents` + a PC. |
| 3 | `/contents/cinderhome_clear` over USB-MSC → clears the latch, tries again. | Same. |
| 4 | Flash `cinder_home_uninstall.upg`. | The Sony updater boots. |
| 5 | `wbrt` eMMC restore. | Nothing — but it wipes `/contents`. |

**Rung 0 is the important one.** Plug the cable in, power on, and you land on stock — no matter
how broken the filesystem is. The cost is that charging at boot also lands on stock; turn that off
for cable-heavy dev with `/data/cinder/cable_escape_off` (or `/contents/cinderhome_cable_off`).

**Why state lives on `/data`:** the counter used to live on `/contents`, which is **vfat** (no
journal) *and* is the partition handed to the PC for USB-MSC — so it is both corruptible and
routinely absent. When it failed to mount, the counter write went nowhere, the launcher's
`>/contents/cinderhome.log` redirect failed, `sh` exited **without exec'ing**, appmgr rebooted, and
the loop repeated forever with the safety net silently disabled. State is now on `/data`
(ext4, journaled, never touched by USB-MSC), the launcher refuses to run at all if it cannot
persist the counter, and the log redirect can no longer block the exec.

Other guardrails:
- **Never SIGKILL the stock Qt app.** It is frozen, not killed *(a true kill caused the
  2026-06-23 boot loop)*. In the current design `.appcfg` is repointed and the Qt binary is
  never touched at all.
- **Missing-binary guard.** No executable `cinder-home` → the launcher runs stock untouched.
- **Verified copies.** Install is atomic (temp → verify → mv) with a final sanity gate that
  reverts to stock if any piece is wrong.
- **Recovery gate in the build.** `cinder-home/tools/test_launcher.sh` drives the generated
  launcher through all 44 escape/failure/supervisor checks in a sandbox; `build.sh` refuses to
  pack if any fail. It is what caught the special-builtin `exec`-redirect bug above.

### The crash supervisor (added 2026-07-29)

The launcher no longer `exec`s cinder-home — it **stays alive and respawns it**. Before this, a
single segfault or allocation failure cost a full reboot: appmgr installs a `SIGCHLD` handler
(`AppManagerService::OnInit` → `sigaction(17, …)`) and `android_reboot`s on
*"Application process is killed! appmgrservice will exit…"*. Staying alive suppresses that
entirely, because appmgr's `SIGCHLD` only fires for **its own direct child** — it
`fork()`+`execvp()`s the launcher and waits on that pid (`ProcessController::WaitFinished`), so a
live shell reaping cinder-home itself looks exactly like a healthy foreground app. Same reason
`SIGSTOP` on the stock Qt app was safe where `SIGKILL` was not.

Only deaths that mean *"crashed"* are respawned — it is a **whitelist**, so an unrecognised exit
code falls through to the old reboot-and-count behaviour rather than disabling the net:

| Exit | Meaning | Supervisor |
|---|---|---|
| 132–136, 139, 141, 158/159 | ILL / TRAP / ABRT / BUS / FPE / SEGV / PIPE / XCPU-XFSZ | **respawn** |
| 0 | deliberate exit — Settings ▸ Boot to stock arms its flag then `_exit(0)` and *relies* on appmgr rebooting | hand back |
| 42 | self-diagnosed fatal (guard / watchdog), whose contract is "die fast so the counter reverts" | hand back |
| 143 / 137 | SIGTERM / SIGKILL — somebody killed us on purpose | hand back |
| anything else | unknown | hand back |

Escalation: **3 consecutive crashes inside 30 s**, or **10 crashes in one boot**, hands that boot
to the Sony player. Deliberately **not** a latch — the next boot tries Cinder again, because
latching on a runtime crash is how a device ends up stuck on stock forever. From the first respawn
onward the GPU present path is dropped (`CINDER_GPU=0`), since the Mali fbdev EGL stack is the
least proven code in the process and the software framebuffer is the proven one.

**Kill switch** — restores the pre-supervisor `exec`, and depends on strictly less than the
supervisor it disables:
```bash
: > /tmp/cinderhome_norespawn
tools/flash.sh --push /tmp/cinderhome_norespawn   # or: touch /data/cinder/no_respawn
```

## Step 0 — plug in the USB cable and power on
That is the escape. You land on stock. If you want the counter to do it instead, leave the device
alone for ~4 boot attempts.

## Step 1 — manual escape hatch over USB-MSC
If it's stable enough to mount as USB-MSC:
```bash
tools/flash.sh --clear-latch             # arms cinderhome_clear -> retry the installed build
: > /tmp/cinderhome_off; : > /tmp/ldac_off
tools/flash.sh --push /tmp/cinderhome_off  # stock UI on next boot
tools/flash.sh --push /tmp/ldac_off        # stop the LDAC bridge supervisor
```

## Step 2 — flash the uninstaller
Removes the wrapper entirely and restores the original scrobbler:
```bash
tools/flash.sh /mnt/c/Users/walkman/Downloads/cinder_uninstall.upg
```
The next boot enters the Sony updater (which never runs the wrapper), so this breaks a loop.

## Step 3 — wbrt eMMC restore (guaranteed; native Windows)
This is brick-insurance and works even in a hard boot loop, because it catches the device in
**MediaTek mode at the very start of boot**, before any of our code runs.

- Backup file (this device, 2026-06-18, ~2.88 GB): `C:\Users\walkman\OneDrive\Desktop\walkman_backup.20260618_185658.bin`
- Tool: `C:\Users\walkman\Downloads\walkman-backup-restore-tool.v1.0.9.exe`
- Driver: `C:\Users\walkman\WalkmanBackupRestoreDriver\installer_x64.exe` (device = `VID_0E8D&PID_2000`)

Steps: run the driver installer → run wbrt → **Backup first (see below)** → **Restore** → pick the
backup → get the device into MediaTek mode (below) → let it finish, don't disconnect.

> **Take a fresh Backup before you Restore.** A restore rewrites the *whole* eMMC, and `/contents`
> is inside it — the music library, playlists and `.scrobbler.log` all roll back to whatever the
> backup image contains. You are already in MediaTek mode with the tool open, so a backup costs
> ~20 more minutes and makes the current library recoverable from that image afterwards, even
> though the device won't boot. Save it to a **new filename**; never overwrite the known-good one.

### After a restore — what state you are actually in
A wbrt restore rewrites the **whole** eMMC from the backup image, so the device comes back as that
image was: **no `cinder-home`, no launcher, no `/contents` flags**, `.appcfg` stock, and the music
library rolled back to whatever the image held. Nothing of Cinder survives — there is no latch to
clear and no counter to reset.

Reinstalling is therefore a full install, and it needs **three** pushes, not one — the installer
stages each helper from the storage root and only *warns* if one is missing, so a partial push
degrades silently (no `cinder-umount` → USB-MSC cannot unmount `/contents` as uid 100; no
`cinder-gpunode` → the GPU path can never be enabled):

```bash
tools/flash.sh --push cinder-home/dist/stable/cinder-home
tools/flash.sh --push cinder-home/dist/stable/cinder-umount
tools/flash.sh --push cinder-home/dist/stable/cinder-gpunode
tools/flash.sh cinder-home/dist/stable/cinder_home_install.upg
# then boot with the cable OUT — a cable at boot is itself the escape to stock
```

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
