// playmotion_selftest — host test for cinder-audio/include/play_motion.h: when the shim says a
// track is playing. The case that mattered (#50): a Pause sent a second ago, with the position's
// last movement still recent, must read as paused, or a seek resumes the track.
#include "play_motion.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

int main() {
    const long long t = 100000;
    check(play_motion_playing(t, t - 900, 0) == 1, "moving, last command Play: playing");
    check(play_motion_playing(t, 0, 0) == 0, "never moved: not playing");
    check(play_motion_playing(t, t - PLAY_MOTION_MS, 0) == 0, "stopped moving a window ago: not playing");
    check(play_motion_playing(t, t - 900, t - 1000) == 0,
          "#50: paused a second ago, last movement still recent: paused");
    check(play_motion_playing(t, t - 100, t - 200) == 0, "a stray update right after the Pause: paused");
    check(play_motion_playing(t, t - 300, t - PLAY_MOTION_MS) == 1,
          "moving again a window after our Pause (headphone button): playing");
    return fails ? 1 : 0;
}
