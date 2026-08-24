/* db_sig.h — "has the library database moved?", as one number.
 *
 * WHY THIS IS NOT JUST st_mtime. `/db/MTPDB.dat` is a SQLite database written by Sony's
 * MediaStoreService, and mtime is the one stamp a SQLite writer can leave untouched across a
 * commit:
 *
 *   - with a write-ahead log the pages land in `MTPDB.dat-wal` and the main file is only
 *     rewritten at a checkpoint, which may be minutes later or never while the writer is up;
 *   - with a rollback journal the main file IS rewritten, but `MTPDB.dat-journal` appears and
 *     disappears around it, which is often the only thing visible at a 10-second poll;
 *   - the DB lives on a filesystem whose mtime granularity is coarse (2 s on vfat), so two
 *     writes inside one tick are one mtime.
 *
 * We do not control which journal mode the writer chose and have not measured it, so the
 * signature covers all three files and, for each, mtime AND size AND inode. Any one of them
 * moving moves the signature. This cannot be defeated by a journal mode.
 *
 * ZERO MEANS "NOTHING READABLE", WHICH IS NOT A CHANGE. /db can be momentarily unreadable, and
 * rebuilding the whole library on a failed stat() would be the worst possible response to it —
 * so 0 is returned for "no part existed", and a real read that happens to hash to 0 is nudged
 * to 1 so the two can never be confused.
 *
 * Header-only and free of every project dependency, so tools/dbsig_selftest.cpp can exercise it
 * on the host — the same treatment bt_edge.h and jack_edge.h get, and for the same reason: this
 * decides whether a user ever sees music they just copied onto the device.
 */
#ifndef CINDER_DB_SIG_H
#define CINDER_DB_SIG_H

#include <sys/stat.h>

/* Fold one (mtime, size, inode) triple into `sig`. FNV-1a-ish; any field moving moves the whole. */
static inline unsigned long long cinder_db_sig_fold(unsigned long long sig,
                                                    unsigned long long mtime,
                                                    unsigned long long size,
                                                    unsigned long long ino) {
    const unsigned long long f[3] = { mtime, size, ino };
    for (int k = 0; k < 3; ++k)
        sig ^= f[k] + 0x9e3779b97f4a7c15ULL + (sig << 6) + (sig >> 2);
    return sig;
}

/* Signature of the store rooted at `db` (e.g. "/db/MTPDB.dat"). 0 = nothing readable. */
static inline unsigned long long cinder_db_signature(const char* db_path,
                                                     const char* wal_path,
                                                     const char* journal_path) {
    const char* parts[3] = { db_path, wal_path, journal_path };
    unsigned long long sig = 0;
    int any = 0;
    for (int i = 0; i < 3; ++i) {
        struct stat st;
        if (!parts[i] || stat(parts[i], &st) != 0) continue;  /* absent journals are normal */
        any = 1;
        sig = cinder_db_sig_fold(sig, (unsigned long long)st.st_mtime,
                                 (unsigned long long)st.st_size,
                                 (unsigned long long)st.st_ino);
    }
    if (!any) return 0ULL;
    return sig ? sig : 1ULL;
}

#endif /* CINDER_DB_SIG_H */
