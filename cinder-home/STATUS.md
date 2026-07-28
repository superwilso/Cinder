# Cinder — status & flash/verify guide (audited 2026-07-26)

> **Production-readiness gap list (2026-07-27):**
> [`../docs/PRODUCTION_READINESS.md`](../docs/PRODUCTION_READINESS.md) — what is left before this is
> a device the owner can rely on with no PC in the room. This file says what *is*; that one says
> what is *missing*.

> **RESUME POINT (2026-07-26).** Two device sessions have run since the 07-03 round (07-25 and
> 07-26) and three commits have landed on top of it. The workspace is clean and every offline gate
> passes: **71 host tests** (40 UI + 21 FFI + 8 DB + 2 font), the 18-case launcher recovery matrix,
> the GLIBC ≤2.23 ceiling on both channels, and the qemu construction preflight.
>
> **Two facts that change what the next flash means:**
> 1. **Cinder is NOT installed on the device.** The 2026-07-26 brick was recovered with a wbrt
>    restore, which rolled the whole eMMC back to the 2026-06-18 image — no `cinder-home`, no
>    launcher, no `/contents` flags, and the music library is whatever that image held.
> 2. **The next flash carries a large unverified batch.** Everything from 07-26 is code-complete
>    and offline-proven but has never run on hardware: the **+2 type-scale pass**, the **non-Latin
>    font fallback**, the **GPU/EGL present path** (opt-in, default off), **screenshot capture**,
>    the **rewritten escape ladder** (state on `/data`, cable-at-boot as rung 0, `MAXBAD=4`,
>    self-healing latch), and the new **`cinder-gpunode`** setuid helper. Play-by-index (07-03) is
>    also still device-unverified.
>
> Because of (2), **run `cinder-probe` before flashing** (STEP 1) — it exercises render/db/audio
> with no easel lifecycle, so it cannot affect boot, and it shrinks the bisect surface if the
> flash misbehaves. Reinstall now needs **three** pushes, not one (STEP 2).
>
> Forward plan + priorities: [`ROADMAP.md`](ROADMAP.md). Full audit incl. known doc drift:
> [`../docs/AUDIT_2026-07-26.md`](../docs/AUDIT_2026-07-26.md).

> ## ⚠️ READ FIRST — a flash hung the device and required a wbrt restore (2026-06-26)
> The first Home-app flash **soft-bricked** the device (stuck on the boot screen, no auto-revert,
> needed wbrt). Two bugs caused it, both now fixed:
> 1. **The launcher's bad-boot counter reset itself on a blind 60-second timer** — and a *hung*
>    process "survives" 60 s, so the counter never accumulated and never auto-reverted. Removed;
>    the counter is now reset **only by cinder-home after it proves healthy**, so a hang makes it
>    climb across reboots → auto-revert after **2** bad boots.
> 2. **The hang watchdog was cancelled once the renderer was up**, so a blocking PlayerService
>    call *in the pump* hung forever with appmgr satisfied → no reboot → soft-brick. Now **every
>    Sony-IPC call runs inside a crash+hang GUARD** (`run_guarded`, host-validated): a crash or
>    hang is caught, that subsystem is skipped, and the **UI keeps running** — a bad service can
>    no longer hang the boot.
>
> **Because of this, the next device step is NOT a Home-app flash.** It's the zero-risk
> `cinder-probe` diagnostic (below), which never replaces the Home app and can't affect boot.
> A standalone probe run under qemu already caught `getPlayController()` null-deref'ing when
> PlayerService isn't reachable — a plausible boot-timing failure we now want to confirm on real
> hardware before trusting a Home flash.

This is the hand-off after the autonomous integration session. It tells you **what works**,
**how to safely diagnose**, **how to flash/verify**, **how to tune the keymap**, and **what
still needs the device**. Read "⚠️ READ FIRST" then "STEP 1: safe diagnosis" before flashing.

> **What's left & in what order: [`ROADMAP.md`](ROADMAP.md)** — the device-session critical path and
> the prioritized backlog (this file is current state; the roadmap is the forward plan).

---

## STEP 1: safe diagnosis (do this first — zero brick risk)

`cinder-probe` runs ONLY the suspect calls (framebuffer, library DB, PlayerService connect,
render+poll) in isolation — no easel/appmgr lifecycle, so it does **not** become the Home app and
**cannot** affect boot. It's watchdog-bounded: on a hang it logs the exact PC and exits. Needs a
shell on the device in **normal boot** (stock UI up, so PlayerService is running) — adb is the
easy path:

```bash
# /tmp is the ONLY writable exec-able mount (/data and /contents are noexec); toolbox chmod
# needs an octal mode, not +x.
adb push cinder-home/dist/stable/cinder-probe /tmp/cinder-probe
adb shell 'chmod 755 /tmp/cinder-probe && \
  LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib \
  /tmp/cinder-probe'
```

Read the printed trace. The **last `[N/4]` line before it stops** is the culprit:
- stops at `[3/4] cinder_audio_init` → PlayerService connect is the problem (matches the qemu
  finding). The fix is device-RE on the connect path / a readiness wait.
- stops at `[2/4] cinder_db_open` → the DB path is wrong or the load is pathologically slow.
- reaches `DONE` cleanly → none of these hang on your device, so a Home flash is much safer.

If you don't have adb: the hardened Home flash (STEP 2) now **auto-reverts** a hang in ~2 forced
reboots and leaves the hang PC in `cinderhome.log`, so it's no longer a wbrt risk — but probe-first
is still preferred.

---

## TL;DR — tenth round (2026-07-03): library ordering + Albums accordion

Library browse gained sort/order options and an inline album view (all 39 UI tests + 8 DB tests
green; qemu preflight PASS; both channels rebuilt + packed). No FFI symbols changed (ABI intact).

1. **Songs SORT chip now 7-way** — `TITLE · ARTIST A-Z · ARTIST Z-A · LENGTH · ADDED · ALBUM ·
   YEAR` (was 3). Tap the chip (header right slot) or press Option to cycle. New keys ride on
   `SongRow` (album_id/disc/track, addedtime, release year), populated from the DB in
   `build_library`; `song_order` sorts client-side.
2. **Albums ORDER chip** — `ARTIST` (artist-grouped, the classic view with section headers) ·
   `A-Z` · `ADDED` · `YEAR`. Flat orders drop the artist headers. Header shows "ORDER · …".
3. **Albums accordion** — tapping an album row's **body** expands its tracks inline (numbered,
   indented, on a panel band); tapping again collapses. Tapping the album **art** (left, x<72)
   still opens the full drill-in page (cover art). Tapping an inline track plays it in album
   context. The Albums tab is now one variable-height display list (`albums_build` →
   Group/Album/Track rows); layout, hit-testing, and pixel-scroll all read from it so they can't
   drift. Single-expand (one album open at a time). Verified in host PNGs
   (`library_albums_expanded`, `library_albums_az`, `library_songs_added`).
4. **Release year — best-effort** — `AlbumRow.year` was always blank; now resolved via a
   `releaseyears(id,value)` lookup (sibling of albums/artists; FK `releaseyear_id`). Table name
   unconfirmed in RE, so `Db::release_years()` tries `releaseyears` then `releaseyear` and
   **degrades to blank on any mismatch** (never fails the build) and logs the hit count. If the
   guess is wrong the YEAR sorts are inert (years blank, as before) and the next `MTPDB_copy.dat`
   pull confirms the real schema. Same DB-pull closes **playlist** detection (still deferred:
   `PlaylistTrack` membership schema not yet RE'd — the Playlists tab remains empty by design).

## TL;DR — ninth round (2026-07-03): smooth & responsive + device-feedback fixes

Every item from the device-feedback list, root-caused and fixed (63 tests green; qemu
preflight PASS; both channels rebuilt + UPGs packed):

1. **Side buttons fixed** — the real NW-A50 key codes are plain keyboard codes (wampy
   glfw.patch): play=28/KEY_ENTER (was mapped to SELECT → opened the Menu), next=106, prev=105.
   New defaults map them to GLOBAL transport (`CINDER_BTN_NEXT/PREV`, work on every screen);
   key repeats now only ramp the volume rocker (a held FF no longer machine-guns skips).
2. **Volume = stock 0–120, persisted** — UI level is the raw 0..120 scale (HUD shows "N / 120"),
   written 1:1 to `amixer card0 'master volume'`; the level persists in
   `cinder_settings.conf` and is **restored to the hardware at boot** (fixes booting at hw
   level 1 ≈ mute). First boot with a mute mixer applies an audible 15/120 default.
3. **Smooth scrolling** — lists scroll in PIXELS with live drag (deltas stream to the UI every
   pump tick, list tracks the finger 1:1) + momentum fling with decay. Canvas grew a y-clip
   band so partial rows draw cleanly. Old row-jump-per-gesture scroll is gone.
4. **USB-MSC** — before flipping the mode we now STOP playback and release the pinned track
   sequence (`cinder_audio_release_sequence`) — a merely-paused PlayerService keeps the track
   file open under /contents → umount EBUSY → LUN write fails → "PC sees nothing". Recovery
   re-points the LUN **and bounces the gadget** (Windows won't rescan an already-enumerated
   no-medium reader). If recovery fails, the whole tmpfs diagnosis (incl. the fd-holder dump)
   is appended to cinderhome.log immediately, so it survives a reboot out of MSC.
5. **adb (dev)** — `setprop ctl.start adbd` (the init.rc `adbd` service exists but is
   class-disabled; `sys.usb.config` writes and a non-init `start` did nothing on this fw).
   3 s later the log records `init.svc.adbd` + process + gadget functions, so the next log
   pull separates "adbd not running" from "Windows driver missing".
6. **Album covers** — root-cause candidate found: `images.value` was read as TEXT; when the
   real DB stores the art as an inline BLOB the whole SQL row errored → art silently None
   (NULL `dataoffset` did the same). Both fixed (BLOB decodes directly; NULLs default), plus a
   one-line per-track art diagnostic in the log and a dev-boot copy of `/db/MTPDB.dat` →
   `/contents/MTPDB_copy.dat` for offline schema work.
7. **Album page = all songs, correct order** — per-album track lists are now keyed by
   `album_id` (names collide) and ordered by disc/track (was: title order, name-keyed).
8. **Back chevron** — tappable target widened to the whole header-left block (y 34..91,
   x<80 ≥44px) on every header screen.
9. **Now Playing toolbar** — inert heart replaced: `library · queue · eq · bt · settings`
   (slot 1 jumps straight to the Library, per request).
10. **Log spam gone** — `run_guarded` traces each label once + on recovery only ("pump: poll
    now-playing" no longer prints every second; recoveries now name the failed call).

## TL;DR — what changed this session

1. **Root-caused & fixed the boot crash.** cinder-home was crashing in
   `CuiAppModule::OnInitialize` (`[this+0x18]=0x12`). Cause: `easel_abi.hpp` declared the
   `ApplicationBase`/`CuiAppModule` device classes with **no storage**, so `new`/stack
   under-allocated and the device ctor overflowed memory. Fixed by reserving the real
   footprint (sizes read from the Ghidra decompiles) + `static_assert`. **cinder-home now
   constructs cleanly** — proven under qemu against the device's own libs.
2. **Two offline gates** (no device needed), wired into `build.sh`:
   - `tools/preflight_qemu.sh` — constructs the real device objects under qemu with guard
     canaries; catches std::function-ABI / ctor-signature / object-SIZE regressions before you
     ever flash.
   - `tools/pack_upg.sh` — reproducible `.UPG` packer (refreshes `dist/`).
3. **The UI is now data-driven & daily-usable** (all verified by rendering PNGs offline):
   real library browsing with **windowed scrolling**, a **volume HUD**, an **interactive EQ**,
   **Bluetooth on/off**, a **built-in scrobbler**, and a **complete input/now-playing pump**.

## Feature status — fully functional vs partial vs stationary

Authoritative, code-verified (2026-06-29). Three tiers:
**✅ Functional** = wired to the device / real data, end-to-end. **◐ Partial** = the UI works but the
backend/hardware leg isn't wired yet. **▢ Stationary** = renders but is a placeholder / no action
(display-only). The matrix below is the single source of truth; the per-feature RE detail lives in
`../analysis/RE_playerservice_sound.md` (§10 audit, §11 bullet-proofing).

### ✅ Fully functional (real device / real data)
- **AUDIO PLAYBACK** (2026-07-27) — measured on device: position advancing 1000 ms/s, listener
  callbacks 1/s with real position + duration, `ALSA pcm4p` reaching `RUNNING`. Three bugs had to
  fall, and the first one is the important one:
  1. **Nothing drove `pst::core::Framework`'s event loop.** Sony's PlayerService client is
     asynchronous — every call marshals a request and the *reply* is dispatched by that looper.
     This file already noted that easel's pump never fires for our non-Qt `CuiAppModule`, but the
     consequence went unnoticed for weeks: with no pump, every out-param stayed **uninitialised**,
     so the client handed back stack garbage. `Connect` "returned" a `0xb6xxxxxx` pointer,
     `IsConnected()` read uninitialised stack as *true*, and `SetTrackSequence` "failed with code
     99". None of those were real error codes, and the service logged nothing at all because no
     transaction ever completed. `cinder_audio_pump_start()` (called from `deferred_up`, after
     `app.run()` has started the Framework — calling `GetReference()` earlier segfaults) fixes the
     whole class of symptom. Wampy's `pstserver` drives the same loop the same way.
  2. **The SoundService "Music" track leaked.** SoundService allows exactly one track per type
     (`SoundServiceImpl.cc:248 "Cannot create multiple tracks that have same type"`). A process
     that exits without `ClosePlayer` leaves it held inside the long-lived `hagodaemon`, and every
     later attempt then dies at `AudioTrackFactory::Create()` → `WMX_AudioOutput::Open()`
     (`0x80001009`) — a track that loads and shows correct metadata but is silent forever.
  3. **`SetTrackSequence` leaves the OMX graph at Idle, and Idle → Executing is illegal.**
     `play_tracks` now does Pause then Play. `playstate_t` is calibrated from the service's own
     logcat, one value per run: `0` = Stop (no transition at all), `1` = Pause (`GapPlayer_pause`,
     Idle → `OMX_StatePause`, and where the track is actually created), `2` = Play.
  **3.5 mm goes out over the hardware DAC, not the CPU**: the PCM device that opens is `hw:0,4` =
  `cxd3778gf-icx-lowpower`, the low-power S-Master path, so this is already the battery-efficient
  route. Diagnosed with `cinder-probe --pump`, which is kept for re-testing.
- **Boot & shell**: launches as the easel `type:Home` app, full lifecycle, dirty-flag framebuffer
  paint (480×800 XRGB8888, triple-buffered, blit bounded against the mapping).
- **Text / non-Latin tags** (2026-07-26): the bundled faces are Latin-only — Hanken Grotesk has
  **no Cyrillic, Greek, CJK or Thai at all** — so Japanese, Chinese, Korean, Russian and Thai tags
  used to render as rows of `.notdef` boxes. `text.rs` now falls back, per codepoint and lazily,
  onto **Sony's own fonts already on the device** (`/system/vendor/sony/lib/fonts`), so nothing is
  bundled or redistributed and a Latin-only library loads none of them. `SST-Roman.otf` leads the
  chain because `SSTJpPro` renders Cyrillic/Greek **full-width** (spaced-out). Both `measure()` and
  `draw()` resolve identically, so truncation/centring stay correct. Full RE of the device font set,
  including a genuine GSUB defect in `SSTUI-Roman.ttf`, is in
  [`../analysis/RE_sony_fonts.md`](../analysis/RE_sony_fonts.md). Guarded by
  `player/cinder-ui/tests/font_coverage.rs`; visible in the host renders `i18n_*`.
- **Input model (NW-A55 = touch + transport buttons, NO d-pad)**: the physical buttons are
  Play/pause, ◁ rewind, ▷ skip, Vol±, Power, and the Hold switch — transport + power only.
  **All navigation is the touchscreen**: tap to open/select, drag to scroll lists, **left-edge swipe
  = Back**, status-bar tap = Menu. Implemented across every screen (`nav::App::tap`/`touch_scroll`);
  the shell classifies tap vs scroll vs edge-swipe and maps raw touch → UI via the panel's reported
  range (`EVIOCGABS`). *(Tap coordinates + the touch range want a quick on-device confirm; the
  discovery probe dumps the input ranges.)*
- **First-run onboarding**: a paged intro (Welcome → Controls → What's inside → Done), shown once
  (persisted), re-openable any time from the Menu as **Help & Controls**.
- **Lock screen = a true keylock** (matches the NW-A55 Hold switch): while locked the **touchscreen
  is disabled** (pocket-safe — taps do nothing), but the **transport + volume buttons still control
  playback** (skip/pause/volume without unlocking). The **Hold switch is the only thing that
  unlocks**; **Power just toggles the screen** (backlight on/off) and never unlocks. The Lock screen
  shows the real current track. *(The shell maps the Hold-switch evdev code via `cinder_keymap.conf`
  → `12`; the code itself comes from the dev keycode log.)*
- **Now Playing** shows the **real current track** (PlayStatus URI → library-DB resolve).
- **Transport**: Play/Pause, Next, Prev, Next/Prev **Album** → PlayerService `PlayController`
  (each call guarded). Album step = the shuffle-by-album primitive. **Shuffle + repeat are tappable**
  on the transport row (repeat cycles off→all→one); the queue-reorder wiring is device-gated.
- **Shelf**: a bottom-sheet overlay to **pin the current place to 3 slots and jump back**, plus Undo
  — fully wired (open/pin/go/clear/close), session-scoped. Opened from the **bookmark glyph in the
  top-right of the status bar** (per the prototype — it overlays wherever you are; the rest of the bar
  opens the Menu).
- **Library browse**: Songs / Albums / Artists tabs, **real DB data**, windowed **scrolling**
  (thousands of rows), Songs sort chip (Title/Artist/Length), grouped album headers, **album drill-in**
  (album → track list), hashed-gradient art until real thumbnails decode.
- **Playlists** (2026-07-26): the Playlists tab lists the device's real playlists with their track
  counts, and tapping one **plays it from the top in saved order**. Sony has no playlist table —
  playlists are containers in a second object tree, with membership rows pointing at tracks by
  `reference_id` — so this is read straight from the DB with no extra service. Two traps handled:
  the `.m3u8` rows in the file tree are decoys with zero children, and deleted playlists leave
  their entries behind (96% of entry rows on the reference DB were orphans), so the container join
  is what stops ghost playlists appearing. Full schema:
  [`../analysis/H_mediastore/RE_findings.md`](../analysis/H_mediastore/RE_findings.md).
  *(Playback path is shared with play-by-index, so it carries the same device-verify caveat.)*
- **Library shuffle bands** (2026-07-26): the accent band at the top of each Library tab now
  works — **Shuffle all songs** (whole library, random), **Shuffle by album** (random album order,
  each album's tracks kept in sequence), **Shuffle by artist** (one random artist, shuffled), and
  **Shuffle a playlist**. The queue is pre-shuffled by Cinder itself, so the order is genuinely
  random regardless of what PlayerService's own shuffle does.
- **EQ → real DSP**: 10-band edit + preset cycle → `EffectCtrlDmp` (guarded). Preset pills, band
  columns and the footer **Reset** are hit-tested through the EQ's own layout helpers (2026-07-26 —
  they previously disagreed with the render, so tapping "A2" applied "JAZZ").
- **Sound effects → real DSP**: DSEE HX, Vinyl, VPT, DC-Phase, Dynamic Normalizer, ClearAudio+ as
  live On/Off toggles → `EffectCtrlDmp`; **A/B compare** (Option = Disable/Reenable whole chain).
- **Battery care**: On/Off → Sony "Itawari" charging (`PowerMgrServiceClient::EnableItawariCharging`);
  real device state read at boot.
- **Settings (live rows)**: Theme day/night, Visualiser **type** (5), Visualiser **animation** on/off.
- **Sleep timer** (Settings): cycles Off/15/30/45/60 min, counts down, shows a live "SLEEP {n}M"
  badge on Now Playing, and **pauses playback on expiry** — pure app logic, no Sony service.
- **Visualiser**: Bars / Mirror / Segments / Dots / Wave; always animates (synthetic), with an optional
  **real audio-reactive** mode via Sony's `AudioAnalyzerService` (default OFF — `cinder-probe
  --analyzer` to validate, then `/contents/cinder_viz.conf: analyzer=1`).
- **Up Next**: the queue = the **current album** (resolved from the library by the now-playing
  track), playing row highlighted, auto-scrolls to follow playback; clean empty state otherwise.
  When the user queue (below) is non-empty, Up Next shows it instead, in add order. **Tapping a
  row plays that track** (2026-07-26 — any tap used to just exit the screen).
- **Swipe-to-queue (Spotify-style)**: rightward swipe on a Library-Songs row, an **expanded album's
  inline track row** (added 2026-07-26 — the gesture previously ignored the Albums tab) or an
  album drill-in track adds it
  to the user queue — "Added to queue" toast + a "+ QUEUED" chip slides off the row (~0.4 s).
  Left-edge→right is still Back (classified first); the two rightward gestures coexist. *(Queue
  display + intent only: PlayerService honoring it is gated on `SetTrackSequence` RE — same gate
  as play-by-index.)*
- **Settings ▸ Storage**: real internal-storage usage ("used / total GB") from `statvfs`.
- **Night-mode backlight (minimal light)**: toggling Settings ▸ Theme to **night dims the panel
  backlight to minimal** (and day restores it). The backlight node is auto-detected (the standard
  Android/MTK paths), so it works with no config on most devices; tune the exact levels/node via
  `/contents/cinder_backlight.conf` (`deploy/cinder_backlight.conf.example`). No-op if no node found.
  **Boot always comes up at DAY brightness** even if night theme is persisted — the dim is a
  per-session action, never resumed on boot, so you can't get locked into an unreadable screen.
- **Settings persistence**: theme (day/night), visualiser type/animation, the **10-band EQ**, and the
  **Sound effects** are saved to `/contents/cinder_settings.conf` and restored at boot — the UI is
  restored before the first paint, and the saved EQ/sound are **re-applied to the DSP** once audio is
  up (guarded). Your full audio + display config survives a reboot.
- **Bluetooth transmit codec selector**: choose **LDAC · aptX HD · aptX · SBC** (the codecs this
  hardware can transmit; AAC is receive-only, excluded) from a checked list, with an **LDAC quality**
  sub-row (Auto/990/660/330, Auto default). It's **one device-wide preference**, persisted to
  `/contents/cinder_settings.conf` and published to `/contents/cinder_bt.conf` so **both normal BT
  playback and the USB-DAC→LDAC bridge use the same codec**. *(The live `BtTransmitterService` apply —
  SetLdac/SetAptxHD/SetSbc + SetLdacSoundQuality — is device-gated, same C++ boundary as `ldac-bridge`.)*
- **USB-DAC → LDAC (the headline feature)**: the USB-DAC screen toggle **engages USB-DAC input and
  routes it to the 3.5 mm jack AND Bluetooth/LDAC at once, without disconnecting BT** (stock forces a
  disconnect). The shell starts the LDAC bridge (`/contents/ldac_on`) + switches the USB gadget to
  UAC. *(The bridge daemon + the `setprop` USB-mode switch are device-gated — validate live; the UI,
  the toggle, and the bridge-engage signalling are wired.)* Mass-storage moved to **Settings ▸ USB mode**.
- **Status bar**: live clock (local time) + battery % (sysfs); **tap anywhere on it → Menu**.
- **Scrobbler**: appends `/contents/.scrobbler.log` (Audioscrobbler/1.1) as you listen.
- **Safety**: bad-boot counter → auto-revert to stock after **2** bad boots; per-frame + construction
  watchdog; every Sony-IPC call inside `run_guarded`; USB-at-launch / `cinderhome_off` escape.

### ◐ Partial (UI works; backend/hardware leg pending — device-gated)
- **Volume keys**: HUD + UI level work, and the hardware path now has a **built-in default from the
  2026-07-02 discovery dump** — no conf needed: `amixer -c0 cset name='master volume' <0..120>` (the
  CXD3778GF master, the stock 120-step range). At boot the UI level is **seeded from the real mixer**
  (`sync_volume_from_hw` → `cinder_set_volume`), so the first Vol± nudges from the actual volume.
  `/contents/cinder_volume.conf` still fully overrides (see `deploy/cinder_volume.conf.example`);
  wrong-hardware safety: an unknown control name makes `amixer cset` fail → keys stay HUD-only.
  *Pending: one on-device verify that Vol± audibly changes output.*
- **Now Playing progress bar**: **real position from the service** (2026-07-27). The
  `PlayEventListener` fires `onPlayTimeUpdated(cur_ms, total_ms)` about once a second; cinder-ffi
  interpolates between updates so the bar is smooth, and re-anchors on every update, so it follows
  seeks, mid-track starts and wrong tag durations — all of which the old local play-clock estimate
  could not. The estimate survives only as the fallback for before the first callback arrives.
  Play/pause state also comes from observed position movement rather than from the shell's
  optimistic view of the last transport action it sent.
- **Status bar shows the REAL clock and battery** (2026-07-27): it was drawn inside each screen's
  own render, and **14 of the 16 call sites passed hardcoded literals** (`"14:32"`, `"FLAC 24/96"`,
  `78`). Only Now Playing and Lock passed live values — so on device the clock read *14:32* and the
  battery *78%* on the Menu, the entire Library, Settings, EQ, Sound, Bluetooth, Up Next, USB-DAC,
  FM and the Receiver: every screen you actually browse in. It is now drawn **once**, by the
  navigator, so it cannot drift again.
- **One-tap return to Now Playing** (2026-07-27): tapping the clock/codec-badge zone of the status
  bar goes straight there, from any screen. The badge *is* the now-playing indicator, so that is
  where a finger already points. Previously the only route back was the Now Playing bar, which
  appears solely on Library and Album — from Settings, EQ, Sound, Bluetooth, Up Next or the Menu
  there was no direct way back at all. Uses `go`, not `push`, so it collapses the stack instead of
  burying Now Playing under whatever was being browsed.
- **Now Playing bar is a real mini-player** (2026-07-27): its left zone is now a **play/pause
  button** (the rest still opens the screen), it carries a **live progress line** along its top
  edge, and an up-chevron marks it as something that opens rather than a passive label.
- **Bigger Shelf target** (2026-07-27): the bookmark hit zone grew 44 → 60 px and the glyph 19 → 23,
  since it is the one part of the strip that doesn't open the Menu — a miss there lands on the wrong
  screen. Room came from dropping the status bar's Bluetooth glyph, which was drawn `faint`
  unconditionally and never reflected any BT state.
- **Menu ▸ Now Playing shows the running track** (2026-07-27): title · elapsed, or "Nothing
  playing". The row was blank.
- **Volume HUD is a slim pill, not a card** (2026-07-27): it was a 320×96 slab centred on the
  panel — parked over the focal point of the album art — stating the same number three ways
  (`18`, `/ 120`, `15%`). Volume is a transient nudge: you already know what you pressed and just
  want confirmation. Now a 40 px pill under the status bar with one icon, one bar and one number
  (`MUTE` at zero), leaving the artwork alone.
- **Visualiser is live-only, and demand-driven** (2026-07-27): it now draws **only** when real
  spectrum data is arriving from Sony's `AudioAnalyzerService`. The synthetic fallback is gone — it
  animated identically for silence, a ballad and a drum solo, so it looked like a representation of
  the audio without being one, the same category of untruth as the hardcoded clock. The analyzer is
  no longer started at boot: it starts on demand only while the panel is **on**, Now Playing is the
  current screen, and audio is actually playing, and stops the moment any of those stops being true.
  That keeps a Sony-service connect off the boot path entirely (it can only run after the app has
  painted and cleared the bad-boot counter) and means no FFT, no IPC and no wakeups while the screen
  is dark or you are browsing — most of the time the device is switched on. Now default **ON**
  (`analyzer=0` in `/contents/cinder_viz.conf` disables it), because with no synthetic fallback the
  visualiser cannot appear at all without it. *Pending the one on-device check that Sony's analyzer
  actually emits frames — `cinder-probe --analyzer` — which has never been run.*
- **16-bit and palette PNG covers** (2026-07-27): the PNG decoder now sets `STRIP_16 | EXPAND`.
  16-bit-per-channel PNGs (real in high-quality rips) arrived as `w*h*6` bytes and the RGB path's
  `truncate(w*h*3)` kept the first half of the interleaved high/low bytes — so those covers rendered
  as **noise** rather than being rejected, the worst of both outcomes. Indexed/palette PNGs were
  skipped outright, so those covers simply never appeared. Both now decode correctly, with tests
  that fail against the old behaviour.
- **USB-MSC log redirect now fails safe** (2026-07-27): entering mass storage moves stdout/stderr
  off `/contents` because an open fd there makes init's `umount /contents` fail `EBUSY` (the LUN
  write then fails and the PC sees a reader with no medium). `redirect_fds` silently did **nothing**
  when its `open` failed, leaving both fds on `/contents/cinderhome.log` — so MSC would break and
  the reason could never appear in any log, because the log was the thing breaking it. That is not
  hypothetical: a whole MSC debugging session was blinded this way when `/tmp/cinder_msc.log` turned
  out never to have been created. It now falls back to `/dev/null` (losing the log beats failing to
  release `/contents`) and reports the failure *before* switching, while stderr still points at the
  old destination, so the explanation survives in the previous log.
- **Scrolling momentum is frame-rate independent** (2026-07-27): the fling stepped `v / 60.0` per
  tick and decayed a flat `0.92` per tick — a hardcoded 60 fps. But this project's own bench
  measured a **scrolling** frame at ~31 ms on device (~32 fps), and flinging *is* scrolling, so the
  assumption was wrong by 2× in both terms at once: each step moved half as far as intended *and*
  the decay compounded twice as fast per second. A flick travelled a fraction of its intended
  distance on hardware while feeling perfect on the host — the kind of gap that just reads as "the
  device is sluggish". Momentum and the HUD/toast countdowns now advance on real elapsed time
  (clamped, so a stall can't teleport a fling). The fling also never coasted on Settings, whose
  scrolling was added without updating its clamp detection.
- **Volume ramp no longer forks a shell 8×/second** (2026-07-27): the rocker auto-repeats a step
  every 120 ms, and the amixer backend costs a `fork`+`exec` of `/bin/sh` *and* of `amixer` per
  step — tens of milliseconds each on a single-core ARMv7, competing with the render thread for the
  only core. Writes are now coalesced (during a ramp only the final value matters) with a trailing
  flush so the level you stop on is always the one that lands. The mixer control name is also
  validated: it is interpolated into a shell command inside single quotes and comes from
  `cinder_volume.conf` on `/contents`, which is user-writable over USB-MSC — a stray apostrophe
  would break out of the quoting, so names outside the ALSA character set are rejected.
- **Scrobble clock follows real time** (2026-07-27): it added a flat 1 s per tick, assuming the
  caller arrives at exactly 1 Hz. Housekeeping fires when *at least* 1000 ms have passed and the
  loop runs at 10 Hz while dark, so the true gap there is 1000–1100 ms — the play clock ran up to
  10% slow exactly when the screen is off, which is how the device is normally used.
- **Battery / idle cost** (2026-07-27, audit): four things ran continuously and now don't.
  `input_pump()` does a non-blocking `read()` on **every** input node **every** loop iteration — 8
  nodes at 60 Hz is ~480 syscalls/s plus 60 thread wakeups, sustained — so the loop drops to 10 Hz
  while the panel is dark (nothing is drawn; the only input that matters then is the one that wakes
  the device). `cinder_force_dirty()` had stayed at 1 Hz *forever*, costing a full raster + 4.6 MB
  blit every second on a static screen, to guard against framebuffer scribbling that only happens
  in the first seconds of boot — now dense for ~10 s, then every 5 s. The Framework pump thread
  drops 50 Hz → 10 Hz while dark. And `poll_now_playing` no longer makes a binder round trip every
  second when nothing is playing: the URI read is gated on the PlayEventListener's callback count
  moving, so idle costs zero IPC. Changing the loop rate also exposed that the ~1 Hz housekeeping
  and battery read were paced by *iteration count* (silently assuming 60 Hz); both are now
  wall-clock paced, so the sleep timer and USB debounce keep their real timing at any rate.
- **Render: the album art was the whole cost, not the visualiser** (2026-07-28). The optimisation
  pass started from a guess that was wrong by two orders of magnitude. Measured with a new
  `cargo test -p cinder-ui --release --test render_bench -- --ignored --nocapture` harness (an
  `#[ignore]`d tool, never a gate): the **visualiser costs ~30 µs** a frame and the **album art
  behind it cost ~8,000**. A Now Playing frame was 8.3 ms on the host for a track with no embedded
  cover and 1.1 ms for one with. Three fixes, all pure — the pixels are unchanged to within 1/255:
  1. **`art::block` recomputed the gradient every frame**: a float divide per pixel for the colour
     ramp and a **`sqrt` per pixel** for the radial highlight, across 230,400 pixels. The ramp is a
     smooth function of one variable, so it is now a 512-entry table (below the eye's resolution —
     a test asserts no adjacent pixel jumps by more than 3), and the highlight contributes exactly
     nothing outside its own disc, so the `sqrt` runs only inside it. **8.1 ms → 3.3 ms.**
  2. **The gradient is now baked once per track**, into the same slot a decoded cover would occupy,
     so the render is a blit either way. It is identical for a track with artwork and one without.
     Neither bake depends on the live theme, so a Day/Night switch never rebuilds them. A test
     requires the baked image to be **pixel-identical** to the drawn one — they share one copy of
     the maths, and that test is what keeps them sharing it.
  3. **`art::draw_image` blitted pixel by pixel** through `Canvas::put`, re-checking four bounds and
     recomputing an index for each of a cover's 230,400 pixels. A new `Canvas::row_run` does the
     clip once per row and hands back a slice. **999 µs → 179 µs.**
  Net: a Now Playing frame is **~430 µs whatever the track has**, down from 1,080 (with art) and
  8,300 (without). Scaled by this project's own host↔device ratio that is roughly **16 ms → 6 ms**,
  and the no-artwork case — which would have been ~125 ms a frame, i.e. 8 fps with the CPU pegged —
  lands in the same place as every other track. The frame is now **present-bound, not raster-bound**:
  the software present measures 9.6 ms and the present thread overlaps it with the raster, so the
  ceiling is ~104 fps against a 60 fps pump. **That closes the GPU question rather than reopening
  it** — the Mali path measured 45.6 ms/present (4.7× *slower*, `FBIOPUT_VSCREENINFO` contending
  with the Mali pipeline) and making the raster cheaper cannot change that. The next real lever is
  partial repaint, which would cut the raster again but not the flip, so it is worth less than it
  looks.
- **Brick-risk sweep of the 07-27/28 work** (2026-07-28). `panic = "abort"` means any Rust panic
  kills the process, appmgr calls `android_reboot`, and the bad-boot counter takes a life — so a
  panic here is a reboot, and four frames of one are a revert to stock. Every new arithmetic and
  indexing site was checked (the accent table, the swatch geometry, the ramp lookup, the contour
  interpolation, the level statistics, `row_run`'s slice bounds, the settings parsers). What
  actually needed fixing:
  - **`gradient_image` allocated a 1.5 MB scratch `Canvas`** — the exact allocation size whose churn
    already caused one on-device allocator abort. It writes straight into its output buffer now.
  - **The Level page called `format!` twice a frame** at ~20 fps, and the Spectrum page called
    `to_uppercase()` once. All three are gone: stack decimals and a static uppercase name table.
  - **`viz_decay` did a `clock_gettime` on every frame** to discover there was nothing to decay.
    The empty check comes first now.
  - **The page swipe used `y < BOT` where it needed a range.** The shell passes `y = 0` for a
    contact that somehow arrives with no `ABS_Y`, and 0 is above the artwork — every one of those
    degenerate swipes would have silently become a page turn instead of the track skip it has
    always been.
  - **`cinder_bench`'s percentile report indexed an empty vector** on a zero-frame run. Probe-only,
    so it could never cost a boot, but an aborted diagnostic is still a diagnostic you don't get.
  - **An unresolved track URI redrew its gradient every frame** — the bake covers that path too now.
- **Now Playing is a pager** (2026-07-27): swipe the artwork left/right for three pages — **Cover**,
  **Spectrum** (the visualiser given the whole block) and **Level** (one big output meter with a
  peak marker and figures). Only the block above the title changes; the title, progress rail,
  transport and toolbar are identical on every page, so nothing moves under your thumb when you
  turn one. Dots above the title mark position — without them a swipe-only feature is undiscoverable.
  This replaces the old design where the visualiser was painted **onto** the cover, which made every
  setting a compromise between seeing the artwork and seeing the audio; as pages they stop competing
  and the visualiser gets a 348px block instead of a 42px strip. **The horizontal swipe is zoned by
  y**: on the artwork it turns the page, below it it still skips tracks, as it always did. That
  keeps both gestures with no modifier and no long-press, and it matches what the finger is on —
  you flip the picture, or you change the track. (The physical FF/REW keys skip from anywhere
  regardless, and on a device with no d-pad they are the primary skip affordance.) **Night pages
  too**, with its own geometry: the compact header stays put and the open space beneath it is what
  changes, so the gesture never has to be relearned when the theme flips. Everything there inherits
  the night palette, whose accent is already at ~55% luminance — the spectrum page at night is a
  dim spectrum, and the visualiser gets no exemption from what the theme is for.
- **Three new visualiser styles, eight total** (2026-07-27): **Ribbon** (a filled shape under a
  smooth contour — one object rather than 36 rectangles competing with the artwork), **Line** (that
  contour alone, the lowest-ink style there is) and **Pulse** (no per-band detail at all: one
  centred bar tracking overall level). They join Bars, Mirror, Segments, Dots and Wave, and the
  style applies to both the cover overlay and the spectrum page. Two defects the host previews
  caught that no amount of reading would have: Ribbon's crest drew as a dotted line because it
  stamped a 2px stub per pixel column instead of joining adjacent points — between columns the
  contour can jump tens of pixels — and Pulse was a 6px sliver pinned to the bottom of a 348px
  block, so it now scales with the box it is given and centres in it. The preview's own test signal
  was replaced as well: it alternated near-full-scale between adjacent bands, which no real music
  does, and it made every contour style look like a sawtooth. Judging a style against data it will
  never see is worse than not previewing it at all.
- **"Cover visualiser": OFF · VEIL · FULL** (2026-07-27): the size axis now governs **only** the
  cover page, and the row is named for that. Calling it "Visualiser · OFF" would have promised to
  switch off a feature that is still one swipe away. **OFF means a genuinely untouched cover** —
  not a smaller visualiser, not a relocated one. A test renders the cover twice with wildly
  different spectrum data and requires the art block to be pixel-identical, which is a stronger
  claim than counting accent pixels (the translucent sizes blend, so none of their pixels is ever
  exactly the accent colour). An intermediate design had six sizes including EDGE, FLOOR and a
  BELOW ART band; EDGE and FLOOR were two strips too alike to be a real choice, and BELOW ART was
  left fighting the progress rail for 16px once the pager made it unnecessary. Sony's analyzer is
  started for the audio pages regardless of this setting, and not at all for a clean cover page —
  a page that shows nothing costs nothing.
- **Visualiser bars fall away instead of freezing** (2026-07-27): the analyzer is demand-driven, so
  the spectrum stream now STOPS on every screen blank, pause and screen wake — and cinder-ffi kept
  drawing the last frame it received, forever, because `viz_levels` was never aged out. The visible
  result would have been a held snapshot of whatever the music was doing a second ago, on **every
  single screen wake** (housekeeping is 1 Hz, so the analyzer restarts up to a second late, plus
  service latency). A frozen snapshot presented as live is the same untruth as the synthetic
  animation that was just removed for the same reason. Frames older than 250 ms (five missed frames
  at the 20 Hz the analyzer is asked for) now decay to nothing over 400 ms and the buffer is
  dropped, so the bars fall away and then the visualiser is simply absent — the honest state when
  nothing is feeding it. Bars dropping reads as "the music stopped"; bars blinking out reads as the
  UI breaking, hence the decay rather than a hard clear. Three tests, including a one-hour frame gap.
- **`cinder_backlight.conf`'s `day=` actually wins now** (2026-07-27): it did not. `load_bl_cfg`
  parsed the value and `recompute_day_level()` overwrote it on the next line of `render_up`, so the
  documented override was dead from the moment the Settings Brightness row landed. That is the
  file's whole reason to exist — the escape for a device whose auto-detected node or
  `max_brightness` produces an unreadable panel, i.e. exactly the case where the UI you would use to
  fix it cannot be read. A pinned `day=` is now respected and logged at boot, so the Brightness row
  looking inert has a written explanation. Both shipped conf examples were rewritten: the backlight
  one no longer hands out an uncommented `day=` that silently disables the row, and the visualiser
  one no longer describes a synthetic fallback that was removed, nor tells the user to disable the
  analyzer by "changing the line to anything other than `analyzer=1`" — the check matches
  `analyzer=0`, so following that instruction would have left it running.
- **Analyzer polling and threading** (2026-07-27, audit): `viz_analyzer_enabled()` opened, read and
  closed a file on `/contents` **every second for the entire runtime of the device** — ~86k opens a
  day on the fragile vfat partition, to re-answer a question that can only change if the user
  rewrites the file over USB-MSC. Cached, and invalidated when a mass-storage session ends (the only
  moment it can change without a reboot). The analyzer shim's SIGALRM mask was a process-wide
  "once" flag, correct when the analyzer started once at boot and wrong now that it starts and stops
  on demand: if Sony hands the callbacks to a fresh thread each time, only the first one was ever
  masked and every later analyzer thread was a valid target for the shell's watchdog alarm. Now
  thread-local. The six `dlsym` lookups are resolved once instead of on every start.
- **Accent colour choice** (2026-07-27, Settings ▸ DISPLAY): six accents — AMBER (Cinder's own,
  the default), CRIMSON, VIOLET, AZURE, MINT and BONE (monochrome, the accent *is* the ink). Only
  the accent tokens move: `acc`, `acc_ink` and `row_sel`. The neutrals — the warm near-black bg,
  the panel, the hairlines, the ink — are the Cinder identity and are the same in all six, so a
  colour choice cannot wreck the design or make anything unreadable, and a test asserts it
  (`accents_change_only_the_accent_tokens`). Picking AMBER reproduces the original palette **byte
  for byte**, also pinned by a test, so a device whose owner never opens the row sees no change at
  all. Each accent carries its own hand-picked night twin, `row_sel` wash and `acc_ink` rather than
  deriving them: a blend that looks right under amber goes muddy under mint, and near-black ink
  reads differently on a bright accent than a dark one — those are contrast decisions, not
  arithmetic. The row draws **all six swatches at once** and a tap selects that colour directly.
  Cycling would have meant stepping blind through five wrong answers to see the sixth; there is
  room on a touch screen to just offer them. The swatch geometry is a single shared source
  (`settings::swatch_x`) read by both the render and `accent_hit`, and a test sweeps every pixel to
  prove the hit band never leaks into the neighbouring rows. The physical Select button still
  cycles the row. Render-only: no shell action, no Sony service, no FFI symbol — one repaint and a
  settings write. Persisted as `accent=` in `cinder_settings.conf`; an out-of-range value snaps to
  the default rather than stranding the UI on a colour the picker can't reach.
- **A–Z jump rail** (2026-07-27): an alphabet strip down the right edge of the Library list — tap a
  letter to jump. On a 304-album library the alternative is ~20 screens of flicking. Letters with no
  rows are drawn faint, so the rail doubles as a map of what the library holds, and tapping one is a
  no-op rather than a jump to the nearest neighbour. Indexes by what each tab is actually ordered by
  (Songs → title, Albums → artist when grouped else album name, Artists/Playlists → name), reading
  the same layout the render and hit test use so a jump can't land where the drawn list disagrees.
  "The Beatles" files under B; non-letters bucket under '#'. **Neither stock nor Wampy has this** —
  and Wampy structurally can't, since it drives Sony's app and never owns the list.
- **Boot to stock** (2026-07-27, Settings ▸ SYSTEM): arms a **one-shot** return to Sony's player and
  restarts. This is the **only escape reachable with no USB cable** — the other four all need one
  (cable-at-boot, `cinderhome_off`/`cinderhome_clear` over USB-MSC), and the bad-boot counter route
  requires cutting power 4× inside Cinder's ~8 s health window *and* latches permanently, so without
  a cable it was previously a one-way trip. One-shot by design: `cinderhome-launch.sh` consumes the
  flag on the boot it fires, so the boot after that is Cinder again, and it is checked **before** the
  bad-boot counter increments so a deliberate choice never spends a bad-boot life. The flag is
  written to `/data/cinder/once_stock` (journaled) **and** `/contents/cinderhome_once` (visible over
  USB-MSC, so it can be deleted from a PC); either alone is enough. No root needed for the restart:
  appmgr calls `android_reboot` when the Home app dies, so `_exit()` *is* the reboot — the flag is
  synced first. The row takes **two taps** (it shows "TAP AGAIN"), and the armed state is cleared by
  touching any other row or leaving the screen. Covered by the launcher recovery matrix (24 cases).
- **Settings scrolling** (2026-07-27): the Settings list is 919 px of content on an 800 px panel, so
  it now scrolls like the library lists. It never did — meaning **"Model" was unreachable and
  "Firmware" was clipped** before this, which a new test caught when the Boot-to-stock row pushed
  both fully off screen. `row_at` takes the scroll offset so the hit test can't drift from the render.
- **Screen-off timer** (2026-07-27): blanks the panel after 15/30/60/120 s of no input, to stop
  paying for the backlight *and* the frame. **Off by default** — an idle blank is opt-in, because a
  failed wake looks like a dead device. Three things make it safe: the auto-off path does **not**
  sleep the touch controller (unlike the Power-button path — a sleeping controller reports nothing,
  so wake-on-touch would be impossible); a waking touch is **consumed**, so waking can't also
  activate whatever is under the finger; and the physical **Power button still wakes it** regardless,
  an escape that depends on strictly less than the touch stack it rescues. Keys wake *and* are
  delivered (transport/volume work whether or not you can see the screen). **Under Hold, touches
  neither wake the panel nor count as activity** — those contacts are pocket noise (nav ignores them
  anyway), so without that the blank would be undone by the first thing the device brushes against,
  losing the saving exactly when it matters most; keys and the Hold switch itself still wake it.
  It never fires over the USB-MSC modal, which is the only indication the volume is handed to the PC.
  The render loop now also **skips painting while the panel is dark** (either cause) — with the
  visualiser running that was a full repaint + 4.6 MB blit every 16 ms, i.e. most of what the timer
  is meant to save; the wake path forces a repaint so nothing stale shows.
- **Brightness** (2026-07-27): the Settings row cycles 5 levels and writes the panel backlight,
  reusing the proven auto-detected node (the same one night dimming uses), as a percentage of its
  own `max_brightness` so it works whatever the raw scale is. Persisted, and applied at boot.
  **Level 1 is 15%, never 0** — the lowest setting reachable from the UI has to stay readable, or a
  single persisted tap leaves the Settings screen needed to undo it invisible across reboots (the
  same reasoning as the boot-always-day rule). An explicit `day=` in `cinder_backlight.conf` still
  overrides, so the file remains the escape hatch. Tests pin the cycle and the clamp.
- **Drag-to-seek**: dragging (or tapping) the Now Playing progress rail seeks. The rail geometry is
  a single shared source (`now_playing::RAIL_*`) used by the render AND the hit test, with a
  52 px grab band that deliberately stops short of the transport row so it can never steal a
  play/pause tap; a contact that starts on the rail is routed entirely to the scrub, so it can't
  also fire the horizontal track-skip swipe. *Pending one on-device verify: `SeekTime`'s
  `media_origin_t::Begin` is still assumed to be 0 (the only unverified value left in this path).*
- **VPT / DC-Phase mode**: On/Off reaches the DSP, but the **specific mode** (Studio/Club/Concert Hall,
  Standard/Low A/B) is on/off-only — the mode enum values are device-gated (TBD).
- **Power button**: toggles the **screen/backlight on/off** (panel dark, app keeps running); it does
  not trigger appmgr-owned device suspend. (Locking is the Hold switch, not Power — see Functional.)
- **Bluetooth codec apply**: the selector + device-wide preference are functional and persisted, but
  the **live `BtTransmitterService` apply** of the chosen codec is device-gated (C++ BT client shim).
- **USB-DAC → LDAC bridge**: the UI/toggle/signalling are wired; the **`ldac-bridge` daemon engaging
  on device** (capture card4 → LDAC socket) + the UAC `setprop` switch need on-device validation.
- *(moved to Functional 2026-07-26: the Playlists tab is populated and playable.)*

### ▢ Stationary (placeholder render / no action — not wired)

> **Dead-UI audit, 2026-07-27.** A full sweep of every `Screen`, every `Action`, and every drawn
> control against its hit test. Findings, worst first:
>
> **Fabricated state shown as real — FIXED in this pass.** These weren't inert, they were *lying*,
> which is worse: an inert control teaches the user it doesn't work, a false value doesn't.
> - `"WH-1000XM5"` was hardcoded as a **connected Bluetooth device** in three live places (Menu
>   caption, Bluetooth screen, USB-DAC screen), shown whenever the UI-only radio toggle was on.
>   No BT client exists, so no device was ever connected. `bluetooth::render` already had an honest
>   "No device connected" empty state; it now gets `None`.
> - Menu captions were the prototype's mock strings and read as fact: `"124 albums · 1,842 tracks"`
>   on a device with 304 albums, `"88.6 MHz"` for an unwired tuner, `"Custom A1"` whatever EQ preset
>   was selected, `"DSEE HX · VPT · Vinyl"` whatever was actually engaged. All now live
>   (`App::menu_subtitles`, extracted from `render` so the strings are unit-testable — `render`
>   only makes pixels, so a mock value creeping back could not otherwise be caught by a test).
> - Settings `"Screen-off timer 30 SEC"` and `"Brightness 3 / 5"` were invented numbers on rows that
>   do nothing when tapped, so they read as settings the user had chosen. Both now show `—`.
> - Settings `Database "REBUILD"` drew a **chevron** — this screen's affordance for "tapping acts"
>   — on a row with no handler. Chevron removed.
>
> **Genuinely inert (drawn, tappable, no effect) — unchanged, listed so it's known:**
> - Now Playing **shuffle** and **repeat** icons: `Action::ShuffleToggle`/`RepeatCycle` flip the icon
>   and `return None`. PlayerService is never told, so play order and repeat behaviour don't change.
>   Now cheap to finish: `NodeTrackSequence::SetOneTrackMode` is exported for repeat-one, and Cinder
>   already pre-shuffles queues itself for the Library shuffle bands.
> - **Bluetooth radio toggle / Disconnect**: `Action::BtToggle` maps to no `CINDER_ACT_*` at all.
> - **Bluetooth "Pair new device"**: `BtHit::Pair` is hit-tested and returns `vec![]`.
> - Settings **Database**: no arm in `settings_activate`. (**Brightness** and the **Screen-off
>   timer** are now wired — see Functional.)
>
> **Unreachable / dead plumbing:**
> - `pairing.rs` renders a complete pairing screen, but **there is no `Screen::Pairing`** — it is
>   reachable only from the host preview harness and the sim. Designed, not wired.
> - `Screen::Fm` and `Screen::Receiver` have no `tap()` branch (they fall to `_ => vec![]`); only
>   Back does anything. `Screen::Fm` also renders a hardcoded `88.6`.
> - `NowPlaying.liked` is threaded through four crates and `icons::heart` exists, but the heart is
>   **never drawn** — an unfinished favourites feature, left in place rather than deleted.
> - FFI exports the shell never calls: `cinder_set_now_playing` (superseded by `_uri`),
>   `cinder_set_theme_night`, `cinder_set_visualizer`, `cinder_set_visualizer_type`,
>   `cinder_visualizer_count`, `cinder_set_pcm` (the analyzer path uses `cinder_set_spectrum`).
>
> One transient worth knowing: `App` starts with `Library::sample()` (6 demo albums), replaced when
> `cinder_db_open` runs in `deferred_up`. A DB failure substitutes an **empty** library, so the demo
> data can never stand in for the user's music — but it is briefly live before the DB loads.

- ~~Play a selected track/album~~ → **WIRED (2026-07-03, awaiting device verify)**: tapping a
  Songs row / Album track / "Play album" band resolves the album context through the DB and hands
  PlayerService a real `NodeTrackSequence` (see the eighth-round notes below). Playlist rows play
  too, since 2026-07-26.
- **Bluetooth radio on/off**: the toggle flips **UI state only**; it doesn't power the radio
  (BtTransmitterService not wired). *(The codec selector beneath it IS functional — see Functional.)*
- **FM Radio** screen: static (88.6 MHz placeholder).
- **BT Receiver** screen: static (off).
- **Pairing** screen: static (the "Pair new device" button is inert).
- **Settings (info/placeholder rows)**: Screen-off timer, Database "REBUILD" take no action. The
  manual **Brightness** slider row is still static (but night-mode backlight dimming IS wired — see
  Functional). **USB mode** row now enters mass-storage **with a modal UsbStorage screen and a clean
  log-fd handoff** (fds → /dev/null before `umount /contents`; Back or unplug remounts + restores
  the log — device-gated `setprop`, validate live). Firmware & Model
  are **honest static info labels** — Firmware reads `CINDER 1.0` (stable) / `CINDER DEV` (dev channel).
- **Now Playing heart** (like) + the library **shuffle-by-album/artist** rows: decorative — no
  on-device action yet. *(Shuffle/repeat on the transport row ARE tappable now — see Functional.)*

---

## Build channels — `stable` (default) and `dev` (same tree, one flag)

```bash
cd cinder-home
bash build.sh            # = build.sh stable  → dist/stable/   (lean player, no adb)
bash build.sh dev        #                    → dist/dev/      ("CINDER DEV" marker + self-enables adb)
bash tools/pack_upg.sh stable      # packs install/uninstall .UPG into dist/stable/
bash tools/pack_upg.sh dev         #                                  dist/dev/
```

Both are built from this one source tree; the only differences:
- **stable**: Settings ▸ Firmware reads `CINDER 1.0 · RUST`. No adb. This is the daily-use build.
- **dev**: cargo `dev` feature flips the marker to `CINDER DEV · RUST` (so you can tell them apart
  on the device), and `-DCINDER_DEV` makes the dev binary **enable adb at boot** (in `deferred_up`,
  behind `run_guarded`, best-effort): `setprop sys.usb.config mtp,adb` + `persist.sys.usb.config` +
  `start adbd`. It touches **no** boot-critical files, so a failure just means "no adb, runs like
  stable". `persist.sys.usb.config` also brings adb up early on later boots → an independent
  **brick-recovery channel** (`adb shell touch /contents/cinderhome_off` reverts without wbrt).
- Artifacts never clobber: `dist/stable/` vs `dist/dev/`, each with its `cinder-home`, `cinder-probe`,
  and the (channel-agnostic) install/uninstall `.UPG`s.

**Dev iteration loop** (after the first dev flash brings up adb): push-and-run, no `.UPG` reflash —
`adb push dist/dev/cinder-home /system/vendor/unknown321/bin/cinder-home` (remount rw first) then
relaunch. The exact `setprop` mechanism is confirmed on that first flash; if adb doesn't appear,
adjust the one `std::system(...)` line in `src/main.cpp` (`#ifdef CINDER_DEV`).

**Device discovery — one run unblocks every device-gated feature.** A read-only dump captures, in a
single pass, the facts that block volume / play-by-index / progress / keymap / USB-DAC: the amixer
master-volume control name + range, ALSA topology, backlight + charge sysfs, USB-gadget config, the
input keycodes, and a live PlayStatus byte dump. It is **roped into the dev channel two ways**:
- **The dev player gathers it for you.** On first boot the dev `cinder-home` auto-writes
  `/contents/cinder_discovery.txt` (read-only, guarded), and the dev input pump **logs every raw key
  code** to `cinderhome.log` (press each button → read off the keymap). Stable does neither.
- **The standalone probe** (`dist/<channel>/cinder-probe --discover [outfile]`) does the full run
  *including* the interactive 12 s keymap capture — best run over adb before flashing the Home app.

Then: `adb pull /contents/cinder_discovery.txt` → it has the control names/offsets to wire volume,
play-by-index, seek-accurate progress, the keymap, and the USB-DAC path. (Built from `src/discover.cpp`,
shared by both binaries.)

> `tools/flash.sh` **is in the tree and working** (detect over MSC/usbipd → push binary / mount+read
> logs / send `.UPG` via `scsitool do_fw_upgrade`). Use it directly; the older "not in the tree yet"
> note was stale.
>
> **First-flash learning (2026-07-01): the installer must not use the updater's ambient shell
> tools.** The very first Home flash aborted at the binary-size sanity check — the updater's bare
> `wc -c` returned `0` on a *good* 2.6 MB copy (and `rm -f` choked on its own flag), so a healthy
> install false-aborted (correctly making **zero** changes → clean boot to stock). Fix: `deploy/
> install_cinderhome.sh` + `uninstall_cinderhome.sh` now route **every** updater-time op through
> `/xbin/busybox` (the anchor Wampy relies on), with a `/system/xbin/busybox` → bare fallback; the
> size check measures via busybox, cross-checks against the source size, and only aborts on a
> *measured* short file (an unmeasurable size proceeds, since `-s` already proved non-empty). The
> brick-critical `.appcfg` write no longer rides on the flaky tools. Re-`pack_upg.sh`'d both channels.
>
> **Second on-device learning (2026-07-01): the non-Qt CuiAppModule pump needs a driver.** With the
> installer fixed, cinder-home installed and **launched as the Home app** (cleared the whole easel
> lifecycle → OnForeground → `render_up` DONE), then **hung in `std::condition_variable::wait`**
> (watchdog `sig=14`, no `cb:pump`). Cause: easel's `CuiAppModule` pump loop blocks on a CV until
> `CuiAppModule::OnPumpTrigger()` notifies it; the stock **Qt** app drives that from Qt's event
> loop, but our non-Qt module had no driver, so the loop waited forever (renderer up, never ticked).
> Fix (`src/main.cpp`): a lightweight **ticker thread** pokes `OnPumpTrigger()` (exported `T` in
> libeaselcui) at ~30 fps. Rendering still runs on the **main** thread inside the pump loop, so the
> per-frame watchdog + `run_guarded`/siglongjmp stay single-threaded; the ticker blocks SIGALRM and
> only does the mutex/flag/notify, and is joined at `OnFinalize` before the module is destroyed.
>
> **Third on-device learning (2026-07-01): the OnPumpTrigger poke wasn't enough — pivot to our own
> render loop.** Thumb-disasm of libeaselcui showed `CuiAppModule::OnForeground`'s 2nd sub-call
> (`[this+0x60]+0x18`) blocks on the module CV driven by Sony's `pst::core::JobQueue`/event-mux —
> infrastructure only **libeaselqt** runs, and `CuiAppModule` has **zero other users on the device**
> (every Sony app, incl. the stock Home app, is Qt). So we stopped fighting it: `render_up` opens the
> framebuffer, then a **`render_driver` worker thread owns the whole frame loop** (paint + touch/button
> input + deferred DB/audio/adb init + housekeeping) while the easel main thread stays parked in its CV.
> **This works and is STABLE on device** — appmgr tolerates the parked main thread (no reboot). SIGALRM
> (per-frame watchdog + `run_guarded`) is routed to the worker; `render_up` blocks it on the main thread
> so guard alarms can't mis-fire there. **Verified on device (2026-07-01):** launches as Home, paints
> full-screen, library loads (3746 tracks), audio connects, `mark_healthy` clears the bad-boot counter.
>   - **Boot-anim overlay:** the stock flow never fires because we bypass the Qt path, so the worker
>     calls Sony's `StopBootAnimation()` (fork+execlp, no framework dep) at first paint + re-issues it
>     after the deferred init, with ~0.5 s of warm-up paints first (the render-only build cleared it that
>     way). *(bootmode=2 is NOT the lever — that's diagnostic mode + remounts /contents.)*
>   - **Touch:** the panel is `himax-hx8526-icx` (event1) — reports `BTN_TOUCH` **and** type-A MT
>     (no TRACKING_ID). Critical gotcha found in the discovery dump: `m_batch_input` (event3) also emits
>     `ABS_X/ABS_Y` (sensor), so input **must be gated to the real touchscreen fd** or the sensor stream
>     overwrites finger coords. `input_pump` now gates to `g_touch_fd` and handles BTN_TOUCH **and**
>     type-A empty-frame lift; the dev build logs the first ~60 raw events for confirmation.
>   - **Still device-pending:** confirm touch responds, confirm the rectangle clears, and adb
>     enumeration (config is `adb` but the host didn't see it — a Windows/usbipd side issue, not ours).

> **Fourth on-device learning (2026-07-02, three boots): boot-anim display layer + silent touch —
> both cracked.**
> 1. **Stale boot pixels have TWO distinct mechanisms.** (a) *Primary-page scribbles*: the
>    dirty-flag renderer never repaints after its first blit (nothing is playing at boot), so any
>    frame the boot video wrote into the fb pages after that blit persisted — fixed with
>    `cinder_force_dirty()` forced repaints (every frame for ~10 s, then 1×/s for life). (b) *A
>    latched OVERLAY*: `icx_bootanimation` composites on an MTK display layer ABOVE the primary
>    fb (our blit writes all 3 pages, yet a full-screen boot logo can still cover the UI). SIGTERM
>    in its steady video-loop state cleans the layer (stock handover timing = our first-paint
>    `StopBootAnimation()` — verified clean); an **early kill (main() start) hits it before its
>    cleanup handler exists → hard death → full-screen logo latched forever over a fully-working
>    UI** (observed on device; touch events flowed underneath). **Never kill the boot animation
>    early** — first-paint + post-deferred re-issue only.
> 2. **Zero input events → fixed; the held EVIOCGRAB is the likely cure.** Two prior boots read
>    nothing from 8 open nodes; the boot after `input_open` started **holding the EVIOCGRAB on
>    the touchscreen**, real himax type-A frames flowed (`ev fd7*TS` in the dev log). Our grab
>    succeeded and the dev /proc scan showed no other holder *at open time* — consistent with a
>    Sony daemon grabbing the node a little later and silently diverting the stream; holding the
>    grab locks that out. Belt+braces from the Wampy source (`artifacts/repos/wampy`,
>    `hagoromo.cpp enableTouchscreen()`): the himax driver has a sysfs sleep switch the stock app
>    normally drives — cinder-home now writes `0` (wake) to
>    `/sys/devices/platform/mt-i2c.1/i2c-1/1-0048/sleep` (A50 family; `1-0020` = WM1Z) in
>    `input_open`, and ties sleep/wake to the Power screen-toggle like stock. Node names
>    (EVIOCGNAME), per-node grab probe, /proc holder scan (dev), read()-error logging and the
>    "still ZERO events" heartbeat all stay in.
> 3. **Same-day sweep while flashing:** library **tap/select hit-testing rewritten** to mirror the
>    render exactly (`library::hit_row`/`song_at`/`album_hit_track` + regression tests): the Songs
>    tab was tapped in **DB order while drawn in sorted order** (wrong song every time sort ≠ DB),
>    the Albums tab ignored the 30 px artist headers (wrong album below the first group), and the
>    album drill-in rows were offset 16 px. Hardware **volume defaults baked in** (see ◐ Volume
>    keys), boot **seeds the UI level from the mixer**, and a screen-off mid-touch no longer leaves
>    a stale contact (state reset in `screen_toggle`).
> 4. **Gesture vocabulary (from the first real user session):** boot was clean and touch frames
>    flowed, but the user "couldn't click through the set up screen" — the raw log shows the first
>    real gesture was a **horizontal swipe** (a paged intro invites swiping) and the classifier had
>    no horizontal-swipe gesture at all. Added `cinder_swipe(dir)`: **Onboarding pages** (left =
>    next/finish, right = back), **Now Playing skips track**; tap tolerance loosened 18→26 UI px
>    (sloppy thumbs read as micro-drags). Also: Wampy's himax sleep-node paths **don't exist on
>    this fw** — added an `/sys/bus/i2c/devices/*/sleep` scan fallback, and since the controller
>    may stay awake, `input_pump` now **drops touch events while the screen is off** (Power
>    toggle) so taps can't navigate invisibly. Dev raw-event log cap 60→200.

> **Fifth on-device learning (2026-07-02 evening): EffectCtrlDmp was 0xA8 bytes, not 8 — heap
> corruption on the first saved-EQ re-apply; and SIGABRT must never be "recovered".**
> 1. **The corruption:** `EffectCtrlDmp`'s ctor (@0xdd40) writes this+0/this+4 **and then
>    `memset(this+8, 0, 0xA0)`** — the first RE pass missed the memset and sized it ~8 bytes
>    (0x10 reserved). The first on-device construction (boot-time saved-EQ re-apply — the path
>    had NEVER run before, EQ was untouched until touch worked) zeroed 152 bytes of neighboring
>    heap → `malloc(): memory corruption (fast)` abort. Fixed: `kEffectCtrlDmpRealSize = 0xA8`,
>    reserve 0x100. **PowerMgrServiceClient re-verified genuinely 8 B** (ctor disasm: writes
>    this+0 vtable ptr, this+4 only); player shim only uses Sony-allocated objects.
>    **The qemu preflight now constructs EffectCtrlDmp + PowerMgrServiceClient under canaries**
>    (negative-tested: the old 0x10 size FAILS the gate; 0x100 passes) — this bug class is now
>    caught offline, before any flash.
> 2. **SIGABRT fail-fast:** glibc raises SIGABRT from *inside malloc with the arena lock held*;
>    the guard's siglongjmp "recovery" left that lock held → the next allocation (the very next
>    guarded call) deadlocked silently, the watchdog's own handler also allocates → wedged dark
>    (observed: log ends mid "re-apply saved sound"). `fault_handler` now treats SIGABRT as
>    always-fatal via async-signal-safe `write()` + `_exit(42)` (→ reboot → counter);
>    `backtrace()` is pre-warmed at install so the SIGALRM path doesn't allocate either.
> 3. **Boot-anim kill timing (the "hit and miss" freeze):** ~~first-paint (~2 s) is a coin flip on
>    whether icx_bootanimation has installed its SIGTERM cleanup handler~~ — **superseded by the
>    Sixth learning below** (the anim has no signal handlers at all; the freeze was never about
>    kill timing).

> **Sixth on-device learning (2026-07-02 night, disasm of `xbin/icx_bootanimation`): mtkfb never
> shows what you write — every frame must be pushed with an ioctl. THE root cause of every
> frozen/invisible-UI boot.**
> 1. **The mechanism:** mtkfb does **not** scan the framebuffer continuously. Pixels reach the
>    panel only when a process calls `FBIOPUT_VSCREENINFO` with `activate |= FB_ACTIVATE_FORCE
>    (0x80)` — icx_bootanimation's per-frame flip (disasm @0x1fae: `orr activate,#0x80; ioctl
>    0x4601`). Our renderer was a pure memcpy into the mmap — **our UI has never once been pushed
>    to the glass by our own code.** Every "working" boot was the anim's own flips happening to
>    push framebuffer pages we had just written; every "frozen boot image" was the glass latched
>    on whatever frame was pushed last before the anim died. This also explains "touch didn't
>    work": input events WERE flowing and the navigator WAS responding (per the logs) — the
>    screen just never updated to show it.
> 2. **The anim has NO signal handlers** (no `signal`/`sigaction` imports) — SIGTERM drops it
>    dead at any point; there was never a "cleanup handler" or a safe kill window. All kill-timing
>    tuning (first-paint vs post-deferred, Fourth/Fifth learnings) was superstition on top of the
>    missing-flip bug.
> 3. **The fix:** `Framebuffer::blit` (cinder-ffi) now ends every blit with the exact trigger
>    sequence the anim uses (offsets pinned 0, `activate|=FORCE`, `FBIOPUT_VSCREENINFO`), plus the
>    same init sequence at open. Boot-anim kill is back at **first paint** (fast takeover,
>    deterministic — our next flip owns the glass), with the post-deferred re-kill + ~15/30 s
>    sweeps kept as respawn insurance. The fb geometry + "flip-on-blit active" line and a one-time
>    "fb flip ioctl FAILED" diagnostic are logged so a silent regression is visible in
>    cinderhome.log. Dirty-flag gating unchanged: idle = no blit = no flip = zero cost.

> **Seventh learning + feature round (2026-07-02, after the first *working* interactive boot):
> feedback was "slow, small text, MSC bugs the device" — all three addressed offline.**
> 1. **60 fps pump.** `render_driver` now sleeps 16 ms/frame (was 33 ms); all `n`-based cadences
>    (housekeeping 1×/s, battery 10 s, force-dirty windows, straggler sweeps) rescaled to keep
>    their wall-clock timing. Dirty-flag still gates the blit, so idle frames stay ~free; overlay
>    frame budgets (volume HUD / toast / queue chip) retuned for the 60 Hz tick.
> 2. **Text bumped +1 px across the lists** (library/up-next/menu/settings: titles 15→16, subs
>    11→12, mono captions 10→11, tabs 11→12) for readability on the 4.4" panel.
> 3. **USB mass storage root cause: OUR OWN LOG FD.** Stock's `sys.sony.config=msc` runs
>    `unmount_msc1` = `umount /contents` first — and the launcher redirects our stdout/stderr to
>    `/contents/cinderhome.log`, so the umount got EBUSY and MSC silently wedged ("mass storage
>    bugs it"). Fix mirrors cinder-device: `enter_usb_msc()` dup2's fds 1+2 → `/dev/null` *before*
>    the setprop; a modal **UsbStorage screen** blocks the UI while the volume belongs to the PC;
>    Back (or cable unplug, watched 1×/s once the cable was seen) emits `ExitUsbMsc` → shell sets
>    `sys.sony.config=adb` (stock's boot default; its init block runs `mount_msc1`), waits ≤5 s
>    for the remount, then points the log back at `/contents/cinderhome.log`. Scrobble writes are
>    gated off while MSC is active (stale mountpoint).
> 4. **Spotify-style swipe-to-queue.** `cinder_swipe(dir, x, y)` now carries the gesture START
>    point; a rightward swipe on a Library-Songs/Album-track row queues that row (same hit-test
>    the tap uses), pops an "Added to queue — …" toast + a "+ QUEUED" chip that slides off the
>    right edge (~0.4 s), and Up Next shows the user queue ahead of the album window. Edge-back
>    is untouched: the shell classifies left-edge→right as Back *before* the swipe branch, so
>    both rightward gestures coexist. (Queue is display+intent; making PlayerService honor it
>    needs `PlayController::SetTrackSequence` RE — same gate as play-by-index.)

> **Eighth round (2026-07-03): play-a-selected-track WIRED, album covers, bigger text round 2,
> Play-album tap, adb guide. All offline-verified (62 tests + qemu preflight); device verify next.**
> 1. **Play a selected track/album — the big gap — is code-complete.** Tap a Songs row, an Album
>    track, or the "Play album" band → `Action::PlayIndex(object_id)` → cinder-ffi resolves the
>    track's whole album in play order (`cinder-db::album_context`) into `cinder_pending_play_*`
>    → the shell (guarded, 10 s) calls the new `cinder_audio_play_tracks(uris, count, start)` →
>    builds the Node-tree JSON (schema RE'd from `ConvJsonToNode` @0x10631: `{"uri":<abs path>,
>    "format":<int>,"children":[…]}`), maps each path through Sony's own
>    `psk::FileUtil::GetFormatFromFilename` (mp3=2, flac=9 confirmed under qemu), parses it with
>    `NodeJsonUtil::ConvJsonStringToNode`, constructs `NodeTrackSequence<UriInfo>` (0x100
>    reserve; single primary vtable at +0 → aliasing-shared_ptr upcast, no adjustment), then
>    `SetTrackSequence` + `ChangePlayState(Play)`. The shim pins the sequence (`g_seq`) because
>    the service PULLS tracks from it during playback. Link needed a hand-written D1→D2 dtor
>    forwarder for `Node<UriInfo>` (the lib exports only D2). **The qemu preflight now constructs
>    the whole JSON→Node→NodeTrackSequence chain with guard canaries** — ABI regressions in this
>    path can't reach the device.
> 2. **Album covers (real art).** New `cinder-ffi/art_load.rs`: resolves art via
>    `images.bmpfile` (pre-rendered BMP) or the embedded blob at `value`+`dataoffset`/`datasize`
>    (JPEG/PNG — zune-jpeg + png crates, pure Rust, 2.23-clean; hand-rolled 24/32bpp BMP reader).
>    Decoded ONCE per track change, pre-scaled (bilinear) to 480×480 full-bleed + 92×92 thumb, and
>    blitted by Now Playing (day full-bleed / night thumb). Decode failure or no DB row → the
>    existing gradient placeholder, so this can't regress the UI. Caps: 16 MB blob, 2048² decoded
>    (PNG header gated BEFORE allocation). **Unverified on device:** whether the stock DB fills
>    `bmpfile` or the offset pair — `adb pull /db/MTPDB.dat` (docs/adb_setup.md §3) settles it.
> 3. **"Play album" band now plays** (it was render-only): tap → PlayIndex(first track). Album
>    drill-in (Albums tab → track list under the album header) verified wired for touch + tests.
> 4. **Text bigger round 2:** list titles 16→18 (menu 17→18), subs 12→13, settings values 11→12.
> 5. **adb for iteration + RE: docs/adb_setup.md.** Dev channel already self-enables adb at boot;
>    the guide covers Windows platform-tools, WSL (ADB_SERVER_SOCKET or usbipd), the push→verify→
>    swap loop (reboot, never kill), live log tailing, and the RE pulls (MTPDB.dat first).
> 6. **Audit:** clippy = style-only across the workspace; fixed real hazards in the new code
>    (PNG pre-alloc header gate, decode dimension caps, pending-play URI width 512).

## STEP 2: flash the Home app (only after STEP 1 looks clean)

From `/home/sony/sony`, with the Walkman plugged in (paths shown for the **stable** channel; for the
dev build swap `dist/stable/` → `dist/dev/`):

**Push all three binaries.** The installer stages each from the storage root
(`/contents/cinder-{home,umount,gpunode}`); a missing helper does not abort the install, it just
warns and silently degrades — no `cinder-umount` means USB-MSC falls back to the path that
**cannot unmount `/contents` as uid 100**, and no `cinder-gpunode` means the GPU path can never be
enabled. Both helpers install setuid-root (mode 4755).

```bash
tools/flash.sh --push cinder-home/dist/stable/cinder-home          # the player
tools/flash.sh --push cinder-home/dist/stable/cinder-umount        # setuid helper: MSC unmount
tools/flash.sh --push cinder-home/dist/stable/cinder-gpunode       # setuid helper: GPU device nodes
tools/flash.sh cinder-home/dist/stable/cinder_home_install.upg     # install (repoints the Home app)
# Power on and let it boot — WITH THE CABLE UNPLUGGED. A cable connected at boot is itself the
# escape to stock (restored 2026-07-26). For cable-heavy dev, opt out once:
#   adb shell 'mkdir -p /data/cinder && touch /data/cinder/cable_escape_off'
```

**The recovery model (rebuilt 2026-06-26; latch fixed 2026-07-26; state moved off `/contents`
and the cable escape restored 2026-07-26 after a brick).** Full ladder in
[`../RECOVERY.md`](../RECOVERY.md); the short version:

- **Escape 0 — boot with the USB cable connected → stock.** Depends on nothing: no filesystem, no
  shell, no counter. Restored 2026-07-26 (it had been removed on 07-25) because the brick left
  *every* file-based escape unreachable. Cost: charging at boot also lands on stock — opt out with
  `/data/cinder/cable_escape_off` or `/contents/cinderhome_cable_off` for cable-heavy dev sessions.
- **Escape 1 — the bad-boot counter** reverts to stock after **4** boots that don't reach
  "healthy". **If it sticks on the boot screen: force-reboot (hold Power ~8 s) four times.**
- cinder-home clears the counter **~8 s after its first painted frame**. It used to wait for the
  whole `deferred_up()` feature-init chain **plus 25 s** — up to ~170 s on dev, and that chain
  blocks the render thread — so a reboot inside that window left the counter set. With the old
  `MAXBAD=2` that meant two impatient reboots latched the device to stock **permanently**.
- **State lives on `/data/cinder/` (ext4), not `/contents`.** `/contents` is vfat *and* is the
  partition unmounted for USB-MSC, so it is both corruptible and routinely absent. On 2026-07-26 it
  stopped mounting: the counter write went nowhere, the launcher's `>/contents/cinderhome.log`
  redirect failed so `sh` **exited without exec'ing**, appmgr rebooted, and the device looped on the
  logo with the safety net silently disabled — wbrt was the only way out. Now: the launcher runs
  stock if `/contents` isn't mounted, **refuses to run at all if it cannot persist the counter**,
  and the log redirect can never block the exec.
- **Manual escape:** create `/contents/cinderhome_off` over USB-MSC and reboot → stock.
- **Un-latching after an auto-revert:** `tools/flash.sh --clear-latch` (arms
  `/contents/cinderhome_clear`, which the launcher consumes on the next boot), **or just install a
  newer cinder-home binary** — the launcher self-heals when the binary is newer than the latch.
- ⚠️ **adb cannot recover a stock-latched device**: dev-channel adb is enabled inside
  `deferred_up()`, which never runs under stock. Recovery goes through the cable escape or USB-MSC.
- **`tools/test_launcher.sh` gates the build** — 18 sandboxed scenarios covering every escape and
  failure mode. `build.sh` refuses to pack if any fail.
- Inside cinder-home, a crash/hang in the library or PlayerService is *caught* and that subsystem
  is skipped — worst case the UI runs without audio/library, not a hung boot.

To read the log (boot to stock, plug in):

```bash
tools/flash.sh --cat cinderhome.log
```

To revert deliberately:

```bash
tools/flash.sh cinder-home/dist/stable/cinder_home_uninstall.upg   # restores the stock .appcfg
# or, no PC: create /contents/cinderhome_off over USB-MSC and reboot
# (wbrt is the last resort only — the counter + guard should make it unnecessary)
```

## Verify it

`cinderhome.log` should now progress **past** the old crash point and show the lifecycle
running. Look for, in order:

```
[cinder-home] main: start
[cinder-home] main: calling app.run()
[N| …|EASL|LifeCycleManager.cc:91] start ToInitialize
[cinder-home] app:OnForeground            <-- past the old crash point
[cinder-home] render_up: cinder_render_init
[cinder-home] render_up: DONE (renderer ready)
[cinder-home] pump: first frame painted          <-- screen is alive; boot screen cleared
[cinder-home] deferred_up: cinder_db_open + build library
[cinder-home] cinder-ffi: library loaded — NNN tracks, MM albums, KK artists
[cinder-home] deferred_up: cinder_audio_init (PlayerService connect)
[cinder-home] deferred_up: DONE
[cinder-home] healthy: bad-boot counter cleared  <-- proven good; won't auto-revert
```

Key new diagnostic lines:
- `pump: first frame painted` — render works; if you see this, cinder-home itself is fine and any
  later problem is a *subsystem* (DB/audio), not a crash.
- `GUARDED CALL FAULTED — skipping that subsystem, UI continues` — a DB/audio call crashed or hung
  and was skipped; the UI stays up. The PC on that line tells us which call (send it to me).
- If `healthy: bad-boot counter cleared` never appears, the boot didn't stabilise → it will
  auto-revert.

On the **panel** you should see the Cinder UI painting (Now Playing). If the screen paints but
buttons do nothing, that's the **keymap** (next section), not a crash.

If it crashes, the log ends with `*** FATAL SIGNAL : PC=0x… ***` + a backtrace + `/proc/self/maps`
— paste that; the PC minus the library's map base gives the exact function (same method that
found the sizing bug).

## Tune the input keymap (likely needed on first boot)

The defaults now ship the **real NW-A50 codes** (ninth round; source: wampy `glfw.patch` —
play=28, next=106, prev=105, vol+=115, vol−=114, power=116, hold=35), so out of the box the
side buttons are global transport and no calibration should be needed. If a unit still
disagrees, override without a rebuild:

1. Get a shell (adb, or stock + a terminal) and run `getevent -lt /dev/input/event*`, pressing
   each physical button; note the **device** and the **decimal key code** for each. (The dev
   channel also logs `input: KEY code=…` for every press — no shell needed.)
2. Create `/contents/cinder_keymap.conf` (over USB-MSC) with one `rawcode button` per line, where
   `button` is: `0`=Up `1`=Down `2`=Left `3`=Right `4`=Select `5`=Back `6`=Option `7`=Play
   `8`=Home `9`=VolUp `10`=VolDown `11`=Power `12`=Hold `13`=Next `14`=Prev. Example:
   ```
   # rawcode  button
   115 9      # vol+  -> VolUp
   114 10     # vol-  -> VolDown
   106 13     # FF    -> Next (global next track)
   105 14     # REW   -> Prev
   28  7      # play  -> Play
   35  12     # hold switch
   ```
3. Reboot. The pump logs `input: applied /contents/cinder_keymap.conf overrides`.

---

## RE follow-ups — mostly UNBLOCKED offline (see `analysis/RE_playerservice_sound.md`)

The 2026-06-26 offline RE pass found the mechanism for all of these (full detail + symbols +
offsets in `analysis/RE_playerservice_sound.md`; ABI capture in
`cinder-audio/src/effect_abi.hpp`). What remains is wiring + small on-device confirmations:

1. **Play a selected track/album** (`Action::PlayIndex`) — **DONE 2026-07-03** (eighth-round
   notes above): JSON schema + format enum RE'd, `cinder_audio_play_tracks` implemented, wired
   end-to-end, qemu-preflighted. Remaining: on-device verify only.
2. **EQ + all Sound effects → DSP** — **complete API** in `libEffectCtrlDmp.so` (`EffectCtrlDmp`,
   default ctor): `SetEq10BandValue`, `SetDseeHx`, `SetVpt`, `SetVinylizer`, … and
   `SetBtAudioSoundEffect(bool)` (= effects-on-Bluetooth, goal #7). Build `effect_shim.cpp` over it.
3. **Now Playing progress + track-change** — **PlayEventListener vtable mapped** (`onPlayTimeUpdated(cur,tot)`
   @slot+0xc gives position/duration). Implement the listener, pass it to `Connect()` (event-driven,
   battery-efficient).  (`PlayStatus.uri@+0x6c` already confirmed.)
4. **USB-DAC → LDAC** (headline) — entry point found: `BtPlayerServiceClient::SetLDAC` +
   `BtPlayerService::LdacWriteSound()` (PCM write) + `BtTransmitterServiceClientFactory`. Needs the
   USB-PCM tap + E4/E5 ALSA topology on device.
5. **Volume** — CXD3778GF **"master volume"** (ALSA mixer on card0 / `/sys/module/snd_soc_cxd3778gf/`).
   Confirm the exact control name with `amixer scontrols`; check `getevent` whether the vol keys
   even reach userspace.

**Wire each behind cinder-home's `run_guarded` guard + reserve object sizes; validate with
`cinder-probe` isolation before any boot-path use.**

## Where things live

| Piece | Path |
|---|---|
| easel shell + pump + input/keymap + crash/hang GUARD | `cinder-home/src/main.cpp` |
| standalone diagnostic (zero boot risk, adb) | `cinder-home/src/probe.cpp` → `dist/<channel>/cinder-probe` |
| launcher recovery (counter + escape window) | `cinder-home/deploy/install_cinderhome.sh` |
| guard recovery self-test (host) | `cinder-home/tools/guard_selftest.cpp` (run by build.sh) |
| device-class ABI (sized!) | `cinder-home/src/easel_abi.hpp` |
| build + gates | `cinder-home/build.sh`, `cinder-home/tools/{preflight_qemu,pack_upg}.sh` |
| flashable artifacts | `cinder-home/dist/<channel>/` |
| Rust UI (screens, nav, model, overlay) | `player/cinder-ui/src/` |
| C-ABI render/input/scrobble bridge | `player/cinder-ffi/src/` (`lib.rs`, `scrobble.rs`) |
| library DB reader | `player/cinder-db/src/lib.rs` |
| playback control shim | `cinder-audio/src/` |
| host PNG preview | `cd player && cargo run -p cinder-host` → `player/out/*.png` |
| **interactive sim** (real nav, keyboard) | `cd player && cargo run -p cinder-sim` (WSLg shows the window; arrows/Enter/Backspace/Tab/Space/`=`/`-`/H/P, Q quits) |
