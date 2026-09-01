// guard_selftest.cpp — host self-test of cinder-home's crash+hang GUARD.
//
// Mirrors the recovery pattern in src/main.cpp: a SIGSEGV or SIGALRM inside a guarded call is
// caught via siglongjmp, the call is skipped, and the process keeps running. Built+run on the host
// by build.sh and by CI's "C++ self-tests" step as a fast regression check (the logic is portable).
//
// ── WHY THIS FILE GREW (2026-09-01) ─────────────────────────────────────────────────────────────
// Tests 1–4 cover the recovery mechanism and always did. What nothing covered was the DECISION that
// sits in front of it: *which* calls may be unwound, and what a recovery is allowed to cost.
//
// The single guard used to answer that question one way for every caller, and four of its call
// sites were not Sony IPC. That was wrong in both directions at once, and both directions had a
// user-visible shape:
//
//   OUTWARDS  a slow mount or statvfs set `ipc_dead`, so a filesystem operation cost the user
//             audio and Bluetooth for the rest of the boot.
//   INWARDS   once `ipc_dead` was set by anything at all, the guard refused EVERYTHING — including
//             the /contents reclaim. One recovered transport timeout meant a cable plugged in
//             later left the library missing and the art grey until a reboot: "I plugged it into
//             my PC and all my music vanished", reached from a timeout the guard had HANDLED.
//
// Neither could be caught by the harness — its clock is virtual and `alarm()` is wall-clock, so no
// scenario can make a real guard recovery happen. It is pure logic, so it is tested here instead.
// Tests 5–9 fail on the code as it stood before that split.
#include <cstdio>
#include <csignal>
#include <cstring>
#include <cstdlib>
#include <unistd.h>
#include <setjmp.h>
#include <sys/wait.h>
#include <initializer_list>

static sigjmp_buf jb; static volatile sig_atomic_t in_guard = 0;

// ── the three kinds, mirroring src/main.cpp ─────────────────────────────────────────────────────
// The question a call site answers is not "might this be slow" but "if this call is abandoned
// mid-flight, what does it leave behind?"
enum GuardKind {
    GUARD_IPC = 0,   // a half-built container inside a closed Sony client -> never call in again
    GUARD_LOCAL,     // nothing -> drop it where it stands and carry on
    GUARD_FATAL,     // a held mutex / an unreaped child -> no safe unwind, so do not attempt one
};
static bool ipc_dead = false;
static const char* unguarded_what = nullptr;

static void h(int sig, siginfo_t*, void*) {
    if (in_guard) { in_guard = 0; alarm(0); siglongjmp(jb, 1); }
    // Un-guarded: in main.cpp this names the call, latches the bad-boot counter and _exit(42)s
    // into the escape ladder. Test 10 forks a child to observe exactly that.
    if (sig == SIGALRM && unguarded_what)
        std::fprintf(stderr, "  (un-abandonable call overran: %s)\n", unguarded_what);
    _exit(42);
}

static int run_guarded_ex(const char* what, unsigned timeout, void(*fn)(), GuardKind kind) {
    // ONLY Sony IPC is refused after a Sony IPC death.
    if (kind == GUARD_IPC && ipc_dead) { std::printf("  refused(%s): ipc_dead\n", what); return -1; }
    std::printf("  run_guarded(%s, kind=%d)\n", what, (int)kind);
    if (kind == GUARD_FATAL) {                 // watchdog, no unwind
        unguarded_what = what; alarm(timeout); fn(); alarm(0); unguarded_what = nullptr; return 0;
    }
    in_guard = 1;
    if (sigsetjmp(jb, 1) == 0) { alarm(timeout); fn(); alarm(0); in_guard = 0; return 0; }
    in_guard = 0; alarm(0);
    if (kind == GUARD_LOCAL) return -1;        // owns nothing -> nothing to poison
    ipc_dead = true;                           // GUARD_IPC: everything Sony is off from here
    return -1;
}
static int run_guarded(const char* w, unsigned t, void(*f)())       { return run_guarded_ex(w,t,f,GUARD_IPC); }
static int run_guarded_local(const char* w, unsigned t, void(*f)()) { return run_guarded_ex(w,t,f,GUARD_LOCAL); }
static int run_watchdog_only(const char* w, unsigned t, void(*f)()) { return run_guarded_ex(w,t,f,GUARD_FATAL); }

static void ok_fn()    { volatile int x = 0; for (int i=0;i<1000;i++) x+=i; }
static void crash_fn() { volatile int* p = (int*)0x12; *p = 7; }      // SIGSEGV at 0x12 (the device addr!)
static void hang_fn()  { for(;;) pause(); }                            // blocks forever -> SIGALRM

static int fails = 0;
static void check(int cond, const char* what) {
    std::printf("  %-4s %s\n", cond ? "ok" : "FAIL", what);
    if (!cond) fails = 1;
}

static void install_handlers() {
    struct sigaction sa; std::memset(&sa,0,sizeof sa);
    sa.sa_sigaction = h; sa.sa_flags = SA_SIGINFO | SA_NODEFER; sigemptyset(&sa.sa_mask);
    for (int s : {SIGSEGV,SIGBUS,SIGABRT,SIGILL,SIGFPE,SIGALRM}) sigaction(s,&sa,nullptr);
}

int main() {
    install_handlers();

    std::printf("test 1: normal call -> expect 0\n");
    int a = run_guarded("ok", 5, ok_fn);   check(a==0, "a normal guarded call returns 0");
    std::printf("test 2: crashing call -> expect -1, process survives\n");
    int b = run_guarded("crash", 5, crash_fn); check(b==-1, "a SIGSEGV inside the guard is recovered");
    check(ipc_dead, "…and that recovery ended Sony IPC for the boot");

    std::printf("test 3: hanging call -> expect -1 after ~2s watchdog\n");
    // Reset first, or test 2's recovery refuses this one before it runs and it passes for the
    // wrong reason — which is exactly what the first version of this file did.
    ipc_dead = false;
    int c = run_guarded("hang", 2, hang_fn);   check(c==-1, "a hang inside the guard is recovered");

    std::printf("test 4: guard state is intact after a crash+hang\n");
    check(run_guarded_local("state-intact", 5, ok_fn)==0, "the jump buffer still works afterwards");
    check(ipc_dead, "a recovered GUARD_IPC call ends Sony IPC for the boot");

    std::printf("test 5: with IPC dead, a further Sony IPC call is refused\n");
    int e = run_guarded("ipc-after-death", 5, ok_fn);
    check(e==-1, "GUARD_IPC is refused once the client is known-broken");

    // ── the INWARDS bug: one recovered transport timeout used to cost the user their library ────
    std::printf("test 6: with IPC dead, LOCAL work still runs (statvfs, and friends)\n");
    int f = run_guarded_local("statvfs", 5, ok_fn);
    check(f==0, "GUARD_LOCAL is NOT gated on the state of a Sony client");

    std::printf("test 7: with IPC dead, the /contents reclaim still runs\n");
    int g = run_watchdog_only("reclaim /contents", 5, ok_fn);
    check(g==0, "GUARD_FATAL is NOT gated either — this is the 'all my music vanished' bug");

    // ── the OUTWARDS bug: a slow mount used to take audio down with it ──────────────────────────
    std::printf("test 8: a recovered LOCAL call does not kill Sony IPC\n");
    ipc_dead = false;                                   // pretend a fresh boot
    int i = run_guarded_local("slow-local", 2, hang_fn);
    check(i==-1, "a hung GUARD_LOCAL call is still recovered");
    check(!ipc_dead, "…and costs the user nothing: Sony IPC is untouched");
    check(run_guarded("ipc-still-live", 5, ok_fn)==0, "so Sony IPC still answers afterwards");

    // ── GUARD_FATAL never unwinds ───────────────────────────────────────────────────────────────
    // Its whole point is that abandoning the call would leak something that outlives it (a held
    // std::sync::Mutex inside cinder_db_open; an unreaped child from system()). So a hang must NOT
    // come back as -1 — it must fall through to the un-guarded path and _exit(42) into the escape
    // ladder. Forked, because observing that means observing a process death.
    std::printf("test 9: a hung GUARD_FATAL call is fatal, not recovered\n");
    pid_t pid = fork();
    if (pid == 0) {
        install_handlers();
        run_watchdog_only("db-open", 1, hang_fn);   // must not return
        _exit(7);                                   // if it did, that is the failure
    }
    int st = 0; waitpid(pid, &st, 0);
    check(WIFEXITED(st) && WEXITSTATUS(st)==42,
          "it reached the un-guarded path and exited 42 (the ladder), not 7");

    std::printf(fails ? "GUARD SELFTEST FAILED\n"
                      : "SURVIVED ALL — recovery works, and each kind costs only what it should\n");
    return fails;
}
