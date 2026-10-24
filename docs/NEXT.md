# Next — the one list

**Every open next step, to-do and device check across Cinder, Flint, cinder-themes and
cinder-sony-analysis, as of 2026-10-24.** This file is the order, not the procedure. Device rows are
run from [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) (safety rules first) and recorded there;
feature state lives in [`../cinder-home/STATUS.md`](../cinder-home/STATUS.md). When something here is
done, delete its line and record the result where its procedure lives. Earlier versions of this
file (battery research in full, Walkman One parity, the 0.3.17 release plan) are in git history.

## Where things stand

| | |
|---|---|
| Cinder | **v0.3.19** is the Latest release (10-20): the PolyForm Noncommercial licence, the installer writing the package Walkman One expects, Bluetooth AAC and SBC XQ as installer options, Simplified Chinese and installer language choice. 0.3.17 (audiobooks, presets, NC with Sony's NC earphones) and 0.3.18 (SD-card playlists) shipped before it. **v0.3.20-rc1** is the pre-release (10-23): everything since 0.3.19, including #30 (receiver AAC/LDAC), #31 (track previews), the guided installer and the Wampy-parity options. Most of it is host-tested only; the device rows below are what promotes it. Since the rc (10-24): the move to `/system/vendor/cinder` (the owner's player already runs from it), Sony's Bookmark Lists, and eight fixes seen on the player |
| Flint | **v0.2.1** is the Latest release (09-28). **v0.2.3-rc3** is the pre-release (10-23; rc2's tag has no release, its Windows build failed on CRLF contract files). Committed since (10-24): swap, backup/restore, the health card, the tag check, lyrics, and the window matching the command line (`window_parity` test) |
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
| 1 | ~~**Flint F1, the rest**~~ | **Decided and built 10-23** (owner: ask in the plan): Flint sweeps only what it wrote; the rest is listed unticked | Flint `docs/AUDIT_2026-10-01.md` Part F |
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

1. ~~**Say what costs battery.**~~ **Done 10-23**: effect rows that are on read "On · costs battery";
   the mode line says the chain is skipped (Direct) or running (Custom, about 11 % more drain).
2. ~~**Presets, round two:** names and more than four.~~ **Done 10-23**: named on Save and by RENAME, up to 12, the list scrolls. zh-CN translated 10-23 from glyphs the font has; a native read wanted.
3. ~~**A Battery preset**~~ Not wanted (owner, 10-23).
4. ~~The redesign plan's Part G still describes per-output profiles~~ **Done 10-23**: Parts B, G and the settings keys now describe the A/B compare and Presets.

**Cinder — standalone: everything Wampy and Walkman One do, as installer options**

The goal: a stock player gets every feature of Wampy and Walkman One from Cinder's installer, one
option at a time, with no Wampy or Walkman One code, files, downloads or installs. People add what
they want as they go. Licences: Wampy is GPLv3 + Commons Clause and Walkman One is closed, so
rebuild from documented facts, never copy, and keep anything GPL in a repo of its own. The README
and release notes describe Cinder's options, not "a replacement for" either project.

Already done: volume curves fetched from Sony (`voltable`), sound signature (`signature`), tone
table, region cap, FM on any firmware, and the 10-band EQ and Tone Control on stock (`preload`,
device-unverified). Walkman One's external tunings hold nothing audible
(`analysis/RE_walkmanone_installers.md`), and the owner's player has none applied (`p3` is its own
factory image, `bc41b677…`, checked 10-22).

**A. Owe them nothing.** First, because the other stages build on it.

1. **`preload` on by default.** Gate: DEVICE_CHECKLIST §35 on a player other than the owner's,
   stock 1.02 with no Wampy. Then the Bluetooth codec options stop needing a second tick.
2. **Retire `mono`** (it rides Wampy's sound library). The installer removes it on Update and ticks
   `preload` in its place.
3. ~~**Retire `w1tuning`.**~~ **Done 10-23** (a969f61). It writes Walkman One's own NVRAM images, and nothing it unlocks is
   audible. Keep reading Walkman One's settings only to detect it.
4. ~~**Move out of Wampy's folder:**~~ **Code done 10-24**: `/system/vendor/cinder/` (volume
   tables in `audio_dac/` directly under it). Step 4b of the installer moves an old install once the
   `.appcfg` names the new launcher; the uninstaller clears both folders; `tools/test_migrate.sh`
   (CI). Found on the way: the owner's `.appcfg.real` named Cinder's launcher, so an uninstall would
   have left no Home app; installer, uninstaller and guard now write Sony's four lines instead.
   Left: DEVICE_CHECKLIST §39.11. Wampy's own paths stay where they are read: its scrobbler (Cinder
   stands down for it) and its `libsound_service_fw.so` (the `mono` route, A2).
5. ~~**Drop the Wampy-era hooks and tooling:**~~ **Done 10-23** (91eac2c). the `ldac-run.sh` hook, the `WAMPY_SS`/`.mod` volume
   lookups, `phases/phase1.sh` cloning Wampy's repos, and CLAUDE.md Part E's "install Walkman One
   and Wampy as the baseline". Detection of either stays, so a player that has them still installs.

6. **What is left of Wampy on the owner's player** (looked at over adb 10-24, nothing changed):
   `/system/vendor/unknown321/rollback/` (the owner's hand backups, root-only); `/contents/wampy/`
   (Wampy's `config.ini`, EQ, fonts, skins and log; Cinder reads none of it); and in the boot
   image, `init.scrobbler.rc` from unknown321/scrobbler (09-21), which starts
   `/system/vendor/unknown321/bin/scrobbler` on every boot. That file left with the rest, so the
   start fails and nothing runs. Cinder never writes the boot image; a firmware flash (Sony's or Walkman One's)
   replaces it. All three are the owner's to remove or keep.

**B. A guided installer.** **Code done 10-23** (both points below; groups are Languages, Library,
Radio, Sound, Bluetooth, Device). Screenshots rendered 10-23 (`docs/SCREENSHOTS.md`). Left: a
real install and update through the new window pages.

1. Pages: the player and its firmware, then a starting point (Cinder; Cinder with the sound
   options; everything), then one page per group, then a review that says what each choice touches,
   then install. Update opens on the review with what is installed now.
2. `components.conf` gains a group (Sound, Bluetooth, Library, Radio, Look) and a risk line (what
   it changes on the player, and how to undo it); `installer/src/catalogue.rs` reads both.

**C. Wampy's features, smallest first.**

| Feature | How | Size |
|---|---|---|
| ~~ATRAC on any region~~ | **Done 10-22** (`atrac`, `src/cinder-atrac.c`): verified on the owner's player by hand-install; the `.UPG` install path is not yet run. Test files: genuine `.aa3`/`.oma` from samples.ffmpeg.org `A-codecs/ATRAC3+/` (ffmpeg-muxed OMA is refused by Sony's parser, `omg_oma_init() error = 0x7`) | — |
| ~~FM recording~~ | **Code done 10-23** (`fmrec`, `src/cinder-fmrec.c`, `src/fmrec.h`): the REC button and BT OUT recording are in; the jack path was verified by hand-install 10-22 (7.9 s WAV). Left: DEVICE_CHECKLIST §39.2–39.3 through the installer | — |
| ~~CUE sheets~~ | **Code done 10-23** (`cinder-ffi/src/cue.rs`, `cinder_cue_skip`, `AlbumRow::cue`): Now Playing names the CUE track, ◁/▷ seek between them, the album page lists them and plays from one (`PlayIndexAt`, the audiobook seek). Scrobbled per CUE track. Left: the Songs tab still lists the file once. Listing its CUE tracks there needs the queue to carry a start position per entry, or playing the Songs list plays the file once per CUE track, each from the top. Parked 10-24: it touches Songs, Shuffle all and SensMe's row indices, and goes past Wampy, which only names the CUE track while it plays (Cinder does that) | — |
| ~~Per-song sound~~ | **Code done 10-23**, by album (Track information ▸ ALBUM SOUND; `album_sound=` in the settings file). Host-tested; device-unverified | — |
| ~~Volume curve editor~~ | **Code done 10-23** (Sound ▸ Advanced ▸ Volume curve; `cinder-voltable curve`, `src/vol_curve.h`, `tools/test_volcurve.sh`). A remap of Sony's own rows; the conf reads `curve,stock,P0,…`. Host-tested; device-unverified | — |
| Low-latency USB-DAC | llusbdac is GPL (zhangboyang's, not Wampy's): its own repo, built from upstream source, fetched by the installer like Sony's tables | 2–3 days + device |
| ~~Winamp skins, cassette, digital clock~~ | **Code done 10-23**: Cassette and Clock styles. Winamp is Wampy's landscape screen (`cinder-ui/src/winamp.rs`): the window across the top, three playlist rows under it, PL for the playlist full screen, every control live, the Winamp chip again for the next skin (`wsz=`), RLE BMPs read. Host-rendered with Winamp's base skin and one other. Left: DEVICE_CHECKLIST §39.9 | — |

**D. Walkman One's last two.**

1. **The RMT-NWS20 remote (`REM`):** a Bluetooth setting; the remote is an ordinary peer. Needs the
   remote to test. **Slots recovered 10-23** (`analysis/G_bt_nfc/RE_findings.md`, last section): StartScan 9/10,
   RequestConnection 14 (`const vector<uint8_t>&`, the proven shape), GetPairedDeviceInfo 17; listener
   registration is slot 18, `AddListener(listener*, const std::string&)`. Left: the listener's vtable order. Start (10-23 notes): `libBtBleCommonService.so` exports only
   `BtBleCommonServiceClientFactory::CreateInstance`; the client's methods each log their own name
   (`BtBleCommonServiceClient::StartScan`, `RequestConnection`, `GetPairedDeviceInfo`, …) and the
   listener's (`OnNotifyScanResult(addr, rssi, adv)`), so the vtable slots can be recovered by
   matching each function's string to its `.data.rel.ro` slot, as `BtCommonServiceClient`'s were
   (main.cpp, "Bluetooth: make the switch drive the RADIO"). Then: scan, list by the advert's name,
   `RequestConnection(addr)`; `BtBleRemoteService` turns the remote's HID reports into a uinput
   device, which cinder-home's input pump has to open alongside event0/event1.
2. **Changing the player's region,** which Walkman One does in NVP. Optional, and it writes NVP:
   back up first, and only if someone asks, since `voltable` already lifts the volume cap.

**Parity audit (10-23): what Wampy and Walkman One have, or wanted, that Cinder does not yet**

From Wampy's `README.md`, `USAGE.md` ("Not implemented"), `TODO.md` (unticked, "want", "hard to
solve") and `not-done.md`, and from Walkman One's measured changes (`docs/VISION_four_builds.md` §3).
Walkman One is closed and has no public tracker, so its unbuilt wishes are unknown. Smallest first
within each group.

*Wampy features Cinder lacks or has in a smaller form:*

| Feature | In Cinder today | To do |
|---|---|---|
| ~~Custom cassette tapes~~ | **Done 10-23** (`cinder-ui/src/tape.rs`, `cinder-ffi/src/tape_load.rs`): Wampy's folder layout and `config.txt`, landscape, the reel at 55 ms, `cassette.txt` per folder, the chip again for the next tape | Device: §39.10. Wampy's `.pkm` atlases are not read |
| ~~Per-song EQ~~ | **Done 10-23**: Track information ▸ ALBUM and SONG; a song's preset wins | Device: §39.4 |
| Volume table editor: every value of every table, DSD too | A 7-point curve over Sony's own rows | Deliberately not raw values: a curve over Sony's rows cannot go past what Sony allows. More points or the DSD table if asked |
| ~~Skin transparency (`region.txt`)~~ | **Done 10-23**: the `[Normal]` polygons, pixel-exact at the skin's own size, black behind | — |
| Skin dropdown | Tap the Winamp chip again (cycles) | A list screen, if cycling is not enough |
| Swap prev/next; ignore keys when the screen is off | The Hold switch covers the second. The first depends on which way the player's own ⏮ ⏭ read on its side, so it waits for a look at the player | Decide with the player in hand |
| ~~Visualiser "Winamp mode" bands, sensitivity~~ | **Covered (checked 10-23)**: the Winamp view draws the levels Visualiser's Range (sensitivity), Response and Bands rows shape, in Winamp's own 19 or 75 columns | — |
| ~~Export Sony's bookmarks~~ | **Done 10-24**, opt-in (`sony_bookmarks` in `cinder_advanced.conf`): the non-empty `/cache/bookmarkNN.json` lists are read-only rows in Playlists, "Bookmark List N" as Sony names them. Fixtures are the owner's two files | Listed, opened and played on the owner's player 10-24 (both lists, covers, no edit bar; Sony's files unchanged) |
| User font | Bundled + Sony fonts | — |
| Low-latency USB-DAC (llusbdac) | `cinder-llusbdac` repo **ready locally 10-23** (`/home/sony/cinder-llusbdac`): pinned build from Sony's NW-A57 kernel (BBDMP5) + upstream eeb31d5 + GCC 4.9.3; vermagic matches the player's modules | Owner: create the public repo and push (the auto-mode classifier refused to). Then the installer component and a USB DAC row; first insmod with the owner present |

Wampy's filter list (Dynamic Normalizer, VPT, 6- and 10-band EQ, Tone, DC Phase, Vinyl, Direct
Source, ClearAudio+) is all on Cinder's Sound screens already.

*Wampy's own unbuilt wishes, which Cinder could do:*

| Wish | Where Cinder stands |
|---|---|
| Winamp playlist scrolling, editing, its bottom buttons | Scrolling **done 10-23** (a swipe pages through the whole queue; Wampy only ever had 15 tracks), rows in Cinder's font so every script shows. Editing is Up Next's |
| ~~More spectrum styles (thin bands, fire, line), an oscilloscope~~ | **Done 10-23** in the Winamp view: V steps normal / fire / line, thick or thin, with peaks; the display cycles spectrum, oscilloscope, off; kept across restarts (`wa=`) |
| Title scrolled by dragging | — |
| Enqueue | Done (Up Next, swipe a row) |
| Apply tunings on boot | Done (the launcher applies voltable/signature/EQ) |
| Buttons on Apple (CTIA) headphones | Research: Wampy found no difference in `regmon`. Open |
| A backlight dimmer below level 1 | Night palette scaling does it in software; writing `duty` panics the kernel (memory) |
| Easter eggs | — |

*Walkman One:* every audio gain is in (signature, gain tables, DAC EQ, region cap); its external
tunings hold nothing audible. Left: the REM remote and the region change (stage D). Walkman One's
losses (FM, NC, VPT, ClearAudio+, Line-out) never apply, because Cinder does not swap the model.

**Installer — finish an install from a Mac** (owner, 10-22; Montree on Head-Fi asked for it)

**Steps 1–3 code done 10-23** (`installer/src/bot.rs` + the macOS `trigger_fw_upgrade` in
`stage.rs`, `rusb` as a macOS-only dependency; a macOS job in `ci.yml`; `build-macos` in
`release.yml` makes one universal binary; `install.md` has the steps). Type-checked here against
rusb's real API (a Cargo `links` override skips libusb's C build, which needs Apple's SDK); the
first real compile is CI's macOS job. The key is not read on a Mac (taking the interface ejects
the volume), so Walkman One is told from the files on the drive there (`device::w1_from_files`). The vendored
libusb is LGPL-2.1, linked statically: **ship with a notice** (owner, 10-23). `release.yml` attaches
libusb's `COPYING` as `LICENSE-libusb.txt` and the release notes name its source and how to relink. Left: step 4, a real Mac.

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
8. **G1.7 contract fixtures, the rest:** ~~the palette corpus~~ (**done 10-23**:
   `contracts/palettes/` + `verdicts.tsv`, Flint's checker agrees word for word) and
   `flint-shared.tsv` (**done 10-23**, Flint's writer and Cinder's reader). `cinder-playlist.m3u8` (**done 10-23**, Cinder's
   playlist file as Flint reads it). Nothing left: a SensMe chunk has
   no reader in Cinder (Sony's scanner reads it, and the 09-17 device proofs are its check). A `#TZ/UTC` log is Flint-only (Cinder writes `#TZ/UNKNOWN`), so it is a Flint
   test, not a contract.
9. ~~**Tests for `cinder-audio`**~~ (`SHORTCOMINGS.md` A2): **done 10-23**, every shipping shim
   has a host self-test (player, tuner, analyzer and storage added; each checked by breaking it).
10. ~~**2038 (goal 10):** an i64 audit of Cinder's own timestamps.~~ **Done 10-23**: saved data was already i64; the Home app's clock (`now_ms`, 32-bit, wrapped after 24.8 days awake) and the mono shim's are now 64-bit, and `build.sh` fails on a new 64→32 narrowing in `main.cpp`.
11. ~~A host benchmark with a 10,000-line playlist~~ (**done 10-23**: the reader stopped at 4,096;
    now 20,000, and 10,000 lines read in 2.5 ms on the host). Discovering an **unpaired** Bluetooth device; a
    Windows installer smoke test per release (Install → Update → Uninstall → Install).
12. **Bluetooth still drops and loses its volume** (owner, 10-23). Connections still have problems
    with the volume and with disconnects. No details captured yet. Next time it happens: switch on
    THIS DEVICE ▸ Debug log first (the HCI snoop, the stack's only failure channel), reproduce it,
    then pull the snoop and `logcat`. Read it against the known traps: the retry mode that refuses
    connects (`BT connect jam`), the route flag trusting `GetBtStatus == 3`, and the BT volume being
    AVRCP steps that are separate from the jack's level.
13. ~~**One file, two albums**~~ (**built 10-23**, device-unverified; Flint item 5):
    `cinder-ffi/src/shared.rs` reads `flint-shared.tsv` with the library and lists each entry under
    its own album as a copy of the kept file's row. Unlike the plan above, both rows share the kept
    file's object id, so plays, likes and ratings count to the one file, and Now Playing shows the
    kept album's tags.

14. **Long copy at the owner's 120 %** (found 10-24 with `cargo run -p cinder-host -- --cut 120`,
    which lists every run cut with "…"). Button labels and chips are fixed; about 20 hint and
    second lines are still cut, among them Sound ▸ Advanced's "Surround, DC Phase, Vinyl, Tone
    Control, DAC…", DAC EQ's "the 10-band stays for…", Bluetooth pairing's "PIN AND CONFIRM
    PROMPTS APPEAR HERE WHEN A DEV…", the receiver's two sentences and "EXPERIMENTAL · TESTED WITH
    WINDOWS AND …", Device's "NO FUEL GAUGE — NO CURRENT, NO CYCLE CO…" and "CARE ON: 100% HERE IS
    ~4.09 V, NOT 4.2 …". **Owner's call:** shorter wording (each needs its Chinese too), a second
    line where the row has room, or a size down as the chips do.

**Flint**

1. ~~Adopt music already on the player by content key (G3.2)~~: **same path done 10-24**: a file where Flint would copy it, with the same audio and tags (`cache::same_file`), is kept and claimed in the manifest (`Plan::kept`). Left: a file under another path (Music Center's layout) is still copied; moving it into place is the next slice.
2. ~~E7 the album unit~~ (**done 10-23**: the folder holding the audio, discs kept with it); ~~E9 refused scrobbles to `.scrobbler.refused.log`~~ (**done 10-23**);
   ~~E10~~ (fixed 10-05); ~~E11 one WinHTTP session for the palette shop~~ (**done 10-23**, run on Windows).
3. Features in the 10-01 order: ~~G1.1 backup of the player's state~~ (**done 10-23**: every read,
   `flint backup` / `flint restore`; RECOVERY.md has the step), ~~G1.2 health card~~ (**done 10-23**:
   read from the files already on the drive, plus one Cinder log line for `/data` free; parser fixtures
   are format strings until a real `cinderhome.log` replaces them), ~~G3.1 library
   lint~~ (**done 10-23**: in Check and `flint lint`; "several years in one album" left out until it is
   shown to split an album), G1.3 device-aware transfer (**flagging done 10-24**: Check lists FLACs over 24 bits; converting them on the copy is left, with R6's convert-on-transfer), ~~G2.1 per-headphone profiles~~ (dropped 10-18 by the owner: Presets, loaded by hand), ~~G1.4 LRCLIB lyrics~~ (**done 10-24**: `flint lyrics <library> --apply` writes `Song.lrc` beside each track LRCLIB has synced lyrics for, never replacing one or fetching over synced lyrics in the tags; checked live against lrclib.net on a scratch library. In the window: SensMe ▸ Fetch lyrics), ~~G2.2 resume~~ (audiobook positions, 0.3.17).
4. R6 / Flint 0.3: Setup, convert-on-transfer, Copying, the palette preview, SensMe reasons, the
   SensMe MP3 path.
5. ~~**The same song on two albums, stored once**~~ (**built 10-23**: Flint suggests
   pairs by audio, the owner ticks them, `flint shared`; first path kept, **Swap** / `--keep-other` for the other (10-23). Removing an album ends its entry at the next sync (tested 10-24), and a kept album that fits on no volume gives the song back to the other album (fixed 10-24).)
   *Original note:* (owner idea, 10-23). A track that appears on two
   albums (the album and a best-of, a single and its album) is often the same recording. You
   select the two, or Flint suggests pairs by matching decoded audio. Flint then copies one file
   and writes the other album's entry into a small list on the player. Cinder (item 13 above)
   shows both under their own albums, so the library looks exactly as on the PC, and the storage
   is spent once. Open questions: which tags the second entry keeps (track number, album artist,
   cover); what happens when one album is removed from the sync; and Sony's own player, which
   will show only the stored copy.

6. ~~**Likes sync options**~~ (**built 10-23**): per-run direction (both / to the
   player / to Last.fm), an add-only switch, and each removal ticked so one can be left alone.
   Conflicts still keep the like.

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
