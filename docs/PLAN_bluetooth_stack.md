# Plan — reaching the Bluetooth stack directly

*2026-08-18. The one item from `AUDIT_2026-08-18_device_vs_sony.md` deliberately NOT built: it is a
reverse-engineering session, not an afternoon, and it should start from a decision rather than from
enthusiasm. This is the brief for that decision.*

## Why bother — what Sony's layer costs us

Every Bluetooth limitation this project has recorded is a limitation of Sony's **presentation
layer**, not of the radio:

| what we want | what Sony's services offer | what the radio has |
|---|---|---|
| link quality / "is this connection actually good" | nothing | `Read RSSI`, `Read Link Quality`, `Read AFH Channel Map` |
| what the peer really supports | a **5-state enum** | `Read Remote Extended Features`, `Read Remote Version` |
| the codec actually negotiated | `BtSoundCodec`, Sony's own enum, set-only in practice | the A2DP capability exchange itself |
| peer battery | **impossible** (`reference_bt_no_peer_battery`) | still impossible — AVRCP carries it the other way. *This item does not change.* |
| why a connect failed | nothing; the MTK stack logs nothing at all | HCI status codes on every command |
| transmit power | nothing | `Read Transmit Power Level` |

That last row is the honest headline: **the stack is silent, so today every BT failure is diagnosed
by side effect** (`reference_bt_radio_wedge` — "judge by side effects"). HCI would replace guessing
with status codes.

## What the device actually gives us — measured, not assumed

```
/dev/stpbt      crw-rw---- system system  192,0    raw MTK BT HCI transport
/dev/stpwmtA    crw-rw---- system system  200,0    combo-chip control (BT/WiFi/GPS power)
/dev/wmtdetect  crw-rw---- system system  154,0
/tmp/bt.app.gap  srwx------ system system           GAP: discovery, connection, link state
/tmp/bt.int.adp  srwx------ system system           adapter protocol, internal
/tmp/bt.ext.adp  srwx------ system system           adapter protocol, external
/tmp/bt.a2dp.stream srwx--- system system           the A2DP PCM pipe (already known + used)
```

**Everything is `system:system`, and cinder-home runs as uid 100 = `system`.** No setuid helper is
needed for any of it — unusual for this project, and it removes the component/install work that
`cinder-fm` needed.

Sony also ships the tooling: `/system/bin/hci_cmd` (usage: `hci_cmd XX XX XX` or `-f FILE`, talks to
`/tmp/hcicmd_socket`), `/system/bin/btut` (633 KB, MTK's BT test harness) and `/system/bin/bt_drv`.
`/proc/btmtk` and `/proc/btcvsd/debug` exist but are `root`-only and read empty.

## The hard part, stated plainly

**Sony's stack owns the transport.** `/tmp/hcicmd_socket` does not exist while the stack is up, so
`hci_cmd` is not a drop-in — it is a *reference implementation of the framing*, nothing more. And a
second reader on `/dev/stpbt` is not obviously safe: HCI is a shared command/event channel with
sequence expectations, and injecting commands under a live stack can desync it.

So there are three routes, and they are not equally sensible.

## Route A — observe the sockets (START HERE)

**Do this first, and possibly only this.** Connect to `/tmp/bt.app.gap` (or attach to the traffic)
during a known-good connect and simply *watch*. No writes, no injection, nothing the stack can
notice.

* Cost: an afternoon.
* Risk: essentially none — a reader that never writes.
* Yield: the frame shape of the MTK adapter protocol, and quite possibly link state and RSSI
  already flowing past as events. That alone may be the whole feature.

Concretely:
1. `cinder-probe --btsniff`: `connect(AF_UNIX, "/tmp/bt.app.gap")`, dump every byte with timestamps.
2. Trigger known events, one at a time, and label the capture: radio on, scan, connect a known
   headphone, play, change volume, disconnect.
3. Diff the captures. Anything that changes only across the connect is connection state; anything
   that ticks continuously while connected is a candidate for RSSI/quality.

**Gate:** if the socket refuses a second connection (likely — a stream socket with one server and
one client), fall to Route B before spending more time.

## Route B — the DEBUG surfaces, with root

`/proc/btmtk` and `/proc/btcvsd/debug` are `root`-only and empty on read, which usually means "write
a command, then read the answer". `btut` almost certainly drives exactly these. Read `btut`'s
strings for the command vocabulary before poking anything.

* Cost: a day.
* Risk: low-to-moderate — writes to a debug node while the stack is live.
* Yield: unknown, possibly everything Route A wanted, possibly nothing.

## Route C — speak HCI on /dev/stpbt

Only if A and B both fail, and only with a plan for the stack desyncing.

* The read-only HCI commands are the goal: `Read RSSI` (0x1405), `Read Link Quality` (0x1403),
  `Read Transmit Power Level` (0x0C2D), `Read Remote Version` (0x041D).
* Every one needs a **connection handle**, which means either observing one go past (Route A) or
  asking the stack for it — so A is a prerequisite in practice, not an alternative.
* **The real hazard is not the command, it is the response.** HCI events are delivered once, to
  whoever reads first. A second reader on `/dev/stpbt` will *steal events from Sony's stack*, and a
  stack that misses the completion of its own command is a stack that has quietly wedged — which on
  this device looks like nothing at all, because it logs nothing.

If Route C happens: do it with BT otherwise idle, one command, with a reboot planned regardless.

## What would make this worth shipping

A "connection quality" line on the Bluetooth screen — real RSSI and link quality for the connected
device, updated once a second — plus honest codec reporting. That is a genuinely better screen than
Sony's, from data Sony collected and did not show.

## What this plan explicitly does NOT promise

* **Peer battery level.** Settled and unchanged: the radio reports *our* battery outward via AVRCP;
  it cannot read theirs. Route A will not change this and nobody should re-investigate it.
* **A better codec than LDAC.** The codec set is the chip's.
* **Fixing the wedge in `reference_bt_radio_wedge`** — that is a Sony-service state problem, and
  reading HCI does not write it.

## Order of work

1. `cinder-probe --btsniff` against `/tmp/bt.app.gap`, read-only, with labelled captures. **Stop and
   reassess here** — this is the gate.
2. If the socket is single-client: `strings btut`, map the debug vocabulary, try `/proc/btmtk`.
3. Only then consider `/dev/stpbt`, BT idle, one command, reboot expected.
4. Whatever is learned goes into `analysis/G_bt_nfc/RE_findings.md` next to the existing listener
   work, not into a new file.
