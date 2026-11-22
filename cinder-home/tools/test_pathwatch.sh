#!/usr/bin/env bash
# test_pathwatch.sh — src/cinder-pathwatch.c on the host, against a stand-in for Sony's PathMgr
# library: the hold appears for a fresh target, is released when the target goes stale, and Sony's
# own setting is never touched when there is no target.
set -eu
HERE=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
cat > "$T/fakepm.c" <<'C'
static int flags[12], calls;
int _ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl5StartEv(void* self) { (void)self; return 7; }
void _ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl19InformDeviceEnabledERKNS1_6DeviceERKNS1_7connmgr7EnabledE(void* self, const int* d, const int* e) { (void)self; flags[*d] = *e; calls++; }
int _ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl17IsDeviceConnectedERKNS1_12OutputDeviceE(void* self, const int* d) { (void)self; return flags[*d]; }
/* Sony's library reaches Start through its own PLT; so does this. */
int run_start(void* self) { return _ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl5StartEv(self); }
int a2dp(void) { return flags[5]; }
int ncalls(void) { return calls; }
void sony_sets(int on) { flags[5] = on; }
C
cat > "$T/main.c" <<'C'
#include <stdio.h>
#include <stdlib.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
#include <dlfcn.h>
/* hagodaemon dlopens the service library privately (not in the global scope); so does this. */
static int (*run_start)(void*), (*a2dp)(void), (*ncalls)(void);
static void (*sony_sets)(int);
static int fails;
static void check(int ok, const char* what) { printf("%s %s\n", ok ? "ok  " : "FAIL", what); fails += !ok; }
int main(int argc, char** argv) {
    (void)argc;
    const char* tp = argv[1];
    static char impl[64];
    void* h = dlopen(argv[2], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("FAIL dlopen %s\n", dlerror()); return 1; }
    run_start = (int (*)(void*))dlsym(h, "run_start");
    a2dp = (int (*)(void))dlsym(h, "a2dp");
    ncalls = (int (*)(void))dlsym(h, "ncalls");
    sony_sets = (void (*)(int))dlsym(h, "sony_sets");
    check(run_start(impl) == 7, "Start still runs Sony's own (a privately loaded library) and returns its result");
    usleep(1200000);
    check(a2dp() == 0 && ncalls() == 0, "no target: nothing is called");
    FILE* f = fopen(tp, "w"); fputs("cinder.a2dp\n", f); fclose(f);
    usleep(1200000);
    check(a2dp() == 1 && ncalls() == 1, "a fresh target holds A2DP on");
    struct timeval old[2] = {{time(NULL) - 60, 0}, {time(NULL) - 60, 0}};
    utimes(tp, old);
    usleep(1200000);
    check(a2dp() == 0 && ncalls() == 2, "a stale target releases the hold");
    sony_sets(1);
    unlink(tp);
    usleep(1200000);
    check(a2dp() == 1 && ncalls() == 2, "Sony's own A2DP is left alone with no target");
    return fails != 0;
}
C
cc -O2 -Wall -Werror -shared -fPIC -o "$T/libPathMgrServiceFw.so" "$T/fakepm.c"
cc -O2 -Wall -Werror -shared -fPIC -DCINDER_A2DP_TARGET="\"$T/target\"" -o "$T/pathwatch.so" \
    "$HERE/src/cinder-pathwatch.c" -ldl -lpthread
cc -O2 -Wall -Werror -o "$T/main" "$T/main.c" -ldl
LD_PRELOAD="$T/pathwatch.so" "$T/main" "$T/target" "$T/libPathMgrServiceFw.so"
echo "test_pathwatch: all passed"
