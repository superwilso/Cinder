# Next — the one list

**Every open next step, to-do and device check across Cinder, Flint, cinder-themes and
cinder-sony-analysis, as of 2026-10-05.** It replaces the lists that each called themselves the
next-steps list: [`AUDIT_2026-10-01.md`](AUDIT_2026-10-01.md) Part H,
[`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) Part E, [`PLAN_community_2026-09-23.md`](PLAN_community_2026-09-23.md)'s
suggested order, [`AUDIT_2026-10-05_ponytail.md`](AUDIT_2026-10-05_ponytail.md) Part G and Flint's
`docs/AUDIT_2026-10-01.md` "Next". Those stay as the record of why.

**How to use it.** This file is the order, not the procedure. Device rows are run from
[`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) (safety rules first) and recorded there; feature state
lives in [`../cinder-home/STATUS.md`](../cinder-home/STATUS.md). When something here is done, delete
its line and record the result where its procedure lives.

---

## 1. Decisions only the owner can make

| # | Decision | Why it waits on you | Where |
|---|---|---|---|
| 1 | **Flint F1: what a sync owns** — the root it mirrors into and what it may sweep. Recommended: `MUSIC/`, and sweep only what Flint wrote | **High.** On a player whose music is already in `MUSIC/`, the first Copy plans to delete all of it. Since 2026-10-05 Copy only carries out a plan that was shown (E2), so the removals are listed first, but the default is still wrong | Flint `docs/AUDIT_2026-10-01.md` Part F |
| 2 | **The history rewrite.** Sony's extracted UI files and decompilation are gone from the tree but still in public history | Prepared and rehearsed (122.7 → 35.2 MB packed, tip byte-identical, every release tag's payload unchanged). A force-push that rewrites every clone: irreversible | [`HISTORY_REWRITE.md`](HISTORY_REWRITE.md), `tools/rewrite_history.sh` |
| 3 | **The cable escape** (checklist 11.4): keep rung 0 as it is, narrow it to a PC data connection, or remove it | Needs 11.3 (five quiet boots) first | `DEVICE_CHECKLIST.md` 11.3–11.4 |
| 4 | **Suspend to RAM as the default** when paused and off the cable — Sony's own idle state | Every interlock is built (USB, charger, any audible source, Bluetooth pending, receiver, MSC); needs §33 run | `DEVICE_CHECKLIST.md` §33 |
| 5 | **The release rule:** stable means its checklist rows have passed, and `release.sh` prints the open rows for the CHANGELOG entries it ships | A policy, then a small script change | `AUDIT_2026-10-01.md` C1 |
| 6 | **Walkman One for strangers:** ship an `nw-wm1a`-sealed package in releases, and have the installer pick it | The owner's player runs Cinder on Walkman One, but the release installer embeds only the stock-sealed `.UPG`, which a W1 player drops without a word (it warns instead) | `DEVICE_CHECKLIST.md` 18.2–18.6 |
| 7 | **Cinder One:** close it. Everything audible Walkman One does is already a Cinder component on stock firmware (§6 below); what the spec has left is W1-settings rows and the analyser delay tables | Recommendation, not a measurement | [`SPEC_cinder_one.md`](SPEC_cinder_one.md), [`VISION_four_builds.md`](VISION_four_builds.md) §7 |
| 8 | `enforce_admins` on `main`; code signing (a free open-source programme, a paid certificate, or stay unsigned) | Settings and money | `PLAN_2026-09-14.md` E5, `SHORTCOMINGS.md` D7 |
| 9 | Publish cinder-sony-analysis's **`wm1a-volume-tables`** draft release (read its wording first) | Still a draft on 2026-10-05; `install.md` links to it | cinder-sony-analysis releases |

## 2. Releases (the owner's machine: the ARM toolchain is not on a runner)

* **Cinder v0.3.15** — `tools/release.sh v0.3.15`. Ships what 2026-10-05 changed in source:
  `cinder-probe` leaves the stable payload (4.77 MB, 48 % of it), the Home app stops linking
  `libMali_linux.so`, `cinder-gpunode` is removed from players that had it. After the release
  commit: drop `libMali_linux.so` from `tools/check_arm_payload.sh`'s allowed list and the
  `cinder-probe` "when present" line, and make `cinder-hagowrap`, `cinder-guard.sh` and
  `cinder-preload.sh` `required` in `installer/build.rs` (its comment says so).
* **Flint 0.2.2** — the F2 scrobble-time fix changes every scrobble's time; also E1, E3–E6, the one
  sync core and E2. Before sending a backlog, check one play's time on Last.fm against the player's
  clock.

## 3. Offline work, in order (no player needed)

**Cinder**

1. **G1.7 contract fixtures, the rest.** `contracts/` now holds the likes key vectors and a
   scrobble log, byte-identical in both repositories, each read by that side's tests;
   `tools/check_contracts.sh` compares the copies (run it before a release of either). Still to
   add: the palette corpus (F3), a `#TZ/UTC` log, playlist forms (F5), a SensMe chunk (F6).
2. **`/data` headroom, the visible half:** `tools/cinder-install.sh` now refuses an install that
   would leave under 1 MB (2026-10-05); G1.2's status card makes the number visible from the PC.
3. **Tests for `cinder-audio`** (`SHORTCOMINGS.md` A2): grow `fake_pst.cpp` a service at a time,
   BtTransmitter first.
4. **The redesign's remaining phases** ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md)
   Part F): R3's Library view bar, Search scopes and the sideways page swipe; then R4's saved views
   (*show as*, *pin*, opening from the bar); R7 the installer's window.
5. Search results for albums and artists; offer Play next on long-press as well as the swipe.
6. **2038 (goal 10):** an i64 audit of Cinder's own timestamps (scrobbler, likes, playlists,
    library).
7. A host benchmark with a 10,000-line playlist before promising big playlists.
8. Discovering an **unpaired** Bluetooth device (the Devices screen's one missing part).
9. A Windows installer smoke test per release (Install → Update → Uninstall → Install, log kept).

**Flint**

1. After F1: G3.2 adopt music already on the player by content key — F1's migration path.
2. E7 the album unit is the top-level folder; E9 refused scrobbles to `.scrobbler.refused.log`;
   `flint-core`'s lossless test takes 25 s in debug (E10); one WinHTTP session for the palette shop
   (E11). E2, E8, the CI double run and `lastfm.conf`'s mode were fixed 2026-10-05.
3. Features, in the 10-01 order: G1.1 backup of the player's state, G1.2 health card, G3.1 library
   lint, G1.3 device-aware transfer, G2.1 per-headphone profiles, G1.4 LRCLIB lyrics, G2.2 resume.
4. R6 / Flint 0.3: Setup, convert-on-transfer, Copying, the palette preview and ticking, SensMe
   reasons; the SensMe MP3 path.

**Streamlining (the owner's direction, 2026-10-05: one Cinder, short by default, the power kept)**

1. *Done 2026-10-05, on the owner's player:* Settings opens as ten everyday rows and **More
   settings** unfolds the rest; nine empty-file switches are also lines in one
   `cinder_advanced.conf` (`cinder-home/deploy/cinder_advanced.conf.example`).
2. Next: the three value files (`cinder_suspend_s`, `cinder_cpufloor.conf`,
   `cinder_mediastore.conf`) as keys of the same file; the same fold for Sound (Advanced already is
   one) and Bluetooth; remember the fold across restarts if it is opened every time.
3. Not started: ship `cinder_advanced.conf.example` to the player's storage from the installer, and
   name it on Help.

**cinder-themes, cinder-sony-analysis:** nothing open beyond decision 9.

## 4. Device checks, grouped by what each session needs

Row numbers are `DEVICE_CHECKLIST.md`'s. Since 2026-10-01 most of 0.3.13/0.3.14 is in daily use
and reported working as a whole; the rows below are the ones that still need a line each.

| Session | Rows | Setup |
|---|---|---|
| **A. Daily use** — mark as noticed | 19.x (redesign screens), 20.x, 22.x, 23.x, 24.x, 25.x (R4), 26.1, 26.2, 26.8, 30.x, 11.7, 11.12, 13.4, 13.7, 13.8, 15.1–15.5, 2D.3, 2D.5–2D.7, 29.x | none |
| **B. Bluetooth** | 2A.1–2A.5, 2C.1–2C.3, 5.2, 5.3, 21.10, 26.4, 26.5, 26.9, 28.2, 28.6, 31.5, 31.6, 32.3, 36.1, 36.2 | headphones (WH-1000XM4 for NFC) |
| **C. The jack rig** — line in, nothing on a head | 2B.2–2B.4, 4d, 4e, 5.1, 5.4, 13.6, 13.9, 13.10, 13.11, 21.1–21.6, 26.6, 26.7, 27.x, 28.1, 28.3, 28.5, 28.7, 28.8, 32.1, 32.2 | `RE_headphone_amp_modes.md`'s rig |
| **D. Boot and recovery** | 11.3 → decision 3, 11.8, 35.2–35.5 (the `preload` trial) | cable out, wbrt backup current |
| **E. Power — a day off the cable** | 5.5 a discharge number, 4b soak, 4c against stock, 13.1, 14.4–14.6, 2C.4, 31.3, 31.4, 33.1–33.5, 34.1–34.4, 36.3–36.5 | `tools/battery_track.sh`, `tools/btpower.sh` |
| **F. SensMe** | 14.1–14.3 | a few files tagged by Flint and by Music Center |
| **G. Walkman One** | 16.5 / 17.6 Clear Bass with the stock UI as reference, 17.7 read `clv`, 18.2–18.6 (re-check against the 10-01 "all installers work" report, then strike what it covers) | the W1 player |
| **H. Probe** (dev channel) | 0e the MediaStore rescan trigger (`strace` across an MSC disconnect), 13.5 the 1 TB card reporter's log | adb |

## 5. Battery — where the rest of the saving is

The offline levers are all in: stage 1 early suspend by default (also under Bluetooth since
2026-10-04), the cable-pull step-out, codec standby, the 1040 MHz cap and kernel-default scheduler
slices behind a dark panel on the jack, the 500 ms dark pump, `poll()` sleeps to the housekeeping
deadline, Bluetooth auto off and the quiet route poll, NFC stopped behind a dark panel, fork-free
volume and (2026-10-05) balance writes, the batched library queries, and the library rebuild only
when its content moves. What is left needs the player:

1. **A number** (5.5). Nothing above has a denominator without one; there is no fuel gauge.
2. **Suspend to RAM when paused off the cable** (decision 4, §33) — the largest lever left.
3. **The CPU cap under Bluetooth.** Stage 1 runs there now; the 1040 MHz cap does not, because LDAC
   encode at 1040 MHz is unmeasured. Measure, then extend `cpu_tune("cap")` to the BT route.
4. **Sony's `icx_syslog`.** Stock starts it (`-n 32 -l 6 -d /emmc@var`, eMMC writes); Walkman One
   never does. Measure its wakeups idle; if they show, stop it behind a dark panel — it is also what
   records appmgr's side of a failure, so not outright.
5. **The codec's own `deep early suspend` control** in place of the 30 s standby timer — the same
   state, with the stream check in the driver (`RE_sony_idle_baseline.md`). A simplification once
   36.3 has run.
6. **What vetoes deep idle under a stream** — `dpidle_block_cnt[by_oth]`, the `wm_key.ko` callback
   (`RE_sony_idle_baseline.md` §2). Research.
7. `VCAMD` 1.2 V and `VCAM_AF` 3.3 V are on with no camera on the board: find what they feed.
8. 09-05 B12, the lit-idle frame rate (~60 wakeups/s while the panel is lit and nothing moves).
   Small next to the backlight; only with a measurement behind it.

## 6. Walkman One — is it all reverse-engineered, and does stock need it?

**Yes, and no.** Every file Walkman One changes is identified and read
([`../analysis/RE_walkmanone_extract.md`](../analysis/RE_walkmanone_extract.md),
[`../analysis/RE_walkmanone_installers.md`](../analysis/RE_walkmanone_installers.md)): the model swap
(three NVP images), the 1,452-line boot script, the eight settings plus the undocumented `ADB`, the
3-byte HAL patch, the gain and analyser tables, the init deltas, the UI patch, and the external
tunings (opened 2026-09-29: an NVRAM checksum token and another model's bootloader, neither audible
on an A50). Wampy's jack measurements found three things that change the signal: two regions'
tables and gain mode 1. All three, and the rest, are reachable on stock firmware without installing
Walkman One:

| Walkman One | On stock, with Cinder |
|---|---|
| Region cap lifted (`REG`, CEW2/KR3 tables) | `voltable=stock`, the default; `region` keeps Sony's cap |
| Gain mode 1 = the NW-WM1A curve (`GMD=1`) | `voltable=wm1a` (bring `ov_127x.tbl`: Sony's file, not shippable) |
| Plus modes / signatures (`PMV`, `PMD`, `SIG` HAL) | `signature=pv1`/`pv2` (and `clock`, `hw1`, `hw2`, which split its two effects) |
| The WM1A tone table | `cinder-voltable tone-wm1a`; differs only for Sony's NC headphones |
| DAC init mode (`DIM`) | nothing audible |
| Icon colour (`COL`) | Cinder's palettes |
| RMT-NWS20 remote (`REM`) | Cinder's Bluetooth screens |
| `ADB` in `settings.txt` | the dev channel |
| FM (dead under W1's mock tuner library) | Cinder drives the Si4708 directly |
| The model swap itself | not wanted: it is what hides FM, VPT, ClearAudio+, Language Study and line-out in Sony's app |

**Walkman One stays supported** (owner, 2026-10-05: people on it can stay). Its "external tuning"
is a gate, not a sound: the boot script compares `md5(mmcblk0p3)` with a constant and, when they
differ, ignores `PMD`/`PMV`/`GMD`/`DIM`/`COL` and reloads `normal_nt` every boot. Since 2026-10-05
`cinder-signature.sh` writes its verified library to that `normal_nt` copy as well, so the
installer's signature choice survives W1's boot without flashing another player's NVRAM and
bootloader (`tools/test_signature.sh`, 16 cases; **not yet applied on a player** — it changes the
audio library the next boot loads). The gain half was already covered (`voltable=wm1a` finds
`gain_l` by content). The tuning itself is an installer option, off by default (`w1tuning`, 2026-10-05): it unpacks
W1's own package in the updater, checks the image, saves the player's NVRAM to
`/contents/cinder_nvram_backup.img`, writes `p3` only and reads it back
(`tools/test_w1tuning.sh`, 17 cases; **never run on a player** — first run belongs with a current
`wbrt` backup). Left: the analyser parameter set that goes with each signature
(`.mod/anls/<mode>`), and naming W1's mode and settings in Cinder's own screens.

## 7. Research with no deadline

* **Clear Bass:** what Sony's six-band waits for (`no desired value, skip`) — 16.5 / 17.6.
* **DSEE AI** (2B.3) and the SensMe channel id → name table (14.2).
* After ClearAudio+ and the normalizer the stream leaves the HAL as `S16_LE`: does Sony dither, and
  can the HAL open 24/32-bit for processed audio? (`AUDIT_2026-09-28_power_sound_w1.md` Part C.)
* Other models (ZX300, WM1A/Z, A40, A30): start from antiheroriot's ZX300 branch as a PR.
* Low: MT8590 BROM SLA/DAA applicability; the DMP-Z1's SoC.
