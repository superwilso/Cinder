# Next — the one list

**Every open next step, to-do and device check across Cinder, Flint, cinder-themes and
cinder-sony-analysis, as of 2026-10-18.** This file is the order, not the procedure. Device rows are
run from [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) (safety rules first) and recorded there;
feature state lives in [`../cinder-home/STATUS.md`](../cinder-home/STATUS.md). When something here is
done, delete its line and record the result where its procedure lives. The 2026-10-05 version of
this file (battery research in full, Walkman One parity) is in git history; its findings are in
[`../analysis/RE_sound_service.md`](../analysis/RE_sound_service.md),
[`COMPARE_cinder_sony.md`](COMPARE_cinder_sony.md) and
[`AUDIT_2026-10-18_sound_power.md`](AUDIT_2026-10-18_sound_power.md).

## Where things stand

| | |
|---|---|
| Cinder | **v0.3.16** is the Latest release (10-11). **v0.3.17-rc2** is the pre-release (10-18): audiobooks, presets, A/B as a compare, the DAC EQ on the jack, the NC probe, the in-place queue edit, Clear Bass in the installer |
| Flint | **v0.2.1** is the Latest release (09-28). **v0.2.3-rc1** is the pre-release (10-18): one Windows file, sync into `MUSIC`, MusicBee playlists, faster tagged copies, no recopy with SensMe off |
| Open issues | Cinder #27 (noise cancelling with IER-NW500N): reply posted 10-18, waiting for the reporter's `cinder_nc_probe.txt` |
| Gates | 835 player tests, the harness, 367 golden previews, the text audit, `cargo fmt --check` (also in pre-push since 10-18) |

---

## 1. Decisions only the owner can make

| # | Decision | Why it waits on you | Where |
|---|---|---|---|
| 1 | **Promote v0.3.17 and Flint 0.2.3 to stable**, or another rc first | Section 4 A's rows for what 0.3.17 changed are the evidence | `tools/release.sh v0.3.17`, Flint's tag |
| 2 | **Flint F1, the rest:** sweep only what Flint wrote | The `MUSIC` half was fixed 10-09; a sync still removes files it did not put there | Flint `docs/AUDIT_2026-10-01.md` Part F |
| 3 | **The history rewrite.** Sony's extracted files are out of the tree but in public history | Rehearsed (122.7 → 35.2 MB, every release payload unchanged). A force-push that rewrites every clone | [`HISTORY_REWRITE.md`](HISTORY_REWRITE.md) |
| 4 | **The cable escape** (11.4): keep, narrow to a PC data connection, or remove | Needs 11.3 (five quiet boots) first | `DEVICE_CHECKLIST.md` 11.3–11.4 |
| 5 | **Suspend to RAM as the default** when paused off the cable | USB after a resume (33.2) is partial; dev adb does not come back | `DEVICE_CHECKLIST.md` §33 |
| 6 | **Bluetooth auto off on by default.** The largest measured battery lever (radio on, jack playing: 0 deep idle entries in 3 min against 291) | You asked to leave Bluetooth as it is (10-18); a recommendation only | [`AUDIT_2026-10-18_sound_power.md`](AUDIT_2026-10-18_sound_power.md) §1 |
| 7 | **Walkman One for strangers:** ship an `nw-wm1a`-sealed package and have the installer pick it | The release embeds only the stock-sealed `.UPG`, which a W1 player drops without a word | `DEVICE_CHECKLIST.md` 18.2–18.6 |
| 8 | `enforce_admins` on `main`; code signing (free open-source programme, paid, or unsigned) | Settings and money | `PLAN_2026-09-14.md` E5 |
| 9 | Publish cinder-sony-analysis's **`wm1a-volume-tables`** draft (still a draft; `install.md` links to it) | Read its wording first | cinder-sony-analysis releases |

## 2. Offline work, in order (no player needed)

**Cinder — sound and battery** ([`AUDIT_2026-10-18_sound_power.md`](AUDIT_2026-10-18_sound_power.md) §3)

1. **Say what costs battery.** Each software effect's row says when it keeps Sony's chain running;
   the mode line says whether the chain runs or is skipped. UI only, about an hour.
2. **Presets, round two:** names (needs the keyboard Search uses) and more than four (the screen
   would scroll).
3. **A Battery preset**: one tap to Direct, or Custom with only the DAC EQ.
4. The redesign plan's Part G still describes per-output profiles (2a, 2g); bring it in line with
   presets.

**Cinder — everything else**

5. **The redesign's remaining phases** ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md)
   Part F): R3's Library view bar, Search scopes, the sideways page swipe; R4's saved views (*show
   as*, *pin*); R7 the installer's window.
6. **Streamlining, the last step:** ship `cinder_advanced.conf.example` to the player from the
   installer and name it on Help.
7. Search results for albums and artists; Play next on long-press as well as the swipe.
8. **G1.7 contract fixtures, the rest:** the palette corpus, a `#TZ/UTC` log, playlist forms, a
   SensMe chunk (`tools/check_contracts.sh`).
9. **Tests for `cinder-audio`** (`SHORTCOMINGS.md` A2): grow `fake_pst.cpp` a service at a time.
10. **2038 (goal 10):** an i64 audit of Cinder's own timestamps.
11. A host benchmark with a 10,000-line playlist; discovering an **unpaired** Bluetooth device; a
    Windows installer smoke test per release (Install → Update → Uninstall → Install).

**Flint**

1. After decision 2: adopt music already on the player by content key (G3.2).
2. E7 the album unit is the top-level folder; E9 refused scrobbles to `.scrobbler.refused.log`;
   E10 the 25 s lossless test; E11 one WinHTTP session for the palette shop.
3. Features in the 10-01 order: G1.1 backup of the player's state, G1.2 health card, G3.1 library
   lint, G1.3 device-aware transfer, G2.1 per-headphone profiles, G1.4 LRCLIB lyrics, G2.2 resume.
4. R6 / Flint 0.3: Setup, convert-on-transfer, Copying, the palette preview, SensMe reasons, the
   SensMe MP3 path.

**cinder-themes:** nothing open. **cinder-sony-analysis:** decision 9.

## 3. Research with no deadline

* **Noise cancelling (#27):** Sony's rule is read (`NcServiceImpl::MakeValidity`): a 5-pin plug,
  an NC model, the single-ended jack, the driver. Next is the reporter's probe file; then, on stock,
  driving `NcAsmService::SetNcSetting` from a Cinder row shown only when the earphones are in.
  Walkman One's library reports no NC (`IsFeaturedNoiseCancel` = 0, a stub service).
* **What each effect costs on its own** (section 4 E): DSEE HX, the normalizer, Vinyl, Clear Bass.
* What Sony's six-band waits for (`no desired value, skip`) on stock — 16.5 / 17.6.
* DSEE AI (2B.3) and the SensMe channel id → name table (14.2).
* Two 10 Hz pollers in Sony's services (KeyService `select()`, DisplayService's LED thread): 20 of
  ~36 wakeups a second at idle. Measure off the cable before patching either.
* Other models (ZX300, WM1A/Z, A40, A30): start from antiheroriot's ZX300 branch as a PR.

## 4. Device checks, grouped by what each session needs

Row numbers are `DEVICE_CHECKLIST.md`'s.

| Session | Rows | Setup |
|---|---|---|
| **A. New in 0.3.17** — first | §37 audiobooks; Sound ▸ Presets (save, load, delete); Sound ▸ Equalizer on the jack opens the DAC EQ, Fit my 10-band curve, the jack switch; the DAC EQ heard at the jack (PLAN_sound §3); 38.1 the queue on Bluetooth and with shuffle | none |
| **B. Daily use** — mark as noticed | 19.x–30.x redesign rows, 11.7, 11.12, 13.4, 13.7, 13.8, 15.1–15.5 | none |
| **C. Bluetooth** | 2A.1–2A.5, 2C.1–2C.3, 5.2, 5.3, 21.10, 26.x, 28.2, 28.6, 31.5, 31.6, 32.3, 36.1, 36.2; 38.4 auto off on the jack | headphones |
| **D. The jack rig** | 2B.2–2B.4, 4d, 4e, 13.6, 13.9–13.11, 21.1–21.6, 27.x, 28.x, 32.1, 32.2 | `RE_headphone_amp_modes.md`'s rig |
| **E. Power** | 5.5 a discharge number; one run each way for the DAC-EQ default; per-effect `tools/ssfw_cost.sh` (changes what you hear — not while listening); §33, §34, 36.3–36.5 | `tools/battery_track.sh` |
| **F. Boot and recovery** | 11.3 → decision 4, 11.8, 35.2–35.5 | cable out, current `wbrt` backup |
| **G. SensMe** | 14.1–14.3 | files tagged by Flint and by Music Center |
| **H. Walkman One** | 18.2–18.6 (re-check against the 10-01 "all installers work" report) | the W1 player |
