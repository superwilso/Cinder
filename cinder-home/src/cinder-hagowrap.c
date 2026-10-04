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
 *   the shim file missing or empty
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
#ifndef HAGO_OFF
#define HAGO_OFF "/data/cinder/preload_off"
#endif
#ifndef HAGO_OFF_MSC
#define HAGO_OFF_MSC "/contents/cinder_preload_off"
#endif

extern char** environ;

static void say(const char* s) { (void)!write(2, s, strlen(s)); }

/* The value for LD_PRELOAD: the shim, after whatever was already there. Static storage: the
 * environment points into it across the exec. Returns 0 if it does not fit. */
static char g_preload[512];
static int preload_value(const char* have)
{
    static const char key[] = "LD_PRELOAD=";
    size_t k = sizeof key - 1, h = have ? strlen(have) : 0, s = sizeof HAGO_SHIM - 1;
    if (k + h + (h ? 1 : 0) + s + 1 > sizeof g_preload) return 0;
    memcpy(g_preload, key, k);
    if (h) { memcpy(g_preload + k, have, h); g_preload[k + h] = ' '; k += h + 1; }
    memcpy(g_preload + k, HAGO_SHIM, s + 1);
    return 1;
}

int main(int argc, char** argv)
{
    char** envp = environ;
    /* Room for every variable init hands a service, plus ours. More than this and we do not add
     * ours: the service still starts, with the environment it was given. */
    static char* env2[256];

    if (argc >= 2 && argv[1] && strcmp(argv[1], "SoundServiceFw") == 0) {
        struct stat st;
        int want = stat(HAGO_SHIM, &st) == 0 && S_ISREG(st.st_mode) && st.st_size > 0
                && access(HAGO_OFF, F_OK) != 0 && access(HAGO_OFF_MSC, F_OK) != 0;
        if (want) {
            const char* have = 0;
            int n = 0, at = -1;
            for (; environ && environ[n]; n++)
                if (strncmp(environ[n], "LD_PRELOAD=", 11) == 0) { at = n; have = environ[n] + 11; }
            /* Already there (a Wampy setenv naming the same file): leave the environment alone. */
            if (have && strstr(have, HAGO_SHIM)) want = 0;
            if (want && n + 2 <= (int)(sizeof env2 / sizeof env2[0]) && preload_value(have)) {
                int j = 0;
                for (int i = 0; i < n; i++) env2[j++] = (i == at) ? g_preload : environ[i];
                if (at < 0) env2[j++] = g_preload;
                env2[j] = 0;
                envp = env2;
            }
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
