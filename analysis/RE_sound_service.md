# The sound service, end to end: packets, filters, the output driver layer and its power policy

Started 2026-10-11. Read from Ghidra listings of `libSoundServiceFw.so` and
`libaudiohal-adleralsa.so` (stock 1.02 tree; the audio driver layer is byte-identical on Walkman
One) and from the kernel image. Tools and addressing: [`FIRMWARE_MAP.md`](FIRMWARE_MAP.md).
Addresses below are Ghidra's (file address + `0x10000`) in the stock library unless marked W1.

**Nothing here has been run on the player yet.** Section 7 is the run sheet.

## 1. What is new, in order of use

1. **The player's filter chain is not the A50's.** Walkman One ships a different build of
   `libSoundServiceFw.so` (WM1-class: DSEE HX custom). Its chain has `eq10band` and `eqtone` and has no `eq6band`, `vpt` or
   `clearphase` (section 3). That is why the six-band EQ and Clear Bass measured as no effect at
   the jack (`RE_clear_bass.md` section 4): the filter is not in the chain on this player.
2. **The jack driver layer owns a power policy** (section 4). On opening an output it switches the
   kernel's deep idle off, and for the ordinary music output it also selects the kernel's *ultra*
   deep-idle mode; the kernel's low-power PCM driver switches deep idle back on when the stream
   starts and off when it stops. So deep idle under a running stream is Sony's design, and it
   explains the off-cable run of 2026-10-05 (294 entries in 181 s, pollers
   firing at a fifth of their rate).
3. **With every effect off the chain is skipped whole**, including the integer-to-float
   conversion at each end (section 3). The 6 to 7 % of a core the service costs with source
   direct on is therefore spent outside the effect chain. One candidate is the MQA decoder in the
   pre-filter stage (section 2); it is a candidate only, because PlayerService's FLAC decoder reads
   the `ENCODER=MQAEncode` tag and the stage may be switched on for tagged files alone.
4. **The service takes parameters as strings** through `SoundService::SetParams` /
   `GetParams` in `libSoundService.so`, including per-filter ones (`filter::master:set:` /
   `get:`), and every filter answers `is_proc`. Cinder does not call these yet.
5. **Five undocumented properties** (section 6): tuning files from the SD card, packet dumps to
   `/contents`, an effect log.

## 2. The packet path

One process (`hagoromo11`, nice -15). A client (PlayerService, the USB-DAC player, the radio)
creates a *track* and writes PCM into a shared-memory queue.

| step | function | what it does per packet |
|---|---|---|
| 1 | `SoundServiceTrackHandler::Handle` 0xfa128 | takes a filled packet from the shared queue, allocates a pool packet and copies the payload |
| 2 | `SoundServiceImpl::HandlePacket` → `HandleAudio` 0xece98 | sends `track::play` on the first packet, re-chunks through `SoundServicePacketQueue::Enqueue`, then `sched_yield()` |
| 3 | `WriteToModulesIn` 0xedf7c | filter input, then renderer input |
| 4 | `FilterInDmp::Write` 0x101888 | `PreFilters::Process` (`dsd2lpcm`, `resampler`, `mqadec`), then `FilterChain::Process` |
| 5 | `RendererDmpMaster` | MQA renderer, mute handling, then the output driver layer (`AudioHal`) |
| 6 | `AudioHalOutAdlerAlsa::WriteBufferAplay` 0x23ec4 | `snd_pcm_writei` a period at a time; `snd_pcm_wait(1000)` on a short write |

Control travels in the same queue as packets named `track::set_audio_info`, `track::play`,
`track::pause`, `track::drop`, `track::stop`, `track::drain`, `track::eos`, `track::destroy`.
The output is closed on a stop-class command (`RendererDmpBase::IsStop`), not on pause.

**Packet size** (`SoundServiceImpl::GetPeriodBufferSize` 0xed6d4): `0x20000 / d` bytes, where
`d` is 8 at 44.1/48 kHz, 4 at 88.2/96, 2 at 176.4/192, 1 at 352.8/384, doubled when the sample
is not 4 bytes. So 2048 frames at 44.1 kHz in either width: 46 ms, about 21.5 packets a second.
DSD tracks use `0x8000`.

**The MQA stage.** `PreFilters::ConfigureMqaDec` 0x1043fc switches the decoder on (`proc=on`)
when the track's source codec type is 0x11, 0x12 or 0x13 and the output device is in its list,
and off otherwise. `MqaDec::ProcessAudioPacket` 0x141180 then runs the Sony MQA decoder over
every packet; `CheckAudioInfo` accepts 44.1 to 384 kHz, two channels, interleaved integer. It has no
"is this MQA" gate ahead of the decoder: detection is the decoder's own job (it reports through
`SetLightState`). Parameters it accepts: `proc=on|…`, `max_output_rate=1fs|2fs|4fs|8fs`,
`output_device=…`.

## 3. The effect chain

`FilterChain` holds a named, ordered list of filters between `i2f` and `f2i`.

| firmware | default master chain |
|---|---|
| stock A50 1.02 | `i2f dseeai heq dynamicnormalizer attn eq6band vpt clearphase alc dcphaselinear vinylizer f2i` |
| Walkman One | `i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc dcphaselinear vinylizer f2i` |

The library can build every filter in either list (`CreateFilter` 0x1104a0 knows 18 names); the
list is a static table, one per firmware.

**When the chain runs at all** (`FilterChain::ConfigureDoProcess` 0x1115d8): only if at least one
filter is enabled, `FilterInConfig::AllowToProcess` agrees, the stream is PCM and the rate is not
352.8 or 384 kHz. Otherwise the configuration is `DO_NOT_PROCESS` and packets pass untouched,
`i2f` and `f2i` included. `SetupFilters` 0x112720 treats `heq`, `attn` and `alc` specially: they
have no screen and are switched by the service itself.

**Effect changes** are queued (`EnqueueEffectParam`) and applied on a packet boundary with a
fade (`DoSmartFading`, `FlushChain`): "EFFECT: Makes into Silence", "Flushes to clear Filters'
buffer", "Flushes to grow up Filters".

## 4. The jack output and its power policy

`libaudiohal-adleralsa.so` has six outputs in a static table. Each carries a strategy, an idle
policy and a latency.

| id | ALSA device | strategy | idle policy | latency | used for |
|---|---|---|---|---|---|
| 0 | `hw:0,4` | 0 | 2 | normal | PCM, latency mode 0: ordinary music |
| 1 | `hw:0,0` | 0 | 0 | low | PCM, latency mode 1 |
| 2 | `hw:0,0` | 0 | 0 | 2 | PCM, latency mode 2 |
| 3 | `hw:0,1` | 0 | 0 | normal | the second (beep) track |
| 4 | `hw:0,2` | 1 | 1 | normal | native DSD |
| 5 | `hw:0,2` | 1 | 1 | low | native DSD, low latency |

`AudioHalOutAdlerAlsa::Open` 0x235d8 and `Close` 0x23b28 write these before `snd_pcm_open` and
after `snd_pcm_close`:

| condition | on open | on close |
|---|---|---|
| strategy 1 or 2 (DSD) | `scaling_min_freq` ← `1040000` | ← `598000` |
| idle policy 2 (`hw:0,4`) | `dpidle_state` ← `0`, then ← `mode 1` | ← `mode 0`, then ← `1` |
| idle policy 1 (DSD) | `dpidle_state` ← `0` | ← `1` |
| idle policy 0 with a low latency (`hw:0,0`) | `dpidle_state` ← `0` | ← `1` |

What the kernel does with them (`dpidle_state_store` 0xc0038884, `dpidle_handler` 0xc0037858):

* A bare number sets the deep-idle switch.
* `mode n` is Sony's addition. It sets a flag and calls `bq24262_wmport_set_ulp_mode(n)`, the
  charger's ultra-low-power mode. With the flag set, `spm_go_to_dpidle` loads the
  `pcm_ultra_dpidle` program in place of `pcm_dpidle` and calls `lp_switch_mode` on the way in and
  out, and the handler first refuses while bit 1 of the register at `0xf000660c` is set.
* **The low-power PCM driver flips the switch itself.** `mt8590_lp_pcm_trigger` 0xc0641040
  writes the deep-idle switch to 1 on START and 0 on STOP. `snd_pcm_common_ioctl1` clears it on
  one more path (not identified).
* Deep idle is entered only when the screen is in early suspend
  (`mt_cpufreq_earlysuspend_status_get`), one core is online, the clock check passes and the next
  timer is far enough away.

Put together: Sony switches deep idle off around the open, the stream start switches it on in
ultra mode, and the SoC sleeps between audio interrupts while music plays behind a dark screen.
`hw:0,4` is the low-power PCM (`mt8590_lp_pcm_hardware`: buffer up to 2 MiB, period 3072 to
30720 bytes, at least 8 periods). A period can therefore be at most 174 ms of 16-bit stereo at
44.1 kHz and half that at 32-bit, so the wake rate has a floor near 6 a second at 16-bit and 12 at
32-bit. Source direct sends 32-bit to this device (measured 2026-10-10), so it can only wake more
often than the EQ path.

Other writers of these nodes: `libUsbDeviceAudioPlayerService.so` (USB-DAC: `cpu_num_base` and
`scaling_min_freq` 1040000), `libaudiohal-uacalsasingletrack.so` (`scaling_min_freq`),
`libPowerMgrServiceFw.so` (`cpu_num_base`, behind `AcquireCpuNumBaseFullLock`, which nothing in
`/vendor/sony` imports).

### The other output libraries

| library | output | named constants |
|---|---|---|
| `libaudiohal-uacalsasingletrack.so` | a USB DAC plugged into the player | `scaling_min_freq` ← `1300000` while open, `598000` on close: the CPU floor is pinned at the top frequency for the whole session. Has its own resampler and DSD converter. |
| `libaudiohal-a2dpsnksingletrack.so` | Bluetooth headphones | despite the name it is the transmit path: it writes PCM to the socket `pst::services::bttransmitterservice` (`reference` note on the transmitter socket). Carries `SingleTrackResampler`. Writes no power node. |
| `libaudiohal-dualtrackmixalsa.so` | two tracks mixed into `hw:0,0` | `SRConvWrapper`, `DualTrackMixer`. Writes no power node. |
| `libaudiohal-genericalsa.so` | capture from `hw:0,0` | |
| `libaudiohal-analyzer.so` | the spectrum tap to `pst::services::audioanalyzerservice` | |

## 5. What this means for battery

The awake time under a stream is the CPU work per wake. Measured on the cable 2026-10-10: sound
service 6.5 to 7.5 % of a core, decoder 2.3 %, everything else under 1 %. The wake rate is fixed
by the kernel's period limit. Measured again 2026-10-11 on a 16-bit FLAC with the EQ path (not
source direct): sound service 1.1 %, pre-filters `DO_NOT_PROCESS` (the MQA stage is off for an
ordinary FLAC), period 5513 frames (125 ms). So the 6 to 7 % belongs to source direct or to that
day, and the MQA suspicion is dead for untagged files. So the levers are, in order:

1. **The sound service's own 6 to 7 %.** Not the effects. Find it with a profile (section 7,
   step 1). If it is the MQA decoder, switch it off with `filter::master:set:` and measure again.
2. **The decoder's 2.3 %** and PlayerService's 100 ms poll while a track plays.
   Not read yet: `libPlayerService.so` differs on Walkman One.
3. **16-bit against 32-bit output.** A 32-bit stream halves the longest period. Worth one
   off-cable comparison before changing anything.
4. **The share of time actually asleep under a stream** is still unmeasured: the 2026-10-05 run
   had no wall clock.

## 6. Hidden switches

| property or path | read by | effect |
|---|---|---|
| `persist.sys.sony.icx.tuning` | `Filter::IsTuningEnabled` 0x10fc50 | load effect parameter files from `/contents_ext` (the SD card) in place of `/system/vendor/sony/etc/effector_params/`: `vinylcoeff.csv`, `DseeAi.bin`, `DseeAi.dcfg`. Waits for the card to mount. |
| `persist.sys.sony.icx.hardvinyl` | `Vinylizer` `LoadEffectParam` 0x136114 | force the vinyl type ("HardLoad") |
| `persist.sys.sony.icx.showselog` | static initialiser | print the `EFFECT:` log lines |
| `persist.sys.sony.icx.cmpsepkt` | static initialiser | write each filter's input and output packets to `/contents/PKT_FILE/before_*.raw`, `after_*.raw` and compare them |
| `persist.sys.sony.seon`, `persist.sys.sony.seoff` | `DriverVolTblChanger` | wait times around the volume-table change when effects switch |
| `/contents/SS`, `/contents/*_PID_*.raw` | `SoundServiceImpl::DumpToFile` 0xed7b0 | per-track raw dump, off unless a dump object exists |
| `persist.sys.sony.icx.sehp.dsd`, `.anls.tune`, `.mqarender` | `libDmpFeature.so` | feature switches: native DSD on the single-ended jack, analyzer tuning, MQA rendering |

Log levels come from the environment: `PST_LOG_SS=8` and `PST_LOG_AHAL=8` in
`init.hagoromo.rc`, which is why the filter configuration is in `logcat`.

## 7. Run sheet for the player

Cable in, a 16/44.1 track playing on the jack, screen off for at least 5 s.

1. `tools/ssfw_cost.sh`. It prints the busy thread's user and kernel split, a `perf` profile if
   the kernel allows one, the filter configuration from `logcat`, the open PCM's parameters and
   the deep-idle switch and mode.
2. Read `filter::master:get:mqadec:is_proc` through `SoundService::GetParams` (needs a
   `cinder-probe` verb; not written).
3. Off the cable, `tools/idle_probe.sh arm` with music playing: wall clock against uptime gives
   the share of time asleep. Once with the EQ on (16-bit), once with source direct (32-bit).

## 8. The Walkman One copy

Decompiled 2026-10-11 (`RE_FW=w1`). It is the same code base built with a different
configuration: the only exported class that differs is `DseeHxCustom` in place of `DseeAi`, and
the functions above exist at shifted addresses (`GetPeriodBufferSize` 0xeaa74,
`ConfigureMqaDec` 0x10179c, `ConfigureDoProcess` 0x10e968, `SetupFilters` 0x10fab0). So sections
2, 4, 5 and 6 hold for the player; section 3's table has its chain.

Two settings turn out to be inert in both builds: `RendererDmpMaster::ResolveLpcmPlaybackMode`
returns 1 and `UpdateLpcmPlaybackMode` does nothing, so `lpcm_playback_mode`
(`SoundServiceSettingsDmp`, `RE_dsp_effects_surface.md`) changes nothing on these models.

The renderer does not convert sample width (`RendererDmpBase::GetFormat` takes the packet's
own). The 32-bit output under source direct is therefore what PlayerService delivers, and the
16-bit output with an effect on is `f2i` narrowing to the source's valid bits.

## 9. Not read yet

* Which source codecs are types 0x11 to 0x13 (`PreFilters::ConfigureMqaDec` enables MQA for
  those three only). PlayerService is stripped; its FLAC metadata callback (W1 0xb626c) looks for
  `ENCODER=MQAEncode` and `ORIGINALSAMPLERATE=`, so the types may mean "tagged as MQA".
* `RendererDmpMaster`: the timed mutes, the slave renderer.
* The other output libraries beyond their constants (above).
* `SoundServicePacketQueue`, `IpcAudioBuffer`: the shared-memory layout.
