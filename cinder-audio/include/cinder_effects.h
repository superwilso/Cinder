/* cinder_effects.h — C ABI over Sony's EffectCtrlDmp (libEffectCtrlDmp.so), the sound-effect
 * controller. Lets the Cinder UI drive the real DSP (EQ + the effect chain) the same way the
 * audio shim drives playback. See cinder-audio/src/effect_abi.hpp + analysis/RE_playerservice_sound.md.
 *
 * SAFETY: the EffectCtrlDmp object is constructed lazily on first use (it connects to the sound
 * service). If that service isn't reachable the construction can crash — so the SHELL must call
 * these from behind its crash+hang guard (cinder-home's run_guarded), exactly like the audio
 * IPC. A failure leaves the UI running (effects just don't apply); it can't brick. */
#ifndef CINDER_EFFECTS_H
#define CINDER_EFFECTS_H
#ifdef __cplusplus
extern "C" {
#endif

/* Apply the 10-band EQ: enables the 10-band EQ and sets each band gain (dB). `bands` points to
 * `n` (<=10) signed bytes. 0 = ok, <0 = effect controller unavailable. */
int cinder_effects_set_eq(const signed char *bands, int n);

/* Individual effect toggles (wire from the Sound screen). 0 = ok, <0 = unavailable. */
int cinder_effects_set_dsee_hx(int on);
int cinder_effects_set_vpt(int on);
int cinder_effects_set_dc_phase(int on);
int cinder_effects_set_dynamic_normalizer(int on);
int cinder_effects_set_vinylizer(int on);
int cinder_effects_set_clearaudio_plus(int on);
/* Goal #7: apply the whole effect chain to Bluetooth output too. */
int cinder_effects_set_bt_audio_effect(int on);

/* A/B compare: bypass != 0 disables the ENTIRE effect chain (B, "direct"); bypass == 0 re-enables
 * the previously-configured chain (A). Lets the Sound screen instantly A/B the DSP. 0 = ok, <0. */
int cinder_effects_set_bypass(int bypass);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_EFFECTS_H */
