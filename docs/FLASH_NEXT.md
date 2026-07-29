# Next flash — run sheet

Built **2026-07-28** from `HEAD`. Staged in `cinder-home/dist/dev/` and `cinder-home/dist/stable/`.

Five things are fixed here, each with a root cause established on hardware rather than inferred:
Power off only slept, Restart froze the device, drag-to-seek moved the bar but not the audio, normal
touch felt clunky, and USB mass storage had never worked at all.

Four of the five were misdiagnosed for weeks in the same way — as timing, ordering or enum-value
problems — and all four turned out to be **permission or state** problems that a single on-device
measurement settled. Worth remembering the next time something here looks like a race.

---

## What changed and why

### 1. Power off / Restart — root cause found, mechanism replaced

Sony's own route cannot work while Cinder is the Home app, and the enum guess was never the problem.
`libpstcore.so` shows shutdown is a **two-phase barrier across every registered service**:

```
OnPreShutdown -> "All services preshutdowned!" -> OnShutdown -> "All services shutdowned!" -> android_reboot
```

`libPowerService.so` agrees: *"Power state transition is stopping! Check all services and reboot the
system..."*. Cinder-home replaced the Qt Home app but does not speak that protocol, so its phase is
never acknowledged, the barrier never clears, and `Reboot()` hangs **holding the UI thread** — the
freeze. `SetStatus(PowerOff)` came back as a sleep for the same reason.

Both now go through **`cinder-power`**, a third setuid-root helper (`reboot(2)`), alongside
`cinder-umount` and `cinder-gpunode`. `reboot(2)` needs `CAP_SYS_BOOT` and appmgr launches
cinder-home capless — the same wall that made `cinder-umount` necessary, same solution.

Already verified on device (no reboot needed to test these):

| Check | Result |
|---|---|
| runs on-device, correct ABI | ✅ |
| no argv → refuses | ✅ rc 2 |
| bad verb (`banana`) → refuses | ✅ rc 2 |
| non-root (`setuidgid 1000`) → refuses **before touching mounts** | ✅ rc 3 |

The euid check is not decoration: without it, a lost setuid bit would remount `/contents` and
`/data` read-only and then fail to reboot, leaving a running system that cannot write its own log.

**Untested branch: `reboot(2)` itself.** That is what this flash is for.

### 2. Power button hold → power menu (the Sony gesture)

Power used to act on the **press** (screen toggle), so holding it could only ever blank the screen —
past about eight seconds the PMIC's own forced reset took over, which is hardware and nothing to do
with Cinder. Now the press only starts a clock:

* held ≥ **1 s** → the Power menu opens (Power off / Restart / Cancel), and the release does nothing
* released before that → the screen toggle, exactly as before

**One assumption, and it is self-defending.** Everything else in the input loop ignores releases, so
nobody has ever checked whether `mtk-kpd` reports a `KEY_POWER` release. If it does not, deferring
the toggle would make a short press do *nothing* — a core function silently dead. So the toggle is
only deferred **once a release has actually been observed**; until then Power behaves exactly as it
did before and the hold menu stays off. The dev log records both edges:

```sh
adb shell 'grep POWER /contents/cinderhome.log'      # "input: POWER release after N ms"
```

If that line never appears, this unit does not report the release, Power still works, and the hold
gesture needs a different trigger.

### 3. Touch latency — input now runs before the paint

`input_pump()` ran **after** `cinder_render_tick()` in the frame loop. So a tap read at the end of
frame N could only reach the glass in frame N+1, and a drag always painted the finger's *previous*
position. A scrolling frame measures ~31 ms on device, so that was ~31 ms of avoidable lag on
everything you touch — which is why Settings felt clunky despite having no album art to blame.

Input is now read at the top of the loop, so the frame about to be painted already reflects the
finger. Gated on `g_deferred_done` so nothing runs earlier in the boot than it used to.

### 4. Drag-to-seek — SOLVED

**The engine will not seek while it is streaming.** With playback running, every origin (swept
0..11), milliseconds, seconds, and even an offset of **zero** came back from
`MediaEnginePlayer.cc:221` as `SeekTime(): Bad parameter. ignored`. A seek to the start of a track
cannot be a bad parameter, so it was never the argument — it was the state.

Pause first and the identical call lands exactly. Measured against a live track: targets 20 s /
30 s / 150 s, forwards and backwards, three for three, zero rejections in logcat. `Suspend()` /
`Resume()` (the engine-level pause) does **not** work; it has to be transport-level
`ChangePlayState`. Sony's own app agrees — it wraps every seek in `dmpapp::AudioPlayerImplStateSeek`,
which carries a `PlayState` alongside the origin and offset.

Nobody had hit this before because nobody had driven the path: Wampy asks the *stock app* to seek
over a Unix socket (`hagoromo.cpp:872`, `CMD_SEEK`), so `PlayController::SeekTime` had never been
called directly on this device. `cinder_audio_seek_ms` now pauses, seeks and resumes — and only
resumes if it actually interrupted something, so seeking inside a paused track leaves it paused.

Also fixed on the way: `SeekTime` is **void**, not `int`, so the shell's old "seek REJECTED" line
was reading a leftover register and asserting something it could not know.

### 5. USB mass storage — SOLVED, and the cause was never a race

MSC had never worked from Cinder, and every earlier fix (trigger ordering, the gadget enable-cycle,
the exit remount) was aimed downstream of the real cause. Both privileged steps are **root-only**,
and cinder-home runs as uid `system` with an empty capability set:

* **Binding the LUN.** Writing `/emmc@contents` to `f_mass_storage/lun/file` makes the *kernel* open
  the backing block device **in the caller's credentials**. `/dev/block/mmcblk0p29` is
  `brw------- root root`, so the open is EACCES and the sysfs write fails. The sysfs node itself is
  `0666 system:system`, so it looks writable and `echo` returns 0 either way — which is exactly why
  this presented as a race. The repeated `LUN STILL empty after retries` (~3 s of retries each,
  eighteen times) is also where the MSC lag came from.
* **Switching the gadget.** `setprop sys.sony.config msc` is refused for uid `system`. The property
  never left `adb`, so init's `on property:sys.sony.config=msc` block **never ran at all**. The old
  `init never reported sys.usb.state=mass_storage,adb` line was reporting a refusal, not a timeout.

Proved directly on device: as root the LUN binds first try and the property takes; as uid 1000 the
write returns 0 with an empty readback and the property stays `adb`.

Fixed with **`cinder-msc`**, a fourth setuid-root helper (same pattern as `cinder-umount` /
`cinder-gpunode` / `cinder-power`) that does the whole handoff in one root context, in the only safe
order — volumes unmounted before the gadget binds them, LUN released before the remount.

**Verified end to end:** `on` → host sees `sde 55.9G WALKMAN vfat`; `off` → LUN cleared, `/contents`
remounted and readable, gadget back to `adb`.

**One more trap, and it cost three wrong fixes.** cinder-home is uid `system` and the helper is
setuid root, so it runs ruid=1000/euid=0 — and the kernel sets `AT_SECURE` on any exec where those
differ, which **propagates to every descendant**. The loader then strips `LD_LIBRARY_PATH` from the
shell the helper spawns and from `setprop` under it, so every toolbox applet died with
`libcutils.so: cannot open shared object file` and the gadget switch silently never happened.
Neither `setenv()` nor inlining `LD_LIBRARY_PATH=...` into the command string helps — the loader
*discards* it at exec. The fix is `setuid(0)` at the top of the helper.

Two things made this hard to see: the identical command from an `adb shell` worked throughout
(that shell is not setuid, so it keeps its environment), and on the DEV channel the failure hid
itself because cinder-home composes the gadget as `mass_storage,adb` at boot for adb, so binding
the LUN alone was enough and both volumes appeared anyway. **Test MSC through the app, never from
an adb shell** — and note that anything setting `sys.sony.config` makes init `stop adbd`, which
kills an adb shell *and its children* mid-run.

Verified after the fix, through the real in-app path: `sony.config=msc`,
`usb.state=mass_storage,adb`, `idProduct=0b8d` — the exact stock composition — with **zero**
`libcutils` errors, and the host enumerating both `sde 55.9G WALKMAN` and `sdf 29.8G` (SD).

Its remount fallback mounts with **stock's own vfat options** (`fmask=0000,dmask=0000,...`). This is
not cosmetic: vfat defaults to root-only masks, so a "successful" default mount hands back a library
cinder-home cannot read — a failure that looks like an empty library rather than a mount problem.

---

## Step 1 — Flash `dist/dev`, cable OUT

```sh
tools/flash.sh --push cinder-home/dist/dev/cinder-home
tools/flash.sh --push cinder-home/dist/dev/cinder-umount
tools/flash.sh --push cinder-home/dist/dev/cinder-power      # NEW — Power off / Restart need this
tools/flash.sh --push cinder-home/dist/dev/cinder-msc        # NEW — USB mass storage needs this
tools/flash.sh cinder-home/dist/dev/cinder_home_install.upg
```

**Unplug the cable before it boots into Cinder** (a cable at boot is rung 0 of the escape ladder, so
booting with it out is what actually tests the app).

Confirm the helper landed, mode **4755**, owner root:

```sh
adb shell 'ls -l /system/vendor/unknown321/bin/cinder-power /system/vendor/unknown321/bin/cinder-msc'
tools/flash.sh --log | grep cinder-power
```

If the mode is not `4755` the euid guard will refuse and Power off will log a clear failure instead
of half-working.

## Step 2 — Settle the seek origin (needs music playing)

Start a track, let it run ~10 s, then:

```sh
adb shell 'echo "0 60000" > /tmp/cinder_seek.req'   # origin 0 -> 60 s
sleep 3; adb shell 'grep "seek probe" /contents/cinderhome.log | tail -1'

adb shell 'echo "1 60000" > /tmp/cinder_seek.req'   # origin 1 -> 60 s
sleep 3; adb shell 'grep "seek probe" /contents/cinderhome.log | tail -1'
```

Each run prints the position before and 1.2 s after, and says `LANDED` or `MISSED`. Whichever origin
lands is the right one — it goes into `cinder_audio_seek_ms` as a one-line change.

If **both** miss, ms is not the unit, and the `before/after` numbers say what the unit actually is.

## Step 3 — The four reported bugs, in order

| # | Test | Expected | If it fails |
|---|---|---|---|
| 1 | **Hold Power ~1 s** | Power menu appears | check the log for `POWER release`; absent = this unit reports no release |
| 2 | Menu ▸ **Cancel** | dismisses, nothing happens | — |
| 3 | Menu ▸ **Restart** | device actually restarts | the `cinder-power rc=` log line names the cause |
| 4 | Menu ▸ **Power off** | device actually powers off | as above; hold Power to bring it back |
| 5 | **Short-press Power** | screen toggles as before | this is the release path; a dead short-press means no release event |
| 6 | **Scroll Settings** | noticeably tighter than last build | — |
| 7 | **Drag the progress rail** | after step 2's fix, audio lands where you dropped it | step 2 already told you which origin |

Settings ▸ Restart and Settings ▸ Power off still exist and still raise their own two-button
confirms — the hold menu is an addition, not a replacement.

## Step 4 — Flash `dist/stable` for daily use

`cinder-power` ships on **both** channels (unlike `cinder-gpunode`): it backs a feature that is
always on, and it widens nothing — two fixed verbs, no caller-supplied paths.

---

## Still open after this flash

| Thing | State |
|---|---|
| **LDAC** | `ldac-bridge` builds, is **not installed**, `TEST.md` never run. 0% validated. Goal #3. |
| **Bluetooth** | Deferred by request. Needs the `BtTransmitterService` shim, which also unblocks route-aware volume and FM→BT. |
| **Album-art decode latency** | ~365 ms inline on the render thread at every track change. Needs moving onto the background decoder with the gradient shown until it lands. The biggest remaining win. |
| **Drag-and-drop queue reorder** | `queue_move` exists; the gesture does not. |
| **A dead Home app is not restarted** | 2026-07-28: cinder-home was killed by its own watchdog during MSC and sat as a **zombie** with the UI dead and nothing relaunching it. The MSC fix removes that trigger, but "the Home app died and nothing brought it back" is an independent gap in the safety net. |
| **`/contents_ext` is not remounted by anything** | Nothing in Cinder or init mounts the SD card — a Sony service does it at boot. Once unmounted it stays unmounted for the rest of the boot, and the SD library silently disappears. `cinder-msc off` now remounts it; nothing else does. |
| **"Clear queue or keep playing later"** | The modal is now three-way capable; the prompt and its wiring are not built. |
| `OneTrackMode::On == 1` | Still a guess. Repeat-one either repeats or it does not. |

---

## If it goes wrong

Escape ladder, weakest dependency first — each rung needs strictly less than the one above:

0. **Cable in at boot** → stock. Depends on nothing.
1. **Settings ▸ Boot to stock** → one-shot, no cable needed.
2. **`/contents/cinderhome_off`** over USB-MSC from a PC.
3. **Bad-boot counter** — four failed boots auto-revert.
4. **wbrt restore** — the backstop.

A reboot loop is most likely a Rust panic; the log names the screen it happened on:

```sh
adb shell 'cat /contents/cinderhome.log.1' | grep -A 4 PANIC
```

**`/tmp`, not `/data`,** for anything you push and execute — `/data` and `/contents` are mounted
`noexec` and fail with a bare `permission denied` that looks like a mode problem and is not.
