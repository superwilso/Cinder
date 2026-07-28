# Next flash — run sheet

Built **2026-07-28** from `HEAD`. Staged in `cinder-home/dist/dev/` and `cinder-home/dist/stable/`.

This build answers four things reported from the last hardware session: Power off did nothing but
sleep, Restart froze the device, drag-to-seek moved the bar but not the audio, and normal touch felt
clunky. Three of the four have a root cause and a fix. The fourth (seek) has a probe that settles it.

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

### 4. Drag-to-seek — one unverified value, and a probe for it

Two findings. First, `PlayController::SeekTime` is **void**, not `int` (disasm @0x13200 packs
`{session, origin, ms}`, calls proxy vtable+0x48, and discards the response — exactly like
`NextTrack`). The shell was reading a leftover register and logging "seek REJECTED" off it, which
was noise: an accepted seek and a rejected one are indistinguishable from the caller.

That leaves exactly one unverified value in the path — `media_origin_t`. `Begin = 0, Current = 1` is
an RE guess the header itself flags as *"calibrate exact values on device"*, and a wrong origin
looks precisely like the reported bug: the bar follows the finger, the audio does not follow the bar.

So: **step 2 below settles it in two runs.**

---

## Step 1 — Flash `dist/dev`, cable OUT

```sh
tools/flash.sh --push cinder-home/dist/dev/cinder-home
tools/flash.sh --push cinder-home/dist/dev/cinder-umount
tools/flash.sh --push cinder-home/dist/dev/cinder-power      # NEW — Power off / Restart need this
tools/flash.sh cinder-home/dist/dev/cinder_home_install.upg
```

**Unplug the cable before it boots into Cinder** (a cable at boot is rung 0 of the escape ladder, so
booting with it out is what actually tests the app).

Confirm the helper landed, mode **4755**, owner root:

```sh
adb shell 'ls -l /system/vendor/unknown321/bin/cinder-power'
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
