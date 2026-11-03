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

## What Cinder's code can and cannot do to the speed (read 2026-11-02)

- **The mono shim never changes how much it sends.** `bt_send` (`cinder-mono.c`) sends exactly
  the bytes it was given, summed in place when mono is on, and returns what the real send
  returned; with mono and ambient off it passes the buffer untouched. The #43 fault (a versioned
  libasound symbol resolved to the old version) was on the jack's ALSA path; Bluetooth does not
  go through ALSA.
- **LDAC, SBC and SBC XQ are Sony's encoders and Sony's pacing.** Cinder only changes what the
  SBC endpoint offers (Dual Channel, bitpool up to 47). Over every SBC capture the bitpool and the
  frames per packet never changed (XM4 47 and 3, CMF Buds Pro 2 39 and 2).
- **The LDAC quality setting cannot reach an SBC link.** `BtMwAvSrcRequestSetLdacQuality`
  (libBtMw) stores the value and applies it only when `btmwAvSrcCodecTypeIsLdac()`. So #44's
  "Best effort is worse" on an SBC XQ link is not this setting.
- **An SBC bitpool change restarts the encoder mid-stream** (`BtMwAvSrcRequestSetSbcBitpool`:
  deinit, `NotifyPcmPreferredSize`, init). Only `BtTransmitterService::SetSbcSoundQuality` calls
  it, and Cinder never does, so an SBC link keeps whatever Sony's own settings last chose.
- **No HCI command is sent while audio streams** in any capture: no sniff mode, no role switch.
- **AAC is the one codec Cinder encodes and paces.** Its encoder runs at the link's rate and did
  not check the PCM against it; a 44.1 kHz stream into a 48 kHz encoder would play 9 % fast.
  Sony reconfigures the link per track (48 → 44.1 kHz seen at play), so it has not been seen.

Logging added for the next reports: the Bluetooth service now times every codec's PCM against
its rate (`cinder-aac/src/pcm_rate.h`), logs a stall over 150 ms, a 10 s window more than 1 % off,
and one line per stream (`stream stop: …`); the AAC case says "this plays at the wrong speed".
The mono shim does the same on the sending side ("bt: PCM sent at … Hz").

Closed the same night: plain SBC (Joint Stereo, one boot with `/data/cinder/bt_sbc_xq` set
aside) counts bytes too, 176428 a second on the CMF Buds Pro 2. It is Sony's behaviour for every
SBC link, not something SBC XQ adds.

Found on the way, and fixed:

- **The AAC switch sent the receiver's setting at every boot.** `aac_switch` asked
  `btmwAvBothIsSrc`, which is 0 until a link has a role (libBtMw: A2DP state 3 or 6 = source, 2 or
  5 = sink, anything else neither). So with SBC XQ or LDAC chosen, AAC stayed on, and the CMF Buds
  Pro 2 reconnected on AAC with "transmit AAC off" in the log. Now only `btmwAvBothIsSnk`, or
  Cinder's receiver mode (`/tmp/cinder_bt_rx`, written by `apply_receiver`), gets the receiver's
  value.
- **SBC XQ kept Sony's minimum bitpool of 35**, and in Dual Channel the CMF Buds Pro 2 stop at 31
  on some connects: the link came up "35..31". The minimum is now 2.
- **The CMF Buds Pro 2 offer LDAC + SBC or AAC + SBC**, one pair at a time, switched in their own
  app with a restart of the buds (owner, 2026-11-02). Tonight's AAC and LDAC links on them were
  the two modes.

## What this leaves for #44

The reporter's player differs from this one in three ways: Walkman One's replacement sound
libraries feed the transmitter (`libSoundServiceFw`, `libSoundService`, `libPlayerService`; its
Bluetooth libraries are byte-identical to stock), the earphones, and SBC XQ's higher rate on those
earphones' radio. Walkman One on its own would have connected them with plain SBC. The next
evidence: the same test with SBC XQ unticked, and a trace from their player (Bluetooth ▸ Debug log)
read with `a2dp_timing.py` — a rise in "queued" before the speed change means the radio, flat
pacing with no queue means the earphones.

## The reporter's trace (2026-11-03): the radio drops SBC XQ packets

The reporter (Walkman One, JBL Live Pro 2) found the speed-up goes away with SBC XQ off, and sent
a 55 s Debug-log trace with it on. The link: SBC, Dual Channel, bitpool 46 (`35..46`), MTU 666,
two 196-byte frames a packet (405 bytes), about 540 kbit/s.

| | This player, XM4, SBC XQ | Reporter, JBL Live Pro 2, SBC XQ |
|---|---|---|
| RTP sequence gaps (packets the stack skipped) | 0 | **545 of about 9,550 (5.7 %)**, all through the capture |
| Controller queue | at most 4 | 4 in every 2 s window |
| Audio sent vs real time | +134 ppm | −1485 ppm, lead falling to −150 ms |
| Worst gap between packets | 39 ms | 45 ms |

The sender is never late (no gap over 45 ms); the controller is full and the stack throws packets
away instead of waiting. Each skipped packet is 256 samples (5.8 ms), so about 3.2 s of every
55 s never reaches the earphones. They play what arrives back to back: music with 5–6 % cut out
in 6 ms slices, which is heard as a speed-up. Plain SBC (about 330 kbit/s) fits that radio;
540 does not. The Bluetooth libraries that drop the packets are stock's (Walkman One does not
replace them), so this is SBC XQ's rate on those earphones, not Walkman One.

## SBC XQ Auto: what the stack already has (read 2026-11-03)

An adaptive SBC XQ, like LDAC's Best effort, needs a signal that the radio is falling behind and
a way to lower the bitpool without a restart. Both are in the Bluetooth service process that
cinder-aac is preloaded into (offsets are file offsets in stock 1.02).

- **The encoder is BlueZ's libsbc** (`/lib/libsbc_enc.so`: `sbc_init`, `sbc_encode`,
  `sbc_reinit`, plus MTK's `sbc_init_mtk`, `sbc_set`, `sbc_encode_mtk`). `sbc_set(state, alloc,
  subbands, blocks, bitpool, mode)` writes the library's own `sbc_t` (blocks +5, subbands +6,
  mode +7, allocation +8, **bitpool +9**). `sbc_encode` compares the frame's bitpool with
  `sbc->bitpool` on every call (`ldrb [r1,#9]; cmp` at 0x1566) and recomputes the frame length
  when it changed, so **a new bitpool takes effect on the next frame, no restart**. blueangel's
  packer (`FUN_00047f50`) reads each encoded frame's length, so packets follow.
- **blueangel calls `sbc_set` and `sbc_encode_mtk` through its PLT**, so cinder-aac can interpose
  them: capture `sbc_set`'s arguments at encoder start, call it again with another bitpool.
- **The signal is the one LDAC's Best effort uses.** `btmtk_a2dp_ldac_send_audio_stream_data`
  calls `btmtk_a2dp_request_avdtp_buffer_status()` (exported; sends message 0x21d to mtkbt) on
  every PCM call while LDAC's ABR is on. mtkbt answers (`AVDTP_RtpBufferStatusReq
  rtp_buffer_status = %d`), and `btmtk_a2dp_avdtp_buffer_status_rsp` hands the status byte
  (message +0x20) to `btmtk_a2dp_ldac_abr_proc` — **through the PLT** (`blx 0x17624`), so
  cinder-aac can take it for SBC and pass it on for LDAC. LDAC feeds it to `ldac_ABR_Proc` as
  its transmit-queue depth.
- **A second, pushed signal:** `MSG_ID_BT_A2DP_STREAM_QOS_IND` (`btmtk_a2dp_handle_stream_qos_ind`,
  "QoS = %d") goes to libBtMw through `bt_av_callbacks` slot 12 (+0x30), the table cinder-aac
  already wraps for slot 4. When mtkbt sends it is not known yet.

**Measured 2026-11-03 (WH-1000XM4, SBC XQ Dual Channel bitpool 47, MTU 882, player beside the
headphones):** mtkbt answers the request on SBC too, about ten answers a second at one request per
100 ms; the status was 0 for 65 s, peaking at 1. So it is not an LDAC-only reply, and an idle link
reads 0.

**The walk (same evening, the owner carrying the XM4 out of range and back, HCI trace alongside):**
the status is the transmit queue's depth in packets, and it fills to a ceiling of 10. Skipped
packets (RTP sequence gaps in the trace) start exactly as it fills:

| Uptime (s) | Status (max in the second) | Packets the stack skipped |
|---|---|---|
| 324–325 | 5 | 2 |
| 362 | 8 | 3 |
| 366–403 | 10, every second | 2,993 (577 events) |
| back in range | 0 | 0 |

So a status above about 4 is the warning and 10 is music being thrown away. An Auto steps the
bitpool down when it reads 4 or more and back up after quiet seconds at 0.

**Walk 2, with the first Auto (one step a second down, 10 s quiet up):** 195 skipped packets
against walk 1's 2,993, in two bursts: 92 while it stepped 47 → 38 → 30 → 24 over three seconds
(the queue already at 10), and 95 after it stepped back up to 30 at the edge of range and the
queue was straight back at 10 (a 610 ms gap). The walks were not the same, so the count is not a
fair comparison; the two bursts are what the second version answers: two steps at once on a
reading of 8 or more, and a step up the queue answers within 15 s doubles the quiet needed for the
next (to 80 s; a minute at the top resets it).

Not known before the walk: the status byte's range on SBC (packets queued, as LDAC's
API implies, or a level), whether mtkbt answers 0x21d when LDAC is not the codec, and the values
around the JBL's 5.7 % drop rate. The plan: log the status per PCM call on SBC XQ (XM4 or CMF,
then out of range on a walk), then step the bitpool down while the queue is deep and back up after
a few quiet seconds, never outside the agreed range.
