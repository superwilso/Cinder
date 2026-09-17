# SensMe channels, and the sync tool that feeds them — plan

**Written 2026-09-17. A plan, not a change.** It supersedes the *analyser choice* in
[`PLAN_sensme.md`](https://github.com/superwilso/Cinder/blob/claude/exciting-planck-7yzhfs/docs/PLAN_sensme.md)
(draft PR #13) and the SensMe line of Sony-sync's `PLAN_walkman_suite.md` (draft PR #1). The two
drafts assumed Sony's classification was out of reach; §1 shows it is not.

**[V]** = verified, with where. **[U]** = unverified, with the check that settles it.

> **Decided by the owner, 2026-09-17:** option **A** (tag the Walkman copies); the rewrite is named
> **Flint**; it is written in **Rust**. The owner is letting Music Center analyse a copied FLAC and
> MP3 (§6 item 1). Work on Flint has started in a new repository (`/home/sony/flint`, not yet on
> GitHub); the Python Sony-sync stays as the reference until Flint replaces it.

---

## 0. The ask, as given

> Someone on Reddit wanted real SensMe, but unknown321 said it added MB to songs for no reason. Can
> we improve that but still use the good classification system, or use a better one, in Sony sync
> (needs a better name?) — scan all the files on the PC first, then upload them to the Walkman the way
> Sony does. Sony sync is public and could do with a proper rewrite and an exe download.

Three outcomes, then:

1. **Real SensMe channels** from Sony's own classification — on Cinder, and ideally on stock too.
2. **Without the bloat**, and without touching the user's PC library.
3. **A rewritten, downloadable sync tool** that does the scanning, as Music Center does.

---

## 1. What was established today

All from [`analysis/RE_sensme_musiccenter.md`](../analysis/RE_sensme_musiccenter.md) §5–§8.

| Fact | Status |
|---|---|
| Sony's engine (`MMLib11.dll`, x86 COM, from Music Center) analyses a 4-min track in **1.3 s**, a 7-min one in **1.7 s** | **[V]** §5 |
| Its whole output is **~6 KB per track** and does not grow with length | **[V]** §5 |
| Tempo is right: 93.4 BPM for a song quoted at 94 | **[V]** §5 |
| The player's scanner parses **exactly those chunks** and computes the five axes, the channel bitmask and the sabi (chorus) position itself | **[V]** §6, Ghidra on `libMediaStoreService.so` |
| FLAC carries the tag as an APPLICATION block, id `SMFM` | **[V]** §7, `OpcFlac.dll` |
| The "almost a megabyte" is **not** the engine's output | **[V]** by size; what it *is* stays **[U]** — one Music Center-tagged file settles it (§8 of the note) |
| Music Center 2.7.3 is installed on the owner's PC; it caches each result as `fringe\audio\<id>\smfmf.bin` | **[V]** §8 |
| The owner's device library has **no** SensMe rows today (6,722 objects, akeys 50–59/121 empty) | **[V]** MTPDB pulled 2026-09-17 |
| `/db` on the player is writable by Cinder (uid 100), 85 MB free | **[V]** 2026-09-17 |
| The device turns our tag into channels | **[U]** — milestone M0 |

**The bloat problem has a clean answer.** Wampy's author saw Music Center grow each file by almost
a megabyte, possibly corrupting ID3 tags on the way. The data the player needs is ~6 KB. Writing
*only that*, *only into the copy on the Walkman*, keeps the PC library byte-for-byte untouched, adds
about 21 MB across a 3,500-track library, and leaves ID3 writing out of the first release (FLAC
only).

---

## 2. Three ways to deliver it

| | **A. Tag the Walkman copy** | **B. Sidecar, Sony's maths on the player** | **C. Our own analyser** (PR #13) |
|---|---|---|---|
| Classification | Sony's, computed by Sony's own scanner | Sony's, computed by calling the scanner's functions in-process | Ours; valence will be weak (PR #13 D1) |
| Files on the player | +~6 KB each (copy only) | Untouched | Untouched |
| Works on **stock** firmware too | **Yes** — this is simply real SensMe | No | No |
| New device code | Channel screens reading MTPDB | Calling stripped, unexported functions by offset | Sidecar reader + channel screens |
| PC requirement | Windows + Music Center installed (engine is Sony's; we do not ship it) | Same | Any OS |
| Main risk | The scanner rejects our tag (M0 proves it) | Offsets are per-firmware; a wrong call crashes Cinder | Quality |

**Recommendation: A first, C later as the non-Windows fallback, B only if A is refused.**

* A is the only option that also answers the Reddit request for someone *not* running Cinder.
* A needs no reimplementation of Sony's classification: Sony's code does it on the player, the same
  way it does for Music Center's tags.
* B is a legitimate fallback if tagging copies is unacceptable, but it adds per-firmware function
  offsets inside the Home app — the kind of dependency this project otherwise avoids.
* C keeps PR #13's design (sidecar format, channel files, mood map) for Linux/macOS users and anyone
  without Music Center. PR #13's channel-UI sections apply to A with one change: channel membership
  comes from MTPDB, not a sidecar.

---

## 3. The PC side: scan first, then transfer

```
PC library ──► scan (size, mtime, tags) ──► analyse new/changed tracks ──► analysis cache
                                                     │  sensme-helper.exe (x86) + MMLib11.dll
                                                     ▼
transfer plan ──► copy each track to the Walkman ──► inject SMFM block into the COPY ──► manifest
```

### 3.1 Analysis cache — never inside the user's files

* One record per track: `(content key, engine version, result 89 bytes, BPM, sabi, analysed_at)`.
* **Content key:** `(size, mtime)` as the fast path, a hash of the audio frames as the truth, so a
  retag or rename does not re-analyse. The likes key (`likesync/keys.py` ↔ `likes.rs`) is for
  *matching*, not caching.
* Location: the tool's own data folder (`%LOCALAPPDATA%\<tool>\analysis.db`, SQLite).
* **Import Music Center's cache** when it exists (`fringe\audio\<id>\smfmf.bin`, matched by file path
  from its `tracks.db`) — **[U]** until §8 of the note shows those files equal result 89.

### 3.2 The engine

* **Not redistributed.** The tool looks for `C:\Program Files (x86)\Sony\Music Center\AVLib\MMLib11.dll`
  **[V]**; if it is missing it says so and links Sony's Music Center download. It never downloads or
  bundles Sony's DLL itself.
* The DLL is 32-bit, so a small **`sensme-helper.exe` (i686)** loads it — `tools/sensme/smfmf_probe.c`
  is that helper in prototype form **[V]**. It reads PCM on stdin and writes the result to stdout.
  Several helpers run in parallel (one per core).
* **Decoding:** FFmpeg when present, optional and detected (the pattern Sony-sync already uses for
  `mutagen`). A Media Foundation decoder inside the helper would remove even that — FLAC support is
  built into Windows 10+ **[U]**.
* **Budget:** 4,267 tracks × (≈1.5 s analyse + decode) ≈ 2 h on one core, ≈ 20–30 min on eight
  **[U]** until measured on the whole library. Only new or changed tracks cost anything after that.
* Linux/macOS: the helper under Wine is plausible (the DLL imports only Windows system DLLs **[V]**),
  **[U]** — Wine is not installed here. Otherwise those users get option C.

### 3.3 Writing the copy

* **FLAC first** — an APPLICATION block, id `SMFM`, payload = result 89 **[V]** container, **[U]**
  payload framing until M0. The block goes *after* existing metadata and *before* audio; a `PADDING`
  block of at least ~6.5 KB is shrunk to make room, so the audio does not move. How many of the
  owner's files carry that much padding is **[U]** — count it during the first scan.
* Write to a temp name, then rename, and never on the user's PC copy. `/contents` is unjournalled vfat
  and has bricked this device once **[V]** (`VISION.md`).
* MP3 (`GEOB` `USR_SMFMF`) and M4A come after M0, once §8 of the note has shown their payloads.
  Anything else is transferred untagged.

### 3.4 The manifest — required, or every sync re-copies everything

Sony-sync decides a copy is current when the sizes match (`sync.py:1278`) **[V]**. A tagged copy is
~6 KB larger, so without a record of what was written, every run would re-copy and re-tag the whole
library. The tool keeps a manifest per device
(`source path, source size, source mtime, copy size, tag hash`) and compares against that. A copy
on the device whose size matches neither the source nor the manifest is treated as changed.

---

## 4. The device side

### M0 — proof, before any tool is built

1. Back up `/db/MTPDB.dat` (and read `reference_mtpdb_rescan_hazard`: **never reboot during a rescan**).
2. On the PC: copy one FLAC, add an `SMFM` block holding result 89.
3. Put it on `/contents` over USB mass storage, unplug, let the scanner run.
4. Pull `MTPDB.dat`; read `object_ext_int` akeys 50–59 and 121 for that object.

**Pass:** tempo/mood/type/style/time rows present, and a channel bitmask and sabi value somewhere.
**Fail:** no rows → the payload needs a wrapper; get the Music Center-tagged file first (§8 of the note).

Also worth doing once, on **stock**: open SensMe channels and see the track listed. That proves A
for users who never install Cinder.

### M1 — Cinder reads it

* `cinder-db` already reads `object_ext_int` by akey **[V]** (`player/cinder-db/src/lib.rs:181`); add
  the SensMe akeys and the bitmask/sabi.
* RE still needed (§6 of the note): the channel-id → name table and which akeys hold the bitmask and
  sabi. M0's MTPDB answers the second.

### M2 — channel screens

* A **SensMe** list: the 14 channels Sony has (shuffle all, morning, daytime, evening, night, midnight,
  active, relax, upbeat, mellow, lounge, emotional, dance, extreme) **[V]** from `HgrmMediaPlayerApp`'s
  assets. Empty channels hidden; no data at all shows one line saying how to get it.
* A channel is a **play context** — the same path an album uses (PR #13 D5 holds unchanged).
* **Start at the sabi**, as Sony's player does (it logs `sabi_position`, falling back to
  `duration / 2`) **[V]** — optional, default on.
* Time-of-day channels: whether Sony picks by clock or by the tag is **[U]**; follow what
  `SensMePlayer` does.

### M3 — later

Mood map, "more like this", and option C for non-Windows users, as PR #13 plans them.

---

## 5. The sync tool rewrite

### 5.1 Name

"Sony" in a public tool's name invites a trademark complaint, and the tool's job is now Cinder's
companion. Candidates in Cinder's family: **Kindling** (it prepares what Cinder burns), **Bellows**
(it feeds the fire), **Hearth**. The owner picks.

### 5.2 Language — recommendation: Rust

| | Rust | Python + PyInstaller |
|---|---|---|
| One exe from CI | Yes; the installer already cross-compiles to Windows from Linux **[V]** | Yes, but 15–30 MB and more antivirus false positives |
| Shares code with Cinder | Likes key normalisation, playlist and palette formats — **byte-identical by construction** | Kept in step by tests on both sides, as today |
| Scanning 4k+ tracks | Fast, real threads | Adequate |
| Cost | A real rewrite of ~9,200 lines **[V]** | Mostly packaging and restructuring |
| GUI | The installer's dependency-free Win32 layer **[V]** (`installer/src/gui.rs`) | Stays a terminal UI |

Rust wins on the thing that has bitten this project before: two implementations of the same key that
"must agree or nothing lines up" (`likes.rs`). If the owner would rather ship sooner, Python with
PyInstaller is a sound interim; the plan below is the same in either language.

### 5.3 What carries over, and what gets fixed

* **Keep:** two-volume planning, preview, stale sweep, playlists, scrobble upload, three-way likes,
  and the tests (44 **[V]**).
* **Fix while moving:**
  * No licence file — nobody may legally reuse a public repo without one. MIT, to match Cinder.
  * 25 `__pycache__` files and `.claude/settings.local.json` are tracked **[V]**.
  * A hard-coded personal source path is the default (`sync.py`) **[V]**.
  * `sync.py` is one 3,138-line file **[V]**.
  * No releases **[V]**.
  * Checked: no API secret has ever been committed (history scanned 2026-09-17) **[V]**.
* **Structure:** `core` (scan, plan, copy, manifest, cache, tags) · `sensme` (helper protocol) ·
  `likes` · `scrobble` · `ui` (TUI now, Win32 GUI later) · `sensme-helper` (i686).
* **Release:** GitHub Actions builds `kindling-windows-x64.exe` and `sensme-helper-x86.exe`, and
  publishes SHA-256 sums. The helper is optional: without it, everything except SensMe works.

### 5.4 Cinder's front door

The install/update/remove entry from Sony-sync's PR #1 §4 still stands, and fits better once the
tool is an exe. It is not needed for SensMe, so it comes after M2.

---

## 6. Order

| # | Work | Size | Needs |
|---|---|---|---|
| 1 | Music Center tags one FLAC and one MP3 copy; compare with result 89 | 15 min | **owner, at the PC** |
| 2 | **M0** device proof (§4) | 1 session | owner OK for a scanner run |
| 3 | `sensme-helper` from the probe: stdin PCM → stdout result, batch mode | half a session | — |
| 4 | Rewrite skeleton + cache + manifest + FLAC injection, still copy-compatible with today's sync | 2–3 sessions | language decision |
| 5 | **M1 + M2** in Cinder | 2 sessions | M0 |
| 6 | MP3/M4A tags, Music Center cache import, parallel helpers | 1 session | item 1 |
| 7 | Release pipeline, rename, licence, README | 1 session | name |
| 8 | Front door; option C | later | — |

---

## 7. Questions for the owner

1. **A (tag the Walkman copies) as the primary route** — acceptable? It changes files on the player
   (not the PC); B is the alternative that changes nothing.
2. **Name:** Kindling, Bellows, Hearth, or something else?
3. **Rust rewrite, or Python + PyInstaller first?**
4. **Item 1 above** — will you let Music Center analyse two copied tracks so the container question
   is settled from Sony's own output rather than guessed?
5. **Non-Windows users:** is option C worth building at all, or is SensMe a Windows feature?
