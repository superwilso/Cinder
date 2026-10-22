# Next — the one list

**Every open next step, to-do and device check across Cinder, Flint, cinder-themes and
cinder-sony-analysis, as of 2026-10-22.** This file is the order, not the procedure. Device rows are
run from [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) (safety rules first) and recorded there;
feature state lives in [`../cinder-home/STATUS.md`](../cinder-home/STATUS.md). When something here is
done, delete its line and record the result where its procedure lives. Earlier versions of this
file (battery research in full, Walkman One parity, the 0.3.17 release plan) are in git history.

## Where things stand

| | |
|---|---|
| Cinder | **v0.3.19** is the Latest release (10-20): the PolyForm Noncommercial licence, the installer writing the package Walkman One expects, Bluetooth AAC and SBC XQ as installer options, Simplified Chinese and installer language choice. 0.3.17 (audiobooks, presets, NC with Sony's NC earphones) and 0.3.18 (SD-card playlists) shipped before it. **On `main`, not yet released:** #30, the receiver's AAC/LDAC negotiation and codec choice (labels from #32); #31, track previews (#29 by @yin8ruo) as the opt-in `preview` installer option with the playback hand-off fixed. Neither has run on a device in its final form |
| Flint | **v0.2.1** is the Latest release (09-28). **v0.2.3-rc1** is the pre-release (10-18) |
| Open issues | None on GitHub. **Reported on Head-Fi, not on GitHub (10-12):** after two weeks unused, one user's library came back empty on internal and SD (0.3.9 and 0.3.16) and they went back to stock — worth an issue and a boot log |
| Open PRs | **#29**, left open for the author's answer: its code is on `main` through #31. Device run for #31: End → Play, leaving FM mid-preview, a second preview |
| Gates | The player and installer tests, the harness, the golden previews and text audit, `cargo fmt --check` (also in pre-push), all run by CI on every PR |

## Reviewing a contributed PR

What the reviews of #24, #29 and #30 came down to. Check these before merging:

1. **New features are opt-in** while they are new: an installer option (`components.conf`, a flag
   in `/data/cinder`), off by default. With it off, the golden previews must not change.
2. **Shared playback code is unchanged for people who do not use the feature.** Diff `main.cpp`'s
   play, resume and queue paths against `main`; a new feature's behaviour belongs behind its own
   state, not in the common path.
3. **Sony IPC slots match the firmware.** A new `bt_slot`/vtable index is checked against the
   class's vtable in the stock library (`readelf -rW` on the `.so` in `artifacts/rootfs_mnt`).
4. **The usual gates pass**, and new strings have translations that draw from the bundled fonts
   (`every_translation_draws_from_the_bundled_fonts`).

`tools/cloud_setup.sh` builds the device toolchain and unpacks Sony's firmware in a fresh Linux
container, if a PR needs an ARM build to check.

---

## 1. Decisions only the owner can make

| # | Decision | Why it waits on you | Where |
|---|---|---|---|
| 1 | **Flint F1, the rest:** sweep only what Flint wrote | The `MUSIC` half was fixed 10-09; a sync still removes files it did not put there. The one open item that can lose a user's files | Flint `docs/AUDIT_2026-10-01.md` Part F |
| 2 | **Promote Flint 0.2.3 to stable**, or leave 0.2.1 as the Latest | Its rc has been out since 10-18 | Flint's tag |
| 3 | **The history rewrite.** Sony's extracted files are out of the tree but in public history | Rehearsed (122.7 → 35.2 MB, every release payload unchanged). A force-push that rewrites every clone; easier the fewer forks there are | [`HISTORY_REWRITE.md`](HISTORY_REWRITE.md) |
| 4 | **The cable escape** (11.4): keep, narrow to a PC data connection, or remove | Needs 11.3 (five quiet boots) first | `DEVICE_CHECKLIST.md` 11.3–11.4 |
| 5 | **Suspend to RAM as the default** when paused off the cable | USB after a resume (33.2) is partial; dev adb does not come back | `DEVICE_CHECKLIST.md` §33 |
| 6 | **Bluetooth auto off on by default.** The largest measured battery lever | You asked to leave Bluetooth as it is (10-18); a recommendation only | [`AUDIT_2026-10-18_sound_power.md`](AUDIT_2026-10-18_sound_power.md) §1 |
| 7 | `enforce_admins` on `main`; code signing (free open-source programme, paid, or unsigned) | Settings and money | `PLAN_2026-09-14.md` E5 |
| 8 | Publish cinder-sony-analysis's **`wm1a-volume-tables`** draft, or delete it | Still a draft; `install.md` links to it | cinder-sony-analysis releases |

## 2. Offline work, optional (no player needed)

Nothing below is needed for what has shipped to keep working. It is what was planned and not
built, in the order it was planned.

**Cinder — sound and battery** ([`AUDIT_2026-10-18_sound_power.md`](AUDIT_2026-10-18_sound_power.md) §3)

1. **Say what costs battery.** Each software effect's row says when it keeps Sony's chain running;
   the mode line says whether the chain runs or is skipped. UI only, about an hour.
2. **Presets, round two:** names (needs the keyboard Search uses) and more than four.
3. **A Battery preset**: one tap to Direct, or Custom with only the DAC EQ.
4. The redesign plan's Part G still describes per-output profiles (2a, 2g); bring it in line with
   presets.

**Cinder — standing alone from Wampy and Walkman One** (owner's direction, 10-22: no feature lost)

No Wampy code is in the tree and nothing is downloaded from either project. Licences: Wampy is
GPLv3 + Commons Clause and Walkman One is closed, so rebuild from documented facts, never copy, and
keep anything GPL in its own repo. The design for Walkman One's gains on a stock player is
[`SPEC_cinder_one.md`](SPEC_cinder_one.md): take what W1 does, from Sony's own files, without the
model swap that causes W1's losses.

1. ~~**Volume curves from Sony.**~~ **Done 10-22:** `tools/sony_tables.sh` takes the `wm1a`/`w1`
   tables from Sony's NW-WM1 3.02 firmware (`fw/pc/WM1/J/NW-WM1_V3_02.dmg`); all three match the
   installer's SHA-256s. **Also done 10-22:** the PC installer fetches them itself when Smooth or
   Walkman One is chosen (`installer/src/sony_tables.rs`): all three tables sit in 36 KB of Sony's
   image, so it is one HTTP Range request, AES-128-CBC, 88 zlib chunks and a SHA-256 check, no DMG
   or ext4 reader. Live test: `cargo test -- --ignored sony`. Needs a Windows run of a real install.
2. **Mono without Wampy by default.** Already built: the `preload` component (hagodaemon wrapper +
   boot guard). It is off by default because the wrapper stands in front of every Sony service;
   turning it on by default needs §35 on players other than the owner's. Then the Wampy-chain
   `mono` component can be retired.
3. **Cinder One, tiers 0–1, for stock players** (`SPEC_cinder_one.md` §3): nothing audible left.
   Sound signature (`signature`), gain mode (`voltable=wm1a`, now fetched from Sony), tone table and
   the 10-band/Tone chain are built. What remains measured the same at the jack (Wampy's REW runs):
   W1's "full DAC reprogramming" (`dacdat auto BBDMP2_linux`) and its per-mode analyser params,
   which only re-time Sony's spectrum display. Build either only if someone asks.
4. **Walkman One's external tunings: nothing audible to remake** (10-22). On an A50 a tuning is an
   NVRAM image whose only job is to match the md5 W1's boot script checks, plus another model's
   bootloader that the stock kernel never reads (`analysis/RE_walkmanone_installers.md`). What it
   unlocks is W1's own settings, and Cinder sets each of those directly: Plus mode (`signature`),
   gain mode (`voltable=wm1a`, from Sony's file), region (`voltable=stock`), tone table, colour
   (themes). `DIM=1` only skips the NC set-up, and the limiter files carry no region difference.
   **The one audible thing W1 had and Cinder lacked** was its WM1-class sound library's chain, with
   the 10-band EQ and Tone Control. Done 10-22: `libcinder_mono.so` hooks `FilterChain::Create` and
   adds `eq10band eqtone` after `eq6band` on a stock list (`filter_chain.h`, host + qemu tests in
   `test_mono_shim.sh`). It rides the `preload` component, so step 2 decides whether it reaches
   everyone. **Device test:** stock player, `preload` on, Bluetooth headphones, a +10 dB band on
   the EQ must be audible and `cinder_mono.log` must say "added eq10band and eqtone". Applying the
   WM1Z tuning on a W1 player by hand: `z2.bin` from W1's package (md5 `ccb29dd2…`) to
   `mmcblk0p3`, backup first; never `p7`.
5. **The folder name.** Cinder installs into `/system/vendor/unknown321/`, Wampy's folder. Wampy's
   uninstaller removes only its own files (checked 10-22), so this is a name, not a dependency.
   Moving to `/system/vendor/cinder/` touches ~110 lines including the launcher's path: do it on
   its own, with a migration and a device test, or not at all.

**Installer — finish an install from a Mac** (owner, 10-22; Montree on Head-Fi asked for it)

Today the macOS build stages every file and stops: the last step, the vendor command
`fc 00 04 'dbmn'` that reboots the player into its updater (`stage::FW_UPGRADE_CDB`), needs raw SCSI,
which macOS gives only to a kernel extension. Sony's own Mac updater ships one
(`SONYWalkmanType00.kext`, x86-64 only, matching `IOSCSIPeripheralDeviceNub` type 0), so it does
not load on Apple Silicon. No release ships a macOS installer at all.

1. **Send the command as raw USB mass storage.** In `stage.rs`, the macOS `trigger_fw_upgrade`
   (and `read_kas`): `diskutil unmountDisk` the player (keep it attached); open it with `rusb`
   (vendored libusb ≥ 1.0.25, Mac target only); find the interface class 8 / subclass 6 /
   protocol 0x50; `detach_kernel_driver` (needs root); write a 31-byte CBW (`USBC`, a tag, the
   transfer length, flags 0x80, LUN 0, CDB length 12, the CDB) to the bulk-OUT endpoint; read the
   data phase and the 13-byte CSW from bulk-IN. The player reboots, so no re-attach is needed.
   Keep `print_trigger_failed`'s macOS text for the "not root" and "capture refused" cases.
2. **Test the framing without a Mac:** a unit test that builds the CBW for `FW_UPGRADE_CDB` and
   `KAS_CDB` and checks it byte by byte, and one that parses a CSW (status 0, 1, 2).
3. **Release it:** a `macos-latest` job in `release.yml` (arm64 + x86-64, `lipo` into one
   binary), unsigned; `install.md` says to run it from Terminal with `sudo` after
   `xattr -d com.apple.quarantine`.
4. **Device proof, on a real Mac:** install and uninstall, stock and Walkman One. Until this
   passes, the release notes call the Mac build experimental. Risk to check first: whether
   current macOS still lets root detach the mass-storage driver (libusb's macOS "capture"); if it
   does not, the fallback is a Linux VM (UTM + USB passthrough) running the Linux installer.

About half a day of code plus the Mac session.

**Cinder — everything else**

5. **The redesign's remaining phases** ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md)
   Part F): R3's Library view bar, Search scopes, the sideways page swipe; R4's saved views (*show
   as*, *pin*); R7 the installer's window.
6. ~~Ship `cinder_advanced.conf.example` to the player from the installer and name it on Help.~~
   **Done 10-22** (Help ▸ ADVANCED; the example now lists all 17 switches the code reads).
7. ~~Search results for albums and artists, and audiobooks in Search; Play next on long-press as
   well as the swipe.~~ **Done 10-22.** Long-press is opt-in (owner): `hold_play_next` in
   `cinder_advanced.conf`. Device check in `TODO_device.md`.
8. **G1.7 contract fixtures, the rest:** the palette corpus, a `#TZ/UTC` log, playlist forms, a
   SensMe chunk (`tools/check_contracts.sh`).
9. **Tests for `cinder-audio`** (`SHORTCOMINGS.md` A2): effect, volume, power and codec shims have
   host self-tests (10-22). Left: player, tuner, analyzer, storage (vtable calls; grow
   `fake_pst.cpp` a service at a time).
10. **2038 (goal 10):** an i64 audit of Cinder's own timestamps.
11. A host benchmark with a 10,000-line playlist; discovering an **unpaired** Bluetooth device; a
    Windows installer smoke test per release (Install → Update → Uninstall → Install).

**Flint**

1. After decision 1: adopt music already on the player by content key (G3.2).
2. E7 the album unit is the top-level folder; E9 refused scrobbles to `.scrobbler.refused.log`;
   E10 the 25 s lossless test; E11 one WinHTTP session for the palette shop.
3. Features in the 10-01 order: G1.1 backup of the player's state, G1.2 health card, G3.1 library
   lint, G1.3 device-aware transfer, G2.1 per-headphone profiles, G1.4 LRCLIB lyrics, G2.2 resume.
4. R6 / Flint 0.3: Setup, convert-on-transfer, Copying, the palette preview, SensMe reasons, the
   SensMe MP3 path.

**cinder-themes:** nothing open. **cinder-sony-analysis:** decision 8.

## 3. Research with no deadline

* **Noise cancelling:** shipped in 0.3.17 for Sony's NC earphones. Left: the ambient filter
  Walkman One deleted, and whether the DAC EQ and the linear amp interact with NC.
* **What each effect costs on its own** (section 4 E): DSEE HX, the normalizer, Vinyl, Clear Bass.
* Whether Sony's receiver keeps `SetAAC`/`SetLDAC` across a restart (asked on #30): if it does,
  "SBC only" in Cinder carries into Sony's own receiver.
* What Sony's six-band waits for (`no desired value, skip`) on stock — 16.5 / 17.6.
* DSEE AI (2B.3) and the SensMe channel id → name table (14.2).
* Two 10 Hz pollers in Sony's services (KeyService `select()`, DisplayService's LED thread): 20 of
  ~36 wakeups a second at idle. Measure off the cable before patching either.
* Other models (ZX300, WM1A/Z, A40, A30): start from antiheroriot's ZX300 branch as a PR.

## 4. Device checks, grouped by what each session needs

Row numbers are `DEVICE_CHECKLIST.md`'s. Most of what shipped from 0.3.13 on is in daily use on the
owner's player and works as a whole; these are the rows nobody has logged one by one. If only one
session happens, make it A.

| Session | Rows | Setup |
|---|---|---|
| **A. New in 0.3.17 → 0.3.19** — first | §37 audiobooks; Sound ▸ Presets (save, load, delete); Sound ▸ Equalizer on the jack opens the DAC EQ, Fit my 10-band curve, the jack switch; the DAC EQ heard at the jack (PLAN_sound §3); 38.1 the queue on Bluetooth and with shuffle; 38.7 NC with an NC earphone; AAC and SBC XQ from the installer, and moving between AAC and aptX HD; the receiver's codec choice (#30) with an iPhone and an Android phone | NC earphones for 38.7 |
| **B. Daily use** — mark as noticed | 19.x–30.x redesign rows, 11.7, 11.12, 13.4, 13.7, 13.8, 15.1–15.5 | none |
| **C. Bluetooth** | 2A.1–2A.5, 2C.1–2C.3, 5.2, 5.3, 21.10, 26.x, 28.2, 28.6, 31.5, 31.6, 32.3, 36.1, 36.2; 38.4 auto off on the jack | headphones |
| **D. The jack rig** | 2B.2–2B.4, 4d, 4e, 13.6, 13.9–13.11, 21.1–21.6, 27.x, 28.x, 32.1, 32.2 | `RE_headphone_amp_modes.md`'s rig |
| **E. Power** | 5.5 a discharge number; one run each way for the DAC-EQ default; per-effect `tools/ssfw_cost.sh` (changes what you hear — not while listening); §33, §34, 36.3–36.5 | `tools/battery_track.sh` |
| **F. Boot and recovery** | 11.3 → decision 4, 11.8, 35.2–35.5 | cable out, current `wbrt` backup |
| **G. SensMe** | 14.1–14.3 | files tagged by Flint and by Music Center |
| **H. Walkman One** | 18.2–18.6, and 0.3.19's installer on a Walkman One player (it picks the package by the player's key) | the W1 player |
