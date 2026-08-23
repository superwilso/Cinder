// dbsig_selftest — host test for the library-database change rule (src/db_sig.h).
//
// Same shape as btedge_selftest / jackedge_selftest, and here for the same reason: this decides
// whether music the user just copied onto the device is ever seen. The version it replaces
// compared st_mtime on the main DB file alone, which a SQLite writer in WAL mode can leave
// untouched across a whole scan.
#include "../src/db_sig.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <utime.h>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

static std::string g_dir;
static std::string P(const char* suffix) { return g_dir + "/MTPDB.dat" + suffix; }

static void put(const std::string& path, const char* bytes) {
    int fd = ::open(path.c_str(), O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) { std::perror("open"); std::exit(2); }
    if (bytes && *bytes) { if (::write(fd, bytes, std::strlen(bytes)) < 0) std::perror("write"); }
    ::close(fd);
}

// mtime is set explicitly rather than waited for: the whole point of this rule is that it does not
// depend on the clock having ticked between two writes.
static void set_mtime(const std::string& path, long t) {
    struct stat st;
    if (::stat(path.c_str(), &st) != 0) { std::perror("stat"); std::exit(2); }
    struct utimbuf ub;
    ub.actime = (time_t)t;
    ub.modtime = (time_t)t;
    if (::utime(path.c_str(), &ub) != 0) { std::perror("utime"); std::exit(2); }
}

static unsigned long long sig() {
    return cinder_db_signature(P("").c_str(), P("-wal").c_str(), P("-journal").c_str());
}

int main() {
    char tmpl[] = "/tmp/cinder-dbsig-XXXXXX";
    const char* d = ::mkdtemp(tmpl);
    if (!d) { std::perror("mkdtemp"); return 2; }
    g_dir = d;

    // Nothing there at all: not a change, and must be distinguishable from a real reading.
    check(sig() == 0, "no database at all reads as 0 (unknown), never as a change");

    put(P(""), "sqlite-ish");
    set_mtime(P(""), 1000000);
    const unsigned long long base = sig();
    check(base != 0, "a database that exists has a non-zero signature");
    check(sig() == base, "an unchanged store keeps its signature (no spurious reloads)");

    // THE CASE THE OLD st_mtime CHECK MISSED: the writer commits into a write-ahead log and the
    // main file is not touched at all.
    put(P("-wal"), "wal-pages");
    set_mtime(P(""), 1000000);
    const unsigned long long with_wal = sig();
    check(with_wal != base, "a -wal appearing is a change even with the DB's mtime untouched");

    // …and the WAL growing, still with the DB untouched.
    put(P("-wal"), "wal-pages-and-more");
    set_mtime(P(""), 1000000);
    check(sig() != with_wal, "a -wal that GREW is a change even with the DB's mtime untouched");

    // A checkpoint: the WAL goes away and the main file is rewritten.
    ::unlink(P("-wal").c_str());
    put(P(""), "sqlite-ish-rewritten");
    set_mtime(P(""), 1000000);          // same coarse mtime as before — vfat granularity
    check(sig() != base, "a same-mtime rewrite of a different size is a change");

    // The rollback-journal shape.
    put(P(""), "sqlite-ish");
    set_mtime(P(""), 1000000);
    const unsigned long long clean = sig();
    put(P("-journal"), "j");
    set_mtime(P(""), 1000000);
    check(sig() != clean, "a -journal appearing is a change");
    ::unlink(P("-journal").c_str());
    check(sig() == clean, "and it going away again returns to the same signature");

    // mtime alone still counts, which is the ordinary case.
    set_mtime(P(""), 1000900);
    check(sig() != clean, "a plain mtime bump is a change");

    // An unreadable /db must read as unknown, not as a change — a failed stat must never trigger
    // a full library rebuild.
    ::unlink(P("").c_str());
    check(sig() == 0, "a vanished database reads as unknown (0), not as a change");

    ::rmdir(g_dir.c_str());
    std::printf(fails ? "\ndbsig_selftest: %d FAILURE(S)\n" : "\ndbsig_selftest: all good\n", fails);
    return fails ? 1 : 0;
}
