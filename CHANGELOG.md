# Changelog

All notable changes to Cinder are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). While the major version is `0`, the
minor version moves for anything user-visible and the patch version for fixes.

**Entries are honest about verification.** A line marked *device-verified* was executed on real
hardware. A line marked *device-unverified* is code-complete and gated on a hardware session — a
distinction this project tracks deliberately, because most of what ships here cannot be proven by
a test suite. See [`cinder-home/STATUS.md`](cinder-home/STATUS.md) for the standing matrix.

This file was backfilled from git history on 2026-09-01. Releases before then are summarised at the
level the commit history supports; from `v0.1.6` onward, entries are written as changes land.

## [Unreleased]

### Changed

- **The Rust half of the player is compiled for speed, not size** (`player/Cargo.toml`).
  `opt-level` was `"z"` from the first commit, with no rationale recorded anywhere in the tree, on
  a device whose constraint is battery rather than flash. It is now `2`. *device-verified.*

  Found by the audit in PR #9, and the headline reproduced exactly on host — 2.0-3.2x across every
  rendering and sorting bench. **On the device it is about 1.1x, not 2-3x**, measured by flashing
  both builds from identical source and reading the windowed raster sampler: `frames 1..300`
  (the only window that follows a fixed boot sequence, so the only comparable one) goes 6.93 ms at
  `"z"` to 6.14 and 6.32 ms across two `2` builds, and the library build goes 4.77 s to ~4.54 s.
  Two `2` builds three percent apart put the noise floor at about +-3%, which a 10% gap clears but
  not by much. **The host bench overstates this device's gain by roughly 2x** — probably because the
  1.5 MB canvas sits in the host's cache and nowhere near the A7's, so a good share of every device
  frame is DRAM bandwidth that no codegen flag can touch. That last part is inference; only the
  direction is measured.

  Two of the audit's numbers did not survive the device. **The size cost is +16.9%, not +4.6%** —
  the audit measured `libcinder_ffi.a`, a static archive that does not ship; the linked, stripped
  ARM binary goes 3,661,868 -> 4,280,388 bytes (+604 KB), which is still nothing against 490 MB
  free on `/system`. And **the boot dead time is not a codegen problem**: the ~4.5 s library build
  moves about 5%, because that path is bundled SQLite, which is branch-heavy rather than loop-heavy
  and is not what `"z"` punished.

- **Hardware volume no longer forks a shell.** `volume_write_now` and `read_volume_hw` ran
  `amixer` through `system()`/`popen()` — two fork+execs per step, up to eight times a second while
  the volume rocker auto-repeats, on the one core the render thread also needs. They now use
  `cinder_codec_set_master_volume` / `..._get_master_volume`, a single ALSA control ioctl that was
  already written, already in the shipped binary and already declared in a header `main.cpp`
  includes — it had simply never been called (audit B4). *device-unverified — needs a volume-key
  press on hardware.*

  Only when the configured mixer is exactly the one the shim drives (card 0, control
  `master volume`). `/contents/cinder_volume.conf` can point the backend elsewhere, and that
  escape hatch keeps the fork; so does any ioctl failure, so this can make volume faster but never
  absent.

- **The gradient cache evicts instead of emptying itself** (`player/cinder-ui/src/art.rs`, audit
  B9). On overflow it called `clear()`, discarding the entries for the ~14 rows *currently on
  screen* along with everything else, so the next frame re-baked all of them — recurring every 64
  distinct album names while scrolling. Entries now carry a last-used tick and the oldest half is
  dropped, which keeps the visible set by construction. The 64-entry cap is unchanged.

### Fixed

- **The raster sampler was measuring the boot, not the UI, and never switched off** (audit B10).
  It reported a single cumulative mean at frame 300; on device those 300 frames span first paint
  (~1.2 s) to about 14.7 s, which is the near-empty boot screen with the render thread starved by
  the synchronous library build. It read 6.21 ms before the compiler-profile change and 6.23 ms
  after — a null result convincing enough to nearly retire the change, and in fact two samples of
  the wrong thing. It now reports each 300-frame window on its own and keeps sampling into real
  use, and it stops completely after 30,000 frames instead of leaving two clock reads and two
  atomic RMWs on the per-frame path for the life of the process.


- **Investigated, then REVERTED: telling the kernel the device went idle.** *(the change is not
  shipped — it wedged the device off-cable; kept here because the RE is worth having and the
  failure is worth not repeating)*
  Turning the screen off wrote the backlight brightness node and called Sony's
  `display_backlight(0)`. Neither touches the kernel PM core. The MTK early-suspend chain — which
  every driver on this SoC registers its low-power handler with — is started only by a write to
  `/sys/power/state`, and nothing on this system was writing it. Result: **the SoC had entered deep
  idle exactly zero times in every boot ever sampled.**

  The blocker was one byte. `dpidle_handler` gates on `mt_cpufreq_earlysuspend_status_get()`, which
  returns `mt_cpufreq_earlysuspend_allow_deepidle_control_vproc` — and the block counter is
  reported as `by_vtg`, which reads like a voltage problem and is nothing of the kind. That name
  cost an hour before the kernel symbol table settled it.

  Cinder was wired to write `mem` on screen blank and `on` on wake. **It does not ship.** Four
  on-cable tests were clean — the chain ran, the panel powered down and came back with a full LCM
  re-init every time. Off-cable it suspended the device to RAM, and **neither the Power key nor
  plugging USB in would wake it.** Forced reboot, which also zeroed the counters the test existed
  to read.

  The chain does not stop at early suspend: it ends with `pm_autosleep_set_state(3)`, the real
  suspend path. On the cable USB holds a wakeup source so it never completes, which is why every
  on-cable run looked like proof. Off-cable and idle, nothing holds one.

  The bad inference is worth naming: Sony's codec driver takes a `CXD3778GF` wakeup source during
  playback, that was measured and is true, and it was used as the safety argument. It protects the
  *playing* case and says nothing about the *idle* case — which is the only case the wiring ever ran
  in. A measurement that is true and irrelevant is more dangerous than one that is simply wrong.

  What survives: the RE (`analysis/RE_early_suspend.md`), and the knowledge that on the cable one
  clock blocks deep idle — `/proc/clkmgr/clk_test` names it `MT_CG_PERI_USB0`, the only clock
  currently on that sits inside the deep-idle condition mask. `main.cpp` carries a do-not-rebuild
  note where the helper was.

- **The DAC and headphone amplifier never went to sleep.** *(mechanism device-verified 2026-09-03;
  the cinder-home wiring is device-unverified — installing it needs a reboot)*
  With the screen off, Bluetooth off, **nothing in the headphone jack** and no PCM substream open
  anywhere, the CXD3778GF was fully awake: blocks enabled, three clock-enable registers non-zero,
  the oscillator running, everything out of reset, the charge pump control bits set and the S-Master
  single-ended path selected. That is the headphone amp powered to drive an empty jack, for as long
  as the device is switched on.

  Sony's driver implements standby properly — writing the control drops the chip so far that regmon
  cannot read a single register over I2C, which is the measurement that matters (a control reporting
  "on" is a flag; `DEVICE_ID=UNREADABLE` is the chip actually powered down). The audio path also
  *clears* standby by itself when a PCM opens. What nothing does is put it back, and the kernel
  early-suspend hook the driver registers never fires because nothing on this system drives that
  chain. So the codec woke for the first sound after boot and stayed awake.

  cinder-home now puts it into standby after 30 s of screen-off and not-playing, and takes it out on
  the way back. This cannot break playback: opening a PCM wakes the codec with no help from
  userspace — verified by putting it to sleep, playing a second of silence, and finding every
  register restored. The 30 s grace stops a pause between tracks cycling the amp.

  Two ioctls on `/dev/snd/controlC0`, no libasound: the binary that would gain the dependency is the
  Home app, and a Home app that will not start is a device recovered by hand. No setuid helper —
  that node is owned by `system` and cinder-home runs as uid 100, which *is* `system` here. The
  control is addressed by name rather than numid, since numid is an artefact of driver registration
  order.

  **This is not the SoC deep-idle block, and the two were explicitly not conflated.** Tested: with
  the codec held in standby for 30 s, `dpidle_cnt` stayed at 0 and `dpidle_block_cnt[by_vtg]` kept
  climbing at an identical ~240/s. Separate problem, still open.

  The power saved is **not measured and cannot be here** — this unit has no `current_now`, only
  `voltage_now`, and a USB cable pins the gauge at Full, so idle draw has to be measured cable-out.
  The case rests on what the registers say is powered.

  Surveyed the other chips at the same time. The **FM tuner is already powered down** (`POWERCFG`
  ENABLE bit clear) and the green LED at full is the charge indicator — neither is a fault. The
  **MTK audio front-end** reads `AFE_DAC_CON0 = 1` (AFE_ON) with no PCM open, which has the same
  shape as the codec finding; left alone rather than guessed at, because its bit meanings are not
  established and guessing at a power register is how devices break. Full detail:
  `analysis/RE_codec_power.md`.

- **A USB cable could delete the microSD library, and did.** *(device-verified 2026-09-03)*
  A third of the reference device's music was missing from the player: 121 album rows in
  `MTPDB.dat` with no tracks behind them, including every album on the card. The internal index was
  perfect — 2326 files in `/contents/MUSIC`, 2326 rows, zero difference in either direction — so
  the fault was entirely on the removable side.

  Sony's storage manager exports removable storage to the PC on **any** USB connection, a charger
  included. Internal storage is exempt, which is why `/contents` and adb survive a cable; the card
  is not, and gets unmounted and handed to the mass-storage gadget as `lun1`. If the media scanner
  is partway through indexing the card at that moment, the partial index is discarded wholesale.
  The evidence was still on the device: `/db/MTPDB.dat.scanning2` held the scanner's checkpoint,
  frozen at `Burial - Untrue/04 - Ghost Hardware.flac`, and a backup DB taken earlier that day had
  **637** external rows where the live one had none. A *completed* index does survive the unmount,
  so this only bites while a scan is in flight — which is exactly the state a new card, or a card
  that has just gained music, is in. Every reconnect restarts the scan from zero.

  Cinder now clears Sony's `AutoExportAsMsc` setting at startup, which stops the export
  transaction being raised at all, so both storages stay mounted across a cable. Nothing is lost:
  deliberate USB transfer still works, because Settings ▸ USB mode goes through init
  (`sys.sony.config`) and never touches StorageMgr. What goes away is the *automatic* handover.
  Cinder re-applies the setting at every startup, and that re-apply is what makes the fix stick.
  The service does write it to NVP (`FNC_MSC_AUTOEXPORT`) and read it back at boot — the round trip
  is right there in the disassembly — but measured after a reboot it was back ON, so it cannot be
  relied on. (That reboot was a kernel panic rather than a clean shutdown, so the clean case is
  unverified rather than disproved; the re-apply covers both, plus a factory reset.)

  Verified on hardware with the cable connected: `Mount(External0) -> 0`,
  `/dev/block/mmcblk1p1` mounted at `/contents_ext`, `lun1 = (empty)`, and hagodaemon reporting
  `status[Mounted]` with adb still up — a combination that was previously unreachable, because the
  card and the PC could not both hold the device at once. Sony's own log states the fix directly:
  `transact ApiId: [Export] to storage: [External0] is disabled`, a line that previously named only
  `[Internal]`.

  Two candidate causes were ruled out with measurements rather than argument. The
  `Not exFAT or failed to access device` line in the log is informational — the card is FAT32 and
  the vfat path works. And `/db`, which had never been measured, has **89 MB free** against a
  5.5 MB database that grows ~1.6 KB per track; `images` stores references into the source FLACs,
  not blobs, so album art costs it nothing. Space was never the constraint, and could not be:
  a full disk cannot retroactively *delete* rows a backup proves existed.

### Added

- `cinder-probe --codec` — reports whether the DAC is asleep and dumps the codec's own power
  registers alongside the control, so the answer comes from the chip rather than from a flag.
  `sleep`/`wake` drive standby; bare invocation is read-only.
- `analysis/RE_codec_power.md` — the codec power finding, and the idle-state survey of the other
  four chips `regmon` exposes.
- `analysis/RE_early_suspend.md` — the kernel disassembly showing that the SoC has never entered
  deep idle under Cinder, why (`by_vtg` is the early-suspend flag, not a voltage check), the
  hardware results, and two corrections to earlier claims made in that same document. Includes how
  the kernel was extracted from the stock `.UPG` offline, since `/dev/kmem` panics this device.
- `cinder-probe --storage` — reads Sony's auto-export setting, and can turn it off/on or mount the
  microSD on demand. Bare invocation is read-only. Also prints `/proc/mounts` and the gadget LUN
  state together, because the card's absence from the mount table only makes sense next to the LUN
  that is holding the block device.
- `analysis/RE_storagemgr.md` — the disassembly behind the above, and the client ABI.
- `analysis/RE_kernel_power.md` and `analysis/kernel/` — the SoC power framework read out of the
  stock kernel image. The SPM wake-source mask (`spm_sleep_wakesrc = 0x01204564`) with the kernel's
  own bit names, MTK's per-scenario golden register settings for `idle`/`dpidle`/`audio_playback`,
  the codec's 210 named registers and the PMIC's 425, and an extractor script that regenerates all
  of it from `vmlinux.bin`. Also documents Sony's `/sys/devices/platform/icx_pm_helper/`, which
  records the wake source and timing of the last suspend and is the post-mortem the failed
  experiment did not have.

  **This corrects the reverted entry above.** That entry says neither the Power key nor USB would
  wake the device, and concluded there was no wake source. The tables say `KP` is genuinely not
  armed, but `EINT` is, the PMIC is EINT 150, and the PMIC power-key interrupt is enabled
  (`INT_CON0 = 0x0420`) — so a wake path existed. The likely failure is in the **resume** path
  instead. Nothing was re-tested: the device has still never completed a suspend/resume
  (`resume_count = 0`), and it will not be retried unattended.
- **Tone-control tables, wired for the first time.** *(device-verified 2026-09-04)*
  Sony loads a `tc_*.tbl` into `/proc/icx_audio_cxd3778gf_data/tct` at every boot and nothing in
  Cinder had ever touched it — it is the other half of what Walkman One sells as a "sound
  signature", and only the volume half was implemented. `cinder-voltable` gained `tone-stock`,
  `tone-w1` and `tone-wm1a`. Separate keys from the volume ones on purpose: applying a volume curve
  must not silently also change tone. Same whitelist-key pattern as before, so a caller still
  cannot name a path (verified: `cinder-voltable ../../etc/passwd` → exit 2).
- `cinder-probe --volcurve [step] [force]` — measures the output volume table by sweeping the
  volume and reading the analogue attenuator (`PHV_L`/`PHV_R`) back at each step. That *is* the
  curve, measured silently with nothing playing, so table comparisons no longer need an ear or a
  recording rig. Restores the original volume, and refuses to sweep with something in the jack
  unless forced.
- `cinder-probe --gain [high|normal] [force]`, and `cinder_codec_get/set_gain_mode`,
  `get/set_playback_latency`, `get_jack_se`, `get/set_master_volume` in the codec shim — the
  S-Master output-gain mode Sony ships disabled. Interlocked: raising gain is refused while
  anything is in the headphone jack, because the failure mode is somebody putting headphones on at
  their usual volume. `normal` is never interlocked — it can only make things quieter.
- `analysis/RE_volume_tables.md` — **a negative result, with the control experiment that makes it
  trustworthy.** The region volume tables (`ov_1291.tbl` vs `ov_1291_cew.tbl`) differ in 7576 bytes,
  always in the quieter direction, and looked exactly like the European output cap. Measured: they
  produce **identical** volume curves. The `limiter_*.bin` files are a second dead end — 3 bytes
  each and byte-identical across regions. Applying the WM1A table in the same session *does* change
  the curve, which is what proves the writes landed and the null result is real. **There is no EU
  cap to lift here.**
- **The SoC entered deep idle for the first time.** *(device-verified 2026-09-04)*
  `dpidle_cnt[0]` went **0 → 78** across three suspend/resume cycles off-cable. Every boot ever
  sampled before this read zero. This is the thing the whole early-suspend investigation was for,
  and it lands exactly where the kernel disassembly said it would: `by_vtg` is the early-suspend
  flag rather than a voltage check, nothing on this system ever set it, and driving the chain opens
  the gate.

  Roughly 39 deep-idle entries per awake window. **`by_vtg` keeps climbing alongside**
  (107569 → 121952), so the gate is intermittent, not permanently open — this is not yet "deep idle
  fixed", and the entry above should not be read that way.

  The SPM wake timer turned out to be unnecessary: `slp_pwake_time` was `-1` for the entire run and
  the device woke anyway, three times, on gaps of 5/168/10 s with `r12=0x20` (GPT — the kernel's own
  timers). It was a safety net that was never needed.

  Now wired into cinder-home as **stage 1** — see the suspend entry below. Stage 2 (the part that
  actually reaches RAM) stays opt-in until the gadget bug is fixed.
- **Remaining: the USB gadget does not re-enumerate after a resume.** The screen comes back, the
  device is fully alive, and the host still never sees it — adb stays down until a reboot. That is
  the entire residue of what was recorded as "the device bricks off-cable".
  `/sys/class/android_usb/android0/enable` is world-writable, so `tools/pmtest.sh` now bounces it
  after leaving suspend. **Untested.**
- **The suspend "wedge" was never a wedge.** *(device-verified 2026-09-04)*
  The device suspends and resumes correctly off-cable — five cycles in one run, `resume_count`
  0→5, `/proc/uptime` gaps of 30/32/32/31/23 s. First completed suspend/resume in the project's
  history.

  With a wake timer armed and nothing holding a wakelock it enters a stable cycle: suspend, wake on
  the timer ~30 s later, stay up for **~18-20 s**, suspend again. That awake window is Sony's own
  resume wakelock — `icx_pm_helper/resume_lock_ms` reads **20000** — a node this investigation had
  already read without realising what it explained.

  It *looks* dead because nothing ever writes `on` back to `/sys/power/state`, so the device never
  leaves the early-suspend state: the panel stays dark through every awake window and a USB replug
  usually lands mid-suspend. A perfectly cycling device is indistinguishable from a hung one from
  the outside, which is exactly how the first attempt got recorded as a wedge.

  **Two earlier diagnoses in this changelog were both wrong** — first "there is no wake source",
  then "the resume path does not complete". The real defect is duller than either: **nothing calls
  the exit.** `echo on > /sys/power/state` takes `autosleep` from `mem` back to `off`, verified.
  cinder-home now calls it on every resume.
- `tools/pmtest.sh` — the harness that got the answer, and self-recovers so a run no longer costs a
  forced reboot. Three things it exists to work around, each found by rehearsing rather than by
  reading: `echo mem > /sys/power/state` is **asynchronous** (so a script that logs on the next line
  logs before anything suspended); the evidence has to outlive the forced reboot that recovers a
  failure, so the log lives on `/contents` and is `sync`'d per line; and **`trap '' HUP` is rejected
  by this shell** — busybox ash wants `trap '' 1 2 15`, and without it the script dies the moment
  the cable is pulled. The detector is `/proc/uptime`, which counts time spent suspended, so a jump
  between two samples taken a second apart *is* the suspend. Lights the LED on resume so success is
  visible with a dead display — note there is only **one** LED on this device and it is also the
  charge indicator, so off-cable any light is the signal and on-cable it means nothing.
- **SoC suspend is wired into cinder-home, in two stages, and stage 1 is device-verified.**
  *(2026-09-04)* Stage 1 takes a wakelock **and then** writes `mem` to `/sys/power/state` — that
  order matters, because `mem` with no lock held arms autosleep and the kernel can reach RAM before
  the next tick. The result is the early-suspend chain running in full (all 15 ES handlers,
  `clkmgr_early_suspend` included) with `try_to_suspend` then declining to go further because
  `active wakeup source: cinder` is ours. Deep-idle gate open, device fully alive, adb up,
  `resume_count` still 0.

  Stage 2 — release the lock and let it reach RAM — is **opt-in behind `/contents/cinder_ram_suspend`**
  and stays that way until the gadget bug above is fixed, because today a suspend-to-RAM costs a
  reboot to undo. Config: `/contents/cinder_suspend_s` is the idle threshold in seconds;
  `/contents/cinder_no_suspend` is an escape hatch checked every tick.

  On resume it does **not** try to work out what woke it. That was tried and it is wrong on this
  SoC: a Power press woke the device in 17 s against a 300 s timer — unambiguously the button — and
  SPM's `R12` still read `0x20`, which decodes as GPT. It writes `on` on any resume without touching
  the panel and lets the ordinary input path light the screen, which cannot mis-classify. Resume
  latency measured **~0.8 s**.
- **Fixed: the codec standby had never once fired on a real boot.** *(device-verified 2026-09-04)*
  Both it and the new suspend gated on `!g_screen_on && !g_playing`, and `g_playing` is *intent*: it
  is initialised `true` and the only place that adopts PlayerService's view sits behind
  `if (have_pos)`, so on a boot where nothing has been played there is no position report and it
  stays true for the life of the process. Measured: screen off since t=33, every ALSA PCM `closed`,
  and at t=304 neither the 30 s standby nor the 60 s suspend had fired.

  This was already written down in `apply_pump_interval()`, and both it and the auto-power-off guard
  work around it — the codec/suspend site was simply missed. Now uses the same idiom as that guard,
  requiring the service to agree and not just our intent:
  `const bool audible = g_playing && cinder_audio_is_playing() != 0;`

  With the fix, on a clean boot: standby at **t=70.579**, 30.3 s after screen-off. Confirmed at the
  hardware level rather than from the log — every `cxd3778gf` register returns `invalid length`
  through regmon while the `mt6323` PMIC answers normally to the identical procedure, which is the
  signature of Sony's standby dropping the chip below I2C reach. Until now the headphone amplifier
  stayed powered to drive an empty jack for the whole session, on every boot.
- **Fixed: a failed read of `/contents` at startup disabled suspend for the whole boot, silently.**
  `/contents` is `/emmc@contents`, and the USB mass-storage gadget rebinds it as a LUN *during boot*
  (`fsg_store_file file=/emmc@contents` at 10.1/11.1/12.7 s) — cinder-home starts at 10.17 s. The
  threshold reader cached `0` on any read miss, so a boot that asked one tick too early never armed.
  It now latches only on an answer worth trusting (a good value, or a readable `/contents` that
  genuinely lacks the file) and retries otherwise — and **logs its decision either way**, where
  before it logged only the enabled case and so stayed silent exactly when it mattered.

  The same window silently cost the launcher its log redirect: `run_home`'s `can_append` probe
  missed, it took the no-redirect branch, and the whole boot's output went to an inherited pty while
  `/contents/cinderhome.log` stayed 0 bytes — which is why the failure above left no evidence of
  itself. `run_home` now falls back to `/data/cinder/cinderhome.log` (ext4, mounted at 3.98 s, never
  touched by the MSC gadget) instead of dropping the redirect.
- **Cinder now answers the framebuffer early-suspend handshake, cutting suspend entry by ~75%.**
  *(2026-09-04)* Every early suspend used to pay a full second to
  `stop_drawing_early_suspend: timeout waiting for userspace to stop drawing` — measured entry cost
  was 1.31 s wall, 1.00 s of it that one handler giving up on an answer nobody was going to send.

  The protocol was read out of the kernel image rather than assumed, and the interesting part is
  that it is the opposite of what the node names suggest: **the acknowledgement is in
  `wait_for_fb_wake`, not `wait_for_fb_sleep`.** Reading `wait_for_fb_wake` is what takes `fb_state`
  from REQUEST_STOP_DRAWING (1) to STOPPED_DRAWING (0) and wakes the queue the early-suspend handler
  is sleeping on; `wait_for_fb_sleep` only waits and reports. So a correct implementation must read
  **both** nodes in order, and must not dawdle between them or the timeout fires anyway.

  Implemented as a dedicated thread (both reads block indefinitely; SIGALRM is blocked on it because
  that signal belongs to the render worker) with `/contents/cinder_no_fbsync` as the escape. It also
  hands Cinder a real late-resume stamp straight from the kernel, which beats noticing a
  `resume_count` change on the next 1 Hz tick.

  **Device-verified**: the timeout line is gone from dmesg entirely and entry latency fell from
  1.31 s to **0.30 s**.
- **`CG_PERI0` bit 24 identified as `I2C0` — and bit 10 = `USB0` is now proven rather than assumed.**
  The `MT_CG_PERI_*` names are a contiguous table in the kernel image, so bit index is table
  position. The deep-idle blocker seen during early suspend, `0x01000400`, is therefore the cable
  (USB0) **plus the codec's I2C bus** — except that on re-measurement I2C0 turned out **not** to be
  a standing blocker at all. Sampled every 2 s across a whole stage-1 window the mask reads
  `0x00000400` throughout, USB0 only; the single sighting of bit 24 came from a window in which this
  investigation was itself hammering `/proc/regmon/cxd3778gf`, i.e. generating the I2C traffic it
  then observed.

  Which leaves the deep-idle picture clean: `by_vtg` is opened by stage 1, and the only remaining
  clock blocker on the cable is the cable. Off-cable there should be nothing left.
- `analysis/RE_kernel_idle_levers.md` — the fb handshake protocol, the PERI0 bit table, the
  connectivity-chip finding, and the closed questions (the CPU path is optimal; AFE is on with no
  audio but does not gate deep idle; the GPU is entirely unused).
- **DEEP IDLE IS OPEN. Off-cable stage 1: `dpidle_cnt[0]` 0 → 18,509.** *(device-verified 2026-09-04)*
  486 s window, 37.8 deep-idle entries per second, `resume_count` **0** throughout — it never went to
  suspend-to-RAM — and USB came straight back on replug with no reboot. That is stage 1 doing exactly
  what it was designed for: the SoC in deep idle continuously while the device stays fully alive.

  **The number that matters is not the 18,509, it is `by_vtg` moving by ZERO** (101039 → 101039
  across all 98 stage-1 samples). Every earlier run had it climbing alongside `dpidle_cnt`, which is
  why this changelog said "the gate is intermittent, not permanently open — this is not yet 'deep
  idle fixed'". That caveat is now retired, and the reason is instructive: it climbed before because
  autosleep kept cycling the device in and out of suspend, clearing and re-setting the early-suspend
  flag each time. Held open by a wakelock, the flag is set once and the gate never blocks again.

  `dpidle_block_mask[CG_PERI0]` also went to literally `0x00000000` once the cable was out — exactly
  what the PERI0 bit table predicted, since USB0 was the only clock blocker on the cable.

  For scale, the previous best was **78**, in short cycles, with the panel cycling dark and the USB
  gadget dead until a forced reboot.

  **This makes stage 2 much less interesting than it looked.** It buys only the delta between deep
  idle and full RAM-off, and pays the unresolved gadget bug for it. Measure the delta first — and
  with no fuel gauge, that means a multi-hour voltage-decay A/B, not a spot check.
- **The DAC no longer stays powered through Bluetooth playback.** The codec standby's first cut
  deliberately kept the amplifier awake on BT ("being conservative in the first cut is worth more
  than the extra saving"), which meant the CXD3778GF was driving an empty jack for the whole of a
  screen-off BT listening session — one of the two ways this device actually gets used. It now goes
  to standby on a linked BT peer even while playing, because A2DP does not route through it.

  Gated on `cinder_get_bt_route()` (a peer that is linked *and* whose name was read back, not merely
  a radio claiming to be up — this device has reported a connection with no peer before). A mid-track
  disconnect self-heals: falling back to the jack opens the local PCM and Sony's driver clears
  standby on PCM open. It deliberately does **not** feed the SoC suspend, which keeps its stricter
  "never while playing anything" rule — early suspend under a live A2DP stream is a separate
  experiment with a separate failure mode.

  **Device-verified 2026-09-04**: standby fired 30 s into an LDAC session with the screen off, a
  track advanced afterwards with the DAC down, every `cxd3778gf` register read back `invalid length`
  while the `mt6323` PMIC answered normally to the identical procedure — and the listener reported
  no audible change at all. Both other directions were checked too: dropping the BT link mid-session
  woke the codec and the audio came out of the jack (Sony's driver clears standby on PCM open, so
  nothing in Cinder had to notice the edge), and — the negative control that actually mattered —
  playing through the jack with the screen off left the codec **awake** for a full minute of
  sampling, confirming the route flag clears rather than going stale and silencing the jack.
- **ATTEMPTED AND REVERTED: moving the library build off the render thread.** *(2026-09-04)*
  *(device-verified 2026-09-04)* The library build is 4837 ms on a 3,456-track library and it ran on
  the render thread, with the frame loop's bring-up gate holding back paint AND input for all of it.

  Two things had to change together, and either alone would have done nothing. `cinder_db_open` took
  cinder-ffi's state lock on its first line and held it across the SQLite open, `build_library`, the
  playlist store and the likes import — so moving it to a worker would only have relocated the stall
  to the render thread's next frame. It now builds against local values and takes the lock once, at
  the end, to install them. And the gate now falls through while the build is in flight, so the loop
  paints and reads input against an empty library until the real one lands and publishes a repaint.

  Measured on device: first frame 2.562 s, build starts 3.217 s, **input live 3.267 s**, build
  finishes 8.062 s. Input comes up 50 ms after the build starts instead of 4.8 s after it ends.

  **It does not work on this device and it is reverted.** The hotplug governor takes cpu1 offline,
  so `/sys/devices/system/cpu/online` reads `0` — there is exactly ONE core. A concurrent build does
  not run "in the background" there; it contends with the render and present threads for the whole
  machine. Measured: about five render-loop iterations across the entire 4.8 s build, even with the
  worker niced to 10.

  It broke two things visibly. The boot animation's last frame was left sitting in the middle of an
  otherwise-drawn Cinder Home for seconds, because the animation dies just after our first paint and
  nothing repainted over it until the build finished. And letting the loop run early meant
  `loop: BT route poll` issued a Sony IPC before hagodaemon was ready — the guard unwound out of it
  (`sig=14`), and an unwound Sony client is dead for the rest of the boot, so audio and Bluetooth
  were gone until a restart.

  Kept from the attempt: `cinder_db_open` now builds WITHOUT holding cinder-ffi's state lock and
  takes it only to install (a strict improvement, and the prerequisite if this is ever retried), and
  the loop's Sony-IPC section is now gated on bring-up having actually finished rather than on the
  loop merely being allowed to run — two different questions the old gate conflated by holding both
  back together. **The boot dead time is back by default**, but the threaded path is KEPT behind
  `/contents/cinder_async_library` rather than deleted — both of its failure modes now have fixes, so
  it is a flag away from being retried. Doing it without the flag needs the paint to make progress
  while the build runs, which on one core means slicing the build, not threading it.
- **Fixed: the boot-animation re-kill was paced by the frame counter.** `n < 300` meant "every
  iteration for the first 300 of them", and how long that is depends on what else the loop is doing
  — ~66 calls while bring-up blocked the loop, ~100 once it no longer did. It is now paced by the
  clock (every 200 ms for the first 3 s, then the existing 5 s ladder to a minute). Found by the
  harness, which caught the change pushing `autooff-idle`'s log budget from 114 to 179 lines against
  a ceiling of 120 — a reminder that budget had six lines of headroom.
- **Partial framebuffer blits: 0.7% of a full blit, measured on device.** *(2026-09-04)* `r.dirty`
  is a bool, so any change repainted and re-blitted the whole 480x800 — a progress bar ticking once
  a second cost exactly as much as a full screen change, and the forced repaint that runs every 5 s
  for the life of the process cost 1.5 MB to the panel plus a heavy ioctl every time.

  The blit now compares each row against what it last wrote and copies only the rows that differ.
  The trade is favourable because of where the memory is: the canvas and the shadow are ordinary
  cached RAM, while the framebuffer is a device mapping where the WRITE is the expensive side, so
  two cached reads to avoid one device-memory write is a good deal at every scale. When nothing
  differs it also **skips the FBIOPUT flip entirely** — the ioctl that occasionally blocks >33 ms —
  which is what makes a genuinely static screen free rather than merely cheap.

  Measured over the first 300 frames of a boot, which is the busiest the screen ever is: **1619 rows
  written of a possible 240,000 — 0.7%.** Verified that pixels still land: fb0 page 0 holds
  1,151,997 non-zero bytes of 1,536,000 with Cinder's background colour in the low bytes, while
  pages 1 and 2 read exactly zero (page-0-only mode, clean init clear).

  It is a pure optimisation of the TRANSFER — the bytes that end up in the framebuffer are exactly
  the bytes a full blit would have put there — so unlike dirty-rect rasterisation it cannot produce
  a wrong pixel. **It is nevertheless INERT as shipped**, because `/contents/cinder_fb_allpages` is
  restored and takes the unconditional path. It also needed a correction on the way: a shadow-based
  blit assumes Cinder is the only writer to fb0, and early in a boot it is not — `icx_bootanimation`
  draws into the same buffer, and a partial blit will not paint over it because the shadow says
  those rows are already correct. Hence the 15 s window after open in which the shadow is distrusted
  entirely. A full write still happens once a minute as insurance against anything else
  touching fb0, and `/contents/cinder_fb_allpages` still forces the old unconditional path.
- **WRONG, AND CORRECTED THE SAME DAY: `/contents/cinder_fb_allpages` is load-bearing.** I removed it
  as a leftover diagnostic and the boot animation immediately appeared on top of the Cinder UI. So
  the paging theory in `docs/DEVICE_SHELL_GOTCHAS.md` is CONFIRMED rather than superseded, and
  `fb0/pan` reading `0,0` is not sufficient evidence that the panel never presents another page. It
  is restored and should stay on; the partial blit below is therefore inert. The original (wrong)
  reasoning, kept because the failure mode is worth remembering:
  `/contents/cinder_fb_allpages` was still present on the device — `touch`ed 2026-08-18 as a one-flag
  ghost-UI test and never undone, so every frame wrote all three fb pages (~4.6 MB) instead of page 0
  (~1.5 MB), on a panel that only ever scans page 0. The ghost it was testing for had a different
  cause entirely (mtkfb returns the previous session's pixels across a reboot), root-caused and fixed
  2026-08-26 with a clear of the mapping at init. The boot log names the active mode — read it.
- **The GPU present path is not worth enabling, and the reason is structural.** `gpu.rs` replaces only
  the PRESENT, not the rasterisation: cinder_ui software-rasterizes the whole frame either way, so
  the GPU path swaps a full-screen memcpy for a full-screen texture upload of the same bytes, plus a
  quad draw, a swap, and powering up MMPLL and the Mali domain. The only gain on offer was vsync
  pacing — and the partial blit above has now made the software path cheaper by two orders of
  magnitude, so the comparison is worse than ever. Left off.
- `analysis/PLAN_deep_idle.md` — the plan for getting the SoC into deep idle without repeating the
  2026-09-04 wedge: `slp_pwake_time` as a hardware self-wake, `icx_pm_helper/resume_count` as the
  single success criterion, phases with explicit abort conditions, and the standing rule that none
  of it gets wired into cinder-home. Plan only — nothing executed.
- `analysis/RE_hardware_surface.md` — inventory of what the audio hardware can do that the firmware
  never asks it to: the S-Master **high-gain output mode** (present, writable, applied live, and
  shipped permanently at `normal`), a headphone-**impedance measurement** block with no driver code
  behind it at all, the S-Master noise-shaper and dither tuning surface, and the codec's
  coefficient RAM. Read-only survey — none of it was written to the device, and the gain mode
  deliberately so, because leaving amplifier gain raised is a hearing-safety matter for the user to
  decide rather than a config default to flip.


## [0.1.9] — 2026-09-02

### Fixed

- **The Linux installer refused to run, and the instructions it would have printed were fiction.**
  Two separate defects that together meant neither non-Windows path installed anything. First,
  `main` opened with `if !cfg!(windows) { exit(2) }` — so `cinder-installer-linux-x64`, which the
  release workflow has been building, testing and publishing on every push, exited immediately with
  "this installer must be run as the Windows release .exe". Every `#[cfg(not(windows))]` branch
  below it was unreachable code. Second, the message those branches would have printed told the
  user to eject the drive and select **Settings ▸ Device Settings ▸ Update** on the player. **The
  NW-A55 has no such menu entry** — this generation is updated only from the host — so a user who
  got past the first defect would have hunted for a button that does not exist while holding a
  correctly staged payload. *Reported from the device.*
- **The Linux installer now finishes the install instead of stopping halfway.** The claim it made
  about itself — that Sony's `SoftwareUpdateTool.exe` "has no equivalent outside Windows" — was
  wrong. The tool's last act is a single 12-byte vendor SCSI command (`fc 00 04 'd' 'b' 'm' 'n'`,
  flag byte 0x80 with an 0x00 fallback), documented by Rockbox's `nwztools/scsitool` and sent by
  this project's own `tools/flash.sh` on every development flash. The installer now sends it
  directly via `SG_IO`, after syncing and unmounting so the payload is actually on the flash before
  the player reboots. This needs root; a permission failure now says so and prints the `sudo`
  command rather than reporting a generic error. Still dependency-free — the ioctl is declared
  inline. *Device-unverified: exercising it reboots a player into its updater.*
- **macOS is now honest rather than wrong.** It cannot send the command at all — raw SCSI there
  needs an IOKit `SCSITaskUserClient`, which the kernel will not grant for a disk it has already
  mounted, which is exactly a staged Walkman. The installer stages the files and says so, and
  points at Linux or Windows to finish.
- The "no Walkman found" hint printed a Windows drive letter (`cinder-installer D:\`) on every
  platform. It now suggests a mount path off Windows.
- `README.md`, `install.md` and the GitHub release body carried the same non-existent-menu
  instruction and the same "no equivalent outside Windows" claim; all three are corrected, and
  `install.md` gains a table of what actually triggers the update on each host.

### Changed

- **The release body is now rendered from a file, with the build's checksums inlined.** It was an
  inline `body:` block in `release.yml` carrying a comment that said "the sums go in the release
  body so a download can be checked without trusting the download itself" — above a step that only
  wrote them to an attached `SHA256SUMS`. Inline YAML cannot hold anything computed, so the comment
  described something the format could not do, and a release body is invisible until it is
  published. The prose now lives in `.github/release-notes.md` with a `{{SHA256SUMS}}` marker, and
  `tools/render_release_notes.sh` — the same script CI runs — substitutes the real sums. It refuses
  to render a body with no sha256 lines in it rather than publish a verification section that looks
  done and is empty. `tools/render_release_notes.sh --preview` shows the body locally.
  (GitHub also displays its own per-asset `sha256:` digest, computed at upload; the two were
  checked against each other on v0.1.8 and agree on all four assets. They cover different steps —
  GitHub's covers storage and transport, these cover the build.)

### Fixed

- **`tools/check_payload_attrs.sh` was committed non-executable, which broke CI.** `ci.yml` invokes
  it as `run: tools/check_payload_attrs.sh`; the blob was mode `100644`, so the runner exited 126
  and took the off-device harness and launcher-recovery steps down with it. It passed locally
  because the working-tree copy had the execute bit — the index did not. `verify_payload_manifest.sh`
  had the same defect and survived only because `release.yml` runs it on `windows-latest`, where
  Git Bash ignores the mode; `release.sh`, `flash.sh` and the battery/BT measurement scripts are all
  documented as bare invocations and were all `100644`. Every one is now `100755`.

## [0.1.8] — 2026-09-02

### Fixed

- **The screen never actually went dark.** `set_backlight` has recorded since 2026-08-19 that the
  sysfs node alone does not darken this panel — measured with the node at 0, DisplayService at 2 and
  the panel still lit — and handles it. But neither path that blanks the screen went through it:
  `screen_auto_off` (idle timeout) and `screen_toggle`'s off-branch each carried their own copy of a
  raw `fputc('0')` to the node and stopped there. So the idle timer and the Power button both
  "turned the screen off" by writing a number the service overrides. Both now call one `panel_dark()`
  that caches the service's level and then zeroes both halves — the duplication is why the defect
  existed twice. *This is a standing bug in its own right and NOT the `g_ipc_dead` wedge; it was
  briefly attributed to that latch while diagnosing v0.1.7, which was wrong.* Two harness scenarios
  (`blank-idle`, `blank-order`) pin it, and both fail against the old code.
- **Losing Sony IPC was completely silent.** When `run_guarded` unwinds a Sony call it latches
  `g_ipc_dead`, and from that moment the device cannot play, pause, skip, drive Bluetooth volume or
  sleep its panel until it is restarted — with the only trace a line in `cinderhome.log`, which
  cannot be read from the device. The status strip now carries a persistent `AUDIO STOPPED —
  RESTART` banner in the codec badge's place, on every screen, set from the guard's recovery path
  through a lock-free atomic (`cinder_set_ipc_dead`) because that path must not take a mutex. The
  banner replaces the badge and the NIGHT label and nothing else — an early version returned before
  the right-hand block and dropped the battery readout, which is the one indicator a degraded device
  most needs. Four host tests.
- **New music copied over USB never appeared.** Nothing asked MediaStore to scan when a transfer
  finished — the DB reload on USB-MSC exit re-reads a store that has never heard of the new album,
  and the only trigger was a Settings row you had to know about. Exiting mass storage now arms the
  same bounded rescan campaign. *Device-unverified.*

### Changed

- **`.gitattributes` pins the release payload to byte-exact.** `release.yml` builds on
  `windows-latest`, where Git's `core.autocrlf` rewrote LF→CRLF at checkout for the two *text*
  members of the payload — so their hashes stopped matching and the v0.1.7 release failed its own
  manifest gate (run 33643552973) with every binary passing and only the text files failing. Nothing
  was stale; the bytes were changed in transit. This was latent from the day the gate was added:
  v0.1.6 was never tagged, so v0.1.7 was the first tag it ever ran on. `cinder-home/dist/**` is now
  `-text`, a new `tools/check_payload_attrs.sh` runs in CI on every push so the gate can no longer
  make its debut during a release, and `verify_payload_manifest.sh` names line endings as the cause
  when only text members fail. It also mattered beyond CI: `cinder-signature.sh` is executed on the
  device, and a CRLF shebang is `#!/bin/sh\r`.

## [0.1.7] — 2026-09-02

### Fixed

- **Skipping back could kill audio, Bluetooth and the screen-off timer for the rest of the boot.**
  `PlayController::PrevTrack` takes a `PrevTrackOption const*` and dereferences it eight
  instructions in, before any null check — there is no null check — and both `cinder_audio_prev_track`
  and `cinder_audio_prev_group` passed `nullptr`. The SIGSEGV landed inside a guarded call, which
  latches `g_ipc_dead` and refuses all further Sony IPC for the boot: the UI kept working while
  playback could not be paused or skipped, the panel would not sleep, the Bluetooth switch read OFF
  while a headphone stayed connected, and the volume rocker no longer drove the BT sink. It needed a
  restart, and the only trace was one line in `cinderhome.log`. Both calls now pass a real
  zero-initialised option. Rare only because `◁` reaches `PrevTrack` just within the 3 s restart
  grace and with no Cinder history to step back through. *Device-verified — diagnosed from
  `cinderhome.log.1` @134.479 (`sig=11 … PrevTrack+0x13`, `addr=(nil)`), whose fault offset is the
  `ldr r2, [r1, #0]` at `libPlayerServiceClient.so` `0x30d4`, to the instruction.*
- **Jumping to a track in Up Next threw away a "Shuffle all songs" order.** The tap emitted
  `PlayIndex`, and `PlayIndex` resolves an object id to its **album** — the only context an object
  id carries — so picking a song four rows down replaced the shuffled library with that song's
  album. The same defect `PlayPlaylistAt` was added for, reached from the other side. Up Next now
  emits `PlayContextAt(row)`: the shell restarts the sequence it already holds at a new index,
  without rebuilding or re-shuffling it. Rows are mapped through the same resolution filter as the
  sequence, so a file deleted since the context was built cannot slide the start onto the wrong
  track. *Device-unverified — two host tests pin it.*
- **Long titles were unreadable.** Now Playing fitted the title to 372px and the artist to whatever
  the codec left, so a classical or remix title lost exactly the part that identifies it
  ("Sinfonia concertante for Violi…"). The title and artist now scroll — dwell, slide, dwell, slide
  back — on both the day and night layouts, clipped to their own box by a new horizontal clip band
  on `Canvas`. List rows keep the ellipsis: forty animating rows would be noise, and a truncated row
  is still enough to pick from. A line that fits is drawn exactly as before and requests no
  repaints, so the common case costs nothing. *Device-unverified — five host tests plus rendered
  frames.*
- **`Settings ▸ Database` only ever scanned part of the library.** One `MediaScanner::Scan()`
  returns `rc=0` and stops well short of the whole tree: measured 2026-09-01, two presses took the
  store from 2,560 → 2,569 → 2,727 tracks, and it stopped there because nobody pressed a third
  time — leaving 696 tracks in 70 whole SD-card album folders absent from `MTPDB.dat`, which is the
  "some albums aren't detected" report. Folders were missing all-or-nothing, never partially, which
  is an interrupted walk rather than a tag or codec problem. A rescan now keeps re-issuing the scan
  from the housekeeping tick until the store stops changing (`db_signature`), bounded at 12 rounds
  10 s apart. *Device-verified as to the symptom and the increments; why one `Scan()` stops early is
  not established — the NULL listener cannot report it.*

- **The UI wedged after a quick run of track skips.** Transport presses were carried out inside
  `input_pump`'s evdev drain loop, where each one is a synchronous ~400 ms Sony round trip — so a
  finger on the glass kept the loop fed and starved the paint, the now-playing poll, the sleep
  timer and the auto power-off for as long as the user kept tapping. Presses are now queued and
  applied one step per frame by `transport_tick()`, the pending steps are net (▷▷×5 then ◁×2 is
  three steps, not seven), the backlog is capped, and the drain loop is bounded at 8 rounds per
  node per frame. Measured off-device: 40 taps in 4 s left the app issuing skips 13.1 s after the
  last one with housekeeping having run once in fifteen seconds; after the fix, 3.7 s and 13 of 15.
  Two harness scenarios (`rapid-skip`, `rapid-skip-touch`) pin it. *Device-unverified — the
  mechanism and the numbers are the harness's.*
- **`SIGALRM` was delivered to the wrong thread, turning recoverable timeouts into boots to stock.**
  `alarm()` is process-directed, and every thread Sony and cinder-ffi create inside `deferred_up()`
  inherited an unblocked `SIGALRM`. A guarded call that timed out usually had its signal land on one
  of the eight threads parked in a condition variable, where the handler fell through to "un-guarded
  hang", latched the bad-boot counter and `_exit(42)`. The handler now forwards a stray `SIGALRM` to
  the watchdog owner with `pthread_kill`. Probabilistic, which is why it read as "spamming skip
  *sometimes* crashes it". *Device-verified — three reboots produced the diagnosis.*
- **Calling back into Sony IPC after a guard recovery crashed the app.** `siglongjmp` out of an IPC
  abandons a half-built `std::string`/`std::vector` inside the client; the next call in dereferences
  it and faults in `libc++`. A five-second cool-down was tried first and was not enough — the object
  is broken, not busy. A recovery now ends Sony IPC for the life of the process: the UI, panel,
  buttons, library and timers survive; audio and Bluetooth need a restart. *Device-verified.*
- **`/contents` unmounted by something else is now reclaimed.** Sony's stack unmounts the music
  volume when a cable appears. Album art read inside that window failed, and the failure was cached
  in `art_key` for the rest of the boot, so an album stayed a grey gradient even after the volume
  came back. `cinder_db_open` now clears `art_key` and `last_track` so the current track re-derives
  immediately, the log and the scrobbler are re-pointed at the reappeared volume, and the mount is
  retried with a back-off.
- **The guard-recovery log line was truncated.** Its buffer was 192 bytes for a 209-byte message, so
  the most important line the fault path prints lost its tail. `-Wformat-truncation` had this red in
  CI.
- **The crash/hang guard could break more than the call it was guarding.** One `run_guarded` served
  every caller, and four of its call sites are not Sony IPC. A slow mount or `statvfs` set
  `g_ipc_dead` and cost the user audio and Bluetooth for the boot; and once that flag was set by
  anything at all, the guard refused *everything* — including the `/contents` reclaim, so one
  recovered transport timeout meant a cable plugged in later left the library missing and the album
  art grey until a reboot. Worse, `siglongjmp` does not run destructors: unwinding out of
  `cinder_db_open`, which holds the render `Mutex` across the SQLite open and library build, left
  that mutex locked with no owner for the life of the process.

  The guard is now three, chosen by what an abandoned call leaves behind — nothing (`GUARD_LOCAL`,
  recover and carry on), a half-built Sony client (`GUARD_IPC`, unchanged), or something that
  outlives the call such as a held mutex or an unreaped `system()` child (`GUARD_FATAL`, never
  unwound — a hang there is a clean labelled `_exit(42)` into the escape ladder, which is what the
  ladder is for). `guard_selftest.cpp` went from 4 tests to 9; the new ones fail on the old logic.
- **Album-art decode did not compile** after `zune-jpeg` 0.5 and `png` 0.18 changed their reader
  traits. Fixed, and `png` 0.18's `output_buffer_size()` now returning `Option` is handled with `?`
  — a second net under the dimension gate, where 0.17 would have allocated on a wrapped value.
  Verified beyond the host suite with a full `cinder-home/build.sh stable`: ARM link clean at
  `GLIBC ≤ 2.18`, under the device's 2.23, with `rusqlite` 0.40's newly bundled SQLite.

- **3.5 mm volume was unresponsive with the screen off.** Reported directly. The volume path was
  not at fault — the frame pacing was. With the panel dark the render loop sleeps in `poll()` to the
  next 1 Hz housekeeping deadline, which is the single biggest battery lever in the app and is right
  for events, because `poll()` returns immediately on one. But two things the rocker needs are not
  events, they are *deadlines* serviced at the top of the loop: the synthesized ramp's next step
  (every 120 ms while held) and the trailing write of a coalesced ramp (an `amixer` write is a
  fork+exec of `/bin/sh`, so steps are batched and the level the user stopped on is written
  afterwards). Nothing woke the loop for either, so releasing the rocker within 150 ms of the last
  write left the final level sitting in `g_vol_pending` until the next housekeeping tick — up to a
  second of nothing after you stopped pressing.

  It only shows with the screen off *deliberately*: an idle blank wakes on any non-Power key, so the
  panel lights, the budget snaps to 16 ms and the defect disappears. Power-off keeps it dark, which
  is the pocket case the rocker exists for. The loop now never sleeps past owed volume work, and
  sleeps exactly as long as before when none is owed. Rule and reasoning in `src/frame_budget.h`,
  checked by `tools/framebudget_selftest.cpp` (the harness cannot reach this — its clock is virtual
  and darkening the panel needs a `carry_out` it stubs).
- **The EQ shim forwarded gains the DSP silently zeroes.** `SetEq10BandValue` takes half-dB units
  and a value outside ±20 does not clamp inside Sony's service — it **zeroes the band**. The UI
  could never produce one (every site in the EQ screen clamps), but the settings loader parses `i8`
  from a file on `/contents`, which is vfat and writable by any PC the player is plugged into. One
  corrupted or hand-edited line silently flattened a band, drew its knob outside the EQ field, and
  was written back out on the next save. Clamped now in the loader *and* in the shim — the loader
  protects Cinder's model, the shim protects the service, which cannot defend itself and fails
  silently. Found while writing `cinder-audio`'s first-ever test (`SHORTCOMINGS.md` §A2).
- **`cinder_get_eq_bands` could leave its output uninitialised.** It returned early when the
  renderer was not up, leaving the caller's `signed char bands[10]` as stack garbage on its way into
  the DSP. Not reachable today, which is how it would have stayed until it wasn't. It now always
  writes all ten, flat when there is nothing to report.
- **`decode_jpeg` had no test.** JPEG is what every cover in a real library is (`magic=FFD8FFE0`,
  embedded in FLACs), and it was the one decoder with no coverage — which is why a semver-major
  bump could break it silently. Three tests added with 8×8 fixtures: RGB channel order, the
  grayscale fan-out branch, and a truncated file.

### Added

- **The Linux installer is published again** (it last shipped in `v0.1.2`). It stages every file and
  then stops — Sony's `SoftwareUpdateTool.exe` performs the USB handoff and has no Linux equivalent
  — and now says so and exits 0 instead of reporting a failure, printing the three manual steps that
  finish the job.
- **Repository groundwork:** `CHANGELOG.md`, `CODE_OF_CONDUCT.md`, issue templates (including one
  for reporting a `DEVICE_CHECKLIST.md` item run on real hardware), a pull-request template with a
  blast-radius section, `docs/README.md` as the documentation index, README badges, and a
  `cargo audit` job in CI with a weekly cron.
- `dependabot.yml`, scoped so it can only propose what CI can actually judge: semver-major updates
  are ignored for every crate the device links, because `cinder-home/build.sh` is the only thing
  that checks this tree against glibc 2.23 and no hosted runner runs it.

### Changed

- `cinder-home/dist/dev/` is no longer tracked. It was 74 of the 117 committed ARM-binary revisions
  in a 1.3 GB `.git`, and it is not what anyone installs — build it with `build.sh dev` when needed.
- `cinder-home/ROADMAP.md` now says which date it is a snapshot of. The forward plan is
  `docs/DEVICE_CHECKLIST.md`.
- **A release can no longer be cut without verification.** `tools/release.sh` rebuilds the stable
  channel and refuses to tag unless every committed byte matches — real, and until now entirely
  opt-in, because `git tag && git push --tags` reaches the workflow directly. The runner cannot
  repeat that comparison (no glibc-2.23 cross toolchain, which is why `dist/` is committed at all),
  so `release.sh` now records what it verified — every payload hash, plus the tag it was verified
  for — and `release.yml` checks the record still describes the tree. A tag cut without the script
  finds a manifest naming the previous version and fails. Shared script, so CI and a contributor run
  the same check (`tools/verify_payload_manifest.sh`). Not a signature; it closes the accident.

*Device-verified 2026-09-01 on an NW-A55: booted in 27.4 s with bootcount 0, library identical at
2560 tracks / 256 albums / 166 artists, zero guard recoveries, zero faults, and the `/contents`
reclaim observed running and succeeding at 13.3 s. Full write-up:
[`docs/AUDIT_2026-09-01.md`](docs/AUDIT_2026-09-01.md).*

## [0.1.5] — 2026-08-30

### Added

- Theme work and album-art handling in the UI (`theme.rs`, `art.rs`), and a wider `cinder-probe`
  (+327 lines of new probes).
- Harness coverage for the `pst` fake service.

### Fixed

- A batch of fixes across `main.cpp`, the FFI boundary and the navigation/settings screens.

## [0.1.4] — 2026-08-26

### Added

- **Device settings page** and a **battery UI**.
- Bluetooth and NFC work, with a full Bluetooth audit written up in
  [`docs/AUDIT_2026-08-26_bluetooth.md`](docs/AUDIT_2026-08-26_bluetooth.md).

### Changed

- Device checks completed and documented — see [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md).
- `build.sh` and `flash.sh` are executable by default again.

## [0.1.3] — 2026-08-20

### Added

- **One-click Windows install.** The installer now stages the payload and hands off to Sony's own
  `SoftwareUpdateTool.exe`, which performs the USB handoff and reboots the player into Cinder. No
  WSL, `usbipd`, or manual SCSI command.

### Changed

- Release workflow reworked: releases publish directly rather than as drafts, with `SHA256SUMS`
  attached.

## [0.1.2] — 2026-08-20

### Added

- **On-screen keyboard and playlist picker** — this device has no d-pad, so text entry is touch.
- **Liked songs**, with import and a TSV export that crosses to the PC
  ([`docs/LIKES_SYNC.md`](docs/LIKES_SYNC.md)).
- SD-card playlist and likes sync ([`docs/PLAYLISTS.md`](docs/PLAYLISTS.md)).
- Bluetooth disconnect handling, a Bluetooth socket observer, an HCI snoop-log decoder, and a
  self-test for the edge logic.
- Battery-consumption measurement tooling for Bluetooth playback (`tools/btpower*`).

### Changed

- Display backlight control and font fallback both reworked.
- Database rescans paced rather than run per poll.

### Fixed

- Playlist refresh produced duplicates and sorted wrongly.

## [0.1.1] — 2026-08-18

### Added

- **`cinder-fm`** — setuid-root helper giving userspace FM tuner register access via
  `/proc/regmon/Si4708icx`.
- **`cinder-voltable`** — volume-table installation helper.

## [0.1.0] — 2026-08-18

First tagged release.

### Added

- **FM tuner** support.
- **`cinder-clock`** — sets system time and the RTC. The device has no Sony service that sets the
  clock, and `cinder-home` runs as uid 100, so this is a setuid helper doing `settimeofday` plus the
  RTC ioctl.
- **A/B sound setups** — two complete saved sound configurations.
- Advanced sound controls.
- Device test documentation ([`docs/DEVICE_TESTS.md`](docs/DEVICE_TESTS.md)).

### Documented

- The wired-headphone volume-change pop: 26 pops below volume 100 against 1 above, and it is not
  the shell or any mixer control ([`docs/`](docs/)).

[Unreleased]: https://github.com/superwilso/Cinder/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/superwilso/Cinder/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/superwilso/Cinder/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/superwilso/Cinder/compare/v0.1.5...v0.1.7
[0.1.5]: https://github.com/superwilso/Cinder/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/superwilso/Cinder/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/superwilso/Cinder/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/superwilso/Cinder/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/superwilso/Cinder/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/superwilso/Cinder/releases/tag/v0.1.0
