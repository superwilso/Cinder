# Deep sweep — 2026-08-24

A second, deeper pass over the areas the 2026-08-23 audits did not touch: cross-thread state, the
setuid-root helpers, the untested shim layer, panic reachability in Rust, and the SQL.

**The headline is that almost nothing was wrong.** Eight of nine areas came back clean, and the
ninth produced seven cosmetic warnings and no defects. That is a real result and it is recorded
here so nobody re-derives it — but it also means the honest conclusion of this sweep is *not* "here
are the bugs", it is **"the remaining risk is not in the code, it is in the fact that nothing was
checking it"**. So the sweep ends by turning the check on permanently.

---

## What was checked, and what was found

| # | Area | Method | Result |
|---|---|---|---|
| 1 | Cross-thread state (Sony's looper vs the render thread) | read every callback and every shared global | **clean** |
| 2 | The seven setuid-root helpers | argv, exec, path and TOCTOU review | **clean** |
| 3 | C/C++ under `-Wall -Wextra` | first compile ever with warnings on | 7 warnings, **0 defects** |
| 4 | Higher-signal warning classes | `-Wshadow -Wsign-compare -Wfloat-equal -Wpointer-arith -Wcast-align -Wuninitialized -Wnull-dereference` | 2 hits, both benign |
| 5 | The `PlayStatus` offset hack + its heap string | read both call sites | **clean** — destructed explicitly at each |
| 6 | `dump_status`'s `snprintf` arithmetic | bounds proof | **clean** — the row guard holds |
| 7 | Rust panic reachability under `panic = "abort"` | classify all 302 `unwrap`/`expect` | **clean** — see below |
| 8 | The BMP decoder against malformed album art | bounds proof | **clean** — `checked_add`/`checked_mul` + length test |
| 9 | `cinder-db` SQL construction | injection review | **clean** — bound parameters throughout |

### 1. Cross-thread state — clean

Every callback that lands on Sony's framework looper (`OnNotifySearchedDevice`,
`OnNotifyPairingComplete`, `OnBluetoothOob`, `OnNotifyChangeVolume`, the three link-state ones)
either takes the right mutex or touches only a `volatile sig_atomic_t` flag. `OnNotifySearchedDevice`
— the one with real work, de-duplicating and renaming entries in a shared vector — holds
`g_bt_found_mx` across the whole read-modify-write and sets its dirty flag after releasing. Correct.

The design rule the code follows throughout is: **callbacks copy and get out; everything that talks
to a `pst` client happens on the render thread.** It is followed without exception.

### 2. The setuid-root helpers — clean

Twelve `chmod 4755` installs, seven distinct binaries. Three take caller input and all three
allowlist it:

* `cinder-clock` — one verb, one argument, parsed **by hand** rather than with `strtol` so that
  nothing but ASCII digits is accepted. It already carries a fix for a 32-bit overflow found on
  2026-08-17 (`long` is 32 bits on armv7, so the range check tested an already-wrapped value; the
  accumulator is now `long long`).
* `cinder-power` — `strcmp` against exactly `off` / `restart`.
* `cinder-voltable` — `strcmp` against a fixed table, and the source is opened `O_NOFOLLOW`.

`cinder-msc` shells out eleven times but every command is a compile-time constant with an inlined
environment; nothing caller-supplied reaches a shell. No path injection, no TOCTOU, no argv
smuggling.

### 7. Rust panic reachability — clean

`panic = "abort"` in the release profile, so **any** panic kills the player and the launcher's crash
supervisor restarts it. That makes every panic site worth classifying:

| | count | reachable at runtime? |
|---|---:|---|
| `lock().unwrap()` on the global cell | 130 | **no** — `panic=abort` cannot poison a mutex |
| condvar `wait().unwrap()` | 6 | **no** — same reason |
| inside `#[cfg(test)]` | ~130 | no — not compiled into the device build |
| in `cinder-sim` / `cinder-host` / `cinder-db` examples | 13 | no — dev tools, never shipped |
| **on real device paths** | **~8** | **all guarded** |

The eight were read individually. The two worth naming:

* `bluetooth.rs:245` — `bt.connected.unwrap()` sits directly under `if bt.on && bt.connected.is_some()`.
  Safe, though `if let Some(name)` would be un-trippable rather than merely-currently-true.
* `art_load.rs` — the BMP header reads are all under a `b.len() < 54` guard and the highest offset
  touched is 0x22.

### 8. The BMP decoder — clean, and worth saying why it matters

Album art is parsed from files on the user's own storage, so a truncated or hostile file reaching a
slice index would abort the player. It does not:

```rust
if data_off.checked_add(stride.checked_mul(h)?)? > b.len() { return None; }
```

`checked_mul` then `checked_add` then a length test — overflow-safe *and* bounds-safe, in that
order. This is the single most exposed parser in the tree and it is correct.

---

## The seven warnings, and what they were

First compile of the C/C++ with warnings enabled, ever. **Seven across 19,435 lines**, none a defect:

| Where | Warning | What it actually was |
|---|---|---|
| `main.cpp:96`, `probe.cpp:117` | unused parameter `uc_` | signal-handler signatures that must match |
| `main.cpp:6823` | unused variable `ev_before` | captured, never compared — dead |
| `main.cpp:5609` | unused function `prop_equals` | **genuinely dead**, and it `popen()`s a shell |
| `main.cpp:6795` | unused function `take_req` | DEV-only helper, warning in stable builds |
| `tuner_shim.cpp:150` | unused function `rd` | a locked register wrapper with no matching `wr` |
| `probe.cpp:1830` | unused typedef `fna` | leftover from a shared typedef block |
| `ldac-bridge/main.c:30` | `_GNU_SOURCE` redefined | — |

All seven **fixed rather than tolerated**, so the tree can be held at zero.

Two more from the higher-signal classes, both left alone with a note:

* `main.cpp:76` `-Wfloat-equal` — `t0 == 0` is a deliberate sentinel test, and `-Wfloat-equal` is
  not in `-Wall`/`-Wextra`.
* `main.cpp:4313` `-Wshadow` — an `unsigned now` (a *frequency*) shadowing `const long now` (a
  *timestamp*) 24 lines up. Benign: the inner one is used two lines later and the outer is not
  wanted there. Confusing enough to be worth a rename if that function is touched again.

---

## The actual conclusion

**`-Wall -Wextra -Werror` is now on in `tools/host_syntax_check.sh`, in both build channels.**

That is the finding. Nine areas came back clean, which says the code is good — but the code was
good *yesterday too*, and nothing would have told anyone if it had stopped being. The seven
warnings had been sitting there through every one of those commits.

The gate proved itself within a minute of being switched on: it immediately caught the
`probe.cpp:117` and `probe.cpp:1830` cases, which this sweep's manual pass had missed because it
only compiled `main.cpp` and the shims by hand.

The check also now compiles `main.cpp` a second time **as the DEV channel builds it**, because
`take_req` and the discovery dump exist only there — and a warning that appears in one channel only
is exactly the kind that ships.

### Residual risk, unchanged

None of this touches the ABI. A host compile proves the code parses and is warning-clean; it cannot
tell you that vtable slot 20 is really `GetPairedDeviceInfo` on the device. That, and the
lifecycle/state class of defect that produced fourteen findings on 2026-08-23, both remain
device-only — which is the argument for the fake-service harness sketched in the simulator
discussion, not for more static sweeping. **This sweep has reached the end of what reading the code
can find.**

---

## Follow-up, same day: the harness exists

The sentence above was acted on rather than filed. [`cinder-home/harness/`](../cinder-home/harness/README.md)
links the real `main.cpp` against faked Sony service clients, a faked `easel` framework and a
virtual clock, runs the appmgr lifecycle, and asserts on the call trace. It is wired into the
`native` CI job and costs about two seconds including the build.

Five scenarios, twenty assertions. Three are regression tests for defects this project already
shipped: that the paired-device list and the notification listener are set up **during bring-up**
and not when a screen first needs them; that a Sony service which is not up yet degrades a feature
rather than the Home app; that the idle Bluetooth poll still backs off instead of running at 2 Hz
for the life of the process.

Two things it settled immediately, neither of which reading the code had:

* **The app boots off-device at all.** That was not obvious before trying — `main.cpp` is 7,900
  lines of device assumptions — and it turned out to need only three faked factories, one faked
  framework and a clock that advances instead of waiting.
* **A wrong slot index is visible now.** The first, hand-written slot map had `AddListener` at 39
  on `BtCommonServiceClient` and 30 on `BtTransmitterServiceClient`; it is the other way round, and
  the harness duly reported a bring-up step that ran correctly as missing. The map is now generated
  from `main.cpp`'s own call sites. This does not make the harness authoritative about the DEVICE's
  vtables — those come from the same RE notes either way — but it does mean the harness and the app
  can no longer disagree about them silently.

What this does not close: the ABI itself, the ARM link and the GLIBC ceiling, UI input (touch
arrives from `/dev/input`), the `dlopen`ed services, and `cinder-audio`'s shims, which sit behind
the stub boundary. Those remain device-gated, and the list of what the next device session owes is
unchanged.
