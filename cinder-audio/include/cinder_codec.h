/* cinder_codec.h — C ABI for putting the CXD3778GF DAC/amp to sleep when nothing is playing.
 *
 * THE PROBLEM. Measured 2026-09-03 on the reference device, idle: screen off, Bluetooth off, no
 * headphones in the jack, no PCM substream open anywhere in /proc/asound — and the codec was still
 * fully awake. Its ALSA controls read `standby off` and `deep early suspend off`, and its registers
 * confirmed it rather than just reporting a flag: blocks enabled (BLK_ON0=0x0F), three clock-enable
 * registers non-zero, the oscillator running, everything out of reset (SW_XRST0/1=0xFF), the charge
 * pump control bits set (CPCTL1=0x84) and the S-Master single-ended output path selected. That is
 * the headphone amplifier and its charge pump powered up to drive a jack with nothing in it.
 *
 * WHY IT HAPPENS. Sony's driver does implement standby, and implements it properly — writing the
 * control drops the chip so far that regmon can no longer read it over I2C at all (`invalid
 * length`). The audio path also CLEARS standby by itself: opening a PCM wakes the codec with no
 * help from userspace. What nothing does is put it BACK. The driver also registers a kernel
 * early-suspend hook (`cxd3778gf_early_suspend`), and on this system nothing drives the
 * early-suspend chain, so that route never fires either. So the codec wakes for the first sound
 * after boot and then stays awake until the device is switched off.
 *
 * WHY THIS IS SAFE. Because the audio path clears standby on PCM open, setting it while idle
 * cannot break playback — the worst case is the wake latency of the next track, and the driver
 * pays that already on the first play after boot. Verified end to end: standby on -> codec dead on
 * I2C -> `aplay` 1 s of silence -> `standby off` and every register back to its pre-standby value.
 *
 * WHAT THIS IS NOT. It does not fix the SoC's deep-idle block. That was tested directly and ruled
 * out: with the codec in standby for 30 s, `dpidle_cnt` stayed at 0 and `dpidle_block_cnt[by_vtg]`
 * kept climbing at the same ~240/s. The two are independent problems and should not be conflated.
 *
 * ACCESS. /dev/snd/controlC0 is owned by `system`, and cinder-home runs as uid 100, which IS
 * `system` on this device — so no setuid helper is needed. Controls are addressed BY NAME rather
 * than by numid, because numid is an ordering artefact of the driver's control registration and
 * would silently address the wrong control on a firmware that registers a different set. */
#ifndef CINDER_CODEC_H
#define CINDER_CODEC_H
#ifdef __cplusplus
extern "C" {
#endif

/* Is the codec currently in standby (powered down)?
 * Returns 1 = in standby, 0 = awake, -1 = unavailable (no control device / no such control). */
int cinder_codec_get_standby(void);

/* Put the codec into standby (on != 0) or take it out (on == 0). Returns 0 = applied, -1 = failed.
 *
 * Call with on=1 only when nothing is playing. Taking it out again is normally unnecessary — the
 * audio path does that on its own when a PCM is opened — but on=0 is available so a caller can be
 * explicit before starting playback rather than relying on that. */
int cinder_codec_set_standby(int on);

/* --- S-Master output gain mode -------------------------------------------------------------
 *
 * The CXD3778GF's class-D output stage has a high-gain setting. Sony's firmware ships it at
 * `normal` and never changes it, on any code path — so the extra output the amplifier is capable
 * of has simply never been available on this model. High gain is what drives higher-impedance
 * headphones to a usable level.
 *
 * WHICH CONTROL, AND WHY IT MATTERS. The driver registers three similarly named controls, and one
 * of them is mis-wired in Sony's firmware: `headphone smaster gain mode` (numid 28) and
 * `headphone smaster btl gain mode` (numid 30) BOTH point at
 * `cxd3778gf_put_headphone_smaster_btl_gain_mode_control` — read out of the driver's own
 * `cxd3778gf_snd_controls` table @0xc0851ac4. BTL is the balanced output, which this model does
 * not have wired (`HPOUT3_*` reads all zeros). So the only control that affects the 3.5 mm jack is
 * `headphone smaster se gain mode`, and that is the one these functions drive. Do not "simplify"
 * this to the shorter-looking name.
 *
 * IT APPLIES LIVE. `cxd3778gf_put_headphone_smaster_se_gain_mode` @0xc063960c stores the value and,
 * when `output device == headphone`, calls the output reconfiguration path — so the change takes
 * effect immediately rather than at the next startup.
 *
 * !! THIS IS A HEARING-SAFETY CONTROL, NOT A PREFERENCE. !!
 * Raising it makes every subsequent playback louder at the same volume setting, and the change
 * persists until something writes it back. A caller must not enable it as a default, silently, or
 * on behalf of a user who has not asked: the failure mode is somebody putting headphones on at
 * their usual volume and being hurt. Cinder keeps it OFF unless explicitly turned on, and puts it
 * back to `normal` if it cannot confirm the user's intent.
 *
 * How much louder is NOT established. `cxd3778gf_device_gain_table` @0xc0bc8958 holds
 * {0x60000, 0xf80000, 0x0} — plausibly Q16 +6.0/-8.0/0.0 dB — but no code reference to that table
 * was found, so the indexing is unknown and the dB figure is a guess. Treat it as "louder by an
 * unmeasured amount" until somebody measures it. */

/* 1 = high gain, 0 = normal, -1 = unavailable. */
int cinder_codec_get_gain_mode(void);

/* Select high gain (high != 0) or normal. Returns 0 = applied, -1 = failed.
 * Read the safety note above before calling this with a non-zero argument. */
int cinder_codec_set_gain_mode(int high);

/* Playback latency mode: 1 = low, 0 = normal, -1 = unavailable. Sony ships `normal` and never
 * changes it. Unlike the gain mode this is harmless either way; whether it is audible or
 * measurable on this hardware is untested. */
int cinder_codec_get_playback_latency(void);
int cinder_codec_set_playback_latency(int low);

/* Is anything in the headphone jack? 0 = empty, >0 = occupied (the codec's own plug detect, which
 * distinguishes 3pin/4pin/5pin/antenna), -1 = unavailable.
 *
 * This exists so a caller can interlock a gain change on an empty jack. It reads the codec's
 * detect rather than Sony's jack service on purpose: the question being asked is "is there a
 * transducer on the end of this amplifier", which is a property of the hardware, not of whatever
 * a service last cached. */
int cinder_codec_get_jack_se(void);

/* The codec's own master volume, 0..120 — the number every UI volume step ultimately moves.
 * Reading it is free; writing it changes how loud the device is, so a caller that writes it during
 * a measurement must put the original back.
 *
 * This is the input side of the output volume table: the table maps this 0..120 onto the analogue
 * attenuator (`0x49 PHV_L` / `0x4B PHV_R`), and reading PHV back at each step IS the curve. That
 * is how the two region tables can be told apart without listening to anything. */
int cinder_codec_get_master_volume(void);
int cinder_codec_set_master_volume(int v);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_CODEC_H */
