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
* **`fakeinput.cpp`** — a touchscreen and a button block. The app reads `/dev/input/event*`
  directly: `opendir`, a non-blocking `open` per node, `EVIOCGABS` to find the panel and its
  coordinate range, `EVIOCGRAB` to hold it, then a `read()` per node per frame. The nodes here are
  real **FIFOs** in the fake tree with the harness holding the write ends, so the app's own `read()`
  is untouched — it opens a path, gets a pipe, and reads `input_event` structs exactly as it would
  from the driver. Only `opendir`, `ioctl` and `poll` are faked around that. The panel reports
  0..480 / 0..800 so raw and UI coordinates are the same thing and a scenario's numbers are the
  navigator's numbers.
* **`fakefs.cpp`** — the device's sysfs, procfs and `/contents`, as far as the app can tell. Nearly
  everything cinder-home knows about its hardware it reads from an absolute path — the battery
  percentage, whether a charger is attached, whether the headphones are plugged in — so `fopen` and
  `open` are overridden to serve those from a private tree, and anything not placed there falls
  through to the real filesystem, where absent still means absent. `access` and `stat` are
  deliberately **not** faked: they are presence checks a scenario wants answered honestly. Files can
  also be scheduled to CHANGE part way through a run (`cinder_harness_fs_write_at`), which is what
  makes edges — the headphones coming out, a PC appearing — into scenarios. Every open is traced,
  because "opened once per second for the life of the process" is a defect this project has already
  had to fix once.
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

## Calls that cost time

Stubs answer instantly, and that is not a neutral default: it quietly makes every scenario a test of
an infinitely fast device, where no amount of tapping can back the app up. `carry_out` runs on the
render thread and every transport action inside it is a synchronous Sony round trip, so on hardware
a press costs a wait, not an event — and the whole frame loop is stopped for it.

`cinder_harness_script_delay(name, ms)` makes a call sleep `ms` of **virtual** time before it
returns, on the app's own thread, which is exactly what a blocking IPC does on the device. That is
what made the `rapid-skip-touch` defect visible off-device: forty taps in four seconds left the app
still issuing skips 13 s after the last one, with housekeeping having run once in fifteen seconds.
Nothing else in the suite could see it, because with an instant `NextTrack` the backlog cannot form.

Costs one global load per stub call while no delay is armed, so the long scenarios are unaffected.

## Two things that behave differently here

* **`poll()` is virtualised, `alarm()` is not.** The frame loop waits on its input descriptors with
  `poll()` once input is up, which is right on the device and would run the harness in *real* time —
  a 70-second scenario took 70 seconds. It now asks the kernel whether anything is ready right now,
  and if not sleeps the timeout on the virtual clock; scheduled input events are part of what the
  clock stops for, so a tap aimed at t=50s arrives at t=50s.
* **`alarm()` is real time.** The virtual clock covers sleeping, not signals, so the app's own
  construction watchdog and every `run_guarded` budget are measured in wall-clock seconds. Scenarios
  therefore cannot exercise the guard timeouts cheaply — a renderer that fails to initialise takes
  20 real seconds to trip the watchdog, which is why that case is explored by hand rather than
  pinned as a scenario.
* **`system` and `popen` never run anything.** They are recorded and return success and null
  respectively, so every setuid helper "fails" — which is usually the case worth testing (three of
  the defects found so far live in what happens when a helper does not work), but it does mean a
  scenario cannot observe a helper's effect, only that it was asked for.

## What it does not do

* **It is not a device session.** The fakes answer the way `analysis/` says the services answer;
  where those notes are wrong, the harness is confidently wrong with them. What it proves is that
  the app does the right thing *given* those answers.
* **It does not build the shipping binary.** `cinder-home/build.sh` does — ARM, glibc ≤ 2.23,
  libc++ ABI, qemu preflight. A harness pass says nothing about whether the thing links for the
  device.
* **The navigator is a stub.** `fakeinput.cpp` delivers real touch and button events (below), so
  everything between a raw evdev code and `cinder_input`/`cinder_tap` is covered — but what the
  navigator *decides* is Rust, and it returns "no action" here. `cinder-ui`'s 404 tests cover that
  half; a scenario that needs an action to actually happen scripts the return value.
* **No `dlopen`ed services.** NFC, the display service and the USB manager are loaded by name at
  runtime; here `dlopen` records the request and returns null, so those paths take their
  degraded branch. Faking them is tier 2.

## Adding a scenario

Add a function and a row to `kScenarios` in `scenarios.cpp`. Set up the world
(`cinder_harness_script*`, `cinder_harness_bt_*`), pick a budget, call `cinder_harness_run()`, then
assert on `cinder_harness_count / _arg / _first_ms / _last_ms / _count_between / _before`.

Each scenario runs in **its own process** — `main.cpp`'s bring-up is one-shot static state, so a
second boot in the same process would not be a boot.
