/* cinder_tuner.h — C ABI over Sony's FM tuner (Silicon Labs Si4708).
 *
 * TWO SERVICES, and both are required — this is the part that is easy to get wrong:
 *
 *   TunerPlayerService   tunes the chip   (Open / SetFrequency / Play / Stop / Close)
 *   AudioInPlayerService carries the audio (captures the codec ADC and creates a track)
 *
 * …plus a MIXER control that neither of them touches. The working sequence, confirmed by ear on
 * 2026-08-18 (97.3 MHz, listenable):
 *
 *     1. 'analog input device' = tuner        <-- REQUIRED
 *     2. Tuner   Open -> SetFrequency -> Play
 *     3. AudioIn Play
 *
 * Miss step 1 and the whole path is SILENT on a strong carrier while every call returns 0 — the
 * capture side is reading a dead ADC. See analysis/RE_fm_tuner.md.
 *
 * SAFETY: every entry point is a Sony service call, so the shell must invoke them from behind its
 * crash+hang guard exactly like the audio and effect shims. Getting an argument wrong here has
 * already rebooted the device once (AudioIn Play("tuner") — "tuner" is not a valid track name). */
#ifndef CINDER_TUNER_H
#define CINDER_TUNER_H
#ifdef __cplusplus
extern "C" {
#endif

/* Bring the tuner up and start playing `khz`. Does all three steps above. 0 = ok, <0 = failed.
 * FM NEEDS THE HEADPHONE CABLE AS ITS AERIAL — with an empty jack every frequency is noise. */
int cinder_tuner_start(int khz);
/* Tear down: AudioIn Stop, tuner Stop + Close, and put the mixer route back. 0 = ok. */
int cinder_tuner_stop(void);

/* Retune while playing. Cheap — no need to stop and restart. 0 = ok. */
int cinder_tuner_set_khz(int khz);
/* What the tuner actually holds, in kHz. 0 if unavailable. Sony VALIDATES this setter: an
 * out-of-band value is rejected and the previous frequency kept, so a read-back is meaningful. */
int cinder_tuner_get_khz(void);

/* Is the tuner open? (TunerState: 0 = closed, 1 = open.) <0 if unavailable. */
int cinder_tuner_state(void);
/* Stereo lock indicator. NOTE: read 0 on every station tested so far, so treat a 0 as "unknown"
 * rather than "definitely mono" until it has been seen to move. */
int cinder_tuner_stereo(void);

/* ── Scanning ────────────────────────────────────────────────────────────────────────────────
 * Sony's own primitives CANNOT find a station on this hardware, and both were checked against a
 * station that was audible at the time:
 *   * GetSignalLevel returns 1 at every frequency in the band.
 *   * StartAutoTuning returns within 100 ms having found nothing, in both directions.
 * So the scan measures the AUDIO from the capture PCM and scores it spectrally: unlocked FM is
 * broadband hiss, a locked carrier is not. Level alone does not work — hiss is often LOUDER.
 *
 * Fills `out_khz` with up to `max` station frequencies, best first, and returns how many. The
 * tuner must NOT be started when this is called: the scan owns the capture PCM and
 * AudioInPlayerService holds it while playing. Scan first, then start. Blocking, ~0.45 s per
 * 100 kHz step, so a full band sweep is around 90 s — call it off the render thread. */
int cinder_tuner_scan(int start_khz, int end_khz, int *out_khz, int max);

/* Progress callback for the scan: called with 0..100 as it sweeps, so the UI can show real work
 * rather than a fake bar. May be NULL. */
typedef void (*cinder_tuner_progress_fn)(int pct);
void cinder_tuner_set_progress_cb(cinder_tuner_progress_fn cb);

/* LIVE SEEK — step from `from_khz` in `dir` (-1/+1) until a station is found or the band ends.
 * Returns the frequency it stopped on, or 0 if it found nothing.
 *
 * This is what a Sony seek looks like on hardware whose own auto-tune does not work: the shell
 * steps the tuner and measures each step, and `on_step` is called with every frequency it passes
 * through so the dial SWEEPS instead of jumping. Roughly 0.14 s per 100 kHz step, so crossing the
 * whole band is a few seconds — visible, which is the point.
 *
 * The tuner must be STARTED (cinder_tuner_start) first. Seek borrows the capture PCM from the
 * audio path and hands it back when it stops. */
typedef void (*cinder_tuner_step_fn)(int khz);
int cinder_tuner_seek(int from_khz, int dir, cinder_tuner_step_fn on_step);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_TUNER_H */
