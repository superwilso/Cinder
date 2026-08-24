# Cinder — project and repository shortcomings

**Audited 2026-08-23.** A standing reference: what is structurally weak about this project and its
GitHub setup, with the evidence for each claim. Not a bug list — `docs/AUDIT_*.md` hold those. This
is about the *conditions that let those bugs exist and survive*.

Everything below was measured against the tree at `c028a37` unless stated. Where a claim is
inference rather than measurement, it says so.

**This project is unusually strong in one dimension and unusually weak in another, and they are the
same fact seen twice.** The reasoning behind the code is documented to a standard most commercial
projects never reach — and almost none of it is enforced by anything a machine checks. Part E lists
what is genuinely good, and it is not a courtesy section: the strengths are what make the weaknesses
survivable.

---

## Part A — The verification gap is inverted

### A1. 41% of the codebase is never compiled by any automated gate

| Surface | Lines | Compiled by CI? | Tested by CI? |
|---|---:|---|---|
| Rust (`player/`, `installer/`) | 35,921 | **yes** | **yes** (404 tests) |
| C / C++ (`cinder-home/src`, `cinder-audio`, `ldac-bridge`) | 19,435 | no | no |
| Shell (33 scripts) | 5,288 | n/a | no |
| **Never touched by CI** | **24,723 (41%)** | | |

That would be defensible if the uncovered 41% were peripheral. **It is the opposite.** The
uncovered half is the code that:

* runs as **root** — twelve `chmod 4755` setuid installs in `install_cinderhome.sh` alone;
* owns the **boot path** — the launcher, the crash supervisor, the bad-boot counter, the
  auto-revert ladder, i.e. every mechanism standing between a bad build and a brick;
* drives **closed Sony services** over hand-recovered vtable offsets, where a wrong argument shape
  reaches `operator new[]` inside a core service (this rebooted the device twice on 2026-08-11);
* performs the **USB-MSC handoff**, where an ordering mistake corrupts the user's music volume.

The well-covered half — `cinder-ui`, with 311 tests — is pure drawing and navigation logic that
cannot brick anything, cannot corrupt anything, and cannot escalate privilege.

**The testing effort is inversely proportional to the blast radius.** This is the single most
important structural finding in this document.

### A2. `cinder-audio` has zero tests and drives every Sony service

2,500 lines of C++ shim — `player_shim.cpp`, `effect_shim.cpp`, `tuner_shim.cpp`,
`analyzer_shim.cpp`, `power_shim.cpp` — is the entire IPC surface to PlayerService, EffectCtrlDmp,
the FM tuner and the power manager. It has **no tests of any kind** and **is not compiled by CI**.

Its correctness rests on hand-recovered ABI declarations in `effect_abi.hpp` /
`playerservice_abi.hpp`, where being wrong does not produce a compile error — it produces a
mis-marshalled call into a closed service. This is precisely the failure mode the project has
already been bitten by, and it is the one part of the tree with no automated check at all.

### A3. Shell is load-bearing, and 5,288 lines of it are unlinted

`install_cinderhome.sh` is 757 lines and contains the crash supervisor, the bad-boot counter, the
escape ladder and the kill switch. `cinder-msc.c`'s helper scripts perform the mount/unmount
ordering that the file's own header warns will "eat the user's library" if reversed.

No `shellcheck`, no `bash -n` syntax gate, no test harness. `tools/test_launcher.sh` exists and
covers a 44-case recovery matrix — **and nothing automatic runs it.**

> **Both halves closed 2026-08-24.** `tools/shell_check.sh` (pinned `shellcheck` + `bash -n` over
> all 36 scripts) and the launcher matrix now both run in the `native` CI job. The matrix also
> stopped lying: one case makes `/data/cinder` unwritable with `chmod`, which does not bind uid 0,
> so run as root it reported a failure about the tester rather than the launcher. It skips itself
> there now, with a root-proof variant covering the same rule — 45 cases as root, 46 as a normal
> user, zero failures either way. The launcher itself was already correct; the guard it needs was
> added after the 2026-07-26 brick and is proven by write-then-read-back rather than `[ -w ]`.

### A4. The self-tests exist, and nothing automatic runs them

`cinder-home/build.sh` runs six C++ self-tests (guard recovery, volume ramp, BT edge, jack edge,
BT switch reconcile, DB signature), the GLIBC ≤2.23 ceiling gate, and the qemu construction
preflight.

**`build.sh` is invoked from exactly one place: `tools/release.sh`** — a manual, local, opt-in
script a maintainer chooses to run. No workflow calls it. So on any given commit those gates are
worth exactly as much as someone remembering.

This is recursive: the self-tests added during the 2026-08-23 audits are subject to the same
problem the moment they were written.

### A5. The device is a single point of verification, and it is a bottleneck

By design — and correctly — most of what matters can only be settled on hardware. But the
consequence is not managed:

* `STATUS.md` carries a standing list of **device-unverified** claims that only grows between
  hardware sessions.
* The project has twice discovered that a *successful write is not evidence a feature works*
  (high gain; and DSEE AI is currently in the same unresolved position — see A7).
* There is no staging environment. `cinder-probe` is the mitigation and a good one, but it still
  requires the device.

> **Partly addressed 2026-08-24.** [`cinder-home/harness/`](../cinder-home/harness/README.md) boots
> the real `main.cpp` off-device against fake services, so bring-up ORDER, service-availability
> behaviour and polling RATE can now be observed without hardware. It does not reduce what the
> device must settle — ABI shapes, whether a write did anything, audio itself — but it moves the
> "did the app do the right thing given that answer" half off the critical path, and that half is
> where the fourteen findings of 2026-08-23 came from.

There is no tracked list of "claims awaiting hardware confirmation" separate from the feature
matrix, so unverified claims and verified ones sit in the same tables and are distinguished only by
prose.

---

## Part B — The recurring defect class

### B1. Process-lifetime thinking about service-lifetime state

Four of the fourteen defects fixed in the 2026-08-23 audits are **the same bug**, and it has a name
worth adopting: *an assertion about somebody else's state, written as if it were our own
preference.* A fifth instance — `SetSelectUsingEq`, uncalled for months — was found earlier and is
listed with them because it is the clearest example of the class.

| Defect | What was assumed | Reality |
|---|---|---|
| BT switch (`cinder_set_bt_on`) | one boot read is the answer forever | the radio answers `-1`/`0` while coming up; nothing ever re-asked |
| BT pairing table | populated when a screen needs it | `bt_reconnect_tick` needs it at boot, when nothing had read it |
| BT connect-wait cache | starts `false` | the state is sticky in the service and outlives the process |
| DSP boot reconcile | "no settings file → nothing to push" | the DSP holds what the *stock player* left |
| `SetSelectUsingEq` | not called at all, for months | the device defaults to a tone system Cinder does not expose |

Each was written by someone who had reasoned carefully about the local case. The shared blind spot
is in the heading: Cinder is a guest process on a device whose services outlive it, boot before it,
and are also written to by stock software — but the code repeatedly assumes that what it learned at
startup stays true, and that what it never set is unset.

**No convention exists to catch this.** `bt_service_retry` reconciles against a getter and explains
why in a comment; the function immediately below it does not. The correct pattern is present in the
codebase, adjacent to violations of it, and nothing propagates it.

> **2026-08-24 — now at least detectable.** A convention is still the right fix (Part F item 6),
> but the class is no longer invisible to automation:
> [`cinder-home/harness/`](../cinder-home/harness/README.md) boots the real `main.cpp` against fake
> Sony services and asserts on the call trace, and every row in the table above is a question about
> that trace. The `bt-late-service` scenario makes the factory fail four times and then work — the
> exact shape of the first two rows — and fails if the app stops asking. The catch is that a
> scenario has to be written: the harness proves a *known* assumption is still handled, it does not
> find the next unexamined one.

### B2. "The write landed" keeps being treated as evidence

The project learned this expensively with high gain — the mixer control accepted the write, read
back `1`, persisted across reboots, and the codec ignored it because the A50 output stage lacks the
hardware. `STATUS.md` records the lesson in capitals.

**DSEE AI is in exactly that position right now**, and shipped for months drawn identically to the
toggles that work, despite `analysis/RE_dsp_effects_surface.md` and `cinder_effects.h` both saying
"UNVERIFIED — treat like high gain until heard". The 2026-08-23 audit changed the label; it did not
settle the question, which needs an ear test.

There is no marker convention (a type, a naming rule, a lint) distinguishing *verified-audible*
from *the-call-returns-0*. The distinction lives only in prose that the UI layer does not read.

---

## Part C — Documentation

### C1. 14,545 lines of Markdown, and the top of the largest file says it is stale

Excluding the design handoff dump, the repo carries ~14.5k lines of Markdown against ~55k lines of
code — a ~26% doc-to-code ratio, which is exceptional. The problem is not volume, it is **decay
without a decay process**:

* `cinder-home/STATUS.md` is **1,628 lines** and opens by saying its own matrix "was last re-audited
  2026-07-30 and several of its entries are now stale; the ones proven wrong have been struck
  through in place."
* `CLAUDE.md` (582 lines) opens with a banner explaining that its own premise is historical.
* Four `AUDIT_*.md` files, two `PLAN_*.md`, `ROADMAP.md`, `PRODUCTION_READINESS.md`,
  `COMPARISON_*.md` — each a point-in-time snapshot, none with an expiry or an owner.

**At least four files claim to be the single source of truth** for overlapping questions
(`STATUS.md` "the single source of truth" for feature state; `ROADMAP.md` for the plan;
`AUDIT_2026-08-16.md` "the current gap list"; `PRODUCTION_READINESS.md` for what is missing). A
reader cannot know which is current without reading all of them and comparing dates.

### C2. The commit history carries none of the reasoning

This is the sharpest contrast in the project. In-code comments routinely explain the measurement,
the wrong hypothesis, the date it was disproved and the log line that settled it. The commit log for
the same work reads:

```
bff3172 Update binary files for cinder-home and cinder-probe, and enhance playback sequence handling
8eeaf4a Update binary files for cinder-home and cinder-probe
9499e94 Unfinished patch.
78acf3f Refactor code structure for improved readability and maintainability
```

`git log` and `git blame` — the tools built for exactly the archaeology this project does
constantly — are close to useless here. The knowledge is real and durable; it is simply in the one
place that cannot be queried by "when did this change and why".

### C3. Fifteen work-item references point at nothing

Code and docs cite `task #21`, `task #26` (×4), `task #31`, `task #40`, `task #46`, `task #56`,
`task #59`, `(#1)`, `(#4)`, `(#25)`, `(#55)`, `(#58)`, `(#63)`.

**The repository has zero issues, open or closed.** Every one of those references is unresolvable by
anyone reading the repo — including its author in six months.

---

## Part D — GitHub and repository setup

### D1. CI covers the safe half and skips the dangerous half

`.github/workflows/ci.yml` runs, on every push and PR:

* `cargo test --release` + `cargo build --release` for `player/` and `installer/`
* a payload-existence check on `cinder-home/dist/stable/*`
* a `file(1)` check that the committed binaries are ARM

It does **not** run: any C/C++ compile, `cinder-home/build.sh` (and therefore none of the six C++
self-tests, the GLIBC ceiling gate, or the qemu preflight), any shell lint, the 44-case launcher
recovery matrix, `cargo clippy`, or `cargo fmt --check`.

The ARM cross-build being absent is **documented and defensible** — it needs a glibc-2.23 +
libc++-3.9.0 toolchain that would be real machinery to maintain on a hosted runner. **But the C++
self-tests do not need that toolchain.** They are host-compiled with plain `cc` and run in
milliseconds. They are skipped not by necessity but because they live inside a script CI never
calls.

### D2. No lint or format gate

No `rustfmt.toml`, no `clippy.toml`, no `.editorconfig`, and neither tool runs in CI. On a codebase
this comment-dense, formatting drift is a live merge-conflict source.

### D3. Pull requests are ceremonial

* **58 commits on `main`; 2 are merge commits.** ~56 changes went straight to `main`.
* **Three PRs have ever existed.** PR #2 was created at `20:56:52` and merged at `20:57:03` —
  **eleven seconds**, which is less than the CI run it was supposed to gate.
* No PR template, no `CODEOWNERS`, no review requirement in evidence.

The direct-push pattern is conclusive evidence that `main` accepts pushes without a PR. (Whether
branch protection is configured and simply permits it, I could not read — that needs repo admin
access.)

This matters more than usual here, because a bad `main` is not an inconvenience: `main` is what
`tools/release.sh` tags, and a release flashes a device with no public recovery path.

### D4. The release integrity guard is real, correct, and opt-in

`tools/release.sh` does the right thing thoroughly: refuses a dirty tree, checks the installer
version against the tag, **rebuilds the ARM payload from source and refuses to tag unless every
committed byte matches**. This is a genuinely well-designed guard against the project's scariest
silent failure — shipping stale device binaries.

**And nothing requires its use.** `release.yml` triggers on `push: tags: ["v*"]`. A plain
`git tag v0.1.4 && git push --tags` bypasses every check above and publishes. The workflow's own
payload step verifies only that the files *exist* and are ARM — never that they match the source.

`release.yml`'s header states the hazard plainly ("BUILD AND COMMIT dist/ BEFORE TAGGING, or the
release ships whatever was last committed"). It is a known, documented, unenforced risk. **Four
releases have shipped** under it.

### D5. Committed binaries have permanently bloated the repository

| File | Size | Commits touching it |
|---|---:|---:|
| `cinder-home/cinder-home.unstripped` | 6.1 MB | 36 |
| `cinder-home/cinder-probe.unstripped` | 6.1 MB | ~36 |
| `dist/stable/cinder-home` | 3.5 MB | 17 |
| `dist/stable/cinder-probe` | 3.6 MB | 17 |

`.git` is **83 MB** for a 1,706-file source repository. Committing `dist/` is a deliberate,
reasoned choice (the cross toolchain is not reproducible on a runner) and is defensible. Committing
the **6 MB `.unstripped` debug artifacts 36 times over** is not — they are build byproducts, not
deliverables, and they are the largest single contributor.

This is unfixable without history rewriting, which is why it belongs in a permanent-record document
rather than a to-do list.

### D6. Missing repository hygiene

Absent: `CONTRIBUTING.md`, `CODEOWNERS`, `SECURITY.md`, issue templates, PR template,
`dependabot.yml`, `.editorconfig`.

`SECURITY.md` is not box-ticking here. This project ships **twelve setuid-root binaries** to a
device, distributes an **unsigned** Windows executable that drives a firmware flasher, and has no
stated way to report a vulnerability in any of it.

### D7. Supply chain: unsigned installer, no provenance

The release attaches `SHA256SUMS` and the body explains how to check it — good, and better than
most hobby projects manage. But:

* the installer is **unsigned**, so SmartScreen warns and users are conditioned to click through;
* there is no build provenance/attestation, so the checksum proves the download matches *what the
  workflow produced*, not that the workflow built the source it claims;
* the ARM payload inside is **committed blobs**, so even a perfect installer build says nothing
  about whether those bytes came from this source tree (see D4).

For software that flashes a device with no recovery path, that is the weakest link in the chain.

---

## Part E — What is genuinely strong

Stated plainly, because the weaknesses above are only survivable *because* of these.

* **The comments are the best artifact in the project.** They record the wrong hypothesis, the
  measurement that killed it, and the date. `cinder-msc.c`'s header — explaining that MSC "never was
  a race" and naming both root-only steps — would have saved weeks if written earlier, and will save
  them for the next reader.
* **Safety engineering is taken seriously and is layered.** Bad-boot counter → auto-revert → crash
  supervisor → kill switch → wbrt restore, each depending on strictly less than the layer above.
  `RECOVERY.md` exists and is honest about there being no DFU path.
* **`run_guarded`.** Every Sony IPC call runs behind crash+hang recovery, so a bad service degrades
  a feature instead of bricking the boot. This is the single best design decision in the codebase.
* **Pure-logic extraction for testability.** `bt_edge.h`, `jack_edge.h`, `vol_ramp.h`, `db_sig.h`,
  `bt_switch.h`, `sound::signal_path` — small rules pulled out where a host test can reach them.
  The pattern is right; it needs a CI hook (D1) and wider application (A2).
* **The UI overflow matrix.** 22 screens × 2 content sets × 2 themes × 7 UI scales, which found five
  real defects that had no visible symptom because `Canvas` clips silently.
* **Honesty about negative results.** High gain was *removed* when measurement disproved it, and the
  removal is documented so it is not re-added. That is rarer and more valuable than most features.

---

## Part F — Remediation, in order of value per unit of effort

> ### Status — 2026-08-23
>
> **Items 1, 2 and 3 are done; 9 is half done.** `ci.yml` gained a `native` job running
> `tools/host_syntax_check.sh` (18 C/C++ files), the six C++ self-tests, `bash -n` and
> `shellcheck -S warning` over all 33 scripts — plus a clippy gate scoped to `correctness` +
> `suspicious` on the Rust jobs. That closes **A3**, **A4** and **D2**, and closes **A1**/**D1**
> apart from the ARM link, the GLIBC ceiling gate and the qemu preflight, which genuinely need the
> cross toolchain.
>
> The same change removed a duplicate trigger (`push: ["**"]` *and* `pull_request` both fired on
> every PR branch), so **CI now covers ~24,700 more lines while running fewer jobs than before —
> 8 per push down to 5.**
>
> Two things worth recording from doing it: the syntax check found a real latent bug on its first
> run (`probe.cpp` using `uintptr_t` in seven places with no `<cstdint>`), and the four shellcheck
> findings were *fixed* rather than silenced — one of which, a hardcoded personal Windows path in
> `flash.sh`, had no business in a public repo.
>
> `cargo fmt --check` (the other half of 9) was deliberately **not** added: `fmt` fails on both
> workspaces today, so the gate would be red on arrival. It needs a formatting commit first, and
> that is a separate decision on a comment-dense tree.
>
> **The gate went red on its own first run**, and the reason is worth keeping. The steps were
> inlined in `ci.yml` and verified locally *by hand*; the local shellcheck was 0.11.0 and the
> runner's was older, and the two disagree about `#!/system/xbin/busybox sh`. So the fix was not
> just the finding — CI and a contributor now run the **same script** (`tools/shell_check.sh`,
> `tools/host_syntax_check.sh`) against a **pinned** linter. A gate whose version floats is a gate
> that can turn red with no source change: the same fragility class as the `<cstdint>` bug above.
>
> **2026-08-24 — the gate got stricter.** `tools/host_syntax_check.sh` now runs
> `-Wall -Wextra -Werror`, in **both** build channels. The C/C++ had never been compiled with
> warnings on; the first run produced seven across 19,435 lines and all seven were fixed rather
> than tolerated. The gate then immediately caught two more that a manual sweep had missed. Deep
> sweep and its nine clean areas: [`AUDIT_2026-08-24_deep_sweep.md`](AUDIT_2026-08-24_deep_sweep.md).
>
> **2026-08-24 — the app now BOOTS in CI.** The deep sweep ended by saying static analysis had
> reached its limit and that the remaining defect class needed a fake-service harness. That harness
> exists: [`cinder-home/harness/`](../cinder-home/harness/README.md) links the real `main.cpp`
> against faked Sony service clients, a faked easel framework and a **virtual clock**, runs the
> appmgr lifecycle, and asserts on the resulting call trace. Five scenarios, twenty assertions,
> about two seconds including the build — because sleeping advances a counter instead of waiting,
> so two virtual minutes of device time costs milliseconds.
>
> This is the first automated check that can see **B1** at all. A test of `bt_switch.h` says the
> reconcile rule is right; the harness says the app *runs* it, during boot, and keeps running it
> when the service was not there the first time. Three of the five scenarios are direct regression
> tests for defects already shipped and fixed.
>
> It also found a real error immediately — in itself. The hand-written slot map had `AddListener`'s
> two indices swapped between the two Bluetooth clients, so a bring-up step that worked was
> reported as missing. That map is now **generated from `main.cpp`'s own call sites**: a harness has
> to be harder to be wrong about than the thing it checks.
>
> What it still does not cover: the ABI (the fakes agree with the RE notes, so where those are
> wrong it is wrong with them), the ARM link and GLIBC ceiling, UI input (touch comes from
> `/dev/input`, which does not exist off-device), and `dlopen`ed services, which take their
> degraded branch. **A2 is untouched** — `cinder-audio`'s shims are behind the stub boundary, so
> the harness exercises the app's use of them, not the shims themselves.
>
> **2026-08-24, end of day — where the harness got to.** Twenty scenarios, 74 assertions, nine
> seconds including the build, and it now also runs from `build.sh`, so it gates a flash and not
> just a push. Beyond the bring-up and pacing work above it grew a fake device filesystem (`fopen`,
> `open` and `stat` served from a private tree, with files that can CHANGE part way through a run)
> and fake input (`/dev/input/event*` as real FIFOs, so touch and buttons reach the app the way the
> driver delivers them). That closed the last two "nothing checks this at all" surfaces:
>
> * **hardware edges** — headphones out mid-track pauses within 496 ms; a PC appearing hands the
>   volume over once and takes it back when the cable goes; auto power-off fires when idle and does
>   not fire while playing or on a charger;
> * **input** — a dark panel wakes on touch *without* also pressing what was under the finger; a
>   tap is a tap and a drag is a drag; raw evdev codes decode to the right buttons; the volume
>   rocker accelerates, stops on release, and gives up on a stuck key.
>
> Seven defects in total, all one shape — **work on a timer, for a condition that cannot change,
> that nobody stops** — and the pacing rule now lives in one place (`retry_log`) rather than seven.
>
> **What it still cannot see** is written down in `cinder-home/harness/README.md` and again in
> [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md): the ABI, the ARM link and GLIBC ceiling, `alarm()`
> and the guard budgets, `dlopen`ed services, the navigator's own decisions, **A2's `cinder-audio`
> shims**, and — the one worth repeating — `system`/`popen` are recording stubs, so every setuid
> helper "fails" there and the SUCCESS paths of MSC and power-off have no coverage at all.
>
> **Still open: 4, 5, 6, 7, 8, 10.**

| # | Action | Cost | Why this order |
|---|---|---|---|
| 1 | ✅ **Call `cinder-home/build.sh`'s self-tests from CI.** Extract the six `cc`-compiled self-tests into a `selftests` job (or a `build.sh --host-tests-only` flag). No cross toolchain needed. | ~1 h | Turns six existing, written, passing gates from opt-in into enforced. Highest ratio in the table. |
| 2 | ✅ **Add a `bash -n` + `shellcheck` job** over the 33 scripts. | ~1 h | 5,288 lines of root-privileged, boot-path shell currently has no syntax gate at all. |
| 3 | ✅ **Compile-check the C++ on the host** — `cinder-audio` + `cinder-home/src` against stub headers, `-fsyntax-only` if linking is impractical. | ~half day | Would have caught this session's C++ edits, which shipped uncompiled. Closes the worst of A1. |
| 4 | **Make `release.sh` the only way to release**: have `release.yml` re-run the payload-vs-source comparison rather than an existence check. | ~2 h | The guard already exists and is correct; it is simply bypassable. Protects the flash path. |
| 5 | **Stop committing `*.unstripped`.** `.gitignore` them; keep `dist/`. | 10 min | Stops the bleeding on D5. Does not fix history, and should not try to. |
| 6 | **Adopt a "service state" convention** for B1 — a naming rule or a helper (`reconcile_*` vs `apply_*`) that makes "assertion about a service" visually distinct from "push a preference". | ~half day | Five defects in one audit came from this. A convention is cheaper than finding the sixth. |
| 7 | **Add `SECURITY.md` + `CONTRIBUTING.md`.** | ~1 h | Twelve setuid binaries and an unsigned flasher, with no disclosure route. |
| 8 | **Either use the issue tracker or stop citing it.** Fifteen dangling references. | ~2 h | Cheap, and it makes the excellent comments navigable. |
| 9 | ◐ **Add `cargo clippy`** (done, scoped to `correctness` + `suspicious`) **+ `cargo fmt --check`** (NOT done — `fmt` fails on both workspaces today, so the gate would be red on arrival; it needs a formatting commit first). | ~30 min | Low value against the rest, listed for completeness. |
| 10 | **Require PRs on `main`.** | ~10 min | Deliberately last: the current workflow is one person moving fast, and a rule nobody wants gets bypassed. Worth doing when a second contributor appears, not before. |

### Deliberately not recommended

* **Rewriting history to purge the blobs.** 83 MB is annoying, not harmful, and a rewrite breaks
  every existing clone and release reference.
* **Reproducing the ARM cross toolchain in CI.** The existing reasoning is sound; item 3 gets most
  of the benefit for a fraction of the cost.
* **Consolidating the Markdown into one file.** The sprawl is a symptom of a real method (write down
  what was measured, when). A dated index with an owner per document fixes C1 better than a merge
  would.
