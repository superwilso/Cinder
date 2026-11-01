/* play_motion.h — "is the track playing?", from the position callbacks and our own last command.
 *
 * PlayerService's state enum is not calibrated, so the shim answers from motion: the position moved
 * in the last PLAY_MOTION_MS. That alone is wrong for PLAY_MOTION_MS after a pause, because the last
 * movement is still recent, and a seek in that window resumed a track the user had just paused
 * (#50: pause, then ◁ to go back to 0:00, and it played). So a Pause or Stop this shim sent wins for
 * the same window; after it, motion decides again, which also catches a resume from somewhere else
 * (headphone buttons, Sony's own logic).
 *
 * Plain arithmetic on CLOCK_MONOTONIC milliseconds, host-tested by
 * cinder-home/tools/playmotion_selftest.cpp. */
#ifndef CINDER_PLAY_MOTION_H
#define CINDER_PLAY_MOTION_H

enum { PLAY_MOTION_MS = 2500 };

/* moved_at: when the position last changed (0 = never). halted_at: when this shim last sent Pause
 * or Stop, 0 when the last command was Play. */
static inline int play_motion_playing(long long now, long long moved_at, long long halted_at)
{
    if (moved_at == 0 || now - moved_at >= PLAY_MOTION_MS) return 0;
    if (halted_at != 0 && now - halted_at < PLAY_MOTION_MS) return 0;
    return 1;
}

#endif
