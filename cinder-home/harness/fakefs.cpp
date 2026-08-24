// fakefs.cpp — the device's filesystem, as far as cinder-home can tell.
//
// Nearly everything the app knows about its hardware it reads with fopen from an absolute path:
// /sys/class/power_supply/battery/capacity, /sys/class/switch/cxd3778gf_h2w/state (the headphone
// jack), /sys/class/power_supply/usb/online, /contents/cinder_settings.conf, the resume queue. On a
// build machine none of those exist, so without this every scenario runs against a device with a
// flat battery, no charger and nothing plugged in — which is a fine default and a useless fixture.
//
// The whole thing is one override: `fopen`. Reads are served from a private tree if a file has been
// placed there, and fall through to the real filesystem otherwise, so an absent file still means
// absent. Writes are redirected into the tree when their directory has been created there, which is
// how "the app persisted its settings" and "the bad-boot counter was cleared" become assertable
// instead of being failures nobody notices.
//
// Every open is traced. That is not incidental: this project has already had to fix a config file
// that was opened, read and closed once per second for the life of the process (viz_analyzer_enabled,
// ~86k opens a day on the fragile vfat partition), and the trace is where the next one shows up.
#include "harness.h"

#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <string>
#include <vector>
#include <dirent.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <fcntl.h>
#include <unistd.h>

namespace {

std::string g_root;

// The private tree lives beside the harness build output so `run.sh` can clean it, and carries the
// pid so scenarios running side by side cannot see each other's device.
const std::string& root() {
    if (g_root.empty()) {
        char buf[256];
        const char* env = std::getenv("CINDER_HARNESS_FSROOT");
        if (env && *env) {
            g_root = env;
        } else {
            std::snprintf(buf, sizeof buf, ".harness/fs-%ld", (long)getpid());
            g_root = buf;
        }
        ::mkdir(".harness", 0755);
        ::mkdir(g_root.c_str(), 0755);
    }
    return g_root;
}

// mkdir -p, for a path already inside the private tree.
void mkdirs(const std::string& path) {
    for (size_t i = 1; i < path.size(); i++) {
        if (path[i] != '/') continue;
        ::mkdir(path.substr(0, i).c_str(), 0755);
    }
    ::mkdir(path.c_str(), 0755);
}

std::string mapped(const char* path) {
    if (!path || path[0] != '/') return std::string();
    return root() + path;
}

bool exists(const std::string& p) {
    struct stat st;
    return ::stat(p.c_str(), &st) == 0;
}

bool is_write_mode(const char* mode) {
    return mode && (std::strchr(mode, 'w') || std::strchr(mode, 'a') || std::strchr(mode, '+'));
}

std::string dirname_of(const std::string& p) {
    size_t i = p.find_last_of('/');
    return i == std::string::npos ? std::string() : p.substr(0, i);
}

// Recursive remove, for the tree this process created. Uses the REAL opendir/unlink — the harness
// overrides fopen only, so directory walking is untouched.
void rm_rf(const std::string& path) {
    DIR* d = ::opendir(path.c_str());
    if (d) {
        struct dirent* e;
        while ((e = ::readdir(d)) != nullptr) {
            if (!std::strcmp(e->d_name, ".") || !std::strcmp(e->d_name, "..")) continue;
            rm_rf(path + "/" + e->d_name);
        }
        ::closedir(d);
        ::rmdir(path.c_str());
        return;
    }
    ::unlink(path.c_str());
}

struct Cleanup {
    ~Cleanup() { if (!g_root.empty() && std::getenv("CINDER_HARNESS_FSROOT") == nullptr) rm_rf(g_root); }
} g_cleanup;

// A read is served from the private tree only if the file is actually there; a write only if its
// DIRECTORY is, so "the app tried to write somewhere that does not exist" stays a failure a
// scenario can be about (it is how the bad-boot counter's own failure path is reached).
bool resolve(const char* path, const char* mode, char* out, int cap) {
    if (!path || path[0] != '/') return false;
    const std::string m = mapped(path);
    if (m.empty()) return false;
    if (!(is_write_mode(mode) ? exists(dirname_of(m)) : exists(m))) return false;
    if ((int)m.size() + 1 > cap) return false;
    std::memcpy(out, m.c_str(), m.size() + 1);
    return true;
}

// Open without going through our own fopen override.
FILE* raw_open(const char* path, bool writing) {
    int fd = ::open(path, writing ? (O_WRONLY | O_CREAT | O_TRUNC) : O_RDONLY, 0644);
    if (fd < 0) return nullptr;
    FILE* f = ::fdopen(fd, writing ? "w" : "r");
    if (!f) ::close(fd);
    return f;
}

} // namespace

extern "C" {

void cinder_harness_fs_write(const char* path, const char* content) {
    const std::string m = mapped(path);
    if (m.empty()) return;
    mkdirs(dirname_of(m));
    FILE* f = raw_open(m.c_str(), true);
    if (!f) return;
    if (content) std::fwrite(content, 1, std::strlen(content), f);
    std::fclose(f);
}

// Pending scheduled changes, applied by the clock (cinder_harness_fs_due).
struct Pending { long long at; std::string path, content; };
std::vector<Pending>* g_pending = nullptr;

void cinder_harness_fs_write_at(long long at_ms, const char* path, const char* content) {
    if (!g_pending) g_pending = new std::vector<Pending>();
    Pending p;
    p.at = at_ms;
    p.path = path ? path : "";
    p.content = content ? content : "";
    g_pending->push_back(p);
}

// Called by the virtual clock every time it moves, BEFORE anything is allowed to observe the new
// time — so a scenario that schedules "the jack goes low at 60 s" gets a device where it went low
// at 60 s, not at the first tick after. THE CLOCK'S LOCK IS HELD HERE, hence the _locked recorder:
// the public one would take it again and deadlock.
void cinder_harness_fs_due(long long now_ms) {
    if (!g_pending) return;
    for (size_t i = 0; i < g_pending->size();) {
        if ((*g_pending)[i].at > now_ms) { i++; continue; }
        cinder_harness_fs_write((*g_pending)[i].path.c_str(), (*g_pending)[i].content.c_str());
        cinder_harness_record_locked((std::string("fs:") + (*g_pending)[i].path).c_str(), 0);
        g_pending->erase(g_pending->begin() + (long)i);
    }
}

void cinder_harness_fs_mkdir(const char* path) {
    const std::string m = mapped(path);
    if (!m.empty()) mkdirs(m);
}

int cinder_harness_fs_read(const char* path, char* buf, int cap) {
    const std::string m = mapped(path);
    if (m.empty() || cap <= 0) return -1;
    FILE* f = raw_open(m.c_str(), false);
    if (!f) return -1;
    size_t n = std::fread(buf, 1, (size_t)cap - 1, f);
    std::fclose(f);
    buf[n] = '\0';
    return (int)n;
}

} // extern "C"

// ── the override ─────────────────────────────────────────────────────────────────────────────
// main.o leaves `fopen` undefined, so this definition is the one the app gets. It cannot call the
// real fopen by name — that name is now this function — so it opens with `open` and wraps the
// descriptor with `fdopen`, neither of which the harness touches. No dlsym games, no recursion.
extern "C" FILE* fopen(const char* path, const char* mode) {
    char redirected[1024];
    const char* target = path;
    bool faked = false;
    if (path && mode && resolve(path, mode, redirected, (int)sizeof redirected)) {
        target = redirected;
        faked = true;
    }
    if (path && path[0] == '/') {
        // Traced because "opened once per second for the life of the process" is a defect this
        // project has already had to fix once, and it is invisible anywhere else.
        cinder_harness_record((std::string(faked ? "fopen:" : "fopen(absent):") + path).c_str(),
                              is_write_mode(mode) ? 1 : 0);
    }

    int flags;
    if (!mode || mode[0] == 'r') flags = std::strchr(mode ? mode : "", '+') ? O_RDWR : O_RDONLY;
    else if (mode[0] == 'w')     flags = O_CREAT | O_TRUNC  | (std::strchr(mode, '+') ? O_RDWR : O_WRONLY);
    else if (mode[0] == 'a')     flags = O_CREAT | O_APPEND | (std::strchr(mode, '+') ? O_RDWR : O_WRONLY);
    else                         flags = O_RDONLY;

    int fd = ::open(target, flags, 0644);
    if (fd < 0) return nullptr;
    FILE* f = ::fdopen(fd, mode ? mode : "r");
    if (!f) ::close(fd);
    return f;
}
