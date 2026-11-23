// preload_glue.cpp — see preload_glue.h. Moved out of main.cpp unchanged (2026-11-23).
#include "preload_glue.h"

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include "cinder.h"

// main.cpp's, whose own are inside its anonymous namespace.
void cinder_home_log(const char* m);
bool cinder_home_adv_on(const char* key);
static void clog_(const char* m) { cinder_home_log(m); }
static bool adv_on(const char* key) { return cinder_home_adv_on(key); }

// ── MONO beyond the LDAC bridge: libcinder_mono.so ───────────────────────────────────────────────
// The library (src/cinder-mono.c) runs inside Sony's SoundServiceFw and sums every stereo buffer on
// its way to the jack and to the Bluetooth transmitter while /tmp/cinder_mono exists. Cinder owns
// that file — it is the Mono switch — and reads the library's own marker, /tmp/cinder_mono_shim, to
// tell the Balance row whether mono reaches everything or only USB-DAC -> LDAC. Both live in tmpfs:
// a boot starts with neither, the library writes its marker when it loads, and this writes the flag
// from the saved setting.
void mono_flag_apply() {
    if (cinder_get_mono()) {
        int fd = ::open("/tmp/cinder_mono", O_WRONLY | O_CREAT | O_CLOEXEC, 0644);
        if (fd >= 0) ::close(fd);
    } else {
        ::unlink("/tmp/cinder_mono");
    }
}
// LONG RING (opt-in, `long_ring` in cinder_advanced.conf): the mono library asks for the largest
// low-power DRAM ring (2 MB, ~12 s at 16-bit 44.1 kHz) instead of Sony's 1 s, so in ultra deep idle
// the CM4 wakes the A7 cores ~ten times less often (analysis/RE_lowest_power_wired.md). Read when a
// stream opens, so it applies from the next song. Cost: sound-setting changes are heard as late as
// the ring is long.
void long_ring_flag_apply() {
    if (adv_on("long_ring")) {
        int fd = ::open("/tmp/cinder_long_ring", O_WRONLY | O_CREAT | O_CLOEXEC, 0644);
        if (fd >= 0) ::close(fd);
    } else {
        ::unlink("/tmp/cinder_long_ring");
    }
}
static int g_mono_shim_seen = -1;
int mono_shim_seen() { return g_mono_shim_seen; }
void mono_shim_poll() {
    const int up = ::access("/tmp/cinder_mono_shim", F_OK) == 0 ? 1 : 0;
    if (up == g_mono_shim_seen) return;
    g_mono_shim_seen = up;
    cinder_set_mono_shim(up);
    clog_(up ? "mono: libcinder_mono.so is loaded in SoundServiceFw — mono reaches the jack and Bluetooth"
             : "mono: libcinder_mono.so is not loaded — mono reaches USB-DAC -> LDAC only");
}

// ── PROOF FOR THE hagodaemon WRAPPER'S TRIAL (deploy/cinder-guard.sh) ───────────────────────────
// On a player with the `preload` component, src/cinder-hagowrap.c stands in front of every Sony
// service, and the boot guard takes it away again unless the last boot left proof that it works.
// The guard runs before anything is up and can only read files, so this writes them:
//   hago_up    this boot reached a healthy Cinder, so every Sony service started through it
//   hago_play  playback has been asked for and has not yet been heard to run
//   hago_ok    audio ran for five seconds through the sound service the wrapper preloads
// hago_play is the one that matters when it goes wrong: the first wrapper build took the sound
// service down at the first hw_params, a healthy boot later, and the player rebooted. A pause or an
// orderly power-off before the five seconds removes it — that is somebody stopping the music, not
// the music failing to start. Nothing here runs on a player without the wrapper.
static int g_hago = -1;        // -1 not looked yet, 0 no wrapper on this player, 1 there is one
static int g_hago_run_s = 0;
static bool g_hago_proven = false;   // audio has run this boot: nothing more to say about playback
static const char kHagoReal[] = "/system/vendor/sony/bin/hagodaemon.real";
static void hago_touch(const char* path) {
    int fd = ::open(path, O_WRONLY | O_CREAT | O_CLOEXEC, 0644);
    if (fd >= 0) ::close(fd);
}
static bool hago_watching() {
    // stat, not access: the off-device harness answers stat from its own tree and leaves access
    // real, and this is a rule worth being able to run there.
    if (g_hago < 0) { struct stat st; g_hago = ::stat(kHagoReal, &st) == 0 ? 1 : 0; }
    return g_hago == 1;
}
void hago_note_healthy() {
    if (!hago_watching()) return;
    hago_touch("/data/cinder/hago_up");
    clog_("wrapper: this boot came up through cinder-hagowrap (proof left for the boot guard)");
}
void hago_note_transport(bool playing) {
    if (g_hago_proven || !hago_watching()) return;
    g_hago_run_s = 0;
    if (playing) hago_touch("/data/cinder/hago_play");
    else ::unlink("/data/cinder/hago_play");
}
// 1 Hz. `running` is the real thing, not the intent: g_playing starts true on every boot.
void hago_tick(bool running) {
    if (g_hago_proven || !hago_watching()) return;
    if (!running) { g_hago_run_s = 0; return; }
    if (++g_hago_run_s < 5) return;
    hago_touch("/data/cinder/hago_ok");
    ::unlink("/data/cinder/hago_play");
    ::sync();
    g_hago_proven = true;
    clog_("wrapper: audio has run for 5 s through the preloaded sound service — confirmed for the boot guard");
}
