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
/* One analysis band, as Sony's service takes it: `hz` is the band's CENTRE frequency and `q` is
 * its quality factor (Sony calls the field `mean`; RE of SpectrumAnalyzer::UpdateCoefSet shows it
 * is used as alpha = tan(pi*f/fs) / q, which is exactly the RBJ bandpass alpha = sin(w0)/(2Q) in
 * the small-angle limit — so it IS Q, and a bigger number is a NARROWER filter).
 *
 * Sony's stock player passes q = 456, i.e. filters roughly 1/300 octave wide: twelve needles with
 * gaps between them, which is why the reported magnitudes jump three decades between frames. An
 * octave-wide band (the width a 12-band display implies) is q ~= 1.4.
 *
 * A band at or above fs/2 gets its coefficients ZEROED by the service and reads 0 for ever, so
 * anything above ~20 kHz is a dead column at 44.1/48 kHz. */
typedef struct { int hz; float q; } cinder_passband_t;

/* Install the passband table. Safe before start (stored and applied at start) and while running
 * (pushed to the service immediately, which recomputes the filter coefficients in place).
 * Returns 0 on success, -2 if SetPassband was not resolvable, -3 with no service instance.
 *
 * SONY CAPS THE ANALYZER AT 12 ACTIVE BANDS and there is no call that raises it: the service's
 * SpectrumAnalyzer builds ceil(12/5) = 3 level-detector objects ONCE, in its constructor, from a
 * hardcoded 12-entry default list, and SetPassband only re-assigns the vector — entries past the
 * 12th are never given a detector and are silently ignored. Passing more is harmless but useless.
 * Each call bumps the generation counter reported by cinder_analyzer_log_get, so a caller that
 * alternates two tables can tell which table a frame belongs to. */
int  cinder_analyzer_set_bands(const cinder_passband_t *bands, int n);

/* Set the detector window (SetCalcSamples) while running; 0 is ignored. This is the analyzer's
 * averaging time — the equivalent of a desktop analyser's "time window". */
int  cinder_analyzer_set_window(unsigned calc_samples);

/* ── Frame log (diagnostics; cinder-probe --vizlab) ────────────────────────────────────────────
 * A ring of the most recent frames with arrival timestamps and the passband generation that was
 * current when each arrived. This is what makes band placement, Q, window and update rate
 * MEASURABLE on device instead of guessed: the timestamps give the true emit rate, and the
 * generation tag says how many frames after a SetPassband are still filter transients. */
int  cinder_analyzer_log_count(void);              /* frames captured since the last reset */
int  cinder_analyzer_log_get(int idx, unsigned *ts_ms, int *gen, int *vals, int max);
void cinder_analyzer_log_reset(void);

/* Copy up to min(max,16) raw band values from the MOST RECENT frame into `out`; returns that
 * frame's true band count (n). Lets the probe print Sony's actual range/units so spectrum::from_bands
 * can be calibrated (dB-style vs linear). */
int  cinder_analyzer_last(int *out, int max);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_ANALYZER_H */
