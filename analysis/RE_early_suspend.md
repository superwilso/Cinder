# RE — Cinder's screen-off is cosmetic, and it costs the whole SoC idle path

**Date:** 2026-09-04 · **Kernel:** Linux 3.10.26 (MT8590), extracted from the stock 1.02 `.UPG`
**Status:** **OBJECTIVE ACHIEVED 2026-09-04 — `dpidle_cnt` 0 → 78, the SoC entered deep idle.**
Suspend/resume works off-cable and the harness now recovers without a forced reboot. One narrow bug
left: the USB gadget does not re-enumerate after a resume. Read the top two sections first; "How
this failed" is kept as the record of two wrong diagnoses. Still UNWIRED in cinder-home.

## The one-line finding

The NW-A55 has never entered SoC deep idle under Cinder — not once, in any boot measured. The
blocker is a single byte that only the kernel's **early-suspend chain** sets, and the only thing
that starts that chain is a write to `/sys/power/state`, which nothing on this system does. Cinder
turns the screen "off" by writing the backlight brightness node; the kernel never learns the device
went idle.

The same chain also carries the DAC's power hook — though **measured, it does not actually power the
codec down** (see below), so the two findings are related but not one fix.

## How the kernel was obtained

No `/dev/kmem` — reading it panicked the device once already (see
[[reference_device_shell_gotchas]]). Offline instead, and it costs the device nothing:

```
artifacts/unpacked/stock/2.bin        Android bootimg, kernel @0x10008000, page 2048
  + 2048                              MTK header, magic 0x58881688, name "KERNEL"
  + 512                               real zImage (magic 0x016f2818 at 0x24 ✓)
  + 15636                             XZ payload  ->  12,543,376 bytes
```

Address mapping is `file_offset = vaddr - 0xc0008000`, verified two ways before anything was read
from it: `dpidle_handler` and `mt_cpufreq_earlysuspend_status_get` both land exactly on the ARM
`mov ip, sp` prologue, and the banner string decodes as
`Linux version 3.10.26 (slave@azslave5q) (gcc 4.8) #1 SMP PREEMPT`.

## The gate, from `dpidle_handler` @0xc0037858

```
c00378d8:  bl    mt_cpufreq_earlysuspend_status_get
c00378dc:  cmp   r0, #0
c00378e0:  moveq r0, #4          ; block reason 4
c00378e4:  beq   0xc00378a8      ; -> bump dpidle_block_cnt[4] and return "cannot enter"
```

and the counter write it jumps to:

```
c00378a8:  add r4, r4, r0, lsl #2
c00378b0:  ldr r3, [r4, #88]
c00378b4:  add r3, r3, #1
c00378b8:  str r3, [r4, #88]
```

`r4` is `dpidle_get_status_tbl` (0xc0c11214) and `dpidle_get_status_tbl + 88` is exactly
`dpidle_block_cnt` (0xc0c1126c) in the symbol table — so the decode is confirmed by the symbols,
not just by reading. Index 4 is the one the device reports as `by_vtg`.

`mt_cpufreq_earlysuspend_status_get` @0xc0030b40 is four instructions: it returns the byte at
**0xc0c0d098**, which kallsyms names
`mt_cpufreq_earlysuspend_allow_deepidle_control_vproc`.

**So `by_vtg` is not a voltage problem, despite the name.** It is the early-suspend flag. That name
sent this investigation down a regulator rabbit hole for an hour; the symbol table settled it.

`hotplug_cpu_count == 1` (only CPU0 online) already passes. The clock check *appears* to pass too —
every `dpidle_block_mask[CG_*]` reads zero on the live device — but that is only because the
early-suspend test short-circuits first and the clock condition is never evaluated. Once the gate
opens, a clock blocker does appear. Reading those masks while the gate is shut says nothing.

## Nothing ever sets the flag

`mt_cpufreq_early_suspend` has **zero direct callers** — it runs off the early-suspend handler list.
Walking back:

- `register_early_suspend` — **17 registrants** (below)
- the chain runs from `early_suspend()`, queued by `request_suspend_state()`
- `request_suspend_state` @0xc00be10c has **exactly one caller: `state_store` +0xac**

`state_store` is the `/sys/power/state` write handler. Nothing on this device writes it:
`autosleep` reads `off`, and Cinder blanks the panel with a brightness-node write plus Sony's
`display_backlight(0)` — neither touches the PM core.

Result: the flag is never set, `dpidle_cnt` stays at 0 for the life of the boot, and the counter
climbs at ~240/s (one per idle attempt) for as long as the device is on.

## What the chain would actually do

All 17 `register_early_suspend` call sites:

| Registrant | Effect | Note |
|---|---|---|
| `mt_cpufreq_pdrv_probe` | sets the deep-idle allow flag | **the fix** |
| `cxd3778gf_i2c_probe` | codec power hook | **measured: does NOT power the codec down** — lighter than standby |
| `mtkfb_init` | blanks the framebuffer | ⚠️ Cinder renders to it |
| `synaptics_ts_probe`, `himax_hx8526_ts_probe` | sleep the touchscreen | ⚠️ touch can no longer wake |
| `lm3630_probe` | backlight IC off | already off |
| `mt_emifreq_init`, `mt_hotplug_mechanism_init`, `smi_init` | memory/hotplug/bus low-power | |
| `pmic_mt6323_init`, `bq24262_wmport_probe` | PMIC + charger | |
| `mt_clkmgr_debug_bringup_init`, `vcodec_driver_init`, `compaction_init`, `hwmsen_probe`, `batch_probe`, `android_power_init` | misc | |

Note `cxd3778gf_i2c_probe` on that list. Before testing, this document predicted that the chain
would therefore power the codec down and make the standby fix redundant. **It measured otherwise** —
the hook runs and the codec stays fully readable on I2C. The standby fix stands on its own.

## Writing `/sys/power/state` — what it really does (CORRECTED)

`state_store` itself never calls `pm_suspend`/`enter_state`: the disassembly ends
`bl request_suspend_state` → `mov r0, r9` → return. It resolves the string against a four-entry
table, passes the matched **index** straight through, and returns `-EINVAL` (`mvn r0,#21`) on no
match. `echo mem` = index 3, `echo on` = index 0.

**But that is not the whole path, and this document originally said it was.** The kernel's own log,
at the end of the early-suspend chain:

```
early_suspend: calling pm_autosleep_set_state() with parameter: 3
```

So the early-suspend worker *does* chain into autosleep, which is the suspend-to-RAM path. It did
not suspend during any test here — `/sys/power/autosleep` still reads `off` afterwards, and
`echo on` resets it — most likely because the USB cable and adb hold a wakeup source. **Off-cable
the outcome is untested**, and a device that suspends to RAM without a working wake source is
recovered by hand. Do not run the off-cable version of this test casually; the Power key is
presumably a wake source, but "presumably" is doing real work in that sentence.

The earlier claim rested on reading `state_store` alone and stopping there. The handler chain past
it was where the answer actually lived.

## The risk as assessed beforehand — and what actually happened

Two of the 17 handlers were the reason this was not fired unattended:

- **`mtkfb` blanks the framebuffer** while Cinder renders into it. Whether the panel returned on
  late-resume was unproven, and a panel that does not come back is a reboot to stock.
- **Both touchscreen drivers sleep**, so touch-to-wake was expected to stop working.

The first concern did not materialise: the panel powered down and came back with a full LCM
re-init, repeatably. The second is **still untested** — it needs a finger on the glass, not a shell.

## THE OBJECTIVE, ACHIEVED — deep idle entered, 2026-09-04

**`dpidle_cnt[0]` went 0 → 78.** The SoC entered deep idle for the first time in this project's
history. Log: `analysis/kernel/pm_deepidle_2026-09-04.log`.

```
pre   dpidle_cnt[0]=0,  slidle_cnt[0]=78548   by_vtg=107569
      ... 3 suspend/resume cycles, screen off, off-cable ...
post  dpidle_cnt[0]=78, slidle_cnt[0]=87754   by_vtg=121952
```

This is exactly what the disassembly predicted. `by_vtg` is the early-suspend flag, nothing on this
system ever set it, and deep idle was gated behind it. Drive the early-suspend chain and the gate
opens — measured, not inferred.

**Where the 78 came from.** The counter was 0 through the first awake window, read 39 immediately
after a 168 s suspend, and 78 by the end of the run — i.e. roughly 39 deep-idle entries per awake
window. During those ~18-20 s windows early suspend is active and nothing holds a wakelock, so the
CPU drops into deep idle between timer ticks. `by_vtg` still climbs alongside it (107569 → 121952),
so the gate is **intermittent, not permanently open** — some idle attempts still see the flag
clear. That is worth understanding before anyone claims deep idle is "fixed".

**The wake timer was never needed.** `slp_pwake_time` read **-1** for this entire run — a reboot had
reset it and it was never re-armed. The device suspended and woke anyway, three times, on gaps of
5 s, 168 s and 10 s. `r12` read `0x00000020` on all three, which against
`analysis/kernel/spm_wakesrc.txt` is **GPT**, the general-purpose timer. So the kernel's own timers
bring it back; the SPM periodic wake was a safety net that turned out to be unnecessary. The
irregular gaps are consistent with ordinary kernel timers rather than a fixed 30 s period.

**Self-recovery works.** After 3 cycles the harness disarmed the timer and wrote `on`:

```
--- 3 cycles done: disarming and writing 'on' to leave suspend ---
recovered: autosleep=off pwake=-1
```

and the screen came back **unaided, with no forced reboot** — the first run of this whole
investigation that ended without one.

**One bug remains, and it is narrow: the USB gadget does not re-init after a resume.** The screen
returns, the device is fully alive, and the host still never sees it — adb stays down until a
reboot. That is the entire residue of what was originally recorded as "the device bricks
off-cable". `/sys/class/android_usb/android0/enable` is world-writable and reads `1` with
`functions=adb`, so bouncing it (0, then 1) is the obvious fix; `tools/pmtest.sh` now does that
after leaving suspend, **untested as of this writing**.

So the original catastrophic-looking failure decomposes into three separate, ordinary things:
suspend/resume working correctly, nothing writing `on` to leave the state, and a gadget that does
not re-enumerate. None of them was the thing it looked like.

## RESOLVED 2026-09-04 — it was never a wedge

**The device suspends and resumes correctly, off-cable, repeatedly.** First completed
suspend/resume in this project's history, and it happened five times in one run:

```
*** SUSPEND RETURNED: gap=30s  rc=1 r12=0x00000001 timer_out=0x000f00a4
*** SUSPEND RETURNED: gap=32s  rc=2 r12=0x00000001
*** SUSPEND RETURNED: gap=32s  rc=3 r12=0x00000001
*** SUSPEND RETURNED: gap=31s  rc=4 r12=0x00000001
*** SUSPEND RETURNED: gap=23s  rc=5 r12=0x00000020 timer_out=0x000a5482
```

Full log: `analysis/kernel/pm_first_suspend_2026-09-04.log`. `rc` is
`icx_pm_helper/resume_count`; the gaps are `/proc/uptime` deltas across a 1 Hz sampling loop, i.e.
time the CPU spent powered down.

**What actually happens.** With `slp_pwake_time=30` and nothing holding a wakelock, the device
enters a stable cycle:

```
suspend ──30 s (wake timer)──> resume ──~18-20 s awake──> suspend ──> …
```

The awake window is **Sony's resume wakelock**: `icx_pm_helper/resume_lock_ms` reads **20000**, and
the measured windows are ~18-20 s. It fits exactly, and it is the same node this investigation had
already read without realising what it explained.

**Why it looked dead.** Nothing ever writes `on` back to `/sys/power/state`, so the device never
leaves the early-suspend state: the panel stays dark through every awake window, and a USB replug
lands in a suspended window more often than not and does nothing. From outside, a device that is
cycling perfectly is indistinguishable from one that is hung — which is exactly what happened on
the first attempt, and why it was recorded as a wedge.

**So the earlier diagnosis was wrong twice over.** First "there is no wake source" (falsified by the
wake-source tables), then "the resume path does not complete" (falsified here). The real defect is
narrower and duller than either: **nothing calls the exit.** `echo on > /sys/power/state` takes
`autosleep` from `mem` back to `off`, verified on-cable in the same session.

`tools/pmtest.sh` now does that itself after N cycles, so the test recovers without a forced reboot.

**Still open:** whether `dpidle_cnt` moves during the awake windows (the run was cut short by a
forced reboot before the post-run counters were read). Note that a real suspend is *better* than
deep idle for power — the CPU is off entirely — so deep idle now matters mainly for the awake
windows.

**Wake source, unresolved.** `r12` read `0x00000001` on four wakes and `0x00000020` on the fifth.
Against the bit table in `analysis/kernel/spm_wakesrc.txt` those are `CPU` and `GPT`, **not**
`PCM_TIMER` — despite the gaps matching `slp_pwake_time=30`. Either the pwake timer surfaces as
something other than bit 1, or another timer is doing the waking. Not settled, and the decode
should not be quoted as fact until it is.

## How this failed — read this first

Wired into cinder-home (write `mem` on screen blank, `on` on wake), then tested off-cable. **The
device suspended to RAM and did not come back.** Power did nothing; plugging USB in was not
detected. It took a forced reboot, and the reboot zeroed the very counters the test existed to
read, so the deep-idle question is still open.

**Why four clean on-cable tests hid it.** The chain does not stop at early suspend — it ends with
`pm_autosleep_set_state(3)`, the real suspend-to-RAM path. On the cable that never completes,
because USB holds a wakeup source. Every on-cable run therefore resumed perfectly and looked like
proof. Off-cable and idle, nothing holds a wakeup source, and the same write goes all the way.

**The specific bad inference.** Sony's codec driver takes a `CXD3778GF` wakeup source during
playback — measured, and true — and that was treated as the safety argument. It is not: it protects
the *playing* case and says nothing about the *idle* case, which is the only case the wiring ever
ran in. A measurement that is true and irrelevant is more dangerous than one that is simply wrong,
because it feels like evidence.

**The compounding error.** The device was left armed and the cable — the only interface that could
write `on` — was then removed, on a build whose Power handler had no resume call. There was no way
back in. That breaks the standing rule that an escape must depend on strictly less than what it
rescues; here there was no escape at all.

**What would have to be true before retrying:** a wake source demonstrated to bring this device back
from suspend-to-RAM off-cable, established with a route back in that does not depend on the thing
being suspended. Until then the helper stays deleted; `cinder-home/src/main.cpp` carries the same
warning where it used to live.

**Partly falsified on 2026-09-04 — see `analysis/RE_kernel_power.md` §1.** The assumption above is
that the device had no wake source. The kernel's own tables say otherwise: `KP` is indeed not in
`spm_sleep_wakesrc` (0x01204564), but `EINT` is, the MT6323 PMIC is EINT 150
(`PMIC_EINT_SETTING` @0xc0443d74), and the PMIC's power-key interrupt enable is set
(`INT_CON0 = 0x0420`, bit 5). So a power-key wake path existed on paper.

That makes the working hypothesis "it woke, or never suspended cleanly, and the **resume** path did
not complete" rather than "nothing could wake it" — a different bug needing a different test. It
does **not** loosen the prerequisite. Trusting a code path that had never run in the state that
mattered is precisely what caused this failure, and a register read is the same class of evidence.
The retry design is in `RE_kernel_power.md` §5; the first thing it must establish is only that
`icx_pm_helper/resume_count` becomes 1, with the user holding the device.

## Executed on hardware, 2026-09-04

`echo mem > /sys/power/state`, wait, `echo on`, with counters either side. All 16 handlers ran
(kernel logs them by name, `ES handlers 0..15` / `LR handlers 0..15`).

**Safe.** The panel powered down and came back with a full LCM re-init
(`PLL config`, `MIPI Change lane rt_code`, `push_table`, `[FB Driver] leave late_resume`), the
touch driver resumed, cinder-home stayed alive, uptime unbroken, backlight left as it was found.

**The gate opens, exactly as predicted.** `by_vtg` gained 403 in the 1.3 s between the write and
`ES handlers 13: [mt_cpufreq_early_suspend]` running — a rate matching the ~343/s baseline — and
then **froze for the rest of the suspended window**. `mt_cpufreq_early_suspend` @0xc0032778 ends
`mov r3,#1 ; strb r3,[r4]` with `r4 = 0xc0c0d098`, which is exactly the byte the dpidle gate reads.

**And a second blocker appeared underneath it.** Over a 25 s suspended window:

| counter | before | during | note |
|---|---|---|---|
| `dpidle_cnt` | 0 | **0** | still never enters |
| `by_vtg` | 711964 | 712258 | +294, then frozen — gate open |
| `by_clk` | **0** | **1113** | new blocker |
| `by_cpu` / `by_tmr` / `by_oth` | 285 / 0 / 0 | unchanged | |

The clock mask names it — one bit, every time:

```
dpidle_block_mask[CG_PERI0] = 0x00000400      (bit 10)
```

Bit 10 of PERI_PDN0 is **USB0** on this MTK family. That is the cable. Which means deep idle may
well be reachable off-cable with early suspend — and is certainly *not* reachable while plugged in,
whatever else is done. This is consistent with the standing rule that idle draw on this device
cannot be measured with USB attached.

**Not yet proven on this exact SoC** — the bit number comes from the MT8127/MT8135-family PERI
layout, not from a table found in this kernel. The name run at 0x9fe814 is an address-map table,
not the clock-gate bit table.

## The codec is NOT powered down by early suspend

`ES handlers 1: [cxd3778gf_i2c_early_suspend]` runs, and afterwards the codec still answers on I2C
(`DEVICE_ID=0x2B`, `BLK_ON0=0x0F`). So the early-suspend hook does something lighter than standby.

**This matters: the codec-standby fix in `RE_codec_power.md` is NOT made redundant by this.** The
two remain complementary — early suspend for the SoC, the `standby` control for the DAC.

## Audio is unaffected — tested, because it was the requirement

The concern was that blanking the screen during playback would cut the output. It does not:

```
playing, before suspend:  pcm4p/sub0 state: RUNNING   codecID=0x2B
playing, DURING suspend:  pcm4p/sub0 state: RUNNING   codecID=0x2B  BLK_ON0=0x0F
after resume:             pcm4p/sub0 state: RUNNING
aplay finished
```

Sony's driver is playback-aware — an active stream is left alone. So firing early suspend on
screen-off is safe whether or not music is playing, and the screen-off-while-listening case (the
most common one) gets the display, touch, EMI and hotplug savings for free.

The **codec standby** control is a different matter and stays gated on not-playing: setting it with
a PCM open is untested and is the one thing that could plausibly cut audio.

## The open question: touch-to-wake

Both touchscreen drivers are on the chain (`himax_hx8526_ts_early_suspend` ran). Cinder's
`screen_auto_off()` deliberately does *not* sleep the touch controller, precisely so a touch can
wake the device. After an early suspend, touch-to-wake is expected to stop working and the Power
button becomes the only way back. **This was not tested** — it needs a finger on the glass, not a
shell. It is the deciding factor for whether Cinder should fire this on every screen blank or only
when idle.

Also observed: `stop_drawing_early_suspend: timeout waiting for userspace to stop drawing` — a 1 s
stall on every early suspend, because that handler waits for userspace to acknowledge via the
`wait_for_fb_sleep`/`wait_for_fb_wake` protocol and Cinder does not implement it. Harmless, but it
is a free second back if Cinder ever does.

## If it works

The wiring is small: Cinder writes `mem` where it currently calls `panel_dark()` for an idle blank,
and `on` on wake. It should NOT do this for the Power-button blank if touch-to-wake is expected
there, and the touch-controller behaviour decides that.

## Related

- `analysis/RE_codec_power.md` — the DAC finding, and why its fix is a symptom fix
- `cinder-home/src/main.cpp` — `panel_dark()`, `screen_auto_off()`
- [[reference_device_shell_gotchas]] — why the kernel came out of the firmware, not `/dev/kmem`

---

## Stage 1 wired into cinder-home — verified on device 2026-09-04 (16:41–16:52)

### The mechanism works, and the dmesg proof is unambiguous

Driven by hand from a shell, on the cable, in this order — **wakelock first, then `mem`**:

```sh
echo cinder > /sys/power/wake_lock
echo mem    > /sys/power/state
```

dmesg:

```
ES handlers 15: [clkmgr_early_suspend], level: 400
early_suspend: calling pm_autosleep_set_state() with parameter: 3
[AUTOSLEEP][pm_autosleep_set_state]pm_wakep_autosleep_enabled(true)
[AUTOSLEEP][queue_up_suspend_work]autosleep_state: 3
[AUTOSLEEP][try_to_suspend]pm_get_wakeup_count
active wakeup source: cinder                 <-- ours, blocking the RAM step
active wakeup source: pmicAuxadc irq wakelock
active wakeup source: sys_sync
```

Result state: `autosleep=mem`, `wake_lock=[cinder]`, `resume_count=0`, adb alive throughout.
The full early-suspend chain ran (all 15 ES handlers, `clkmgr_early_suspend` included), autosleep
armed, and `try_to_suspend` then declined to go to RAM because our wakelock is an active wakeup
source. That is precisely the two-stage design: **deep-idle gate open, device still up.**

`dpidle_cnt[0]` stayed 0, as predicted — the cable holds the USB0 clock and
`dpidle_block_mask[CG_PERI0]=0x400` blocks deep idle regardless of the early-suspend flag. Stage 1
is an off-cable win by construction; the on-cable run only proves the mechanism.

### `/sys/power/autosleep` is written ASYNCHRONOUSLY — do not read it back immediately

Read straight after the `mem` write it still says `off`; seconds later it says `mem`. The chain runs
on `kworker/u4:0`. **Any test that samples `autosleep` immediately after the write will conclude,
wrongly, that the write did nothing.** Sample it at least one second later.

### Why the first armed boot did nothing: /contents is not there when cinder-home starts

The first attempt at this test found `autosleep=off` after 60+ s of a dark screen with the config
file present and readable. Cause, from dmesg:

```
[   10.142139] (1)[398:hagodaemon]fsg_store_file file=/emmc@contents, count=14, curlun->cdrom=0
[   11.121555] (0)[398:hagodaemon]fsg_store_file file=/emmc@contents, count=14, curlun->cdrom=0
[   12.691799] (1)[398:hagodaemon]fsg_store_file file=/emmc@contents, count=14, curlun->cdrom=0
```

`/contents` is `/emmc@contents`, and the USB mass-storage gadget **rebinds it as a LUN during boot**
at 10.1/11.1/12.7 s. cinder-home starts at 10.17 s (`/proc/<pid>/stat` field 22 = 1017 jiffies).

That one window caused two failures at once:

1. **`threshold_s()` latched a failed read as "disabled"** for the whole boot. It cached `0` on any
   read miss, so a boot that asked one tick too early never armed suspend.
2. **The launcher's log redirect silently dropped.** `run_home`'s `can_append "$LOGF"` probe missed
   in the same window, so it took the no-redirect branch and cinder-home's stdout/stderr went to
   the fd the launcher had inherited (`/dev/pts/10`, a pty nothing reads).
   `/contents/cinderhome.log` stayed **0 bytes** — so failure #1 left no evidence of itself.

Both fixed:

- `threshold_s()` now latches only on an answer worth trusting — a good value, or an
  `access("/contents", R_OK|X_OK)` that succeeds while the config file is genuinely absent.
  Anything else stays undecided and retries next tick. It also **logs its decision either way**;
  the old version logged only the enabled case, i.e. stayed silent exactly when it mattered.
- `run_home` now falls back to `/data/cinder/cinderhome.log` instead of dropping the redirect.
  `/data` is ext4, mounted at 3.98 s, and the MSC gadget never touches it. `/contents` stays first
  only because the user can read it over USB.

Verified on the next boot: `suspend: enabled, idle threshold 60 s (/contents/cinder_suspend_s, 0
retries)` at t=12.950, log written to `/contents/cinderhome.log`. The `0 retries` is luck of the
draw on that boot — which is the point of the retry path.

### Boot dead time — unrelated finding, same log

Between **1.574 s** (`StopBootAnimation`, glass shows Cinder) and **7.44 s** (deferred_up done) the
device paints no frames and reads no input: `deferred_up()` runs on the render thread and the frame
loop gates on it (`if (!g_deferred_done) { … deferred_up(); continue; }`). The library build is
4.8 s of that (`cinder_db_open` 2.165 s -> playback restore 6.966 s; 3456 tracks, 340 albums, art
cache). So "a couple of seconds after boot with no touch response" is ~5.3 s of a blocked render
thread, not dropped input.

### The idle test itself was dead on every fresh boot — `g_playing` starts `true`

The first armed boot with a correct threshold *still* did nothing. Neither the SoC suspend (60 s) nor
the **codec standby (30 s, shipped and previously believed working)** fired, with the screen off
since t=33 and every ALSA PCM reading `closed` at t=304.

Cause: both gate on

```c
const bool idle = !g_screen_on && !g_playing;
```

and `g_playing` is **`static bool g_playing = true;`** — it is *intent*, not state. The only place
that adopts the service's view sits behind `if (have_pos)`, so on a boot where nothing has ever been
played there is no position report and it **stays true for the life of the process**.

This was already known and written down in this file — `apply_pump_interval()` carries the comment
"g_playing is INTENT and it starts `true`, so on a boot where nothing has been played it stays true
forever", and both it and the auto-power-off guard work around it. The codec/suspend site was simply
missed, which means **the codec standby has never once fired on a fresh boot** — consistent with the
standing "the DAC/amp never enters standby on its own" observation, which turns out to have had two
independent causes stacked on it.

Fixed with the idiom the auto-power-off guard already uses:

```c
const bool audible = g_playing && cinder_audio_is_playing() != 0;
const bool idle    = !g_screen_on && !audible;
```

Requiring the *service* to agree, not just our intent. That same test is trusted to switch the whole
device off, so it is more than good enough to gate a codec standby, and it fails in the safe
direction: `cinder_audio_is_playing()` is derived from the position having moved recently, so it only
reads 0 once playback has genuinely stopped, and a one-tick flicker costs a single increment the next
tick resets.

**Generalisation worth carrying:** three features in this file now gate on "is it idle", and each one
had to independently discover that `g_playing` cannot answer it. Any *fourth* should use `audible`.

### `adb shell` does not propagate the remote exit code

`adb shell 'exit 3'; echo $?` prints **0**. So `until adb shell '<test>'; do …; done` fires on the
first iteration and proves nothing — it produced a false "stage 1 fired" here. Match on stdout:

```sh
until adb shell 'grep -a "X" /path' 2>/dev/null | tr -d '\r' | grep -q .; do sleep 5; done
```

### Codec standby now actually engages — confirmed by regmon readback

With the `audible` fix in, on a clean boot with nothing ever played:

```
[cinder-home]    8.092 suspend: enabled, idle threshold 60 s (/contents/cinder_suspend_s, 0 retries)
[cinder-home]   40.262 screen: idle timeout -> panel off (touch or Power wakes it)
[cinder-home]   70.579 codec: idle 30 s -> DAC/amp to standby (a PCM open wakes it)
```

30.3 s after screen-off, as designed. Verified at the hardware level rather than from the log —
a register read is `echo <reg> > /proc/regmon/<chip>/target` then `cat …/value` (writing `target`
is a read; writing `value` is forbidden):

| chip | 0x03 / 0x00 | 0x12 / 0x0E |
|---|---|---|
| `cxd3778gf` (codec) | `invalid length` | `invalid length` |
| `mt6323` (PMIC, control) | `0x00000063` | `0x00000005` |

The control matters: identical procedure, same padded target form, and the PMIC answers. The codec
does not answer **at all**, which is exactly the signature Sony's standby produces — the chip drops
below the point where regmon can reach it over I2C. Before this fix the amplifier stayed powered to
drive an empty jack for the whole session on any boot where nothing had been played, i.e. always.

### Stage 1 verified firing from cinder-home (2026-09-04, uptime 263)

```
[cinder-home]    8.092 suspend: enabled, idle threshold 60 s (/contents/cinder_suspend_s, 0 retries)
[cinder-home]   40.262 screen: idle timeout -> panel off (touch or Power wakes it)
[cinder-home]   70.579 codec: idle 30 s -> DAC/amp to standby (a PCM open wakes it)
[cinder-home]  247.501 suspend: idle 60 s -> early suspend (deep idle on, still awake)
```

Predicted 248.09 (arm at 8.092 + 180 s boot grace + 60 s idle), actual **247.501**. Kernel after:
`autosleep=mem`, `wake_lock=[cinder]`, `resume_count=0`, adb alive. Never reached RAM, by design.

**Full early-suspend handler list, in order** (`early_suspend_count = 16, forbid_id = 0x0`):

| # | handler | level |
|---|---|---|
| 0 | `wmt_dev_early_suspend [mtk_stp_wmt_soc]` | 0 |
| 1 | `cxd3778gf_i2c_early_suspend` | 50 |
| 2 | `pmic_early_suspend` | 51 |
| 3 | **`himax_hx8526_ts_early_suspend`** | 51 |
| 4 | `hwmsen_early_suspend` | 99 |
| 5 | `batch_early_suspend` | 99 |
| 6 | `stop_drawing_early_suspend` | 100 |
| 7 | `vcodec_early_suspend` | 149 |
| 8 | `SMI_common_early_suspend` | 149 |
| 9 | `bq24262_wmport_early_suspend` | 150 |
| 10 | `mtkfb_early_suspend` | 150 |
| 11 | `kick_compaction_early_suspend` | 151 |
| 12 | `mt_emifreq_early_suspend` | 350 |
| 13 | `mt_cpufreq_early_suspend` | 350 |
| 14 | `mt_hotplug_mechanism_early_suspend` | 400 |
| 15 | `clkmgr_early_suspend` | 400 |

**Two consequences worth naming.**

**(a) The touchscreen driver early-suspends (handler 3), BELOW Cinder.** `screen_auto_off()`
deliberately does *not* sleep the touch controller precisely so a tap can wake the device — stage 1
undoes that at the kernel level, where Cinder's choice does not reach. So after stage 1 engages,
touch-to-wake is expected to stop working and Power becomes the only way back. Power is proven (a
press woke the device in 17 s against a 300 s timer). **This is the open question on shipping stage 1
as wired** — needs a finger to settle, not a shell.

**(b) Entering costs ~1.3 s, and 1.0 s of that is avoidable.** 257.959 -> 259.268, of which
`stop_drawing_early_suspend: timeout waiting for userspace to stop drawing` is a full second: Cinder
does not implement the `wait_for_fb_sleep`/`wait_for_fb_wake` handshake, so that handler always times
out rather than being acknowledged. Implementing it would cut entry latency by ~75%.

---

## OFF-CABLE STAGE 1 — SOLVED, 2026-09-04

`dpidle_cnt[0]` **0 → 18,509**. Log: `analysis/kernel/pm_offcable_stage1_2026-09-04.log`.

| metric | value |
|---|---|
| climbing window | 486 s (t=1652 → t=2138) |
| rate | **37.8 deep-idle entries/sec** |
| `by_vtg` across the whole `as=mem` window (98 samples) | 101039 → 101039, **delta 0** |
| `by_clk` | +2042, confined to the samples where the cable was still in |
| `dpidle_block_mask[CG_PERI0]` | `0x00000400` (cable) → **`0x00000000`** |
| `resume_count` | **0** for the entire run |
| wakelock reasserts by the harness safety net | **0** |
| USB after replug | back immediately, no reboot |

**The `by_vtg` delta of zero is the headline, not the 18,509.** Every previous run had `by_vtg`
climbing alongside `dpidle_cnt` (107569 → 121952), which is why this document and the project memory
both carried "the gate is INTERMITTENT, not permanently open — don't claim fixed". That caveat is
now **retired**: it climbed before because autosleep kept cycling the device in and out of suspend,
so the early-suspend flag kept being cleared and re-set. Held open by a wakelock, the flag is set
once and stays set, and the gate never blocks again.

**And the block mask went to literally zero off-cable**, which is exactly what the PERI0 bit table
predicted (`analysis/RE_kernel_idle_levers.md` §2): the only clock blocker on the cable was USB0,
i.e. the cable itself.

**Stage 1 delivered its actual design goal:** deep idle continuously, with the device fully alive —
`resume_count` never moved, adb came back the instant the cable went in, and nothing needed a reboot.
Compare the previous best: **78** entries, in short cycles, with the panel cycling dark and the USB
gadget dead until a forced reboot.

**This makes stage 2 much less interesting than it looked.** Stage 1 already gets the SoC into deep
idle and keeps it reachable. Stage 2 buys only the delta between deep idle and full RAM-off, and
pays for it with the unresolved gadget bug. That delta should be measured before any more work goes
into it — and with no fuel gauge on this device, measuring it means a multi-hour voltage-decay A/B,
not a spot check.

## Codec standby during Bluetooth playback — device-verified 2026-09-04

```
[cinder-home]  216.466 codec: playing over Bluetooth 30 s -> DAC/amp to standby (not in the path)
[cinder-home]  223.183 bt-sound: codec:0x02 channel:0x02 frequency:0x01 flag:0
```

30 s after the BT link settled, with the screen off and LDAC streaming. The `bt-sound` line *after*
the standby — with the frequency field changing, which is the per-track source rate — is a track
advancing with the DAC powered down.

Confirmed by regmon rather than from the log, with the PMIC as a live control:

| chip | 0x03 / 0x00 | 0x12 / 0x0E |
|---|---|---|
| `cxd3778gf` (codec) | `invalid length` | `invalid length` |
| `mt6323` (PMIC, control) | `0x00000063` | `0x00000005` |

So the headphone amplifier is genuinely off through a Bluetooth listening session, which is one of
the two ways this device is actually used. Before this it stayed powered for the whole session,
driving an empty jack.

**Listening check, 2026-09-04: "i heard no change".** No stutter, no gap, no artefact at the moment
the amplifier drops — which is what should happen, since it was never carrying the audio. Worth
stating explicitly because a register readback proving the chip is off says nothing about whether
the listener noticed.

### Full test matrix — all three directions verified on device, 2026-09-04

| case | expected | measured |
|---|---|---|
| BT playing, screen off | codec → standby | fired 30 s in; every register `invalid length`; listener heard **no change** |
| BT link drops mid-session | codec wakes, audio falls back to the jack | woke (`0x03`→`0x03`, `0x12`→`0x0F`), `pcm4p` RUNNING, audio out of the jack |
| **jack playing, screen off** | codec **stays awake** | awake for 60 s of sampling, `0x12` held at `0x0F`, no standby line |

**The third row is the one that mattered.** It is the negative control for the risk this change
introduced: if `cinder_get_bt_route()` went stale and still reported a BT peer after a disconnect,
the codec would be put into standby *while carrying jack audio* and the user would get silence. The
flag clears correctly — `bt-vol: rocker now drives the 3.5 mm jack (GetBtStatus=6, peer gone)` — and
the codec stayed up.

The disconnect fallback also confirms the property the whole standby rests on: Sony's driver clears
standby by itself when the local PCM is opened. Nothing in Cinder had to notice the edge.
