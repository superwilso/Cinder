/* cinder-wifi — setuid-root helper: Wi-Fi on, off, status, scan, join, forget.
 *
 * WHY IT EXISTS. The player's Wi-Fi needs four kernel modules loaded, a WMT power write, and
 * wpa_supplicant/udhcpc running as root (wifi/README.md). cinder-home runs as uid 100 (system), so
 * like cinder-clock and cinder-power this is the one narrow door: static, setuid root, fixed paths,
 * a verb and nothing else from the caller. The Wi-Fi password arrives on stdin, never in argv,
 * where any process could read it from /proc.
 *
 *   cinder-wifi on        load (once per boot), power, join the saved network, DHCP     -> 0 / 1
 *   cinder-wifi off       leave the network and power the radio down                   -> 0
 *   cinder-wifi status    key=value lines: state, ssid (hex), ip, signal, mac          -> 0
 *   cinder-wifi scan      one line a network: <signal dBm>\t<security>\t<ssid hex>       -> 0 / 1
 *   cinder-wifi join      stdin: SSID line, then password line (empty = open network)  -> 0 / 2
 *                         writes the saved network; does not connect (run `on` after)
 *   cinder-wifi forget    delete the saved network                                     -> 0
 *
 * Exit codes: 0 done, 1 the operation failed (last line of output says why), 2 bad input,
 * 3 not running as root (the setuid bit is missing), 4 Wi-Fi is not installed.
 *
 * SSIDs leave and enter as HEX: an SSID is up to 32 arbitrary bytes, and hex is the one form that
 * survives a shell, a log line and wpa_supplicant's config parser unchanged.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#ifndef DIR_
#define DIR_ "/system/vendor/cinder/wifi"   /* a test build may say otherwise; the release never does */
#endif
#define DATA "/data/cinder/wifi"
#define CONF DATA "/wpa.conf"
#define RUN "/tmp/cinder-wifi"

static char* const ENV[] = { "PATH=/sbin:/system/sbin:/system/bin:/system/xbin:/bin", "HOME=/", NULL };

/* Run argv with a clean environment; stdout/stderr pass through. Returns the exit status. */
static int run(char* const argv[])
{
    pid_t p = fork();
    if (p < 0) return -1;
    if (p == 0) {
        execve(argv[0], argv, ENV);
        _exit(127);
    }
    int st;
    while (waitpid(p, &st, 0) < 0)
        if (errno != EINTR) return -1;
    return WIFEXITED(st) ? WEXITSTATUS(st) : -1;
}

/* Run argv and capture its stdout (at most cap-1 bytes) into buf. */
static int capture(char* const argv[], char* buf, size_t cap)
{
    int fd[2];
    if (pipe(fd) < 0) return -1;
    pid_t p = fork();
    if (p < 0) return -1;
    if (p == 0) {
        dup2(fd[1], 1);
        close(fd[0]);
        close(fd[1]);
        execve(argv[0], argv, ENV);
        _exit(127);
    }
    close(fd[1]);
    size_t n = 0;
    ssize_t r;
    while (n + 1 < cap && (r = read(fd[0], buf + n, cap - 1 - n)) > 0) n += (size_t)r;
    buf[n] = 0;
    close(fd[0]);
    int st;
    while (waitpid(p, &st, 0) < 0)
        if (errno != EINTR) return -1;
    return WIFEXITED(st) ? WEXITSTATUS(st) : -1;
}

/* The value of `key=` on its own line in s, copied into out. */
static int field(const char* s, const char* key, char* out, size_t cap)
{
    size_t k = strlen(key);
    for (const char* l = s; l && *l; l = strchr(l, '\n') ? strchr(l, '\n') + 1 : NULL) {
        if (strncmp(l, key, k) == 0 && l[k] == '=') {
            const char* v = l + k + 1;
            size_t n = strcspn(v, "\n");
            if (n >= cap) n = cap - 1;
            memcpy(out, v, n);
            out[n] = 0;
            return 1;
        }
    }
    out[0] = 0;
    return 0;
}

static void hex(const unsigned char* b, size_t n, char* out)
{
    static const char d[] = "0123456789abcdef";
    for (size_t i = 0; i < n; i++) { out[2 * i] = d[b[i] >> 4]; out[2 * i + 1] = d[b[i] & 15]; }
    out[2 * n] = 0;
}

/* wpa_cli prints SSIDs with \xNN escapes for anything unprintable; undo that into bytes. */
static size_t unescape(const char* s, unsigned char* out, size_t cap)
{
    size_t n = 0;
    while (*s && n < cap) {
        if (s[0] == '\\' && s[1] == 'x' && isxdigit((unsigned char)s[2]) && isxdigit((unsigned char)s[3])) {
            char h[3] = { s[2], s[3], 0 };
            out[n++] = (unsigned char)strtoul(h, NULL, 16);
            s += 4;
        } else if (s[0] == '\\' && (s[1] == '\\' || s[1] == '"')) {
            out[n++] = (unsigned char)s[1];
            s += 2;
        } else {
            out[n++] = (unsigned char)*s++;
        }
    }
    return n;
}

static int wpa_cli(const char* cmd, char* buf, size_t cap)
{
    char* const a[] = { DIR_ "/wpa_cli", "-p", RUN "/ctrl", "-i", "wlan0", (char*)cmd, NULL };
    return capture(a, buf, cap);
}

static int do_status(void)
{
    static char st[4096], link[2048];
    int up = access("/sys/class/net/wlan0", F_OK) == 0;
    if (!up) { puts("state=off"); return 0; }
    wpa_cli("status", st, sizeof st);
    char v[256], ssid[80];
    field(st, "wpa_state", v, sizeof v);
    const char* state = strcmp(v, "COMPLETED") == 0 ? "connected" : v[0] ? "connecting" : "on";
    printf("state=%s\n", state);
    if (field(st, "ssid", v, sizeof v)) {
        unsigned char raw[32];
        size_t n = unescape(v, raw, sizeof raw);
        hex(raw, n, ssid);
        printf("ssid=%s\n", ssid);
    }
    if (field(st, "ip_address", v, sizeof v)) printf("ip=%s\n", v);
    if (field(st, "freq", v, sizeof v)) printf("freq=%s\n", v);
    char* const iw[] = { DIR_ "/iw", "dev", "wlan0", "link", NULL };
    if (capture(iw, link, sizeof link) == 0) {
        const char* s = strstr(link, "signal: ");
        if (s) printf("signal=%d\n", atoi(s + 8));
    }
    FILE* m = fopen("/sys/class/net/wlan0/address", "r");
    if (m && fgets(v, sizeof v, m)) printf("mac=%s", v);
    if (m) fclose(m);
    return 0;
}

static int do_scan(void)
{
    static char out[16384];
    if (access("/sys/class/net/wlan0", F_OK) != 0) { puts("wifi is off"); return 1; }
    wpa_cli("scan", out, sizeof out);
    sleep(4);
    if (wpa_cli("scan_results", out, sizeof out) != 0) { puts("no scan results"); return 1; }
    /* bssid \t freq \t signal \t flags \t ssid — skip the header line. */
    char* save = NULL;
    for (char* l = strtok_r(out, "\n", &save); l; l = strtok_r(NULL, "\n", &save)) {
        char* f[5] = { 0 };
        int k = 0;
        for (char* p = l; k < 5; k++) {
            f[k] = p;
            char* t = k < 4 ? strchr(p, '\t') : NULL;
            if (!t) break;
            *t = 0;
            p = t + 1;
        }
        if (k < 4 || !f[4] || !strchr(f[0], ':')) continue;
        unsigned char raw[32];
        char h[65];
        size_t n = unescape(f[4], raw, sizeof raw);
        if (n == 0 || raw[0] == 0) continue;             /* hidden network */
        hex(raw, n, h);
        const char* sec = strstr(f[3], "SAE") && !strstr(f[3], "PSK") ? "wpa3"
                        : strstr(f[3], "WPA") ? "wpa2" : strstr(f[3], "WEP") ? "wep" : "open";
        printf("%d\t%s\t%s\n", atoi(f[2]), sec, h);
    }
    return 0;
}

/* Read one line (without the newline) from stdin. */
static int line(char* buf, size_t cap)
{
    if (!fgets(buf, (int)cap, stdin)) { buf[0] = 0; return 0; }
    buf[strcspn(buf, "\r\n")] = 0;
    return 1;
}

static int do_join(void)
{
    char ssid[256], pass[256];
    if (!line(ssid, sizeof ssid)) return 2;
    line(pass, sizeof pass);
    size_t sn = strlen(ssid), pn = strlen(pass);
    if (sn == 0 || sn > 32) { puts("the network name must be 1 to 32 bytes"); return 2; }
    if (pn && (pn < 8 || pn > 63)) { puts("a WPA password is 8 to 63 characters"); return 2; }
    for (size_t i = 0; i < pn; i++)
        if (pass[i] < 32 || pass[i] > 126) { puts("the password has a character WPA does not allow"); return 2; }
    char h[65];
    hex((const unsigned char*)ssid, sn, h);
    mkdir(DATA, 0700);
    const char* tmp = CONF ".tmp";
    int fd = open(tmp, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) { puts("cannot write the saved network"); return 1; }
    FILE* f = fdopen(fd, "w");
    fprintf(f, "ctrl_interface=" RUN "/ctrl\nupdate_config=0\nnetwork={\n ssid=%s\n", h);
    if (pn) fprintf(f, " key_mgmt=WPA-PSK SAE\n ieee80211w=1\n psk=\"%s\"\n", pass);
    else fprintf(f, " key_mgmt=NONE\n");
    fprintf(f, "}\n");
    if (fflush(f) != 0 || fsync(fd) != 0) { fclose(f); unlink(tmp); puts("cannot write the saved network"); return 1; }
    fclose(f);
    memset(pass, 0, sizeof pass);
    if (rename(tmp, CONF) != 0) { unlink(tmp); puts("cannot save the network"); return 1; }
    /* A running supplicant reads the new network now; a stopped one at the next `on`. */
    char out[256];
    wpa_cli("reconfigure", out, sizeof out);
    puts("saved");
    return 0;
}

int main(int argc, char** argv)
{
    if (argc != 2) { fputs("usage: cinder-wifi on|off|status|scan|join|forget\n", stderr); return 2; }
    if (setuid(0) != 0 || setgid(0) != 0) { puts("not root: the setuid bit is missing"); return 3; }
    const char* v = argv[1];
    if (access(DIR_ "/wlan_mt.ko", R_OK) != 0) { puts("Wi-Fi is not installed"); return 4; }
    if (strcmp(v, "on") == 0) {
        if (access(CONF, R_OK) != 0) { puts("no saved network"); return 1; }
        char* const a[] = { "/system/bin/sh", DIR_ "/wifi-up.sh", DIR_, CONF, RUN, NULL };
        return run(a) == 0 ? 0 : 1;
    }
    if (strcmp(v, "off") == 0) {
        char* const a[] = { "/system/bin/sh", DIR_ "/wifi-down.sh", DIR_, RUN, NULL };
        run(a);
        return 0;
    }
    if (strcmp(v, "status") == 0) return do_status();
    if (strcmp(v, "scan") == 0) return do_scan();
    if (strcmp(v, "join") == 0) return do_join();
    if (strcmp(v, "forget") == 0) { unlink(CONF); puts("forgotten"); return 0; }
    fputs("usage: cinder-wifi on|off|status|scan|join|forget\n", stderr);
    return 2;
}
