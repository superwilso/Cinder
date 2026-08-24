# The off-device harness

Boots the **real** `cinder-home/src/main.cpp` on a build machine, against fake Sony services and a
virtual clock, and asserts on the sequence of calls it makes.

```sh
cinder-home/harness/run.sh              # every scenario  (~2 s, from scratch)
cinder-home/harness/run.sh boot         # one scenario
CINDER_HARNESS_TRACE=1 cinder-home/harness/run.sh boot   # …and dump the trace if it fails
```

No device, no cross-compiler, no network. Nothing to install beyond a C++ compiler and python3.

## Why

Every Bluetooth and battery defect this project has shipped so far has been a defect in a **call
sequence**, not in a value:

| defect | what it actually was |
|---|---|
| "Bluetooth never reconnects" | the paired list and the notification listener were only set up when the user opened the Bluetooth screen |
| the switch and the radio disagreed | the switch believed its own last state instead of reading the radio |
| `SetSelectUsingEq` never applied | a call that was never made at all |
| the DSP reconcile was skipped | it ran only `if (g_settings_loaded)` |
| BT playback ate the battery | four services polled at ~2 Hz for the life of the process |

None of those are visible in a unit test of a pure function, and every one of them is obvious in a
boot trace. The pure-logic headers (`bt_switch.h`, `bt_poll.h`, `db_sig.h`, …) test the *rules*; this
tests whether the app **runs** them, when, and how often.

## How it works

* **`stubs.cpp` (generated, `gen_stubs.py`)** — one recording stub for each of the ~230 `cinder_*`
  functions the app calls, with signatures taken from the real headers so a stub cannot drift from
  the thing it replaces. Each stub records the call and returns a value the test can script.
* **`fake_pst.cpp`** — the three Sony service clients `main.cpp` links against directly. The app
  reaches them through raw vtable slot indices; the slot→method map is **generated from those call
  sites** (`gen_slotmap.py`), because the hand-written first version had `AddListener`'s two slots
  swapped and reported a working bring-up as a missing one. The Bluetooth fake is stateful: it
  models the status enum `main.cpp` documents (7 = off, 2 = on, 3 = connected), and a connect
  against a powered-down radio is accepted and silently dropped — the device behaviour that made
  "Bluetooth doesn't connect automatically" so hard to see.
* **`fake_easel.cpp`** — Sony's app framework, implemented against the same hand-recovered
  `easel_abi.hpp` the device build uses, and driving the same lifecycle appmgr does
  (Initialize → PostInitialize → Activate → Foreground). Booting *is* the test.
* **`fakefs.cpp`** — the device's sysfs, procfs and `/contents`, as far as the app can tell. Nearly
  everything cinder-home knows about its hardware it reads with `fopen` from an absolute path — the
  battery percentage, whether a charger is attached, whether the headphones are plugged in — so one
  `fopen` override serves those from a private tree and lets anything not placed there fall through
  to the real filesystem, where absent still means absent. Files can also be scheduled to CHANGE
  part way through a run (`cinder_harness_fs_write_at`), which is what makes edges — the headphones
  coming out, a PC appearing — into scenarios. Every open is traced, because "opened once per second
  for the life of the process" is a defect this project has already had to fix once.
* **`harness.cpp`** — the trace store, the scripting table, and a **virtual clock**. `usleep`,
  `sleep`, `clock_gettime` and `time` are defined here and win over libc, so sleeping does not wait,
  it advances a counter. Two virtual minutes of device time cost a few milliseconds.

  Advancing it is discrete-event scheduling: every sleeping thread registers the virtual time it
  wants to wake at, and the clock jumps to the **earliest** of those, but only once no thread is
  still executing app code. The obvious alternative — "one thread owns the clock, claimed by the
  first to sleep" — looked right and was wrong: the app's own `healthy_timer` is a detached thread
  created by the frame loop one statement before the frame loop's first sleep, and it sleeps for
  nine seconds and exits. Lose that race and the clock jumped nine seconds in one step and then
  belonged to a dead thread. It passed locally and hung in CI.

## What it does not do

* **It is not a device session.** The fakes answer the way `analysis/` says the services answer;
  where those notes are wrong, the harness is confidently wrong with them. What it proves is that
  the app does the right thing *given* those answers.
* **It does not build the shipping binary.** `cinder-home/build.sh` does — ARM, glibc ≤ 2.23,
  libc++ ABI, qemu preflight. A harness pass says nothing about whether the thing links for the
  device.
* **No UI input yet.** Touch and buttons arrive from `/dev/input`, which does not exist here, so
  scenarios cover bring-up, service availability, pacing and hardware edges — not gestures. Feeding
  synthetic `input_event` frames through a faked `open()` on `/dev/input/event*` is the obvious next
  step; `fakefs.cpp` already does the equivalent for everything reached with `fopen`.
* **No `dlopen`ed services.** NFC, the display service and the USB manager are loaded by name at
  runtime; here `dlopen` records the request and returns null, so those paths take their
  degraded branch. Faking them is tier 2.

## Adding a scenario

Add a function and a row to `kScenarios` in `scenarios.cpp`. Set up the world
(`cinder_harness_script*`, `cinder_harness_bt_*`), pick a budget, call `cinder_harness_run()`, then
assert on `cinder_harness_count / _arg / _first_ms / _last_ms / _count_between / _before`.

Each scenario runs in **its own process** — `main.cpp`'s bring-up is one-shot static state, so a
second boot in the same process would not be a boot.
