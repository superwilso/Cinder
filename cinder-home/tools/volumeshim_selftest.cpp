// volumeshim_selftest — host test for cinder-audio/src/volume_shim.cpp, where Cinder's volume cap
// comes from (Sony's AVLS threshold, docs/SHORTCOMINGS.md §A2).
//
// The shim calls non-virtual exported members of pst::services::volume::VolumeService, so a fake
// defining the same members receives the calls. What matters here is the rule between Sony's
// answer and Cinder's cap: a threshold of 0, or one above the 0..120 scale, must read as "no limit
// known" (-1), never as a cap of 0 or as a number the volume can never reach; and a service that
// throws must not take the shell down. The struct offsets are Sony's marshalling
// (VolumeServiceFwClient::WriteAvlsCondition), so they are pinned too.
#include <cstddef>
#include <cstdio>

#include "../../cinder-audio/src/volume_shim.cpp"

namespace {
unsigned g_threshold = 0;
bool g_avls = false;
bool g_ret = true;
bool g_throw = false;
int g_alive = 0;
} // namespace

namespace pst { namespace services { namespace volume {
VolumeService::VolumeService() {
    if (g_throw) throw 1;
    ++g_alive;
}
VolumeService::~VolumeService() { --g_alive; }
unsigned VolumeService::GetVolume() { return 0; }
bool VolumeService::GetAvls() { return g_avls; }
unsigned VolumeService::GetAvlsThresholdValue() { return g_threshold; }
bool VolumeService::GetAvlsCondition(AvlsCondition* c) {
    c->on = 1;
    c->threshold = g_threshold;
    c->adaptive = 1;
    return g_ret;
}
} } } // namespace pst::services::volume

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    using pst::services::volume::AvlsCondition;
    std::printf("Sony's layout\n");
    eq("threshold at +4", offsetof(AvlsCondition, threshold), 4);
    eq("adaptive at +8", offsetof(AvlsCondition, adaptive), 8);
    eq("working at +9", offsetof(AvlsCondition, working), 9);

    std::printf("the threshold becomes the cap only when it is a real one\n");
    g_threshold = 63;
    eq("63 (measured on device) is the cap", cinder_volume_avls_threshold(), 63);
    g_threshold = 120;
    eq("120, the top of the scale, is kept", cinder_volume_avls_threshold(), 120);
    g_threshold = 0;
    eq("0 is no limit known, not a cap of 0", cinder_volume_avls_threshold(), -1);
    g_threshold = 121;
    eq("past 120 is no limit known", cinder_volume_avls_threshold(), -1);
    g_threshold = 63;
    g_ret = false;
    eq("the getter's false is not trusted: the filled fields decide", cinder_volume_avls_threshold(), 63);
    g_ret = true;

    std::printf("Sony's own flag, for diagnostics\n");
    g_avls = true;
    eq("on", cinder_volume_avls_enabled(), 1);
    g_avls = false;
    eq("off", cinder_volume_avls_enabled(), 0);

    std::printf("a service that throws\n");
    g_throw = true;
    eq("threshold: -1, no crash", cinder_volume_avls_threshold(), -1);
    eq("flag: -1, no crash", cinder_volume_avls_enabled(), -1);
    g_throw = false;
    eq("every client is destroyed again", g_alive, 0);

    std::printf(fails ? "volumeshim_selftest: FAILED\n" : "volumeshim_selftest: all passed\n");
    return fails;
}
