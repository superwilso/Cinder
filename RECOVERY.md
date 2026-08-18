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

### What survives kernel work (audited 2026-08-18)

Everything above assumes the main kernel boots. **Kernel work breaks that assumption**, so the
ladder has to be re-read for it. Facts, all verified on the live device:

**wbrt still covers you — for module work and even for a bad `bootimg`.** wbrt is
`flash_tool.exe` + `MTK_AllInOne_DA_5.2136.bin`, its driver binds `VID_0E8D&PID_2000`
("MT65xx **Preloader**"), and its backup length is hardcoded `2885156864` bytes from **user-area
offset 0**. Against `/proc/dumchar_info`:

| region | where | inside wbrt's 2.68 GB? |
|---|---|---|
| `preloader` | `/dev/misc-sd` = eMMC **boot0**, outside the user area | **NO — never backed up, never restored** |
| `uboot` (LK) | `mmcblk0p7` @ 0x2120000 | yes |
| `bootimg` — where kernel work lands | `mmcblk0p8` @ 0x2180000, 16 MB | yes |
| `recovery` | `mmcblk0p9` @ 0x3180000, 16 MB | yes |
| `nvp` / `android` / `usrdata` | p22 / p19 / p28 | yes |
| `contents` | `mmcblk0p29` @ 0x92f80000 | first ~400 MB only |

So the only thing wbrt cannot rewrite is the preloader in boot0 — and nothing in the FM kernel
plan writes there. **Never write boot0.** That is the one action with no way back.

`/proc/sys/kernel/panic` = **5** and `panic_on_oops` = **1** (`CONFIG_PANIC_TIMEOUT=5`), so an oops
or panic auto-reboots after 5 s instead of hanging silently. Every reboot re-runs the preloader, so
wbrt's catch window recurs — the same trick RECOVERY step 3 already relies on.

**But rungs 0-4 mostly do not survive.** Rungs 0-3 are all userspace — cable escape, bad-boot
counter, `/contents` flags — and every one needs the main kernel to boot. Rung 4 is armed by
`scsitool do_fw_upgrade` over USB-MSC, which also needs a booting main kernel. A dead `bootimg`
therefore drops you from rung 0 straight to rung 5.

**There is probably a kernel-independent rung 4, but it is UNPROVEN.** `recovery` (`mmcblk0p9`) is
a fully self-contained updater: its own kernel (3,822,632 B) and its own 3.8 MB ramdisk of 133
entries, whose `init.rc` ends in

```
exec /bin/sh /install_update_script/icx_start_update.sh
```

with every `mount ... /system|/data|/db` line commented out (`#@`). It depends on neither `bootimg`
nor the `android` partition. LK carries the standard Android BCB (`boot-recovery`,
`mboot_recovery_load_misc`) plus an `MT65XX_RECOVERY_KEY` combo. Two things would have to be armed
**from a working system, before the risky boot**: the `misc` BCB (`mmcblk0p11`, currently all
zeros) and NVP `fup` (currently `0xFFFFFFFF`; `icx_start_update.sh` hard-requires `0x70555766`).
Nobody has tested this. Until someone does, treat a bad `bootimg` as "wbrt only".

**Therefore the working rule for kernel work:**

| what you do | worst case | needs wbrt? |
|---|---|---|
| `insmod` by hand over adb | panic -> 5 s -> reboot -> module gone, self-healed | no |
| module loaded from the launcher at boot | bad-boot counter -> stock, taking the module load with it | no |
| **replace the kernel in `bootimg`** | no boot; rungs 0-4 all gone | **yes** |

Stay in the first two rows. The third is the only tier that bets the device on wbrt, and no FM work
so far justifies going there.

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
tools/flash.sh /mnt/c/Users/<you>/Downloads/cinder_uninstall.upg
```
The next boot enters the Sony updater (which never runs the wrapper), so this breaks a loop.

## Step 3 — wbrt eMMC restore (guaranteed; native Windows)
This is brick-insurance and works even in a hard boot loop, because it catches the device in
**MediaTek mode at the very start of boot**, before any of our code runs.

- Backup file: wherever you saved your `wbrt` Backup output (see the note above — take a fresh one
  before every Restore, and never overwrite your known-good image).
- Tool: `walkman-backup-restore-tool` — `github.com/unknown321/wbrt/releases`
- Driver: the MediaTek VCOM driver `wbrt`'s README links (device = `VID_0E8D&PID_2000`)

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
