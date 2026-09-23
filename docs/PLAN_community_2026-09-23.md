# Community reports and requests — 2026-09-23

**Sources read:**
* GitHub `superwilso/Cinder`: issues #14 (closed) and #16 (open), all comments.
* `superwilso/Sony-sync` and `superwilso/flint`: no issues filed.
* The r/walkman announcement thread: all 60 comments from 2026-09-12 to 2026-09-21, read through its
  RSS feed.

Every item below carries its evidence class. The bug fixes named here landed alongside
[`AUDIT_2026-09-23.md`](AUDIT_2026-09-23.md). Replies in the thread and on the issues are the
owner's to write; Part C lists the facts each reply needs.

---

## Part A — Bugs

| # | Report | Root cause | State |
|---|---|---|---|
| A1 | **#16** (miknios, v0.3.9, stock 1.02): "installing doesn't do anything", boots Sony's UI, no `cinderhome.log`. **-Fateless-** (thread, 09-21), the same after reverting from Walkman One: "the installer thinks Cinder is installed, but it doesn't boot into it" | v0.3.9 creates `/data/cinder` owned by root, so the uid-100 launcher cannot write its state and escapes to stock. v0.3.9 predates the breadcrumbs, so the escape writes no line. **Inferred from the code and the log shape;** it is the mechanism that was device-verified on W1 on 09-21 | **Fixed in 0.3.11-rc1** (`chown 100:100`). Only pre-releases carry it; GitHub still serves v0.3.9 as Latest (audit C1). **Needs a stable release** |
| A2 | **Installing over Walkman One does nothing** (-Fateless-, le_mangin and Miknios asking): "boots into the updater for a second, but reboots back into Mr. Walkman" | W1 rewrites the model identity. An A55 under W1 reports the NW-WM1A KAS, and the updater silently drops a package sealed `nw-a50`. **Device-verified 09-21** | Diagnosis done. The installer now **warns** when it sees W1's `CFW/` folder (audit B5). The real fix is a package sealed `nw-wm1a`. It builds (`pack_upg.sh <ch> nw-wm1a`) but has **never been installed** (DEVICE_CHECKLIST 18.1), and releases do not ship it (18.2) |
| A3 | **#14** (antiheroriot, 0.3.8): after an update the player boots Sony's UI | The cable pass was written to the updater's RAM disk (`/data` never mounted) | **Fixed in 0.3.9** and device-verified. Today's audit (B3) fixes a regression in that fix: a sentinel left by a live install made the next update skip the pass |
| A4 | A 1 TB SD card is not found and the scan "times out" (antiheroriot, 09-13) | A font-fallback crash on a glyph, plus SD handling | **Fixed in 0.3.4** (owner reply, 09-13). No report since |
| A5 | A car Bluetooth transmitter won't connect (No-Plan59, 09-17) | **Unknown: no data.** The MTK stack logs nothing by default (`reference_bt_mtk_transport`) | Needs the device name and `cinderhome.log`. The HCI snoop log (`reference_bt_hci_snoop`, slot 26) is the tool that would settle it; see B6 |
| A6 | antiheroriot has local fixes (ZX300 port, W1 boot after updates) and "does not have permission to push a branch" | GitHub gives outside contributors no push access to someone else's repo, by design | The route is **fork → branch → pull request**, and CONTRIBUTING.md says so. The ZX300 work is the most valuable outside contribution offered so far (B4) |

---

## Part B — Feature requests, ranked by payoff over effort

Already delivered, so each only needs pointing at:
* library search (0.3.4);
* a database scan that no longer blocks startup (a Settings ▸ Database button);
* a play queue and "play next" (Up Next);
* SensMe channels (0.3.9, install option `sensme`);
* the release year (album rows and Track information);
* detailed song information (tap the current track);
* accent colours (palettes, plus the stock-look `Sony` palette, unreleased);
* the Bluetooth quick icon Miknios liked.

### B1. A quick-settings pull-down, OPTIONAL and off by default — *asked by Uneekuza; also covers Michlob and Miknios*

Uneekuza: "a drop-down menu … like Shanling uses … brightness, Bluetooth, remote on/off". Michlob
wants the sleep timer without digging through Settings, and Miknios wants Bluetooth reachable
quickly.

**The owner's constraint (2026-09-23): it is opt-in, and the Shelf does not change.** The owner uses
the Shelf as it is. So the panel is its own overlay with its own gesture, enabled by a Settings row
(`Quick settings: Off/On`, persisted in `cinder_settings.conf`, **default Off**). With it off,
nothing about the app changes: no gesture, no pixels, and the golden previews are identical.

* **What:** with the setting on, a swipe down from the status bar opens a sheet of large toggles,
  then a sleep-timer row: brightness (the five levels), Bluetooth on/off, the BLE remote, night
  theme, and sleep timer (off/15/30/60).
* **Where:** a new `quick.rs` overlay. It may borrow the Shelf's dim-and-sheet *drawing helpers*,
  but not its file, state or hit test, so the Shelf's code and behaviour stay untouched. Every
  control maps onto an action that already exists (`Action::SleepTimer`, the brightness row, the
  Bluetooth radio switch), so there is no new FFI.
* **Risk:** the gesture must not fight list scrolling. Take it only when the drag starts inside the
  status bar (top 40 px), the same rule stock uses. The player has touch and transport keys only,
  with no d-pad (`feedback_nwa55_input_model`).
* **Tests to write first:**
  * with the setting Off, a status-bar drag does nothing and every existing golden hash is unchanged;
  * with it On, the drag opens the panel and a list drag below the bar still scrolls;
  * every Shelf test passes untouched.
* **Effort:** about 1½ days with host tests and golden previews, then one device check for the
  gesture.

### B2. A one-tap lyrics shortcut — *Miknios*

Lyrics are two taps deep today (Track information ▸ Lyrics). Add a Lyrics chip on Now Playing,
shown only when the track has lyrics (`.lrc`, or the tags the loader already reads). About half a
day. Miknios embeds lyrics in every file for exactly this, so it is a strong daily-use win for one
person at least.

### B3. Walkman One support — *le_mangin, Miknios, -Fateless-*

This is the 2026-09-21 parity goal ([`PLAN_walkman_one_parity.md`](PLAN_walkman_one_parity.md),
[`VISION_four_builds.md`](VISION_four_builds.md)), and the most-asked request in the thread. The
order:

1. **18.1:** install a `nw-wm1a`-sealed `.UPG` on the W1 player. It is the owner's device and one
   flash, with wbrt ready. Everything else waits on this answer.
2. **18.2:** `release.sh` packs both models.
3. The installer picks the package. It already detects `CFW/` (audit B5); the choice needs the
   player's KAS or a W1 marker that disappears on revert, and `boot_log.txt` freshness is the
   candidate.
4. **18.4:** uninstall on W1.

About one device session plus a day of packaging.

### B4. Other players: ZX300, WM1A/Z, A40, A30 — *antiheroriot (ZX300 working locally), le_mangin, Uneekuza, koistenshi*

Do not start this from scratch. **Get antiheroriot's branch in as a PR first.** They report the
ZX300 installing and running after "a few tweaks like disabling the FM tuner". The model-specific
parts are already known:

* the KAS used to seal the package;
* whether an FM chip exists;
* the volume table;
* the screen geometry.

Each should become a detected property instead of a constant. Effort depends on the size of their
diff. Supported status needs a device report per model, because the owner has only the A55.

### B5. Clear Bass+ from the ZX100 — *-Fateless-*

This is research, not a feature. Measured 09-17: band 0 of the A50's six-band EQ is Clear Bass, and
driving it through `EffectCtrlDmp` had **no audible effect**, while the ten-band control moved
+7.9 dB (`project_clear_bass_found`). What engages it has to be found first; the ZX100 tables and
decompilations are in `cinder-sony-analysis`. There is no estimate until that is known, and nothing
should be promised in the thread.

### B6. A Bluetooth debug log switch — *follows from A5*

Expose the HCI snoop log (slot 26 writes a btsnoop file to `/tmp`) as a Settings ▸ Bluetooth ▸
Debug log toggle that copies the file to the drive root. Every "won't connect" report after that
comes with evidence. About a day, most of it making sure the log is off by default and bounded in
size.

### B7. A theme marketplace — *owner's own reply to Illustrious_Maybe_41*

Covered by [`PLAN_skins.md`](PLAN_skins.md). Palettes are done; skins are next in that plan. No new
work item.

**Suggested order:** the stable release (A1) → B3 step 1 → B1 → B2 → B4 (when the PR arrives) →
B6 → B5.

---

## Part C — Facts for the replies (the owner's words go around these)

* **#16 and -Fateless-:** v0.3.9 cannot start after a fresh install. It is fixed in 0.3.11-rc1
  (and the next stable). Install that over the top; no uninstall is needed.
* **Walkman One:** the updater rejects the package without a word because W1 changes the player's
  firmware key. A package sealed for that key exists but is untested. Reverting to stock and then
  installing a build newer than v0.3.9 works.
* **antiheroriot:** fork the repo, push your branch to the fork, and open a pull request against
  `superwilso/Cinder`. The ZX300 changes would be very welcome even as a draft.
* **No-Plan59:** which transmitter (make and model), and the `cinderhome.log` from the drive root
  after a failed connect.
* **Uneekuza and Michlob:** a pull-down quick panel with brightness, Bluetooth, remote and the sleep
  timer is planned as an optional setting (B1).
