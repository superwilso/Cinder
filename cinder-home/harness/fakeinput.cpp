// fakeinput.cpp — a touchscreen and a button block, for a machine that has neither.
//
// cinder-home reads /dev/input/event* directly: `opendir`, `open` each node non-blocking,
// `EVIOCGABS` to find which one is the panel and what its coordinate range is, `EVIOCGRAB` to hold
// it, then a `read()` per node per frame. None of that exists on a build machine, so until this
// file the harness could boot the app and watch it run but could never touch it — every gesture,
// every button, the whole `carry_out` action surface, had no off-device exercise at all.
//
// The nodes are REAL FIFOs in the fake filesystem tree, and the harness holds their write ends. The
// app's own `read()` is untouched: it opens a path, gets a pipe, and reads `input_event` structs out
// of it exactly as it would from the driver. Only three things are faked around that — `opendir`
// (so /dev/input has entries), `ioctl` (so EVIOCGABS answers with the panel's range and EVIOCGRAB
// succeeds), and the schedule that writes the bytes at the right VIRTUAL time.
//
// Two nodes, matching the device: event0 is the button block (GPIO keys — Power, volume, transport)
// and event1 is the himax multi-touch panel. The app picks the panel by asking for
// ABS_MT_POSITION_X, so only event1 answers that.
#include "harness.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <string>
#include <vector>
#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

namespace {

// The app's own struct, byte for byte — it is compiled here by the same compiler, so the layouts
// match without anyone having to think about it.
struct ev_event { long tv_sec; long tv_usec; unsigned short type; unsigned short code; int value; };

const unsigned short EV_SYN = 0x00, EV_KEY = 0x01, EV_ABS = 0x03;
const unsigned short ABS_X = 0x00, ABS_Y = 0x01;
const unsigned short ABS_MT_POSITION_X = 0x35, ABS_MT_POSITION_Y = 0x36;
const unsigned short BTN_TOUCH = 0x14a;

// The panel is 480×800 and the app maps raw → UI through the range EVIOCGABS reports. Reporting
// exactly 0..480 / 0..800 makes that mapping the identity, so a scenario's coordinates are the
// coordinates the navigator sees and nobody has to reason about scaling.
const int PANEL_W = 480, PANEL_H = 800;

struct Node { std::string path; int wr_fd; bool is_touch; };
std::vector<Node>* g_nodes = nullptr;

struct Pending { long long at; int node; ev_event ev; };
std::vector<Pending>* g_pending = nullptr;

bool g_enabled = false;

// EVIOCGABS(abs) — the same _IOR('E', 0x40+abs, 24) the app computes by hand.
unsigned eviocgabs(unsigned abs) {
    return (2u << 30) | (24u << 16) | ((unsigned)'E' << 8) | (0x40u + abs);
}
const unsigned EVIOCGNAME_64 = (2u << 30) | (64u << 16) | ((unsigned)'E' << 8) | 0x06;

struct absinfo { int value, minimum, maximum, fuzz, flat, resolution; };

bool is_touch_fd(int fd) {
    if (!g_nodes) return false;
    for (size_t i = 0; i < g_nodes->size(); i++)
        if ((*g_nodes)[i].is_touch && (*g_nodes)[i].wr_fd >= 0) {
            // The app holds a DIFFERENT descriptor for the same FIFO, so identity has to come from
            // the file rather than the number: same device+inode means the same node.
            struct stat a, b;
            if (::fstat(fd, &a) == 0 && ::stat((*g_nodes)[i].path.c_str(), &b) == 0
                    && a.st_dev == b.st_dev && a.st_ino == b.st_ino)
                return true;
        }
    return false;
}

bool is_our_fd(int fd) {
    if (!g_nodes) return false;
    struct stat a;
    if (::fstat(fd, &a) != 0) return false;
    for (size_t i = 0; i < g_nodes->size(); i++) {
        struct stat b;
        if (::stat((*g_nodes)[i].path.c_str(), &b) == 0
                && a.st_dev == b.st_dev && a.st_ino == b.st_ino)
            return true;
    }
    return false;
}

void queue(long long at, int node, unsigned short type, unsigned short code, int value) {
    if (!g_pending) g_pending = new std::vector<Pending>();
    Pending p;
    p.at = at;
    p.node = node;
    p.ev.tv_sec = (long)(at / 1000);
    p.ev.tv_usec = (long)((at % 1000) * 1000);
    p.ev.type = type;
    p.ev.code = code;
    p.ev.value = value;
    g_pending->push_back(p);
}

} // namespace

extern "C" {

// Build the nodes. Must run before the app starts — it opens them once, during bring-up.
void cinder_harness_input_enable(void) {
    if (g_enabled) return;
    g_enabled = true;
    if (!g_nodes) g_nodes = new std::vector<Node>();

    cinder_harness_fs_mkdir("/dev/input");
    // The fake tree is private to this process; ask it where /dev/input landed by writing a marker
    // and reading the path back out of the resolver.
    static const char* kPaths[] = {"/dev/input/event0", "/dev/input/event1"};
    for (int i = 0; i < 2; i++) {
        char real[1024];
        // fs_write creates the file so the resolver will map the path; then it is replaced by a
        // FIFO, which is what makes a non-blocking read behave like a driver rather than a file.
        cinder_harness_fs_write(kPaths[i], "");
        if (!cinder_harness_fs_resolve(kPaths[i], "r", real, (int)sizeof real)) continue;
        ::unlink(real);
        if (::mkfifo(real, 0666) != 0) continue;
        // O_RDWR so this end never sees EOF and never blocks: with only a writer, the app's reader
        // would get EOF the moment we were idle, and the app treats that as "node closed".
        Node n;
        n.path = real;
        n.wr_fd = (int)syscall(SYS_openat, AT_FDCWD, real, O_RDWR | O_NONBLOCK, 0);
        n.is_touch = (i == 1);
        g_nodes->push_back(n);
    }
}

// A button. `code` is the RAW evdev code the device reports — 116 Power, 115/114 volume up/down,
// 106/105 next/prev, 35 the hold switch. `value` is 1 press, 0 release.
void cinder_harness_key_at(long long at_ms, int code, int value) {
    queue(at_ms, 0, EV_KEY, (unsigned short)code, value);
    queue(at_ms, 0, EV_SYN, 0, 0);
}

// A tap: contact down at (x, y), then lift. Coordinates are UI coordinates — the panel's reported
// range makes raw and UI the same thing here on purpose.
void cinder_harness_tap_at(long long at_ms, int x, int y) {
    queue(at_ms, 1, EV_ABS, ABS_MT_POSITION_X, x);
    queue(at_ms, 1, EV_ABS, ABS_MT_POSITION_Y, y);
    queue(at_ms, 1, EV_KEY, BTN_TOUCH, 1);
    queue(at_ms, 1, EV_SYN, 0, 0);
    // The lift is a separate frame, a couple of ticks later: a down and an up inside one frame is
    // not a tap the app can see, since it reads the whole node once per frame.
    queue(at_ms + 100, 1, EV_KEY, BTN_TOUCH, 0);
    queue(at_ms + 100, 1, EV_SYN, 0, 0);
}

// A swipe, delivered as `steps` intermediate frames so the app sees travel rather than teleporting.
void cinder_harness_swipe_at(long long at_ms, int x0, int y0, int x1, int y1, long long dur_ms) {
    const int steps = 8;
    queue(at_ms, 1, EV_ABS, ABS_MT_POSITION_X, x0);
    queue(at_ms, 1, EV_ABS, ABS_MT_POSITION_Y, y0);
    queue(at_ms, 1, EV_KEY, BTN_TOUCH, 1);
    queue(at_ms, 1, EV_SYN, 0, 0);
    for (int i = 1; i <= steps; i++) {
        const long long t = at_ms + dur_ms * i / steps;
        queue(t, 1, EV_ABS, ABS_MT_POSITION_X, x0 + (x1 - x0) * i / steps);
        queue(t, 1, EV_ABS, ABS_MT_POSITION_Y, y0 + (y1 - y0) * i / steps);
        queue(t, 1, EV_SYN, 0, 0);
    }
    queue(at_ms + dur_ms + 50, 1, EV_KEY, BTN_TOUCH, 0);
    queue(at_ms + dur_ms + 50, 1, EV_SYN, 0, 0);
}

// Called by the virtual clock every time it moves, with its lock held — same contract as the
// filesystem's scheduled changes, and for the same reason: an event has to be in the pipe before
// anything is allowed to observe the time it was supposed to arrive at.
void cinder_harness_input_due(long long now_ms) {
    if (!g_pending || !g_nodes) return;
    for (size_t i = 0; i < g_pending->size();) {
        const Pending& p = (*g_pending)[i];
        if (p.at > now_ms) { i++; continue; }
        if (p.node < (int)g_nodes->size() && (*g_nodes)[p.node].wr_fd >= 0)
            (void)!::write((*g_nodes)[p.node].wr_fd, &p.ev, sizeof p.ev);
        g_pending->erase(g_pending->begin() + (long)i);
    }
}

// When the next scheduled event is due, so the clock can stop there rather than jumping over it.
// Without this an event scheduled inside a frame's poll window would only be written when that
// window ended — up to a second late with the panel dark, where the app's own budget is 1000 ms.
int cinder_harness_input_next(long long* out) {
    if (!g_pending || g_pending->empty()) return 0;
    long long earliest = (*g_pending)[0].at;
    for (size_t i = 1; i < g_pending->size(); i++)
        if ((*g_pending)[i].at < earliest) earliest = (*g_pending)[i].at;
    if (out) *out = earliest;
    return 1;
}

// ── the overrides ────────────────────────────────────────────────────────────────────────────

// opendir, so /dev/input has entries. It cannot call the real one by name, and glibc's opendir does
// not go through our `open` (it uses an internal non-cancellable open), so this opens the directory
// by syscall and wraps the descriptor with fdopendir.
DIR* opendir(const char* path) {
    char redirected[1024];
    const char* target = path;
    if (cinder_harness_fs_resolve(path, "r", redirected, (int)sizeof redirected))
        target = redirected;
    const int fd = (int)syscall(SYS_openat, AT_FDCWD, target, O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0);
    if (fd < 0) return nullptr;
    DIR* d = ::fdopendir(fd);
    if (!d) ::close(fd);
    return d;
}

// ioctl, so the app can find the panel and grab it. Everything else is forwarded to the kernel.
int ioctl(int fd, unsigned long request, ...) {
    va_list ap;
    va_start(ap, request);
    void* arg = va_arg(ap, void*);
    va_end(ap);

    if (is_our_fd(fd)) {
        const bool touch = is_touch_fd(fd);
        if (request == eviocgabs(ABS_MT_POSITION_X) || request == eviocgabs(ABS_X)) {
            if (!touch) return -1;
            absinfo* a = (absinfo*)arg;
            std::memset(a, 0, sizeof *a);
            a->maximum = PANEL_W;
            return 0;
        }
        if (request == eviocgabs(ABS_MT_POSITION_Y) || request == eviocgabs(ABS_Y)) {
            if (!touch) return -1;
            absinfo* a = (absinfo*)arg;
            std::memset(a, 0, sizeof *a);
            a->maximum = PANEL_H;
            return 0;
        }
        if (request == EVIOCGNAME_64) {
            std::snprintf((char*)arg, 64, touch ? "himax-touchscreen" : "gpio-keys");
            return 0;
        }
        // EVIOCGRAB and anything else: succeed. A grab that FAILS is the app's "somebody else has
        // the panel" diagnostic, and pretending that here would be inventing a failure.
        return 0;
    }
    return (int)syscall(SYS_ioctl, fd, request, arg);
}

} // extern "C"
