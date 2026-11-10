// wifi_conf_selftest.cpp — src/wifi_conf.h: the saved-network list cinder-wifi writes, and the
// cinder_wifi.txt import (#55). Run by tools/selftests.sh.
#include <cstdio>
#include <cstring>
#include "../src/wifi_conf.h"

static int fails;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

int main()
{
    std::puts("== wifi_conf");
    char b[1024], out[2048];
    const char* why;

    CHECK(wifi_block(b, sizeof b, "686f6d65", "", "hunter22", "", &why) > 0 && std::strstr(b, "psk=\"hunter22\"")
          && std::strstr(b, "key_mgmt=WPA-PSK SAE"), "a personal network");
    CHECK(wifi_block(b, sizeof b, "686f6d65", "", "", "", &why) > 0 && std::strstr(b, "key_mgmt=NONE"), "an open one");
    CHECK(wifi_block(b, sizeof b, "686f6d65", "", "short", "", &why) < 0, "a 5-character WPA password is refused");
    CHECK(wifi_block(b, sizeof b, "6564", "ab123@uni.ac.uk", "s3cret", "", &why) > 0
          && std::strstr(b, "key_mgmt=WPA-EAP") && std::strstr(b, "identity=\"ab123@uni.ac.uk\"")
          && std::strstr(b, "phase2=\"auth=MSCHAPV2\"") && !std::strstr(b, "ca_cert"),
          "an enterprise network, server not checked");
    CHECK(wifi_block(b, sizeof b, "6564", "ab123", "s3cret", "uni.ac.uk", &why) > 0
          && std::strstr(b, "domain_suffix_match=\"uni.ac.uk\"") && std::strstr(b, "ca_cert="),
          "...and with the server checked against its domain");
    CHECK(wifi_block(b, sizeof b, "6564", "ab123", "", "", &why) < 0, "an enterprise network needs a password");
    CHECK(wifi_block(b, sizeof b, "6564", "ab\n123", "s3cret", "", &why) < 0, "a newline cannot reach the file");

    const char* conf =
        "ctrl_interface=/tmp/cinder-wifi/ctrl\nupdate_config=0\n"
        "network={\n ssid=616161\n key_mgmt=NONE\n}\n"
        "network={\n ssid=626262\n key_mgmt=NONE\n}\n";
    wifi_conf_drop(conf, "616161", out, sizeof out);
    CHECK(!std::strstr(out, "616161") && std::strstr(out, "626262") && std::strstr(out, "ctrl_interface"),
          "drop removes that network only");
    wifi_conf_drop(conf, "6161", out, sizeof out);
    CHECK(std::strstr(out, "616161") != nullptr, "a name that is only a prefix is not a match");
    wifi_conf_drop("update_config=0\nnetwork={\n ssid=\"Home\"\n psk=\"hunter22\"\n}\n", "486f6d65", out, sizeof out);
    CHECK(!std::strstr(out, "Home") && std::strstr(out, "update_config"), "a network saved by name in quotes");

    char l1[] = "Home | hunter22\r", l2[] = "  # a comment", l3[] = "eduroam | ab123 | s3cret | uni.ac.uk",
         l4[] = "Cafe", l5[] = "";
    char *s, *u, *p, *d;
    CHECK(wifi_import_line(l1, &s, &u, &p, &d) && !std::strcmp(s, "Home") && !*u && !std::strcmp(p, "hunter22"),
          "Name | password");
    CHECK(!wifi_import_line(l2, &s, &u, &p, &d) && !wifi_import_line(l5, &s, &u, &p, &d), "comments and blanks");
    CHECK(wifi_import_line(l3, &s, &u, &p, &d) && !std::strcmp(u, "ab123") && !std::strcmp(p, "s3cret")
          && !std::strcmp(d, "uni.ac.uk"), "Name | username | password | domain");
    CHECK(wifi_import_line(l4, &s, &u, &p, &d) && !std::strcmp(s, "Cafe") && !*p, "a bare name is open");
    std::printf("%d failed\n", fails);
    return fails != 0;
}
