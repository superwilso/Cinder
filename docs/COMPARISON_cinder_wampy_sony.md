# How Cinder, Wampy and Sony's stock player each do it

Written 2026-07-28, from the wampy source at `artifacts/repos/wampy`, the extracted stock rootfs at
`analysis/binwalk/6.bin/_6.bin.extracted/ext-root`, and the on-device discovery dump at
`artifacts/cinder_discovery.txt`. Where a claim comes from wampy's own documentation rather than
from code I read, it says so — wampy's notes are first-hand device work and worth more than a guess,
but they are still someone else's measurement.

The point of this file is not a feature scoreboard. It is: **where does Cinder differ, and is the
difference deliberate?** Three of the differences found here were bugs, and they are fixed in
`e3d5612`.

---

## The one structural difference everything else follows from

| | Approach |
|---|---|
| **Sony** | `HgrmMediaPlayerApp` — a Qt 5.3 app, the `type:Home` app in `.appcfg`, talking to ~30 services hosted in `hagodaemon` processes over `pst` binder IPC. |
| **Wampy** | **Runs alongside Sony's app and drives it.** GLFW + OpenGL + ImGui skinned as Winamp; reaches Sony's services partly by `LD_PRELOAD`-ing the stock app and partly through its own `pstserver` helper. Sony's app stays installed and running. |
| **Cinder** | **Replaces** the Home app entirely. Sony's Qt player never starts. Software raster to `/dev/graphics/fb0`, own IPC shims. |

That choice explains most of the rest. Wampy can borrow whatever the stock app has already set up;
Cinder has to set up everything itself — and *that* is the shape of the bug found in the analyzer
below. Wampy inherits Sony's configuration by standing next to it. Cinder inherits nothing.

---

## Where Cinder was wrong

### The spectrum analyzer — Cinder never told it what to analyse

`AudioAnalyzerService` needs its **passbands** set. Sony's player sets twelve
(50, 100, 160, 250, 500, 750, 1000, 2000, 4000, 8000, 16000, 28000 Hz), each with a `mean` of 456 —
406 for the topmost — and the service **caps at twelve**. Wampy derived this by `LD_PRELOAD`-ing the
stock app and reading the calls it made (`MAKING_OF_VIS.md`); the `SetPassband` symbol and the
`{int; float}` `Passband` layout are confirmed independently against Sony's own
`libAudioAnalyzerServiceClient.so`.

Cinder set the mode, the update rate and the calc-samples, then called `Start`. It never set the
passbands. **That is very likely the whole reason the visualiser has never been seen to produce a
frame** — and it is exactly the shape you would predict from the structural difference above: when
Wampy connects, Sony's app has already configured the service. Nothing had configured it for Cinder.

Two more defects in the same path, both from the data being different than assumed:

- **The values are logarithmic and Cinder mapped them linearly.** Sony reports raw amplitudes
  "ranging from 40k to millions" — three decades inside one frame. Dividing by the peak and taking a
  `sqrt` left everything but the loudest band or two in the bottom tenth of the display. It is a
  real dB mapping now. Sony's player converts to sound pressure levels for the same reason.
- **Twelve bands were bucket-averaged into 36 bars**, giving three identical bars per band: a
  staircase of twelve wide steps claiming to be 36 bars. It interpolates now.

Wampy asks for 30 Hz updates; Cinder asks for 20. Either is fine — that one is a preference.

### `cinder-gpunode` — a setuid TOCTOU, and a binary that should not have shipped

It called `lstat()` and then `chmod()`, with a comment claiming the `lstat` rejected a planted
symlink. It did not: `chmod()` resolves the path again and follows symlinks. `/dev` is root-owned so
it was not reachable in practice, but that is not the standard setuid code should be held to. Now
`O_PATH|O_NOFOLLOW` + `fstat` + a chmod through `/proc/self/fd`, so the check and the change are
bound to one inode.

Separately: it was shipping on **both** channels. It is setuid-root and its entire job is to make
four kernel graphics nodes world-writable, in service of a GPU present path that is default OFF and
measured **4.7× slower** than the software one. It is dev-only now.

---

## Where Cinder was right, and it is worth knowing why

### Volume

Wampy's `MAKING_OF_VOLUME_TABLES.md` is a long investigation into how region affects sound. The
conclusion that matters here: the perceptual curve lives in **region-selected DAC gain tables**
(`ov_1291`, `ov_1290_cew`, …) applied by the `cxd3778gf` driver *below* the mixer. So writing the
raw `master volume` step 0..120 — which is what Cinder does — is exactly what the stock player does,
and the curve is applied for us. **No change needed.** Worth recording, because "our volume is
linear and Sony's is not" looks like a bug until you know where the table lives.

### The Framework pump

Wampy's `pstserver` drives `pst::core::Framework`'s event looper on a thread. Cinder now does the
same, and had to: without it every service out-param stays uninitialised stack. Cinder reached this
independently (2026-07-27) and painfully; wampy's code confirms it is the intended pattern, not a
workaround.

### The audio output device

Both use `hw:0,4` (`cxd3778gf-icx-lowpower`), the low-power S-Master path. Confirmed on device for
Cinder on 2026-07-27.

---

## Where Cinder does more

| | |
|---|---|
| **Library** | Cinder reads `/db/MTPDB.dat` directly (`cinder-db`), so it owns sort order, the Albums accordion, playlists and the A–Z jump rail. Wampy structurally cannot — it drives Sony's app and never owns the list. |
| **Boot safety** | Cinder's escape ladder is five rungs (cable-at-boot, `cinderhome_off`/`_clear` over MSC, the bad-boot counter, Settings ▸ Boot to stock) with a 24-case offline test matrix. Wampy has a bad-boot counter. Cinder replaces the Home app, so it needs more. |
| **Art** | Persistent thumbnail cache on ext4 `/data`, baked gradient fallbacks, 16-bit and palette PNG support. |
| **Scrobbler** | Built in, rather than a separate daemon. |

## Where Wampy does more

| | |
|---|---|
| **FM radio** | Fully working, including recording. Cinder's FM screen is a static `88.6`. See below. |
| **EQ** | Per-song settings and filter work (`MAKING_OF_EQUALIZER_*`), well past Cinder's 10-band. |
| **Bluetooth** | Cinder has none wired at all. |
| **Device breadth** | A30/A50/ZX300/WM1A/Z/DMP-Z1. Cinder targets NW-A55/A50 only. |

---

## FM radio: 3.5 mm as antenna, Bluetooth as output

**Architecturally yes, and every link in the chain is evidenced.** Nothing of it is built.

The hardware is a **Silicon Labs Si4708**, loaded by
`insmod /system/lib/modules/radio-si4708icx.ko` from `/bin/load_sony_driver` (with
`deemphasis=750` for the UC and LA regions). The module's symbol set is **`video_*` only** —
`video_register_device`, `video_ioctl2` — and contains **no ALSA symbol whatsoever**. So it is a
pure V4L2 control device (`/dev/radio0`): it tunes, it mutes, and it never carries audio.

The audio is analog, and it goes into the codec:

```
Si4708 (analog L/R out)
      ↓
CXD3778GF  'analog input device' mux  ── item #1 is literally 'tuner'
      ↓
codec ADC
      ↓
hw:0,1  ( /dev/snd/pcmC0D1c — a real capture PCM )
      ↓
[ PCM in the SoC — this is where it becomes ours ]
      ↓
BtTransmitterService  (SetLdac / SetLdacSoundQuality / NotifyOpenAudio)
      ↓
Bluetooth headphones
```

`'analog input device'` and its `tuner` item come from the on-device `amixer contents` dump.
`pcmC0D1c` comes from the `/dev/snd/` listing in the same dump — card 0, device 1, **capture**,
i.e. `cxd3778gf-standard` is full duplex. And wampy **records FM to a file on real hardware** by
opening exactly `hw:0,1` (`src/rec/rec.cpp`), which proves the capture leg works rather than merely
existing.

**The antenna is the cable, not the audio path.** That is the Si470x reference design and it is why
stock requires headphones for FM. So a 3.5 mm cable plugged in purely as an antenna — a bare
extension lead, nothing on the far end — is a perfectly good antenna while the audio leaves over
Bluetooth. Nothing about the antenna cares where the audio goes.

**This is the same problem Cinder already solved once.** `ldac-bridge` is "capture a PCM source →
LDAC socket". FM is that with a different capture device. The existing capture scanner deliberately
*skips* card 0 (it is hunting the USB-DAC gadget card), so an FM mode is a flag, not a rewrite.

### Three traps wampy hit first, which are worth inheriting

1. **A Sony service turns the mux back off.** On a power-button press the stock player sets a timer,
   and on expiry something flips `'analog input device'` from `tuner` to `off`, killing the radio a
   few seconds later. Wampy's fix is to poll the control once a second and re-assert it. Cinder
   replaces the stock player, so this *may* not fire for us at all — but it must be verified, not
   assumed.
2. **ALSA control events cannot be watched.** `poll`/`epoll` on `/dev/snd/controlC0` reports nothing
   on this device because the communication is `ioctl`-based; `amixer sevents` and `alsactl`'s
   `monitor.c` simply do not work here. Wampy says it lost an embarrassing amount of time on this.
   Polling is the only option.
3. **Power state kills it.** On power press the stock player sets the system power state to `mem`,
   which stops the radio dead. Wampy holds `/sys/power/wake_lock` (writing a lock name, and the same
   name to `wake_unlock` to release).

Wampy's enable sequence, verbatim from `src/util/util.cpp`:

```sh
amixer cset name='analog input device' 1     # 1 = tuner
amixer cset name='analog playback mute' 0
amixer cset name='headphone amp' 0
echo <lockname> > /sys/power/wake_lock
```

For **BT output specifically** that sequence wants one change: `'analog playback mute'` is the
*analog bypass to the headphone amp*. Leave it muted, and leave `'headphone amp'` off, so nothing
leaks to the jack and the amp costs nothing — the audio is taken digitally from `hw:0,1` instead.
That is also the answer to "does the antenna cable have to be silent": yes, and deliberately so.

### What it would actually take

1. Wire `Screen::Fm` at all (it currently draws a hardcoded `88.6` and has no `tap()` branch).
   `TunerPlayerService` is fully RE-able: `SetFrequency`, `GetSignalLevel`, `StartAutoTuning`,
   `SetStereoMode`, `GetStereoState`, `SetMuteMode`, plus RDS via `OnReceivedPs`. Full 76–108 MHz.
2. The mixer + wake-lock sequence above, plus the 1 Hz re-assert poll if the timer turns out to fire.
3. Point `ldac-bridge` at `hw:0,1`.
4. **Which needs B4 from the readiness list — the `BtTransmitterService` shim, which does not exist
   yet.** FM→BT cannot ship before FM→wired or before Bluetooth works at all, so it belongs after
   the headline USB-DAC→LDAC feature, not before it: they share the same missing piece.

Honest expectation on quality: FM → ADC → LDAC adds an encode and a radio hop to an already
band-limited source. For radio that is fine. It will use more power than stock FM, which keeps the
whole path analog — so it should be a mode you choose, not the default.
