# The 2026-09 redesign — what landed, and a home for every feature

**What this is.** The owner commissioned a design pass over Cinder, [Flint](https://github.com/superwilso/flint)
and the Windows installer. It arrived on 2026-09-27 as a handoff folder: HTML mock-ups of 29
screens, a written spec, and a design brief. This document is the ledger that goes with it.

* **Part A** says what was built from it in the first pass.
* **Parts B to D** go through every screen in the handoff and give each one a state.
* **Part E** does the same for everything the project has promised that the handoff does not draw:
  the ten goals, Walkman One parity, and the community's requests.
* **Part F** puts the open items in order.

The spec itself, with every measurement Cinder's code is held to, is
[`SPEC_redesign_2026-09.md`](SPEC_redesign_2026-09.md).

The mock-ups themselves are not in this repository. They are HTML drawings rather than code, and
the folder they came in also holds captures of Sony's own screens. Screen IDs below (1a, 2c, 5k, …)
are the handoff's. **Cinder** IDs are 1x, 2x and 5x; **Flint** IDs are 3x and 6x; the
**installer** is 4a.

**State legend:**

| | |
|---|---|
| **Done** | Built in this pass, host-tested, in the golden previews. Where the device has run it, the row says so. |
| **Existing** | Cinder already did this before the handoff, and the mock draws what is there. |
| **Partial** | Part of the screen is built. The row names the rest. |
| **R2 … R7** | Planned, in the phase named in Part F. |
| **Research** | Needs something found out first. No estimate until it is. |
| **Not planned** | Stated plainly, with the reason, rather than implied. |

---

## Part A — What landed in the first pass (R1)

### A1. Cinder

1. **A shared kit (`player/cinder-ui/src/kit.rs`).** The spec's shared anatomy — section label,
   strip, 64 px row, switch, chips, primary button — as one set of drawing functions, each with
   its hit test beside it. It closes the brief's consistency items for the screens that use it:
   * one row anatomy;
   * one switch;
   * one way to put an action in a section label.

   Screens move onto the kit one at a time. Nothing was restyled in bulk.
2. **Settings ▸ Display (5k), a new screen.** Colour settings came off the Settings list and onto
   their own screen:
   * Palette, with a swatch of its colours;
   * Accent, which reads "Set by the palette" when the palette pins its own;
   * Night;
   * **Volume: Full / Minimal**;
   * Size;
   * Visualiser.
3. **Settings (2c) rebuilt on the kit.**
   * Five sections: DISPLAY, PLAYBACK, LIBRARY, SYSTEM, ABOUT.
   * The Display row's subtitle is "Palette · accent · volume display".
   * Every other row keeps its behaviour and its place in its section.
4. **Minimal volume display (2e).** A 3 px bar across the full width under the status bar, instead
   of the pill. It uses the same timeout as the pill. The setting is `volume_hud = full|minimal`,
   and the default is `full`, so nothing changes for anyone who does not pick it.
5. **The Menu (5a) rebuilt.**
   * **The strip.** A strip under the header shows what is playing, and tapping it opens Now
     Playing. It replaces the Now Playing and Up Next rows: what is playing is state, not a place,
     and the queue is one tap from Now Playing's toolbar.
   * **The rows.** Each is a kit row with a second line saying what is behind it.
   * **START ON.** Chips at the foot pick where the player opens: **Library** (the new default), Now
     Playing, Menu, or Last screen. A HOME tag marks the chosen row.
   * **Last screen.** It reopens the place you left, using the Shelf's own pin format. It does not
     touch the Shelf.

   New keys: `home_screen`, and `home_last`, which is written only when Last screen is chosen.
6. **Bluetooth (2g).**
   * The header switch is the kit switch.
   * PAIRED DEVICES is a section label, and PAIR NEW is its action (the big button is gone).
   * A new **THIS DEVICE** section has a Sound quality row. Its value is the LDAC rate, or the
     codec's name when the link is not LDAC.
7. **USB-DAC (2h).** The sentence comparing it with stock is removed.

**Gates, 2026-09-28:**
* 653 host tests (up from 612);
* clippy's correctness and suspicious lints clean;
* 49 harness scenarios;
* 271 golden previews, each changed one checked by eye;
* **a text audit** (`cinder-host --audit`, and `tests/ui_overflow.rs` with a hostile library):
  no text leaves the glass, lands on other text, or sits under something drawn over it. It found
  six real defects in existing screens, all fixed (CHANGELOG, *Fixed*).

**Device:** R1 and R2 both checked on the owner's A55 on 2026-09-28 — every row of
[`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §19 passes (dev build `b2a428d1`).

### A2. Flint (0.2, unreleased)

1. **The window is seven tabs**, in the handoff's order: Sync · On the player · Check · SensMe ·
   Likes & plays · Palettes · Settings. Sync is the 0.1 window and still the only page with orange
   on it.
2. **On the player** reads each drive:
   * the albums, and which of them Flint put there;
   * the plays in `.scrobbler.log`;
   * the liked songs;
   * the palettes in `cinder_palettes/`.
3. **Check** shows `flint check`'s verdicts as counts and as a table of files with reasons.
4. **Settings:**
   * Theme — Light, Dark or System;
   * the folders;
   * where the analysis cache is;
   * whether Last.fm is set up.
5. **Remembering.** The window remembers its folders, switches and theme in `gui.conf` beside the
   cache.
6. **Fixed a 0.1.0 bug.** The outlined buttons (Analyse library, Import Music Center, Check FLACs,
   Choose…, Clear) were drawn and ignored every click.
7. **Palettes checks and sends** (R6, pulled forward): a folder of `.palette` files on the PC is
   checked with Cinder's own readability rules and compared with the player's; **Send** copies the
   new and changed ones. The only write off the Sync page, and outlined, not orange: the accent
   means music about to be written.

---

## Part B — Cinder, screen by screen

| ID | Screen | State | What is left | Depends on |
|---|---|---|---|---|
| 1a | Now Playing | Existing | Nothing in the mock changes it | — |
| 1a | Library: view bar (sort · filter · rows) | **R3** | Grow the FILTER strip into the bar. The sort and filter state already exist (`lib_sort`, `album_sort`, the genre filter) | Kit |
| 1a | Swipe a row left = play next, right = add to Up Next, in every list | Existing | Landed before the handoff (CHANGELOG, Unreleased): the Library, Search, Folders, SensMe and every other track list | — |
| 1a | Sideways swipe changes page; Back is the bar arrow only | **R3** | The left-edge swipe is Back today, and the help screens teach it. Changing it is a behaviour change, so it gets its own golden and harness pass | — |
| 1a | Up Next | Existing | One editable list since the Unreleased change. What is left of [`SPEC_queue_v2.md`](SPEC_queue_v2.md) §1 — repeat album, stop after current, save Up Next as a playlist — is R3; shuffle by album and by artist is R4 | `album_artist` (R4) |
| 1a | Search | Existing | Scopes (songs · albums · artists) | R3 |
| 2a | Sound: signal-path strip | **Partial** | The strip is **Done** (R2): two fixed lines under the header, keeping the path's end when it is too long. Still to come: the mock's ENHANCE / SPACE / LEVEL grouping with values instead of pills, and the Profile row, which arrive with the profiles (R5) | R5 |
| 2a / 2b | A / B sound profiles per output | **R5** | A profile is a named set of every DSP value. One is remembered per output (jack, Bluetooth, USB-DAC) and applied when the route changes | The effects parity work (E3) |
| 2c | Settings | **Done** | — | — |
| 2d | Lock | Existing | No layout change, by the spec | — |
| 2e | Volume display: Full / Minimal | **Done** | — | — |
| 2f | FM radio | Existing | The meter and fast scan already appear only with the FM helper | — |
| 2g | Bluetooth | **Done** | The **Sound profile** row (value `A`/`B`) waits for R5. Until then it would be a row that does nothing | R5 |
| 2h | USB-DAC | **Done** | — | — |
| 5a | Menu | **Done** | Folders, Equalizer and USB-DAC keep their rows until the Library view bar and Sound hold them (the mock omits them) | R3, R5 |
| 5b | Playlist editing: ≡ to move, × to remove, UNDO, DONE | **R4** | Saved back as `.m3u8`, so Flint keeps the edit. Only Cinder's own playlists can be edited; Sony's live in its database ([`PLAYLISTS.md`](PLAYLISTS.md)) | — |
| 5c | Saved view: rules, sort, show as, pin to the bar | **R4** | A view is a name plus the view bar's state. The rating rule needs ratings, and "last played" needs play history | Ratings, the view bar (R3) |
| 5d | SensMe: 2-column grid of 12 channels, time-of-day switch | **Done** (R2) | `sensme.rs`: 68 px tiles with count bars, a NOW tile, *Follow the time of day* (`sensme_follow_time`), one primary button. The hour → channel split is Cinder's, not read from Sony | — |
| 5e | Battery: big %, CHARGER only with the helper | **Done** (R2) | Settings ▸ Device on the kit: strip, big number and bar, plain rows; CHARGER says "Needs the battery helper" without it. What differs is in Part G | — |
| 5g | Playlists: SMART above YOURS, EDITED tag | **R4** | **Drawn in the Library's Playlists tab, not in `shelf.rs`.** The Shelf stays exactly as it is (owner's rule) | Saved views, 5b |
| 5h | Album: rating in the right slot, playing row highlighted | **Partial** | The playing row is highlighted today. The rating waits for ratings | Ratings (R4) |
| 5m | Artist: albums newest first, then the 3 most played | **R4** | Needs a play count per track. Cinder writes plays to `.scrobbler.log` but keeps no count | Play history (R4) |
| 5i | Help & controls | **Done** (R2) | `help.rs`, `Screen::Help`: one list — the way back to Sony, getting around, the swipes, Now Playing, buttons — and a row that replays the first-run intro | — |
| 5j | Palette picker: list with swatches, SKIPPED section, ADD row | **Done** (R2) | `palette_list.rs`, `Screen::Palette`. Sort by name or date added (the file's mtime, read by the shell). Tapping picks and stays on the page | — |
| 5k | Display | **Done** | Two things differ from the mock on purpose; see Part G | — |
| 5l | Bluetooth ▸ Sound quality | **Done** (R2) | On the kit: a strip naming the live codec (and the fallback, when there is one), codec chips, LDAC's four modes as rows, VOLUME CONTROL | — |

---

## Part C — Flint

| ID | Page | State | What is left |
|---|---|---|---|
| 3a | Band and seven tabs | **Done** | The Sync sub-row (Plan · Conversion · Copying) |
| 6a | Setup (first run, no tabs) | **R6** | Library, player and likes cards. Until then, first run opens on Sync, as in 0.1 |
| 3a | Sync ▸ Plan: capacity per card, tickable plan rows | **Partial** | The capacity bars and the plan exist. Ticking rows to leave them out is R6 |
| 6a | Sync ▸ Conversion: format × card grid | **R6** | Convert-on-transfer (FLAC → FLAC 16/44 or AAC 256). This is new engine work in `flint-core`, not only a page |
| 6a | Sync ▸ Copying: plan rows with a state each | **R6** | One bad file skips one track and shows one line. The engine already does the skip; the page shows it |
| 3a | On the player | **Done** | Removing an album Flint put there (needs a confirm step) |
| 3a | Check | **Done** | — |
| 6a | SensMe: progress, 12 channel bars, NOT TAGGED by reason | **Partial** | The buttons and progress exist. The channel bars and reasons need the analysis job to report them. The MP3 path is part of the same work |
| 3a | Likes & plays | **Done** | Reviewing plays before they are sent. Sending is still `flint scrobble` and `flint likes` |
| 6a | Palettes: table, preview, Send | **Partial** | Done: a folder on the PC compared with `cinder_palettes/`, one row per palette with its swatch, state (BUILT IN · NEW · CHANGED · ON · ON THE PLAYER · REFUSED) and first reason; `palette.rs`'s readability rules ported to `flint-core/src/palette.rs`, the same floors and the same numbers; **Send N to the player** (new and changed only, never refused, lowercased, through a temporary name). Still to come: the Added column and Name/Added sort, the preview panel for the selected row, and ticking rows — Send takes every new and changed file today |
| 6a | Settings | **Partial** | Theme, the folders, the cache and Last.fm exist. Still to come: clearing the conversion cache, what likes become, and the Always keep list. All three arrive with Conversion |

---

## Part D — The installer (4a)

All **R7**. The installer works today; this is a rebuild of its window around the same engine
(`installer/src/stage.rs`).

| Page | What it adds |
|---|---|
| Player | A fact strip: PLAYER · FIRMWARE · CINDER · LAST BOOT, the last from the launcher's breadcrumb. Also an alert card for a Walkman One folder or a player handed back to stock. The CFW warning already exists (audit 2026-09-23 B5) |
| Options | Three groups with cost tags (`setuid root`, `needs Wampy`, `needs tags`, `new`, `dev only`) and a detail pane that says what each option costs |
| Confirm | A diff table: what changes, is added and is removed. Also a 4-step timeline |
| Installing / Done / Failed | Named steps from `stage.rs`, and the recovery ladder from `RECOVERY.md` on both Done and Failed |

The choice between the Cinder shell and a stock-like shell also belongs to this installer. That
shell is **on hold** (Part E4).

---

## Part E — Everything else the project has promised

### E1. The ten goals ([`VISION.md`](../VISION.md))

| # | Goal | State | Home in this plan |
|---|---|---|---|
| 1 | Faster boot, better battery | Largely done; not measured against stock | Measurement, not UI work — [`DEVICE_TESTS.md`](DEVICE_TESTS.md) |
| 2 | Improved UI | This document | R1 to R4 |
| 3 | USB-DAC in, LDAC and 3.5 mm out | Gated on `FuncMode==1`; recovered, not shipped | Device work, outside the redesign. The 2h screen is ready for it |
| 4 | Night mode plus a dimmer backlight | Done | 5k holds it |
| 5 | Built-in scrobbler | Done | Flint's Likes & plays page reads its log |
| 6 | Queue and shelf | Done | The rest of the queue plan in R3 and R4. The Shelf is unchanged |
| 7 | Keep every effect, and apply them to Bluetooth | Effects: 13 of Sony's ~54 methods wrapped. On Bluetooth: research | E3, then R5 |
| 8 | Use the built-in sound card | Done by construction | — |
| 9 | Lock screen with live buttons | Done | 2d unchanged |
| 10 | 2038 | Partial by necessity | Unchanged by the redesign |

### E2. Community requests ([`PLAN_community_2026-09-23.md`](PLAN_community_2026-09-23.md))

| Request | Home |
|---|---|
| B1 Quick-settings pull-down | **Done** (R2): `quick.rs`, Settings ▸ Pull-down panel, default Off. Brightness, Bluetooth, night, sleep timer. Its own overlay and state; the Shelf is untouched. The BLE remote toggle is left out because Cinder has no remote support (E4). The shell gesture (`cinder_quick_pull_begin` / `_open`) is device-verified (DEVICE_CHECKLIST 19.7–19.8) |
| B2 One-tap lyrics on Now Playing | **Done** (R2). A LYRICS chip top-left of the cover, drawn only when the song has lyrics |
| B3 Walkman One support | Device work (checklist 18.1 to 18.4). Outside the redesign, and first in line after a stable release |
| B4 Other players (ZX300 and others) | Waits for antiheroriot's pull request |
| B5 Clear Bass+ | Research |
| B6 Bluetooth debug log switch | **Done** (R2): THIS DEVICE ▸ Debug log. `SetHciLogEnabled` (slot 26), copied to the drive on switch-off, stopped at 4 MB, never persisted. Device-verified: the copy reaches the drive (DEVICE_CHECKLIST 19.12) |
| B7 Theme marketplace | [`PLAN_skins.md`](PLAN_skins.md). Palettes, the 5j picker and Flint's check-and-send page are done. A shared place to find palettes is the next step |

### E3. Walkman One parity ([`PLAN_walkman_one_parity.md`](PLAN_walkman_one_parity.md))

| Item | Home |
|---|---|
| W1's eight settings as on-device rows | A **WALKMAN ONE** section on Settings, shown only on a W1 player. Waits for Cinder running on W1 (checklist 16.2) |
| Clear Bass | Research. If the six-band never engages, Clear Bass is rebuilt on the ten-band |
| Effects parity: 13 → ~54 Sony methods | Ordinary work; the enums are recovered. It must come before R5, because a profile can only hold effects Cinder can set |
| SensMe | Done in Flint and on the device. The channel grid (5d) is R2 |
| Language Study, Alexa, Help Guide | **Not planned** |
| Noise cancelling, Ambient Sound | **Not planned**: inert without Sony's own NC headphones |

### E4. From the four-builds matrix ([`VISION_four_builds.md`](VISION_four_builds.md) §5)

These have no screen in the handoff. Each gets one when its engine work is done, drawn with the
kit:

* **Library:**
  * CUE sheets;
  * bookmarks;
  * per-song audio settings (5c's rules could hold them).
* **Radio:**
  * FM recording;
  * the extended band (76–108).
* **Bluetooth:**
  * receiver mode (the Receiver screen exists; the path does not);
  * the RMT-NWS20 remote.
* **Output:** high gain, and line-out.
* **Stock-like shell (builds ② and ④):** **on hold** by the owner's choice. The handoff leaves it
  out, and so does this plan. [`PLAN_walkman_one_parity.md`](PLAN_walkman_one_parity.md) §7 keeps
  its brief.

---

## Part F — Order

| Phase | What | Rough size |
|---|---|---|
| **R1** | Part A | Done |
| **R2** | **Done.** Screens on the kit that need no new data: ~~5j Palette picker~~, ~~5d SensMe grid~~, ~~5l Sound quality~~, ~~5e Battery~~, ~~5i Help~~, ~~the Sound strip (2a)~~. Then the three small community items: ~~B1~~, ~~B2~~, ~~B6~~ | — |
| **R3** | The Library view bar, Search scopes, the sideways page swipe, and the queue extras that need no new data: repeat album, stop after current, save Up Next as a playlist | 1–2 weeks |
| **R4** | Data the screens are waiting on, then the screens: ratings, a play count, `album_artist` (which also unlocks shuffle by album and by artist). Then saved views (5c), smart playlists (5g), playlist editing (5b), and Album and Artist (5h, 5m) | 2 weeks |
| **R5** | Effects parity, then A/B profiles per output (2a/2b, and the Bluetooth Sound profile row) | 2 weeks, most of it effects parity |
| **R6** | Flint 0.3: Setup, Conversion (convert-on-transfer), Copying, ~~Palettes send~~ (done in 0.2; the preview panel and ticking are left), SensMe reasons | 2 weeks |
| **R7** | The installer's window | 1 week |

Device work — Walkman One (B3), USB-DAC (goal 3) and Clear Bass — runs beside these, on the
owner's schedule, from [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md).

---

## Part G — Where the build differs from the mock, on purpose

| Where | Mock | Built | Why |
|---|---|---|---|
| 5k Accent | One value and a chevron | The six swatches stay | One tap per colour instead of a sub-screen. When a palette pins its own accent, the row still says "Set by the palette", as the mock does |
| 5k Size | Chips | The slider stays | The sizes do not fit as 44 px chips across 440 px |
| 5a Menu rows | No Folders, Equalizer or USB-DAC | They stay | The mock moves them into the Library bar and Sound, and those are R3 and R5. Removing the rows first would strand the screens |
| 5a Start on | Library · Now Playing · Menu · Last screen | Same, default **Library** | The brief: "Home: user-selectable, ship Library as the default." Before this pass the player opened on Now Playing |
| 2g Sound profile | A row with value `B` | Not drawn yet | A row that does nothing is worse than no row. It arrives with R5 |
| 5g Playlists | Mapped to `shelf.rs` | Will be the Library's Playlists tab | The Shelf is the owner's and stays as it is |
| 5l Codec | No codec picker | Codec chips (LDAC · aptX HD · aptX · SBC) above the LDAC rows | The mock drops the codec choice; the player transmits four, and the choice has to live somewhere. Chips, so the page still fits without scrolling |
| 5l Steps | A "Steps 127" row | Not drawn | It would be false: AVRCP's scale is 0–127, but this firmware moves 4 units a step (`bluetooth.rs`, `fine_volume`). Fine volume is the row that answers "how fine" |
| 5l Strip | Name · codec · rate / depth | Name · codec, and "ASKED FOR …" on a fallback | The link reports no rate or bit depth. The fallback line is the thing the old LIVE tag did |
| 5e Battery | Fuel-gauge health, cell temperature, charge current, time left | Not drawn; the screen stays Settings ▸ Device | This hardware has no fuel gauge (`device.rs`). The facts it does have — die temperatures, CPU, memory, storage — stay below the battery |
| 5i Help | "Change page: SWIPE ← →", "Back: ‹" | "Back: ARROW · LEFT EDGE", no sideways-page row | The sideways page swipe is R3; today the left-edge swipe is Back. The list says what the player does now |
| 5i Help | Three rungs of the way back | Four: Boot to stock added, and "not on the first start after an install" under the cable | Both are true (`RECOVERY.md`), and the exception is exactly when the cable rule surprises people |
| 2a Strip | One line | Two fixed lines | The full chain is ~90 characters. Fixed height, so the rows below never move with the text size |

## New settings keys

| Key | Values | State |
|---|---|---|
| `volume_hud` | `full` · `minimal` | Done |
| `home_screen` | `library` · `now_playing` · `menu` · `last` | Done |
| `home_last` | a Shelf pin | Done; written only when `home_screen = last` |
| `palette_sort` | `name` · `added` | Done (5j) |
| `quick_settings` | `0` · `1` | Done (B1); `0` by default |
| `sensme_follow_time` | `0` · `1` | Done (5d); `0` by default |
