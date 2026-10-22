// codecshim_selftest — host test for cinder-audio/src/codec_shim.cpp, the shell's ioctls on the
// CXD3778GF's ALSA controls (docs/SHORTCOMINGS.md §A2).
//
// The shim's open/ioctl/close are redirected to an in-memory control device holding the controls
// it names, so each check can say which control was written with what. The rules pinned here are
// the ones a wrong edit would make silently: the amp switch never writes item 2 (the balanced
// output this model does not have), the gain switch is the SE control and not the two BTL-wired
// ones beside it, every control is addressed by NAME with numid 0, and no fd is left open.
#include <fcntl.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <sound/asound.h>
#include <string>
#include <vector>

namespace {
struct Ctl {
    const char* name;
    bool enumerated;
    std::vector<std::string> items;
    long value;
    int writes;
};
std::vector<Ctl> g_ctls = {
    { "standby", false, {}, 0, 0 },
    { "headphone smaster se gain mode", true, { "normal", "high" }, 0, 0 },
    { "headphone smaster gain mode", true, { "normal", "high" }, 0, 0 },
    { "headphone smaster btl gain mode", true, { "normal", "high" }, 0, 0 },
    { "headphone amp", true, { "normal", "smaster-se", "smaster-btl" }, 1, 0 },
    { "master volume", false, {}, 40, 0 },
    { "l balance volume", false, {}, 0, 0 },
    { "r balance volume", false, {}, 0, 0 },
    { "noise cancel headphone", true, { "off", "nw750n", "nw500n", "nc31" }, 0, 0 },
};
int g_open = 0, g_by_numid = 0;
bool g_no_device = false;

Ctl* find(const snd_ctl_elem_id& id) {
    if (id.numid != 0 || id.iface != SNDRV_CTL_ELEM_IFACE_MIXER) ++g_by_numid;
    for (auto& c : g_ctls)
        if (!std::strcmp(c.name, reinterpret_cast<const char*>(id.name))) return &c;
    return nullptr;
}

int fake_open(const char* path, int, ...) {
    if (g_no_device || std::strcmp(path, "/dev/snd/controlC0") != 0) return -1;
    ++g_open;
    return 3;
}
int fake_close(int) { --g_open; return 0; }
int fake_ioctl(int, unsigned long req, void* arg) {
    if (req == SNDRV_CTL_IOCTL_ELEM_READ || req == SNDRV_CTL_IOCTL_ELEM_WRITE) {
        auto* v = static_cast<snd_ctl_elem_value*>(arg);
        Ctl* c = find(v->id);
        if (!c) return -1;
        if (req == SNDRV_CTL_IOCTL_ELEM_READ) {
            if (c->enumerated) v->value.enumerated.item[0] = (unsigned)c->value;
            else v->value.integer.value[0] = c->value;
            return 0;
        }
        c->value = c->enumerated ? (long)v->value.enumerated.item[0] : v->value.integer.value[0];
        ++c->writes;
        return 0;
    }
    if (req == SNDRV_CTL_IOCTL_ELEM_INFO) {
        auto* in = static_cast<snd_ctl_elem_info*>(arg);
        Ctl* c = find(in->id);
        if (!c) return -1;
        in->type = c->enumerated ? SNDRV_CTL_ELEM_TYPE_ENUMERATED : SNDRV_CTL_ELEM_TYPE_INTEGER;
        if (c->enumerated) {
            in->value.enumerated.items = (unsigned)c->items.size();
            unsigned i = in->value.enumerated.item;
            if (i >= c->items.size()) return -1;
            std::snprintf(in->value.enumerated.name, sizeof in->value.enumerated.name, "%s", c->items[i].c_str());
        }
        return 0;
    }
    return -1;
}
Ctl& ctl(const char* name) {
    for (auto& c : g_ctls) if (!std::strcmp(c.name, name)) return c;
    return g_ctls[0];
}
} // namespace

#define open fake_open
#define close fake_close
#define ioctl fake_ioctl
#include "../../cinder-audio/src/codec_shim.cpp"
#undef open
#undef close
#undef ioctl

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    std::printf("the jack amplifier\n");
    for (int asked : { 1, 0, 2, 99, -1 }) {
        cinder_codec_set_hp_amp_linear(asked);
        const long v = ctl("headphone amp").value;
        eq("never smaster-btl, whatever is asked", v == 0 || v == 1, 1);
    }
    cinder_codec_set_hp_amp_linear(1);
    eq("linear is item 0", ctl("headphone amp").value, 0);
    eq("…and reads back as linear", cinder_codec_get_hp_amp_linear(), 1);
    cinder_codec_set_hp_amp_linear(0);
    eq("S-Master is item 1", ctl("headphone amp").value, 1);
    eq("…and reads back as not linear", cinder_codec_get_hp_amp_linear(), 0);

    std::printf("the SE gain switch\n");
    cinder_codec_set_gain_mode(5);
    eq("any non-zero is high on the SE control", ctl("headphone smaster se gain mode").value, 1);
    eq("the mis-wired 'headphone smaster gain mode' is never written",
       ctl("headphone smaster gain mode").writes, 0);
    eq("nor the BTL one", ctl("headphone smaster btl gain mode").writes, 0);
    eq("reads back", cinder_codec_get_gain_mode(), 1);

    std::printf("plain values\n");
    cinder_codec_set_standby(7);
    eq("standby is 0 or 1", ctl("standby").value, 1);
    cinder_codec_set_master_volume(88);
    eq("master volume", cinder_codec_get_master_volume(), 88);
    eq("balance writes both sides", cinder_codec_set_balance(4, 9), 0);
    eq("left", ctl("l balance volume").value, 4);
    eq("right", ctl("r balance volume").value, 9);

    std::printf("an enum item by its name, as Sony's NC service writes it\n");
    eq("found", cinder_codec_set_enum_item("noise cancel headphone", "nw500n"), 0);
    eq("written as the driver's own index", ctl("noise cancel headphone").value, 2);
    const int before = ctl("noise cancel headphone").writes;
    eq("an item the driver lacks is refused", cinder_codec_set_enum_item("noise cancel headphone", "wh1000"), -1);
    eq("…and nothing is written", ctl("noise cancel headphone").writes, before);
    char out[64];
    cinder_codec_describe("noise cancel headphone", out, sizeof out);
    eq("describe names the item", std::string(out) == "nw500n", 1);
    cinder_codec_describe("master volume", out, sizeof out);
    eq("describe prints an integer", std::string(out) == "88", 1);

    std::printf("no device\n");
    g_no_device = true;
    eq("a read is -1", cinder_codec_get_master_volume(), -1);
    eq("a write is -1", cinder_codec_set_standby(0), -1);
    g_no_device = false;

    std::printf("hygiene\n");
    eq("every control addressed by name, numid 0", g_by_numid, 0);
    eq("every fd closed", g_open, 0);

    std::printf(fails ? "codecshim_selftest: FAILED\n" : "codecshim_selftest: all passed\n");
    return fails;
}
