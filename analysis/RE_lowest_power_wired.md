# RE — the lowest-power wired playback this hardware allows

The question (2026-11-23): what is the least power the NW-A50 series can play music to the 3.5 mm
jack with — fewest chips powered, scrobbling kept — limited only by the hardware?

Sources: Sony's kernel source (`kernel/work/kernel`), the CM4 coprocessor's firmware read from the
player's `cm4` partition (p21, read-only) and decompiled, Sony's audio HAL
(`libaudiohal-adleralsa.so`, decompiled), and live reads on an NW-A55 during screen-off jack
playback (sysfs and `/proc` only). Nothing was written to the player for this note.

There is no fuel gauge on this platform (`RE_battery.md`), so nothing here is a current
measurement. Savings are argued from what is powered and how often things wake; each one is
proven only by an off-cable battery A/B run.

## 1. The answer in one paragraph

MediaTek already built the hardware floor into this chip, and Sony's stock software already uses
it. During wired playback with the screen off, the two Cortex-A7 cores can be powered off while a
small Cortex-M4 (the CM4, clocked from the 26 MHz crystal) feeds the I2S output from 128 KB of
on-chip SRAM. The CM4 wakes DRAM out of self-refresh only to copy the next chunk, and wakes the
A7 cores only when the music buffer in DRAM is nearly empty. What keeps today's playback above
that floor is mostly software: the DRAM buffer Sony asks for is 1 s, where the hardware takes
11.9 s (and more with a kernel change), and anything that vetoes the low-power mode (the Bluetooth
radio, the USB cable, a second CPU core) wakes the A7 cores far more often.

## 2. How "ultra deep idle" works

The low-power PCM is ALSA device `hw:0,4` (`cxd3778gf-icx-lowpower`,
`sound/soc/mediatek/mt8590/mt8590-lp-audio.c`). Sony's HAL opens it for ordinary music and writes
`mode 1` to `/sys/power/dpidle_state` (`RE_sound_service.md` §4).

- **The music buffer is in DRAM**, up to 2 MB (`buffer_bytes_max` and the preallocation, both
  `2 * 1024 * 1024`). The A7 side writes it through normal ALSA.
- **The CM4 plays from SRAM.** Its firmware lives in partition p21 (`cm4`, 512 KB, MediaTek
  header `0x58881688` then a 22 KB Thumb image loaded at 0, stack at `0x20015000`). The preloader
  copies it to the CM4 code SRAM (`0x11300000`); the kernel only backs it up and releases the CPU
  reset (`mt8590_lp_audio_probe`). The CM4 copies from the DRAM ring into the AFE's 128 KB SRAM
  (`0x112A0000`) by DMA, and the two CPUs share a control block in the 128 KB "bitstream" SRAM
  (`0x11340000`, `struct lp_info` in `mt8590-afe.h`).
- **Its main loop** (decompiled, `FUN_000027a6`): when the A7 side has said "low power"
  (`lp_switch_mode(1)`) and the A7 cores read as powered off (`FUN_00004554`), it logs
  `receive LP_MODE_LOWPOWER and CA7-OFF`, requests DRAM self-refresh (`dram goes to sleep`) and
  waits for an interrupt. On each wake it brings DRAM back (`dram is awake`, `FUN_000045ea`),
  copies the next chunk, and checks how much unread music is left in the DRAM ring. **Only when
  that falls below two copy chunks does it wake the A7 cores** (`FUN_00004562`: clears a bit in
  the power controller at CM4 `0x40000010` and waits for the A7 power status) and leave low-power
  mode.
- **The A7 side does not wake on its own timers.** `spm_go_to_dpidle` in mode 1 uses
  `WAKE_SRC_FOR_ULTRA_DP`: the CM4, keys and the PMIC (EINT), the connectivity chip (CONN, its
  watchdog), USB, thermal, IR and `SYSPWREQ`. The audio interrupt (AFE) and the general-purpose
  timer (GPT) are left out, and the sleep timer is set to 0. Kernel and user timers wait until
  the next wake. Sony's 10 Hz pollers therefore do not keep the A7 cores awake in this mode.
- **What blocks entry** (`dpidle_can_enter`, read live in `/sys/power/dpidle_state`): the
  connectivity domain powered (Bluetooth radio on: `by_oth`), a key held (`by_oth`), the screen
  not in early suspend (`by_vtg`), a second core online (`by_cpu`), a clock in use such as USB0
  with the cable in (`by_clk`), or a timer under 2 ms away (`by_tmr`).
- **The CM4 can reach all of DRAM.** Its address mapping (`FUN_00002d40`/`FUN_00002d0a`) covers AP
  physical `0x80000000` to `0x80000000 + 0x587a4000`, so the 2 MB ring limit is only the
  kernel's preallocation.
- **The CPU clock is held at 1040 MHz in early suspend** (`mt_cpufreq_early_suspend`): DVFS is
  frozen because deep idle drives the CPU voltage itself on every entry and exit. A lower clock
  does not save energy here (598, 747.5 and 1040 MHz share one voltage); the A7 cores would just
  stay awake longer per refill.

Live, screen off, jack playback on the cable (2026-11-23): `hw:0,4` at 44.1 kHz S16_LE,
`period_size 5513`, `buffer_size 44104` — **1.0 s of music in a ring that can hold 11.9 s.**
`dpidle_mode=1`. The CM4 firmware itself warns when the DRAM ring is smaller than three of its
SRAM fills ("DRAM size is less than MAX SRAM size * 3").

## 3. What is powered during wired playback (screen off, stage 1, 2026-11-23)

| Part | State | Needed? |
|---|---|---|
| A7 cores (`VPROC`) | off between refills in ultra deep idle; 1040 MHz when awake | awake only to refill the ring |
| CM4 | on, 26 MHz, mostly waiting for an interrupt | yes: it is the player |
| AFE, AUD1PLL (44.1 kHz family) or AUD2PLL (48 kHz family), I2S | on | yes |
| DRAM (`VM`, `SYS_DPY`) | self-refresh, woken by the CM4 for each SRAM fill | yes; fill rate is set by the bit rate |
| Codec CXD3778GF (`VIBR`) | on, S-Master single-ended amp, charge pump | yes |
| Noise-cancelling MCU (Kinetis, `0-0010`) | deep sleep ("ucom may be in deep sleep") | no, already asleep |
| Connectivity chip (Bluetooth and Wi-Fi: `VCN28`, `VCN33*`, `VCN_1V8`, `SYS_CONN`) | **off** when the radio is off | no — and when on it vetoes ultra deep idle |
| FM tuner Si4708 | powered down | no |
| Display (`SYS_DIS`), GPU, video, ISP | off | no |
| Touch controller | early-suspend sleep (`himax_hx8526_ts_early_suspend`) | no |
| NFC reader | polling paused while the screen is dark (Cinder `nfc_stop`) | no |
| SD card (`VMCH`, `VMC`) | on | while the music is on the card |
| eMMC (`VIO18` and others) | idle between reads | yes |
| `VCAMD` 1.2 V, `VCAM_AF` 3.3 V | **on, with no known user** | see §5 step 6 |
| `VUSB`, USB PHY | on with the cable in | no; the cable also blocks deep idle (USB0) |
| PMIC, `VTCXO`, `VRTC`, `VA`, `VIO28` | on | yes |

The `VCAMD` and `VCAM_AF` regulators are the camera's on the MediaTek reference design. The
bootloader leaves them on, no driver but the absent camera's ever touches them, and the only
userspace reader is Sony's factory test `diag_main`, which switches every rail in turn. The
`/sys/devices/platform/mt-pmic/LDO_*_STATUS` nodes accept writes on this build (Sony's
`CONFIG_ARCH_MT8590_ICX` branch of `store_LDO_*_STATUS`, root, mode 0664), so they can be
switched off and on at run time without a kernel change.

## 4. The floor, and what sets it

Once the A7 cores sleep between refills, what is left is hardware that has to run for sound to
come out: the codec and its amplifier, the audio PLL, the I2S port, the CM4 at 26 MHz, the SRAM,
DRAM in self-refresh, the crystal, the PMIC, and the storage the music sits on.

Two things software still decides inside that floor:

- **How often DRAM wakes.** The CM4 refills 128 KB of SRAM at a time, so DRAM leaves
  self-refresh about every 0.74 s at 44.1 kHz 16-bit stereo (176 KB/s). A 24-bit file plays as
  32-bit words (352 KB/s at 44.1 kHz, 768 KB/s at 96 kHz): two to four times the DRAM wakes and
  the DMA traffic. The hardware floor is lowest for 16-bit 44.1 kHz.
- **How often the A7 cores wake.** Once per DRAM ring, nearly: 1 s today, 11.9 s with the full
  2 MB ring, longer with a kernel that preallocates more.

## 5. The plan, in order

Each step stands alone and is measured before the next. Steps 1–3 need no code.

1. **Bluetooth radio off while listening wired.** With the radio on, `SYS_CONN` stays powered and
   `dpidle_can_enter` refuses ultra deep idle outright (`by_oth`, measured 2026-10-11). This is the
   largest single item: the radio's own rails plus every refill and timer waking the A7 cores.
2. **Off the cable.** USB0 blocks deep idle (`by_clk`); every on-cable reading here is above the
   floor by definition.
3. **No software sound effects.** Sony's chain in Custom mode costs about 10 % of a core at
   1040 MHz against 0.6 % in Direct (2026-10-11). The codec's own EQ (DAC EQ) runs in the codec
   and costs the A7 cores nothing.
4. **A 2 MB ring (≈ 12 s at 16/44.1).** Cinder's mono shim already loads into Sony's sound service.
   Hooking `snd_pcm_hw_params_set_buffer_size_near` and `…_set_period_size_near` for `hw:0,4` only
   would cut A7 wakes from about one a second to one every ~10 s, with no change to Sony's
   effects or volume. To check on the player: pause and skip still answer at once (the HAL
   imports `snd_pcm_drop`), and Now Playing, the visualiser and the scrobble timestamps still
   follow the ear (correct by the ring delay, `snd_pcm_delay`, where they lead).
5. **A kernel with a larger ring** (patch: `buffer_bytes_max` and the preallocation in
   `mt8590-lp-audio.c`, e.g. 8 MB ≈ 47 s at 16/44.1, 11 s at 96/24). The CM4 maps all of DRAM, and
   8 MB is under 2 % of the player's 467 MB. Booted from RAM first, as every kernel test is.
6. **Switch `VCAMD` and `VCAM_AF` off** while playing, then on again (`echo 0 > …_STATUS`). With
   the owner holding the player: if something on the board does use them, its symptom has to be
   seen. Undone by `echo 1` or a restart.
7. **The undervolt** (`kernel/README.md`, patch 0010): each refill runs at 1040 MHz, and at 1090
   instead of 1150 mV that costs about 10 % less dynamic energy. Small once refills are rare.
8. **Play hi-res as 16-bit 44.1 kHz when power matters most** (optional, a Sound setting): a
   quarter of the DRAM traffic and wakes of 24-bit 96 kHz. This changes what you hear, so it is
   the user's choice, never a default.

Scrobbling is unaffected by all of it: Cinder writes the log at song boundaries, which arrive with
the refills, and step 4's delay correction keeps the play times right.

Not worth doing, with the reason:

- **A lower CPU clock during refills.** Same voltage at 598–1040 MHz, so it only stretches each
  refill.
- **True suspend to RAM during playback.** The audio clocks stop. Ultra deep idle is already the
  low-power state with the audio clocks kept; changing what suspend keeps means changing
  MediaTek's sleep firmware (`pcm_ultra_dpidle`), a binary blob.
- **Removing Sony's 10 Hz pollers for wired playback.** In ultra deep idle they do not wake the
  A7 cores; they run once per refill.
- **Replacing Sony's playback stack with Cinder's own straight to `hw:0,4`.** Step 4 reaches the
  same wake rate with Sony's effects kept. It is worth it only if Sony's per-refill work turns
  out to be large off the cable.

## 6. How to measure it

- **Wake rate:** `dpidle_cnt` and `dpidle_block_cnt` in `/sys/power/dpidle_state` over a minute
  off the cable (`tools/idle_probe.sh` waits for the unplug and samples on its own). Today's
  expectation is about one entry a second; after step 4, about one every ten.
- **Energy:** an off-cable voltage-decay A/B with `tools/battery_track.sh`, same album, same
  volume, one run per step.
- **Rails:** `/sys/devices/platform/mt-pmic/*_STATUS`, `/proc/clkmgr/subsys_test` (read only).

## Related

`RE_kernel_power.md` (wake sources), `RE_sony_idle_baseline.md`, `RE_codec_power.md`,
`RE_power_stock_vs_cinder.md`, `RE_sound_service.md` §4 (the HAL's `dpidle_state` writes),
`kernel/README.md` (booting a test kernel from RAM).
