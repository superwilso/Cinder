// harness.cpp — trace recording, call scripting, and the virtual clock.
//
// The clock is the interesting part. main.cpp's frame loop is `do work; usleep(16000)`, and every
// pacing decision in the app (1 Hz housekeeping, the 1 Hz deferred retry, the 15 s BT radio-down
// window, the 30 s now-playing backstop) is read off CLOCK_MONOTONIC. So the harness defines
// usleep/sleep/clock_gettime/time itself: those symbols are undefined in main.o, and a definition
// in a linked object beats libc's. Sleeping does not wait — it ADVANCES a counter. A test that
// wants to know what the app is doing a minute into a Bluetooth session runs that minute in about
// a millisecond, and it runs it the same way every time.
//
// Only one thread may drive the clock, or two sleeping threads would advance it at twice the rate
// and the pacing under test would be a lie. The first thread to sleep claims it (that is the frame
// loop, which sleeps once per frame); every other thread — the main thread waiting out the budget,
// the detached healthy_timer — WAITS for virtual time to arrive instead of setting it.
//
// Waiters block on a condition variable rather than spinning. The first version had them
// sched_yield() in a loop, which worked when a scenario was run on its own and fell apart when the
// suite ran back to back: on a small container the spinning waiters starved the one thread that
// was allowed to move the clock, and a 20-second scenario burned the whole real-time budget
// reaching 9 virtual seconds. A busy-wait for a signal only another thread can send is a race
// against the scheduler, and this one lost.
#include "harness.h"

#include <pthread.h>
#include <sched.h>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <string>
#include <utility>
#include <vector>
#include <sys/syscall.h>
#include <unistd.h>
#include <time.h>

namespace {

struct Call { std::string name; long long arg; long long at_ms; };
struct Script { std::string name; std::vector<long long> vals; size_t next; };

pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
pthread_cond_t  g_tick = PTHREAD_COND_INITIALIZER;   // broadcast whenever the clock moves
std::vector<Call>*   g_trace  = nullptr;
std::vector<Script>* g_script = nullptr;
std::vector<std::pair<std::string, long long> >* g_state = nullptr;   // the faked UI state store

long long   g_now_ms   = 0;
pthread_t   g_clock_owner = 0;
bool        g_owner_set = false;
std::vector<pthread_t>* g_never_owner = nullptr;

struct Lock {
    Lock()  { pthread_mutex_lock(&g_lock); }
    ~Lock() { pthread_mutex_unlock(&g_lock); }
};

void ensure() {
    if (!g_trace)  g_trace  = new std::vector<Call>();
    if (!g_script) g_script = new std::vector<Script>();
    if (!g_never_owner) g_never_owner = new std::vector<pthread_t>();
    if (!g_state) g_state = new std::vector<std::pair<std::string, long long> >();
}

// The REAL clock — the harness's own watchdog cannot use the virtual one it is policing.
long long real_ms() {
    struct timespec ts;
    syscall(SYS_clock_gettime, CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

long long g_started_real = 0;

// A frame loop that stops sleeping (a spin, a deadlock, a blocking call we forgot to stub) would
// freeze virtual time and hang the waiters forever. Bound the whole run in REAL seconds.
const long long kRealBudgetMs = 60000;

// The trace printer with no locking of its own: both callers already hold g_lock — the public
// cinder_harness_dump takes it, and the watchdog fires from inside the wait loop, which holds it.
void dump_unlocked(int max_lines) {
    int n = 0;
    std::fprintf(stderr, "--- trace (%zu calls) ---\n", g_trace->size());
    for (auto& c : *g_trace) {
        if (max_lines > 0 && n++ >= max_lines) {
            std::fprintf(stderr, "  ... %zu more\n", g_trace->size() - (size_t)max_lines);
            break;
        }
        std::fprintf(stderr, "  %8lldms  %s(%lld)\n", c.at_ms, c.name.c_str(), c.arg);
    }
}

// Called with g_lock HELD (it is reached from inside the wait loop), so it must not take it again.
void watchdog_or_die() {
    if (g_started_real == 0) g_started_real = real_ms();
    if (real_ms() - g_started_real <= kRealBudgetMs) return;
    std::fprintf(stderr, "\nHARNESS: real-time budget exhausted at virtual t=%lldms — the frame "
                         "loop stopped advancing the clock (spin? unstubbed blocking call?).\n",
                 g_now_ms);
    dump_unlocked(200);
    std::fflush(stderr);
    _exit(3);
}

// Advance to `target` if this thread owns the clock; otherwise wait for it to get there.
void sleep_for(long long ms) {
    Lock l; ensure();
    const long long target = g_now_ms + ms;
    bool banned = false;
    for (size_t i = 0; i < g_never_owner->size(); i++)
        if (pthread_equal((*g_never_owner)[i], pthread_self())) banned = true;
    if (!g_owner_set && !banned) { g_owner_set = true; g_clock_owner = pthread_self(); }
    if (g_owner_set && pthread_equal(g_clock_owner, pthread_self())) {
        if (target > g_now_ms) { g_now_ms = target; pthread_cond_broadcast(&g_tick); }
        return;
    }
    while (g_now_ms < target) {
        // Bounded wait so the real-time watchdog still gets a chance to fire if the owner has
        // stopped moving the clock altogether.
        struct timespec until;
        syscall(SYS_clock_gettime, CLOCK_REALTIME, &until);
        until.tv_nsec += 50 * 1000000L;
        if (until.tv_nsec >= 1000000000L) { until.tv_sec++; until.tv_nsec -= 1000000000L; }
        pthread_cond_timedwait(&g_tick, &g_lock, &until);
        watchdog_or_die();
    }
}

} // namespace

extern "C" {

void cinder_harness_record(const char* name, long long arg) {
    Lock l; ensure();
    g_trace->push_back(Call{name ? name : "?", arg, g_now_ms});
}

int cinder_harness_scripted(const char* name, long long* out) {
    Lock l; ensure();
    for (auto& s : *g_script) {
        if (s.name != name) continue;
        if (s.vals.empty()) return 0;
        size_t i = s.next < s.vals.size() ? s.next : s.vals.size() - 1;
        s.next++;
        if (out) *out = s.vals[i];
        return 1;
    }
    return 0;
}

void cinder_harness_state_set(const char* key, long long value) {
    Lock l; ensure();
    for (size_t i = 0; i < g_state->size(); i++)
        if ((*g_state)[i].first == key) { (*g_state)[i].second = value; return; }
    g_state->push_back(std::make_pair(std::string(key), value));
}

long long cinder_harness_state_get(const char* key, long long fallback) {
    Lock l; ensure();
    for (size_t i = 0; i < g_state->size(); i++)
        if ((*g_state)[i].first == key) return (*g_state)[i].second;
    return fallback;
}

void cinder_harness_reset(void) {
    Lock l; ensure();
    g_trace->clear();
    g_script->clear();
    g_state->clear();
    g_now_ms = 0;
    g_owner_set = false;
    g_never_owner->clear();
    g_started_real = 0;
}

void cinder_harness_script(const char* name, long long value) {
    cinder_harness_script_seq(name, &value, 1);
}

void cinder_harness_script_seq(const char* name, const long long* vals, int count) {
    Lock l; ensure();
    for (auto& s : *g_script) {
        if (s.name == name) { s.vals.assign(vals, vals + count); s.next = 0; return; }
    }
    Script s; s.name = name; s.vals.assign(vals, vals + count); s.next = 0;
    g_script->push_back(s);
}

int cinder_harness_count(const char* name) {
    Lock l; ensure();
    int n = 0;
    for (auto& c : *g_trace) if (c.name == name) n++;
    return n;
}

long long cinder_harness_arg(const char* name, int nth) {
    Lock l; ensure();
    int n = 0;
    for (auto& c : *g_trace) if (c.name == name && n++ == nth) return c.arg;
    return 0;
}

long long cinder_harness_first_ms(const char* name) {
    Lock l; ensure();
    for (auto& c : *g_trace) if (c.name == name) return c.at_ms;
    return -1;
}

long long cinder_harness_last_ms(const char* name) {
    Lock l; ensure();
    long long r = -1;
    for (auto& c : *g_trace) if (c.name == name) r = c.at_ms;
    return r;
}

int cinder_harness_count_between(const char* name, long long from_ms, long long to_ms) {
    Lock l; ensure();
    int n = 0;
    for (auto& c : *g_trace)
        if (c.name == name && c.at_ms >= from_ms && c.at_ms < to_ms) n++;
    return n;
}

int cinder_harness_before(const char* a, const char* b) {
    long long fa = cinder_harness_first_ms(a), fb = cinder_harness_first_ms(b);
    if (fa < 0 || fb < 0) return -1;
    if (fa != fb) return fa < fb ? 1 : 0;
    // Same virtual millisecond — fall back to trace order, which is the real answer.
    Lock l; ensure();
    for (auto& c : *g_trace) {
        if (c.name == a) return 1;
        if (c.name == b) return 0;
    }
    return -1;
}

void cinder_harness_dump(int max_lines) {
    Lock l; ensure();
    dump_unlocked(max_lines);
}

void cinder_harness_clock_never_owner(void) {
    Lock l; ensure();
    g_never_owner->push_back(pthread_self());
}

long long cinder_harness_now_ms(void) { Lock l; return g_now_ms; }

// ── libc overrides: the virtual clock ────────────────────────────────────────────────────────
// These win over libc because main.o leaves them undefined and the linker takes the first
// definition it finds — ours, in this object.

int usleep(useconds_t us) {
    sleep_for((long long)(us / 1000));
    return 0;
}

unsigned int sleep(unsigned int sec) {
    sleep_for((long long)sec * 1000);
    return 0;
}

int clock_gettime(clockid_t clk, struct timespec* ts) {
    // glibc marks ts nonnull, so no null check — a caller passing null is already undefined.
    long long ms;
    { Lock l; ms = g_now_ms; }
    // Wall clock gets an epoch so date maths in the app sees a plausible year, monotonic starts
    // at zero like a freshly booted device.
    long long base = (clk == CLOCK_REALTIME) ? 1756000000LL : 0;
    ts->tv_sec  = base + ms / 1000;
    ts->tv_nsec = (ms % 1000) * 1000000;
    return 0;
}

time_t time(time_t* t) {
    long long ms; { Lock l; ms = g_now_ms; }
    time_t v = (time_t)(1756000000LL + ms / 1000);
    if (t) *t = v;
    return v;
}

// ── libc overrides: the device surface main.cpp shells out to ────────────────────────────────
// Recorded, not executed. `system("… cinder-msc usb-rescue")` on a build machine would at best do
// nothing and at worst find a same-named binary; in the trace it is a fact the test can assert on.

int system(const char* cmd) {
    cinder_harness_record("system", 0);
    if (cmd) { Lock l; ensure(); g_trace->back().name = std::string("system:") + cmd; }
    return 0;
}

FILE* popen(const char* cmd, const char* /*mode*/) {
    Lock l; ensure();
    g_trace->push_back(Call{std::string("popen:") + (cmd ? cmd : "?"), 0, g_now_ms});
    return nullptr;   // "the command is not available" — the app's own degraded path
}

int pclose(FILE*) { return -1; }

void* dlopen(const char* path, int) {
    Lock l; ensure();
    g_trace->push_back(Call{std::string("dlopen:") + (path ? path : "?"), 0, g_now_ms});
    return nullptr;   // no Sony .so on a build machine — the optional-service paths degrade
}

void* dlsym(void*, const char* sym) {
    Lock l; ensure();
    g_trace->push_back(Call{std::string("dlsym:") + (sym ? sym : "?"), 0, g_now_ms});
    return nullptr;
}

char* dlerror(void) { return const_cast<char*>("harness: no dynamic loading"); }
int   dlclose(void*) { return 0; }

} // extern "C"
