/* cinder_analyzer.h — C ABI over Sony's built-in spectrum analyzer
 * (libAudioAnalyzerServiceClient.so -> pst::services::audioanalyzerservice::AudioAnalyzerService).
 *
 * This is the REAL audio-reactive visualiser source: Sony's analyzer does the FFT itself and emits
 * per-band magnitudes via an IEventListener callback (OnSpectrumUpdate(vector<int>)). The shim
 * registers a listener and forwards each spectrum frame to cinder_set_spectrum() (cinder.h), so
 * the visualiser shows the actual audio with NO FFT cost on our side. The stock HgrmMediaPlayerApp
 * uses the exact same API for its spectrum-analyzer screen, so the path is known-good on device.
 *
 * SAFETY (this is a Sony-service call, and the analyzer is OPTIONAL eye-candy):
 *  - The library is loaded with dlopen at runtime; if it is absent or a symbol is missing, start
 *    returns a negative code and the feature is simply OFF (the synthetic visualiser keeps running).
 *    It is therefore NOT a link dependency of cinder-home — it can never block the boot binary's
 *    dynamic load (contrast the EQ effect shim, which links directly).
 *  - Every entry point must still be called from behind cinder-home's run_guarded crash+hang guard,
 *    and validated with `cinder-probe --analyzer` on device BEFORE it is enabled in the boot path.
 *    It is default-OFF (gated by /contents/cinder_viz.conf: `analyzer=1`).
 *
 * The AudioAnalyzerService instance comes from a Sony factory (GetInstance) — Sony-allocated, so
 * the object-sizing rule does not apply. The only ABI we reproduce is the IEventListener vtable
 * ([~dtor, deleting-dtor, OnLevelUpdate, OnSpectrumUpdate]); RE-confirmed in analyzer_shim.cpp. */
#ifndef CINDER_ANALYZER_H
#define CINDER_ANALYZER_H
#ifdef __cplusplus
extern "C" {
#endif

/* mode_t for cinder_analyzer_start(): the analyzer emits either an overall level (per the
 * delay_*_level_* params) or a spectrum (delay_*_spectrum_*). We want SPECTRUM for the bars.
 * Values inferred from RE (listener slot order Level<Spectrum + param-file naming); the probe can
 * sweep them if a device check shows no OnSpectrumUpdate callbacks. */
typedef enum { CINDER_ANALYZER_LEVEL = 0, CINDER_ANALYZER_SPECTRUM = 1 } cinder_analyzer_mode_t;

/* Connect to AudioAnalyzerService and start streaming spectrum frames into cinder_set_spectrum().
 *   mode          : CINDER_ANALYZER_SPECTRUM for the visualiser (see above).
 *   update_hz     : emit rate; <=0 leaves the service default. ~20 Hz is plenty for the display.
 *   calc_samples  : FFT window the service uses; 0 leaves the service default.
 * Returns 0 on success, or a negative stage code so the probe can report exactly where it failed:
 *   -1 dlopen failed (lib absent)      -2 a required symbol was missing (dlsym)
 *   -3 GetInstance returned NULL       -4 already started
 * Calls Set* only when the corresponding arg is > 0, so the safest invocation is
 * (CINDER_ANALYZER_SPECTRUM, 0, 0) — let the service use the same params the stock app does. */
int  cinder_analyzer_start(int mode, float update_hz, unsigned calc_samples);

/* Stop streaming (calls AudioAnalyzerService::Stop). Safe to call when not started. */
void cinder_analyzer_stop(void);

/* 1 if a stream is currently started, else 0. */
int  cinder_analyzer_is_running(void);

/* Diagnostics for `cinder-probe --analyzer` (on-device validation + calibrating Sony's band units):
 * number of spectrum frames received so far. 0 after start succeeded => the mode is wrong or no
 * audio is playing. */
int  cinder_analyzer_frames(void);
/* Copy up to min(max,16) raw band values from the MOST RECENT frame into `out`; returns that
 * frame's true band count (n). Lets the probe print Sony's actual range/units so spectrum::from_bands
 * can be calibrated (dB-style vs linear). */
int  cinder_analyzer_last(int *out, int max);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_ANALYZER_H */
