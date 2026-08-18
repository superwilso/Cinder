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

/* Start/stop ONLY the local audio path (AudioInPlayerService), leaving the tuner tuned and
 * playing. Bluetooth output needs hw:0,1, and AudioInPlayerService owns it while the radio is
 * audible on the jack — so the two cannot run together. Stop the audio path, bridge, and start it
 * again when Bluetooth output is switched off. */
int cinder_tuner_audio_start(void);
int cinder_tuner_audio_stop(void);

/* Retune while playing. Cheap — no need to stop and restart. 0 = ok. */
int cinder_tuner_set_khz(int khz);
/* What the tuner actually holds, in kHz. 0 if unavailable. Sony VALIDATES this setter: an
 * out-of-band value is rejected and the previous frequency kept, so a read-back is meaningful. */
int cinder_tuner_get_khz(void);

/* Is the tuner open? (TunerState: 0 = closed, 1 = open.) <0 if unavailable. */
int cinder_tuner_state(void);
/* Stereo lock indicator. Reads the chip's own ST bit when the register path is up; falls back to
 * Sony's GetStereoState, which read 0 on every station ever tested and so means little. */
int cinder_tuner_stereo(void);

/* ── The chip, directly ──────────────────────────────────────────────────────────────────────
 * Sony's driver publishes every Si470x register through the kernel register monitor at
 * /proc/regmon/Si4708icx. The nodes ship root-only; `cinder-fm` (setuid) widens them, and this
 * shim probes and uses them automatically. Everything below degrades to the audio-measured routes
 * if that path is unavailable, so none of it is a hard dependency. */

/* Is the register path live? 1 = the meter and hardware seek are real, 0 = audio measurement. */
int cinder_tuner_hw(void);

/* REAL signal strength — STATUS_RSSI[7:0], the graded meter Sony's GetSignalLevel is not
 * (it returns a constant 1 at every frequency in the band). <0 when the register path is down.
 *
 * SCALE, measured 2026-08-18 on this unit with the cable as aerial: noise floor 5-6, real
 * carriers 9-14. The theoretical range is 0..75 dBuV, but nothing here approaches that, so a UI
 * meter should saturate around 15 rather than 75 or it will never leave the first bar. */
int cinder_tuner_signal(void);

/* Is there real PCM on the source the Bluetooth bridge reads? Returns the RMS of `ms` (20..2000)
 * of capture from hw:0,1, 0..32767, or <0 if the path could not be opened.
 *
 * WHY IT EXISTS: FM audio is ANALOGUE into the codec and only becomes PCM because `analog input
 * device` routes it to the ADC. Get that wrong and the capture still opens, still returns frames,
 * and every one of them is silence — the same failure that cost two 45-minute "I hear nothing"
 * sessions on the local path. Near-zero here means the bridge would transmit silence.
 *
 * BORROWS hw:0,1: AudioInPlayerService owns it while the radio is audible on the jack, so the local
 * path is stopped for the duration and handed back. The radio goes briefly quiet. */
int cinder_tuner_capture_rms(int ms);

/* ── Scanning ────────────────────────────────────────────────────────────────────────────────
 * Fills `out_khz` with up to `max` station frequencies, strongest first, and returns how many.
 * Blocking either way — call it off the render thread.
 *
 * WITH the register path (cinder_tuner_hw() == 1): steps the chip and reads STATUS_RSSI, waiting
 * on STC. MEASURED at ~9 s for the whole band (206 steps, ~45 ms each — the chip's own tune time,
 * which no amount of code removes). Needs no capture PCM and may be called while the radio is
 * playing: the chip is muted for the sweep and unmuted after.
 *
 * WITHOUT it: falls back to measuring the AUDIO from the capture PCM and scoring it spectrally
 * (unlocked FM is broadband hiss, a locked carrier is not; level alone does not work because hiss
 * is often LOUDER). ~0.45 s per 100 kHz step, so around 90 s for the band — and in THAT mode the
 * tuner must NOT be started when this is called, because the scan needs hw:0,1 and
 * AudioInPlayerService holds it while playing.
 *
 * Sony's own primitives cannot do this at all, and both were checked against a station that was
 * audible at the time: GetSignalLevel returns 1 at every frequency, and StartAutoTuning is a
 * 48-byte stub that returns within 100 ms having found nothing in either direction. */
int cinder_tuner_scan(int start_khz, int end_khz, int *out_khz, int max);

/* Progress callback for the scan: called with 0..100 as it sweeps, so the UI can show real work
 * rather than a fake bar. May be NULL. */
typedef void (*cinder_tuner_progress_fn)(int pct);
void cinder_tuner_set_progress_cb(cinder_tuner_progress_fn cb);

/* LIVE SEEK — from `from_khz` in `dir` (-1/+1) until a station is found or the band wraps.
 * Returns the frequency it stopped on, or 0 if it found nothing. `on_step` is called with every
 * frequency passed through, so the dial SWEEPS instead of jumping.
 *
 * WITH the register path: the CHIP walks the band (POWERCFG SEEK/SEEKUP, polled on STC) and
 * on_step is driven from READCHAN as it moves. It needs no capture PCM, so the radio stays
 * AUDIBLE throughout and a seek can no longer leave the audio path stopped.
 *
 *   Why Sony's own seek never worked, and it is not a code defect: stock SEEKTH is 18 and no
 *   station here reads above 14, so the threshold sits above the whole band and every seek runs
 *   to the band limit. This lowers it, using the noise floor the last scan measured.
 *
 * WITHOUT it: steps the tuner and measures each step from the capture PCM, ~0.14 s per step. In
 * that mode the tuner must be STARTED first, and seek BORROWS hw:0,1 from the audio path — the
 * radio goes quiet for the duration and is handed back when it stops. */
typedef void (*cinder_tuner_step_fn)(int khz);
int cinder_tuner_seek(int from_khz, int dir, cinder_tuner_step_fn on_step);

/* ── CHUNKED scan and seek — use these from a UI thread ──────────────────────────────────────
 * cinder-home runs input, actions AND painting on one thread, so the blocking calls above freeze
 * the screen for their whole duration — ~10 s for a scan, 1-4 s for a seek. It also makes the
 * seek's dial sweep invisible, because the thread that would paint it is inside the loop.
 *
 * These do the identical work a slice at a time. One slice is about one channel — ~45 ms, the
 * chip's own tune time — so the UI keeps painting, the progress bar means something, and a scan
 * can actually be cancelled. Both are REGISTER-PATH ONLY: they return 0 from _begin when
 * cinder_tuner_hw() is 0, and the caller should fall back to the blocking calls above.
 *
 * Scan:  begin() -> step() per frame until it returns -1 -> collect().
 *        step() returns 0..99 progress; collect() fills out_khz strongest-first and returns how
 *        many. collect() ALSO ends the job and restores the chip, so it is the cancel path too:
 *        call it early and it simply peak-picks whatever was gathered.
 *
 * Seek:  begin() -> step(&cur) per frame. 0 = still walking and *cur is where the chip is right
 *        now (drive the dial with it), >0 = landed on that frequency, -1 = found nothing.
 *        abort() gives up and puts the chip back. */
int  cinder_tuner_scan_begin(int start_khz, int end_khz);
int  cinder_tuner_scan_step(void);
int  cinder_tuner_scan_collect(int *out_khz, int max);

int  cinder_tuner_seek_begin(int from_khz, int dir);
int  cinder_tuner_seek_step(int *cur_khz);
void cinder_tuner_seek_abort(void);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_TUNER_H */
