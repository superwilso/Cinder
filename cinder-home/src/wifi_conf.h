/* wifi_conf.h — the saved-network list (wpa_supplicant's wpa.conf) as text: build one network,
 * drop one by name, and read /contents/cinder_wifi.txt (#55, plaquette: "import Wi-Fi passwords
 * from a file", "networks that need a username and password"). Pure string work, so
 * tools/wifi_conf_selftest.cpp checks the same code cinder-wifi runs.
 *
 * cinder_wifi.txt, one network a line ('#' starts a comment):
 *   Name | password                                  WPA2/WPA3 personal (no password: open)
 *   Name | username | password                       WPA2 enterprise (PEAP or TTLS, MSCHAPv2)
 *   Name | username | password | example.ac.uk       ...and the server's certificate is checked
 *                                                    against that domain
 * Without a domain the server is not checked, as a phone does when told "do not validate". */
#ifndef CINDER_WIFI_CONF_H
#define CINDER_WIFI_CONF_H
#include <stdio.h>
#include <string.h>

#define WIFI_CA "/system/vendor/cinder/wifi/cacert.pem"

static int wifi_printable(const char* s)
{
    for (; *s; s++)
        if ((unsigned char)*s < 32 || (unsigned char)*s > 126) return 0;
    return 1;
}

/* One network block. user empty: personal (pass empty: open). Returns the length, or -1 when a
 * field is not usable (the reason in *why). */
static int wifi_block(char* out, size_t cap, const char* ssid_hex, const char* user, const char* pass,
                      const char* domain, const char** why)
{
    size_t pn = strlen(pass);
    *why = "";
    if (!wifi_printable(user) || !wifi_printable(pass) || !wifi_printable(domain)) {
        *why = "a character WPA does not allow";
        return -1;
    }
    int n;
    if (*user) {
        if (!pn) { *why = "an enterprise network needs a password"; return -1; }
        n = snprintf(out, cap,
                     "network={\n ssid=%s\n key_mgmt=WPA-EAP\n eap=PEAP TTLS\n identity=\"%s\"\n"
                     " password=\"%s\"\n phase2=\"auth=MSCHAPV2\"\n",
                     ssid_hex, user, pass);
        if (n > 0 && *domain && (size_t)n < cap)
            n += snprintf(out + n, cap - (size_t)n, " ca_cert=\"" WIFI_CA "\"\n domain_suffix_match=\"%s\"\n",
                          domain);
    } else if (pn) {
        if (pn < 8 || pn > 63) { *why = "a WPA password is 8 to 63 characters"; return -1; }
        n = snprintf(out, cap, "network={\n ssid=%s\n key_mgmt=WPA-PSK SAE\n ieee80211w=1\n psk=\"%s\"\n",
                     ssid_hex, pass);
    } else {
        n = snprintf(out, cap, "network={\n ssid=%s\n key_mgmt=NONE\n", ssid_hex);
    }
    if (n < 0 || (size_t)n + 3 > cap) { *why = "too long"; return -1; }
    return n + snprintf(out + n, cap - (size_t)n, "}\n");
}

/* conf without the network named ssid_hex, into out (cap bytes). Returns the length. A network
 * saved by hand or by an older Cinder names itself in quotes (ssid="Home"), so that form counts too. */
static size_t wifi_conf_drop(const char* conf, const char* ssid_hex, char* out, size_t cap)
{
    char want[80], quoted[48];
    size_t q = 0;
    snprintf(want, sizeof want, " ssid=%s\n", ssid_hex);
    q += (size_t)snprintf(quoted, sizeof quoted, " ssid=\"");
    for (const char* h = ssid_hex; h[0] && h[1] && q + 4 < sizeof quoted; h += 2) {
        unsigned v;
        if (sscanf(h, "%2x", &v) != 1) break;
        quoted[q++] = (char)v;
    }
    snprintf(quoted + q, sizeof quoted - q, "\"\n");
    size_t n = 0;
    while (*conf) {
        const char* end;
        if (strncmp(conf, "network={", 9) == 0) {
            const char* close = strstr(conf, "\n}");
            end = close ? close + 2 : conf + strlen(conf);
            if (*end == '\n') end++;
            const char* hit = strstr(conf, want);
            const char* hit2 = strstr(conf, quoted);
            if ((hit && hit < end) || (hit2 && hit2 < end)) { conf = end; continue; }
        } else {
            end = strchr(conf, '\n');
            end = end ? end + 1 : conf + strlen(conf);
        }
        size_t k = (size_t)(end - conf);
        if (n + k >= cap) break;
        memcpy(out + n, conf, k);
        n += k;
        conf = end;
    }
    out[n] = 0;
    return n;
}

static char* wifi_trim(char* s)
{
    while (*s == ' ' || *s == '\t') s++;
    char* e = s + strlen(s);
    while (e > s && (e[-1] == ' ' || e[-1] == '\t' || e[-1] == '\r')) *--e = 0;
    return s;
}

/* Split one line of cinder_wifi.txt in place. Returns 1 for a network, 0 for a blank or comment. */
static int wifi_import_line(char* line, char** ssid, char** user, char** pass, char** domain)
{
    static char empty[1] = "";
    char* f[4] = { empty, empty, empty, empty };
    line = wifi_trim(line);
    if (!*line || *line == '#') return 0;
    int k = 0;
    f[k++] = line;
    for (char* p = line; *p && k < 4; p++)
        if (*p == '|') { *p = 0; f[k++] = p + 1; }
    for (int i = 0; i < 4; i++) f[i] = wifi_trim(f[i]);
    *ssid = f[0];
    if (k <= 2) { *user = empty; *pass = f[1]; *domain = empty; }
    else { *user = f[1]; *pass = f[2]; *domain = f[3]; }
    return **ssid != 0;
}
#endif
