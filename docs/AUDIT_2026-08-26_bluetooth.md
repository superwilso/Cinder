# Bluetooth audit — 2026-08-26

Triggered by: *"pairing a new device works but connecting is hit and more often miss"*,
*"BT is not working for me at the moment"*, *"nfc tap to pair/connect is not connecting
automatically"*.

Method: read every BT call path in `cinder-home/src/main.cpp`, then measure on device with
`cinder-probe --btwho` / `--btlink` and a full HCI capture (`--btlink hci on`, decoded with
`analysis/tools/btsnoop_decode.py`).

---

## Root cause: the app jammed its own connects

`SetConnectRetryMode(true, …)` (BtTransmitter slot 27) puts the service into a mode where **every**
connect request is refused. Measured, same address, minutes apart, capture running throughout:

```
retry OFF -> RequestConnection(AC:80:0A:56:A9:91) rc=1   CMD Create Connection -> AC:80:…:91
retry ON  -> RequestConnection(AC:80:0A:56:A9:91) rc=0   nothing on the air at all
retry OFF -> RequestLastDeviceConnection()        rc=1   LINK IN 1.53 s
```

**rc is accept/reject on this path — 1 = accepted.** The `Pairing` convention, not the
transaction-status-0 convention `GetConnectInformation` taught. `bt_request_connection` logged rc
and never read it, so every refusal was silent.

While armed, the service pages **`paired[0]` and nothing else**, on a **25 s** cycle (5 s page +
20 s gap; the earlier `interval + 10` reading came from too short a capture). 200 s of trace shows
eight service-driven pages to `paired[0]` and **zero** on-air effect from the app's three connect
calls in the same window.

`bt_reconnect_tick` armed the mode on the first tick after any drop — about a second after the
radio came up — and disarmed it only once a link existed, i.e. exactly when it was no longer
needed. The app's own log, while the user tapped a Devices row three times:

```
1417.857 bt-paired: row 0: RequestConnection rc=0        <- rejected
1418.575 bt: SetConnectRetryMode(true, 5, 20) rc=0
1420.631 bt-paired: row 0: RequestConnection rc=0        <- rejected
1421.592 bt: SetConnectRetryMode(true, 5, 20) rc=0
1421.770 bt-paired: row 0: RequestConnection rc=0        <- rejected
```

Each tap re-armed the ladder, which re-armed the jam. Pairing was unaffected because it goes
through a different service (`BtCommonService::Pairing`) — hence "pairing works, connecting
doesn't".

**Corollary:** the "the service holds no usable last-device record" theory in the previous round of
this code was wrong. The record was fine; the zero-arg call was jammed.

## Second defect: an idle radio is not a link

`GetBtStatus` reaches 3 with nothing on the other end — 0.61 s after powering an idle radio:

```
btwho: GetBtStatus=3  AvSrc=2  Avrcp=1
btwho: GetConnectInformation rc=0 addr=(none) name=''
```

`refresh_bt_route` used `st == 3` as the route test. Both halves were wrong, in one boot:

* with no peer, the rocker left the 3.5 mm jack, `IsSupportedAbsoluteVolume` was asked of a sink
  that did not exist, a volume nudge went out over AVRCP to nobody, and the reconnect-edge cascade
  (enhanced mode, volume listener, resync, `write_bt_pref`, `fx_cache_drop`, re-apply EQ, re-apply
  sound) ran against thin air;
* and when the real device connected later the route was **already** 1, so none of that cascade ran
  on the actual link — the exact case it was written for.

The address is the link, which `refresh_bt_connected` already knew.

## What was changed

| Fix | Where |
|---|---|
| Never arm the retry mode; disarm it before every connect; reconcile a stale one at startup | `bt_service_retry`, `bt_request_connection`, `apply_bt_toggle`, `bt_reconnect_tick` |
| Route on the peer address, not `GetBtStatus` | `refresh_bt_route` |
| Check the connect rc (1 = accepted) and say so in the log | `bt_request_connection`, `apply_bt_toggle` |
| Fall back to a named device on a REJECTED zero-arg, not after two blind tries | `bt_reconnect_tick` |
| Cap the ladder at `BT_RECONNECT_MAX_TRIES` (~35 min) — it capped its interval and never its count, so headphones in a drawer cost a page every 5 min forever | `bt_reconnect_tick` |
| Give the boot radio-restore a settle window (`g_bt_toggle_at`) — the one radio-power path without one | frame loop |
| `GetPairedDeviceInfo`: gate on the filled container, not rc alone | `refresh_bt_paired` |
| Put the scan switch back when the radio's 30 s search expires; stop a scan before connecting | `apply_bt_scan`, `bt_request_connection`, housekeeping |
| Clear `g_bt_pairing_addr` when a pairing never completes | paired-recheck give-up branch |

## Harness

Neither defect was catchable off-device, and that was itself a defect:

* slots 6 (`RequestConnection`) and 7 (`RequestLastDeviceConnection`) both pointed at **one**
  handler that always succeeded, so a targeted connect and a zero-arg one were indistinguishable
  and neither could fail;
* the retry mode was not modelled at all;
* the fake answered **2** for an idle radio, so `st == 3` was safe off-device and wrong on it.

All three fixed, plus three scenarios: `bt-no-self-jam`, `bt-stale-jam`, `bt-idle-not-link`.

## Verified on the new build

```
14.451 bt: radio was ON when the player was last used — restoring it
15.442 bt: RequestStartConnectWait() rc=1
15.442 bt-reconnect: link is down — connectable, first attempt in 10s
21.263 bt-vol: rocker now drives BLUETOOTH (GetBtStatus=3, peer named)
21.265 bt-vol: sink takes ABSOLUTE volume (SetCurrentVolume)
21.269 bt: resync volume after reconnect
21.317 bt: re-apply EQ after reconnect
21.423 bt: re-apply sound after reconnect
21.498 bt-reconnect: link is up again — ladder disarmed
```

Buds reconnected ~6 s after the radio came up, through connect-wait, before the ladder's first
attempt was even due. No `SetConnectRetryMode(true, …)` anywhere. The route moved only once a peer
was named, and the re-assert cascade ran on the real link.

## Still open

1. **`SetCurrentVolume REFUSED` on the connect edge** (line 21.266 above). The sink reports absolute
   volume support and then refuses the first push, ~2 ms after the link is named — AVRCP is
   probably not up yet at that instant. Self-corrects: the sink reports its own level 230 ms later
   and the UI adopts it. Newly visible, because this path previously only ever ran against a
   phantom link. Fix is to defer or drop the initial push.
2. **`OnNotifyPairingComplete` arguments are still undecoded**, so a FAILED pairing is
   indistinguishable from a successful one — it still ends the scan and starts a recheck that finds
   nothing. The fallout is now cleaned up, but the callback itself needs RE of
   `libBtCommonService.so`.
3. **`RequestStartConnectWait` has no getter**, so its state is a guess reconciled by one
   unconditional call per process. Unchanged; documented at the call site.
4. **One link at a time** by design — connect-wait is closed once a peer is linked.
5. **`reference_bt_radio_wedge`'s "3 = connected"** is wrong and has been corrected in memory: 3 is
   reached by an idle radio too.
