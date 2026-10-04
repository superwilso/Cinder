# Soundscapes — procedural ambient sound, over the music or on its own

*Written 2026-10-04. Built and host-tested the same day; the device run is
[`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §25.*

Menu ▸ **Soundscapes** plays a background sound like Apple's Background Sounds: rain, a storm, a
beach, a stream, wind, a fire, a night, and white, pink and dark noise. It plays over the music or
on its own, with **one level for each**. A soundscape is a bed under a song and the whole of what
you hear when nothing else plays, so it needs two different levels.

## Nothing loops

None of the sounds is a recording. Each one is made as it plays (`cinder-home/src/soundscape.h`)
from noise and short events. The events are rain drops and patter, bubbles whose pitch rises,
crackles and pops, cricket chirps, waves that swell, break and wash, gusts and distant thunder. Each
event's timing, pitch, loudness and place come from a xorshift128 generator seeded once per session,
with a period of 2^128 − 1. Slow random drifts sit on top: how hard it rains, how big the next wave
is, how strong the wind blows.

The self-test checks this in two ways rather than assuming it:

- It searches every sound's envelope for a repeating period between 2 and 60 s. The tested signal is
  how far any lag stands above its neighbours, so a slow drift is not mistaken for a loop.
- It searches a full minute of raw audio for any repeat of a one-second stretch, by FFT
  cross-correlation. A loop matches itself at about 1.0; the worst sound matches at under 0.5.

| Sound | Made from |
|---|---|
| White noise | Flat spectrum (measured slope −0.1 dB/octave) |
| Pink noise | Paul Kellet's three-pole filter, poles moved to the engine rate (−3.2 dB/octave) |
| Dark noise | A leaky integrator, −6 dB/octave above 18 Hz, with a DC block (−6.2 dB/octave) |
| Rain | A band-limited bed whose intensity wanders, a dense patter of noise bursts (300/s) and pitched drops (1.7–6 kHz resonators) |
| Storm | Heavier, brighter rain, plus distant thunder every 20–70 s: low-passed brown noise with a slow attack, a long decay and a rolling modulation. Distance sets its brightness and loudness |
| Beach | A low sea roar that never stops, and waves that each have their own size, pace, brightness and place. Each one swells (rising low-pass), breaks and washes up the sand (foam hiss). The next starts while the last is still washing in |
| Stream | Minnaert bubbles (sines that rise in pitch as they shrink, 520 Hz–2.5 kHz) over the band-limited rush of the water |
| Wind | Noise through a band-pass whose centre and level follow the gusts, higher and louder as the wind picks up. Each ear's band wanders on its own. A faint whistle comes in on the strongest gusts |
| Fire | A flickering low roar, a soft hiss, crackles in bursts of one to five snaps, and the odd pitched pop |
| Night | Five crickets, each with its own pitch, pace and distance. They chirp out of step and now and then fall silent, over the faintest movement of air |

The sounds are level-matched by K-weighted loudness (ITU-R BS.1770, the curve loudness meters use)
to within 5 dB of each other, so switching sound at the same setting does not jump. Night sits 4 dB
under the others because it is the quiet one. Every sound peaks below 0.9 of full scale before
any level is applied.

## Where it plays

| Situation | Path | Level used |
|---|---|---|
| Library music playing | `libcinder_mono.so` inside Sony's SoundServiceFw mixes it into the jack (`snd_pcm_writei`) and the Bluetooth transmitter socket, after the sum when Mono is on | With music |
| USB-DAC → LDAC, or FM → Bluetooth | The shell's LDAC pump mixes it into the stream it is already sending | With music |
| Nothing playing | The shell's own soundscape player opens the output. That is the transmitter socket when headphones are linked (the FM → Bluetooth path, with generated audio in place of the radio), and otherwise the jack PCM (`hw:0,4`, then `hw:0,0`) | On its own |
| FM on the jack, USB storage, BT receiver | Not played; the page says "no output right now" | — |

**Over the music it sits after the EQ and before the volume.** Sony's EQ and effects do not colour
it, and the volume buttons move it together with the song.

### The one rule

The soundscape player never holds the output when music wants it. Everything that starts music
calls `ambient_yield_for_music()` first:

- `set_transport(true)`, `play_pending_sequence`, the USB-DAC and FM bridges, and the receiver.

That call stops the player and waits (300 ms at most) until it has let go; the housekeeping tick
then keeps it stopped while music plays. A player that still held `hw:0,4` when SoundServiceFw
opened it would make the song fail to start. A yield also keeps the player off for 4 s, which is
long enough for the service to report the music playing.

### What over-music needs

`libcinder_mono.so` loads only where Wampy's preload exists (install option `mono`; see
`analysis/RE_mono_audio.md` §9). **Walkman One has no such preload.** Without it, a soundscape still
plays on its own and over USB-DAC and the radio, but while library music plays it is silent. The
page says so in two places:

- the strip reads "SILENT WHILE MUSIC PLAYS";
- the note under the sliders names the install option.

Getting a preload onto Walkman One is a separate decision with its own risk. The options are a
system-wide `/etc/ld.so.preload`, or wrapping W1's own `libaudiohal-adleralsa.so` the way W1
already replaces it. A crash in SoundServiceFw switches the player off, so this is the owner's call
and is not taken here.

## The page

`cargo run -p cinder-host` renders it as `out/soundscape_*.png`.

- A switch, which remembers the sound while off.
- Ten chips: tapping one plays it.
- Two level sliders, 0–100% in steps of 5. The level curve is even in decibels: 1% is −36 dB, 50% is
  about −18 dB and 100% is 0 dB (`soundscape::gain_milli`, shared by the UI and the shell so they
  cannot disagree).
- A strip that says where the sound is going, as the shell reports it: over the music, on its own
  through the headphones or over Bluetooth, over USB-DAC or the radio, silent while music plays,
  waiting for the output, or no output.

A Menu row shows the sound or "Off". The sleep timer switches the soundscape off with the music,
because a sleep timer means silence.

Persisted in the settings file:

- `ambient=<id>` and `ambient_on=0/1`;
- `ambient_levels=<alone>,<music>`.

The ids are `soundscape.h`'s `SS_*` numbers, a file format that must never be renumbered.

## The control line

The shell writes `/tmp/cinder_ambient`, one line: `<sound> <gain × 1000>`.

- It writes a temporary file and renames it, so the shim never reads half a line.
- It writes only on a change, so the 1 Hz tick costs a compare.

The shim reads the file at most every 250 ms, the same rate as the mono flag. Anything malformed
reads as off. Booting to stock unlinks the file, because SoundServiceFw runs under Sony's player too.

## Safety, inside SoundServiceFw

- **No allocation, no locks held across audio, no syscalls, no libm.** The shim gains no
  `DT_NEEDED`. The generator is a fixed 6 KB state, one per route.
- **Try-locks only.** A hook that finds its generator busy sends the music without the soundscape
  for that one buffer rather than make an audio thread wait.
- **Off is bit-exact.** With the gain at 0 and the fade finished, the mix returns before touching
  the buffer, and nothing is computed.
- **Bluetooth bytes are changed only after the handshake**, and only on whole frames inside one
  write; the existing walk in `mono_sum.h` decides which bytes those are. A soundscape is not
  deterministic the way a sum is, so when the kernel splits a frame, the commit keeps the bytes it
  actually went out with and the rest of that frame follows them (`cm_bt_stream.sent`). The mono
  self-test now includes a one-byte-at-a-time kernel to pin this.
- **DSD over PCM is never touched.** Unknown formats pass through as they came.
- **The jack rate** is read back from `snd_pcm_hw_params` after the HAL commits it, with the rate
  setters as a fallback. A stream whose rate was never seen gets no soundscape, and the log says so.

## Cost

The engine runs at 48 kHz at most, whatever the stream's rate: 96 and 192 kHz streams are upsampled
from it by 2 or 4. Every filter has one or two poles, and events come from fixed pools whose loops
stop at the highest live voice.

Measured on the build host, the worst sound costs about 2–3 ms of CPU per second of audio. That
puts it in single-digit percent of one Cortex-A7 core, but the device number belongs to §25.4.

One fault was found and fixed while measuring. Recursive filters fed by voices that fall silent
decay through denormal floats; a −180 dB noise floor on those inputs cut the fire's cost by eight
times.

Like music, a soundscape on its own keeps the audio path on. The power logic counts it as audible:
no codec standby, no SoC suspend and no auto power-off while it plays.

## Tests

| Gate | What it pins |
|---|---|
| `tools/soundscape_selftest.cpp` (69 checks, CI) | Level and peak of every sound; K-weighted spread; white/pink/dark slopes; rain bright, wind dark, crickets in band; beach waves; no repeating period and no repeated second; thunder present; ramps without clicks; off bit-exact; S16/S24/S24-in-32/S32 and mono, saturating; every rate 8–384 kHz; the control line; cost; state size |
| `tools/mono_selftest.cpp` (+8) | The mix hook: never before the handshake, whole frames only, the handshake's rate, mono then mix, odd writes, partial and one-byte accepts never tear a frame |
| `tools/test_mono_shim.sh` (+7) | Through `LD_PRELOAD`: the soundscape reaches the jack and the transmitter, fades to bit-exact off, ignores a malformed line, never writes the caller's buffer, and does nothing in any other process |
| `cinder-ui` (+10) | The page's hit tests and layout, the level curve, the strip, chips, switch, sliders and buttons, the Menu row, setters that refuse nonsense |
| Harness `ambient-alone`, `ambient-music`, `ambient-off` | The line carries the right level for the play state; the page's route is honest with no libasound; libasound is probed once |
| Golden previews | `soundscape_off`, `soundscape_rain_alone`, `soundscape_beach_music_night` |

`SS_WAV_DIR=<dir> soundscape_selftest` writes 20 s of every sound as a WAV, for listening on a PC.
