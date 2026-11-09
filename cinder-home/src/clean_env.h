/* clean_env.h — for the setuid-root helpers that run a shell (system/popen).
 *
 * After setuid(0) the real uid is root too, so the kernel no longer marks our children AT_SECURE
 * and the dynamic loader honours whatever the CALLER put in the environment: LD_PRELOAD=/tmp/x.so
 * would be loaded into the root /system/bin/sh that system() starts. So the caller's environment
 * is dropped and a fixed one is set. One variable is kept: ANDROID_PROPERTY_WORKSPACE, which
 * getprop needs to find the property area (without it every getprop prints nothing — measured
 * 2026-11-09 with `env -i`). Every directory named here is on the read-only /system. */
#ifndef CINDER_CLEAN_ENV_H
#define CINDER_CLEAN_ENV_H
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <stdio.h>
#include <stdlib.h>

static void clean_env(void)
{
    char props[64] = "";
    const char *w = getenv("ANDROID_PROPERTY_WORKSPACE");
    if (w) snprintf(props, sizeof props, "%s", w);
    clearenv();
    if (props[0]) setenv("ANDROID_PROPERTY_WORKSPACE", props, 1);
    setenv("PATH", "/system/bin:/system/xbin:/xbin:/sbin:/bin", 1);
    setenv("LD_LIBRARY_PATH",
           "/system/lib:/vendor/lib:/system/vendor/sony/lib:/system/usr/local/lib:/usr/lib:/usr/local/lib:/lib", 1);
}
#endif
