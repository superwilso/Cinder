# Bluetooth send timing, measured over the air path (#44)

2026-11-02, the owner's NW-A55 on Sony's stock 1.02 with Cinder (dev), a WH-1000XM4. Question:
does the player send A2DP audio unevenly, especially when the screen goes dark and stage 1
(early suspend) starts, so that headphones change their playback speed to keep their buffer
level? Report: #44 (Walkman One 3.02 with an external tuning, non-LDAC earphones, the link was
SBC XQ).

## Method

Sony's own HCI trace (`BtCommonServiceClient::SetHciLogEnabled`, a btsnoop file in `/tmp`) holds
every A2DP media packet with the time mtkbt handed it to the controller and its RTP timestamp
(audio time). `analysis/tools/a2dp_timing.py` compares the two: "lead" is how far ahead of real
time the audio has been sent; a stall shows as a gap between packets and a drop in lead, a burst
as audio sent faster than real time. "queued" is ACL packets handed to the controller and not yet
reported sent: a rise there is the radio not getting packets out.

`tools/devctl.sh` drives the player without the screen (buttons, taps, screenshots, the capture).
Each capture: play, screen times out, stage 1 starts (`suspend: idle 5 s -> early suspend`),
Power wakes it (leaves stage 1), it times out again.

## Results

| Codec | Seconds | Packets/s | Worst gap | Stalls / bursts | Sender vs wall clock | Stage 1 |
|---|---|---|---|---|---|---|
| LDAC (Auto, ~930 kbit/s) | 219 | 172 | 35 ms | none | +119 ppm | steady state and a wake |
| AAC (Cinder's encoder, 269 kbit/s) | 189 | 43 | 34 ms | none | −3 ppm | two entries, one exit |
| SBC XQ (Dual Channel, bitpool 47, 551 kbit/s) | 189 | 115 | 39 ms | none | +134 ppm | one entry, one exit |

Two more, the same evening, to test "the CPU is too busy" and "it happens after a boot":

| Test | Seconds | Worst gap | Stalls / bursts |
|---|---|---|---|
| LDAC, screen off, 2 then 4 busy processes at the sender's own priority (nice 0) on both cores | 136 | 24 ms (14 ms under load) | none |
| LDAC from 40 s after a reboot, through the library build and the first stage 1 entry when the 180 s boot grace ends (198 s) | 240 | 35 ms | none |

Load makes the pacing more even, not less: busy cores stay out of deep idle and wake on time.
The sender (BtTransmitterService and mtkbt, both nice 0; SoundServiceFw is nice −15) is not
starved on two cores by anything at its own priority. A walk out of range, by contrast, shows
clearly: 65 skipped packets, the audio up to 106 ms behind, then a 4 % catch-up — heard as
stutters on the XM4, not as a speed change.

Outside the walk, no RTP sequence gaps in any capture, and the controller never held more than 4 packets. On this
player the sender is even through stage 1 entry and exit for all three codecs: whatever #44's
reporter hears is not produced by Cinder's suspend path on stock firmware.

## Facts found on the way

- **mtkbt's SBC RTP timestamps count PCM bytes, not samples:** they advance 176400 a second at
  44.1 kHz (4 bytes a stereo sample). LDAC's advance in samples; AAC's are Cinder's
  (`AAC_FRAME_SAMPLES` per frame). This is Sony's stock behaviour, not Cinder's.
- **Sony's senders run about 120–135 ppm fast against the system clock** (LDAC, SBC); Cinder's AAC
  pacer, on `CLOCK_MONOTONIC`, is within 3 ppm. Headphones absorb this by resampling; 130 ppm is
  about half a second an hour and is not audible on its own.
- **SBC XQ packs 3 frames of 200 bytes into a packet** (613 bytes on an 882-byte MTU), 115 a
  second; plain SBC at bitpool 53 joint stereo would be about 330 kbit/s.
- **The AAC switch takes effect at the next Bluetooth start, not the next connection.** With AAC
  offered and SBC XQ chosen, a disconnect and reconnect still came up AAC (the player's own
  SET_CONFIGURATION picked AAC); after Bluetooth off and on it came up SBC XQ. The Bluetooth
  screen says so honestly ("SBC XQ requested · AAC in use").
- **The HCI trace fills `/tmp` (32 MB) in about four minutes on LDAC.** A full `/tmp` empties
  `/tmp/cinder_aac_status`, and the screen then names an AAC link "SBC XQ".

## What this leaves for #44

The reporter's player differs from this one in three ways: Walkman One's replacement sound
libraries feed the transmitter (`libSoundServiceFw`, `libSoundService`, `libPlayerService`; its
Bluetooth libraries are byte-identical to stock), the earphones, and SBC XQ's higher rate on those
earphones' radio. Walkman One on its own would have connected them with plain SBC. The next
evidence: the same test with SBC XQ unticked, and a trace from their player (Bluetooth ▸ Debug log)
read with `a2dp_timing.py` — a rise in "queued" before the speed change means the radio, flat
pacing with no queue means the earphones.
