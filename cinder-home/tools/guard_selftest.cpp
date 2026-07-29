// guard_selftest.cpp — host self-test of cinder-home's crash+hang GUARD (run_guarded).
// Mirrors the recovery pattern in src/main.cpp: a SIGSEGV or SIGALRM inside a guarded
// call is caught via siglongjmp, the call is skipped, and the process keeps running.
// Built+run on the host by build.sh as a fast regression check (the logic is portable).
// Standalone validation of the run_guarded crash+hang recovery (host x86; logic is portable).
#include <cstdio>
#include <csignal>
#include <cstring>
#include <unistd.h>
#include <setjmp.h>
#include <initializer_list>
static sigjmp_buf jb; static volatile sig_atomic_t in_guard = 0;
static void h(int sig, siginfo_t*, void*) {
    if (in_guard) { in_guard = 0; alarm(0); siglongjmp(jb, 1); }
    std::fprintf(stderr, "FATAL sig %d (un-guarded)\n", sig); _exit(42);
}
static int run_guarded(const char* what, unsigned timeout, void(*fn)()) {
    std::printf("  run_guarded(%s)\n", what);
    in_guard = 1;
    if (sigsetjmp(jb, 1) == 0) { alarm(timeout); fn(); alarm(0); in_guard = 0; return 0; }
    in_guard = 0; alarm(0); return -1;
}
static void ok_fn()    { volatile int x = 0; for (int i=0;i<1000;i++) x+=i; }
static void crash_fn() { volatile int* p = (int*)0x12; *p = 7; }      // SIGSEGV at 0x12 (the device addr!)
static void hang_fn()  { for(;;) pause(); }                            // blocks forever -> SIGALRM
int main() {
    struct sigaction sa; std::memset(&sa,0,sizeof sa);
    sa.sa_sigaction = h; sa.sa_flags = SA_SIGINFO | SA_NODEFER; sigemptyset(&sa.sa_mask);
    for (int s : {SIGSEGV,SIGBUS,SIGABRT,SIGILL,SIGFPE,SIGALRM}) sigaction(s,&sa,nullptr);
    std::printf("test 1: normal call -> expect 0\n");
    int a = run_guarded("ok", 5, ok_fn);   std::printf("  => %d %s\n", a, a==0?"PASS":"FAIL");
    std::printf("test 2: crashing call -> expect -1, process survives\n");
    int b = run_guarded("crash", 5, crash_fn); std::printf("  => %d %s\n", b, b==-1?"PASS":"FAIL");
    std::printf("test 3: hanging call -> expect -1 after ~2s watchdog\n");
    int c = run_guarded("hang", 2, hang_fn);   std::printf("  => %d %s\n", c, c==-1?"PASS":"FAIL");
    std::printf("test 4: another normal call AFTER a crash+hang -> expect 0 (state intact)\n");
    int d = run_guarded("ok2", 5, ok_fn);  std::printf("  => %d %s\n", d, d==0?"PASS":"FAIL");
    std::printf("SURVIVED ALL — guard recovers crash & hang, process keeps running\n");
    return (a==0&&b==-1&&c==-1&&d==0)?0:1;
}
