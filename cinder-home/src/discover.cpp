// discover.cpp — see discover.h. Read-only on-device discovery dump; shared by cinder-probe and
// the dev cinder-home. Captures the facts that unblock the device-gated features in one run.
#include "discover.h"
#include "cinder_audio.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <unistd.h>
#include <dirent.h>
#include <ctime>

namespace {

const char* g_path = "/contents/cinder_discovery.txt";

// Append the output of a shell command to the report (and flush first so our own fprintf lines stay
// in order with the command's appended output). Best-effort: a missing tool just logs its error.
void run(const char* cmd) {
    char full[512];
    std::snprintf(full, sizeof full, "%s >> %s 2>&1", cmd, g_path);
    std::system(full);
}

// Append a section header to the report.
void section(FILE* f, const char* title) {
    std::fprintf(f, "\n===== %s =====\n", title);
    std::fflush(f);
}

// Dump a single sysfs/proc file's contents (one line, labelled). Read-only.
void cat1(FILE* f, const char* path) {
    FILE* g = std::fopen(path, "r");
    if (!g) return;
    char buf[256] = {0};
    size_t n = std::fread(buf, 1, sizeof buf - 1, g);
    std::fclose(g);
    if (n == 0) return;
    // trim trailing newline
    while (n && (buf[n - 1] == '\n' || buf[n - 1] == '\r')) buf[--n] = 0;
    std::fprintf(f, "  %s = %s\n", path, buf);
}

// For each entry under `dir`, cat the named child files (e.g. backlight brightness/max_brightness).
void scan(FILE* f, const char* dir, const char* const* files, int nfiles) {
    DIR* d = opendir(dir);
    if (!d) return;
    struct dirent* e;
    while ((e = readdir(d)) != nullptr) {
        if (e->d_name[0] == '.') continue;
        std::fprintf(f, " [%s/%s]\n", dir, e->d_name);
        for (int i = 0; i < nfiles; ++i) {
            char p[320];
            std::snprintf(p, sizeof p, "%s/%s/%s", dir, e->d_name, files[i]);
            cat1(f, p);
        }
    }
    closedir(d);
}

// Timed evdev key-code capture: open every /dev/input/event*, read for ~`secs` seconds, and log each
// key press as (code,value) so the physical-button → keycode map can be read off. ONLY call when no
// other reader owns the nodes (the probe; never cinder-home).
struct ev { long s, us; unsigned short type, code; int value; };
void capture_keys(FILE* f, int secs) {
    std::fprintf(f, "  PRESS EACH PHYSICAL BUTTON NOW (%d s): play/pause, |<<, >>|, vol+, vol-, "
                    "home, back, power, hold…\n", secs);
    std::fflush(f);
    int fds[16], nfd = 0;
    DIR* d = opendir("/dev/input");
    if (d) {
        struct dirent* e;
        while ((e = readdir(d)) && nfd < 16) {
            if (std::strncmp(e->d_name, "event", 5) != 0) continue;
            char p[64];
            std::snprintf(p, sizeof p, "/dev/input/%s", e->d_name);
            int fd = open(p, O_RDONLY | O_NONBLOCK);
            if (fd >= 0) fds[nfd++] = fd;
        }
        closedir(d);
    }
    time_t end = time(nullptr) + secs;
    while (time(nullptr) < end) {
        for (int i = 0; i < nfd; ++i) {
            ev evs[16];
            ssize_t n = read(fds[i], evs, sizeof evs);
            int cnt = n > 0 ? (int)(n / (ssize_t)sizeof(ev)) : 0;
            for (int k = 0; k < cnt; ++k) {
                if (evs[k].type == 0x01 /*EV_KEY*/ && evs[k].value != 0) {
                    std::fprintf(f, "  KEY event%d: code=%u (0x%x) value=%d\n",
                                 i, evs[k].code, evs[k].code, evs[k].value);
                    std::fflush(f);
                }
            }
        }
        usleep(50000);
    }
    for (int i = 0; i < nfd; ++i) close(fds[i]);
}

} // namespace

extern "C" void cinder_run_discovery(const char* path, int with_audio, int with_input) {
    if (path && *path) g_path = path;
    FILE* f = std::fopen(g_path, "w"); // truncate: a fresh report each run
    if (!f) return;
    time_t now = time(nullptr);
    std::fprintf(f, "CINDER DISCOVERY REPORT — %s", ctime(&now));
    std::fprintf(f, "(read-only; pull this file back to wire volume/keymap/progress/etc.)\n");
    std::fflush(f);

    section(f, "SYSTEM / PROPS");
    run("getprop | grep -iE 'sony|usb|board|product|model|build|hw|adb'");
    run("cat /proc/version");

    section(f, "MOUNTS (storage path for statvfs)");
    run("cat /proc/mounts");
    run("df");

    section(f, "ALSA TOPOLOGY (USB-DAC / output routing)");
    run("cat /proc/asound/cards");
    run("aplay -l");
    run("arecord -l");
    run("ls -l /dev/snd");
    run("cat /proc/asound/card0/pcm*/sub*/status 2>/dev/null");

    section(f, "VOLUME — amixer controls + values (the master-volume control name + range)");
    run("amixer scontrols");
    run("amixer contents");

    section(f, "BACKLIGHT / BRIGHTNESS (sysfs path + range)");
    {
        const char* files[] = {"brightness", "max_brightness", "actual_brightness", "type"};
        scan(f, "/sys/class/backlight", files, 4);
        const char* lf[] = {"brightness", "max_brightness"};
        scan(f, "/sys/class/leds", lf, 2);
    }

    section(f, "POWER / BATTERY (charge nodes; battery-care)");
    {
        const char* files[] = {"capacity", "status", "health", "present", "charge_type",
                               "constant_charge_voltage", "charge_control_limit"};
        scan(f, "/sys/class/power_supply", files, 7);
    }

    section(f, "USB GADGET CONFIG (USB-DAC / MSC switching)");
    run("getprop sys.usb.config; getprop sys.sony.config; getprop persist.sys.usb.config");
    run("cat /sys/class/android_usb/android0/functions /sys/class/android_usb/android0/state "
        "/sys/class/android_usb/android0/idProduct 2>/dev/null");

    section(f, "INPUT DEVICES (button → keycode map)");
    run("cat /proc/bus/input/devices");
    run("ls -l /dev/input");

    if (with_audio) {
        section(f, "PLAYSTATUS BYTES (position/duration offsets — play a track first!)");
        char hex[1024];
        int n = cinder_audio_dump_status(hex, sizeof hex);
        if (n > 0) std::fprintf(f, "%s\n", hex);
        else std::fprintf(f, "  (no status: n=%d — is a track loaded/playing?)\n", n);
        std::fflush(f);
    }

    if (with_input) {
        section(f, "KEYMAP CAPTURE");
        capture_keys(f, 12);
    }

    section(f, "END");
    std::fclose(f);
}
