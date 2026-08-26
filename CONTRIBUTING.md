# Contributing to Cinder

Cinder is a replacement Home application for the Sony NW-A50 series. It drives Sony's closed
services over binder IPC through hand-recovered vtable offsets, runs as the device's launcher, and
ships setuid-root helpers. **The device has no public DFU or EDL recovery path.**

So the bar here is not "does it compile". It is: *if this is wrong, can the user still get their
player back?*

## Before you start

Read [`docs/DEVICE_CHECKLIST.md`](docs/DEVICE_CHECKLIST.md) — the safety rules at the top,
especially. Then [`SECURITY.md`](SECURITY.md) for the four rules that exist because something
already went wrong.

## Running the checks

Everything CI runs, you can run locally, and you should:

```sh
tools/host_syntax_check.sh          # C/C++ parse + -Wall -Wextra -Werror, both channels
tools/shell_check.sh                # bash -n + pinned shellcheck over the shell scripts
cinder-home/harness/run.sh          # boots the real main.cpp against fake Sony services
bash cinder-home/tools/test_launcher.sh   # 46 cases over the escape ladder
(cd player && cargo test --release)
(cd installer && cargo test --release)
```

The linter is **pinned** (`shellcheck-py==0.11.0.1`) and CI runs the same scripts you do. A gate
whose version floats is a gate that can turn red with no source change — that has happened here.

`cinder-home/build.sh [stable|dev]` is the only gate that does the ARM link, the GLIBC ≤ 2.23
ceiling and the qemu preflight. **CI cannot do this** — it has no cross toolchain — so a green CI
says nothing about whether the thing links for the device. Run it before claiming a change builds.
It also runs the harness and the self-tests.

## The off-device harness

[`cinder-home/harness/`](cinder-home/harness/README.md) links the real `main.cpp` against faked
Sony service clients, a faked easel framework and a **virtual clock** — sleeping advances a counter
instead of waiting, so hours of device time cost milliseconds. It boots the app and asserts on the
resulting call trace.

If you fix a defect in the shell, add a scenario. It is the only automated thing that can see the
project's recurring defect class (work on a timer, for a condition that cannot change, that nobody
stops).

Be honest about its limits: it cannot see the ABI, the GLIBC ceiling, `dlopen`ed services, or
audio. `system`/`popen` are recording stubs, so every setuid helper "fails" — the **success** paths
of USB-MSC and power-off have no coverage there.

## What needs a device, and what that means

Plenty of this project cannot be verified off-device. That is expected, and the rule is simply to
**say so**. If a change is reasoned from call sites rather than observed on hardware, mark it
DEVICE-UNVERIFIED in the comment and add it to `docs/DEVICE_CHECKLIST.md`. Do not describe a
measurement you did not take.

Probe first: `cinder-probe` has no easel lifecycle and cannot affect boot. Nothing that can affect
boot should happen until a probe run looks clean.

Note `/contents` is mounted **noexec** — push a probe to `/tmp` (tmpfs, executable) and set
`LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/lib:/usr/lib`, or it will not run.

## Naming: intent, fact, and the difference between them

The project's recurring defect class is **process-lifetime thinking about service-lifetime state**
(`docs/SHORTCOMINGS.md` B1): code that configures a Sony service once and then assumes the
configuration is still there. It isn't, necessarily — services restart, other clients write the
same settings, links drop, and a value you set at boot can be gone by lunchtime.

Three verbs, and the distinction is load-bearing:

| Prefix | Direction | When it runs | The rule |
|---|---|---|---|
| `apply_*` | intent -> service | a user action | May assume it is being called *because* something changed. |
| `refresh_*` | service -> app | a poll or a listener event | Must never cache the answer as permanent. A service that said "no device" once will say something else later. |
| `reconcile_*` | assert intent still holds | a timer or an event | Must be **idempotent**, must re-read before deciding, and must not assume a previous `apply_` survived. |

Most B1 defects are an `apply_` that should have been a `reconcile_`, or a `refresh_` whose first
answer got cached. Two real examples, both shipped and both fixed:

* The Bluetooth peer name was read once and cached, so the screen listed headphones that had been
  switched off — and worse, the same flag was read as "we are connected", which disarmed the
  reconnect retry and meant the radio never reconnected on its own.
* `GetBtStatus` was believed to mean "connected". It reads `3` during connection *setup* as well as
  when linked, so the check fired early. The address, not the status, is the link.

If you are writing something that has to stay true rather than merely become true once, name it
`reconcile_` and give the off-device harness a scenario for it — that harness exists specifically
because this class of defect is invisible to static analysis.

## Style

Match the surrounding code. The tree is deliberately comment-dense, and the comments carry the
*why* — usually the incident that produced the rule. When you change behaviour that a comment
explains, update the comment in the same commit; a stale comment here is worse than none, because
these are load-bearing.

Record what you measured, not what you assume. "MEASURED 2026-08-26: …" is the house style, and
values recovered from a device should say so.

## Commits and pull requests

Conventional-ish subjects (`fix:`, `feat:`, `docs:`) and a body that says **why**. The history is
the only place some of this reasoning survives.

`main` does not currently require PRs — this is one person moving fast. If you are not that person,
open one.

## What not to do

* Do not guess vtable slot indices into Sony services.
* Do not write `/proc/regmon/<chip>/value`.
* Do not commit build byproducts. `*.unstripped` is gitignored; `dist/` is deliberate and stays.
* Do not add a settings option that cannot be undone from the device without a reboot.
* Do not reboot a test device casually — it boots to stock, and the user recovers by hand.
