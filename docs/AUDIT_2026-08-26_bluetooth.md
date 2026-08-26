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

## Follow-up fix: restoring the level across a connect

The corrected connect edge immediately exposed the next defect:

```
21.265 bt-vol: sink takes ABSOLUTE volume (SetCurrentVolume)
21.266 bt-vol: SetCurrentVolume REFUSED — falling back to a step
21.496 bt-vol: sink reports 39/127 -> UI level 39
```

The address is readable at the START of AVRCP coming up (`Avrcp` reaches 2 a beat later), so a push
1 ms after the address appears has nothing to travel over. It is refused; the sink then volunteers
its own level; the UI adopts that — and the user's saved level is gone. "Resume where you left off"
failed in the direction that looks like nothing happening, and could not be seen failing before,
because the edge it hung off fired when the radio powered up rather than when a device arrived.

Fixed by deferring and retrying: the level is captured at connect time (before the sink's report can
overwrite it) and pushed 1.2 s later, retried up to 4 times with a widening gap, then given up on —
leaving the sink's own level, which is what happened every time before. `bt_send_absolute_volume`
now owns the UI-steps → AVRCP 0..127 mapping so the rocker and the restore cannot disagree.

## Dual Bluetooth output: not possible on this hardware

Asked for as an advanced setting. It cannot be built — the stack is single-sink by construction.

**Sony's service layer is singular throughout.** Every method in
`libBtTransmitterService.so`'s prototype list takes no device and returns one:
`RequestDisconnection()`, `SetCurrentVolume(const uint8_t&)`, `GetSocketName(string&)` (ONE PCM
pipe), `GetSoundStatus(codec, freq, chan, bool)` (ONE negotiated codec),
`GetConnectInformation(addr, name)` (ONE peer). There is no device index or handle anywhere.

**The MTK layer below it is single-instance too.** In `libBtMw.so`:

* `btmwAvSrcGetInstance` / `btmwAvSrcSetInstance` — one instance, no index;
* `btmwAvSrcEncoderInit` / `btmwAvSrcEncoderDeinit` — **one encoder**;
* `btmwAvSrcDataPathLock` / `btmwAvSrcDataPathUnLock` — one data path, one lock;
* `btmwAvSrcIsConnectedBdAddr` disassembles to a single `memcmp(instance+8, addr, 6)` — one
  address in one struct, not a list walk.

Dual output needs two encoders and two streams. There is one of each.

**Two things that look like multipoint and are not:**

* `BtCommonService::DisconnectAll()` — the device can hold several *links* (A2DP, AVRCP, HFP, SPP,
  BLE), which is what this tears down. Not several A2DP sinks.
* `BtMwBtMulti*` in `libBtMw.so` (`Init`, `RequestConnect`, `RequestDisconnect`,
  `RequestSearchMode`, `RequestWaitMode`, `RequestSendAvrcpCommand`, `SetEirData`) — a complete,
  exported API that **no Sony service references** (`libBtMw.so` is the only file in the vendor tree
  that mentions it), with no audio or PCM entry point, and tied to the SINK role by
  `btmwAvSnkNotifyBtMultiStatusCallback`. It is the receiver-side connection manager, not dual
  source output.

**What IS reachable, and is the nearest useful thing:**
`BtCommonService::SwitchDeviceSession(const bool&)` — **client vtable slot 12**, already recovered
in `analysis/G_bt_nfc/vtable_BtCommonServiceClient.txt`, paired with the
`OnNotifyStartSwitchDevice` callback the listener already implements. This hands the audio session
from one paired device to another without a trip through the menus. One device at a time, but no
disconnect/reconnect cycle. Not built — offered.

Untested and separate: **jack + Bluetooth simultaneously**. `SoundServiceFw`'s constraint is on
duplicate track *types*, not coexistence, and `libaudiohal-dualtrackmixalsa.so` exists. That is a
different feature from two Bluetooth sinks and would need its own investigation.

## Follow-up: the ladder no longer asks while the radio is asking

`GetAvSrcConnectionStatus` (transmitter slot 3) has been read by the probe since 2026-07-30 and by
the shell never. Measured against a radio in each state, 2026-08-26:

| value | meaning |
|---|---|
| 0 | radio down |
| 1 | disconnected (what a deliberate `--btlink drop` leaves) |
| 2 | idle — up, nothing in flight |
| **3** | **connecting — a page is on the air** |
| 4 / 5 | connected |

The ladder now reads it and stands aside while an attempt is already in flight, bounded at 6 skips
so a status wedged at 3 cannot silence it. It is used for that one question only — 4/5 arrive before
`GetConnectInformation` has an address, and the address is still what decides the route.

## Follow-up: `OnNotifyPairingComplete`'s signature, recovered

Read by hand from `libBtCommonService.so`, the same way `OnNotifySearchedDevice` was, and anchored:
the same pass finds `cb44: ldr r4, [r1, #24]` for slot 6, the dispatch `RE_findings.md` already
documents verbatim.

```
c75e: ldr.w r0, [r9, #0x24]   ; the listener we registered
c762: sub.w r2, r7, #49       ; arg2 -> a single stack BYTE
c766: add   r3, sp, #16       ; arg3 -> a 48-byte struct
c768: ldr   r1, [r0]
c76a: ldr   r4, [r1, #16]     ; vtable[4] = OnNotifyPairingComplete
c76c: add   r1, sp, #72       ; arg1 -> a {begin,end,cap} byte vector
c76e: blx   r4
```

Unpack order above the call: `Get(1)` → one byte, stored at r7-49; `Get(4)` → a count; then a
`Get(1)` push_back loop of that many bytes into sp+72. So:

* **arg1** `const std::vector<uint8_t>&` — a byte string, address or link key;
* **arg2** `const uint8_t&` — **the result**, which is what was missing;
* **arg3** a 48-byte struct with a vector at +0, a vector at +16 and a `std::string` at +28 — the
  layout of `BtPairedDeviceInformation` (its destructor runs on sp+44, and sp+32 is freed). Taken as
  `const void*` and not dereferenced.

The polarity is NOT yet known: `btmw_ret_code_t` one layer down
(`bt::BtCommonComponentIfReceiver::NotifyPairingComplete(btmw_ret_code_t, btmw_bdaddr_t*,
btmw_linkkey_t*, unsigned, unsigned char, unsigned char*)`) would make 0 = OK, but pst may have
reduced it to a bool. So the byte is recorded and logged beside the ground truth the paired-list
recheck establishes by polling `GetPairedDeviceInfo`. One successful pairing and one failed one
settle it; then the gate is a one-line change.

## Follow-up: paired devices past the fourth could not be forgotten

Not a radio defect — a UI one, found sweeping the screens. Two different silent caps:

* `pairing.rs` `MAX_PAIRED = 4` — `devices.iter().take(4)` in the render, `paired.min(4)` in the hit
  test, and a header printing `PAIRED · 6` beside four rows;
* `bluetooth.rs` `PAIRED_SHOWN = 5` — a different number on the summary screen.

FORGET exists only on the Devices screen, and `DeleteLinkkey` is the only way to clear a link key,
which lives in the radio's table and nowhere else. So a fifth paired device was invisible,
unconnectable and **impossible to delete from anywhere in the app**. The user already has three.

The cap is now a WINDOW: four rows at a time with a MORE control that pages and wraps, header
reading `PAIRED · 1-4 OF 6`. No scroll offset — this screen positions its FOUND section from the
paired row count, and the one screen in this app whose render and hit test have drifted apart is the
one that grew a scroll offset. A page index is a single integer both sides derive from, via three
shared functions. Three tests: every device reachable on some page for n = 0..9, hit stops where the
last drawn row ends, and MORE only exists with a second page. The first fails on the old code at
n = 5. Previews `pairing_page1` / `pairing_page2` cover both pages.

The summary screen now names the count when it is showing only part of the list.

## Still open

1. **A connect request while one is already in flight returns rc=0.** Observed on the new build:
   `25.220 RequestLastDeviceConnection rc=1` then `45.317 … rc=0`, with the first attempt still
   paging. Benign — the ladder's fallback converges on the same device either way — but it means a
   refusal currently has two causes the code cannot tell apart, and only one of them was the jam.
2. **`OnNotifyPairingComplete` arguments are still undecoded**, so a FAILED pairing is
   indistinguishable from a successful one — it still ends the scan and starts a recheck that finds
   nothing. The fallout is now cleaned up, but the callback itself needs RE of
   `libBtCommonService.so`.
3. **`RequestStartConnectWait` has no getter**, so its state is a guess reconciled by one
   unconditional call per process. Unchanged; documented at the call site.
4. **One link at a time** by design — connect-wait is closed once a peer is linked.
5. **`reference_bt_radio_wedge`'s "3 = connected"** is wrong and has been corrected in memory: 3 is
   reached by an idle radio too.

## SetCurrentVolume is inert on this hardware, and each output now restores its own level

The connect-edge fix exposed a "SetCurrentVolume REFUSED" line, and the first theory — AVRCP not up
yet — was wrong. A deferred, four-times-retried push was written on that theory and was refused
every time over 12 s. Measured properly with `cinder-probe --btvollisten`, which registers a volume
listener and prints what the sink reports back:

```
4x SetVolumeUp / SetVolumeDown   ->  *a=43  *a=39  *a=43  *a=39     <- every step reported
3x SetCurrentVolume(39, 30, 39)  ->  nothing, from either listener  <- inert
```

Three absolute writes, one of which would have moved the sink nine units, produced no notification
and no audible change; four step calls produced four notifications. **The absolute-volume path is a
no-op on this firmware**, and all three capability bits that gate it lie
(`IsAvrcpTgVolumeSupported=1  GetControlAbsoluteVolume=1  IsSupportedAbsoluteVolume=1`).

`SetCurrentVolume` also returned **0 for all three**, including the real change — so it is not a
success flag and not the state-changed flag `SetConnectRetryMode` uses. It carries nothing.
`apply_bt_volume` gated its step fallback on `!= 0`, so that test was always false and the rocker
has only ever worked by always falling through to a step.

**What each output does now.** The two attenuators hold different numbers on purpose — 3.5 mm is the
CXD3778GF codec master, Bluetooth is the sink's own over AVRCP — and nothing tries to reconcile
them. What the route edge does is put each one back to where that output was last left:

* **Bluetooth** — the sink is WALKED to the target. Steps work and every step comes back as an
  `OnNotifyChangeVolume` carrying the sink's real level, so the loop closes on the notification
  rather than a return code: step toward the target, read where it landed, repeat. One measured step
  is 4 units of 127, so it stops within half a step (closer is unreachable, and insisting on an
  exact match would oscillate). Bounded at 24 steps, and it gives up early if the sink stops moving
  — a sink at its own ceiling answers every step with the same number.
* **3.5 mm** — `bt_resync_volume` on the DISCONNECT edge. It was running on the connect edge, which
  is the one moment the jack is not the output.

Verified on device: `bt-vol: Bluetooth was last at 39/127 — walking the sink back to it` /
`bt-vol: walked the sink to 39/127 (wanted 39) in 0 step(s)` — the zero-step case. The stepping
path needs the sink and the saved level to actually differ, which only happens when the headphones'
own buttons move it while the player is away.

## NFC: three things between a tap and a link

1. **Three IPC round trips before the decision.** The tap refreshed the paired list AND the
   connected peer unconditionally, reasoning that "the tap may be the first thing that has ever
   touched Bluetooth this session". That stopped being true once `deferred_up` seeded the pairing
   table at boot and the route poll started keeping the peer address fresh. The reads now happen
   only when the tapped address is UNRECOGNISED — the case where a stale "no" is expensive, because
   it turns a connect into a redundant re-pair.
2. **A tap on a second pair of headphones had nowhere to land.** The transmitter carries one sink at
   a time, so a connect aimed at device B while A is linked cannot work — and tapping your other
   headphones is exactly the gesture that hits it. It read as "NFC doesn't work" while the old pair
   kept playing. The tap now hangs up the current link first, without setting the user-disconnect
   latch (they asked for a different device, not for silence).
3. **Tap-to-pair waited out a poll before connecting.** `OnNotifyPairingComplete` fires before the
   link key is visible, so the connect was scheduled behind up to 300 ms of paired-list polling. The
   radio can now be asked instead: `RequestConnection` answers 1 for accepted, so the connect is
   attempted immediately and the poll stays as the fallback for a refusal.

Still gated on the radio: the NFC reader is only armed while Bluetooth is on
(`want_nfc = cinder_get_bt_on()`), so a tap with Bluetooth off does nothing at all. Sony's own
behaviour is that a tap powers the radio up. Changing that means running the reader whenever the app
is up, which costs continuous field polling on a device where idle drain is already the constraint —
a measurement, not a code change, and not yet made.
