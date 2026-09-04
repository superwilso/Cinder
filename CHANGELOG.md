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

### Fixed

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
