/* cinder_audio.h — C ABI over Sony's PlayerService client, for the Rust/C++ Cinder player.
 *
 * The C++ shim (player_shim.cpp) wraps pst::services::playerservice::PlayController so the
 * rest of Cinder (Rust cinder-ffi / cinder-home shell) can drive playback through a flat C
 * ABI — no libc++ types cross this boundary. Control calls are fully RE-verified; the
 * status read returns only the track URI for now (other PlayStatus fields await layout RE).
 */
#ifndef CINDER_AUDIO_H
#define CINDER_AUDIO_H
#ifdef __cplusplus
extern "C" {
#endif

/* Bootstrap: PlayerService::GetInstance() -> getPlayController(name) -> Connect(NULL).
 * `name` is the controller slot (e.g. "cinder"). 0 = ok, <0 = error. */
int  cinder_audio_init(const char *name);
/* Disconnect + drop the controller. */
void cinder_audio_shutdown(void);

/* Transport (ChangePlayState). 0 = ok, <0 = error. */
int  cinder_audio_play(void);
int  cinder_audio_pause(void);
int  cinder_audio_stop(void);

/* Track / group skip. NextGroup/PrevGroup move by ALBUM (the shuffle-by-album primitive). */
int  cinder_audio_next_track(void);
int  cinder_audio_prev_track(void);
int  cinder_audio_next_group(void);
int  cinder_audio_prev_group(void);

/* Seek to an absolute position from the start of the track, in milliseconds. */
int  cinder_audio_seek_ms(int ms);

/* Poll the current track URI into `buf` (NUL-terminated, truncated to cap). Returns the
 * length written, or <0 on error. This is the PlayStatus.uri field (offset +0x6c);
 * playstate / position / duration are NOT yet exposed (PlayStatus layout RE pending) —
 * until then, Cinder derives position from PlayerService elsewhere or polls less. */
int  cinder_audio_current_uri(char *buf, int cap);

/* DIAGNOSTIC (cinder-probe --discover): hex-dump the first 128 bytes of the raw PlayStatus struct
 * after GetCurrentStatus, into `buf` (NUL-terminated hex). Used on-device to map the position/
 * duration int offsets (play a track at a known elapsed time, then match the ms values in the dump).
 * Returns bytes written or <0. Read-only; frees the URI std::string at +0x6c like current_uri. */
int  cinder_audio_dump_status(char *buf, int cap);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_AUDIO_H */
