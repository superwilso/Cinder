# Bring-up that never completes froze the whole app

**Found 2026-08-24, off-device, by the first exploratory run of
[`cinder-home/harness/`](../cinder-home/harness/README.md).** One defect, two ordinary triggers, and a failure mode
worse than either trigger — followed by two more of the same shape, found the same way.

**The pattern, stated once:** work on a timer, for a condition that cannot change, that nobody
stops. Seven instances in one afternoon — the boot-animation re-kill, the bad-boot counter, the
power-off helper, the silent-input heartbeat, a wedged mass-storage session, and the
bring-up retry's own narration — all invisible to every gate this project had, all obvious in a
trace.

## The defect

`render_driver`'s frame loop deferred everything until the slow init finished:

```c
if (!g_deferred_done) {
    if (n < 30) { ++n; usleep(16000); continue; }   // warm-up paints first
    deferred_up();
    if (g_app) g_app->StopBootAnimation();          // re-kill in case init respawned it
    ++n; usleep(16000); continue;
}
```

That is correct reasoning for the case it was written against: `deferred_up()` completes in about a
second, so re-killing the boot animation on every frame and skipping the rest of the loop for
thirty frames costs nothing.

**But `deferred_up()` returns without completing on two ordinary failures**, and retries at 1 Hz
forever when it does:

* the library DB will not open — `cinder_db_open("/db/MTPDB.dat")` fails, so `g_db_ready` stays
  false and the function returns early;
* `cinder_audio_init()` fails — PlayerService is not up, or is wedged.

In either case `g_deferred_done` never becomes true, and **this block is the entire frame loop for
the life of the process.**

## What that costs

Measured with the harness (`no-db` and `no-audio`, 180 virtual seconds), counting the last minute:

| | healthy boot | DB will not open |
|---|---:|---:|
| `StopBootAnimation()` | 0 | **3,778** |
| housekeeping ticks (`cinder_get_screen_off_s`) | 61 | **0** |
| input pumped | yes | **never** |

`StopBootAnimation()` is a framework call into `libeaselcore`, and `CinderApp` logs every one of
them — `clog_` writes to `/contents/cinderhome.log` and **`fflush`es every line**. So the failure
mode is 62 framework calls and 62 flushed writes to the vfat partition, every second, forever.

Worse than the burn is what was frozen. Everything below the `continue` never ran:

* the **idle screen-off** — so the panel stayed lit;
* the **auto power-off** — added in the 2026-08-16 battery audit precisely because "a paused device
  with the screen dark ran until the battery was flat";
* the sleep timer, the battery gauge, the USB-host debounce, the automatic mass-storage entry;
* and `input_pump()`, which is gated on the same flag — so **no touch and no buttons at all**.

A Walkman that cannot open its library therefore sat with the screen on, ignoring its own power
button, writing to flash sixty times a second, until the battery was flat. The one thing it did do
is paint, so it looked alive.

## The fix

Bring-up gets a **grace window**, not the whole session (`DEFERRED_GRACE_MS = 10000`). Inside it,
nothing changes — a healthy boot clears `deferred_up` in about a second, an order of magnitude
inside the window, and `g_bringup_settled` is set in the same statement that sets `g_deferred_done`,
so input and the full loop start at exactly the moment they always did. Outside it, bring-up is
**stalled rather than in progress**: `deferred_up()` keeps retrying at its own 1 Hz from the top,
the boot-animation re-kill drops to once per 5 s (the same "dense early, rare later" shape as the
forced-repaint insurance above it), and the rest of the loop runs.

Re-enabling `input_pump` is deliberate rather than incidental. `screen_auto_off()` can only be
undone by an input event, so running housekeeping *without* input would blank the panel and leave
nothing able to wake it — strictly worse than the burn it fixes. Either both or neither.

The original gate's stated reason — "carry_out would drive uninitialised audio" — still holds in the
sense that those calls do nothing rather than something bad: `change_state()` returns `-1` with no
`g_ctrl`, and the library screens read an empty DB, which is the state `cinder-ui`'s host tests
cover most heavily.

**DEVICE-UNVERIFIED.** The throttle and the housekeeping arm are mechanical. The input arm is
reasoned from the call sites and the shims' null checks, not observed on hardware — a device session
should force the failure (rename `/db/MTPDB.dat`) and confirm the UI is usable, the panel sleeps,
and nothing crashes. That is item **2D.1** in [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md), where it
is flagged as the highest-risk item on the list.

## Regression test

`cinder-home/harness/run.sh stalled-bringup` boots the app with a DB that never opens and asserts
on the last minute: at most 20 `StopBootAnimation()` calls, housekeeping still evaluating the
screen-off and the sleep timer, the battery still reaching the status bar. It **fails on the
pre-fix code** (3,750 calls, zero housekeeping) and passes after, which is the only reason to
believe it is testing anything.

## Also measured, and clean

The same runs answered a question PR #4 had only reasoned about. Steady state, radio on, a
WH-1000XM4 linked, playing, counting one minute three minutes in:

| call | per minute |
|---|---:|
| `BtCommon::GetBtStatus` | 2 |
| `BtXmit::GetConnectInformation` | 2 |
| `BtXmit::GetSoundStatus` | 1 |
| `BtCommon::GetPairedDeviceInfo` | 0 |

**Five Sony IPC calls a minute (~0.08/s)** during Bluetooth playback, against the ~2.2/s the
2026-08-24 battery work started from. Per frame the loop makes four FFI calls
(`cinder_render_tick`, `cinder_get_bt_on`, `cinder_get_bt_route`, `cinder_get_usb_dac`) and nothing
else, and a forced full repaint runs once every 5 s as designed. That is the number the PR claimed,
now measured rather than argued.

## A second one, same shape, found the same way

A six-hour virtual session (the harness runs it in about a second) turned up a smaller version of
the same defect. `mark_healthy_maybe()` writes `/data/cinder/bootcount` to tell the launcher this
boot was good; if the file cannot be opened it logs a failure and retries on the next housekeeping
tick — every second, forever, with a line each time. **21,594 of that log's 21,700 lines were the
same sentence.**

The retry is right: `/data` can mount after the app has painted. The logging was not. It now writes
two lines instead — the first carrying `errno`, which is the whole diagnosis (`ENOENT` is "/data is
not mounted", `EROFS` is "it is, read-only"), and a second one a minute in saying what the failure
actually **costs**: with the counter uncleared the launcher treats this boot as bad and will
auto-revert to stock. That sentence matters and it was buried under eighty thousand copies a day of
the first one.

`cinder-home/harness/run.sh log-volume` now puts a ceiling on the whole class — under 300 log lines
across five steady-state hours, measured from an hour in so boot is still allowed to be chatty. It
reads 40 today, and 18,040 before this fix. Every line is an `fflush` to `/contents`, the same vfat
partition the user's music is on, so the count is a flash-write budget rather than a tidiness
preference.

## And a third, once the harness could fake the hardware

With a filesystem behind it — a battery, a charger state, a headphone jack — the harness can enable
features that were previously unreachable off-device. Auto power-off is one, and it turned up the
same defect a third time.

`power_action()` **only returns when the helper failed**; a successful shutdown never comes back. Both
automatic callers sit inside the ~1 Hz housekeeping tick, and once the idle threshold is met it
stays met — so a device whose `cinder-power` setuid bit is gone (twelve `chmod 4755` installs, one
of the things `SHORTCOMINGS.md` §D7 is about) **forked the helper and wrote three log lines every
second, for ever**. One virtual hour: 3,541 forks, 10,623 flushed lines. A `fork` + `exec` per
second on this SoC is a real power cost on its own, quite apart from the writes.

Both callers now back off five minutes after a returning attempt, and both the attempt announcement
and `power_action`'s failure lines stop after the third. It keeps trying — the helper could in
principle come back — it just stops shouting. The same hour now costs 12 forks and 9 lines.

Three scenarios pin the behaviour and the three guards that keep this out of somebody's hand:
`autooff-idle` (fires a minute after the last input, and backs off when the helper fails),
`autooff-playing` and `autooff-charging` (never fires).

## A fourth: the silent-input heartbeat

`input_pump()` logs "still ZERO events from every node" when the input system has produced nothing —
a genuine diagnostic for a real failure mode (a foreign grab, a dead driver). It was paced
`++calls % 450`, which is the same assumption the housekeeping block twenty lines below it had
already been explicitly fixed for: the loop runs at 60 Hz awake and 1 Hz with the panel dark, so
"every 450 calls" meant **every 7.5 seconds awake and every 7.5 minutes dark** — neither of them the
"~15 s" its own comment claimed. And input that is genuinely dead never starts working, so it
repeated for ever: **499 lines an hour**, each an `fflush` to `/contents`.

Now paced by the wall clock and backing off — 15 s, 1 min, 4 min, 16 min, then hourly. Four lines
in the first hour, one an hour after that. The condition it reports cannot change on its own, so
after the first few the only useful thing it can do is stop.

This is why `log-volume` runs with the panel **on**: at 60 Hz anything paced by a frame count shows
up at sixty times the rate it would in the dark, and that is what caught it.

## A fifth, and the worst-behaved: a wedged mass-storage session

`ensure_msc_lun()` binds the gadget's mass-storage LUN to `/emmc@contents` after `/contents` is
unmounted. When the LUN comes up empty — the "PC sees a reader with NO medium" case its own comments
are about — it runs a ladder of eight write-and-confirm attempts with settle sleeps, costing about
**two seconds**. And it is called from the ~1 Hz housekeeping tick for the whole session.

So on a device where the LUN cannot be backed, the render thread spent two seconds out of every one
inside that ladder: **an entire USB-MSC session with a UI that does not repaint and a Back button —
the only way out of the modal — sampled every other second**, plus a failure line every two seconds.
Now: the ladder backs off ten seconds after a failure, and the line stops after three. The fast path
(the LUN is already backed, which is every healthy session) is a single file read and is unchanged.

The `msc-cycle` scenario covers the whole round trip — cable in, handover, a wedged session, cable
out, release — and pins the retry count.

## A sixth and a seventh, from running everything broken at once

With the fixes above in, one more run: `cinder_audio_init` failing forever, no Bluetooth services,
auto power-off enabled, USB-DAC on — six virtual hours. Two things still repeated.

**`deferred_up`'s retry lines.** All three ("DB unavailable", "audio pump unavailable", "audio
unavailable") logged on every 1 Hz retry: **3,571 an hour, each an `fflush` to `/contents`.** The
retry is right — `/data` can mount late and a service can arrive late — the narration was not. The
pacing rule now lives in one place, `retry_log()`: say it immediately, then after a minute, then
roughly tripling to hourly. Prompt enough to diagnose from the log; a device stuck for a day costs a
couple of dozen lines instead of eighty thousand. 21,428 → 10 over six hours.

**The boot-animation re-kill, again.** The earlier fix throttled it to once per 5 s; it never
stopped, so a stalled bring-up still made a framework call and wrote a line every five seconds for
ever — 4,585 in six hours. The animation is dead within seconds and the only thing that could bring
it back is init respawning it, which the healthy path's straggler sweep already covers by ~30 s. It
is now bounded to the first minute. 4,585 → 284, all of them in that minute.

`stalled-bringup` asserts the quiet as well as the liveness: under 60 log lines across the last
fifty minutes of a stalled boot. It reads 3.

## Why nothing found this before

Every gate the project has was looking somewhere else. It is not a syntax error, not a warning, not
a wrong value, and not a rule a pure-logic self-test could hold: `deferred_up()` is individually
correct, the frame loop is individually correct, and the defect is in what the two do together over
three minutes when one of them does not finish. It needed something that runs the app and watches.
That is the whole argument for the harness, and this is the first thing it returned.
