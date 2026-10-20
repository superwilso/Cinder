# How the NW-A50 Bluetooth stack sends audio, and where AAC fits

Notes from reverse engineering for interoperability, in our own words. Function names are the
exported symbol names; nothing here is copied from Sony's or MediaTek's code. Firmware: stock
1.02 and Walkman One 3.02 (the Bluetooth libraries are byte-identical between them).

## The three layers

| layer | where it runs | what it does for a transmit link |
|---|---|---|
| `libBtMw.so` (Sony middleware) | hagodaemon `BtCommonService …` | holds the negotiated codec, starts the matching encoder, feeds it PCM from the transmitter socket |
| `libbluetooth.blueangel.so` (MediaTek) | same process | builds the endpoint list, runs the SBC/LDAC/aptX encoders, sends encoded frames to `mtkbt` |
| `mtkbt` (MediaTek) | its own process | AVDTP: advertises the endpoints, picks the codec, builds the RTP packets |

blueangel and mtkbt talk over datagram sockets in `/tmp` (`bt.int.adp`, `bt.a2dp.stream`).

## What already supports AAC

- **mtkbt** converts an AAC endpoint entry into AVDTP capabilities, and when choosing a codec
  as source tries, in order: LDAC, aptX HD, aptX, **AAC**, MP3, SBC. AAC is only considered while
  its AAC switch is on (message `0x222`, the same one the receiver's AAC setting sends).
- **mtkbt's packet path** is codec-agnostic: it copies the payload, adds the content-protection
  byte if SCMS-T is on, and adds the caller's step to the RTP timestamp after each packet.
- **libBtMw** reads an AAC link's sample rate correctly.
- **The player ships an AAC encoder**: `libFraunhoferAAC.so` exports `aacEncOpen` /
  `aacEncEncode`. Only its decoder is used (by the receiver).

## What is missing

1. **No AAC source endpoint.** blueangel's `btmtk_a2dp_sep_default_setting_initialized` fills four
   source entries: SBC, LDAC, aptX HD, aptX. mtkbt accepts at most four.
2. **libBtMw drops PCM for any codec but SBC, LDAC and aptX.** Its send-data request checks the
   codec and has no branch for AAC, and its encoder start does nothing for AAC either.
3. blueangel has no AAC encoder call.

## How cinder-aac fills the gaps

All hooks are functions blueangel calls through its own PLT (it is linked without `-Bsymbolic`,
with `BIND_NOW`), so an `LD_PRELOAD` in front of the service process stands in for them.

| hook | does |
|---|---|
| `btmtk_a2dp_sep_default_setting_initialized` | after blueangel fills the source list, replaces entry 2 (aptX HD) with AAC-LC, 44.1/48 kHz, stereo, up to 320 kbit/s, unless `/data/cinder/bt_aptxhd` says aptX HD is the choice. blueangel rebuilds the list in every `btmtk_a2dp_send_activate_req` (each Bluetooth start) and sends mtkbt a fresh copy, so the slot can change between starts |
| `btmtk_a2dp_send_aptxHD_enable_req` | while AAC holds the slot, Sony's aptX HD switch is the AAC switch (AAC has no transmit switch of its own), so Cinder's codec choice turns AAC on and off live |
| `btmtk_a2dp_send_aac_enable_req` / `btmtk_a2dp_send_activate_req` | re-sends that choice whenever the stack sets the switch; receiving keeps the receiver's own AAC setting |
| the codec-config slot of `bt_av_callbacks` (exported) | when a *source* link comes up AAC, libBtMw is told it is SBC at the same rate, so it runs its SBC path |
| `btmtk_a2dp_stream_data_encoder_init` / `_deinit` | libBtMw's "SBC" encoder start/stop opens/closes an AAC encoder instead |
| `btmtk_a2dp_send_audio_stream_data` | libBtMw's PCM, 1024 frames at a time, encoded to LATM (StreamMuxConfig in every frame), paced to real time, handed to `btmtk_a2dp_send_audio_encoded_stream_data` with a timestamp step of 1024 |

Source vs receiver is told apart with libBtMw's exported `btmwAvBothIsSrc`.

## Trade-offs

- **AAC and aptX HD take turns.** mtkbt holds four source endpoints, so the two share one;
  Cinder writes `/data/cinder/bt_aptxhd` when aptX HD is the choice and restarts Bluetooth when
  the slot holds the other one.
- **AAC rides Sony's aptX HD switch.** Picking AAC in Cinder calls `SetAptxHD(true)`; any other
  choice turns it off, so headphones without the chosen codec fall back to SBC, as they do for
  aptX and LDAC.
- **Sony's own status reports SBC** for an AAC link; `/tmp/cinder_aac_status` has the truth, and
  Cinder's Bluetooth screen reads it.

## Files on the player

| path | what |
|---|---|
| `/system/vendor/unknown321/lib/preload/BtCommonService.so` | this library; Cinder's hagodaemon wrapper preloads any `preload/<Service>.so` into that service |
| `/data/cinder/bt_aac` | present = AAC on (read when the service starts) |
| `/data/cinder/bt_sbc_xq` | present = SBC XQ on (read when the service starts) |
| `/data/cinder/bt_aptxhd` | present = the shared slot stays aptX HD (read at each Bluetooth start; Cinder writes it) |
| `/tmp/cinder_aac.log` | what the hooks did |
| `/tmp/cinder_aac_status` | `AAC <rate>` from the moment a link is AAC, `AAC <rate> <bitrate>` while it streams; empty for any other codec |
| `/tmp/cinder_aac_codecs` | what this boot offers, one a line: `aac`, `sbc-xq`, and after Bluetooth starts `slot aac` or `slot aptxhd` (Cinder reads it) |
| `/system/vendor/unknown321/lib/libfdk-aac-enc.so` | the fdk-aac 2.x encoder, preferred over the firmware's |

## Encoder: upstream fdk-aac 2.x preferred

`build_fdk.sh` builds upstream fdk-aac 2.0.3's encoder (source fetched into `third_party/`, not
committed) as `libfdk-aac-enc.so`, libc-only, for glibc 2.23. `aac_enc.c` loads it from
`/system/vendor/unknown321/lib/` first and falls back to the firmware's `libFraunhoferAAC.so`.
The upstream build accepts `AACENC_PEAK_BITRATE`, so no frame exceeds the average: the bitrate
can use 90% of the packet room instead of 70% (XM4, 882-byte payload: ~268 kbit/s, was ~188).

## SBC XQ (`/data/cinder/bt_sbc_xq`)

blueangel's source SBC entry (byte 4/5 bitpool min/max, byte 10 channel-mode mask: joint 1,
stereo 2, dual 4, mono 8) is rewritten to Dual Channel only, max bitpool 47. mtkbt's SBC choice
takes the first channel mode both sides share (joint first) and max bitpool = the smaller of the
two maxima, so this yields Dual Channel at up to 47 per channel: ~550 kbit/s at 44.1 kHz.

## Looked at, not done

- **A fifth source endpoint.** mtkbt clamps source endpoints to 4 and its 4 source slots
  (0x178 bytes each) are followed directly by the receiver's 3; a fifth would overwrite the
  receiver's first. Moving the arrays means patching every site that indexes them.
- **A newer LDAC.** The firmware's LDAC core reports 1.0.0, the same as AOSP's open-source
  libldac; only AOSP's wrapper is newer (2.0.2 vs 1.2.4). The firmware already runs Sony's
  adaptive bitrate (`ldac_ABR_Proc`); AOSP's adds settable thresholds, not a different method.
