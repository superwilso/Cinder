# Cinder — battery, performance and optimisation audit, 2026-09-05

*Run against tree `7d55c97` (`refactor: Revert async library build changes…`), the head of
`claude/project-audit-battery-performance-9o98u0` at the time of writing. Offline: no device was
attached for this pass, so every device claim below is either cited from an existing
device-verified document or explicitly marked device-gated.*

## How to read the evidence classes

This document follows `docs/README.md`'s convention, and states the class **per claim**:

| Class | Means |
|---|---|
| **Measured (host)** | A number was taken on this machine today. The command is given. Host absolutes are not device absolutes; the **ratios** transfer. |
| **Measured (device)** | A number taken on hardware, cited to the document that took it. |
| **Read** | Established by reading the code. No number; the claim is about structure, not cost. |
| **Estimated** | Arithmetic from a measured or read quantity. The arithmetic is shown. |
| **Device-gated** | Cannot be settled without the player in hand. The test that would settle it is given. |

Nothing in this audit was changed in the tree. It is a reading and measuring pass only; the working
tree is identical to `7d55c97` apart from this file and its index entry.

---

## Summary — findings, worst first

Ranked by **impact × (1/effort)**. "Effort" is engineering effort, not risk; the risk column is
separate because on this device they are not the same thing.

| # | Finding | Class | Impact | Effort | Risk |
|---|---|---|---|---|---|
| **B1** | The whole Rust half of the player is compiled `opt-level = "z"`. Switching to `2` is **2.2–3.2× faster across every rendering and sorting benchmark** for +4.6% archive size. | perf + battery | **Very high** | **One line** | Low |
| **B2** | Stage-1 early suspend — the largest measured battery lever this project has ever found (`dpidle_cnt` 0 → 18,509, 37.8 deep-idle entries/sec) — **ships disabled**, behind a hand-created file with no UI and no installer default. | battery | **Very high** | Low–medium | Medium (needs the promotion criteria in B2) |
| **B3** | The N+1 query defect that froze the device on 2026-08-18 was fixed at one call site and **survives at four others**. Measured 15× (by id) and **145×** (by filename) versus the bulk query it replaces. | perf | **High** | Medium | Low |
| **B4** | Every hardware volume write forks `/bin/sh` + `amixer`. The fork-free ioctl replacement **is already written, already shipped and already declared in a header `main.cpp` includes** — it is simply not wired up. | perf + battery | High | **Very low** | Low |
| **B5** | Per-frame heap churn in the list renderers: `song_order` (27.7 KB/frame), `up_next::layout` (~29 KB/frame), and 1–3 allocations per visible row per frame. `PERF_PLAN_2026-08-20` P4 and P5 are still open. | perf + stability | High | Medium | Low–medium |
| **B6** | Sony's HAL pins `scaling_min_freq` to **1.04 GHz for the whole of playback**. Cinder has the machinery to lower it and has only ever been used to raise it. Nobody has tried the downward direction. | battery | High (unquantified) | Low | **Medium–high** (audio underrun) |
| **B7** | The audio pump wakes 4×/s with the panel dark, to feed consumers that run at 1 Hz. | battery | Medium | Very low | Low |
| **B8** | `run_guarded` costs ~4 syscalls per call and is on the unconditional per-frame path for a tick that is a no-op in the common case. | battery | Low–medium | Very low | Low |
| **B9** | The gradient cache **clears itself entirely** when full instead of evicting. | perf | Low | Very low | Low |
| **B10** | Dead raster instrumentation runs on every painted frame for the life of the process; it prints once at frame 300 and then costs two atomics and two clock reads forever. | perf | Low | Very low | None |
| **B11** | The art-cache builder forces a full-screen repaint after every decoded cover, whether or not that row is on screen. | battery | Low | Very low | Low |
| **B12** | The frame loop runs at a flat 60 Hz whenever the panel is lit, including when nothing is animating and the renderer is clean. | battery | Low (bounded by the 30 s screen-off default) | Medium | Low |

**If only one thing is done: B1.** It is a one-line change, it is measured, and it makes B5's two
open items roughly a third as valuable — so it also changes what is worth doing next.

---

## Part A — What was measured today, and how

### A1. The host test suite is green

```
(cd player && cargo test --release)
→ 17 (cinder-db) + 85 (cinder-ffi) + 334 (cinder-ui) + 2 + 8 = 446 passed, 0 failed, 3 ignored
```

**Measured (host).** Note for `CLAUDE.md`: its "host tests green: 39 UI + 8 DB" line is badly out of
date — it is 334 UI, 17 DB and 85 FFI now. That is a documentation defect, not a code one, but it is
the kind that makes a later session mistrust the rest of the paragraph.

### A2. The render benchmark, at the shipping profile

```
cargo test -p cinder-ui --release --test render_bench -- --ignored --nocapture
```

Run twice to establish noise. Run-to-run variance was **4–16%**, so only ratios above ~1.3× are
reported as real below.

| bench | run 1 | run 2 |
|---|---:|---:|
| `song_order sort=TITLE` | 47.7 | 44.4 |
| `library songs (gradients)` | 463.1 | 390.2 |
| `up_next::render_view` | 521.8 | 486.9 |
| `cover, viz OFF` | 9060.3 | 7998.3 |
| `art::block gradient 480x480` | 8777.8 | 8447.3 |
| `just canvas fill` | 138.7 | 126.1 |

### A3. The N+1 database benchmark

A temporary `#[ignore]`d test was added to `cinder-db`, run, and **reverted** (the tree is clean).
It builds an in-memory fixture with the real RE'd schema at two library sizes and times the bulk
query against the per-row queries the FFI actually issues.

| N | `tracks()` — one query | N × `track_by_object_id` | ratio | N × `track_by_filename` | ratio |
|---:|---:|---:|---:|---:|---:|
| 512 | 1.04 ms | 16.68 ms | **16×** | 39.58 ms | **38×** |
| 3,463 | 7.50 ms | 115.13 ms | **15×** | **1,090.92 ms** | **145×** |

**Measured (host), in memory, with no eMMC in the path at all.** The device pays the same
multipliers on a slower core with real page reads behind them, so these are floors.

`track_by_filename` scales quadratically because it is an equality on a bare basename against a
column the RE'd schema does not index — each call is a full scan of `object_body` joined against
`artists`, `albums`, `albumartists` and `object_ext_int`.

### A4. The compiler-profile A/B

Same benchmark, same machine, three profiles, `player/Cargo.toml` restored afterwards.

| bench (µs/frame) | `"z"` (shipping) | `2` | `3` | best speedup vs `"z"` |
|---|---:|---:|---:|---:|
| `just canvas fill` | 126.1 | **38.0** | 40.0 | **3.32×** |
| `song_order sort=TITLE` | 44.4 | 18.8 | **16.7** | **2.66×** |
| `az_present songs` | 78.9 | **21.2** | 24.7 | **3.72×** |
| `up_next::layout (shuffle-all)` | 29.6 | — | **6.7** | **4.42×** |
| `library playlists (gradients)` | 210.9 | — | **87.5** | **2.41×** |
| `library songs (gradients)` | 390.2 | 257.9 | **239.7** | **1.63×** |
| `up_next::render_view` | 486.9 | 213.8 | **214.6** | **2.28×** |
| `art::block gradient 480x480` | 8447.3 | **3384.4** | 3357.6 | **2.52×** |
| `cover, viz OFF` | 7998.3 | **3503.4** | 3573.1 | **2.28×** |

And the size those speedups cost, on `libcinder_ffi.a` (the staticlib linked into `cinder-home`):

| profile | `libcinder_ffi.a` | delta vs `"z"` |
|---|---:|---:|
| `"z"` | 14,210,648 B | — |
| **`2`** | **14,859,928 B** | **+649,280 (+4.6%)** |
| `3` | 15,631,240 B | +1,420,592 (+10.0%) |

**`opt-level = 2` captures the entire win at less than half the size cost of `3`.** On several
benches `3` is marginally *slower* than `2`, inside noise. This is the recommendation.

---

## Part B — The findings

### B1. The player is compiled for size, on a device whose constraint is battery

**`player/Cargo.toml:6` — `opt-level = "z"`.** No comment, no rationale anywhere in the repository
(`grep -rn 'opt-level' --include='*.md'` returns nothing). In a tree that comments the reasoning
behind a 150 ms coalescing window and a 512-entry lookup table, an uncommented performance-relevant
build setting is almost certainly an unexamined template default rather than a decision.

**Why it matters more here than it usually would.** `opt-level = "z"` disables the inlining and
autovectorisation that a software rasteriser lives on. Every hot loop in this project is exactly the
shape `z` penalises hardest: `grad_row` (`player/cinder-ui/src/art.rs:273`) is a per-pixel float
loop with a table lookup and a conditional `sqrt`; `Canvas` blits are per-pixel; `song_order`
(`player/cinder-ui/src/library.rs:761`) is a comparison-heavy sort. The benchmark in §A4 is that
prediction confirmed: the closer a bench is to a raw pixel loop, the larger the win
(`just canvas fill` 3.32×).

**The battery argument, not just the speed argument.** A frame that costs 2.3× less CPU is not
merely smoother — it is a core that returns to 598 MHz sooner and stays there longer. Per
`analysis/RE_kernel_idle_levers.md` §4 the governor already spends 77% of its time at the 598 MHz
floor; shortening every render burst is a direct reduction in the 20%/4% spent at 1040/1300 MHz,
and the voltage difference makes that saving superlinear in the frequency.

**The cost is 649 KB of archive on an 800 MB rootfs.** The linked, stripped `cinder-home` is ~2.9 MB
(`cinder-home/build.sh` header); the Rust portion of that will grow by well under the archive delta
after LTO and `strip`, both of which are already on.

**Remediation.** `player/Cargo.toml:6`, `opt-level = "z"` → `opt-level = 2`. Then re-run §A4's
benchmark and put the numbers in the commit, per this project's own convention.

Leave `installer/Cargo.toml:18` at `"z"`. The installer is a one-shot host binary where size is the
only axis that matters; that one *is* correctly set, which is mild evidence that the player's
setting was copied rather than chosen.

**One thing to check on device before believing the whole win.** These are host numbers on x86-64.
The ratios should transfer — the mechanism (inlining and vectorisation of pixel loops) is not
architecture-specific — but ARMv7 NEON autovectorisation is weaker than AVX2, so the device ratio
will likely be smaller than 2.3×. `cinder-probe --bench` before and after is the check, and it is
the same check `docs/PERF_PLAN_2026-08-20.md` already asks for.

---

### B2. The biggest battery lever in the project ships turned off

`analysis/RE_early_suspend.md` records the outcome of the 2026-09-04 work, and it is the strongest
battery result this project has produced:

> `dpidle_cnt[0]` **0 → 18,509** … rate **37.8 deep-idle entries/sec** … `by_vtg` delta **0** across
> the whole window … `dpidle_block_mask[CG_PERI0]` → `0x00000000` off-cable … `resume_count` **0**
> for the entire run … USB back immediately, no reboot.

**Measured (device), 2026-09-04**, log at `analysis/kernel/pm_offcable_stage1_2026-09-04.log`. Before
this, the SoC had entered deep idle **zero times in every boot ever sampled**.

And `cinder-home/src/main.cpp:2977`:

```c
const int thr = threshold_s();
if (!thr) return;                       // disabled — the default
```

`threshold_s()` reads `/contents/cinder_suspend_s`. Absent, empty, `0` or unparseable ⇒ disabled.
There is **no Settings row, no installer default, and no mention in `README.md` or `install.md`** —
`grep -rn 'cinder_suspend_s'` finds it in `CHANGELOG.md`, `main.cpp`, and the RE document, and
nowhere a user would look. So every installed device today gets none of this.

**This was a correct decision when it was made, and the comment says why**
(`cinder-home/src/main.cpp:2718`): *"OFF BY DEFAULT … because this is still new: it is trivially
inspectable and trivially removable from a PC, which a Rust-side setting is not."* That reasoning is
about a mechanism whose failure mode had, at the time, cost two forced reboots. It is not a
permanent verdict, and the conditions it was waiting on have partly been met since: the exit path is
now written (`write_node(kStateNode, "on")` on every resume), the boot grace is in place
(`kBootGraceS = 180`, `main.cpp:2893`), the escape hatch is checked every tick, and the off-cable run
completed with `resume_count` at 0 and no reboot.

**What is still genuinely unmet**, and belongs in the promotion criteria rather than being waved
past:

1. **Nothing has measured the actual power saving.** `dpidle_cnt` proves the SoC reaches deep idle;
   it does not say what that is worth. `docs/BATTERY_BT.md` specifies the method
   (`tools/btpower.sh`, cable out, three matched windows) and `analysis/RE_early_suspend.md` notes
   there is no fuel gauge, so this is a multi-hour voltage-decay A/B, not a spot check.
2. **Stage 1 has been verified on one device, in runs of minutes.** A default needs an overnight.
3. **The USB gadget bug is stage 2's, not stage 1's** — but the two share a config surface, and a
   user who finds `cinder_suspend_s` and then finds `cinder_ram_suspend` next to it in the same
   documentation is one file away from a state that costs a reboot.

**Remediation, in the order that keeps the safety argument intact.**

1. **Measure it.** One overnight idle A/B with and without `/contents/cinder_suspend_s`, cable out,
   per `docs/BATTERY_BT.md`'s method. Until there is a number, "the biggest lever" is a claim about
   `dpidle_cnt`, not about battery.
2. **Then make it discoverable before making it default.** A Settings ▸ Device row —
   *Deep idle: OFF / 60 s / 5 min* — writing the same file the C code already reads. The file stays
   the source of truth (so the "removable from a PC" escape survives intact); the UI just stops the
   feature being invisible. This is the cheap half and it can ship before the overnight completes.
3. **Only then consider a default**, and if so `60 s` rather than something aggressive, with the
   existing `kBootGraceS` window unchanged.
4. **Keep stage 2 exactly where it is.** `analysis/RE_early_suspend.md` already concludes stage 2 is
   *"much less interesting than it looked"* now stage 1 works. Nothing here disagrees.

**Related, and already done — do not re-derive it.** The framebuffer early-suspend handshake
(`namespace fbsync`, `cinder-home/src/main.cpp:2756`) cut suspend entry latency from 1.31 s to
0.30 s, device-verified. It is one of the cleanest pieces of RE in this tree and it only pays off
when B2 is switched on.

---

### B3. The 2026-08-18 freeze was fixed at one call site out of five

`player/cinder-ffi/src/lib.rs:2005` carries the write-up of a defect reported as *"toggling shuffle
can crash the device when there is a lot queued"*, root-caused to `play_order_uris` issuing one full
join query per row while holding the renderer mutex, and fixed by doing one query for the whole map.

The same shape survives at four other sites:

| Site | Call | Input size | Runs when |
|---|---|---|---|
| `lib.rs:4474` | `db.track_by_object_id` per id | the saved context — **the whole library** after "Shuffle all songs" | **every boot** (resume) |
| `lib.rs:2314` | `db.track_by_object_id` per id | `r.app.context()` — **the whole library** after shuffle-all | on a tap (`PlayContextAt`) |
| `lib.rs:2279` | `db.track_by_object_id` per id | the user queue | on a tap (`PlayQueueAt`) |
| `lib.rs:2166` | **`db.track_by_filename`** per entry | every entry of **every** playlist | whenever the playlist model is rebuilt |
| `lib.rs:2230` | **`db.track_by_filename`** per entry | one playlist's entries | on playing a user playlist |

Against §A3's numbers, at a 3,463-track library:

* `lib.rs:4474` and `lib.rs:2314` — **115 ms of pure SQL on the host** where 7.5 ms would do, and
  `2314` pays it **on the render thread, holding `cell().lock()`**, which is the precise
  configuration that produced the reported freeze.
* `lib.rs:2166` and `lib.rs:2230` use the **145×** variant. Five playlists of 200 tracks is 1,000
  full scans of `object_body` — on the host, in memory, ~315 ms; on the device, with eMMC pages and
  a 598 MHz core, materially worse.

`lib.rs:2166` is the one to fix first: it is the largest multiplier, it is on a screen the user
opens casually, and unlike the others it has no size bound at all.

**Remediation.** Add two batch resolvers to `cinder-db` beside the existing `query_tracks`, and use
them at all five sites:

```rust
pub fn tracks_by_object_ids(&self, ids: &[i64]) -> Result<HashMap<i64, Track>>
pub fn tracks_by_filenames(&self, names: &[&str]) -> Result<HashMap<String, Track>>
```

Both are one `query_tracks` with an `IN (…)` (chunked to SQLite's `SQLITE_MAX_VARIABLE_NUMBER`), or
— simpler and bounded — one unfiltered `tracks()` indexed into a map, which is exactly the shape
`play_order_uris` already uses and which §A3 measures at 7.5 ms for the whole library.

`track_by_filename`'s callers additionally want the basename-then-full-path disambiguation it does
today; that logic moves into the resolver unchanged and runs against the in-memory map, where it
costs nothing.

**A cheap independent mitigation, worth doing regardless:** `Db::open`
(`player/cinder-db/src/lib.rs:110`) sets **no PRAGMAs at all**. `PRAGMA temp_store = MEMORY` keeps
the `ORDER BY` scratch for a 3,463-row sort off the eMMC, and costs one statement at open. *(Read.
`PRAGMA mmap_size` would likely help more and is the obvious next thought — but the library DB is
written by Sony's scanner while we hold it open read-only, and mmap I/O against a
concurrently-written database is exactly the configuration to measure rather than assume. Not
recommended without a device test.)*

---

### B4. The fork-free volume write is already built and simply not called

`cinder-home/src/main.cpp:1959`, in `volume_write_now`:

```c
std::snprintf(cmd, sizeof cmd, "amixer -c %d cset name='%s' %d >/dev/null 2>&1", …);
std::system(cmd);
```

Its own comment (`main.cpp:1941`) states the cost: *"a fork+exec of /bin/sh AND of amixer per step —
on a single-core ARMv7 that is tens of milliseconds, eight times a second, competing with the render
thread for the only core."* That is why `VOL_WRITE_EVERY_MS = 150` (`main.cpp:1946`) exists — the
coalescing window is a workaround for the cost of the mechanism.

**The mechanism does not need to cost that.** `cinder-audio/src/codec_shim.cpp` already drives ALSA
controls with a bare ioctl on `/dev/snd/controlC0`, addressed **by name** so numid renumbering cannot
land on the wrong control. And it already includes the exact control this path wants:

```c
cinder-audio/src/codec_shim.cpp:34   const char kMasterVolCtl[] = "master volume";
cinder-audio/src/codec_shim.cpp:119  int cinder_codec_get_master_volume(void) { return get_int(kMasterVolCtl); }
cinder-audio/src/codec_shim.cpp:120  int cinder_codec_set_master_volume(int v) { return set_int(kMasterVolCtl, v); }
```

Both are declared in `cinder-audio/include/cinder_codec.h:110-111`, and
**`cinder-home/src/main.cpp:62` already `#include`s that header** — it calls
`cinder_codec_set_standby` from the same file at `main.cpp:10005`. So the replacement for two
process spawns is one function call that is already compiled into the binary.

**`grep` confirms nothing calls either function.** They were written for this and left unwired.

The same applies to `sync_volume_from_hw` (`main.cpp:1871`), which shells out to `amixer cget` —
and which runs at boot *and on every screen wake*, via `bt_resync_volume("screen wake")`. That is a
fork of `/bin/sh` and a dynamic load of `amixer` every time the user presses Power.

**Remediation.**

1. Generalise the shim to carry the card index, so the config surface survives:
   `int cinder_codec_set_ctl_int(int card, const char* name, int val)` — `ctl_ioctl`'s device path
   becomes `/dev/snd/controlC%d`. Ten lines.
2. In `volume_write_now`, take the ioctl when `g_vol.amixer` is set, and keep the `system()` call as
   the fallback for a control the ioctl cannot resolve. Same for `sync_volume_from_hw`.
3. **Do not shorten `VOL_WRITE_EVERY_MS` in the same change.** The coalescing is also what keeps the
   mixer from being written 8×/s, which is a separate good; and `frame_budget.h`'s whole deadline
   contract is written against that constant. Change the mechanism first, measure, then decide.

**Why this is a battery item and not only a latency one:** a fork on this device dirties pages,
runs the dynamic linker twice, and pulls the governor off its 598 MHz floor — for a volume step.

---

### B5. Per-frame heap churn in the list renderers

This is the hazard class the codebase has already been burned by twice — `art.rs:336` and
`lib.rs:1529` both carry notes about an on-device allocator abort caused by render-path churn
(*"memory allocation of 1536000 bytes failed" → SIGABRT → reboot*). Three sites still feed it.

**B5a. `song_order` is recomputed and reallocated every frame** —
`player/cinder-ui/src/library.rs:1094`, inside `render()`:

```rust
let order = song_order(lib, sort); // shared with hit_row/selection — keep in sync
```

`song_order` (`library.rs:761`) filters and sorts a `Vec<usize>` over the whole library. At 3,463
tracks that is **27.7 KB allocated and a full sort, per painted frame** — 60×/s on the Songs tab, so
~1.66 MB/s of allocator traffic. Worse, `song_at` (`library.rs:791`) calls `song_order` and then
reads **one element**, so every hit test on the Songs tab pays the same allocation and sort.

*Measured (host):* 44.4 µs (TITLE), 74.9 µs (ARTIST A-Z) at the shipping profile; 18.8 µs at
`opt-level = 2`.

**There is a documented disagreement here and it should be settled.** `docs/AUDIT_2026-08-16.md:483`
concluded *"P6/`song_order` and `albums_build` were **not** worth memoising — 22 µs and 6 µs"*.
`docs/PERF_PLAN_2026-08-20.md` put it back on the list four days later as **P5**, at 56.6 µs. This
audit sides with PERF_PLAN, for a reason neither document leads with: **the allocation is the
stronger argument than the microseconds.** 27.7 KB of churn per frame on a heap that also holds the
Mali/EGL surfaces, the library and the decoded covers is the documented failure mode, and it does not
show up in a µs/frame column at all.

**B5b. `up_next::render_view` materialises the whole layout to draw fourteen rows** —
`player/cinder-ui/src/up_next.rs:357`:

```rust
let l = layout(v.tracks.len(), v.current, v.queue.len());
```

`layout` (`up_next.rs:220`) allocates one slot per track. After "Shuffle all songs" that is the whole
library — its own comment says *"~29 KB, once per painted frame … this is most of what an Up Next
frame costs beyond the ~14 rows it actually draws."* `metrics()` (`up_next.rs:182`) is the O(1)
arithmetic twin, is already tested against `layout()` by `metrics_matches_layout`, and is already
used for the auto-follow. This is `PERF_PLAN_2026-08-20` **P4**, unlanded.

*Measured (host):* `up_next::layout (shuffle-all)` 29.6 µs at `"z"`, 6.7 µs at `opt-level = 3`.

**B5c. One to three heap allocations per visible row, per frame.**
`player/cinder-ui/src/library.rs:1243`, in the Artists arm of `render()`:

```rust
let arts: Vec<&str> = ar.arts.iter().map(|s| s.as_str()).collect();
art_stack(c, t, lib, 22, cy, &arts, &ar.album_ids);
```

A heap `Vec` per row per frame — and `art_stack` (`library.rs:909`) reads only `arts[0]` and
`arts[1]`. It never needs a `Vec` at all. Alongside it, `library.rs:1251` builds the subtitle with
`format!`, and `plural()` (`library.rs:905`) does another `format!` inside that. Three allocations ×
~14 visible rows × 60 fps ≈ **2,520 allocations/second** on the Artists tab; the Songs, Albums and
Playlists arms each do one or two of the same (`library.rs:1183`, `1185`, `1208`, `1287`, `1289`).

**Remediation, in ascending effort.**

1. **B5c, today.** Change `art_stack` to take `&[String]` (or `&ar.arts`) — the `Vec` disappears with
   no other change. Then move the row subtitles onto the model rows, built once in `set_library`:
   they are a pure function of data that only changes when the library does.
2. **B5a**, as PERF_PLAN P5 specifies: cache `Vec<usize>` on `App`, keyed on
   `(sort, filter genre, hi-res filter, library generation)`, with the generation counter bumped in
   exactly `set_library` and `set_playlists` and nowhere else. The whole safety argument is that
   counter; a stale memo shows the wrong track for a tapped row.
3. **B5b**, as PERF_PLAN P4 specifies: give `Metrics` a `slot_at(content_y)` and walk the visible
   window. Keep `layout()` for the reorder drag, which genuinely wants the whole map, and keep
   `metrics_matches_layout` as the contract between them. PERF_PLAN rates this medium risk and is
   right to — a renderer and a hit test drifting apart is this screen's recurring bug.

**Sequencing note.** Do **B1 first**. At `opt-level = 2` the *time* half of B5a and B5b shrinks by
~2.4×, which may take them below the bar. The *allocation* half does not shrink at all, so B5c and
the P5 memo remain worth doing on churn grounds alone — but the case for P4 should be re-made
against fresh numbers rather than against the ones in PERF_PLAN.

---

### B6. Playback holds the CPU at 1.04 GHz, and nobody has tried lowering it

`cinder-home/deploy/cinder-signature.sh:10` documents what Walkman One's paid "sound signature"
actually is — two things, one of which is *"the CPU clock floor held during playback
(`scaling_min_freq`: 1040000 vs 1300000)"*. So **Sony's own HAL pins the core at 1.04 GHz for the
duration of every playback session**, and the paid mod raises it to 1.3 GHz.

Cross-referenced against `analysis/RE_kernel_idle_levers.md` §4 (device-measured):
`time_in_state` = **77% at 598 MHz / 20% at 1040 / 4% at 1300**. The 20% is, on a device that is
mostly a music player, largely playback.

Cinder already has the lever and it is already safe-by-default:
`apply_cpu_floor` (`cinder-home/src/main.cpp:1416`) writes `scaling_min_freq` — the node is `0666` on
this device — holding a configured floor while playing and restoring the governor's value when
stopped. It reads `khz=` from `/contents/cinder_cpufloor.conf`, defaults off, and its comment
correctly warns that *"holding the CPU off its lowest step costs battery"*.

**Every use of this mechanism, in this project and in Walkman One, has been to raise the floor.
Nobody has tried lowering it.** `scaling_available_frequencies` on this SoC offers 598000 / 747500 /
1040000 / 1196000 / 1300000 — so there are two steps below what the HAL imposes, and at 598 MHz the
core voltage is lower too, which makes the saving superlinear rather than proportional.

**Two things make this harder than setting the config file, and both are findable in the code.**

1. `apply_cpu_floor` restores `g_cpufloor_orig` — the value the node held *before* Cinder touched
   it. If that read happens before the HAL raises the floor, `khz=598000` is a no-op against a node
   Sony then overwrites.
2. It is only called from `set_transport` (`main.cpp:1434`), i.e. on transport transitions. The HAL
   writes the node when the stream opens. **Whoever writes last wins, and the order is racy.**
   Pinning the floor low would need a re-assert from the 1 Hz housekeeping block — which the code
   deliberately does not do today.

**The risk is real and is the reason this is a proposal, not a recommendation.** Sony presumably
chose 1.04 GHz with decode margin. A floor too low means underruns on high-bitrate FLAC or DSD, and
the symptom is audible stutter rather than a log line.

**Remediation — as an experiment with a stated stopping rule, not a default.**

1. Re-assert the configured floor from the 1 Hz housekeeping while `g_playing`, behind the existing
   `cinder_cpufloor.conf`, so the setting survives the HAL. Read the node back (the code already
   does — cpufreq silently clamps to a value the table has).
2. Test the ladder downward — 1040000 → 747500 → 598000 — on the **worst** content in the library
   (highest-bitrate FLAC, and DSD if present), screen off, over a full album each.
3. Stop at the first step that produces any audible artefact, and back off one.
4. Only then measure: `tools/btpower.sh`, matched windows, per `docs/BATTERY_BT.md`.

If 747500 holds, that is a ~28% frequency reduction across the state this device spends its life in,
with a voltage reduction on top. That is a bigger prize than everything in Part B below this line
combined — and it is entirely unexplored.

---

### B7. The dark audio pump wakes four times a second for consumers that run at one

`cinder-home/src/main.cpp:9246`:

```c
cinder_audio_pump_set_interval(g_screen_on ? 20 : (just_pressed ? 100 : 250));
```

The 250 ms figure is already the result of a good piece of work — it was 100 ms, and
`docs/BATTERY_BT.md` records the pump thread as *"the joint-largest source of cinder-home's standby
wakeups (10.2 ctxt/s of 20.9 total)"*.

The comment above it also contains the argument for going further: *"What does ride on it is the
position callback (~1/s) and the track boundary, both consumed by 1 Hz housekeeping — so a 250 ms
delivery window is invisible to every consumer."* **If the consumers run at 1 Hz, 250 ms is four
times finer than anything can observe.** The responsiveness case is already covered separately by
the `just_pressed` grace window, and the panel is dark, so nothing displays position.

*Estimated:* 250 → 500 ms halves the remaining pump wakeups in the pocket-playing state, from 4/s to
2/s. Combined with the render loop's ~1/s, that takes `cinder-home`'s dark-playing wakeup budget from
roughly 5/s to roughly 3/s.

**Remediation.** 250 → 500 ms, one constant. Verify `pause-on-disconnect` latency is unchanged (it
rides `refresh_bt_route`, not the pump, so it should be — but that is the one behaviour worth
checking, because it is the one a user notices). Do not go to 1000 ms in the same step: the position
callback and the housekeeping tick would then be at the same period and could alias.

---

### B8. `run_guarded` costs four syscalls to run a no-op

`run_guarded_ex` (`cinder-home/src/main.cpp:370`) does, on every call, before `fn()` is reached:

* `alarm(0)` to capture the outer watchdog — a syscall;
* a linear scan of a 64-entry `seen[]` array for the first-call log;
* `sigsetjmp(g_guard_jb, 1)` — **savemask is 1, so this is an `rt_sigprocmask` syscall**;
* `alarm(timeout)` — a syscall;

and after it, `alarm(0)` plus a conditional `alarm(prev)`. So **~4–5 syscalls per call**, minimum.

`cinder-home/src/main.cpp:9518` puts one of these on the unconditional per-frame path:

```c
run_guarded("loop: BT volume walk", 6, bt_vol_walk_tick);
```

Its comment says the tick *"costs one integer test a frame while idle, which is every frame but the
second or two after a connect."* That is true of the tick. It is not true of the guard around it.

*Estimated:* at 60 Hz with the panel lit, ~240 syscalls/second to reach an integer test that returns
immediately. Two other per-frame guards (`loop: BT sound status`, `loop: USB-DAC status`) are
conditional on being in that mode, so they are correct as written.

**Remediation.** Hoist the cheap predicate out of the guard — `if (bt_vol_walk_pending()) run_guarded(…)`
— so the guard is paid only when there is a walk in flight. That is one line and it preserves the
guard exactly where it matters. This is small; it is listed because it is nearly free and because the
`run_guarded` cost is not obvious from any call site.

---

### B9. The gradient cache empties itself instead of evicting

`player/cinder-ui/src/art.rs:348`:

```rust
if cache.len() >= GRAD_CACHE_MAX && !cache.contains_key(&key) {
    cache.clear();
}
```

`GRAD_CACHE_MAX` is 64 and ~14 rows are visible. Scrolling steadily past 64 distinct album names
wipes the cache entirely, including the entries for the rows currently on screen, and the next frame
re-bakes all fourteen.

*Estimated:* a 48×48 bake is 2,304 px; against the measured `art::block gradient 480x480` figure
(8,447 µs for 230,400 px ≈ 0.037 µs/px) that is ~85 µs, so ~1.1 ms on the frame after each wipe —
about 3.5% of a ~31 ms scrolling frame. Small, and it recurs every 64 rows of scrolling.

**Remediation.** Retain the visible working set: evict half (drain the first 32 entries) rather than
`clear()`, or key eviction on insertion order. The cap is the contract and it stays; the fix is only
about *which* entries survive. `grad_cache_len`/`grad_cache_max` already exist for the test.

---

### B10. Dead instrumentation on the shipping render path

`player/cinder-ffi/src/lib.rs:1578-1588`, inside `cinder_render_tick`, after every painted frame:

```rust
let n = RASTER_N.fetch_add(1, Relaxed) + 1;
let us = RASTER_US.fetch_add(raster_t0.elapsed().as_micros() as u64, Relaxed)
    + raster_t0.elapsed().as_micros() as u64;
if n == 300 { println!("cinder-ffi: raster — …"); }
```

It prints exactly once, at frame 300, and then costs two atomic RMWs and **two** `clock_gettime`
calls on every painted frame for the life of the process. (`raster_t0.elapsed()` is evaluated twice;
the second call is redundant and also returns a slightly different value from the one accumulated.)
The comment above it says it is a *"ONE-SHOT RASTER COST SAMPLE"* — the intent is clearly one shot;
the code is not.

**Remediation.** Return early once `n > 300`, or drop the block and take the number from
`cinder-probe --bench`, which measures the same thing deliberately.

---

### B11. Every decoded cover forces a full repaint

`player/cinder-ffi/src/lib.rs:4676`, in the art-cache builder thread:

```rust
r.app.library_mut().thumbs.insert(album_id, t48);
r.dirty = true; // the row this belongs to may be on screen right now
```

*May be* — but the flag is set unconditionally. On a first boot after install that is ~340 forced
full-screen rasters and blits, spread over the ~2.7 minutes the builder takes (365 ms decode +
120 ms yield per album, per `art_cache.rs`'s header). Whenever the user is on Settings, or on
Now Playing, or anywhere that album's row is not visible, the repaint draws a byte-identical screen.

It costs nothing while the panel is dark — the paint is skipped there — so this only bites during
the exact window a new user is most likely to be poking at the device.

**Remediation.** Gate on the library list being the current screen; better, on the album being in
the visible window, which the renderer already knows. The builder already holds the lock at this
point, so the test is free.

---

### B12. The loop runs at 60 Hz whenever the panel is lit, animating or not

`cinder-home/src/main.cpp:10438`:

```c
if (g_screen_on) {
    left = 16 - (now_ms() - frame_start);
} else { … }
```

The dark path is exemplary — `poll()` on the input nodes, budgeted to the next housekeeping
deadline, clamped by `cinder_vol_deadline`, and measured on device at ~1 context switch/second
against 10.2 before. **The same argument has not been applied to "lit but static."** `poll()` returns
immediately on any input at any budget, so a lit-and-idle budget of 100 ms would cost nothing in
touch latency: the finger that starts a drag still lands in the very next iteration, and the loop
snaps back to 16 ms from there.

*Estimated:* each iteration does a non-blocking `read()` on 8 input nodes plus the `poll()` plus two
`alarm()` pairs plus B8's guard — roughly **1,000 syscalls/second** while lit and idle, against
~130/s at 10 Hz.

**Two things bound how much this is worth, and they should be stated plainly.**

1. **The screen-off default is 30 s** (`player/cinder-ui/src/nav.rs:1190`,
   `SCREEN_OFF_PRESETS[2]`), so lit-and-idle is bounded to 30 s after each interaction rather than
   being a long-lived state. *Estimated:* 100 interactions a day ≈ 50 minutes/day of it.
2. Genuine animation must not be throttled — the visualiser (already capped at 20 fps internally),
   the marquee, a fling in flight, and the volume HUD fade.

That second point is also why this is medium effort rather than low: it needs the renderer to
publish what it wants, not the shell to guess. `cinder_render_tick` already computes exactly that —
`r.dirty`, `animate`, `marquee_scrolled()`, the fling state — and throws it away.

**Remediation.** Export a `cinder_next_frame_hint_ms()` from `cinder-ffi` returning the delay until
the renderer next needs a frame (0 = dirty now, ~50 for the visualiser, ~1000 for a static screen).
Fold it into the lit budget with the existing `cinder_clamp_budget`, exactly as the volume deadline
is folded into the dark one. The pattern, the helper and its host self-test
(`tools/framebudget_selftest.cpp`) all already exist; this is a second caller for them.

---

## Part C — What is already right, and must not be undone

This tree is, on the whole, unusually well optimised, and several of the obvious "improvements" a
reader might reach for have already been tried and are load-bearing. Recorded so a later pass does
not undo them:

* **The dark frame budget** (`frame_budget.h`, `main.cpp:10438`) — 60 Hz → ~1 Hz with `poll()` on the
  input nodes, *and* the volume-deadline clamp that keeps the rocker responsive through it. Measured
  on device: 10.2 → ~1 context switch/second, with better touch response, not worse.
* **The event-driven Bluetooth polling** (`bt_poll.h`, `docs/BATTERY_BT.md`) — ~2.2 IPC/s → ~0.07
  IPC/s in a steady session, gated on `g_bt_listener_on` being the *real* registration result so a
  failed `AddListener` restores every old interval.
* **The codec standby** (`main.cpp:10005`) — device-verified in all three directions on 2026-09-04,
  including the negative control (jack playing, screen off ⇒ codec stays awake). The headphone
  amplifier no longer drives an empty jack through a Bluetooth session.
* **The framebuffer early-suspend handshake** (`namespace fbsync`) — 1.31 s → 0.30 s suspend entry,
  device-verified, protocol read out of the kernel image rather than assumed.
* **The present thread** (`player/cinder-ffi/src/present.rs`) — max(raster, present) instead of the
  sum, with a depth-1 blocking handoff that is deliberately *not* a dropping queue because the
  watchdog contract depends on backpressure.
* **The transport-press queue** (`STATUS.md`, 2026-08-31) — transport actions queued and applied one
  per frame instead of being carried out inside the evdev drain loop. 40 taps in 4 s: 13.1 s → 3.7 s,
  housekeeping 1/15 → 13/15.
* **The reused canvas and the pre-baked gradient** — both exist because allocation churn on this
  device has caused a real `SIGABRT`. Anything in B5 must be implemented in a way that reduces
  churn, never one that trades churn for speed.
* **`art::block_cached`, `az_present_memo`, `Layout::at`'s `partition_point`** — P2, P6-half and P3
  of `PERF_PLAN_2026-08-20`, all landed and verified.
* **The retry-log pacing** (`retry_log`, `main.cpp:710`) — a line per second to `/contents` is 86,400
  flash writes a day; the tripling backoff is why that class of defect is closed.

---

## Part D — Recommended order of work

Ordered so that each step's measurement informs the next.

**Stage 1 — one-line changes, offline, measured before and after.**

1. **B1** — `opt-level = 2`. Re-run `render_bench`; put the numbers in the commit.
2. **B10** — return early past the raster counters.
3. **B8** — hoist the predicate out of `run_guarded` on the volume-walk line.
4. **B5c (first half)** — `art_stack` takes `&[String]`; the per-row `Vec` disappears.

**Stage 2 — small, local, still offline.**

5. **B4** — wire `cinder_codec_set_master_volume` / `_get_` into the volume backend, `system()`
   retained as fallback.
6. **B9** — evict half instead of `clear()`.
7. **B11** — gate the builder's `dirty` on visibility.
8. **B7** — dark pump 250 → 500 ms.

**Stage 3 — the structural items, re-justified against Stage 1's numbers.**

9. **B3** — the two batch resolvers, `lib.rs:2166` first. Add `PRAGMA temp_store = MEMORY`.
10. **B5a** — the `song_order` memo (PERF_PLAN P5), on churn grounds.
11. **B5b** — `metrics::slot_at` (PERF_PLAN P4), only if Stage 1 leaves it worth doing.
12. **B12** — the next-frame hint.

**Stage 4 — device, and only device.**

13. **B2 step 1** — the overnight idle A/B for early suspend. This is the single highest-value thing
    on the whole list and it cannot be done offline.
14. **B2 step 2** — the Settings row, once there is a number to put behind it.
15. **B6** — the downward CPU-floor ladder, with the stopping rule in B6.

---

## Part E — What this audit did not settle

* **Every absolute number here is a host number.** The ratios transfer; the milliseconds do not.
  `cinder-probe --bench` on device, before and after each change, is what this project's own
  convention asks for and it has not been run for any of this.
* **B2's actual power saving is unmeasured.** `dpidle_cnt` 0 → 18,509 proves the SoC reaches deep
  idle. It does not say what that is worth in hours of playback, and with no fuel gauge on this
  device only a multi-hour voltage-decay A/B can.
* **B6 is untested in the downward direction, full stop.** No claim is made that 747500 or 598000
  will hold; the claim is that nobody has looked, and that the machinery to look is already present.
* **The always-on codec block remains open.** `docs/BATTERY_BT.md` established that the oscillators,
  the four `BLK_ON0` block-enables, the serial-data path and the DNC engine read identically in
  every state including idle — *"they are an **idle** cost, present the whole time the player is
  awake, and that is where any remaining prize lives."* B2 may or may not touch it; the register
  comparison during a stage-1 window has not been taken. **Never write `/proc/regmon/<chip>/value`**
  while chasing it — that rule stands.
* **The GPU present path is untested.** `analysis/RE_kernel_idle_levers.md` §4 notes the Mali-450
  path exists (`player/cinder-ffi/src/gpu.rs`, opt-in via `/contents/cinder_gpu_on`) and that the
  software framebuffer is what runs today. A ~16.6 ms software blit moved onto the GPU would free
  the core — but it would also power up a block that currently reads `AD_MMPLL_CK=0`, so whether it
  is a net battery win is genuinely unknown and needs the A/B, not an argument.
* **CI would not catch a performance regression from any of this.** `render_bench` is `#[ignore]`d
  by design (a slow runner must not fail a build) and nothing else measures. Publishing the bench
  output as a CI **artifact** — not a gate — would make a regression visible without reintroducing
  the flakiness the `#[ignore]` exists to prevent.

---

## Part D — Device follow-up, 2026-09-05 (same day, after the audit)

The audit above is an offline document: its evidence classes say so, and every number in Part A is
host-measured by design. Four of its findings were taken to the hardware the same afternoon. Two of
its numbers did not survive that, and this section corrects them in place rather than leaving the
document to be quoted as written.

### D1. B1 landed, and it is ~1.1× on device, not 2–3×

*Measured (device).* Both builds flashed from identical source, read through the windowed raster
sampler in `cinder_render_tick` (rewritten for this — see D4).

| | `"z"` | `2` | `2` (2nd build) |
|---|---:|---:|---:|
| raster, `frames 1..300` | 6.93 ms | **6.14 ms** | 6.32 ms |
| library build | 4.77 s | **4.52 s** | 4.56 s |

Only the `frames 1..300` window is comparable — it follows a fixed boot sequence, so the same work
is sampled every time. The `frames 301..600` window is **not** controlled: what is on screen by then
depends on Bluetooth reconnect attempts and art decoding, and it read 4.83, 5.97 and 7.50 ms across
builds in no consistent order. It was briefly quoted as a 1.24× win; that was reading noise.

Two `"2"` builds landing three percent apart put the noise floor near ±3%, which the 10% gap clears
but not by much. **The direction is consistent across every host bench and every device sample; the
magnitude is not.** The host bench overstates this device's gain by roughly 2×. The likely reason is
that the canvas is 480×800×4 = 1.5 MB — comfortably inside the host's L2/L3, nowhere near the A7's
cache — so a good share of every device frame is DRAM bandwidth, which no codegen flag can improve.
*That last sentence is inference, not measurement.*

**Anyone sizing future work off `render_bench` alone should expect well under half of what it
promises.** That is the most useful thing this follow-up found, and it applies to B5 and B9 too.

### D2. The size cost is +16.9%, not +4.6%

*Measured (device toolchain).* §A4 measured `libcinder_ffi.a`. A static archive is not what ships —
it carries per-object metadata the linker discards. The linked, stripped ARM binary is the artefact:

| | `"z"` | `2` | delta |
|---|---:|---:|---:|
| `cinder-home` (dev, stripped ARM) | 3,674,172 B | 4,292,692 B | **+618,520 (+16.8%)** |
| `cinder-home` (stable) | 3,661,868 B | 4,280,388 B | **+618,520 (+16.9%)** |

Still an easy trade — `/system` has 490 MB free — but it is nearly four times the figure in §A4, and
the archive number should not be quoted again.

### D3. B1 does **not** fix the boot dead time

*Measured (host + device).* The ~4.5 s library build moves about 5%. Profiling it with the new
`profile_library_build` stopwatch (`player/cinder-ffi/src/lib.rs`, opt-in via `CINDER_PROFILE_DB`)
against the real 3,349-track DB shows why — the build is roughly **77% SQLite**:

```
  Db::open                14.3 ms        db.tracks(Title)         25.1 ms
  db.release_years         0.4 ms        db.tracks_album_order    18.2 ms
  db.albums                2.3 ms        build_library (whole)    60.0 ms
```

Bundled SQLite is branch-heavy, not loop-heavy, and is not what `"z"` punished (release `"z"` →
`2` on this path: 28.5 → 23.8 ms, 1.2×). Bulk device I/O is not the constraint either: the 5.5 MB
`/db/MTPDB.dat` reads in ~60 ms.

**This is NOT B3.** B3's five N+1 sites are all outside this window — the boot one (`lib.rs:4474`,
the resume) runs *after* `build_library` returns and resolves 17 rows on this device, not 3,456:
`resumed 17 context + 0 queued`. B3 remains worth fixing on its own terms (a shuffle-all context
*is* the whole library, and `lib.rs:2314` pays it on the render thread holding the lock), but it
will not move the boot.

The three queries `build_library` issues are **already** the bulk shape B3 asks for. So the open
question is narrower and more interesting than either finding: those queries cost ~46 ms on the
host and the whole build costs ~4.5 s on device — roughly **100×**, where a 1 GHz in-order A7
against this host should be nearer 20–30× for SQLite work. Something other than raw CPU is being
paid. The lead is the PRAGMA note buried at the end of B3's remediation: `Db::open` sets **no
PRAGMAs at all**, so a 3,456-row `ORDER BY ob.sort_str, ob.title` spills its sort scratch to
SQLite's default `temp_store`, which on this device is eMMC. That is a measurement, not an
argument, and D5 takes it.

### D4. Landed this session

| Finding | State |
|---|---|
| **B1** | Shipped. `opt-level = 2`, with the corrections above recorded in `player/Cargo.toml`. |
| **B4** | Shipped, *device-unverified* — needs a volume-key press. Guarded by `vol_ctl_is_shim_control()` so an overridden `cinder_volume.conf`, or any ioctl failure, keeps the fork. |
| **B9** | Shipped. Entries carry a last-used tick; the oldest half is evicted, so the on-screen set survives by construction. Cap unchanged. |
| **B10** | Shipped, and it was worse than the audit knew — see below. |

**B10 was not merely dead weight; it was actively misleading.** The audit records that it prints
once at frame 300 and then costs two atomics and two clock reads forever, which is true. What
neither the audit nor this project had noticed is that *the number it printed was wrong*: on device
those 300 frames span first paint (~1.2 s) to about 14.7 s, so it sampled the near-empty boot screen
with the render thread starved by the synchronous library build — not the UI. It read 6.21 ms before
the B1 change and 6.23 ms after, a null result convincing enough to nearly retire B1 as
"doesn't transfer to hardware". It is now windowed, keeps sampling into real use, and switches off
completely after 30,000 frames.

### D5. B3 *is* the boot dead time — via site `lib.rs:2166`, and it is now fixed

*Measured (device).* D3 above said the boot cost was not B3. That was right about site `lib.rs:4474`
(the resume, 17 rows) and wrong about the conclusion, because it reasoned from the host profile —
where `/contents` does not exist, so the playlist and likes phases return instantly and the
breakdown looks like pure SQLite. Instrumenting the phases **on device** says otherwise:

```
cinder_db_open 4570 ms = db_open 85 + build 432 + playlists 24 + likes 2
                       + import 9 + lock 0 + install 3805 + artcache 211
install        3805 ms = set_library 1 + refresh_playlists 3802 + liked_save 0 + set_liked_count 0
```

**`refresh_playlists` is 83% of the boot dead time.** It calls `user_playlist_rows` — audit site
`lib.rs:2166`, the one §B3 correctly nominated to fix first — which resolves every entry of every
playlist through `track_by_filename`, a full `object_body` scan per call. The audit estimated
~315 ms on host and listed it as running "whenever the playlist model is rebuilt"; on this device,
with 8 playlists over a 3,456-track library, it is **3,802 ms and it runs on every boot**.

Fixed by the batch resolver §B3 proposed, `Db::tracks_by_filenames` — one scan, indexed by
basename — used at both filename sites:

| | before | after |
|---|---:|---:|
| `refresh_playlists` | 3,802 ms | **133 ms** (28.6×) |
| `cinder_db_open` | 4,570 ms | **895 ms** (5.1×) |
| boot window | ~4.6 s | **~0.9 s** |

Two notes for whoever does the remaining three sites (the `track_by_object_id` ones, which still
want `tracks_by_object_ids`):

* **The equivalence test is not optional.** `Track.filename` is the full path `query_tracks`
  reconstructs from parent rows; the `ob.filename` COLUMN that `track_by_filename` matches on is a
  bare basename. They are different strings, the first implementation indexed the wrong one, and it
  resolved *nothing* — caught immediately by `batch_filename_resolution_matches_single`, which
  covers the ambiguous-basename case precisely because that is where candidate ORDER decides the
  answer.
* **`build_library` was never the problem** — it is ~420 ms of the window, and its own SQL is
  ~300 ms of that. The PRAGMA lead in §B3's remediation is therefore worth at most a slice of
  ~300 ms and should be re-costed accordingly; it is no longer the interesting question it looked
  like in D3.

`cinder-home` now logs a one-line breakdown of `build_library` and of `cinder_db_open` on every
boot, so this question is answerable from a log rather than a flash-and-instrument cycle.

### D6. A latent test flake, seen once

`cinder-ui`'s `the_banner_keeps_the_battery_indicator` failed once under the parallel test runner
(`drew 286 px where the normal strip draws 374`) and did not reproduce in 6 runs on the change or 6
on the committed baseline. It cannot be the gradient change: `chrome.rs` never touches `art::`.
Every writer of the `ipc_dead` global takes `latch_lock()`, so the likelier suspect is the lazily
loaded shared `FontSet` racing across test threads — missing glyphs would explain a pixel count
that is low rather than wrong. Device-irrelevant (fonts load once, single-threaded, at startup),
but it is in the suite that gates every install, so it is written down rather than forgotten.

### D7. B3 completed, plus B7, B8, B11

*Device-verified for the boot path; the queue and context taps are code-complete and want a tap on
hardware.*

`tracks_by_object_ids` now sits beside `tracks_by_filenames`, and all five §B3 sites use one of
them. The two remaining `track_by_object_id` / `track_by_filename` calls in `cinder-ffi` are genuine
single lookups (one track added to a playlist; one now-playing URI) and are correct as written.

`PlayContextAt` was the site worth the most care: it resolved one id at a time **on the render
thread holding the renderer mutex**, over what is the entire library after "Shuffle all songs" —
the configuration behind the 2026-08-18 freeze, fixed in `play_order_uris` and left standing here.
Its start-index walk is deliberately unchanged; only the resolution moved.

B7 (dark pump 250 → 500 ms), B8 (hoist the cheap predicate out of the per-frame `run_guarded`) and
B11 (only repaint for a decoded cover on a screen that can show artwork) all landed as the audit
proposed. Measured after: **3.44 voluntary ctxt/s across all threads with the panel dark**, against
§B7's predicted ~3/s. The pump's own share of that is arithmetic (1/interval), not separately
isolated — an A/B would need a second flash and the deterministic part is not in doubt.

**Open from this list: B2, B5, B6, B12.** B2 and B6 both want a decision rather than a patch — B2
is a shipping default with a genuine "nobody has put a power number on it" caveat, and B6 carries a
real audio-underrun risk. B5 (per-frame heap churn) and B12 (flat 60 Hz while lit) are ordinary work.
