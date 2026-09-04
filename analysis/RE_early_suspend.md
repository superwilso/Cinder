# RE — Cinder's screen-off is cosmetic, and it costs the whole SoC idle path

**Date:** 2026-09-04 · **Kernel:** Linux 3.10.26 (MT8590), extracted from the stock 1.02 `.UPG`
**Status:** mechanism proven from kernel disassembly and executed on hardware. **The fix is
UNWIRED and must stay that way** — off-cable it suspended the device to RAM with no working wake
source and cost a forced reboot on 2026-09-04. Read "How this failed" before touching it.

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
