// btsource.c — Phase 4a (docs/PLAN_hardware_first.md): an A2DP source on Linux's Bluetooth stack
// that plays what Sony's sound service sends, after all of Sony's effects.
//
//   btsource AA:BB:CC:DD:EE:FF [GAIN_DB]      needs `btbridge -w` holding hci0, Sony's Bluetooth off
//
// 1. Loads the link key Sony's stack saved for these headphones (/data/Bluetooth/devdb/devdb) into
//    the kernel, with pairing switched OFF: a key that does not match fails the link, and nothing
//    here can make a new pairing that would replace the one the headphones keep for Sony's stack.
// 2. Connects A2DP signalling (needs the player's real address: btbridge without CINDER_BT_ALT),
//    lists the headphones' endpoints, and keeps reconnecting every 5 s while they are away.
// 3. Listens on the abstract socket `cinder.a2dp` and names it in /tmp/cinder_a2dp_target, rewritten
//    each second: the mono shim then sends the sound service's Bluetooth stream here instead of to
//    Sony's transmitter, and cinder-pathwatch keeps PathMgr's route on Bluetooth while it is fresh.
// 4. Per connection from the sound service: the type-1 frame gives rate and channels; the PCM
//    (S16) is scaled by GAIN_DB (default -20, for headphones without absolute volume; 0 dB when they
//    have it, since they then apply the volume; /tmp/cinder_bt_gain, one number in dB, overrides
//    both live), encoded (bt_enc.h: LDAC, else AAC, else SBC, as bt_codec.h picks; CINDER_BT_CODEC=
//    ldac|aac|sbc allows only that one) and sent as RTP at the real-time pace. The stream is suspended
//    when the sound service hangs up (pause).
//
// 5. A control thread answers the headphones' service directory (SDP) and remote control (AVRCP,
//    bt_ctl.h): their buttons reach Cinder's command channel (/tmp/cinder_cmd, dev builds), their
//    volume is logged, and /tmp/cinder_bt_volume (0-127) sets it when they support absolute volume.
//
// Every exit removes the target, so the next Play goes to the jack (or Sony's stack) again.
#include <fcntl.h>
#include <math.h>
#include <pthread.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/un.h>

#include "bt_enc.h"
#include "bt_l2.h"
#include "bt_ctl.h"

#define SOL_BLUETOOTH 274
#define BT_SECURITY 4
static const char kTarget[] = "/tmp/cinder_a2dp_target", kName[] = "cinder.a2dp", kGain[] = "/tmp/cinder_bt_gain";

static volatile sig_atomic_t g_quit;
static volatile int g_linked, g_playing;   // main thread writes, control thread reads
static volatile long long g_linked_at;
static void on_signal(int s) { (void)s; g_quit = 1; }

// Sony's record (blueangel bt_ddb4w.c, the Blue SDK BtDeviceRecord): 8-byte header, then 0x80-byte
// records of address (6, little-endian), trusted (1), link key (16), key type (1). Read 11-22: both
// of the owner's headphones at type 5 (HCI: authenticated combination key, P-192).
static int load_key(unsigned char key[16], unsigned char* type) {
    unsigned char db[8192];
    const int f = open("/data/Bluetooth/devdb/devdb", O_RDONLY);
    if (f < 0) return -1;
    const ssize_t n = read(f, db, sizeof db);
    close(f);
    for (ssize_t o = 8; o + 0x80 <= n; o += 0x80)
        if (!memcmp(db + o, g_addr, 6)) {
            memcpy(key, db + o + 7, 16);
            *type = db[o + 23];
            return 0;
        }
    return -1;
}

// Pairing off, our key in. Returns 0 when the kernel took the key.
static int kernel_setup(void) {
    unsigned char key[16], type;
    if (load_key(key, &type)) { printf("no saved key for this address in Sony's device list\n"); return -1; }
    const unsigned char on = 1, off = 0, noio = 3;
    mgmt(0x0005, &on, 1, 5000, "power on");
    mgmt(0x0009, &off, 1, 3000, "pairing off");
    mgmt(0x000b, &on, 1, 3000, "secure simple pair");
    mgmt(0x0018, &noio, 1, 3000, "no keyboard/screen");
    unsigned char lk[3 + 25] = {0, 1, 0};   // debug keys off, one key
    memcpy(lk + 3, g_addr, 6);
    lk[9] = 0;      // BR/EDR
    lk[10] = type;
    memcpy(lk + 11, key, 16);
    lk[27] = 0;     // PIN length
    printf("Sony's key for this device: type %u\n", type);
    return mgmt(0x0012, lk, sizeof lk, 3000, "load link key") == 0 ? 0 : -1;
}

static int l2cap(int psm) {
    int s = socket(AF_BT, SOCK_SEQPACKET, 0);
    unsigned char la[14] = {AF_BT, 0}, ra[14] = {AF_BT, 0, (unsigned char)psm, 0};
    memcpy(ra + 4, g_addr, 6);
    const unsigned char sec[2] = {2, 0};   // BT_SECURITY_MEDIUM: authenticated with the key, encrypted
    if (s < 0) return -1;
    if (setsockopt(s, SOL_BLUETOOTH, BT_SECURITY, sec, sizeof sec) < 0 || bind(s, (struct sockaddr*)la, sizeof la) < 0
        || connect(s, (struct sockaddr*)ra, sizeof ra) < 0) {
        printf("  L2CAP %d: %s\n", psm, strerror(errno));
        close(s);
        return -1;
    }
    return s;
}

static unsigned char g_label = 1;
static unsigned char next_label(void) { g_label = (unsigned char)((g_label + 1) & 15); return g_label; }

// The link: A2DP signalling, the headphones' endpoints and the one configured.
struct link {
    int sig, media, mtu, rate, cur;   // rate 0 = not configured; cur = index into eps
    int n;
    struct bt_ep eps[8];
};
static int g_only = BT_NONE;   // CINDER_BT_CODEC: the one codec allowed, for testing each

static int link_up(struct link* k) {
    memset(k, 0, sizeof *k);
    k->media = -1;
    k->sig = l2cap(25);
    if (k->sig < 0) return -1;
    unsigned char r[512];
    const int n = avdtp(k->sig, next_label(), 0x01, NULL, 0, r, sizeof r);   // DISCOVER
    for (int i = 2; i + 1 < n && k->n < 8; i += 2) {
        const int seid = r[i] >> 2, inuse = (r[i] >> 1) & 1, sink = (r[i + 1] >> 3) & 1;
        if (!sink || inuse) continue;
        unsigned char q = (unsigned char)(seid << 2), c[256];
        const int m = avdtp(k->sig, next_label(), 0x02, &q, 1, c, sizeof c);   // GET_CAPABILITIES
        for (int j = 2; m > 0 && j + 1 < m && j + 2 + c[j + 1] <= m; j += 2 + c[j + 1]) {
            const int len = c[j + 1] < 16 ? c[j + 1] : 16, codec = bt_codec_of(c + j + 2, len);
            if (c[j] != 7 || codec == BT_NONE || (g_only != BT_NONE && codec != g_only)) continue;
            struct bt_ep* e = &k->eps[k->n++];
            e->seid = seid;
            e->codec = codec;
            e->len = len;
            memcpy(e->cap, c + j + 2, (size_t)len);
            printf("endpoint %d: %s\n", seid, bt_codec_name(codec));
        }
    }
    if (!k->n) { printf("no free endpoint on the headphones in a codec this sends\n"); close(k->sig); k->sig = -1; return -1; }
    printf("linked: %d endpoint(s)\n", k->n);
    return 0;
}

// The headphones' AAC bit rate cap, bounded to 320 kbit/s.
static unsigned aac_max(const struct bt_ep* e) {
    const unsigned m = (unsigned)(e->cap[5] & 0x7f) << 16 | e->cap[6] << 8 | e->cap[7];
    return m == 0 || m > 320000 ? 320000 : m;
}

static void link_down(struct link* k) {
    if (k->media >= 0) close(k->media);
    if (k->sig >= 0) close(k->sig);
    k->media = k->sig = -1;
    k->rate = 0;
}

// Choose and configure the endpoint for `rate`, open the media channel, start. Returns 0 when
// streaming may begin; k->eps[k->cur] is the codec. LDAC is passed over when the media channel
// comes up too small for it (configured again, once, without it).
static void stream_close(struct link* k) {
    unsigned char r[64];
    const unsigned char se = (unsigned char)(k->eps[k->cur].seid << 2);
    avdtp(k->sig, next_label(), 0x08, &se, 1, r, sizeof r);   // CLOSE
    if (k->media >= 0) close(k->media);
    k->media = -1;
    k->rate = 0;
}

static int stream_start(struct link* k, int rate) {
    unsigned char r[256];
    if (k->rate && k->rate != rate) stream_close(k);   // a new rate: close and configure again
    for (int ldac_ok = 1; !k->rate; ldac_ok = 0) {
        k->cur = bt_pick(k->eps, k->n, rate, ldac_ok);
        if (k->cur < 0) { k->cur = 0; printf("no endpoint takes %d Hz\n", rate); return -1; }
        const struct bt_ep* e = &k->eps[k->cur];
        const unsigned char se = (unsigned char)(e->seid << 2);
        unsigned char cfg[24] = {se, 1 << 2, 1, 0, 7, 0};
        const int len = bt_config(e, rate, aac_max(e), cfg + 6);
        cfg[5] = (unsigned char)len;
        if (avdtp(k->sig, next_label(), 0x03, cfg, 6 + len, r, sizeof r) < 0) { printf("set configuration refused (%s)\n", bt_codec_name(e->codec)); return -1; }
        if (avdtp(k->sig, next_label(), 0x06, &se, 1, r, sizeof r) < 0) { printf("open refused\n"); return -1; }
        k->media = l2cap(25);
        if (k->media < 0) return -1;
        unsigned short opt[6] = {0};   // struct l2cap_options: omtu first
        socklen_t ol = sizeof opt;
        getsockopt(k->media, 6 /* SOL_L2CAP */, 1 /* L2CAP_OPTIONS */, opt, &ol);
        k->mtu = opt[0] ? opt[0] : 672;
        k->rate = rate;
        if (e->codec == BT_LDAC && k->mtu < BT_LDAC_MTU_MIN + 13 && ldac_ok) {
            printf("media MTU %d is too small for LDAC: configuring again without it\n", k->mtu);
            stream_close(k);
        }
    }
    const unsigned char se = (unsigned char)(k->eps[k->cur].seid << 2);
    if (avdtp(k->sig, next_label(), 0x07, &se, 1, r, sizeof r) < 0) { printf("start refused\n"); return -1; }
    return 0;
}

static void stream_suspend(struct link* k) {
    unsigned char r[64];
    const unsigned char se = (unsigned char)(k->eps[k->cur].seid << 2);
    avdtp(k->sig, next_label(), 0x09, &se, 1, r, sizeof r);
}

static int read_all(int fd, void* b, size_t n) {
    for (size_t got = 0; got < n;) {
        const ssize_t r = read(fd, (char*)b + got, n - got);
        if (r <= 0) return -1;
        got += (size_t)r;
    }
    return 0;
}

static double gain_db(double dflt) {
    char b[32] = "";
    const int f = open(kGain, O_RDONLY);
    if (f < 0) return dflt;
    const ssize_t n = read(f, b, sizeof b - 1);
    close(f);
    return n > 0 ? atof(b) : dflt;
}

static void target_touch(void) {
    const int f = open(kTarget, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
    if (f < 0) return;
    fchmod(f, 0644);
    (void)!write(f, kName, sizeof kName - 1);
    close(f);
}

// ── control thread: SDP and AVRCP ─────────────────────────────────────────────────────────────

static int l2_listen(int psm) {
    int s = socket(AF_BT, SOCK_SEQPACKET, 0);
    unsigned char la[14] = {AF_BT, 0, (unsigned char)psm, (unsigned char)(psm >> 8)};
    if (s < 0 || bind(s, (struct sockaddr*)la, sizeof la) < 0 || listen(s, 2) < 0) {
        printf("listen PSM %d: %s\n", psm, strerror(errno));
        if (s >= 0) close(s);
        return -1;
    }
    return s;
}

static void cinder_cmd(const char* c) {
    const int f = open("/tmp/cinder_cmd", O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
    if (f < 0) return;
    fchmod(f, 0644);
    (void)!write(f, c, strlen(c));
    (void)!write(f, "\n", 1);
    close(f);
}

static int g_av = -1, g_av_label, g_play_label = -1, g_vol = -1;
static volatile int g_abs_vol;   // the headphones apply the volume themselves (AVRCP absolute volume)

// The headphones' level for Cinder's volume keys, which step from it (/tmp/cinder_bt_volume).
static void vol_publish(int v) {
    g_vol = v;
    const int f = open("/tmp/cinder_bt_volume_now", O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
    if (f < 0) return;
    fchmod(f, 0644);
    char b[8];
    (void)!write(f, b, (size_t)snprintf(b, sizeof b, "%d\n", v));
    close(f);
}

static void av_send(int label, int response, int ctype, int opcode, const unsigned char* ops, int n) {
    unsigned char b[64];
    if (g_av >= 0) send(g_av, b, (size_t)avctp_frame(b, label, response, ctype, opcode, ops, n), 0);
}

static void av_ask(int ctype, int pdu, const unsigned char* par, int n) {   // a command of ours
    unsigned char ops[32];
    g_av_label = (g_av_label + 1) & 15;
    av_send(g_av_label, 0, ctype, 0x00, ops, avrcp_vd(ops, pdu, par, n));
}

static void av_up(void) {
    printf("remote control (AVRCP) connected\n");
    g_play_label = -1;
    g_abs_vol = 0;
    const unsigned char ev = 0x03;
    av_ask(AVC_STATUS, 0x10, &ev, 1);   // their events: absolute volume?
}

static void av_input(const unsigned char* b, int n) {
    if (n < 6) return;
    const int label = b[0] >> 4, response = (b[0] >> 1) & 1, ctype = b[3], opcode = b[5];
    const unsigned char* ops = b + 6;
    const int on = n - 6;
    unsigned char out[32];
    if (b[1] != 0x11 || b[2] != 0x0e) {   // not remote control: invalid profile
        const unsigned char r[3] = {(unsigned char)(label << 4 | 2 | 1), b[1], b[2]};
        send(g_av, r, 3, 0);
        return;
    }
    if (!response) {
        if (opcode == 0x7c && on >= 2) {   // pass through: a button
            av_send(label, 1, AVC_ACCEPTED, opcode, ops, on);
            const int op = ops[0] & 0x7f;
            if (ops[0] & 0x80) return;   // the release
            const char* c = op == 0x44 ? "play" : op == 0x46 || op == 0x45 ? "pause" : op == 0x4b ? "next" : op == 0x4c ? "prev" : NULL;
            printf("headphones pressed 0x%02x%s%s\n", op, c ? " -> " : " (ignored)", c ? c : "");
            if (c) cinder_cmd(c);
        } else if (opcode == 0x30 || opcode == 0x31) {   // unit / subunit info
            const unsigned char r[5] = {0x07, 0x48, 0xff, 0xff, 0xff};
            av_send(label, 1, AVC_STABLE, opcode, r, 5);
        } else if (opcode == 0x00 && on >= 7) {
            const int pdu = ops[3];
            const unsigned char* par = ops + 7;
            if (pdu == 0x10 && on >= 8 && par[0] == 0x02) {
                const unsigned char r[5] = {0x02, 1, 0x00, 0x19, 0x58};
                av_send(label, 1, AVC_STABLE, 0, out, avrcp_vd(out, pdu, r, 5));
            } else if (pdu == 0x10 && on >= 8 && par[0] == 0x03) {
                const unsigned char r[3] = {0x03, 1, 0x01};   // playback status changes
                av_send(label, 1, AVC_STABLE, 0, out, avrcp_vd(out, pdu, r, 3));
            } else if (pdu == 0x31 && on >= 8 && par[0] == 0x01) {
                g_play_label = label;
                const unsigned char r[2] = {0x01, (unsigned char)(g_playing ? 1 : 2)};
                av_send(label, 1, AVC_INTERIM, 0, out, avrcp_vd(out, pdu, r, 2));
            } else if (pdu == 0x30) {
                const unsigned char r[9] = {0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, (unsigned char)(g_playing ? 1 : 2)};
                av_send(label, 1, AVC_STABLE, 0, out, avrcp_vd(out, pdu, r, 9));
            } else {
                const unsigned char r[1] = {pdu == 0x31 ? 0x01 : 0x00};   // invalid parameter / command
                av_send(label, 1, AVC_REJECTED, 0, out, avrcp_vd(out, pdu, r, 1));
                printf("headphones asked PDU 0x%02x (refused)\n", pdu);
            }
        } else {
            av_send(label, 1, AVC_NOT_IMPLEMENTED, opcode, ops, on);
        }
        return;
    }
    // Answers to ours.
    if (opcode != 0x00 || on < 8) return;
    const int pdu = ops[3];
    const unsigned char* par = ops + 7;
    if (pdu == 0x10 && ctype == AVC_STABLE && par[0] == 0x03) {
        for (int i = 0; i < par[1] && 2 + i < on - 7; i++) g_abs_vol |= par[2 + i] == 0x0d;
        printf("headphones: absolute volume %s\n", g_abs_vol ? "yes" : "no");
        if (g_abs_vol) {
            const unsigned char r[5] = {0x0d, 0, 0, 0, 0};
            av_ask(AVC_NOTIFY, 0x31, r, 5);
        }
    } else if (pdu == 0x31 && par[0] == 0x0d && on >= 9) {
        vol_publish(par[1] & 0x7f);
        printf("headphones' volume %d/127%s\n", g_vol, ctype == AVC_CHANGED ? " (changed)" : "");
        if (ctype == AVC_CHANGED) {
            const unsigned char r[5] = {0x0d, 0, 0, 0, 0};
            av_ask(AVC_NOTIFY, 0x31, r, 5);
        }
    } else if (pdu == 0x50 && on >= 8) {
        vol_publish(par[0] & 0x7f);
        printf("headphones' volume set: %d/127 (%s)\n", g_vol, ctype == AVC_ACCEPTED ? "accepted" : "refused");
    }
}

static void* control(void* arg) {
    (void)arg;
    sdp_init();
    const int sdp_l = l2_listen(1), av_l = l2_listen(23);
    int sdp_c[4] = {-1, -1, -1, -1}, last_play = -1, last_set = -1;
    long long tried = 0;
    while (!g_quit) {
        struct pollfd pf[8];
        int n = 0;
        pf[n++] = (struct pollfd){sdp_l, POLLIN, 0};
        pf[n++] = (struct pollfd){av_l, POLLIN, 0};
        pf[n++] = (struct pollfd){g_av, POLLIN, 0};
        for (int i = 0; i < 4; i++) pf[n++] = (struct pollfd){sdp_c[i], POLLIN, 0};
        poll(pf, (nfds_t)n, 200);   // also the pace of the volume file
        unsigned char b[1024], out[1100];
        if (pf[0].revents & POLLIN) {
            const int c = accept(sdp_l, NULL, NULL);
            int put = 0;
            for (int i = 0; i < 4 && c >= 0 && !put; i++) if (sdp_c[i] < 0) { sdp_c[i] = c; put = 1; }
            if (c >= 0 && !put) close(c);
        }
        for (int i = 0; i < 4; i++) {
            if (sdp_c[i] < 0 || !(pf[3 + i].revents & (POLLIN | POLLHUP | POLLERR))) continue;
            const int r = (int)recv(sdp_c[i], b, sizeof b, 0);
            if (r <= 0) { close(sdp_c[i]); sdp_c[i] = -1; continue; }
            const int o = sdp_answer(b, r, out);
            printf("service directory: request 0x%02x answered 0x%02x (%d bytes)\n", b[0], out[0], o);
            if (o > 0) send(sdp_c[i], out, (size_t)o, 0);
        }
        if (pf[1].revents & POLLIN) {
            const int c = accept(av_l, NULL, NULL);
            if (c >= 0 && g_av < 0) { g_av = c; av_up(); }
            else if (c >= 0) close(c);
        }
        if (g_av >= 0 && (pf[2].revents & (POLLIN | POLLHUP | POLLERR))) {
            const int r = (int)recv(g_av, b, sizeof b, 0);
            if (r <= 0) { printf("remote control closed\n"); close(g_av); g_av = -1; }
            else av_input(b, r);
        }
        if (!g_linked && g_av >= 0) { close(g_av); g_av = -1; }
        // Ours, 2 s after the music link, unless the headphones opened it first.
        if (g_linked && g_av < 0 && now_ms() - g_linked_at > 2000 && now_ms() - tried > 10000) {
            tried = now_ms();
            const int c = l2cap(23);
            if (c >= 0 && g_av < 0) { g_av = c; av_up(); }
            else if (c >= 0) close(c);
        }
        if (g_av >= 0 && g_play_label >= 0 && g_playing != last_play) {
            unsigned char ops[16];
            const unsigned char r[2] = {0x01, (unsigned char)(g_playing ? 1 : 2)};
            av_send(g_play_label, 1, AVC_CHANGED, 0, ops, avrcp_vd(ops, 0x31, r, 2));
            g_play_label = -1;   // they register again
        }
        last_play = g_playing;
        if (g_av >= 0 && g_abs_vol) {
            char vb[16] = "";
            const int f = open("/tmp/cinder_bt_volume", O_RDONLY);
            if (f >= 0) { (void)!read(f, vb, sizeof vb - 1); close(f); }
            const int want = vb[0] ? atoi(vb) : -1;
            if (want >= 0 && want <= 127 && want != last_set) {
                last_set = want;
                const unsigned char v = (unsigned char)want;
                av_ask(AVC_CONTROL, 0x50, &v, 1);
            }
        }
    }
    return NULL;
}

// One connection from the sound service: handshake, then PCM until it hangs up. Returns -1 when the
// headphones are gone (the caller reconnects), 0 otherwise.
static int serve(int c, struct link* k, double dflt_db) {
    unsigned hdr[2];
    unsigned char cfg[28];
    for (;;) {
        if (read_all(c, hdr, 8)) return 0;
        if (hdr[0] == 1 && hdr[1] == 28) break;
        unsigned char skip[12];
        if ((hdr[0] == 0 && hdr[1] == 0) || (hdr[0] == 2 && hdr[1] == 12 && !read_all(c, skip, 12))) continue;
        printf("sound service sent frame type %u length %u: hung up\n", hdr[0], hdr[1]);
        return 0;
    }
    if (read_all(c, cfg, 28)) return 0;
    const int chans = cfg[4], rate = cfg[24] | cfg[25] << 8 | cfg[26] << 16 | cfg[27] << 24;
    printf("sound service: %d Hz, %d channel(s), flag %u\n", rate, chans, cfg[20]);
    if ((rate != 44100 && rate != 48000) || chans < 1 || chans > 2) {
        printf("not a format this sends over Bluetooth\n");
        return 0;
    }
    if (stream_start(k, rate)) return -1;
    const char* why = "";
    static struct bt_enc e;
    if (bt_enc_open(&e, &k->eps[k->cur], rate, k->mtu, (int)aac_max(&k->eps[k->cur]), &why)) {
        printf("%s encoder: %s\n", bt_codec_name(k->eps[k->cur].codec), why);
        stream_suspend(k);
        return 0;
    }
    printf("streaming: %s, media MTU %d\n", bt_codec_name(e.codec), k->mtu);
    g_playing = 1;
    static int16_t in[AAC_FRAME_SAMPLES * 2], pcm[AAC_FRAME_SAMPLES * 2];   // AAC's frame is the largest
    static unsigned char pkt[2048];
    unsigned seq = 0;
    long long frames = 0, sent = 0, lost = 0;
    int rc = 0;
    int g = 0;
    struct timespec t0;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    long long last_touch = 0;
    while (!g_quit) {
        if (read_all(c, in, (size_t)e.samples * 2 * chans)) break;   // paused or stopped
        if (now_ms() - last_touch >= 1000) {
            last_touch = now_ms();
            target_touch();
            // With absolute volume the headphones turn it down themselves, as under Sony's stack.
            const double db = gain_db(g_abs_vol ? 0 : dflt_db);
            g = (int)(32767.0 * pow(10.0, (db > 0 ? 0 : db) / 20.0));
        }
        for (int i = 0; i < e.samples; i++) {
            const int l = in[chans == 2 ? 2 * i : i], r = in[chans == 2 ? 2 * i + 1 : i];
            pcm[2 * i] = (int16_t)((l * g) >> 15);
            pcm[2 * i + 1] = (int16_t)((r * g) >> 15);
        }
        const int n = bt_enc_feed(&e, pcm);
        if (n > 0) {
            const uint32_t ts = e.pkt_ts;
            const unsigned char h[12] = {0x80, BT_RTP_PT, (unsigned char)(seq >> 8), (unsigned char)seq,
                                         (unsigned char)(ts >> 24), (unsigned char)(ts >> 16),
                                         (unsigned char)(ts >> 8), (unsigned char)ts, 0, 0, 0, 1};
            memcpy(pkt, h, 12);
            memcpy(pkt + 12, e.pkt, (size_t)n);
            if (send(k->media, pkt, (size_t)n + 12, 0) == n + 12) {
                sent++;
            } else if (errno == ENOTCONN || errno == ECONNRESET || errno == EPIPE) {
                printf("the headphones went away\n");
                rc = -1;
                break;
            } else {
                lost++;
            }
            seq++;
        } else if (n == -2) {
            lost++;
        } else if (n == -1) {
            printf("%s encoder failed\n", bt_codec_name(e.codec));
            break;
        }
        frames++;
        // The real-time pace, so the sound service is held back as Sony's transmitter holds it. A
        // read that waited (the stream was starved) moves the clock on instead of bursting after.
        long long due = frames * e.samples * 1000000000LL / rate;
        struct timespec now, d = {t0.tv_sec + (time_t)(due / 1000000000LL), t0.tv_nsec + (long)(due % 1000000000LL)};
        if (d.tv_nsec >= 1000000000L) { d.tv_sec++; d.tv_nsec -= 1000000000L; }
        clock_gettime(CLOCK_MONOTONIC, &now);
        if ((now.tv_sec - d.tv_sec) * 1000 + (now.tv_nsec - d.tv_nsec) / 1000000 > 200) {
            t0 = now;
            frames = 0;
        } else {
            clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &d, NULL);
        }
    }
    g_playing = 0;
    printf("stream ended: %lld packets sent, %lld lost, %s at %d kb/s\n", sent, lost, bt_codec_name(e.codec), bt_enc_bitrate(&e) / 1000);
    bt_enc_close(&e);
    if (!rc) stream_suspend(k);
    return rc;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    unsigned a[6];
    if (argc < 2 || sscanf(argv[1], "%x:%x:%x:%x:%x:%x", &a[0], &a[1], &a[2], &a[3], &a[4], &a[5]) != 6) {
        printf("usage: btsource AA:BB:CC:DD:EE:FF [GAIN_DB]\n");
        return 1;
    }
    for (int i = 0; i < 6; i++) g_addr[i] = (unsigned char)a[5 - i];
    const double dflt_db = argc > 2 ? atof(argv[2]) : -20;
    const char* only = getenv("CINDER_BT_CODEC");
    if (only) g_only = !strcmp(only, "ldac") ? BT_LDAC : !strcmp(only, "aac") ? BT_AAC : !strcmp(only, "sbc") ? BT_SBC : BT_NONE;
    signal(SIGINT, on_signal);
    signal(SIGTERM, on_signal);
    signal(SIGPIPE, SIG_IGN);
    g_m = socket(AF_BT, SOCK_RAW | SOCK_CLOEXEC, 1);
    unsigned char sa[6] = {AF_BT, 0, 0xff, 0xff, 3, 0};   // HCI_DEV_NONE, HCI_CHANNEL_CONTROL
    if (g_m < 0 || bind(g_m, (struct sockaddr*)sa, sizeof sa) < 0) { printf("management socket: %s\n", strerror(errno)); return 1; }
    if (kernel_setup()) return 2;
    pthread_t ct;
    pthread_create(&ct, NULL, control, NULL);

    int ls = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    struct sockaddr_un un = {.sun_family = AF_UNIX};
    memcpy(un.sun_path + 1, kName, sizeof kName - 1);
    if (ls < 0 || bind(ls, (struct sockaddr*)&un, (socklen_t)(offsetof(struct sockaddr_un, sun_path) + sizeof kName)) < 0
        || listen(ls, 1) < 0) {
        printf("listen @%s: %s\n", kName, strerror(errno));
        return 1;
    }
    struct link k = {.sig = -1, .media = -1};
    while (!g_quit) {
        if (k.sig < 0) {
            unlink(kTarget);   // the route falls back while the headphones are away
            g_linked = 0;
            if (link_up(&k)) {
                for (int i = 0; i < 50 && !g_quit; i++) usleep(100000);
                continue;
            }
            g_linked_at = now_ms();
            g_linked = 1;
        }
        target_touch();
        struct pollfd pf[2] = {{ls, POLLIN, 0}, {k.sig, POLLIN, 0}};
        if (poll(pf, 2, 1000) <= 0) continue;
        if (pf[1].revents & (POLLHUP | POLLERR)) { printf("the headphones went away\n"); link_down(&k); continue; }
        if (pf[1].revents & POLLIN) {   // the headphones' own signalling: a general reject for now
            unsigned char b[256];
            if (recv(k.sig, b, sizeof b, 0) <= 0) { link_down(&k); continue; }
            if ((b[0] & 3) == 0) {
                const unsigned char rj[2] = {(unsigned char)((b[0] & 0xf0) | 1), b[1]};
                send(k.sig, rj, 2, 0);
            }
            printf("headphones sent signal 0x%02x (general reject)\n", b[1] & 0x3f);
        }
        if (pf[0].revents & POLLIN) {
            const int c = accept(ls, NULL, NULL);
            if (c < 0) continue;
            if (serve(c, &k, dflt_db) < 0) link_down(&k);
            close(c);
        }
    }
    unlink(kTarget);
    link_down(&k);
    printf("stopped\n");
    return 0;
}
