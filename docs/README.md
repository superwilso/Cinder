# Cinder documentation

Twenty-six files land in this directory over time, most of them dated audits. This index says which
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
| [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) | **The run sheet.** Every device-gated item in the project in one ordered list, safety rules first, with what a PASS looks like for each. If you have the player in your hand, this is the file. |
| [`SHORTCOMINGS.md`](SHORTCOMINGS.md) | Standing reference: what is structurally weak about the project and its repository, with evidence per claim. Cited by section ID (A1, B1, D4…) from other documents. |
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

## Subsystems

| Document | What it is |
|---|---|
| [`PLAN_bluetooth_stack.md`](PLAN_bluetooth_stack.md) | Reaching the Bluetooth stack below Sony's services. Rewritten 2026-08-19 from measurement — the earlier route was wrong. |
| [`BATTERY_BT.md`](BATTERY_BT.md) | Battery during Bluetooth playback: the measurement method first, then the finding. Written before any optimisation on purpose. |
| [`PLAYLISTS.md`](PLAYLISTS.md) | Playlists made on the device (`.m3u8` under `/contents`, negative ids). |
| [`LIKES_SYNC.md`](LIKES_SYNC.md) | The liked-songs device ⇄ PC contract, and the TSV format it crosses as. |
| [`PERF_PLAN_2026-08-20.md`](PERF_PLAN_2026-08-20.md) | The remaining list-render cost, against measured `render_bench` numbers rather than estimates. |

## Audits

Each is a point-in-time pass with the tree SHA it was run against. Newer audits supersede older
ones where they overlap; the header of each says what it covers.

| Audit | Scope | Standing |
|---|---|---|
| [`AUDIT_2026-09-01.md`](AUDIT_2026-09-01.md) | Repository, CI and release process, plus a defect pass over the guard/watchdog machinery. Found `main` red, two open defects in the guard, and 863 MB of committed binaries in a 1.3 GB `.git`. | **Most recent.** Its Part D is the open decision list. |
| [`AUDIT_2026-08-26_bluetooth.md`](AUDIT_2026-08-26_bluetooth.md) | Pairing, connecting, NFC tap-to-pair. | Most recent Bluetooth pass. |
| [`AUDIT_2026-08-24_deep_sweep.md`](AUDIT_2026-08-24_deep_sweep.md) | Cross-thread state, the setuid helpers, the untested shim layer, panic reachability in Rust, the SQL. | Current. |
| [`AUDIT_2026-08-24_stalled_bringup.md`](AUDIT_2026-08-24_stalled_bringup.md) | One defect: a bring-up that never completes froze the whole app. Found off-device by the harness's first exploratory run. | Fixed; kept as the worked example of what the harness is for. |
| [`AUDIT_2026-08-23_sound_effects.md`](AUDIT_2026-08-23_sound_effects.md) | The DSP/effects chain. Five defects, all fixed. | Current. |
| [`AUDIT_2026-08-23_three_reports.md`](AUDIT_2026-08-23_three_reports.md) | Three user-reported defects run to root cause. | Current. |
| [`AUDIT_2026-08-16.md`](AUDIT_2026-08-16.md) | Sony functional parity, queue/playback behaviour, and a measured performance + battery sweep. | Largely worked off; its ordering superseded the ROADMAP's. |
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
