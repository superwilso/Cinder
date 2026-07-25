/* glibc223_compat.c — symbol shims so a binary built with a modern toolchain runs on
 * the NW-A50's glibc 2.23 (Gentoo 2.23-r3, 2016). Compile against the device 2.23 headers
 * (build.sh does this with -nostdinc -isystem <xenial-2.23>) so _STAT_VER and struct stat
 * match the device ABI exactly.
 *
 * WHY: glibc < 2.33 does NOT export stat/fstat/lstat/fstatat as real symbols — it exports
 * the versioned indirection __xstat/__fxstat/__lxstat/__fxstatat (@GLIBC_2.4) and the
 * headers normally inline stat()->__xstat(_STAT_VER,...). But code that takes the ADDRESS
 * of stat (SQLite's syscall-pointer table) needs a real `stat` symbol, which the device
 * lacks. These thin wrappers provide the plain names, forwarding to the __x* the device
 * DOES export. (fcntl/open/time/gettimeofday are real device symbols — no shim needed.)
 *
 * This is the correct, ABI-safe way to target the device's old glibc; it does NOT touch
 * the device. See cinder-home/README.md + the project glibc-2.23 notes.
 */
#define _GNU_SOURCE
#include <sys/stat.h>
#include <sys/types.h>

extern int __xstat(int ver, const char *path, struct stat *buf);
extern int __fxstat(int ver, int fd, struct stat *buf);
extern int __lxstat(int ver, const char *path, struct stat *buf);
extern int __fxstatat(int ver, int dirfd, const char *path, struct stat *buf, int flags);
extern int __xstat64(int ver, const char *path, struct stat64 *buf);
extern int __fxstat64(int ver, int fd, struct stat64 *buf);
extern int __lxstat64(int ver, const char *path, struct stat64 *buf);
extern int __fxstatat64(int ver, int dirfd, const char *path, struct stat64 *buf, int flags);

int stat(const char *path, struct stat *buf) { return __xstat(_STAT_VER, path, buf); }
int fstat(int fd, struct stat *buf) { return __fxstat(_STAT_VER, fd, buf); }
int lstat(const char *path, struct stat *buf) { return __lxstat(_STAT_VER, path, buf); }
int fstatat(int dirfd, const char *path, struct stat *buf, int flags) {
    return __fxstatat(_STAT_VER, dirfd, path, buf, flags);
}

int stat64(const char *path, struct stat64 *buf) { return __xstat64(_STAT_VER, path, buf); }
int fstat64(int fd, struct stat64 *buf) { return __fxstat64(_STAT_VER, fd, buf); }
int lstat64(const char *path, struct stat64 *buf) { return __lxstat64(_STAT_VER, path, buf); }
int fstatat64(int dirfd, const char *path, struct stat64 *buf, int flags) {
    return __fxstatat64(_STAT_VER, dirfd, path, buf, flags);
}
