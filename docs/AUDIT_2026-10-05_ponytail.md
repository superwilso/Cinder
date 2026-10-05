# Ponytail audit — 2026-10-05

**What can be deleted, merged, or should never have been written**, across all four repositories.
One question per finding, asked in this order, stopping at the first that holds: *does it need to
exist? · is it already in the tree? · does the standard library or the platform do it? · can it be
one line?* Correctness, security and boot safety were never on the table (Part F).

| Repository | Tree audited | Changed here |
|---|---|---|
| Cinder | `a8cf697` | Part B — **−678 / +69 lines** of code, scripts and config; 9 build outputs and 2 stray gitlinks untracked |
| Flint | `f6ab2ca` | Nothing. Dead-code scan is clean; findings D1, D2 are merges, not deletions |
| cinder-sony-analysis | `1d7a894` | `ui/walkman_one/_superseded_2026-09-22/` deleted (26 PNGs, 1.3 MB, nothing links to it) |
| cinder-themes | `deae0f0` | `layouts/` deleted: a folder whose only file said "empty on purpose, don't send anything" |

Evidence classes as in `docs/README.md`: *measured*, *verified* (by a gate or a grep over the whole
tree), *inferred*.

---

## Part A — Gates, before and after

All run in this session, on the host. Nothing here is device-verified; nothing changed needs to be.

| Gate | Before | After |
|---|---|---|
| `cargo test --release` (player) | 796 passed, 3 ignored | **794** passed, 3 ignored (−3 tests of deleted code, +1 test that had never run — B3) |
| `cinder-host --check` (361 golden pixel hashes) | match | **match** — the helper merges in B4 move no pixel |
| `cinder-host --audit` | clean | clean |
| clippy `correctness` + `suspicious` | clean | clean |
| installer tests | 53 | 53 |
| `tools/host_syntax_check.sh` | 26 files parse | 26 files parse |
| C++ self-tests | 13 in CI, 12 in build.sh, 11 in pre-push | **13 everywhere** (B6) |
| harness (`cinder-home/harness/run.sh`) | all scenarios | all scenarios |
| launcher matrix | 108 pass (root; one case skips itself) | unchanged (not touched) |
| mono shim through `LD_PRELOAD` | ok | ok |
| `tools/shell_check.sh` (shellcheck 0.11.0) | 53 scripts clean | 53 scripts clean (+ `selftests.sh`) |
| Flint `cargo test --workspace --release` | 173 passed, 2 ignored | unchanged |

---

## Part B — Done in this pass (Cinder)

**B1. Thirteen FFI exports the shell never calls — deleted.** *Verified* by grep over every C, C++,
Python and Rust file: `cinder_set_now_playing`, `cinder_set_theme_night`, `cinder_set_visualizer`,
`cinder_set_visualizer_type`, `cinder_visualizer_count`, `cinder_set_pcm`, `cinder_set_volume_limit`,
`cinder_is_liked`, `cinder_toggle_liked`, `cinder_liked_count`, `cinder_bt_paired_count`,
`cinder_bt_found_count`, `cinder_bt_prompt_kind`. `STATUS.md` had listed six of them as "exports the
shell never calls" since August; they were documented as dead instead of being deleted. Their
`cinder.h` declarations went with them, and `spectrum::levels` (the bin-indexed FFT only
`cinder_set_pcm` reached — the live PCM tap uses `from_pcm`) with its three tests.

**B2. Eighteen `cinder-ui` items nothing references — deleted.** `az_has`, `hit_heart`,
`hit_toolbar` (both replaced by `now_playing::layout`), `tab_at` (replaced by `lib_tab_zones`),
`hit_new_playlist`, `current_song_object_id`, `is_playing`, `is_scrubbing`, `set_stats`,
`usb_dac_format`, `bt_paired_len`, `bt_found_len`, `bt_prompt_kind`, `range_index_of`,
`unresolved_report`, `chain_char_list`, `library::PAGE`, `CINDER_VERSION`, and the artist-page mock
data (`ARTIST_*`, `TopSong`). Method: every item name defined in `player/` and `installer/`, counted
across the tree with comments stripped; a name that occurs once is its own definition.

**B3. A test that never ran — restored, and it passes.** `cinder-ffi`'s `build_library_from_db` sat
in the test module without `#[test]`, so the compiler reported it as an unused function and nothing
ran it. It is the only test of `build_library` against a database fixture. *Verified*: passes as is.

**B4. The same helper written five times — now once.** `widgets::fill_rect` and `widgets::sty` were
already public; `chrome.rs` and `lock.rs` each carried byte-identical private copies of both.
`eq.rs`, `tone.rs` and `dac_eq.rs` each carried the same `disc`. One `widgets::disc` now.
*Verified*: every golden hash unchanged. (`icons::disc` is NOT the same function — it centres with
`Circle::new` and rounds differently — and was left alone.)

**B5. One atomic write, not six — root cause, not symptom.** `lib.rs` had `write_atomic`: temp file,
`fsync`, rename, temp removed on failure. Five other saves hand-rolled temp + rename **without
fsync and without cleanup**: saved views, liked songs, the loved-songs TSV, track stats
(`stats.rs`) and playlists (`playlists.rs`, whose own comment explained that `/contents` is flash a
user unplugs). All five now call `write_atomic`. Shorter, and the one failure mode the code was
written to prevent — a torn file after an unplug — is now prevented on every path that claimed to.
`art_cache::store` (sets permissions between write and rename) and `write_png` (streams an encoder)
keep their own.

**B6. The self-test list existed three times and had drifted.** CI ran 13, `build.sh` 12 (never
`mono`), `.githooks/pre-push` 11 (never `codeceq` or `soundscape`) — under a comment reading "keep the
two lists equal". All three now call `cinder-home/tools/selftests.sh`, which globs
`*_selftest.cpp`: there is no list left to keep equal. `build.sh` loses 112 lines of twelve
copy-pasted blocks (one of which sat under the comment written for a different test). Each test file already
carries its own rationale at the top.

**B7. Fifteen `cinder-audio` shim functions nothing calls — deleted.** Not the shell, not
`cinder-probe`, not the harness: `cinder_audio_stop`, `cinder_audio_pump_stop`,
`cinder_analyzer_log_reset`, `cinder_codec_set_playback_latency`, six `cinder_effects_is_*`
read-backs, and five `cinder_sound_settings_set_*` setters for DSD/LPCM modes no screen offers. The
ABI each wrapped stays recorded in its `*_abi.hpp`, so re-adding one is a one-liner when a feature
needs it. *Verified*: syntax check, harness and mono shim test all pass.

**B8. Build output in the repository.** Tracked although every one is rebuilt by `build.sh` and
copied into `dist/<channel>/` (the committed copy): `cinder-home/cinder-{battery,clock,fm,gpunode,
msc,power,umount,voltable}` and `cinder-home/libcinder_mono.so` — the last one *already listed in
`.gitignore`*, which is how three of them differed from `dist/stable/`. Also tracked: two
`.claude/worktrees/agent-*` gitlinks (submodule entries with no `.gitmodules`, left by agent
sessions) and `artifacts/session/battery_track.tsv` under an `artifacts/` that is ignored wholesale.
All untracked; `.gitignore` now says `/cinder-home/cinder-*` once instead of naming files.

---

## Part C — Delete next (owner's call; largest payoff first)

Each of these is either shipped to users or built by CI, and each is dead or superseded by the
tree's own account. Not done here because each is an owner decision about a feature, not a typo.

**C1. `cinder-probe` out of the stable payload — 4.77 MB, 47.7 % of it.** *Measured*: 4,767,876 of
9,995,498 bytes in `dist/stable/`. `installer/build.rs` stages it to every user's drive root;
`install_cinderhome.sh` never installs it; stable has no adb, so nothing on a stable player can run
it. It is the RE workbench (8,740 lines of `--probe` modes) and belongs on the dev channel, exactly
like `cinder-gpunode`. Change: `required: false` + dev-only in `build.rs`, drop it from the CI
heredoc, `release.sh` and `check_arm_payload.sh`.

**C2. The GPU present path — 678 lines plus a setuid helper, for a path measured 4.7× slower.**
`STATUS.md`: "default OFF and measured **4.7× slower** than the software one". It still costs:
`gpu.rs` (596), `cinder-gpunode.c` (82) — setuid root, chmods four kernel nodes to 0666 — the
install-script branch, the `cinder_gpu_on` switch, and `-l:libMali_linux.so` linked into the
**stable** Home app. Delete all of it; the software framebuffer is the shipped renderer.

**C3. `ldac-bridge/` — 866 lines (code, scripts, its two docs) that, by the tree's own account, cannot work.** `main.cpp`'s
USB-DAC → LDAC section: "a standalone daemon pumps nothing, so its calls do not fail — they return
uninitialised stack". The feature shipped inside `cinder-home` and is device-verified. The bridge is
still syntax-checked by CI and pointed at by `VISION.md` and `ROADMAP.md`.

**C4. `player/cinder-device` + `player/deploy/` — 499 lines of the superseded SIGSTOP overlay.**
Nothing builds, packages or installs it; CI compiles it on every run because it is a workspace
member.

**C5. `cinder-sim`'s old click-driven browser (`src/main.rs`, 548 lines).** The crate's own manifest
calls it "the older" binary; `--bin device` drives the real `nav::App` and is what `/verify` uses.
Deleting it also lets `data.rs`'s remaining mock tables go.

**C6. The PNG encoder inside the Windows installer (`png.rs`, 155 lines).** It exists so
`--screenshots` can write README images, and `render_installer_screenshots.sh` decodes and
re-encodes every file in Python anyway. Write PPM (`P6\n{w} {h}\n255\n` + the RGB bytes, one line)
and let the Python step — which already has `zlib` and a chunk writer — produce the PNG.

**C7. LDAC bring-up diagnostics in the shipping app.** `main.cpp` "bridge diagnostics"
(`log_proc_file`, `ldac_dump_pcm`, `pcm_state_name`, …): written to explain a first device run that
stalled. The path has since run a logged 192 s session. Keep one line on failure; drop the dumps.

---

## Part D — Merge: one copy instead of N

**D1. Flint: the sync pipeline exists twice and has already diverged.** `flint/src/main.rs`
`sync_cmd` and `flint-gui/src/job.rs` `sync_job` repeat the same ~150 lines (scan library → adopt
SensMe tags → per-volume budget → playlists → plan), held together by comments ("the same sum the
command line does"). They differ already: the GUI drops sidecar files unless `extras`, the CLI never
does. Do what `library::scan` already does for scanning — one core function with an event callback,
two thin front ends.

**D2. The palette rules are written twice, in two repositories.** `flint-core/src/palette.rs` opens
with "this is a port, and it must stay one" of `cinder-ui/src/palette.rs` + `theme.rs`. cinder-themes
already consumes the rules from `flint-core` by git. One dependency-free palette crate (in Flint,
which clones in seconds; Cinder's history does not) used by `cinder-ui`, Flint and the checker
retires the port and the drift risk the port's tests exist to catch.

**D3. The payload list is hand-written in eight files.** `installer/build.rs`, the CI `payload`
heredoc, `tools/release.sh`, `tools/check_arm_payload.sh`, `tools/cinder-install.sh` (three loops),
`install_cinderhome.sh`, `uninstall_cinderhome.sh` and `build.sh`. `PAYLOAD.sha256` already *is*
the list, written by `release.sh`. Smallest step: the CI heredoc becomes
`grep -v '^#\|^tag' cinder-home/dist/PAYLOAD.sha256 | sha256sum -c -` — one line, and it checks
content, not just presence. *Verified*: passes on this tree.

**D4. Four power tools, ~930 lines, two of them the same method.** `btpower.sh` and
`idle_probe.sh` both arm a sampler, wait for the cable to come out, take two cumulative-counter
snapshots and read them back after replug. `battery_track.sh` (long-run) and `pmtest.sh`
(suspend/resume) answer different questions. Fold `idle_probe` into `btpower` as a label.

**D5. `render_release_notes.sh` is copied between Cinder and Flint** (88 and 117 lines). GitHub
release pages now show each asset's SHA-256 digest natively; if that is enough, both copies and the
CI step that renders a fake release body go. If not, one script in one place.

**D6. `tools/flash.sh` (397 lines) re-implements the installer's last step.** It needs Rockbox's
`scsitool` to send the update trigger that `cinder-installer` now sends itself over `SG_IO` on
Linux. A dev-channel installer covers `flash.sh install|uninstall`; what remains (`--cat`, `--push`,
`--pull`) is `mount` + `cp`.

**D7. cinder-themes' `accept.yml` re-derives a palette id in Python** (lower-case, dash, 32 chars,
`cinder` → `cinder-2`) that `flint_core::palette::id_for` already computes. Have the checker print it.

---

## Part E — Prose debt

**E1. Comments that are a changelog.** Comment share: `main.cpp` 43 %, `install_cinderhome.sh` 41 %,
`ci.yml` 43 %, `player/Cargo.toml` 55 comment lines around `opt-level = 2`. `main.cpp` has 194
comment lines carrying a date and 80 narrating what the code *used to* do. The *why* belongs at the
code; the *story* belongs in the commit message, `CHANGELOG.md` or an audit — all of which this
project already keeps. Rule for new code: if a comment would still be true after `git blame` is
gone, keep it; if it is a story, move it.

**E2. More documentation than anyone can keep current.** *Measured*: 35,272 lines of Markdown
(`docs/` alone 15,191) beside 116,077 of code and scripts; 16 audits and 16 plans/specs. Two files
each call themselves the next-steps list (`PLAN_2026-09-14.md`: "start here if you are asking what
should I do next"; `AUDIT_2026-10-01.md` Part H). `CLAUDE.md` is read into every session and still
carries a block titled "HISTORY, superseded by the banner above" and Part G's July progress notes.
Delete the history from `CLAUDE.md`; pick one next-list; delete rather than archive the documents
`docs/README.md` already marks as history (`ROADMAP.md`, `PRODUCTION_READINESS.md`, `FLASH_NEXT.md`,
`audit_notes.md`, `AUDIT_2026-07-26.md`) — git keeps them. `cinder-home/README.md` still describes
the June build and a `src/render.c` "to add".

**E3. An open decision is a cost too.** `HISTORY_REWRITE.md` + `tools/rewrite_history.sh` (238
lines) have been "prepared and rehearsed, never pushed" since 2026-09-01, and `.githooks/pre-push`
warns about `dist/stable` weight on its behalf. Run it or delete both.

---

## Part F — Kept on purpose

The lazy answer was considered and refused for each of these.

* **Eight separate setuid helpers**, not one multi-call binary: each does one hard-coded thing; a
  combined one would hold every privilege at once.
* **The storage shim's `EnableExportAsMsc` symbol** (unused as a call): it is part of the
  all-or-nothing check that refuses a `libStorageMgrServiceFw.so` other than the one reverse-engineered.
* **The launcher's escape ladder, guards and watchdogs**: their size is what stands between a bad
  build and a device that needs `wbrt`.
* **Hand-rolled MD5, XML, HTTP, PNG decode, Win32** in the installer and Flint: the price of the
  zero-dependency rule both state, paid once each.
* **The checks**: harness, golden hashes, self-tests, launcher matrix. B3 and B6 are cases of checks
  that silently were not running; the fix was to make them run, not to have fewer.

---

## Part G — Ordered next list

Smallest diff first; each stands alone.

1. D3's one-line `sha256sum -c` in CI.
2. C3 `ldac-bridge/`, C4 `cinder-device`, C5 the old `cinder-sim` binary — pure deletions, gated by `cargo test` and the syntax check.
3. E2 the history block out of `CLAUDE.md`; delete the five history docs.
4. C1 `cinder-probe` dev-only — needs a release to take effect.
5. C2 the GPU path — needs a release; removes a setuid helper from the dev channel.
6. D1 Flint's sync core; D2 the shared palette crate.
7. C6, C7, D4–D7 as each file is next touched.
