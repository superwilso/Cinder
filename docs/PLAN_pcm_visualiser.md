# Plan — the visualiser from the decoded audio, not Sony's analyzer

*Written 2026-09-29, after a read-only look at the player while it played. Steps 2–4 and the
Bands half of step 5 are built and host-tested the same day; the device run is
[`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §24.*

> **Built:** `cinder-ffi/src/pcm_tap.rs` (find, parse, choose the slot by timestamp, `pread`),
> `spectrum::from_pcm` (2048-point FFT, log bands 40 Hz–16 kHz by energy), the switch-over in
> `lib.rs` (`viz_tap`, `tap_fresh`, `cinder_viz_wants_analyzer`), and Settings ▸ Visualiser ▸
> Bands. **Measured before:** Sony's analyzer used 160 CPU ticks in 20 s (7.9% of one of the
> two cores) with the visualiser on screen. **Open:** `TAP_LEAD_MS` is 0 until §24.2 measures
> it. **2026-10-04:** the tap now returns both channels, and five styles draw from the samples —
> Scope, Stereo field, Spectrogram, Meters and Radial (`cinder-ui/src/viz.rs`, `cinder-ffi/src/vizsig.rs`;
> checklist §26).

## Why

The visualiser gets its levels from Sony's `AudioAnalyzerService`. That service is a bank of IIR
bandpass filters, not an FFT, and it has a hard limit of **12 bands**
(`analysis/` notes; `cinder_analyzer.h`). Every column past 12 is interpolation. Alternating two
band tables to reach 24 was measured and does not work. The analyzer also runs as a separate
process, and Cinder fetches every frame from it over binder.

## What was found

PlayerService hands its decoded audio to SoundServiceFw through two files in `/dev/shm`:

| File | Size | What it holds |
|---|---|---|
| `MappedShmHolderTK_MUSIC_PID_<pid>_PKT_131072_QUE_5_1` | 216 B | Ring state: counters that step once per packet |
| `…_packet` | 657,200 B | 5 slots, each a 368-byte header and room for 131,072 bytes of audio |

Each slot, as read on 2026-09-29 while a 16-bit, 44.1 kHz FLAC played:

| Offset | Value | Meaning |
|---|---|---|
| +4 | 44100 | Sample rate |
| +8 | 16 | Bits |
| +12 | 2 | Channels |
| +240 | 130,127,526 … | **Timestamp in µs.** It steps by exactly 46,441 µs, which is 2048 ÷ 44,100 s |
| +248 | 1.0 (float) | Unknown, probably gain or speed |
| +364 | 8192 | Bytes of audio in this slot: 2,048 stereo frames |
| +368 | … | Interleaved 16-bit little-endian PCM |

Both files belong to the `system` user with mode 0600. cinder-home runs as `system`, so **it can
read them, with no preload, no Wampy and no change to Sony's files.** The audio is decoded but not
yet processed: it comes before EQ, DSEE and volume, so the visualiser would no longer change with
the volume.

The other tap, the mono shim inside SoundServiceFw, only loads where Wampy is installed, because it
borrows Wampy's preload. This player has no Wampy. The shm queue needs neither.

## What it would give

- **Any number of bands:** 24, 32, 48 or 64 real log-spaced bands from one FFT, instead of 12 real
  bands interpolated.
- **Real peaks, and both channels.** That makes new visualisers possible: an oscilloscope, a
  left/right level meter with true peak, a stereo-width scope and a spectrogram.
- **Cheaper, and nothing in the path.** A 2048-point real FFT is a few tens of thousands of
  multiply-adds, well under a millisecond per frame on the Cortex-A7. It runs only while the
  visualiser is on screen. It replaces a second process filtering every sample, plus a binder round
  trip per frame. *To be measured, not assumed:* the analyzer's CPU with the visualiser on screen,
  from `/proc/<pid>/stat`, before and after.

## Limits

- **Library playback only.** The queue is `TK_MUSIC`. FM, USB-DAC input and the Bluetooth receiver
  do not go through it. For those, keep Sony's analyzer as the fallback, so nothing that works today
  stops working.
- **About 230 ms ahead.** Five slots of 46 ms are queued ahead of the DAC. Draw from the slot whose
  timestamp matches the playback position, or the newest slot SoundServiceFw has taken, plus a fixed
  output delay found on the device.
- **Formats not yet seen:** 24-bit, hi-res rates, DSD, and what happens across a gapless track
  change. Anything not recognised as PCM falls back to the analyzer.

## Safety rules

1. **Read with `pread`, never `mmap`.** If PlayerService shrank or recreated the file under a
   mapping, the next read would be `SIGBUS`, and cinder-home would die. On this device, cinder-home
   dying leaves the player with no Home app.
2. **Never write** to either file, and never hold it open across a PlayerService restart. The name
   carries PlayerService's PID. Find it with a glob, and open it again when the name changes.
3. **A torn read is harmless.** One frame of mixed old and new samples just draws one odd frame. No
   lock is taken on Sony's ring.
4. The work runs on the UI thread's existing visualiser tick, not in any Sony thread.

## Order

| # | What | Effort | Gate |
|---|---|---|---|
| 1 | **Probe.** Add `cinder-probe --pcmtap`: log the ring counters and each slot's header at 50 Hz for 10 s, across a pause, a seek, a track change, a 24-bit file, a hi-res file and a DSD file. Pin which counter is "taken by SoundServiceFw", and the output delay | One device session | The ring's meaning is written down with real lines |
| 2 | **`pcm_tap.rs`** in `cinder-ffi`: find the files, pick the slot, `pread` it, and hand back a frame of samples. Host-tested against files built from the real header layout (lines copied from the probe) | ½ day | Tests: slot choice, format refusal, a missing or renamed file, a short file |
| 3 | **FFT and bands:** a real FFT with a Hann window, log-spaced bands (count from Settings), and the same dB scale and fall as `spectrum.rs` today, so the look carries over | ½–1 day | Tests: a sine lands in its band; silence is flat; band count 12–64 |
| 4 | **Switch-over:** use the tap when it has fresh PCM, and Sony's analyzer otherwise. Start the analyzer only when the tap has nothing | ½ day | Host test of the choice. On the device: FM still shows a visualiser |
| 5 | **Settings ▸ Visualiser ▸ Bands** (12 / 24 / 32 / 48 / 64), and a new "Scope" visualiser type drawn from the raw samples | ½ day | Golden previews; overflow audit |
| 6 | **Device run:** a new `DEVICE_CHECKLIST.md` section. Sync by eye on a drum track, CPU before and after, FM fallback, and a 24-bit file | One session | Checklist PASS |

Steps 2–5 need no device. Step 1 needs the player, playing, with adb.
