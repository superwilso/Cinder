# RE — kernel levers for idle power and latency

**Date:** 2026-09-04 · Device-measured unless marked otherwise.
Kernel: `artifacts/unpacked/stock/2.bin` → bootimg +2048 → MTK header +512 → zImage → **XZ at file
offset 18196** → 12,543,376 bytes. `file_offset = vaddr - 0xc0008000`. Symbols from the live
`/proc/kallsyms` (77,843 entries) — **pull it as a file** (`cat > /data/local/tmp/ks.txt` then
`adb pull`), because `adb exec-out` produced an empty file here.

---

## 1. The framebuffer early-suspend handshake — IMPLEMENTED

Every early suspend paid a full second to this:

```
stop_drawing_early_suspend: timeout waiting for userspace to stop drawing
```

Measured entry cost 1.31 s wall (dmesg 257.959 → 259.268); 1.00 s of it was this one handler.

**The protocol, read out of the image rather than assumed.** `fb_state` @ `0xc0c37bc0`
(`fb_state_lock` @ `0xc0c37bbc`, `fb_state_wq` @ `0xc0c37bc4`):

| state | meaning | written by |
|---|---|---|
| 2 | DRAWING | `start_drawing_late_resume` @ `c00be61c` |
| 1 | REQUEST_STOP_DRAWING | `stop_drawing_early_suspend` @ `c00be8b0`, which then waits for 0 with a 1 s timeout |
| 0 | STOPPED_DRAWING | **`wait_for_fb_wake_show` @ `c00be76c`** |

**The acknowledgement lives in `wait_for_fb_wake`, not `wait_for_fb_sleep`** — the opposite of what
the node names suggest, and the whole reason this is worth writing down. `c00be76c` is the branch
taken when `fb_state == 1`: it stores 0 and wakes the queue the early-suspend handler is sleeping
on. `wait_for_fb_sleep_show` @ `c00be7ac` only *waits* (while `fb_state == 2`) and returns
`"sleeping"` (8 bytes @ `0xc099bddc`); `wait_for_fb_wake_show` returns `"awake"` (5 bytes @
`0xc099bdd4`).

So the userspace loop is:

```
read(/sys/power/wait_for_fb_sleep)   # blocks while fb_state==2 -> "sleeping": suspend beginning
read(/sys/power/wait_for_fb_wake)    # ACKs 1->0, then blocks until fb_state==2 -> "awake": resumed
```

Both reads return `-ERESTARTSYS` when a signal is pending, so **EINTR is normal and must be
retried**. Both nodes already exist and are readable — no kernel change needed.

Implemented as `namespace fbsync` in `cinder-home/src/main.cpp` (a dedicated thread, since both
reads block; SIGALRM blocked on it because that signal belongs to the render worker). Escape:
`/contents/cinder_no_fbsync`.

**Device-verified 2026-09-04.** The timeout line is gone from dmesg entirely, and entry latency
dropped from 1.31 s to **0.30 s** (`Beginning early_suspend` 257.787 -> `pm_autosleep_set_state`
258.088), a 77% cut. It also gives Cinder a **real late-resume stamp from the kernel**,
which is strictly better than noticing a `resume_count` change on the next 1 Hz tick.

---

## 2. `CG_PERI0` bit names — bit 24 identified

`dpidle_block_mask[CG_PERI0]` was `0x01000400` during early suspend. The `MT_CG_PERI_*` strings are
a contiguous table starting at `0xc0989fe8`, so **bit index = position in the table**:

| bit | clock | | bit | clock |
|---|---|---|---|---|
| 0 | `NFI` | | 18 | `NLI` |
| 1 | `THERM` | | 19–22 | `UART0..3` |
| 2–9 | `PWM1..7`, `PWM` | | 23 | `BTIF` |
| **10** | **`USB0`** | | **24** | **`I2C0`** |
| 11 | `USB1` | | 25–27 | `I2C1..3` |
| 12 | `AP_DMA` | | 28 | `AUXADC` |
| 13–17 | `MSDC30_0..4` | | 29 | `SPI0` |
| | | | 30 | `ETH` |
| | | | 31 | `USB0_MCU` |

So `0x01000400` = **USB0 (the cable) + I2C0**. This also **proves bit 10 = USB0 on this SoC** — it
had been carried as a guess from the MT8127/8135 family layout.

**I2C0 is the codec bus.** Devices on it: `0-004e CODEC_CXD3778GF`, `0-0010 mkl17z32vda4_fw` (an
NXP Kinetis KL17 MCU — the `cxd3778gf_ucom` μCOM), `0-004c pcm1795` (a TI DAC), `0-007f
kd_camera_hw`. The latter two are almost certainly unpopulated probe stubs on this model.

**RESOLVED — I2C0 is NOT a standing blocker, and the one observation of it was self-inflicted.**
Sampled every 2 s across a full stage-1 window (t=264..281), the mask reads **`0x00000400`
throughout — USB0 only**, with `by_clk` climbing ~50/s entirely from the cable. Bit 24 never
appears. The single `0x01000400` reading came from a window in which *this investigation* was
repeatedly reading `/proc/regmon/cxd3778gf/{target,value}`, i.e. driving I2C0 traffic itself.
A measurement that perturbs what it measures — worth remembering before chasing the next one.

cinder-home's own 1 Hz jack poll was also considered and is innocent regardless: `jack_watch_tick`
reads `/sys/class/switch/cxd3778gf_h2w/state`, a switch-class node the driver updates from its jack
IRQ — a memory read, not an I2C transaction.

**So the deep-idle picture is now complete and clean:** `by_vtg` is opened by stage 1, and the only
remaining `by_clk` contributor on the cable is USB0 — the cable itself. Off-cable there should be
nothing left blocking, which makes an off-cable stage-1 run the measurement that settles it.

Note `/sys/power/dpidle_state` accepts `echo disable <id> > …` to mask a blocking condition. That is
a live grenade — masking a clock condition lets the SoC deep-idle while a device still needs its
clock — and it is not the fix for anything here.

---

## 3. The connectivity chip is powered from boot — real, but no safe lever

`AD_WHPLL_CK` reads **480 MHz** on a fresh boot with Bluetooth off, and still 480 MHz *during* early
suspend. Never drops. From dmesg:

```
[7.468887] (1)[243:pwr_on_conn][WMT-STP-EXP][I]mtk_wcn_wmt_func_on: mtk_wcn_wmt_func_on_f type(9)
```

`pwr_on_conn` is a thread **inside `6620_launcherA`**, which `/init.project.rc:106` starts as an init
service. So MTK's own launcher powers the combo chip up unconditionally at boot, regardless of BT
state — and does it again after every late resume (same line at t=289.4, right after `LR handlers
15: [wmt_dev_late_resume]`).

**DOWNGRADED 2026-09-04 (same day): `AD_WHPLL_CK` reads 479984 while Bluetooth is ACTIVELY
STREAMING LDAC — identical to the BT-off reading.** The PLL does not move between "radio idle" and
"radio carrying an A2DP stream", so it is not a proxy for what the chip is doing and it is weak
evidence for the chip burning meaningful power. Treat the whole item as unquantified.

**Why this is a lead and not an action:**
- The driver has its own power-save layer (`wmt_lib_ps_action`, `wmt_lib_ps_enable`,
  `wmt_lib_notify_stp_sleep`, and a live `mtk_stp_psm_a` thread), so an enabled PLL does **not**
  prove the chip is burning meaningful power.
- `/dev/stpbt` is **not open by any process**, so the BT function itself is idle.
- There is no userspace power lever: `/proc/driver/wmt_psm` is **absent** (so `wmt_dbg_a`'s
  "echo 15 xx > /proc/driver/wmt_psm" hint is a dead end) and
  `/sys/class/{stpbt,wmtdetect,stpwmtA}/*` expose no attributes at all. `/dev/stpwmtA` is held open
  by the launcher.
- The only way to act is to stop an init service that owns Bluetooth. Not worth it, and poking this
  stack has rebooted this device before.
- **And it cannot be measured**: there is no fuel gauge on this device, so "is it worth it" needs a
  multi-hour voltage-decay A/B, not a spot check.

Function type 9 is past the four named types (`DRV_TYPE_BT/FM/GPS/WIFI` = 0..3 in
`mtk_stp_wmt_soc.ko`); in the stock MTK enum that position is `LPBK`. Not confirmed for this tree.

---

## 4. Closed questions

- **AFE is on with no audio**: `AFE_DAC_CON0 = 0x00000001` (bit 0 = AFE_ON) via `/proc/regmon/afe_reg`.
  Resolves the old "MTK AFE looks on (unresolved)" note. But `dpidle_condition_mask[CG_AUDIO] = 0`,
  so it does **not** gate deep idle — a small static cost, not a blocker.
- **The CPU path is optimal.** Governor `hotplug`, `scaling_min_freq` = 598000 = the hardware floor,
  `time_in_state` 77% at 598 MHz / 20% at 1040 / 4% at 1300, cpu1 hotplugs offline on its own.
  `scaling_cur_freq` always reads 1300000 because your own adb command ramps the core — use
  `time_in_state`.
- **`VENCPLL`/`MMPLL`/`MSDCPLL` behave correctly**: VENCPLL 295 MHz screen-on, **0** screen-off;
  the other two 0 throughout.
- **The GPU is entirely unused** — `AD_MMPLL_CK=0`, cinder-ffi logs "software framebuffer present
  path (GPU opt-in flag absent)". The Mali-450 path exists (`player/cinder-ffi/src/gpu.rs`); enable
  with `/contents/cinder_gpu_on`. Untested.

## Tooling notes

- `/proc/clkmgr/fmeter` reports **actual measured clock rates** — the direct way to answer "what is
  powered". Read bounded: `dd if=… bs=4096 count=1`.
- **There is no `timeout` binary on the device.** Put the timeout on the host side of adb.
- **`adb shell` never propagates the remote exit code** (`adb shell 'exit 3'; echo $?` → 0), so
  `until adb shell '<test>'` fires on the first iteration. Match on stdout instead.
