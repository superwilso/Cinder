# Audit — three reported defects (2026-08-23)

Three reports, audited against the tree at `bff3172`:

1. **Library not auto-updating / new albums never appear.**
2. **No plain (non-shuffled) Play on a playlist; picking one song plays only that song.**
3. **Bluetooth not connecting to the WH-1000XM4; NFC not pairing or connecting on its own.**

All three are real. Report 2 and report 3 are **fully diagnosed and fixed here**. Report 1 has
**two** causes: the detection half is fixed here; the trigger half is a genuine missing feature that
is device-gated and must not be guessed at from the host — see §1.

Everything below cites the code it is talking about. Host gates after these changes: **398 Rust
tests** across the workspace (up from 395), the 8-case UI overflow matrix, and a new 10-case host
self-test for the library-change rule.

---

## 1. The library never picks up new albums

### 1a. THE ROOT CAUSE — nothing ever asks for a re-scan (still open, device-gated)

`/db/MTPDB.dat` is **Sony's** MediaStore database, written by `MediaStoreService` inside
`hagodaemon`. Cinder only ever **reads** it: `cinder-db` opens it `SQLITE_OPEN_READ_ONLY`
(`player/cinder-db/src/lib.rs:110`) — that is Path A of `analysis/H_mediastore/RE_findings.md`, and
Path B (the `MediaStoreClient` IPC) was never wired. Grepping the whole tree for a MediaStore call
site returns nothing but documentation.

The thing that used to ask for a scan when music arrived was **the stock Qt app that Cinder
replaces**. `cinder-home/deploy/install_cinderhome.sh` repoints appmgr's `HgrmMediaPlayerApp.appcfg`
at Cinder, and Cinder never took over that responsibility. The USB-MSC handoff makes it worse, not
better: `cinder-msc` does the unmount, the gadget switch and the remount itself
(`cinder-home/src/cinder-msc.c`), so no Sony service is in the loop at the moment the volume
changes underneath it.

Net effect: copy an album on over USB-MSC and the database on the device does not know about it, so
no amount of reloading on Cinder's side can show it.

**Why this is not fixed here.** Closing it means driving `libMediaStoreServiceClient.so` — a vtable
slot and an argument shape that have not been recovered. This project has already rebooted the
device twice by sending a wrong-shaped payload into a core service (STATUS.md, the 2026-08-11 BT
handshake), and MediaStore is exactly that class of service. The RE has to happen on device.

**The device-session plan**, in safety-gradient order:

1. `cinder-probe`, read-only: `dlopen("libMediaStoreServiceClient.so")`, resolve
   `_ZN3pst8services…MediaStoreServiceClientFactory14CreateInstanceEv`, dump the client vtable the
   way `analysis/G_bt_nfc/vtable_BtCommonServiceClient.txt` was dumped. **Call nothing yet.**
2. Watch the stock app do it: boot stock, `strace -f` the `hagodaemon` hosting MediaStoreService
   across a USB-MSC disconnect, and record what the app sends and what touches `/db/MTPDB.dat`.
   That names the verb without guessing.
3. Only then wire it, behind `run_guarded`, on the MSC-exit path.

Step 2 also settles a question worth knowing regardless: whether the scan is app-driven at all, or
whether the service watches the mount and Cinder's direct `mount(2)` is simply invisible to it. If
it is the latter, the fix is in `cinder-msc`, not in an IPC call.

### 1b. FIXED — the change watcher could not see a scan that did happen

Even when the database *is* rewritten (an adb push during dev, a scan Sony re-runs on its own
schedule), the watcher could miss it. `cinder-home/src/main.cpp` compared **`st_mtime` on the main
file alone**, and that is the one stamp a SQLite writer can leave untouched across a commit:

* in **WAL** mode the pages land in `MTPDB.dat-wal` and the main file is only rewritten at a
  checkpoint — possibly never while the writer is up;
* in rollback-journal mode `MTPDB.dat-journal` appears and vanishes around the write, and at a
  10-second poll that is often the only visible evidence;
* the store's filesystem has **2-second mtime granularity**, so two writes in one tick are one
  mtime.

The rule now lives in **`cinder-home/src/db_sig.h`** and covers all three files, and for each one
mtime **and** size **and** inode. It is header-only and free of project dependencies so it can be
exercised on the host — the same treatment `bt_edge.h` and `jack_edge.h` get, for the same reason:
it decides something the user sees. `cinder-home/tools/dbsig_selftest.cpp` pins ten cases,
including the two the old check failed (a `-wal` appearing or growing with the DB's mtime frozen,
and a same-mtime rewrite of a different size) and the two it must never get wrong (an unreadable
`/db` reads as *unknown*, never as a change — a failed `stat()` must not trigger a full library
rebuild). `build.sh` runs it alongside the other self-tests.

`exit_usb_msc` now re-seeds the signature after its own reload, so the next poll does not repeat
the ~3,500-track rebuild it just did.

---

## 2. Playlists: no plain Play, and a track plays only its album

Two distinct defects on one screen, both fixed.

### 2a. Picking a track played its ALBUM, not the playlist

Every "play this" in the UI funnelled through `Action::PlayIndex(object_id)`, and the shell
resolved that id through **`Db::album_context`** (`player/cinder-ffi/src/lib.rs`). An object id
carries exactly one context — the album it belongs to — so tapping track 4 of a 60-track playlist
built a sequence of *track 4's album* and stopped there. On a playlist of singles, or one where each
album contributes one track, that is precisely the report: **one song, then silence.**
`tap_playlist` even recorded the row index (`self.playlist_track_idx = i`) and then threw the
context away.

The context has to travel **with** the tap, so there is now
`Action::PlayPlaylistAt(playlist_id, index)` (`player/cinder-ui/src/nav.rs`), handled in
`carry_action` on the same pending-play channel `PlayPlaylist` already used — the members become the
sequence, starting at the tapped row. Both entry points use it: the touch path and the
hardware-button `Select` path.

The queue-replace prompt sat in the middle of that funnel and would have downgraded the action back
to a bare object id, so `pending_song: Option<i64>` is now `pending_play: Option<Action>` and the
prompt replays the action it was actually holding. A test pins that.

### 2b. There was no non-shuffled Play at all

`Screen::Playlist` had a single accent band, and it was **`ShufflePlaylist`**. The Album page has a
"Play album" band (`library::album_play_band`); the playlist page had no equivalent, so a curated
list could not be played in the order it was curated in — the one thing a playlist is for.

The band is now **split**: PLAY on the left (62%), SHUFFLE on the right. Split rather than stacked
deliberately — `playlist_content_top`, the edit bar and the whole UI-overflow matrix all derive from
that one rect, so nothing below the band moves. `playlist_play_band()` / `playlist_shuffle_band()`
are the single source for both the renderer and the hit test, and a test asserts the two halves
tile the original band exactly: adjacent, covering, and neither answering for the other.

Rendered and eyeballed in both themes via `cargo run -p cinder-host` (`out/playlist_page_*.png`);
the overflow matrix passes unchanged.

### Related, NOT changed — flagged for a decision

The same "an object id only knows its album" limitation still applies to the **Artist page** and the
**Songs tab**: tapping a track there plays that track's album, not the artist's discography or the
sorted song list. That is arguably the intended design for those screens (the comment at
`nav.rs`'s tab-row handler says as much), and it is not what was reported, so it is left alone. If
it should change, `PlayPlaylistAt` is the pattern to copy.

---

## 3. Bluetooth and NFC

**Second pass (2026-08-23, after "I think the BT bugs run deeper" — correct).** The first pass found
two defects; a full sweep of the subsystem found a third that is *upstream of both* and explains the
symptom on its own. All five are fixed. Read §3.0 first — it subsumes §3a in the bad case.

Five defects, in causal order: the switch never re-read the radio (§3.0), the pairing table was
never seeded (§3a), the NFC reader disarmed itself (§3b), the radio's own notification listener was
never registered (§3d), and the connect-wait cache guessed at service state (§3e). §3f is a pacing
correction with no user-visible symptom.

### 3.0. THE DEEP ONE — the shell decided once, at boot, whether Bluetooth existed

`cinder_set_bt_on` has **exactly one call site in the entire shell**: a single, un-retried
`GetBtStatus` read in `deferred_up`, on the boot path, inside a `run_guarded` that swallows failure.

```c
int st = bt_status();
cinder_set_bt_on(bt_radio_up(st) ? 1 : 0);
```

`bt_status()` returns **-1** when `BtCommonServiceClient` cannot be constructed and **0** for the
service's own "unknown" — both of which a `hagodaemon` still coming up produces — and
`bt_radio_up()` calls both of those OFF. **Nothing ever asked again.** `refresh_bt_route()` has
re-read that very same `GetBtStatus` every 3 seconds for the whole session and used the answer only
to point the volume rocker; the app could sit there *knowing* the radio was up while
`cinder_get_bt_on()` said it was off.

And that flag is not passive. Everything downstream reads it:

* **`bt_reconnect_tick`** takes its "radio off" branch — which does not merely return, it calls
  `bt_connect_wait(false)` and `bt_service_retry(false, false)`, i.e. actively tells the radio to
  **stop its own retry** and **stop accepting incoming links**. The player does not just fail to
  connect; it disables the two mechanisms that would have connected it.
* **The NFC reader is never armed** — `want_nfc = cinder_get_bt_on() != 0`. §3b's pacing fix does
  nothing while this flag is false.
* **The Settings switch shows Bluetooth off** while the radio is on.

So one unlucky boot read disables Bluetooth *and* NFC for the entire session, with no way back
except the user noticing and toggling the switch by hand. It would present exactly as reported, and
it would look intermittent — because it is a race.

`deferred_up` already has a retry ladder for the DB for precisely this class of race ("a transient
mount or PlayerService race must not permanently leave the app without a library or playback. Retry
only the failed prerequisite, paced at 1 Hz"). The radio never got one.

**Fix:** `refresh_bt_route()` now reconciles the switch from the status it was already reading. The
decision is in **`cinder-home/src/bt_switch.h`**, host-tested by `tools/btswitch_selftest.cpp`
(24 cases), and it is deliberately asymmetric:

* **Only 2 / 3 / 7 are evidence.** `0` and `-1` are exactly what the failing boot read produced, so
  acting on them is the bug itself — they decide nothing and do not even advance the tally.
* **UP wins on one read.** A status of 2 or 3 is positive proof; there is nothing to wait for.
* **OFF needs three consecutive known-off reads** (~9 s). "Not up yet" and "off" are
  indistinguishable in one sample.
* **A user toggle buys a 6 s settle window.** `apply_bt_toggle` deliberately stops waiting for the
  radio after ~0.9 s (a longer wait freezes the UI, which read as "the switch doesn't work"), so the
  next poll can honestly still see "not up" on a switch just turned ON — and reconciling then would
  slam it back under the finger that moved it. The tally still accrues while settling, so the truth
  lands on the first read after the window rather than restarting the count.

On a correction to ON it also clears the reconnect schedule and reads the pairing table, because
everything the "off" branch tore down has to be reachable again.

The policy, stated once: **the radio is the truth, not the switch.** Outside the settle window the
switch is what moves, in either direction — including a switch the user turned OFF against a radio
that stayed up, because `bt_set_rf(false)` returning is not evidence the radio went down. Same rule
the project already applies to the mixer.

### 3a. Auto-reconnect did nothing for a whole boot — `g_bt_paired` was never seeded

`bt_reconnect_tick` bails early on `if (g_bt_paired.empty()) return;` — reasonably, since "nothing
to reconnect to" is not a failure. But **nothing populated `g_bt_paired` at startup.** Every
`refresh_bt_paired()` call site was reactive: opening Settings ▸ Bluetooth ▸ Devices
(`CINDER_ACT_BT_PAIRED_REFRESH`), an NFC tap, a connect/forget row, or a pairing completing.
`deferred_up` read the radio status, the route and the connected peer — but not the pairing table.

So after a reboot the list was empty, the tick returned on its first line every second, and **two**
things never happened:

* `RequestLastDeviceConnection` — the player never reached for the headphones; and
* `bt_connect_wait(true)` (`RequestStartConnectWait`) — the player was never made to *accept* the
  headphones reaching for it, which is how a WH-1000XM4 normally lands when you power it on.

The device therefore sat there doing neither until the user happened to open the Devices screen.
There is a second, visible consequence. `refresh_bt_paired` is also what pushes the list into the
UI (`cinder_bt_paired_add`), and the **Bluetooth screen's own device rows** read that list —
`BtHit::PairedRow(i)` indexes it directly. The only thing that emitted `BtPairedRefresh` was
*entering the Devices screen* (`BtHit::Pair`). So after a reboot the Bluetooth screen showed **no
paired devices at all** until the user drilled one level deeper and came back.

That is the report exactly. Fix: `deferred_up` now calls `refresh_bt_paired()` beside the reads it
already does. With a non-empty table the tick's first pass sees "radio up, nothing linked, something
paired", arms the radio's own retry and the connect-wait, and runs the backoff ladder after that —
all of which was already written and simply unreachable.

### 3b. NFC tap-to-pair switched itself off ~80 ms into every boot

The reader is armed from the render loop, bounded to five attempts so a permanently-missing
`libNfcService` cannot re-run three IPC calls per frame forever. The comment says five is "enough to
ride out a service that is still coming up at boot".

**It is not, because that block is not the 1 Hz housekeeping.** Brace-depth check on
`render_driver`: the NFC block sits at depth 2, inside `if (g_deferred_done)`, in the **per-frame**
half of the loop — the 1 Hz `if (g_house_due || house_now - last_house_ms >= 1000)` block does not
open until ~130 lines later. The screen is on at boot, so the loop runs at ~60 Hz and **five
unpaced attempts are spent in about 80 milliseconds** of the first frame after deferred init — long
before `NfcService` is answering. It then logged

```
nfc: reader would not start — tap-to-pair off for this session
```

and that was true for the rest of the session: `g_nfc_arm_tries` is only reset by turning the radio
**off**. Tap-to-pair was dead on essentially every boot, which is why taps did nothing and the only
audio you got was the headphones' own auto-reconnect — the same misreading the 2026-08-17 round
already corrected once at the dispatch layer (`nfc_service_tap`), one level below where it was
actually failing.

Fix: gate the retry on the **wall clock** as the comment always assumed — ten attempts, one every
two seconds, ~20 s of patience for the same three calls the count exists to limit.

### 3d. The radio's notification listener was never registered on an ordinary boot

`bt_listener_register()` had **one** call site: inside `apply_bt_scan()`, and only when the user
starts a device scan. On any boot where nobody opened Devices and pressed Scan, `CinderBtListener`
was never registered at all.

That listener is what sets `g_bt_state_dirty` from `OnNotifyBtStatus`, `OnNotifyAclStateChanged` and
`OnNotifyDisconnectEnd`. The render loop's fast path is written around it, and says so:

> …and the RADIO NOW TELLS US, so the poll is a safety net rather than the mechanism. […] this reads
> the route on the very next frame instead of up to 3 s (or 15 s from a radio that last read down)
> later. That latency is the whole of why stock felt faster to connect.

In the ordinary case that path was **dead**, and every link change waited out the poll — up to 15 s
when the radio last read down, which is exactly the state a reconnect starts from. Fix:
`deferred_up` registers it. Registration is idempotent and permanent by design, `apply_bt_scan`
still calls it, and it is safe outside a scan (`OnNotifySearchedDevice` only fires while the radio
is actually searching).

### 3e. The connect-wait cache guessed at state that lives in the service

`bt_connect_wait` early-returns when the requested state matches `g_bt_connect_wait_on`, which
starts `false` — a **guess** about state that lives in `BtTransmitterService`, not in us. The
function directly above it, `bt_service_retry`, documents why that guess is unsafe and reconciles
against a getter:

> The mode is SERVICE state, and it is STICKY — it outlives this process (proved by setting it from
> cinder-probe and reading it back from a second run).

Connect-wait is the same kind of state with no getter to reconcile against. A Cinder that went away
— or was respawned by the launcher's crash supervisor — with the service still in connect-wait would
early-return on its first `bt_connect_wait(false)` and leave it armed for good, letting a second
device walk in behind the one that is playing, which is the whole thing the call exists to prevent.
Fix: the first call of a process always transmits. One extra IPC per boot.

### 3f. Also corrected — `bt_reconnect_tick` ran ~60x a second

Same misplacement, no user-visible symptom: the function's header says "runs from the 1 Hz
housekeeping" and its call site is in the per-frame half. The backoff ladder is wall-clock so it
behaved correctly, but every frame re-ran its state tests and, on the first pass after a drop, two
IPC calls on the render thread. Now gated to 1 Hz, matching what it documents.

---

## What is verified, and what is not

| Change | Verified how |
|---|---|
| Playlist play/shuffle band, playlist-context playback | 398 host tests (3 new), UI overflow matrix, host PNG render in both themes |
| Library change rule (`db_sig.h`) | 10-case host self-test, wired into `build.sh` |
| Switch reconcile in the live poll | Confirm on device: `bt: switch was OFF but GetBtStatus=2 — reconciled to ON` should appear within ~3 s on a boot that lost the race, and should appear on NO boot that did not |
| Switch/radio reconcile rule (`bt_switch.h`) | 24-case host self-test, wired into `build.sh` |
| `refresh_bt_paired()` at boot, listener registration, connect-wait first call | **Not host-testable** — Sony IPC. Reasoning is from the call-site audit above; confirm on device by `bt-paired: N device(s)` and `bt-scan: AddListener rc=0` appearing in `deferred_up`, and a link coming up after a reboot with no screen touched |
| NFC retry pacing | **Not host-testable.** Confirm on device: `nfc: Start(1) rc=0 mode=1` should now appear within ~20 s of boot instead of the "off for this session" line |
| MediaStore re-scan trigger | **Not implemented** — see §1a for the device-session plan |

The C++ changes could not be compiled here: `cinder-home/build.sh` needs the glibc-2.23 xenial
sysroot and the libc++ 3.9.0 headers, neither of which is in this environment. They are
comment-and-guard-level changes to existing call sites plus one header-only helper that **is**
compiled and tested on the host, but the cross-build gate has not been run — do that before the
next flash.
