/* cinder-hagowrap.c — a per-service LD_PRELOAD for Sony's SoundServiceFw on players with no Wampy.
 *
 * WHY. libcinder_mono.so (mono, and soundscapes over music) has to be PRELOADED into SoundServiceFw:
 * its Bluetooth hooks are libc's write/send, and libc is in the global scope of every process, so
 * nothing loaded later can stand in front of it. Wampy's installer gives that service an
 * `LD_PRELOAD` in the boot image; Walkman One has none, and Cinder never edits a boot image. The
 * other persistent route, /etc/ld.so.preload, loads into EVERY process on the player — far more
 * than this needs, and a fault there is below every escape Cinder has.
 *
 * WHAT. init starts each Sony service as `/bin/logwrapper /system/vendor/sony/bin/hagodaemon
 * <Service> …`. The owner-approved install (2026-10-04) keeps Sony's binary as hagodaemon.real and
 * puts this one in its place. For every service it is one execve of the real binary and nothing
 * else. For SoundServiceFw alone, and only when the shim is installed and not switched off, it adds
 * LD_PRELOAD first.
 *
 * ANY SERVICE can be given a preload by a file at HAGO_DIR/<Service>.so (the service name as init
 * passes it, letters, digits and '_' only). Nothing in Cinder installs one; it is the door for a
 * separate project that hooks some other Sony service, so that project never has to replace this
 * wrapper. The same off switches and the same "missing or empty means no" rule apply to it.
 *
 * argv is passed through UNCHANGED, argv[0] included, so /proc/<pid>/cmdline reads exactly as it
 * does on stock — the shim's own "am I SoundServiceFw" test and every `hagodaemon <Service>` match
 * keep working. Only /proc/<pid>/exe differs.
 *
 * THIS SITS IN FRONT OF EVERY SONY SERVICE, so it is held to the launcher's rule: nothing here may
 * be able to stop the exec. No allocation, no stdio, static libc; every decision that fails, fails
 * toward "run Sony's binary exactly as it was asked for".
 *
 * Off switches, any one of which makes this a plain pass-through:
 *   /data/cinder/preload_off        persistent
 *   /contents/cinder_preload_off    settable over USB-MSC (only seen if /contents is mounted yet)
 *   the shim file missing or empty (each shim on its own)
 * The shim has its own as well (cinder-mono.c: safe mode after two loads that never reached audio,
 * and /data/cinder/mono_shim_off).
 */
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#ifndef HAGO_REAL
#define HAGO_REAL "/system/vendor/sony/bin/hagodaemon.real"
#endif
#ifndef HAGO_SHIM
#define HAGO_SHIM "/system/vendor/unknown321/lib/libcinder_mono.so"
#endif
#ifndef HAGO_DIR
#define HAGO_DIR "/system/vendor/unknown321/lib/preload"
#endif
#ifndef HAGO_OFF
#define HAGO_OFF "/data/cinder/preload_off"
#endif
#ifndef HAGO_OFF_MSC
#define HAGO_OFF_MSC "/contents/cinder_preload_off"
#endif

extern char** environ;

static void say(const char* s) { (void)!write(2, s, strlen(s)); }

/* The value for LD_PRELOAD: whatever was already there, then each of `shims` it does not already
 * name. Static storage: the environment points into it across the exec. Returns 0 if there is
 * nothing to add or it does not fit. */
static char g_preload[1024];
static int preload_value(const char* have, const char* const* shims, int n)
{
    static const char key[] = "LD_PRELOAD=";
    size_t k = sizeof key - 1, h = have ? strlen(have) : 0;
    int added = 0;
    if (k + h + 1 > sizeof g_preload) return 0;
    memcpy(g_preload, key, k);
    memcpy(g_preload + k, have ? have : "", h);
    k += h;
    for (int i = 0; i < n; i++) {
        size_t s = strlen(shims[i]);
        if (have && strstr(have, shims[i])) continue;
        if (k + 1 + s + 1 > sizeof g_preload) return 0;
        if (k > sizeof key - 1) g_preload[k++] = ' ';
        memcpy(g_preload + k, shims[i], s);
        k += s;
        added = 1;
    }
    g_preload[k] = 0;
    return added;
}

static int usable(const char* path)
{
    struct stat st;
    return stat(path, &st) == 0 && S_ISREG(st.st_mode) && st.st_size > 0;
}

/* HAGO_DIR/<name>.so for a service name init could have passed; 0 for anything else. */
static char g_svc[192];
static int service_shim(const char* name)
{
    size_t d = sizeof HAGO_DIR - 1, n = strlen(name);
    if (n == 0 || d + 1 + n + 4 > sizeof g_svc) return 0;
    for (size_t i = 0; i < n; i++) {
        char c = name[i];
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_'))
            return 0;
    }
    memcpy(g_svc, HAGO_DIR "/", d + 1);
    memcpy(g_svc + d + 1, name, n);
    memcpy(g_svc + d + 1 + n, ".so", 4);
    return 1;
}

int main(int argc, char** argv)
{
    char** envp = environ;
    /* Room for every variable init hands a service, plus ours. More than this and we do not add
     * ours: the service still starts, with the environment it was given. */
    static char* env2[256];

    const char* shims[2];
    int ns = 0;
    if (argc >= 2 && argv[1] && access(HAGO_OFF, F_OK) != 0 && access(HAGO_OFF_MSC, F_OK) != 0) {
        if (strcmp(argv[1], "SoundServiceFw") == 0 && usable(HAGO_SHIM)) shims[ns++] = HAGO_SHIM;
        if (service_shim(argv[1]) && usable(g_svc)) shims[ns++] = g_svc;
    }
    if (ns) {
        const char* have = 0;
        int n = 0, at = -1;
        for (; environ && environ[n]; n++)
            if (strncmp(environ[n], "LD_PRELOAD=", 11) == 0) { at = n; have = environ[n] + 11; }
        /* Every shim already named (a Wampy setenv naming the same file): environment untouched. */
        if (n + 2 <= (int)(sizeof env2 / sizeof env2[0]) && preload_value(have, shims, ns)) {
            int j = 0;
            for (int i = 0; i < n; i++) env2[j++] = (i == at) ? g_preload : environ[i];
            if (at < 0) env2[j++] = g_preload;
            env2[j] = 0;
            envp = env2;
        }
    }

    execve(HAGO_REAL, argv, envp);
    /* Only reached if Sony's binary could not be run. Try once more without our addition, in case
     * that was what the kernel refused, and then say so: init's logwrapper carries stderr to the
     * log, and this line is the whole diagnosis. */
    if (envp != environ) execve(HAGO_REAL, argv, environ);
    say("cinder-hagowrap: could not exec " HAGO_REAL "\n");
    return 127;
}
