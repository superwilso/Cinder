# Battery during Bluetooth playback — method, and the first finding

*2026-08-20. Written before any optimisation, because the last time this project optimised from a
guess ("the visualiser must be the expensive part") the guess was wrong by two orders of magnitude.*

## What is already done

* **The analyzer stops when the panel is dark.** `viz_analyzer_tick` gates on `g_screen_on`, so the
  per-frame IPC into Sony's `AudioAnalyzerService` is not paid with the screen off — which is how a
  Bluetooth session is normally spent.
* **The audio pump backs off when dark:** 20 ms lit → 250 ms dark (100 ms for a grace window after
  a transport press, so a button still feels immediate).
* **Idle draw was measured cable-out at 99.84% @ 598 MHz, 321 ctxt/s, cinder-home 0.65% of a core**
  (memory: `reference_power_measurement`). There is nothing left in the idle path.

## Measuring it — `tools/btpower.sh`

```
tools/btpower.sh start bt      # opening sample; then UNPLUG, play over Bluetooth, screen off
tools/btpower.sh report bt     # replug; closing sample + the deltas
```

Two one-shot samples of **cumulative** counters, so the sampling is the only intrusion and the
window is however long you left it. Run the same length three times — `bt`, `jack`, `idle` — and
compare; a single run tells you almost nothing.

Why it is built this way:

* **adb wakes the core.** The device sits at 598 MHz idle and reads 1.3 GHz the moment a shell
  attaches, and a cable pins the gauge to "charging" so the battery level says nothing. Hence:
  cable out for the window, and cumulative counters rather than instantaneous ones.
* **No daemon.** The first version backgrounded a sampler on the device with `nohup`; adb kills the
  process group when its shell exits, so the closing sample never ran and the file came back with
  an opening block and nothing else.
* **Process names come from `cmdline`, not `comm`.** Sony starts its services under `logwrapper` —
  30 processes on this device report the comm `(logwrapper)` — so matching on comm finds
  `cinder-home` and none of the audio or Bluetooth services.

It reports: CPU busy % and seconds, average clock and the time-in-state histogram, context
switches/s, per-process CPU for cinder-home and every `hagodaemon`, battery capacity/voltage (with
a %/hour extrapolation when it moved), which ALSA substreams were open, and a set of CXD3778GF
registers at both ends of the window.

## The first finding, and the open question

With **nothing playing, no PCM open, and nothing in the jack**, the codec is not asleep:

```
SYSTEM        0x03      OSC_ON     0x10     OSC_SEL   0x01     OSC_EN   0x10
BLK_ON0       0x0F      SD_ENABLE  0x05     PLUG_DET  0x10
PHV_L/PHV_R   0xD8      PHV_CTRL0  0x80     HPOUT2_CTRL1 0x0F  DNC1_START 0x50
```

Oscillators enabled, four block-enables set in `BLK_ON0`, the serial-data path enabled, both
headphone attenuators loaded and the DNC engine's start register non-zero — on a chip that has
nothing to render.

**The question this raises for Bluetooth:** LDAC audio never touches the CXD3778GF — it is encoded
by Sony's `BtTransmitterService` and leaves through the MTK radio (`analysis/E_usbdac_ldac/`). If
these same bits are still set during a Bluetooth session, the device is clocking and biasing a DAC,
a headphone amp and a noise-cancelling engine for an output nobody is listening to, for the whole
session. On a 3.5 mm session they are exactly right and must not be touched.

**Do not change a register before the A/B says so.** The order is:

1. `btpower.sh` runs for `idle`, `jack` and `bt`, same length, cable out.
2. Compare the codec block across the three. If `bt` matches `jack` rather than `idle`, the
   hypothesis holds and the size of the prize is the `jack`−`idle` CPU/voltage gap.
3. Only then look at what turns them off, and how Sony's own stack behaves when it switches routes
   — the driver may own these bits, in which case the lever is a route call, not a register poke.

**Never write `/proc/regmon/<chip>/value`** while chasing this. Selecting a register through
`target` is a read; writing `value` changes the audio hardware under the running player, and the
codec is the one part of this device with no software recovery path.


---

## 2026-08-26 — THE A/B, ANSWERED: the hypothesis does not hold

Measured on device with a live LDAC link (`GetBtStatus=3 AvSrc=4`, `GetConnectInformation` naming
`CMF Buds Pro 2`, `GetSoundStatus codec=0x02`) and real playback running (`pos=2000/304173`). No
ALSA PCM was open at any point, which is the expected shape: LDAC leaves through the radio, not the
CXD3778GF.

| Register | **idle, NO jack** | jack, idle | BT connected | BT **playing** |
|---|---|---|---|---|
| `SYSTEM` | `0x03` | `0x03` | `0x03` | `0x03` |
| `OSC_ON` / `OSC_EN` | `0x10` | `0x10` | `0x10` | `0x10` |
| `BLK_ON0` | `0x0F` | `0x0F` | `0x0F` | `0x0F` |
| `SD_ENABLE` | `0x05` | `0x05` | `0x05` | `0x05` |
| `DNC1_START` | `0x50` | `0x50` | `0x50` | `0x50` |
| `CODEC_PLAYVOL` | `0x00` | `0x00` | `0x33` | `0x33` |
| `PHV_L` / `PHV_R` | **`0xE4`** | `0xE4` | **`0x00`** | **`0x00`** |
| `HPOUT2_CTRL1` | **`0x0F`** | `0x0F` | **`0x00`** | **`0x00`** |
| `SMS_NS_PMUTE` | `0x80` | `0x00` | `0x00` | `0x00` |

**The Bluetooth-specific waste this file predicted is already being avoided.** The driver drops the
headphone output stage for a Bluetooth route: both attenuators go to zero and `HPOUT2_CTRL1` clears.
`bt` therefore does NOT match `jack` — it is strictly below it — which is the opposite of the
condition step 2 set for the hypothesis holding. A Bluetooth session is not biasing a headphone amp
for nobody.

**What IS still on is on in every state, including idle.** The oscillators, the four `BLK_ON0`
block-enables, the serial-data path and the DNC engine read identically on the jack, on Bluetooth
connected, and on Bluetooth playing — and the idle block at the top of this file, taken with nothing
playing and nothing in the jack, already had `BLK_ON0 0x0F`, `SD_ENABLE 0x05` and `DNC1_START 0x50`.
So those bits are not a Bluetooth cost. They are an **idle** cost, present the whole time the player
is awake, and that is where any remaining prize lives.

Connected and playing are indistinguishable at the codec, so nothing is gated on the stream itself.

**No register was written.** Every read went through `target` as a selector; `value` was only ever
read, per rule 2.

### The no-jack idle column, taken later the same day — and it makes the answer worse for idle

Captured with `h2w` state **0** (nothing in the jack), the radio **off** (`GetBtStatus=7`) and no
PCM open. It is identical to the jack column but for `SMS_NS_PMUTE`, which sets to `0x80` when
nothing is plugged.

**The headphone amplifier is biased with nothing plugged into the player at all.** `PHV_L`/`PHV_R`
sit at `0xE4` and `HPOUT2_CTRL1` at `0x0F` on an empty jack, exactly as they do with headphones in —
while a Bluetooth session has all three at zero.

So the ranking is the opposite of this file's original suspicion. At the codec, **Bluetooth is the
cheapest state the player has**: the driver powers the output stage down for it, and does not for an
idle device with an empty jack. The prize is not in a Bluetooth session; it is in idle, and it is
there whether or not anything is connected.

### What this does NOT settle

The power number. This is a register comparison, not a measurement of what those bits cost. The
`btpower.sh start` / unplug / listen / replug / `report` cycle still has to run for idle, jack and
bt to put a figure on the always-on block — and with the amp already being powered down, the honest
expectation for a Bluetooth-specific saving is now close to zero.

---

## 2026-08-23 — the sustained IPC of a steady session, and what was done about it

*Written after the CI/audit work of the same day, because one of its fixes is what made this safe.*

### What a connected session actually cost

Connected, playing, panel dark, in a pocket — the state a Bluetooth session is almost entirely
spent in — Cinder was making **~2.2 synchronous binder round trips per second, on the render
thread, for the whole session**:

| Call | Rate | What it is for | How often that can actually change |
|---|---:|---|---|
| `GetCurrentStatus` (uri) | 1.00/s | detect a track boundary | ~once per 3–4 minutes |
| `GetSoundStatus` | 0.50/s | the negotiated codec | once per link |
| `GetBtStatus` | 0.33/s | link up/down | on a link event |
| `GetConnectInformation` | 0.33/s | which peer | on a link event |

Every one of them already had an event-driven or free replacement in place.

### Why this is safe NOW and was not before

`OnNotifyBtStatus`, `OnNotifyAclStateChanged` and `OnNotifyDisconnectEnd` set `g_bt_state_dirty` the
moment the link moves. But **until 2026-08-23 that listener was registered only by `apply_bt_scan`**
— only if the user had opened Devices and pressed Scan. On an ordinary boot it was never registered
at all (`AUDIT_2026-08-23_three_reports.md` §3d), so the timer genuinely *was* the mechanism, and
3 s was the right number.

With the listener registered at boot, the timer is the safety net its own comment always claimed it
was — and can be relaxed.

### The one thing this must not break

**Pause-on-disconnect.** When headphones drop the music must stop promptly, and
`cinder_bt_should_pause` rides `refresh_bt_connected`, which the route poll calls. That stays
instant, because a listener event forces the poll on the very next frame regardless of the timer.

So the relaxed interval is **gated on `g_bt_listener_on` — the real registration result, not an
assumption that it worked.** If `AddListener` ever fails, every interval returns to its old value,
because there the timer is once again the only thing that can notice a dropped link. The rule is in
`src/bt_poll.h` with a host self-test whose most important cases are exactly those.

### The changes

* **Route poll** 3 s → **30 s** while a peer is named and the listener is up. Unchanged at 3 s while
  connecting or searching (where latency is the point) and 15 s with the radio down.
* **Peer re-read** (`GetConnectInformation`) 2 s → **30 s** on the same condition. Unchanged while
  the name is still missing, which is the case that retry was written for.
* **Codec poll** 2 s → **60 s** on the same condition. Its own comment already said the answer
  "changes on the order of once a session", and its two event bypasses are untouched.
* **The URI round trip is gated on a free signal.** A track boundary is always visible in two
  numbers already in hand — the duration changes, and/or the position jumps backwards — and both
  are atomic loads the PlayEventListener pushed (`cinder_audio_position` does no IPC at all). So the
  round trip is now spent when something *might* have changed, with a 30 s backstop for the one
  case the free signals cannot see: two consecutive tracks of exactly the same length whose reset
  also lands between two samples.

Arithmetic: **~2.2 IPC/s → ~0.07 IPC/s** in a steady session. Over three hours that is roughly
23,000 round trips down to about 750.

### What is NOT claimed

**This has not been measured.** It is a reduction in work that is arithmetically certain and a
battery saving that is not — each round trip is cheap, and during playback `hagodaemon` is awake
decoding anyway, so the marginal cost of waking it is lower than the raw count suggests. Run
`tools/btpower.sh` before and after with the same window length before putting a number on it.

**The big prize is still the open question at the top of this file** — whether the CXD3778GF is
still clocked and biased through a Bluetooth session, for an output nobody is listening to. That
needs the device, and the A/B is specified above. Nothing here touches it.
