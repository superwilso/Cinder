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

#ifdef __cplusplus
}
#endif
#endif /* CINDER_CODEC_H */
