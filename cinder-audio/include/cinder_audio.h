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

/* Drive pst::core::Framework's event looper on a background thread. REQUIRED BEFORE ANY OTHER
 * CALL HERE: Sony's client proxies are async and the reply is delivered by that looper, so with
 * no pump every call below leaves its out-param uninitialised and returns stack garbage (that was
 * the whole "playback does nothing" bug — Connect appeared to return a pointer, IsConnected read
 * as true, SetTrackSequence "failed with 99", and the service logged nothing at all).
 * Must be called only AFTER the easel app lifecycle has started the Framework (cinder-home does
 * it from deferred_up): calling it earlier constructs an unstarted singleton and Pump segfaults.
 * `interval_ms` <= 0 keeps the 20 ms default. 0 = ok, -1 = thread spawn failed,
 * -2 = disabled via CINDER_NOPUMP=1. Idempotent. */
int  cinder_audio_pump_start(int interval_ms);
void cinder_audio_pump_stop(void);
/* Change the pump period while running. The shell slows it down when the panel goes dark: nothing
 * on screen needs sub-100 ms IPC latency, and this thread otherwise wakes 50x/second for hours
 * while a track plays in a pocket. Takes effect on the next iteration. */
void cinder_audio_pump_set_interval(int interval_ms);
/* Pump iterations so far — 0 while playback is broken is the signature of a dead looper. */
unsigned cinder_audio_pump_ticks(void);

/* Bootstrap: PlayerService::GetInstance() -> getPlayController(name) -> Connect(listener).
 * `name` is the controller slot (e.g. "cinder"). Retries Connect with backoff and FAILS if the
 * service never acknowledges — a nonzero Connect means the listener was not registered, so no
 * position callbacks would ever arrive. 0 = ok, <0 = error (-3 = Connect never succeeded). */
int  cinder_audio_init(const char *name);
/* 1 once the service has acknowledged the Connect (IsConnected). Registration is async IPC:
 * calls made before this flips (SetTrackSequence, GetCurrentStatus) are rejected. */
int  cinder_audio_is_connected(void);
/* Real position/duration from the PlayEventListener (onPlayTimeUpdated). Returns 1 when at
 * least one time update has arrived, 0 before (outputs are -1 then). */
int  cinder_audio_position(int *cur_ms, int *total_ms);
/* Total listener callbacks seen. 0 after playback started = the listener vtable never fired. */
unsigned cinder_audio_listener_events(void);
/* Is audio REALLY playing? Derived from the position having moved in the last 2.5 s, not from the
 * shell's optimistic view of the last transport action it sent. */
int  cinder_audio_is_playing(void);
/* Raw onPlayStatusUpdated state int, encoding not yet calibrated. Diagnostic only. */
unsigned cinder_audio_play_state(void);
/* Engine-level unpause / pause. After play_tracks the OMX graph reaches OMX_StatePause with the
 * SoundService track created but silent; resume is the transition into Executing. 0 = ok. */
int  cinder_audio_resume(void);
int  cinder_audio_suspend(void);

/* Release the service-side player (and with it SoundService's single "Music" track). Called by
 * shutdown; exposed separately so a fresh session can reclaim a track leaked by a process that
 * died without shutting down. 0 = ok, <0 = no controller. */
int  cinder_audio_close_player(void);
/* ClosePlayer + Disconnect + drop the controller. */
void cinder_audio_shutdown(void);

/* Transport (ChangePlayState). 0 = ok, <0 = error. */
int  cinder_audio_play(void);
int  cinder_audio_pause(void);
int  cinder_audio_stop(void);
/* Stop playback AND drop the shim's pinned track sequence so PlayerService closes the current
 * media file (required before USB-MSC hands /contents to the PC — an open fd under /contents
 * makes the umount fail EBUSY and the PC sees no medium). Playback resumes only via a fresh
 * cinder_audio_play_tracks. */
int  cinder_audio_release_sequence(void);

/* Track / group skip. NextGroup/PrevGroup move by ALBUM (the shuffle-by-album primitive). */
int  cinder_audio_next_track(void);
int  cinder_audio_prev_track(void);
int  cinder_audio_next_group(void);
int  cinder_audio_prev_group(void);

/* DEV PROBE: re-hand PlayerService the last sequence while it is playing, touching no transport
 * state. `dup_after_current` inserts a copy of the current track behind it, approximating a queue
 * insert. Answers whether "Play Next" can change the queue without interrupting the current track —
 * PlayerService has no insert, so a new SetTrackSequence is the only way to do it. 0 = accepted. */
int  cinder_audio_reissue_sequence(int dup_after_current);

/* Repeat-one on/off (NodeTrackSequence::SetOneTrackMode). Sticky: applied to every sequence at
 * construction, and applied live to the current one if there is one. 0 = applied live, 1 = stored
 * only (nothing playing yet). Sony's OneTrackMode enum values are undocumented; 0/1 is assumed and
 * is DEVICE-UNVERIFIED. There is no known repeat-ALL primitive — see ROADMAP. */
int  cinder_audio_set_repeat_one(int on);

/* Seek to an absolute position from the start of the track, in milliseconds.
 * Returns 0 = SENT, -1 = no controller. NOT 0 = "worked": PlayController::SeekTime is void
 * (RE disasm @0x13200 discards the response slot), so a rejected seek is indistinguishable from
 * an accepted one here. Use the dev probe below to find out where it actually landed. */
int  cinder_audio_seek_ms(int ms);

/* Same, with media_origin_t selectable. Those enum values are UNVERIFIED (Begin=0/Current=1 is an
 * RE guess) and a wrong origin looks exactly like the 2026-07-28 bug — the progress bar follows
 * the finger, the audio does not follow the bar. Driven from the dev-channel
 * `echo "<origin> <ms>" > /tmp/cinder_seek.req` probe, which logs the resulting position. */
int  cinder_audio_seek_ms_origin(int origin, int ms);

/* Play a track list: hands PlayerService a NodeTrackSequence built from `count` absolute file
 * paths (play order), starting playback at index `start`. This is the play-a-selected-track/
 * album entry point — the shell feeds it the cinder_pending_play_* URIs after a
 * CINDER_ACT_PLAY_INDEX action. The sequence stays alive inside the shim until replaced or
 * shutdown (the service pulls tracks from it during playback).
 * 0 = ok, -1 = bad args/not connected, -2 = JSON->Node build failed, -3 = SetTrackSequence
 * rejected, else the ChangePlayState result. */
int  cinder_audio_play_tracks(const char* const* uris, int count, int start);

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
