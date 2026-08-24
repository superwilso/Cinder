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
// Advancing it is discrete-event scheduling, and it took two wrong versions to get there:
//
//   1. "One owner, claimed by the first thread to sleep." The frame loop sleeps once per frame, so
//      it was expected to claim it. It usually did — but the app's own healthy_timer is a detached
//      thread created BY the frame loop one statement before the frame loop's first usleep, and it
//      sleeps for nine seconds and then exits. Lose that race and the clock jumped to 9 s in one
//      step and then belonged to a dead thread forever. It passed locally and hung in CI.
//   2. Waiters spinning on sched_yield() while they waited for the owner. On a small container the
//      spinners starved the one thread allowed to move the clock; a 20-second scenario burned the
//      whole real-time budget reaching 9 virtual seconds.
//
// So there is no owner. Every sleeping thread registers the virtual time it wants to wake at, and
// the clock jumps to the EARLIEST of those — which is what a discrete-event simulator does, and is
// correct no matter which threads are sleeping. It only jumps once nothing is still executing app
// code, tracked by a per-thread flag that `cinder_harness_record` sets: a thread that is running
// has not yet said when it wants to wake, and jumping past it is exactly the bug above. A thread
// that exits without ever sleeping again would block that forever, so quiescence in REAL time is
// the fallback — 20 ms, against frames that take microseconds.
#include "harness.h"

#include <pthread.h>
#include <sched.h>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <map>
#include <string>
#include <utility>
#include <vector>
#include <poll.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <time.h>

namespace {

// A trace entry. The NAME IS AN INTERNED ID, not a string: a six-virtual-hour scenario with the
// panel on is 1.3 million frames and several million calls, and storing a std::string per entry
// spent more time in the allocator than in the app. Interning made the suite five times faster and
// the long scenarios possible at all.
struct Call { int name_id; long long arg; long long at_ms; };
struct Script { std::string name; std::vector<long long> vals; size_t next; };

pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
pthread_cond_t  g_tick = PTHREAD_COND_INITIALIZER;   // broadcast whenever the clock moves
// Passive waiters (the harness's own main thread) poll on this one, which is never signalled. They
// are waiting for the END of the run, so waking them on every clock move — 1.3 million times in a
// six-hour screen-on scenario — is pure cost for a question whose answer changes once.
pthread_cond_t  g_slow = PTHREAD_COND_INITIALIZER;
// The earliest virtual time a PASSIVE waiter is waiting for. They are not woken by every clock
// move, so the clock wakes them when it crosses this — otherwise a run overshot its budget by
// however far the frame loop got during one 20 ms poll, which at these speeds was tens of seconds.
long long g_passive_target = 0;
std::vector<Call>*   g_trace  = nullptr;
std::vector<Script>* g_script = nullptr;
std::vector<std::string>* g_names = nullptr;                 // id -> name
std::map<std::string, int>* g_name_id = nullptr;             // name -> id
std::vector<std::pair<std::string, long long> >* g_state = nullptr;   // the faked UI state store

long long   g_now_ms   = 0;

// Per-thread role. 0 = not seen yet, RUNNING = executing app code, SLEEPING = parked with a wake
// target registered, PASSIVE = the harness's own main thread, which waits out the run budget and
// must never influence the clock (its target is the whole scenario; letting it count would jump
// straight to the end).
enum { T_NEW = 0, T_RUNNING = 1, T_SLEEPING = 2, T_PASSIVE = 3 };
__thread int t_role = T_NEW;

struct Waiter { pthread_t th; long long target; };
std::vector<Waiter>* g_waiters = nullptr;   // threads parked with a wake target
int       g_running = 0;                    // threads known to be executing app code right now
long long g_last_move_real = 0;             // real ms of the last clock jump — the stall fallback

struct Lock {
    Lock()  { pthread_mutex_lock(&g_lock); }
    ~Lock() { pthread_mutex_unlock(&g_lock); }
};

void ensure() {
    if (!g_trace)  g_trace  = new std::vector<Call>();
    if (!g_script) g_script = new std::vector<Script>();
    if (!g_waiters) g_waiters = new std::vector<Waiter>();
    if (!g_state) g_state = new std::vector<std::pair<std::string, long long> >();
    if (!g_names) g_names = new std::vector<std::string>();
    if (!g_name_id) g_name_id = new std::map<std::string, int>();
}

extern "C" void cinder_harness_fs_due(long long now_ms);   // fakefs.cpp
extern "C" void cinder_harness_input_due(long long now_ms);   // fakeinput.cpp
extern "C" int  cinder_harness_input_next(long long* out);    // fakeinput.cpp

// Name -> id, creating the id if this is the first time. Caller holds the lock.
int intern(const char* name) {
    const std::string key(name ? name : "?");
    std::map<std::string, int>::iterator it = g_name_id->find(key);
    if (it != g_name_id->end()) return it->second;
    const int id = (int)g_names->size();
    g_names->push_back(key);
    (*g_name_id)[key] = id;
    return id;
}

// …and the read-only direction, for the query side: -1 means "nothing was ever recorded under that
// name", which every counter below then reports as zero rather than inventing an id for it.
int name_id_of(const char* name) {
    if (!g_name_id) return -1;
    std::map<std::string, int>::const_iterator it = g_name_id->find(std::string(name ? name : "?"));
    return it == g_name_id->end() ? -1 : it->second;
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
        std::fprintf(stderr, "  %8lldms  %s(%lld)\n", c.at_ms, (*g_names)[c.name_id].c_str(), c.arg);
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

// This thread is about to execute app code. Called from cinder_harness_record (with the lock held),
// so any thread that touches the app is counted even if it has never slept — which is the whole
// point: the frame loop is mid-frame, not parked, at the moment healthy_timer first sleeps.
void mark_running() {
    if (t_role == T_NEW) { t_role = T_RUNNING; g_running++; }
}

// Jump to the earliest wake time anybody is waiting for, if nothing is still running. Caller holds
// the lock.
void try_advance() {
    if (!g_waiters || g_waiters->empty()) return;
    if (g_running > 0) {
        // Something is executing app code and has not said when it wants to wake. Normally we wait
        // for it — but a thread that exits without sleeping again would stall the clock for good,
        // and the app has exactly one of those: healthy_timer sleeps nine seconds, clears the
        // bad-boot counter and returns. So give up on the runners after 20 ms of real time with no
        // progress, and FORGET them — otherwise every later advance pays that 20 ms again, which
        // turned a two-second suite into a four-minute one. A frame takes microseconds, so this
        // cannot fire on a thread that is genuinely working, and a forgotten thread re-counts
        // itself when it next returns from a sleep.
        if (real_ms() - g_last_move_real < 20) return;
        g_running = 0;
    }
    long long earliest = (*g_waiters)[0].target;
    for (size_t i = 1; i < g_waiters->size(); i++)
        if ((*g_waiters)[i].target < earliest) earliest = (*g_waiters)[i].target;
    // A scheduled input event is a wake-up too: stop the clock there rather than stepping over it,
    // or a tap aimed at t=50s would only reach the app when the frame it landed in finished
    // sleeping — up to a second later with the panel dark.
    long long ev = 0;
    if (cinder_harness_input_next(&ev) && ev > g_now_ms && ev < earliest) earliest = ev;
    if (earliest <= g_now_ms) return;
    g_now_ms = earliest;
    if (g_passive_target != 0 && g_now_ms >= g_passive_target) pthread_cond_broadcast(&g_slow);
    // Scheduled changes to the device's world land BEFORE anything observes the new time.
    cinder_harness_fs_due(g_now_ms);
    cinder_harness_input_due(g_now_ms);
    g_last_move_real = real_ms();
    pthread_cond_broadcast(&g_tick);
}

void wait_a_moment(bool passive = false) {   // caller holds the lock; releases it while parked
    struct timespec until;
    syscall(SYS_clock_gettime, CLOCK_REALTIME, &until);
    until.tv_nsec += 20 * 1000000L;
    if (until.tv_nsec >= 1000000000L) { until.tv_sec++; until.tv_nsec -= 1000000000L; }
    pthread_cond_timedwait(passive ? &g_slow : &g_tick, &g_lock, &until);
}

void sleep_for(long long ms) {
    Lock l; ensure();
    if (g_last_move_real == 0) g_last_move_real = real_ms();
    const long long target = g_now_ms + ms;

    if (t_role == T_PASSIVE) {
        // The harness's own main thread: it waits for virtual time to arrive and contributes
        // nothing to when the clock jumps.
        if (g_passive_target == 0 || target < g_passive_target) g_passive_target = target;
        while (g_now_ms < target) {
            try_advance();
            if (g_now_ms >= target) break;
            wait_a_moment(true);
            watchdog_or_die();
        }
        g_passive_target = 0;
        return;
    }

    mark_running();
    t_role = T_SLEEPING;
    if (g_running > 0) g_running--;   // may already be 0 if the stall fallback forgot us
    Waiter w; w.th = pthread_self(); w.target = target;
    g_waiters->push_back(w);

    while (g_now_ms < target) {
        try_advance();
        // RE-CHECK BEFORE PARKING. In the common case this thread's own wake is the earliest one
        // pending, so try_advance has just moved the clock to exactly what it was waiting for and
        // there is nothing to park for. Parking anyway cost one wait per frame — with the wait
        // bounded at 20 ms of REAL time for the stall fallback, that made every 16 ms virtual frame
        // take 20 ms of wall clock, and a two-second suite ran for four minutes.
        if (g_now_ms >= target) break;
        wait_a_moment();
        watchdog_or_die();
    }

    for (size_t i = 0; i < g_waiters->size(); i++)
        if (pthread_equal((*g_waiters)[i].th, pthread_self())) {
            g_waiters->erase(g_waiters->begin() + (long)i);
            break;
        }
    t_role = T_RUNNING;
    g_running++;
}

} // namespace

extern "C" {

// The hot path. `slot` is a per-call-site cache of the interned id: the generated stubs are called
// millions of times in a long scenario, and looking the name up in a map — under the lock, with a
// string compare — was most of the cost of running one.
void cinder_harness_record_cached(int* slot, const char* name, long long arg) {
    Lock l; ensure();
    mark_running();
    if (*slot < 0) *slot = intern(name);
    g_trace->push_back(Call{*slot, arg, g_now_ms});
}

void cinder_harness_record(const char* name, long long arg) {
    Lock l; ensure();
    mark_running();   // this thread is executing app code — the clock must not jump past it
    g_trace->push_back(Call{intern(name), arg, g_now_ms});
}

// For callers that ALREADY HOLD g_lock — currently only the scheduled-filesystem-change hook,
// which the clock invokes from inside try_advance. Taking the lock again there deadlocks: it is a
// plain mutex, not a recursive one.
void cinder_harness_record_locked(const char* name, long long arg) {
    ensure();
    g_trace->push_back(Call{intern(name), arg, g_now_ms});
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
    g_names->clear();
    g_name_id->clear();
    g_now_ms = 0;
    g_waiters->clear();
    g_running = 0;
    g_passive_target = 0;
    g_last_move_real = 0;
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
    const int id = name_id_of(name);
    if (id < 0) return 0;
    int n = 0;
    for (auto& c : *g_trace) if (c.name_id == id) n++;
    return n;
}

long long cinder_harness_arg(const char* name, int nth) {
    Lock l; ensure();
    const int id = name_id_of(name);
    if (id < 0) return 0;
    int n = 0;
    for (auto& c : *g_trace) if (c.name_id == id && n++ == nth) return c.arg;
    return 0;
}

long long cinder_harness_first_ms(const char* name) {
    Lock l; ensure();
    const int id = name_id_of(name);
    if (id < 0) return -1;
    for (auto& c : *g_trace) if (c.name_id == id) return c.at_ms;
    return -1;
}

long long cinder_harness_last_ms(const char* name) {
    Lock l; ensure();
    const int id = name_id_of(name);
    if (id < 0) return -1;
    long long r = -1;
    for (auto& c : *g_trace) if (c.name_id == id) r = c.at_ms;
    return r;
}

int cinder_harness_count_between(const char* name, long long from_ms, long long to_ms) {
    Lock l; ensure();
    const int id = name_id_of(name);
    if (id < 0) return 0;
    int n = 0;
    for (auto& c : *g_trace)
        if (c.name_id == id && c.at_ms >= from_ms && c.at_ms < to_ms) n++;
    return n;
}

int cinder_harness_before(const char* a, const char* b) {
    long long fa = cinder_harness_first_ms(a), fb = cinder_harness_first_ms(b);
    if (fa < 0 || fb < 0) return -1;
    if (fa != fb) return fa < fb ? 1 : 0;
    // Same virtual millisecond — fall back to trace order, which is the real answer.
    Lock l; ensure();
    const int ia = name_id_of(a), ib = name_id_of(b);
    for (auto& c : *g_trace) {
        if (c.name_id == ia) return 1;
        if (c.name_id == ib) return 0;
    }
    return -1;
}

void cinder_harness_dump(int max_lines) {
    Lock l; ensure();
    dump_unlocked(max_lines);
}

void cinder_harness_clock_passive(void) {
    Lock l; ensure();
    if (t_role == T_RUNNING) g_running--;
    t_role = T_PASSIVE;
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

// poll(), virtualised. The frame loop waits on its input descriptors with poll() rather than
// usleep() once input is up — which is right on the device (an event returns immediately at any
// budget) and would run the harness in REAL time, because poll is a syscall the virtual clock knows
// nothing about. A 70-second scenario took 70 seconds and tripped the watchdog.
//
// So: ask the kernel whether anything is ready RIGHT NOW, and if not, sleep the timeout on the
// virtual clock and ask once more. Events are written into the fake input FIFOs by the clock itself
// as it crosses their scheduled time, so by the second ask they are there.
int poll(struct pollfd* fds, nfds_t nfds, int timeout) {
    int r = (int)syscall(SYS_poll, fds, (long)nfds, 0);
    if (r != 0 || timeout == 0) return r;
    sleep_for(timeout < 0 ? 1000 : timeout);
    return (int)syscall(SYS_poll, fds, (long)nfds, 0);
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

// ── the app's log, counted ───────────────────────────────────────────────────────────────────
// clog_ writes one prefixed line to stderr per event and fflushes it, and the launcher redirects
// stderr to /contents/cinderhome.log — a file on the fragile vfat partition the user's music also
// lives on. So a log line is a flash write, and a log line ON A TIMER is a flash write per tick
// forever. Two defects of exactly that shape have already shipped (a boot-animation re-kill at
// 62 Hz, and a bootcount retry at 1 Hz that logged its failure every time and accounted for 99.5%
// of a six-hour log). Counting the lines makes the whole class assertable: see the `log-volume`
// scenario.
//
// Only lines carrying the app's own prefix are counted. fprintf is a shared symbol — the harness
// prints through it too — and the prefix is what tells the app's voice from ours.
int fprintf(FILE* stream, const char* fmt, ...) {
    char buf[2048];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    if (!std::strncmp(buf, "[cinder-home]", 13)) {
        Lock l; ensure();
        // The line itself goes into the trace name, trimmed: a dump then shows WHICH line ran away,
        // which is the only question worth asking once the count is too high.
        std::string text(buf);
        while (!text.empty() && (text[text.size() - 1] == '\n' || text[text.size() - 1] == ' '))
            text.erase(text.size() - 1);
        if (text.size() > 96) text.resize(96);
        g_trace->push_back(Call{intern("log"), 1, g_now_ms});
        g_trace->push_back(Call{intern(text.c_str()), 0, g_now_ms});
    }
    if (n > 0) fwrite(buf, 1, (size_t)(n < (int)sizeof buf ? n : (int)sizeof buf - 1), stream);
    return n;
}

int system(const char* cmd) {
    cinder_harness_record("system", 0);
    if (cmd) { Lock l; ensure(); g_trace->back().name_id = intern((std::string("system:") + cmd).c_str()); }
    return 0;
}

FILE* popen(const char* cmd, const char* /*mode*/) {
    Lock l; ensure();
    g_trace->push_back(Call{intern((std::string("popen:") + (cmd ? cmd : "?")).c_str()), 0, g_now_ms});
    return nullptr;   // "the command is not available" — the app's own degraded path
}

int pclose(FILE*) { return -1; }

void* dlopen(const char* path, int) {
    Lock l; ensure();
    g_trace->push_back(Call{intern((std::string("dlopen:") + (path ? path : "?")).c_str()), 0, g_now_ms});
    return nullptr;   // no Sony .so on a build machine — the optional-service paths degrade
}

void* dlsym(void*, const char* sym) {
    Lock l; ensure();
    g_trace->push_back(Call{intern((std::string("dlsym:") + (sym ? sym : "?")).c_str()), 0, g_now_ms});
    return nullptr;
}

char* dlerror(void) { return const_cast<char*>("harness: no dynamic loading"); }
int   dlclose(void*) { return 0; }

} // extern "C"
