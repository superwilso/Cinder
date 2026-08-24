# Plan — reaching the Bluetooth stack below Sony's services

*Rewritten 2026-08-19 from measurement. The 2026-08-18 version of this file proposed sniffing
`/tmp/bt.app.gap` as Route A, then `/proc/btmtk`, then raw HCI on `/dev/stpbt`. **Route A as written
is impossible and Route C is unnecessary** — both for the same reason, which is that the transport
was never actually looked at. What follows is what the device says.*

## The topology, measured

```
cinder-home ── pst IPC ──> hagodaemon (PID 354, uid system)
                             hosts BtCommonService, BtTransmitterService, BtPlayerService, BtBle*
                             links libBtMw.so  ──>  libbluetooth.blueangel.so (the MTK CLIENT lib)
                             binds /tmp/bt.app.gap (fd 7) and /tmp/bt.ext.adp (fd 9)
                                     │  AF_UNIX SOCK_DGRAM, both directions
                                     ▼
                           mtkbt (PID 146, uid system)
                             binds /tmp/bt.int.adp (fd 3) and /tmp/bt.a2dp.stream (fd 4)
                                     │
                                     ▼
                              /dev/stpbt  (HCI transport, MTK combo chip)
```

Everything above is `system:system`, and cinder-home runs as uid 100 = `system`. That part of the
old plan holds: **no setuid helper is needed anywhere in this area.**

### Why "sniff the socket" cannot work

`/tmp/bt.app.gap` is **SOCK_DGRAM**, not SOCK_STREAM (`connect()` as STREAM and SEQPACKET both
return `EPROTOTYPE`; `/proc/net/unix` shows type `0002` for all four `bt.*` sockets). A datagram
socket delivers each message to exactly one receiver — whoever the sender addressed. Connecting a
second client to `/tmp/bt.app.gap` succeeds and receives **nothing, ever**: it is hagodaemon's
inbox, and mtkbt addresses hagodaemon. Measured with `analysis/G_bt_nfc/btsniff.c`, which connected
cleanly and read 0 bytes.

The names are not negotiable either. `libbluetooth.blueangel.so` binds fixed paths per profile
(`bind gap app socket failed` is one of its error strings), and hagodaemon already holds the two
that matter. The per-profile family it can bind is visible in the binary — `/tmp/bt.ext.adp.a2dp`,
`.avrcp`, `.hfp`, `.spp`, `.hid`, `.gattc`, `.gatts`, `.l2cap`, `.pan`, `.map`, … — and none of
those are bound today, which is the one door left standing if a future profile (SPP, HID, GATT) is
ever wanted. **For observing the link, none of it is needed.**

## What replaced it: three tiers, all reachable, all cheap

The stack is not silent. It has a full command surface at every level, and the level Cinder uses is
the *thinnest* one.

| tier | how to call it | what it adds |
|---|---|---|
| `pst` services (what Cinder uses today) | `BtCommonServiceClient` / `BtTransmitterServiceClient` vtable slots | `GetRssi` (slot 25) + `OnNotifyRssi` (listener slot 11), `SetHciLogEnabled` (26), `SetConnectRetryMode` (xmit 27), `RequestStartConnectWait` (xmit 10) |
| `libBtMw.so` (Sony's wrapper, 508 exports) | link it, plain C symbols | `BtMwCommonRequestGetRssi`, `BtMwCommonSetHciLogEnabled`, `BtMwCommonSetStackLogEnabled`, `BtMwCommonGetBtVersion`, `BtMwCommonRequestGetCoexBtWifiRatio` |
| `libbluetooth.blueangel.so` (MTK client, 1354 exports) | link it — but the socket names are taken | `btmtk_gap_get_rssi`, `btmtk_gap_send_hci` + `btmtk_gap_hci_cmd_cnf`, `btmtk_gattc_read_remote_rssi`, `btmtk_config_hci_logging` |

**The headline: `btmtk_gap_send_hci` exists.** Arbitrary HCI, serialised by the stack that owns the
transport, with the completion delivered back as a confirmation — which is exactly what Route C
wanted from `/dev/stpbt` and could not get safely. Route C is therefore withdrawn: a second reader
on `/dev/stpbt` would steal events from a live stack for a capability the stack already exports.

## HCI tracing works, today, from one call — CONFIRMED

`BtCommonServiceClient::SetHciLogEnabled(const bool&)` is client vtable **slot 26**. Measured
2026-08-19 (`cinder-probe --btlink hci on`):

```
[cinder-probe] btlink: SetHciLogEnabled(true)
[cinder-probe] btlink:   /tmp/hci_sniffer_log_20260819091929.cfa  16 bytes
```

mtkbt opens `/tmp/hci_sniffer_log_<YYYYMMDDhhmmss>.cfa` the moment it is switched on (the format
name and the `btsnoop` string both live in the mtkbt binary). **This is the failure channel this
project has spent a year not having** — every "the middleware has no error path, judge by side
effects" note in `reference_bt_radio_wedge` is now optional rather than forced. Pull the file with
`adb pull` and read it against the HCI spec; status codes on every command.

Cost of leaving it on: a growing file in a tmpfs. Switch it off (`--btlink hci off`) after a
capture, and never ship it enabled by default.

## The two calls Cinder never made — and what they measured

Both are on `BtTransmitterServiceClient`, both were in the recovered vtable, and grep says nothing
in this project has ever called either (the sink-side `BtPlayerService` equivalents are used by
`--btrx`, which is a different service).

### `SetConnectRetryMode(const bool&, const uint32_t&, const uint32_t&)` — slot 27

The service has a `ConnectRetryWorkThread(const uint32_t&, const uint32_t&)` of its own; the two
u32s are its interval and count. **Measured 2026-08-19, on a device where it had never been
touched:**

```
btlink: SetConnectRetryMode(true, 5, 10) rc=1   GetConnectRetryMode 0 -> 1
btlink: t+ 0.26s  avsrc=1 -> 3        <- connecting
btlink: t+ 5.06s  avsrc=3 -> 2        <- gave up, back to idle
btlink: t+15.14s  avsrc=2 -> 3        <- and again, on the 5 s interval we asked for
btlink: t+20.19s  avsrc=3 -> 2
```

So **it defaults to OFF (0), it accepts being switched on, and the service then retries the
connection itself on the interval given.** That matters because cinder-home's own reconnect ladder
(`BT_RECONNECT_FIRST_S 10`, doubling to `BT_RECONNECT_MAX_S 300`) is a UI-thread timer doing badly
what a service thread will do properly: at the top of that curve, headphones switched on at the
wrong moment wait up to five minutes.

The HCI trace pins the arguments down further than the prototype does:

```
 0.015 CMD 0x0405 Create Connection        -> 00:00:5E:00:53:02   (the LAST device, service's choice)
 5.014 CMD 0x0408 Create Connection Cancel                        <- exactly `interval` later
 5.016 EVT 0x03   Connection Complete      status=PAGE TIMEOUT
 5.021 CMD 0x0c1a Write Scan Enable        0x02 (page scan)
15.032 CMD 0x0405 Create Connection        -> 00:00:5E:00:53:02
```

#### The count has a floor of 5, and it is enforced SILENTLY (verified 2026-08-19)

Re-running the call across a range of counts, clearing the mode between each so the readback means
something:

| `count` | rc | `GetConnectRetryMode` | paging on the air |
|---|---|---|---|
| 1 | 0 | 0 → 0 | none |
| 3 | 0 | 0 → 0 | none |
| 4 | 0 | 0 → 0 | none |
| 5 | 1 | 0 → **1** | yes |
| 7 | 1 | 0 → **1** | yes |
| 10 | 1 | 0 → **1** | yes |
| 20 | 1 | 0 → **1** | yes |

**Below 5 the call is a no-op** — it returns, changes nothing, and logs nothing. The floor is on the
count by itself, not on its relation to the interval: `(interval 10, count 7)` is accepted. Cinder
ships `BT_SVC_RETRY_COUNT 20`, comfortably above it, and there is now a `static_assert` on that
constant so a later "let's make it less aggressive" edit cannot silently switch the feature off.

**`rc` is a state-CHANGED flag, not a success code.** Arming a mode that is already armed also
returns 0 — the same value a rejected count returns. This cost a wrong conclusion during the check:
the first `(5, 20)` run showed `rc=0, 1 -> 1` and read as a failure when it was a no-op on a mode
still armed from the previous run. **Only the readback distinguishes the two**, so never judge this
call by its return value.

**The mode is sticky and outlives the process.** Set it from `cinder-probe` and a second, unrelated
run reads it back as 1. So an app that starts with a cached "off" can be wrong from its first
instruction, and an early-return on that cache would then never disarm a radio that is still paging
every ~15 s. `bt_service_retry()` reconciles its cache against `GetConnectRetryMode` (slot 28) on
first use for exactly this reason.

**`interval` is the page timeout allowed per attempt, not the gap between attempts** — the gap adds
a further ~10 s, so the cycle is `interval + 10` and `count` is that many cycles. `status=0x02` on
every attempt is a page timeout, which is the right answer for headphones that are switched off:
the negative control this measurement needed.

### `RequestStartConnectWait()` — slot 10

Meant to make the transmitter accept a link the headphones initiate when they power on. Cinder has
never called it, so every reconnect to date has had to be one we initiated. It returns rc=0 and
does not throw (re-verified 2026-08-19), and `RequestStopConnectWait()` (slot 11) closes the window
again.

**What the trace says it is not:** the controller already sits at `Write Scan Enable 0x02` (page
scan on, inquiry scan off) between connection attempts, so the baseband is connectable with or
without this call. Whatever it changes is above the baseband — the A2DP/AVRCP accept path.

**Still untested:** whether a headphone switched on during the window actually lands. The 90 s
window run on 2026-08-19 saw no incoming link, but the headphones' state was unconfirmed, so that
is an untested case rather than a negative result. Re-run `cinder-probe --btlink wait 90 keep` with
a headphone deliberately powered on inside the window.

## What ships from this

1. **Reconnect** (`bt_reconnect_tick`) — **DONE 2026-08-19.** `bt_service_retry()` and
   `bt_connect_wait()` are armed when the radio comes up and again on the first notice of a drop,
   and torn down on a link, on a deliberate disconnect, and when the radio goes off. The old
   exponential ladder stays as the backstop for after the service's count runs out.
2. **A real link-quality line** on the Bluetooth screen: `GetRssi()` + `OnNotifyRssi`, which is one
   listener slot Cinder already declares and never reads.
3. **HCI capture as a dev-channel diagnostic**, off by default, for the next "it just didn't
   connect" report.

## What this plan still does NOT promise

* **Peer battery level.** Settled: AVRCP carries *our* battery outward, never theirs. Nothing found
  at any of the three tiers changes this. Do not re-investigate.
* **A better codec than LDAC.** The codec set is the chip's.
* **`/proc/btmtk` and `/proc/btcvsd/debug`.** Still root-only, still empty on read. With HCI
  tracing available through a supported call, they are no longer worth poking.

## Order of work

1. ~~`--btsniff` on `/tmp/bt.app.gap`~~ — done, and the answer is that it cannot work (DGRAM).
   `analysis/G_bt_nfc/btsniff.c` stays as the negative control.
2. `--btlink status | last | wait | retry | hci | rssi | drop` and `--nfctap` — the measurement
   tools. **Done.** `analysis/tools/btsnoop_decode.py` reads the capture.
3. Wire retry mode + connect-wait into cinder-home's reconnect path, then re-measure the
   power-on-to-audio time against the old ladder.
4. `GetRssi` with a device connected, then the UI line.
5. Findings go into `analysis/G_bt_nfc/RE_findings.md`, next to the listener work.
