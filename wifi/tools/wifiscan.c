/*
 * wifiscan — trigger an nl80211 scan on one interface and print what it found.
 * Raw generic netlink, no libnl, so it builds static with nothing but a C library.
 *
 *   wifiscan [wlan0]        exit 0 = at least one network seen, 1 = scan ran, nothing seen,
 *                           2 = could not scan (no nl80211, no such interface, scan refused)
 *
 * Output, one network a line:  <bssid> <freq MHz> <signal dBm> <ssid>
 */
#include <errno.h>
#include <linux/genetlink.h>
#include <linux/netlink.h>
#include <linux/nl80211.h>
#include <net/if.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#define BUF 65536
static int g_fd;
static uint32_t g_seq = 1;

struct msg { char b[1024]; size_t len; };

static void put_attr(struct msg* m, uint16_t type, const void* data, uint16_t len)
{
    struct nlattr* a = (struct nlattr*)(m->b + m->len);
    a->nla_type = type;
    a->nla_len = NLA_HDRLEN + len;
    if (len) memcpy((char*)a + NLA_HDRLEN, data, len);
    m->len += NLA_ALIGN(a->nla_len);
}

/* A nested attribute: open returns its offset, close fixes its length. */
static size_t nest_open(struct msg* m, uint16_t type)
{
    size_t at = m->len;
    put_attr(m, type | NLA_F_NESTED, NULL, 0);
    return at;
}
static void nest_close(struct msg* m, size_t at)
{
    ((struct nlattr*)(m->b + at))->nla_len = (uint16_t)(m->len - at);
}

static void start(struct msg* m, uint16_t family, uint8_t cmd, uint16_t flags)
{
    memset(m, 0, sizeof *m);
    struct nlmsghdr* h = (struct nlmsghdr*)m->b;
    h->nlmsg_type = family;
    h->nlmsg_flags = NLM_F_REQUEST | flags;
    h->nlmsg_seq = g_seq++;
    struct genlmsghdr* g = (struct genlmsghdr*)(m->b + NLMSG_HDRLEN);
    g->cmd = cmd;
    g->version = 1;
    m->len = NLMSG_HDRLEN + GENL_HDRLEN;
}

static int send_msg(struct msg* m)
{
    ((struct nlmsghdr*)m->b)->nlmsg_len = (uint32_t)m->len;
    struct sockaddr_nl to = { .nl_family = AF_NETLINK };
    return sendto(g_fd, m->b, m->len, 0, (struct sockaddr*)&to, sizeof to) == (ssize_t)m->len ? 0 : -1;
}

static struct nlattr* find(void* start, int len, uint16_t type)
{
    for (struct nlattr* a = start; len >= NLA_HDRLEN && a->nla_len >= NLA_HDRLEN && a->nla_len <= len;
         len -= NLA_ALIGN(a->nla_len), a = (struct nlattr*)((char*)a + NLA_ALIGN(a->nla_len)))
        if ((a->nla_type & NLA_TYPE_MASK) == type) return a;
    return NULL;
}
#define DATA(a) ((void*)((char*)(a) + NLA_HDRLEN))
#define DLEN(a) ((int)(a)->nla_len - NLA_HDRLEN)

/* Read replies until DONE/ACK. `each` sees every genl message; returns 0, or -errno from an ACK. */
static int recv_all(void (*each)(struct genlmsghdr*, int, void*), void* ctx)
{
    static char buf[BUF];
    for (;;) {
        ssize_t n = recv(g_fd, buf, sizeof buf, 0);
        if (n < 0) return -errno;
        for (struct nlmsghdr* h = (struct nlmsghdr*)buf; NLMSG_OK(h, (size_t)n); h = NLMSG_NEXT(h, n)) {
            if (h->nlmsg_type == NLMSG_DONE) return 0;
            if (h->nlmsg_type == NLMSG_ERROR) return ((struct nlmsgerr*)NLMSG_DATA(h))->error;
            if (each) each((struct genlmsghdr*)NLMSG_DATA(h), (int)(h->nlmsg_len - NLMSG_HDRLEN), ctx);
            if (!(h->nlmsg_flags & NLM_F_MULTI)) return 0;
        }
    }
}

static void got_family(struct genlmsghdr* g, int len, void* ctx)
{
    struct nlattr* id = find((char*)g + GENL_HDRLEN, len - GENL_HDRLEN, CTRL_ATTR_FAMILY_ID);
    if (id) *(uint16_t*)ctx = *(uint16_t*)DATA(id);
}

static int g_seen;
static void got_bss(struct genlmsghdr* g, int len, void* ctx)
{
    (void)ctx;
    struct nlattr* bss = find((char*)g + GENL_HDRLEN, len - GENL_HDRLEN, NL80211_ATTR_BSS);
    if (!bss) return;
    struct nlattr* mac = find(DATA(bss), DLEN(bss), NL80211_BSS_BSSID);
    struct nlattr* fr = find(DATA(bss), DLEN(bss), NL80211_BSS_FREQUENCY);
    struct nlattr* sig = find(DATA(bss), DLEN(bss), NL80211_BSS_SIGNAL_MBM);
    struct nlattr* ies = find(DATA(bss), DLEN(bss), NL80211_BSS_INFORMATION_ELEMENTS);
    char ssid[33] = "";
    if (ies) {
        const uint8_t* p = DATA(ies);
        for (int left = DLEN(ies); left >= 2 && p[1] + 2 <= left; left -= p[1] + 2, p += p[1] + 2)
            if (p[0] == 0) {
                int n = p[1] > 32 ? 32 : p[1];
                for (int i = 0; i < n; i++) ssid[i] = (p[2 + i] >= 32 && p[2 + i] < 127) ? (char)p[2 + i] : '?';
                ssid[n] = 0;
                break;
            }
    }
    const uint8_t* b = mac ? DATA(mac) : (const uint8_t*)"\0\0\0\0\0\0";
    printf("%02x:%02x:%02x:%02x:%02x:%02x %u %d %s\n", b[0], b[1], b[2], b[3], b[4], b[5],
           fr ? *(uint32_t*)DATA(fr) : 0, sig ? *(int32_t*)DATA(sig) / 100 : 0, ssid[0] ? ssid : "(hidden)");
    g_seen++;
}

int main(int argc, char** argv)
{
    const char* ifname = argc > 1 ? argv[1] : "wlan0";
    unsigned ifindex = if_nametoindex(ifname);
    if (!ifindex) { fprintf(stderr, "wifiscan: no interface %s\n", ifname); return 2; }
    g_fd = socket(AF_NETLINK, SOCK_RAW, NETLINK_GENERIC);
    struct sockaddr_nl me = { .nl_family = AF_NETLINK };
    if (g_fd < 0 || bind(g_fd, (struct sockaddr*)&me, sizeof me) < 0) { perror("wifiscan: netlink"); return 2; }

    struct msg m;
    uint16_t fam = 0;
    start(&m, GENL_ID_CTRL, CTRL_CMD_GETFAMILY, 0);
    put_attr(&m, CTRL_ATTR_FAMILY_NAME, "nl80211", 8);
    if (send_msg(&m) || recv_all(got_family, &fam) || !fam) { fprintf(stderr, "wifiscan: no nl80211 (is cfg80211 loaded?)\n"); return 2; }

    /* Active scan for every network: one empty SSID in the list. */
    start(&m, fam, NL80211_CMD_TRIGGER_SCAN, NLM_F_ACK);
    put_attr(&m, NL80211_ATTR_IFINDEX, &(uint32_t){ ifindex }, 4);
    size_t at = nest_open(&m, NL80211_ATTR_SCAN_SSIDS);
    put_attr(&m, 1, NULL, 0);
    nest_close(&m, at);
    int rc = send_msg(&m) ? -errno : recv_all(NULL, NULL);
    if (rc && rc != -EBUSY) { fprintf(stderr, "wifiscan: scan refused: %s\n", strerror(-rc)); return 2; }

    /* Results arrive when the scan finishes; ask once a second for up to 10 s. */
    for (int t = 0; t < 10 && !g_seen; t++) {
        sleep(1);
        start(&m, fam, NL80211_CMD_GET_SCAN, NLM_F_DUMP);
        put_attr(&m, NL80211_ATTR_IFINDEX, &(uint32_t){ ifindex }, 4);
        if (send_msg(&m) == 0) recv_all(got_bss, NULL);
    }
    fprintf(stderr, "wifiscan: %d network(s)\n", g_seen);
    return g_seen ? 0 : 1;
}
