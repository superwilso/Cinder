# RE — mono audio (accessibility), and why it stops where it does

**Date:** 2026-09-15 · **Device:** NW-A55, firmware 1.02 · **Codec:** Sony CXD3778GF
**Status:** settled for every path except one, and the exception is named at the end. Nothing here
was written to the device; this is an inventory question and the inventories already existed.

**The question.** "Mono audio" in the accessibility sense: sum left and right so that both ears get
the whole mix. It is what makes a stereo recording usable with hearing in one ear, and every phone
has it. Asked for on 2026-09-15, with the follow-up "can audio also be mono over bt as well, like
system wide if it is turned on".

**The answer in one line.** Cinder can sum the channels only on audio it carries itself, which on
this device is the USB-DAC → LDAC bridge and nothing else. It is not a gap in the UI and it is not
device-gated work waiting on a session: three of the four routes are closed by evidence and the
fourth is closed by this project's own security policy.

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

## 7. What is left, honestly

One lever has not been swept: **`Audio I2sout Mch Config`** (numid unknown, value `5,6`, range
0..6) — an MTK-side multichannel I2S output configuration, not a codec register, so it is an
ordinary `amixer` write and **not** covered by the rule in §5. Whether any of its seven values
produces a channel sum is unknown; nothing in the name says it would, and the other six values may
simply silence or mis-route the output. It is reversible (a mixer value does not survive a reboot),
so a device session could sweep it with the bench rig from `RE_headphone_amp_modes.md` and settle it
by measurement rather than by ear.

**Until something is measured, nothing about the jack changes on screen.** The Balance row says what
mono actually reaches and no more, which is the same rule that row already follows for Bluetooth.
