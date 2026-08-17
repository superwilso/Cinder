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
/* VPT surround MODE and DC Phase Linearizer FILTER TYPE. Raw ints: the enumerators are not
 * recovered yet, but both have an exported getter, so `cinder-probe --vpt` settles them by writing
 * a candidate and reading it back. Return -1 if the effect client is unavailable. */
int cinder_effects_set_vpt_mode(int mode);
int cinder_effects_get_vpt_mode(void);
int cinder_effects_set_dc_phase_type(int type);
int cinder_effects_get_dc_phase_type(void);
/* Which tone system is in the path (Equalizer vs Tone Control — Sony treats them as alternatives).
 * Raw int; enumerators unrecovered. `cinder-probe --eqsel` settles it by ear. */
int cinder_effects_get_eq_band(int i);
/* The 10-band's dB read-back — the CONTROL for every other dB getter, since its scale is the one
 * already measured on device (raw = half-decibels). */
float cinder_effects_get_eq_band_db(int i);
int cinder_effects_set_eq_band(int i, int gain);
int cinder_effects_set_select_using_eq(int t);
/* Which tone system is actually IN THE SIGNAL PATH — the same call, with the ordinals settled on
 * device 2026-08-17: 0 none, 1 the 6-band EQ, 2 the 10-band EQ, 3 Tone Control. Nothing had ever
 * called it, and the device sat on 1, so Cinder's 10-band was stored and never applied. */
int cinder_effects_set_tone_system(int sys);
#define CINDER_TONE_SYS_NONE 0
#define CINDER_TONE_SYS_EQ6  1
#define CINDER_TONE_SYS_EQ10 2
#define CINDER_TONE_SYS_TONE 3
int cinder_effects_get_select_using_eq(void);
int cinder_effects_set_dc_phase(int on);
int cinder_effects_set_dynamic_normalizer(int on);
int cinder_effects_set_vinylizer(int on);
int cinder_effects_set_clearaudio_plus(int on);
/* Goal #7: apply the whole effect chain to Bluetooth output too. */
int cinder_effects_set_bt_audio_effect(int on);

/* A/B compare: bypass != 0 disables the ENTIRE effect chain (B, "direct"); bypass == 0 re-enables
 * the previously-configured chain (A). Lets the Sound screen instantly A/B the DSP. 0 = ok, <0. */
int cinder_effects_set_bypass(int bypass);

/* ── the rest of Sony's effect surface ────────────────────────────────────────────────────────
 * Added 2026-08-17. Every symbol behind these was verified present in libEffectCtrlDmp.so's
 * dynamic table before being declared — see analysis/RE_dsp_effects_surface.md. All return
 * 0 (or the read value) on success and <0 if the effects client is not up.
 *
 * ENUM VALUES ARE NOT ALL SETTLED. Catalogue order (from the UTF-16BE .qm files) is almost
 * certainly enum order, but an echoed read-back does NOT bound an enum on this device — the
 * service stores whatever int it is handed. Anything marked "unsettled" below needs an ear test.
 */

/* Source Direct — bypasses the whole chain. OVERRIDES everything below it, like ClearAudio+. */
int cinder_effects_set_source_direct(int on);
int cinder_effects_is_source_direct(void);

/* Clear Phase (headphone). Speaker/Wmport describe hardware the A55 lacks and are not wired. */
int cinder_effects_set_clear_phase(int on);
int cinder_effects_is_clear_phase(void);

/* DSEE AI. Present in the API; whether the A50 has the hardware is UNVERIFIED — treat like the
 * high-gain finding (the write landing is not evidence it does anything). */
int cinder_effects_set_dsee_ai(int on);
int cinder_effects_is_dsee_ai(void);

/* DSEE HX Custom + its mode. Catalogue: Standard, Female Vocal, Male Vocal, Percussion, Strings. */
int cinder_effects_set_dsee_hx_custom(int on);
int cinder_effects_is_dsee_hx_custom(void);
int cinder_effects_set_dsee_hx_mode(int mode);
int cinder_effects_get_dsee_hx_mode(void);

/* Vinylizer character. Catalogue: Standard, Turntable Resonance, Arm Resonance, Surface Noise. */
int cinder_effects_set_vinylizer_type(int type);
int cinder_effects_get_vinylizer_type(void);

/* Tone Control — 3 bands (catalogue order BASS, MIDDLE, TREBLE), each with a selectable centre
 * frequency. Mutually exclusive with the 10-band EQ; cinder_effects_set_select_using_eq() picks
 * which of the two is actually in the path. Centre-frequency ordinals are UNSETTLED. */
int cinder_effects_set_tone_control(int on);
int cinder_effects_is_tone_control(void);
int cinder_effects_set_tone_value(int band, int gain);
int cinder_effects_get_tone_value(int band);
float cinder_effects_get_tone_value_db(int band); /* same reading, converted by the service */
int cinder_effects_set_tone_freq(int band, int f);
int cinder_effects_get_tone_freq(int band);

/* 6-band EQ — where Sony's NAMED presets live (Bright, Excited, Mellow, Relaxed, Vocal,
 * Custom 1, Custom 2). The 10-band Cinder drives has no presets of its own. */
int cinder_effects_set_eq6(int on);
int cinder_effects_is_eq6(void);
int cinder_effects_set_eq6_preset(int p);
int cinder_effects_get_eq6_preset(void);
int cinder_effects_set_eq6_band(int b, int gain);
/* The dB reading is COMPUTED by the service, so it settles the unit and the clamp without ears —
 * the raw value only echoes what it was handed. */
int cinder_effects_get_eq6_band(int b);
float cinder_effects_get_eq6_band_db(int b);

/* Sony's own two saved setups (Custom 1 / Custom 2). */
int cinder_effects_save_user_preset(int no);
int cinder_effects_load_user_preset(int no);

/* Read-back used to grey out rows something upstream is overriding. */
int cinder_effects_is_clearaudio_plus(void);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_EFFECTS_H */
