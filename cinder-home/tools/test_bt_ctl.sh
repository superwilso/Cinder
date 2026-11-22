#!/usr/bin/env bash
# test_bt_ctl.sh — bt_ctl.h's service directory answers (SDP), checked on the host: every answer
# parses as nested data elements, a search finds exactly the records that hold its UUIDs, the
# attribute list is filtered, and a small byte budget is carried over in continuation pieces.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
cat > "$T/t.c" <<'C'
#include "bt_ctl.h"
static int fails;
static void check(int ok, const char* what) { printf("%s %s\n", ok ? "ok  " : "FAIL", what); fails += !ok; }
// Walks one element and everything inside it; 1 when every length adds up exactly.
static int valid(const unsigned char* p, int n) {
    int hl, dl;
    if (sdp_elem(p, n, &hl, &dl)) return 0;
    if ((p[0] >> 3) == 6) {
        for (int i = hl; i < hl + dl;) {
            int ehl, edl;
            if (sdp_elem(p + i, hl + dl - i, &ehl, &edl) || !valid(p + i, ehl + edl)) return 0;
            i += ehl + edl;
        }
    }
    return hl + dl == n;
}
static int has(const unsigned char* p, int n, const unsigned char* pat, int pn) {
    for (int i = 0; i + pn <= n; i++) if (!memcmp(p + i, pat, (size_t)pn)) return 1;
    return 0;
}
// ServiceSearchAttributeRequest for one UUID, all attributes, max bytes, continuation offset.
static int ask(unsigned uuid, int max, int cont, unsigned char* out) {
    unsigned char q[64] = {0x06, 0x12, 0x34, 0, 0, 0x35, 3, 0x19, (unsigned char)(uuid >> 8), (unsigned char)uuid,
                           (unsigned char)(max >> 8), (unsigned char)max, 0x35, 5, 0x0a, 0, 0, 0xff, 0xff};
    int n = 19;
    if (cont >= 0) { q[n++] = 2; q[n++] = (unsigned char)(cont >> 8); q[n++] = (unsigned char)cont; } else q[n++] = 0;
    q[3] = (unsigned char)((n - 5) >> 8); q[4] = (unsigned char)(n - 5);
    return sdp_answer(q, n, out);
}
int main(void) {
    sdp_init();
    unsigned char o[1100];
    int n = ask(0x110a, 0xffff, -1, o);
    const int bytes = o[5] << 8 | o[6];
    check(o[0] == 0x07 && o[1] == 0x12 && o[2] == 0x34, "A2DP source search answered, transaction id kept");
    check((o[3] << 8 | o[4]) == n - 5 && bytes == n - 8 && o[n - 1] == 0, "lengths add up, no continuation");
    check(valid(o + 7, bytes), "the attribute lists parse as data elements");
    const unsigned char psm25[] = {0x19, 0x01, 0x00, 0x09, 0x00, 0x19}, src[] = {0x19, 0x11, 0x0a};
    check(has(o + 7, bytes, psm25, 6) && has(o + 7, bytes, src, 3), "it is the A2DP source on PSM 25");
    const unsigned char tg[] = {0x19, 0x11, 0x0c};
    check(!has(o + 7, bytes, tg, 3), "the AVRCP records are not in it");
    n = ask(0x0100, 0xffff, -1, o);   // L2CAP: in all three
    check(valid(o + 7, o[5] << 8 | o[6]) && (o[7 + 1] << 8 | o[7 + 2]) > 3 * 40, "an L2CAP search returns all three records");
    n = ask(0x110e, 0xffff, -1, o);
    const unsigned char cat2[] = {0x09, 0x03, 0x11, 0x09, 0x00, 0x02};
    check(has(o + 7, o[5] << 8 | o[6], cat2, 6), "the controller record offers category 2 (absolute volume)");
    // The same answer in 16-byte pieces.
    unsigned char whole[1100], got[1100];
    const int wn = ask(0x110e, 0xffff, -1, whole), wb = whole[5] << 8 | whole[6];
    int have = 0, cont = -1, pieces = 0;
    do {
        n = ask(0x110e, 16, cont, o);
        const int b = o[5] << 8 | o[6];
        memcpy(got + have, o + 7, (size_t)b);
        have += b;
        cont = o[7 + b] == 2 ? o[8 + b] << 8 | o[9 + b] : -1;
        pieces++;
    } while (cont >= 0 && pieces < 50);
    check(wn > 0 && have == wb && !memcmp(got, whole + 7, (size_t)wb) && pieces > 2, "continuation reassembles the same answer");
    unsigned char bad[5] = {0x06, 0, 1, 0, 0};
    check(sdp_answer(bad, 5, o) == 7 && o[0] == 0x01, "a malformed request gets an error, not a crash");
    return fails != 0;
}
C
cc -O2 -Wall -Wextra -Werror -I"$HERE" -o "$T/t" "$T/t.c"
"$T/t"
echo "test_bt_ctl: all passed"
