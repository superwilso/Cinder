// bt_ctl.h — what headphones ask a Bluetooth audio source besides the music (btsource.c, Phase 4a):
// the service directory (SDP, L2CAP PSM 1) and remote control (AVRCP over AVCTP, PSM 23).
//
// Seen 11-22 with no SDP server: the CMF Buds Pro 2 asked SDP twice, were refused, and never opened
// remote control, so their buttons and volume could not reach the player. These answer both with
// three fixed records (A2DP source, AVRCP target, AVRCP controller, category 2 = absolute volume).
// Header-only, static; everything runs on btsource's control thread.
#ifndef CINDER_BT_CTL_H
#define CINDER_BT_CTL_H
#include <stdint.h>
#include <stdio.h>
#include <string.h>

// ── SDP ───────────────────────────────────────────────────────────────────────────────────────

// One data element: header length and data length at p (n bytes left); -1 if it does not fit.
static inline int sdp_elem(const unsigned char* p, int n, int* hl, int* dl) {
    if (n < 1) return -1;
    const int type = p[0] >> 3, idx = p[0] & 7;
    if (type == 0) { *hl = 1; *dl = 0; }
    else if (idx <= 4 && type != 6 && type != 7 && type != 4 && type != 8) { *hl = 1; *dl = 1 << idx; }
    else if (idx == 5 && n >= 2) { *hl = 2; *dl = p[1]; }
    else if (idx == 6 && n >= 3) { *hl = 3; *dl = p[1] << 8 | p[2]; }
    else if (idx == 7 && n >= 5) { *hl = 5; *dl = (int)((unsigned)p[1] << 24 | p[2] << 16 | p[3] << 8 | p[4]); }
    else return -1;
    return *hl + *dl <= n ? 0 : -1;
}

struct sdp_rec { uint32_t handle; uint16_t uuids[6]; int nuuids; unsigned char attrs[128]; int len; };
static struct sdp_rec g_recs[3];

// Attribute list builders. Each attribute: uint16 id, then its value; ids ascending.
static inline void b_put(struct sdp_rec* r, const unsigned char* b, int n) { memcpy(r->attrs + r->len, b, (size_t)n); r->len += n; }
static inline void b_u16(struct sdp_rec* r, unsigned v) { const unsigned char b[3] = {0x09, (unsigned char)(v >> 8), (unsigned char)v}; b_put(r, b, 3); }
static inline void b_uuid(struct sdp_rec* r, unsigned v) {
    const unsigned char b[3] = {0x19, (unsigned char)(v >> 8), (unsigned char)v};
    b_put(r, b, 3);
    r->uuids[r->nuuids++] = (uint16_t)v;
}
static inline int b_seq(struct sdp_rec* r) { const unsigned char b[2] = {0x35, 0}; b_put(r, b, 2); return r->len - 1; }
static inline void b_end(struct sdp_rec* r, int at) { r->attrs[at] = (unsigned char)(r->len - at - 1); }

// class1/class2: service classes; proto: 0x0019 AVDTP or 0x0017 AVCTP; profile + version; features.
static inline void sdp_record(struct sdp_rec* r, uint32_t handle, unsigned class1, unsigned class2, unsigned psm,
                              unsigned proto, unsigned proto_ver, unsigned profile, unsigned prof_ver, unsigned features) {
    memset(r, 0, sizeof *r);
    r->handle = handle;
    r->uuids[r->nuuids++] = 0x1002;   // the public browse group, below
    b_u16(r, 0x0000);
    const unsigned char h[5] = {0x0a, (unsigned char)(handle >> 24), (unsigned char)(handle >> 16), (unsigned char)(handle >> 8), (unsigned char)handle};
    b_put(r, h, 5);
    b_u16(r, 0x0001);
    int s = b_seq(r);
    b_uuid(r, class1);
    if (class2) b_uuid(r, class2);
    b_end(r, s);
    b_u16(r, 0x0004);
    s = b_seq(r);
    int t = b_seq(r);
    b_uuid(r, 0x0100);   // L2CAP
    b_u16(r, psm);
    b_end(r, t);
    t = b_seq(r);
    b_uuid(r, proto);
    b_u16(r, proto_ver);
    b_end(r, t);
    b_end(r, s);
    b_u16(r, 0x0005);
    s = b_seq(r);
    const unsigned char br[3] = {0x19, 0x10, 0x02};
    b_put(r, br, 3);
    b_end(r, s);
    b_u16(r, 0x0009);
    s = b_seq(r);
    t = b_seq(r);
    b_uuid(r, profile);
    b_u16(r, prof_ver);
    b_end(r, t);
    b_end(r, s);
    b_u16(r, 0x0311);
    b_u16(r, features);
}

static inline void sdp_init(void) {
    sdp_record(&g_recs[0], 0x10001, 0x110a, 0, 25, 0x0019, 0x0103, 0x110d, 0x0103, 0x0001);         // A2DP source, player
    sdp_record(&g_recs[1], 0x10002, 0x110c, 0, 23, 0x0017, 0x0104, 0x110e, 0x0106, 0x0001);         // AVRCP target, category 1
    sdp_record(&g_recs[2], 0x10003, 0x110e, 0x110f, 23, 0x0017, 0x0104, 0x110e, 0x0106, 0x0002);    // AVRCP controller, category 2
}

// Every UUID of the search pattern (16-bit, or the Bluetooth base form of 32/128-bit) in the record.
static inline int sdp_match(const struct sdp_rec* r, const unsigned char* p, int n) {
    int hl, dl, any = 0;
    if (sdp_elem(p, n, &hl, &dl) || (p[0] >> 3) != 6) return 0;
    for (int i = hl; i < hl + dl;) {
        int ehl, edl;
        if (sdp_elem(p + i, hl + dl - i, &ehl, &edl)) return 0;
        const unsigned char* u = p + i + ehl;
        unsigned v = 0;
        if ((p[i] >> 3) == 3 && edl == 2) v = (unsigned)(u[0] << 8 | u[1]);
        else if ((p[i] >> 3) == 3 && (edl == 4 || edl == 16) && !u[0] && !u[1]) v = (unsigned)(u[2] << 8 | u[3]);
        int found = 0;
        for (int k = 0; k < r->nuuids; k++) found |= r->uuids[k] == v;
        if (!found) return 0;
        any = 1;
        i += ehl + edl;
    }
    return any;
}

// The attributes of r that the id list asks for, as one sequence, appended to out.
static inline int sdp_attrs(const struct sdp_rec* r, const unsigned char* ids, int n, unsigned char* out) {
    int hl, dl, o = 3;
    if (sdp_elem(ids, n, &hl, &dl)) return 0;
    for (int a = 0; a < r->len;) {
        const unsigned id = (unsigned)(r->attrs[a + 1] << 8 | r->attrs[a + 2]);
        int vhl = 0, vdl = 0;
        if (sdp_elem(r->attrs + a + 3, r->len - a - 3, &vhl, &vdl)) break;
        const int alen = 3 + vhl + vdl;
        int want = 0;
        for (int i = hl; i < hl + dl;) {
            int ehl, edl;
            if (sdp_elem(ids + i, hl + dl - i, &ehl, &edl)) break;
            const unsigned char* v = ids + i + ehl;
            if (edl == 2) want |= id == (unsigned)(v[0] << 8 | v[1]);
            if (edl == 4) want |= id >= (unsigned)(v[0] << 8 | v[1]) && id <= (unsigned)(v[2] << 8 | v[3]);
            i += ehl + edl;
        }
        if (want) { memcpy(out + o, r->attrs + a, (size_t)alen); o += alen; }
        a += alen;
    }
    out[0] = 0x36;   // sequence, 2-byte length
    out[1] = (unsigned char)((o - 3) >> 8);
    out[2] = (unsigned char)(o - 3);
    return o;
}

// One request in, one response out (returns its length). Continuation = a 2-byte offset into the
// whole answer, which is rebuilt the same way each time.
static inline int sdp_answer(const unsigned char* q, int n, unsigned char* out) {
    static unsigned char all[1024];
    if (n < 5) return 0;
    const unsigned char pdu = q[0];
    const unsigned char* p = q + 5;
    const int pn = n - 5;
    int hl, dl, o = 0, total = 0;
    memcpy(out + 1, q + 1, 2);   // transaction id
    if (pdu == 0x06 || pdu == 0x02) {   // service search (+ attributes)
        if (sdp_elem(p, pn, &hl, &dl)) goto bad;
        const unsigned char* pat = p;
        const int patn = hl + dl, rest = pn - patn;
        const unsigned char* r = p + patn;
        if (rest < 3) goto bad;
        const int max = r[0] << 8 | r[1];
        if (pdu == 0x02) {   // handles only; three records always fit
            int cnt = 0;
            for (int k = 0; k < 3; k++)
                if (sdp_match(&g_recs[k], pat, patn) && cnt < max) {
                    const uint32_t h = g_recs[k].handle;
                    all[4 + 4 * cnt] = (unsigned char)(h >> 24); all[5 + 4 * cnt] = (unsigned char)(h >> 16);
                    all[6 + 4 * cnt] = (unsigned char)(h >> 8); all[7 + 4 * cnt] = (unsigned char)h;
                    cnt++;
                }
            out[0] = 0x03;
            memcpy(out + 5, (unsigned char[]){0, (unsigned char)cnt, 0, (unsigned char)cnt}, 4);
            memcpy(out + 9, all + 4, (size_t)(4 * cnt));
            o = 9 + 4 * cnt;
            out[o++] = 0;
            out[3] = (unsigned char)((o - 5) >> 8);
            out[4] = (unsigned char)(o - 5);
            return o;
        }
        const unsigned char* ids = r + 2;
        int ihl, idl;
        if (sdp_elem(ids, rest - 2, &ihl, &idl)) goto bad;
        const unsigned char* cont = ids + ihl + idl;
        total = 3;
        for (int k = 0; k < 3; k++)
            if (sdp_match(&g_recs[k], pat, patn)) total += sdp_attrs(&g_recs[k], ids, ihl + idl, all + total);
        all[0] = 0x36; all[1] = (unsigned char)((total - 3) >> 8); all[2] = (unsigned char)(total - 3);
        const int from = cont < q + n && cont[0] == 2 ? cont[1] << 8 | cont[2] : 0;
        int take = total - from;
        if (take > max) take = max;
        if (take < 0) take = 0;
        out[0] = 0x07;
        out[5] = (unsigned char)(take >> 8); out[6] = (unsigned char)take;
        memcpy(out + 7, all + from, (size_t)take);
        o = 7 + take;
        if (from + take < total) { out[o++] = 2; out[o++] = (unsigned char)((from + take) >> 8); out[o++] = (unsigned char)(from + take); }
        else out[o++] = 0;
        out[3] = (unsigned char)((o - 5) >> 8);
        out[4] = (unsigned char)(o - 5);
        return o;
    }
    if (pdu == 0x04 && pn >= 7) {   // attributes of one record
        const uint32_t h = (uint32_t)p[0] << 24 | p[1] << 16 | p[2] << 8 | p[3];
        const int max = p[4] << 8 | p[5];
        const unsigned char* ids = p + 6;
        int ihl, idl;
        if (sdp_elem(ids, pn - 6, &ihl, &idl)) goto bad;
        const unsigned char* cont = ids + ihl + idl;
        for (int k = 0; k < 3; k++) if (g_recs[k].handle == h) total = sdp_attrs(&g_recs[k], ids, ihl + idl, all);
        if (!total) { out[0] = 0x01; out[3] = 0; out[4] = 2; out[5] = 0; out[6] = 2; return 7; }   // invalid handle
        const int from = cont < q + n && cont[0] == 2 ? cont[1] << 8 | cont[2] : 0;
        int take = total - from;
        if (take > max) take = max;
        if (take < 0) take = 0;
        out[0] = 0x05;
        out[5] = (unsigned char)(take >> 8); out[6] = (unsigned char)take;
        memcpy(out + 7, all + from, (size_t)take);
        o = 7 + take;
        if (from + take < total) { out[o++] = 2; out[o++] = (unsigned char)((from + take) >> 8); out[o++] = (unsigned char)(from + take); }
        else out[o++] = 0;
        out[3] = (unsigned char)((o - 5) >> 8);
        out[4] = (unsigned char)(o - 5);
        return o;
    }
bad:
    out[0] = 0x01;   // error: invalid request syntax
    out[3] = 0; out[4] = 2; out[5] = 0; out[6] = 3;
    return 7;
}

// ── AVRCP ─────────────────────────────────────────────────────────────────────────────────────

enum { AVC_CONTROL = 0x00, AVC_STATUS = 0x01, AVC_NOTIFY = 0x03, AVC_NOT_IMPLEMENTED = 0x08, AVC_ACCEPTED = 0x09,
       AVC_REJECTED = 0x0a, AVC_STABLE = 0x0c, AVC_CHANGED = 0x0d, AVC_INTERIM = 0x0f };

// AVCTP frame: label, command (0) or response (1), then the AV/C frame (ctype, panel subunit, opcode, operands).
static inline int avctp_frame(unsigned char* b, int label, int response, int ctype, int opcode, const unsigned char* ops, int n) {
    b[0] = (unsigned char)(label << 4 | response << 1);
    b[1] = 0x11; b[2] = 0x0e;   // AV remote control
    b[3] = (unsigned char)ctype;
    b[4] = 0x48;                // panel subunit, id 0
    b[5] = (unsigned char)opcode;
    memcpy(b + 6, ops, (size_t)n);
    return 6 + n;
}

// Vendor-dependent (Bluetooth SIG) operands: company 00 19 58, PDU, single packet, parameters.
static inline int avrcp_vd(unsigned char* ops, int pdu, const unsigned char* par, int n) {
    ops[0] = 0x00; ops[1] = 0x19; ops[2] = 0x58;
    ops[3] = (unsigned char)pdu; ops[4] = 0;
    ops[5] = (unsigned char)(n >> 8); ops[6] = (unsigned char)n;
    memcpy(ops + 7, par, (size_t)n);
    return 7 + n;
}

#endif
