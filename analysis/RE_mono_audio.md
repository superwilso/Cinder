# RE — mono audio (accessibility), and why it stops where it does

**Date:** 2026-09-15 · **Device:** NW-A55, firmware 1.02 · **Codec:** Sony CXD3778GF
**Status:** §1–6 settle where a *cheap* mono can and cannot go, and §6 is what shipped. §7, added on
a second pass, answers the separate question of what a genuinely **system-wide** toggle would take —
it is possible, it is two userspace shims, and the mechanism is already proven in production by
someone else. Nothing here was written to the device.

**The question.** "Mono audio" in the accessibility sense: sum left and right so that both ears get
the whole mix. It is what makes a stereo recording usable with hearing in one ear, and every phone
has it. Asked for on 2026-09-15, with the follow-up "can audio also be mono over bt as well, like
system wide if it is turned on".

**The answer in one line.** *Without new machinery*, Cinder can sum the channels only on audio it
carries itself — the USB-DAC → LDAC bridge and nothing else, which is what §6 ships. *With* new
machinery, system-wide mono is reachable: §7 shows it takes two `LD_PRELOAD` shims into Sony's own
audio services, using an injection mechanism that has been running on this device in production for
years. What it never takes is a codec register (§5, §7.6).

## 1. There is no ALSA control that does it

Card 0 publishes **51 controls** and they have all been read on the device
(`RE_hardware_surface.md` §2, `docs/AUDIT_2026-08-18_device_vs_sony.md` §B). The two that touch
the channels are `l balance volume` and `r balance volume`, and both are **attenuation only**
(INTEGER 0..88 of half-decibels of cut — `cinder-home/src/main.cpp`, `apply_balance`). Attenuating
one side cannot put its signal into the other. Nothing else in the 51 mentions channels, mixing,
downmix or mono.

## 2. Sony's DSP surface has no mono call

`cinder-audio/src/effect_abi.hpp` is the recovered `libEffectCtrlDmp` surface, and
`RE_dsp_effects_surface.md` is the catalogue behind it: DSEE HX (+AI, +Custom), VPT, DC Phase
Linearizer, Dynamic Normalizer, ClearAudio+, Vinylizer, Clear Phase, Source Direct, Tone Control,
the 6- and 10-band EQs, user presets. Every one is a filter or a bypass. **There is no channel
operation of any kind in the ABI.**

## 3. Sony's own firmware has no such setting — primary evidence

The strongest piece, and the reason this file exists rather than a "probably not". Sony's
`HgrmMediaPlayerApp` ships its entire English UI as a label table, extracted in
`superwilso/cinder-sony-analysis` (`analysis/ui_assets/labels_en.json`). It holds **729 labels**.
Searching all of them for mono, monaural, downmix, "both ears", single channel:

```
'040002' 'Mono'
'040010' 'Mono/Auto'
```

**Two hits, and both are the FM radio.** The `04xxxx` family is the tuner (`040000` "FM Radio
Settings", `040007` "Scan Sensitivity", `040013` "Auto Preset"), and Mono/Auto is the ordinary
stereo-blend control every FM receiver has. Every "mono" in the QML is `SCmnMonospaceLabel` — a
font.

So Sony never shipped an accessibility mono setting on this player. There is nothing to imitate
and nothing to re-enable: the feature was never in the firmware.

## 4. Bluetooth does not pass the codec at all, so a codec fix would not reach it

This is the answer to "system wide". The two outputs do not share a signal path:

```
  local file ──► PlayerService ──► SoundServiceFw ──► CXD3778GF ──► 3.5 mm
  local file ──► AudioInRecorder (OMX capture) ──socket──► BtTransmitterService ──► LDAC ──► BT
```

Bluetooth transmit is **non-ALSA**: PCM goes over an abstract AF_UNIX socket straight into
`BtTransmitterService`, which encodes and hands it to the MTK radio (`E_usbdac_ldac/RE_findings.md`,
"BtTransmitterService — the PCM entry point"). It never passes the codec or any mixer. That is
already why the Balance row says "Wired output only — Bluetooth is not affected", and any
channel-sum done at the codec would inherit exactly that limit.

**And the transmitter client has no channel setter.** Its vtable is fully recovered
(`G_bt_nfc/RE_findings.md`, slots 3–38): connection control, AVRCP metadata, volume, and the codec
selectors `SetLdac` / `SetAptxHD` / `SetAptxClassic` / `SetSbcSoundQuality` / `SetLdacSoundQuality`.
`BtSoundChannel` appears **only as an out-param of `GetSoundStatus` (slot 26)** — it reports what
A2DP negotiated, it does not set it.

## 5. The codec register is not a candidate — it is forbidden

`CODEC_DATA_SEL` (0x24, `analysis/kernel/cxd3778gf_regmap.txt`) is the only register in the map
whose name suggests input-data routing, and it is the obvious thing to reach for.

**Do not.** `SECURITY.md` rule 1 is standing and was written after an incident:

> **Never write `/proc/regmon/<chip>/value`.** Selecting a register through `target` is a read;
> writing `value` changes audio hardware under the running player, and the codec is the one part
> of this device with no software recovery path.

`cinder-probe`'s own `codec_read_reg` carries the same note. So this route is closed by policy, not
by ignorance, and a probe that tested it would be a probe that broke the rule. **An earlier draft of
this work named that register as "the one candidate" for a device session; that was wrong, and this
section is here so it is not proposed again.**

## 6. What Cinder CAN do, and does

The USB-DAC → LDAC bridge is the one place Cinder holds the samples: `ldac_pump`
(`cinder-home/src/main.cpp`) reads S32_LE frames from the UAC gadget's capture PCM and writes
S16_LE frames to the transmitter's socket. Summing there is four lines, and it is real audio, not a
flag that lands somewhere.

* **`(L + R) / 2`**, halved rather than clipped: L+R of a correlated mix is up to +6 dB, and a
  bridge that distorted the moment mono was switched on would be worse than no mono.
* **The wire format stays two channels**, both carrying the sum. "Both ears get everything" is what
  the setting means; switching the handshake to `chans=1` would halve the bandwidth but needs a
  renegotiation, and the handshake's channel field is read once at connect
  (`+4` in the type-1 payload — "1 stays 1, anything else becomes 2").
* Read **once per session**. The pump runs ~86 times a second and the getter takes the renderer's
  lock; a change lands on the next session, and `CINDER_ACT_MONO_CHANGED` logs that it will.

## 7. What a SYSTEM-WIDE toggle would actually take

*Added 2026-09-15, second pass, after the question "if I want a system wide mono toggle what is
needed".* §1–6 say where mono cannot go; this says what it would cost to put it there anyway. The
new evidence is `unknown321/wampy`, which has been doing userspace injection on this exact device in
production for years and publishes its sources — a stronger reference than a fresh Ghidra pass,
because it is running code rather than a reading.

**No Ghidra was used and none could be.** The firmware binaries are not in this clone (`artifacts/`
is gitignored and `make phase1` needs a `.UPG` that only a browser can fetch from Sony), and Ghidra
is not installed here. Everything below is from published RE, this repo's own measurements, and the
kernel/driver sources Wampy quotes. Where something needs the binary in hand to settle, it says so.

### 7.1 The injection mechanism is proven, and it is not exotic

Wampy adds a line to the service's entry in the boot ramdisk's `init.hagoromo.rc`
(`wampy/installer/run.sh`):

```sh
sed -i '/SoundServiceFw/a \ setenv LD_PRELOAD /system/vendor/unknown321/lib/libsound_service_fw.so' \
    ${INITRD_UNPACKED}/init.hagoromo.rc
```

…and does the same for `PlayerService` with `libdmp_feature.so`. The shim recovers each original
with `dlsym(RTLD_NEXT, "<mangled C++ symbol>")` and calls through. So **arbitrary code can be run
inside Sony's audio services**, per service, and this repo's own install notes already record both
libraries as present on the reference device.

The cost is a boot-image edit, which means repacking and flashing a `.UPG` — machinery Cinder's
installer already has.

### 7.2 Sony's DSP chain, and why no filter list can give us mono

`SoundServiceFw` runs a **named, ordered filter chain over FLOAT samples**. Wampy intercepts
`FilterChain::Create(DynamicAllocPacketPool*, const std::vector<std::string>&)` and substitutes its
own list; the full A50 chain it installs is:

```
i2f → dseeai → heq → dynamicnormalizer → attn → eq6band → eq10band → eqtone
    → vpt → clearphase → alc → dcphaselinear → vinylizer → f2i
```

`i2f`/`f2i` bracket the chain, so everything between them is floating-point DSP — which is exactly
the shape a channel sum wants. Two things follow:

* **Sony has more filters than its UI exposes** (`alc`, `attn`, `heq` are in the chain and on no
  screen), and Wampy turns them on simply by naming them.
* **None of them is mono.** The complete set Wampy enumerates is `alc, attn, clearphase,
  dcphaselinear, dseeai, dseehxcustom, dseehxlegacy, dynamicnormalizer, eq10band, eq6band, eqtone,
  heq, vinylizer, vpt` — and the WM1Z chain, i.e. the flagship, has the same members with a
  different DSEE. **There is no channel operation anywhere in Sony's DSP, on any model here.** That
  closes §2 with the implementation's own list rather than an ABI dump.

So a filter list cannot deliver mono. A *new filter object* spliced into that chain could — see
7.5.

### 7.3 The cheapest thing that works: interpose the ALSA write (jack only)

`libaudiohal-adleralsa.so` is the HAL that opens the codec's PCM. That is not inferred: Walkman One
ships patched copies of that exact file (`etc/.mod/adler/{normal,normal_nt,pv1,pv2}/`) whose entire
difference is the ALSA device name — "Plus v1 changes output `hw:0,4` (cxd3778gf-icx-lowpower) to
`hw:0,0` (cxd3778gf-hires-out)" (`wampy/MAKING_OF_VOLUME_TABLES.md`). The device string is inside
that library, so that library opens and writes the PCM.

Preload into the service that hosts it, interpose `snd_pcm_writei` (and the mmap variant), sum the
two channels in the buffer, call through. Roughly the same four lines already in `ldac_pump`.

**The one thing that has to be checked on the binary first:** LD_PRELOAD only interposes symbols
resolved through the PLT of a `DT_NEEDED` dependency. If `libaudiohal-adleralsa.so` `dlopen`s
libasound and `dlsym`s the entry points — which is exactly what `cinder-home` itself does for the
same library — **interposition silently does nothing**. One command settles it:

```sh
readelf -d /system/vendor/sony/lib/libaudiohal-adleralsa.so | grep NEEDED
readelf -r /system/vendor/sony/lib/libaudiohal-adleralsa.so | grep snd_pcm
```

If it is `dlopen`'d, the fallback is to **replace the HAL** with a wrapper that forwards to a
renamed original — and Walkman One already replaces this specific file, so the pattern and the
risk are both known.

**Covers:** the 3.5 mm jack, all local playback. **Does not cover Bluetooth** (§4).

### 7.4 Bluetooth needs its own hook

A2DP PCM never reaches ALSA. It is written to `BtTransmitterService`'s socket by Sony's
`AudioInRecorder` after an OMX capture (`E_usbdac_ldac/RE_findings.md`). So the second hook is a
preload into whichever `hagoromo` hosts that producer, interposing `write`/`send`/`sendmsg` and
filtering by the socket fd — the frames are S16_LE stereo, which this project has already confirmed
by draining the socket at exactly `rate x channels x 2` bytes per second.

**This one needs a device session before it can be written**, and the questions are small:

1. Which `hagoromo` process holds the connected socket while A2DP plays (`ls -l /proc/*/fd` for the
   abstract name `pst::services::bttransmitterservice`)?
2. Is `AudioInRecorder` in that same process, or does it hand off again?

Nothing on the host can answer those.

### 7.5 The single point that would cover everything

The chain in 7.2 sits **before** the jack/Bluetooth split, so one filter there is the only place a
single switch covers every route at once. Adding one means constructing a filter object Sony's
factory never makes: its vtable, its per-packet entry point, and the packet layout
(`DynamicAllocPacketPool` — Wampy has a partial shape for it in `sound_service_fw.h`, carried as
opaque byte arrays, which is a good sign of how much is understood and how much is not).

This is the deepest option and the only one that can crash a core audio service on a bad guess,
which is the failure mode `SECURITY.md` rule 3 exists for ("never guess vtable slot indices").
It is also the only one that is genuinely, architecturally *system-wide*.

### 7.6 The forbidden route does not become the right route

The codec register from §5 is what this project's rules rule out, and lifting that rule does not
buy the feature:

* **There is no evidence any codec register sums channels.** `CODEC_DATA_SEL` is a name that
  sounds promising in a 210-entry map; nothing in the driver or the register map says it mixes.
* **Even if it did, it would not be system-wide** — Bluetooth never passes the codec (§4). It
  would buy exactly what 7.3 buys, for far more risk.

So the honest ranking is: **7.3 + 7.4 for a real system-wide toggle** (two small shims, both
recoverable by deleting a file and re-flashing), **7.5 if one clean switch is worth deep RE**, and
**the codec register never** — not because it is forbidden, but because it is the worst trade on
the list.

### 7.7 What to do first

In order, cheapest first, and the first two are a single short device session:

1. `readelf -d`/`-r` on `libaudiohal-adleralsa.so` → decides 7.3's shape (preload vs. replace).
2. `ls -l /proc/*/fd` with LDAC playing → names the process for 7.4.
3. Build the 7.3 shim against the answer to (1), flash, listen. That is mono on the jack.
4. Then 7.4, and mono is system-wide in the sense that was asked for.

## 8. The lever nobody has swept

One lever has not been swept: **`Audio I2sout Mch Config`** (numid unknown, value `5,6`, range
0..6) — an MTK-side multichannel I2S output configuration, not a codec register, so it is an
ordinary `amixer` write and **not** covered by the rule in §5. Whether any of its seven values
produces a channel sum is unknown; nothing in the name says it would, and the other six values may
simply silence or mis-route the output. It is reversible (a mixer value does not survive a reboot),
so a device session could sweep it with the bench rig from `RE_headphone_amp_modes.md` and settle it
by measurement rather than by ear.

**Until something is measured, nothing about the jack changes on screen.** The Balance row says what
mono actually reaches and no more, which is the same rule that row already follows for Bluetooth.
