# Cinder documentation

Many files land in this directory over time, most of them dated audits. This index says which
one to open and, just as importantly, which ones are **history rather than current state** — several
were accurate on the day they were written and have been overtaken since. Where that is true it says
so on the row.

The four documents that are always current live outside this directory:

| | |
|---|---|
| [`../README.md`](../README.md) | What Cinder is, and how to install it. |
| [`../RECOVERY.md`](../RECOVERY.md) | **Read before flashing anything.** No public DFU or EDL path exists for this device. |
| [`../cinder-home/STATUS.md`](../cinder-home/STATUS.md) | The feature matrix — current state, kept current rather than aspirational. |
| [`../VISION.md`](../VISION.md) | The living goals list and the rationale. |

---

## Start here

| Document | What it is |
|---|---|
| [`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md) | **The redesign ledger.** Every screen in the 2026-09 design handoff (Cinder, Flint, the installer) and every promised feature the handoff does not draw — the goals, Walkman One parity, the community's requests — each with a state and a phase. What landed first, and where the build departs from the mock on purpose. |
| [`SPEC_redesign_2026-09.md`](SPEC_redesign_2026-09.md) | The redesign's spec: the shared row / section / switch / chip anatomy, the tokens, and each screen's changes. `player/cinder-ui/src/kit.rs` is held to it. |
| [`PLAN_sound_2026-09-29.md`](PLAN_sound_2026-09-29.md) | **Sound settings.** Which levers change the signal (region table, volume curve, headphone amp, Sony DSP, tone table) and which don't (Walkman One's signatures and tuning), what the live codec tables hold, and the order: linear-amp listening test, then the curve decision, then decoding the tone and device-gain tables. |
| [`DESIGN_GUIDE.md`](DESIGN_GUIDE.md) | **Designing for Cinder.** The one guide for anyone making a palette, a mockup or a style: the device, the three layers (palette, style, accent), the screen's bands, colour tokens, type roles, touch targets, gestures, glyphs, Now Playing's must-haves, performance limits, and how to contribute. |
| [`PLAN_design_styles.md`](PLAN_design_styles.md) | **Design styles.** Nocturne and Terminal for Now Playing (built 2026-09-29), the pure layout both draw and tap read, the contract every style passes, and the order for the rest: the kit, the header, Lock and Menu, the Library. |
| [`PLAN_pcm_visualiser.md`](PLAN_pcm_visualiser.md) | **The visualiser from the decoded audio.** PlayerService's PCM queue in `/dev/shm` is readable by cinder-home, so our own FFT can replace Sony's 12-band analyzer for library playback: the layout found, the safety rules (`pread`, never `mmap`), and the order. |
| [`PLAN_now_playing_layouts.md`](PLAN_now_playing_layouts.md) | **Custom Now Playing screens, shared as files.** A bounded `.layout` format for Now Playing only (an exception to "skins only" that the owner has to accept), the checks the player applies, sharing through the community repo and Flint, and the order: Now Playing `LayoutMap` first. |
| [`PLAN_community_2026-09-23.md`](PLAN_community_2026-09-23.md) | **What users reported and asked for** (GitHub #14/#16, the r/walkman thread): each bug with its root cause and state, the feature requests ranked, and the facts each reply needs. |
| [`PLAN_2026-09-14.md`](PLAN_2026-09-14.md) | **The current audit and forward plan.** Every offline gate re-run against v0.3.4, what is open and in what order, the revised skins contract, and how the maintainer watch works. Start here if you are asking "what should I do next". |
| [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) | **The run sheet.** Every device-gated item in the project in one ordered list, safety rules first, with what a PASS looks like for each. If you have the player in your hand, this is the file. |
| [`SHORTCOMINGS.md`](SHORTCOMINGS.md) | Standing reference: what is structurally weak about the project and its repository, with evidence per claim. Cited by section ID (A1, B1, D4…) from other documents. |
| [`HISTORY_REWRITE.md`](HISTORY_REWRITE.md) | **Open repository decision.** Taking Sony's files and superseded build output out of every commit — prepared and rehearsed, never pushed: what it removes, what it breaks, and the order to do it in — and the commands that first move Sony's files to a repository of their own. |
| [`DEVICE_TESTS.md`](DEVICE_TESTS.md) | The backlog of things only ears or a hand can settle, ordered by payoff. |
| [`DEVICE_SHELL_GOTCHAS.md`](DEVICE_SHELL_GOTCHAS.md) | The device's busybox is not your shell. Written after an install reported success while doing three things wrong. Read before writing anything that runs on the player. |

## Reference — how the device actually behaves

| Document | What it is |
|---|---|
| [`baseline_v1.4.md`](baseline_v1.4.md) | The NW-A55 technical baseline, with explicit trust tiers per claim (`[Verified]` / inferred). The longest-lived document here. |
| [`AUDIT_2026-08-18_device_vs_sony.md`](AUDIT_2026-08-18_device_vs_sony.md) | What the hardware can do versus what Sony's services expose. Prompted by the FM result: the chip could seek and measure signal; `TunerPlayerService` could not. |
| [`COMPARISON_cinder_wampy_sony.md`](COMPARISON_cinder_wampy_sony.md) | How Cinder, Wampy and the stock player each solve the same problems. Bluetooth and FM rows revised 2026-08-26. |
| [`adb_setup.md`](adb_setup.md) | adb on the dev channel — fast iteration and reverse-engineering access. |
| [`open-questions.md`](open-questions.md) | What is still unknown about the device. Several entries closed by on-device work; see the header. |
| [cinder-sony-analysis](https://github.com/superwilso/cinder-sony-analysis) | **Reverse-engineering without the player.** The Sony-derived material kept out of this repository: a scrubbed map of a running NW-A55 (services, sockets, kernel config, I2C, mixer), catalogues of the A50 and ZX100 firmware images (files, hashes, dependencies, every exported symbol), and the Clear Bass tables and decompilations. Regenerated with [`../tools/device_map/device_map.py`](../tools/device_map/device_map.py) and [`../tools/firmware_catalogue.py`](../tools/firmware_catalogue.py). |

## Plans and specs not listed above

| Document | What it is |
|---|---|
| [`PLAN_walkman_one_parity.md`](PLAN_walkman_one_parity.md) | Feature parity with Walkman One, row by row, and what each gap costs. Its USB-DAC row ("recovered, not shipped") disagrees with `STATUS.md` and checklist 11.10 — see `AUDIT_2026-10-01.md` D3. |
| [`VISION_four_builds.md`](VISION_four_builds.md) | **Draft for selection (2026-09-21).** Cinder as a full firmware replacement: four builds, what can go in each, and the decisions the owner has to make. Nothing in it is decided. |
| [`SPEC_cinder_one.md`](SPEC_cinder_one.md) | **A proposal (2026-09-22), nothing built.** The concrete design behind `VISION_four_builds.md` §7's "Cinder One — yes or no?". |
| [`SPEC_queue_v2.md`](SPEC_queue_v2.md) | Up Next v2 as a context chain — the design (revised 2026-09-22 after two reviews) behind Up Next as one list and the queue extras. |

## Subsystems

| Document | What it is |
|---|---|
| [`PLAN_bluetooth_stack.md`](PLAN_bluetooth_stack.md) | Reaching the Bluetooth stack below Sony's services. Rewritten 2026-08-19 from measurement — the earlier route was wrong. |
| [`SCREENSHOTS.md`](SCREENSHOTS.md) | Every screen in one gallery, rendered by `cinder-host`, plus the Windows installer. The README links here instead of carrying the tables. |
| [`PALETTES.md`](PALETTES.md) | Writing a palette: the colour keys, the readability rules the player enforces, and previewing one on a PC. |
| [`PLAN_skins.md`](PLAN_skins.md) | Swappable UIs: palettes (done), then skins behind one layout contract — the coupling measured, the design, and the order. |
| [`BATTERY_BT.md`](BATTERY_BT.md) | Battery during Bluetooth playback: the measurement method first, then the finding. Written before any optimisation on purpose. |
| [`PLAYLISTS.md`](PLAYLISTS.md) | Playlists made on the device (`.m3u8` under `/contents`, negative ids). |
| [`LIKES_SYNC.md`](LIKES_SYNC.md) | The liked-songs device ⇄ PC contract, and the TSV format it crosses as. |
| [`PLAN_sensme_sync.md`](PLAN_sensme_sync.md) | SensMe channels: where the analysis comes from, why tagging the copy on the player is the route, and the PC tool ([Flint](https://github.com/superwilso/flint)) that writes it. M0 device-proven; M1/M2 landed 2026-09-18. |
| [`PERF_PLAN_2026-08-20.md`](PERF_PLAN_2026-08-20.md) | The remaining list-render cost, against measured `render_bench` numbers rather than estimates. Its P2/P3/P6-half landed; **P4 and P5 are still open** — see `AUDIT_BATTERY_PERF_2026-09-05.md` §B5, which also argues they should be re-costed after the compiler-profile change rather than taken at these numbers. |

## Audits

Each is a point-in-time pass with the tree SHA it was run against. Newer audits supersede older
ones where they overlap; the header of each says what it covers.

| Audit | Scope | Standing |
|---|---|---|
| [`AUDIT_2026-10-01.md`](AUDIT_2026-10-01.md) | **Cinder and Flint together**: every offline gate in both repositories re-run, the 0.3.12–0.3.14 code read, Flint read end to end, and the contracts between the two checked by test (scrobble log, likes, palettes, playlists, the sync root). Two cross-repo defects: Flint's sync roots at the drive and sweeps `MUSIC/`, and Flint sent `#TZ/UNKNOWN` scrobble times unconverted. Feature proposals for both. | **Most recent.** Part H is the ordered next list. |
| [`AUDIT_2026-09-28_power_sound_w1.md`](AUDIT_2026-09-28_power_sound_w1.md) | Battery, performance and sound, measured on Walkman One. Screen-off kept the display pipeline scanning at 60 Hz; stage-1 early suspend is now on by default (idle and jack playback) and device-verified, including a cable pulled mid-suspend that had blocked deep idle. Also: the rescan loop after USB-MSC (13 rebuilds for no new music) and Play doing nothing after MSC. Sound: bit-perfect for CD audio; the owner's W1 tuning is not applied. | Most recent **power/sound** pass. Part D is its open list. |
| [`AUDIT_2026-09-23.md`](AUDIT_2026-09-23.md) | The gates re-run, plus the Walkman One, boot-guard and install-mount work since 09-18. Five fixed: the boot guard's rescue (and two other `.appcfg` restores) wrote the file unreadable to uid 100; a `/data` sentinel left by live installs made the next update skip the cable pass (it was on the owner's player); the mount test was never in CI; the installer could not say why a player ignored an install; a misplaced doc comment. Recorded: v0.3.9, the Latest release, cannot start after a fresh install. | Part D is the ordered next list. |
| [`AUDIT_2026-09-18.md`](AUDIT_2026-09-18.md) | The gates re-run, plus the seams SensMe and the battery report exposed. Three fixed: a voltage-derived battery gauge the app chased and that could power the player off on one sagged sample; a glyph cache that made a character's pixels depend on which screen was drawn first (and drew some text at the wrong size at UI scale 140%); and SensMe itself, whose research and device proofs were finished while nothing in `player/` mentioned it. Five findings left open with reasons, including channel names that are inferred rather than verified. | Current; superseded as the next-steps list by `AUDIT_2026-09-23.md`. Part E is still the device session it needs. |
| [`AUDIT_2026-09-06_ui.md`](AUDIT_2026-09-06_ui.md) | The UI and its consistency across the device — every screen read as code and looked at as pixels. Nine defects, all fixed (Folders reserving a Now Playing bar it never drew, the Menu's inert back chevron, an invisible and mis-scaled scrollbar on both pickers, a caption claiming NFC was not wired when it works, a Receiver switch that could not be moved, and no empty state on any Library tab). Four consistency findings reported and deliberately not fixed — row heights, the type scale, the header's right slot, and three different scrolling experiences. Also: the committed preview screenshots are two stale, divergent sets totalling 26 MB. | Most recent **UI** pass. Its Part C is the open design list and Part D an open repo decision. |
| [`AUDIT_2026-09-06_queue_playback.md`](AUDIT_2026-09-06_queue_playback.md) | The queue and playback system end to end: the queue/context model, the resolve-and-flush layer, the shell's transport and end-of-queue watcher, the sequence shim. Twelve defects, all twelve fixed — among them a swipe that queued a track and never told the shell, Up Next naming the previous song for the whole of a queued one, SHUFFLE starting playback on a paused player, and repeat-all both looping a truncated sequence and firing on a pause near the end of any track. | Current; supersedes the queue/playback half of `AUDIT_2026-08-16.md`. |
| [`AUDIT_BATTERY_PERF_2026-09-05.md`](AUDIT_BATTERY_PERF_2026-09-05.md) | Battery, performance and optimisation, whole tree. Twelve findings ranked by impact × effort, each tied to `file:line`. The headline is measured: the player is compiled `opt-level = "z"`, and `2` is 2.2–3.2× faster for +4.6% size. Also finds the early-suspend lever shipping disabled, the 2026-08-18 N+1 freeze surviving at four more call sites, and a fork-free volume path already written but never wired up. | Most recent **battery/performance** pass. Its Part D is the ordered work list; Part C is what must not be undone. |
| [`AUDIT_2026-09-01.md`](AUDIT_2026-09-01.md) | Repository, CI and release process, plus a defect pass over the guard/watchdog machinery. Found `main` red, two open defects in the guard, and 863 MB of committed binaries in a 1.3 GB `.git`. | Most recent **repository** pass. Its Part D is the open decision list. |
| [`AUDIT_2026-08-26_bluetooth.md`](AUDIT_2026-08-26_bluetooth.md) | Pairing, connecting, NFC tap-to-pair. | Most recent Bluetooth pass. |
| [`AUDIT_2026-08-24_deep_sweep.md`](AUDIT_2026-08-24_deep_sweep.md) | Cross-thread state, the setuid helpers, the untested shim layer, panic reachability in Rust, the SQL. | Current. |
| [`AUDIT_2026-08-24_stalled_bringup.md`](AUDIT_2026-08-24_stalled_bringup.md) | One defect: a bring-up that never completes froze the whole app. Found off-device by the harness's first exploratory run. | Fixed; kept as the worked example of what the harness is for. |
| [`AUDIT_2026-08-23_sound_effects.md`](AUDIT_2026-08-23_sound_effects.md) | The DSP/effects chain. Five defects, all fixed. | Current. |
| [`AUDIT_2026-08-23_three_reports.md`](AUDIT_2026-08-23_three_reports.md) | Three user-reported defects run to root cause. | Current. |
| [`AUDIT_2026-08-16.md`](AUDIT_2026-08-16.md) | Sony functional parity, queue/playback behaviour, and a measured performance + battery sweep. | Largely worked off; its ordering superseded the ROADMAP's. Its queue/playback half is superseded by `AUDIT_2026-09-06_queue_playback.md`. |
| [`AUDIT_2026-07-26.md`](AUDIT_2026-07-26.md) | Full project audit — every existing and planned feature against the code. | **History.** Useful for the touch-input sweep in §F6b; otherwise overtaken. |
| [`audit_notes.md`](audit_notes.md) | How two external audits were integrated into the v1.4 baseline. | History. |

## History — accurate when written, overtaken since

| Document | Why it is still here |
|---|---|
| [`../cinder-home/ROADMAP.md`](../cinder-home/ROADMAP.md) | The 2026-07-28 forward plan, now carrying a banner saying so. Several entries are still open; it predates Bluetooth, NFC, FM, playlists and the August audits. The live plan is `DEVICE_CHECKLIST.md`. |
| [`PRODUCTION_READINESS.md`](PRODUCTION_READINESS.md) | The 2026-07-28 gap list. Its headline count ("33 commits since the last hardware-verified one") is long out of date, but the *shape* of the argument — what has to be true before this is something a stranger relies on — is the one this project keeps returning to. |
| [`FLASH_NEXT.md`](FLASH_NEXT.md) | The 2026-07-28 flash run sheet. Superseded by `DEVICE_CHECKLIST.md`; kept as the model of what a run sheet should contain. |

---

## Conventions used in these documents

* **A claim states its evidence class.** *Measured* means a number was taken; *device-verified*
  means it was executed on hardware; *inferred* means it was reasoned from the binaries and has not
  been run. Documents that mix the three say which is which per claim, and the ones that do not are
  the ones that have misled a later session.
* **Dated filenames are deliberate.** An audit is a photograph, not a specification. When one is
  overtaken it is not edited into agreement — it is superseded, and the newer document says so.
* **Section IDs are stable.** `SHORTCOMINGS.md §A1` means the same thing in six months.
