# Does Sony's DSP reach Bluetooth? (goal 7) — what is known, and how to settle the rest

Research note for phase R5 of the redesign ([`PLAN_redesign_2026-09.md`](PLAN_redesign_2026-09.md),
goal 7: *keep every effect, and apply them to Bluetooth*). Written 2026-09-30, host only: nothing
here was run on the player for this note. It separates three kinds of statement and keeps them
apart, because this question has been answered "yes" in the repository before on weaker evidence
than the answer deserves:

* **Static** — read from the firmware's libraries on the build machine. True of the code, not
  proof of what a listener hears.
* **Measured earlier** — an observation already recorded in this repository, with where.
* **Unknown** — nobody has measured it. Each one has a device step in
  [`DEVICE_CHECKLIST.md`](DEVICE_CHECKLIST.md) §26.

**Short answer.** The effect chain is *upstream* of the Bluetooth transmitter, there is a single
switch that lets it through (`SetBtAudioSoundEffect`), and Cinder sets that switch on every apply.
For the three tone stages (10-band EQ, 6-band EQ, Tone Control) the library carries Bluetooth-specific
tables, which is strong static evidence they run on that route. For VPT, DC Phase, the Dynamic
Normalizer and Clear Phase the library decides per output device from tables this note did **not**
decode, so whether they run on Bluetooth is **unknown**. DSEE HX, DSEE AI and the Vinyl Processor
have no per-device gate at all in the library. **Nothing has been measured at a Bluetooth sink.**

---

## 1. Where the chain sits (static)

Ordinary playback goes PlayerService → `libSoundServiceFw.so` → an output. Inside
`libSoundServiceFw.so` the effects are `pst::services::sound::mobile::Filter` subclasses assembled
by `mobile::FilterChain` (78 exported members: `Create`, `Setup`, `SetupFilters`, `SetEnable`,
`SetParam`, `CheckNeeded`, `ConfigureDoProcess`). The classes, from the dynamic symbol table:

`Eq10band`, `Eq6band`, `EqTone`, `Vpt`, `DcPhaseLinearizer`, `DynamicNormalizer`, `ClearPhase`,
`Vinylizer`, `Heq`, `DseeAi`, `Alc`, plus the plumbing (`Dsd2Lpcm`, `Resampler`, `Attn`,
`PacketFader`, `I2f`/`F2i`, `MqaDec`/`MqaRen`).

The Bluetooth transmitter is not an ALSA device: PCM leaves through `BtTransmitterService`'s socket
([`../analysis/RE_volume_service.md`](../analysis/RE_volume_service.md) §6, measured
2026-09-07 — no codec-side control reaches A2DP). So anything that changes Bluetooth audio has to
happen in this filter chain, before the renderer. There is no later stage to do it in.

## 2. The switch (static + measured earlier)

`EffectCtrlDmp::SetBtAudioSoundEffect(bool)` / `IsBtAudioSoundEffectOn()` exist, the stock player
imports both, and `libSoundServiceFw.so` contains the parameter name `btaudiosoundeffect`. Cinder
calls `SetBtAudioSoundEffect(1)` at the end of every chain apply, uncached
(`cinder-home/src/main.cpp`, `apply_sound_fn`), since 2026-08-23
([`AUDIT_2026-08-23_sound_effects.md`](AUDIT_2026-08-23_sound_effects.md) §E1).

*Measured earlier:* `cinder-probe --fx` prints the flag's read-back
(`BtAudioSoundEffect=…`). **A read-back is not evidence on this device** — the service stores
whatever it is handed (the high-gain and `SelectUsingEq` findings). The flag reading 1 says the
write landed, not that the chain is in the Bluetooth path.

## 3. Which effects the library allows on which output (static)

Each filter answers `IsAdaptedDevice(const OutputDevice&)`. Disassembly of `libSoundServiceFw.so`:

| Effect class | `IsAdaptedDevice` | What that means |
|---|---|---|
| `Filter` (the base) @0xfeef8 | returns `true` unconditionally | A class that does not override it is allowed on **every** output |
| `Vinylizer`, `Heq` (DSEE HX), `DseeAi`, `Alc` | **not overridden** | No per-device gate in the library. Whether they are *instantiated* for the Bluetooth track is decided elsewhere (`FilterChain::Create` / `CheckNeeded`), not read here |
| `Eq10band` @0x11cc38, `Eq6band` @0x120bf4, `EqTone` @0x124a2c | a lookup in a static `std::map` keyed by `OutputDevice` (the red-black-tree walk is visible: compare key at node+16, descend left/right) | Allowed on the outputs that have a table entry |
| `Vpt` @0x114290, `DcPhaseLinearizer` @0x118694, `DynamicNormalizer` @0x10dba8, `ClearPhase` @0x111350 (+ `…Hp` / `…Spk` / `…Wp`) | the same map lookup, against each class's own map | Allowed on the outputs in that map — **contents not decoded** |

For the three tone stages the map's *values* are volume tables, and their names are in the
library's strings:

```
kEq10bandVolTableA2dp   kEq6bandVolTableA2dp   kEqToneVolTableA2dp   kAlcVolTableA2dp
kEq10bandVolTableLineOut … kEq10bandVolTableSe … kEq10bandVolTableBtlGainHigh …
```

An `A2dp` table exists for `Eq10band`, `Eq6band`, `EqTone` and `Alc`. That is the static evidence
that Sony built those four to run on the Bluetooth route. There is **no** `Uac`/`Usb` table for any
of them — see §5.

What this note did **not** do: decode the static initialisers that fill the maps for `Vpt`,
`DcPhaseLinearizer`, `DynamicNormalizer` and `ClearPhase`, or map the numeric values of
`sound::OutputDevice` (the volume library's `funcarch::OutputDevice` has 5 = A2DP, but that is a
different enum in a different namespace and must not be assumed to be the same numbering). So for
those four effects the honest state is *unknown*, not *probably*.

## 4. What has been observed on the device (measured earlier)

* The **10-band EQ changes the level of the stream the analyzer sees**: a flat −10 dB curve moved
  the analyzer reading −9.5 dB, a flat −5 dB moved it −5.8 dB (`main.cpp`, `apply_eq_fn` comment;
  the Bluetooth fine-volume trim is built on it). The analyzer taps the chain, not the sink, so on
  its own this is evidence about the chain. The one observation of an EQ write being *heard* over
  Bluetooth is indirect: with fine volume on, each volume press clicked in the headphones, traced
  to the EQ filter being re-instantiated (`DEVICE_CHECKLIST.md` 5.2, still open). The trim's level
  steps themselves have not been measured at a sink, and all of this is the EQ only.
* `isproc` is the service's own statement of whether an effect is processing
  (`<Effect>::UpdateProcCond(bool,bool) … isproc is N` in `logcat`). It has been read for the tone
  selector on the jack (`DEVICE_TESTS.md`), **never with a Bluetooth link up**. Every class in §3
  with an `IsAdaptedDevice` override also exports `UpdateProcCond`, so the same log line exists for
  all of them.
* `--dseemeter` (DSEE HX's effect on the top analyzer bands) has not been run with a Bluetooth link
  ([`PLAN_2026-09-14.md`](PLAN_2026-09-14.md), goal 7 row).

## 5. USB-DAC (static, and a warning for the profile screen)

In USB-DAC mode the PC's audio is rendered by `UsbDeviceAudioPlayerService` (to the jack) or
bridged by Cinder straight to the transmitter socket (to Bluetooth, `ldac_start`). Neither path is
known to pass through `mobile::FilterChain`, and no effect has a USB table (§3). The Profiles
screen therefore says *"effects on PC audio unverified"* on its USB-DAC row: the row correctly
selects which profile is live in DAC mode — and the profile's **balance** is a codec mixer control
that does reach the jack — but whether any Sony effect is heard on the PC's audio is unknown. The
bridged-to-Bluetooth case almost certainly gets none: Cinder copies the capture PCM to the socket
itself.

## 6. How to settle it (device, all in §26 of the checklist)

No ears needed for the first pass; the service says it itself.

1. Connect Bluetooth headphones, play a track, `logcat -c`.
2. On Sound, switch one effect on, wait two seconds, `logcat -d | grep isproc`. Repeat per effect:
   10-band EQ, Tone Control, VPT, DC Phase, Dynamic Normalizer, Clear Phase, Vinyl, DSEE HX.
3. Expected if the effect runs on Bluetooth: `<Class>::UpdateProcCond … isproc is 1` on switch-on
   and `… is 0` on switch-off. An effect whose line stays at 0 with a Bluetooth link and goes to 1
   on the jack is **not adapted for A2DP** — its row should then say so on the Bluetooth route, the
   way Balance already does ("Wired output only").
4. `cinder-probe --fx` with the link up: `BtAudioSoundEffect=1` expected (necessary, not
   sufficient).
5. Then, and only for the effects that report `isproc is 1`, an ear check on the headphones.
6. USB-DAC: the same `isproc` read while a PC is streaming, jack output. No lines at all means the
   chain is not in that path; the USB-DAC row's wording then becomes a statement instead of a
   caveat.

Until then, what Cinder may claim is exactly this: *the EQ very probably reaches Bluetooth; the other effects are
sent to the service for Bluetooth too, and whether each one is applied there has not been measured.*
