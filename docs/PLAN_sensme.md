# Mood / tempo channels — plan

**Written 2026-09-17. Nothing here is implemented.** A SensMe-style feature built from *our own*
analysis data: a PC tool computes a few values per track, writes a sidecar SQLite file onto the
player, and Cinder reads it to offer channels, a 2D mood map and a "more like this" radio.

Every claim below is marked **[V]** verified (with the file or URL it came from) or **[U]**
unverified (with the check that would settle it). Where a claim came from another document in this
repository rather than being re-derived here, it says so.

**This is not Sony's SensMe.** Sony's is data-gated on a tag only Music Center writes
([`analysis/RE_sensme_musiccenter.md`](../analysis/RE_sensme_musiccenter.md)); that line of work is
separate and stays separate. Nothing here copies Sony/Gracenote SMFMF data or asserts how their
engine worked internally.

## Constraints this plan is written under

| | |
|---|---|
| Licence | Cinder is MIT **[V]** (`LICENSE`). Nothing GPL/AGPL/non-commercial may be linked into `cinder-home`, `player/` or `installer/`. The analyser is a **separate host tool** and never ships inside the installer. |
| Installer | Stays dependency-free **[V]** — `installer/Cargo.toml` has an empty `[dependencies]` and says why. No analysis goes in it. |
| Sony's DB | `/db/MTPDB.dat` is opened **read-only** today **[V]** (`player/cinder-db/src/lib.rs:166`, `SQLITE_OPEN_READ_ONLY`). Nothing here writes to it. |

---

## 1. Research questions

### R1. Licences and output ranges of the candidate analysers

**Licences — settled this pass:**

| Thing | Licence | Source |
|---|---|---|
| **Essentia** (library) | **AGPL-3.0** **[V]** | [`MTG/essentia/COPYING.txt`](https://raw.githubusercontent.com/MTG/essentia/master/COPYING.txt); its docs source adds *"available under an open licence, Affero GPLv3, for non-commercial applications"* **[V]** ([`licensing_information.rst`](https://raw.githubusercontent.com/MTG/essentia/master/doc/sphinxdoc/licensing_information.rst)) |
| **Essentia pre-trained models** | **CC BY-NC-ND 4.0** **[V]**, same file: *"All the models are available under the CC BY-NC-ND 4.0 license for non-commercial use."* | **Correction to the brief:** the licence is BY-NC-**ND**, not BY-NC-SA. ND is *stricter* — no derivatives at all. Out of bounds either way. |
| **bliss-audio** 0.13.0 | **GPL-3.0-only** **[V]** | [`bliss-rs/Cargo.toml`](https://raw.githubusercontent.com/Polochon-street/bliss-rs/master/Cargo.toml) |
| **aubio** | **GPL-3.0-or-later** **[V]** | [`aubio/README.md`](https://raw.githubusercontent.com/aubio/aubio/master/README.md) |
| **madmom** | code BSD-2-Clause, **models CC BY-NC-SA 4.0** **[V]** | [`CPJKU/madmom/LICENSE`](https://raw.githubusercontent.com/CPJKU/madmom/main/LICENSE) — *"pickled Processors (i.e. saved models) fall into this category"*. This is the trap: BSD code, NC weights. |
| **LAION CLAP** (repo) | **CC0-1.0** **[V]** | [`LAION-AI/CLAP/LICENSE`](https://raw.githubusercontent.com/LAION-AI/CLAP/main/LICENSE). The *checkpoints* are hosted separately and their terms are **[U]**. |
| `symphonia` 0.6.1 | MPL-2.0 **[V]** | crates.io API |
| `rustfft` 6.4.1 | MIT OR Apache-2.0 **[V]** | crates.io API |
| `ebur128` 0.1.10 | **MIT** **[V]** | crates.io API — EBU R128 / ITU-R BS.1770 loudness |

**bliss's real dependencies — the brief asked specifically.** From its `Cargo.toml` **[V]**:

* `default = ["ffmpeg"]`, and `ffmpeg = ["analysis", "_any_decoder", "dep:ffmpeg-next", "dep:ffmpeg-sys-next"]` — so **FFmpeg is on by default** but is an *optional decoder feature*, and the file's own comment says it exists so *"you can implement the decoding of the tracks yourself"*.
* `symphonia` is a **separate optional feature** (`symphonia = ["analysis", "_any_decoder", "dep:symphonia", "dep:rubato", "dep:audioadapter-buffers"]`) — an alternative decoder, not an addition.
* **aubio is not a dependency of bliss at all.** It does not appear in `[dependencies]`. The analysis maths is `rustfft` + `ndarray` + `ndarray-stats` + `noisy_float`. **[V]**

So the FFmpeg/Symphonia question is "which decoder", not "both"; and the aubio association is wrong.
None of that rescues it: **bliss is GPL-3.0-only, so it cannot be linked into anything we ship** —
not the player, not the installer, and not an MIT-licensed host tool we distribute.

**Output ranges — all [U].** No candidate's numeric range has been confirmed by running it. This is
the single biggest gap and it is cheap to close.
*How:* on the host, run each candidate over the same 20-track set (R-test below) and record
min/max/mean per output. Specifically: whether Essentia's `emomusic`/`deam` arousal-valence heads
emit the datasets' 1–9 scale or a normalised 0–1; whether bliss's features are z-scored or bounded;
what `ebur128` returns for integrated loudness on quiet and loud masters. **No device needed.**

### R2. Does any open tool do chorus/hook detection?

Three that exist, verified by reading their own repositories:

| Tool | Licence | What it actually claims |
|---|---|---|
| **`pychorus`** | **MIT** **[V]** ([README](https://raw.githubusercontent.com/vivjay30/pychorus/master/README.md)) | *"The algorithm is largely based on a paper by Masataka Goto with some simplifications and modifications."* **[V]** It makes **no accuracy claim** **[V]**. PyPI metadata is thin: version 0.1, no licence field, no declared dependencies **[V]**. |
| **MSAF** (Music Structure Analysis Framework) | **MIT** **[V]** ([`LICENSE.md`](https://raw.githubusercontent.com/urinieto/msaf/main/LICENSE.md)) | Segment **boundaries and labels**, not "the chorus". Turning a label sequence into a hook is our problem. |
| **`allin1`** (All-In-One Music Structure Analyzer) | **[U]** — its README names no licence **[V]** | Emits a literal `chorus` label: vocabulary `start, end, intro, outro, break, bridge, inst, solo, verse, chorus` **[V]**. But it needs **madmom** **[V]**, whose models are CC BY-NC-SA **[V]**, plus PyTorch, NATTEN and Demucs. Non-commercial contamination, and heavy. |

**Algorithm names — checked, as asked.** "Goto's chorus-detection method" is real and is what
`pychorus` cites **[V]**. The name **RefraiD** for that method is **[U]** — I did not confirm it in a
primary source this pass, so the plan does not use it. Do not write it into code comments until
someone has the paper in front of them.

*How to close:* run `pychorus` over the 20-track set and eyeball whether the returned offset lands
in a chorus. **No device needed.** If it does not, the fallback that needs no new dependency is "the
loudest sustained 30 s window", which is not a hook detector and must not be called one.

### R3. Device CPU / RAM, and reading music in USB-MSC mode

All **[V]** from our own device map
(`cinder-sony-analysis/device/nw-a55/fw-1.02-wampy-cinder/`):

* **CPU:** `Hardware: MT8590`, **two** ARMv7 cores, `CPU part 0xc07` = Cortex-A7 (`system/proc_cpuinfo.txt`). `598000`–`1300000` kHz, governor `hotplug` (`sysfs/cpu.txt`). *Note:* `docs/AUDIT_BATTERY_PERF_2026-09-05.md` calls this "single-core ARMv7" — under `hotplug`, cpu1 is offline most of the time, so that is the right *practical* assumption but the wrong *hardware* statement.
* **RAM:** `MemTotal: 467512 kB`, `MemFree: 124068 kB`, **`SwapTotal: 0`** (`system/proc_meminfo.txt`). No swap means an over-large in-memory index is an OOM, not a slowdown.
* **Storage** (`storage/df.txt`, `system/proc_mounts.txt`): `/contents` = **vfat**, 58.6 GB, **97 % full, 1.5 GB free**. `/db` = **ext4**, 94 MB total, **83 MB free**. `/data` = ext4, 36 MB total, 18 MB free. `/contents_ext` (SD) = vfat, 99 % full.
* **Can the player read the music partition during USB-MSC? No, and it must not.** `cinder-home/src/cinder-msc.c` unmounts `/contents` *before* binding the mass-storage LUN, and its header comment says why: *"`/contents` must be unmounted BEFORE the gadget binds it, or the host and the kernel have the same vfat mounted twice and the volume corrupts."* **[V]** So the sidecar is written by the PC while Cinder cannot see it, and read after the volume comes back — exactly the lifecycle `cinder_liked_import.tsv` already has.

Consequence for the design: **the device must never open audio files to match sidecar rows.** Any
key that requires decoding or hashing 30 GB of audio on a 598 MHz A7 is disqualified.

### R4. Does MTPDB.dat have SensMe columns worth reading as a bonus?

**[V] from this repository, not re-derived here:**
`analysis/RE_sensme_musiccenter.md` §4 reports that `libMediaStoreService.so` parses SMFMF and
stores `SENSMECHANNELID / TEMPO / MOOD / TYPE / STYLE / TIME`, `SMFMF12TONEV1/V2` and
`SMFMFBEATIZER` as **`object_ext_*` akeys 51–59**. Cinder already reads `object_ext_int` by `akey`,
resolving names through the `schema` table **[V]** (`player/cinder-db/src/lib.rs:181`), so reading
these would be a few lines, not a project.

**[U]: whether any row is populated.** `README.md` states no library Cinder has been tested with
contains any **[V]**, and the reference `MTPDB.dat` is not in a fresh checkout (`artifacts/` is
gitignored and holds only `session/` **[V]**).

*How:* on any pulled `MTPDB.dat`, off-device —
```sql
SELECT akey, prop_name FROM schema WHERE prop_name LIKE '%SENSME%' OR prop_name LIKE '%SMFMF%';
SELECT akey, COUNT(*) FROM object_ext_int WHERE akey BETWEEN 51 AND 59 GROUP BY akey;
```
`cargo run -p cinder-db --example schema_dump -- <MTPDB.dat>` already exists **[V]**.
**Verdict for this plan: treat as a bonus column, never a dependency.** Zero rows is the expected
answer and the feature must be complete without it.

### R5. Does our `.scrobbler.log` record skips?

**No. [V]** `player/cinder-ffi/src/scrobble.rs` has a `format_line(t, rating, start_unix)` whose
doc-comment says *"`rating` is 'L' or 'S'"*, but **both** call sites pass `"L"` (lines 109 and 134);
`"S"` is never written. The module comment states the rule it implements: half the length or 240 s,
and *"Shorter/abandoned tracks are dropped."* **[V]**

So there is no skip history today. Two things to weigh before adding one:

* The log is **consumed and cleared** by the PC side on upload **[V]** (`Sony-sync/sync.py`, `rewrite_scrobble_file`), so skips written there are transient unless the uploader keeps them.
* An `S` row must not be submitted to Last.fm as a listen. Whatever writes it has to be paired with a change in the uploader in the same release.

*How to close:* a design decision, not a measurement. **No device needed.**
**Recommendation: out of scope for M1–M4.** Skips are a good future signal for personalising channel
ordering; they are not needed to make channels work, and they add a cross-repo coupling.

### R6. Where should the host tool live?

**This is a question for the owner, not a guess.** See §5 Q1. The two candidates and what each
costs are written out there.

---

## 2. Decisions to make

### D1. Analyser choice

| Option | Trade-off |
|---|---|
| **(a) Our own, MIT-only** — `symphonia` (MPL-2.0) to decode, `rustfft` (MIT) for spectra, `ebur128` (MIT) for loudness; BPM from an onset-strength envelope + autocorrelation; arousal/valence/danceability from a small documented feature mapping | Ships anywhere, no licence question, one language with the rest of the tree, ~6 crates. **Quality is the risk**: arousal maps reasonably onto loudness + onset rate + spectral centroid; **valence is genuinely hard** from hand-rolled features and will be the weakest number we produce. |
| **(b) Essentia or bliss as an optional, user-installed backend** | Best-in-class values. But **we could not distribute it** (AGPL / GPL / NC models), so it becomes "install this yourself and point the tool at it" — a support burden, and a feature most users would never turn on. |
| **(c) CLAP embeddings + a small regressor** | Strongest "more like this". Needs PyTorch on the PC, ~GB of checkpoints of **[U]** licence, and a 512-float vector per track. |

**Recommendation: (a) for M1–M4, with the schema designed so (b) and (c) can fill the same columns
later.** The durable thing this project is creating is the **sidecar format and the device-side
reader** — the analyser behind it is replaceable and should be treated as such. `model_version` in
the schema (D2) is what makes swapping it a re-analysis rather than a migration.

State plainly in the tool's README that M1 valence is a heuristic. The 20-track labelled set (§4) is
how we find out whether it is good enough to ship, and it is the right place to be wrong cheaply.

### D2. Sidecar schema

**The key problem, stated honestly.** R3 rules out any key the device would have to compute from
audio. So the *device-side* match must use something already in `MTPDB.dat`: a path, or the tags.
A content hash is still worth storing — but as the **host's** own re-keying aid, never as the
device's lookup key.

| Key option | Survives rename | Survives retag | Verdict |
|---|---|---|---|
| `object_id` | no — renumbered on rescan | n/a | **No.** |
| Path only | **no** | yes | Alone, no. |
| Normalised `artist \t title` | yes | no (if those tags change) | **Yes, as primary.** Already proven: `likes.rs` and `likesync/keys.py` share this normalisation and resolve **129 of 131** liked tracks on the owner's real library **[V]** (`Sony-sync/README.md`). |
| Audio-stream hash | yes | yes | **Yes, as a host-side column** — lets the tool re-key a row after a rename *or* a retag without re-analysing. Never read by the device. |

**Recommended shape:**

```sql
CREATE TABLE meta (k TEXT PRIMARY KEY, v TEXT);   -- schema_version, tool_version, written_utc, row_count

CREATE TABLE track (
  key         TEXT PRIMARY KEY,   -- normalised "artist\ttitle" (likes.rs rules, byte-identical)
  rel_path    TEXT,               -- path under the music root; disambiguates duplicate keys
  audio_sha   BLOB,               -- 16 bytes over the decoded/framed audio. HOST ONLY.
  valence     REAL,               -- 0.0..1.0, 0 = negative/sad, 1 = positive/happy
  arousal     REAL,               -- 0.0..1.0, 0 = calm, 1 = energetic
  bpm         REAL,               -- > 0, estimated tempo
  danceability REAL,              -- 0.0..1.0
  loudness_lufs REAL,             -- integrated, EBU R128 (negative)
  hook_ms     INTEGER,            -- offset of the best hook, or NULL when undetected
  confidence  REAL,               -- 0.0..1.0, the analyser's own confidence
  model_version TEXT NOT NULL     -- per ROW, so a partial re-analysis is legal
);
CREATE INDEX track_va ON track(valence, arousal);

CREATE TABLE track_vec (key TEXT PRIMARY KEY, dim INTEGER, vec BLOB);  -- separate: skippable
```

Three rules that are the point of the schema:

1. **Documented fixed scales, not free-floating numbers.** `0.0..1.0` for the three normalised axes; BPM in BPM; loudness in LUFS. Written in the `meta` table too, so a file is self-describing.
2. **`model_version` per row.** Re-analysing 200 tracks must not invalidate 3,300.
3. **No baked-in channel mask.** The sidecar stores *values*. Which values make "Late Night" is the channel file's business (D4), so a user can redefine a channel without re-running anything.

`track_vec` is a separate table so a device that does not want the similarity vector never pays for
reading it. On a 3,500-track library that is roughly 7 MB at 512 f32 — measure before shipping it.

### D3. Where the file lives on the player

| Option | Trade-off |
|---|---|
| **`/contents/cinder_analysis.db`** | The MSC-exposed vfat volume, beside `cinder_playlists/`, `cinder_palettes/` and `cinder_liked_import.tsv` — the pattern already shipping. A PC writes it with no adb and no root. **But vfat has no journal [V], and the partition is repeatedly taken away and handed back [V]**, which is how `/contents` failed badly enough to brick the device once (`VISION.md`). Also: **1.5 GB free [V]**. |
| `/db/cinder_analysis.db` | ext4, journalled, 83 MB free **[V]**, beside `MTPDB.dat`. But not reachable over MSC — needs adb or root, so a PC tool cannot write it. |
| Write to `/contents`, validate, copy to ext4, read the copy | vfat as transport only. `/data` has just 18 MB free **[V]**; `/db` has 83 MB but whether uid 100 may **write** there is **[U]**. |

**Recommendation: `/contents/cinder_analysis.db` for M1**, read-only, with a self-check that refuses
a bad file rather than trusting it — the same posture `likes.rs` already takes (*"a file without the
header is not one of ours"*, *"a file whose rows resolve to nothing is ignored and left in place"*)
**[V]**. Concretely: `meta.row_count` must match `SELECT COUNT(*) FROM track`, `schema_version` must
be one we know, and a failure logs one line and leaves the library untouched.
Revisit the copy-to-ext4 option after M1 measures the real file size and the real open cost.

### D4. Channel definition format

**A text file, exactly like palettes.** `/contents/cinder_channels/*.channel`, reusing the palette
loader's proven shape **[V]** (`player/cinder-ui/src/palette.rs`: `key = value`, `#` comments,
`MAX_BYTES`, `MAX_FILES`, refuse-and-log, case-insensitive extension for FAT).

```
# late-night.channel
name  = Late Night
where = arousal < 0.35
where = valence > 0.30
bpm   = 60..95
order = arousal asc
limit = 200
```

* Multiple `where` lines **AND** together. No `or`, no parentheses, no expressions.
* **Deliberately not a language.** `PLAN_skins.md`'s own verdict on layouts-as-data applies verbatim: writing an interpreter is *"a project of its own, and you pay for it in frame time and battery on the player's Cortex-A7"* **[V]**.
* **Ship 6–8 built-in channels compiled in**, so the feature works with no files at all — the same rule as the built-in `cinder` palette **[V]**.
* A channel that matches nothing must render the empty state, not a blank screen. This repository has already fixed that exact defect once (`AUDIT_2026-09-06_ui.md`, quoted in `STATUS.md`) **[V]**.

### D5. How channels map to the existing queue and player

**A channel is a context. Nothing else.** Build a `Vec<SongRow>` and hand it to the same path an
album or playlist uses — `start_play_action` / `Action::PlayContextAt` **[V]** (`nav.rs`), which
`play_order_uris` then turns into file paths for `SetTrackSequence` **[V]** (`cinder-ffi/src/lib.rs`).

Three facts that make this cheap, all already measured:

* `SetTrackSequence` is **flat at ~0.25 s from 1 to 512 tracks** **[V]** (`cinder-probe --seqtime`, quoted in `lib.rs`).
* The whole-library map used to build a play order costs **7.5 ms** **[V]** (`AUDIT_BATTERY_PERF_2026-09-05` §A3).
* The shell already caps a sequence at 512 **[V]**. Channels take the same cap.

**No new playback code.** "More like this" is the same thing: nearest neighbours around the current
track become a context.

### D6. UI screens

| Screen | Recommendation |
|---|---|
| **Channels list** | A new Library tab, or a Menu entry. Reuses the existing list renderer and empty state. **Cheapest, do it first.** |
| **Mood map** | The one genuinely new render: valence on x, arousal on y, a dot per track, drag a selection to make a context. On a 3,463-track library that is 3,463 dots per frame on a 598 MHz A7 with 1.5 MB of canvas — **budget it with `render_bench` before building it**, and expect to need a pre-binned grid rather than per-track dots. |
| **More like this** | A row on the track-info sheet, **not a screen**. |
| **Hook preview** | Park it until R2 says hook detection works at all. |

---

## 3. Milestones, smallest first

| # | Deliverable | Done when |
|---|---|---|
| **M1** | **Host tool writes the sidecar for a test folder; Cinder reads it; one channel list plays.** One hard-coded channel, no channel files, no mood map, no vectors. | The tool analyses ~20 tracks into `cinder_analysis.db`; `cinder-db` opens it read-only and resolves rows by the normalised key; one channel appears in the Library and plays through the existing context path. |
| **M2** | Channel files + built-ins | `cinder_channels/*.channel` loads with the palette loader's rules; 6–8 built-ins work with no files present; a broken file is skipped with one log line. |
| **M3** | Mood map | Renders inside the frame budget on the real library size; a drag selects and plays. Golden hashes recorded. |
| **M4** | More like this | A row on track info builds a neighbour context. |
| **M5** | Similarity vectors, hook offsets | Only if M1's values prove good enough to build on, and only after the size and load cost are measured. |

Between M1 and M2, decide D3's copy-to-ext4 question with the measured file size in hand.

---

## 4. Test plan

**Host-side unit tests.**
* Key normalisation is **byte-identical to `likes.rs`** — the same fixture list run through both, asserted equal. This is the one test that, if it fails, makes everything else meaningless. (`likes.rs`'s own comment already says the two sides *"must agree or nothing lines up"* **[V]**.)
* Every value in its documented range; `model_version` non-empty on every row.
* Round trip: write a sidecar, read it with `cinder-db`, get the same values back.
* Refusal cases, mirroring the `likes.rs` posture **[V]**: unknown `schema_version`, `row_count` mismatch, a truncated file, an empty file, a file that is not SQLite at all. Each must leave the library untouched and log one line.
* Channel parsing: unknown keys ignored, a malformed `bpm` range skipped with a reason, a channel matching nothing yields an empty list rather than an error.

**A labelled test set — 20 tracks sorted by hand.**
The owner picks 20 tracks spanning the corners (calm-sad, calm-happy, energetic-sad, energetic-happy)
and ranks them on each axis. Then:
* **Rank correlation, not absolute error** — the question is whether the ordering is sane, not whether a number hits a target.
* **BPM is checkable exactly** against a tapped or known value; a half/double-time error is the expected failure and should be reported as such rather than silently halved.
* Publish the result honestly. If valence ranks no better than chance on 20 tracks, that is the finding, and D1's option (b) or (c) is what it argues for.

**`cinder-host` / `cinder-sim` renders for any new screen.**
* Every new screen gets a `cinder-host` preview and a pixel hash in `player/cinder-host/golden.txt` (234 hashes today **[V]**, `PLAN_2026-09-14.md` §A).
* Day and night, at 100 % and 140 % UI scale, against an **empty** sidecar and a full one.
* `cinder-sim` click-through for the mood map's drag.
* `tests/ui_overflow.rs` must still pass — nothing drawn off the panel.

**New lines for `docs/DEVICE_CHECKLIST.md`** (drafted here, to be moved there when M1 lands):

| # | Item | Do | PASS | If it fails |
|---|---|---|---|---|
| 14.1 | **Sidecar missing** | Boot with no `cinder_analysis.db` | The Channels tab shows its empty state and names the missing file; the rest of the library is unaffected; one log line | A blank screen, or a library that fails to load, means the reader is not isolated from the rest of `build_library` |
| 14.2 | **Sidecar corrupt** | Truncate the file to half its length over USB, and separately write 100 bytes of junk with the right name | Refused with a reason in `cinderhome.log`; the file is **left in place**, not deleted; library unaffected | A crash, or a silently empty channel list with no log line |
| 14.3 | **Large library load time** | A full-library sidecar (~3,500 rows). Read the `cinder_db_open` / `build_library` breakdown the app already logs **[V]** | The added time is stated as a number, and the boot path is not measurably slower than the recorded baseline | If it is significant, this is what decides D3's copy-to-ext4 question |
| 14.4 | **Memory** | `/proc/<pid>/status` `VmRSS` before and after, with the sidecar loaded | The increase is stated in KB. Context: `MemTotal 467512 kB`, **no swap** **[V]** | An increase large enough to matter means the in-memory index must be by-id, not by-string |
| 14.5 | **Playback is unaffected** | Play a channel for a full track; then the mood map open while playing | No audio stall, no dropped frames beyond the recorded baseline, scrobble written as usual | The mood map is the suspect — it is the only new per-frame work |
| 14.6 | **Survives a USB round trip** | Write the sidecar over MSC, unplug, open Channels; then plug in, modify it, unplug | Picked up on the next read without a reboot, the way `cinder_palettes` is rescanned when Settings opens **[V]** | Note which event triggers the rescan; a file that needs a reboot is a worse feature than one that needs a screen re-open |

---

## 5. Open questions for the owner

**Q1. Where does the host tool live — this repository, or `sony_sync.py`?** *(asked, not guessed)*
* **In Cinder** (`tools/analyser/`, Rust): one language with the rest of the tree, MIT by default, CI can test it, and it sits next to the reader it must agree with. Costs: a second Rust binary to build and release, and it is not where the owner's music workflow already lives.
* **In Sony-sync** (Python): it is already the thing that walks the library and writes to the player, and the key normalisation already lives there (`likesync/keys.py`). Costs: Sony-sync is **deliberately standard-library only** **[V]** — decoding audio and doing FFTs would end that, which is a real property to give up.
* A third shape exists: the analyser is a Rust binary in Cinder, and Sony-sync *invokes* it. That keeps Sony-sync dependency-free and puts the DSP where the licence story is simplest.

**Q2. The 20-track labelled set** — will you pick and rank them, and on which axes (valence and arousal only, or danceability too)?

**Q3. Is a rough valence acceptable for M1?** The honest expectation for option (a) is that arousal and BPM will be usable and valence will be weak. Ship it labelled as approximate, or hold channels until a better analyser is chosen?

**Q4. Does any `MTPDB.dat` you still have contain non-null akeys 51–59?** Two SQL lines in R4 settle it. Bonus only — the plan does not depend on it.

**Q5. Similarity vectors: worth ~7 MB on a partition with 1.5 GB free [V], or leave "more like this" to the four scalar values?**

**Q6. Skip tracking** — add `"S"` rows to `.scrobbler.log` now (and change the uploader in the same release), or leave it out entirely? R5 recommends leaving it out until M4.

**Q7. Channel screen placement** — a new Library tab, or a Menu entry? The Library tab strip already has to fit its existing tabs at 140 % scale.

---

*Not indexed in [`README.md`](README.md) yet — add a row when M1 starts, per this directory's rule
that every document here is indexed.*
