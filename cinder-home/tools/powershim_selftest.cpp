// powershim_selftest — host test for cinder-audio/src/power_shim.cpp (docs/SHORTCOMINGS.md §A2).
//
// Battery care is the shim's only job. What it must NEVER do is reach Sony's Reboot() or
// SetStatus(): on 2026-07-28 the first froze the player and the second put it to sleep, because
// Sony's shutdown waits on a barrier the Home app replacement never acknowledges (power_shim.cpp).
// The declarations stay in power_abi.hpp as a record, so the fake defines them too and fails the
// test if any entry point calls them.
#include <cstdio>

#include "../../cinder-audio/src/power_shim.cpp"

namespace {
bool g_care = false;
int g_ctors = 0;
int g_forbidden = 0;
} // namespace

namespace pst { namespace services { namespace funcarch { namespace powermgr {
PowerMgrServiceClient::PowerMgrServiceClient() { ++g_ctors; }
PowerMgrServiceClient::~PowerMgrServiceClient() {}
void PowerMgrServiceClient::EnableItawariCharging(bool const& on) { g_care = on; }
bool PowerMgrServiceClient::IsItawariChargingEnabled() { return g_care; }
void PowerMgrServiceClient::Reboot() { ++g_forbidden; }
void PowerMgrServiceClient::SetStatus(IPowerMgrService::PowerStatus const&) { ++g_forbidden; }
} } } } // namespace pst::services::funcarch::powermgr

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    std::printf("battery care\n");
    eq("reads off", cinder_power_get_battery_care(), 0);
    eq("set on returns 0", cinder_power_set_battery_care(7), 0);
    eq("any non-zero is on", cinder_power_get_battery_care(), 1);
    cinder_power_set_battery_care(0);
    eq("0 is off", cinder_power_get_battery_care(), 0);
    eq("one client for every call", g_ctors, 1);

    std::printf("the calls that froze or slept the player\n");
    eq("Reboot and SetStatus were never reached", g_forbidden, 0);

    std::printf(fails ? "powershim_selftest: FAILED\n" : "powershim_selftest: all passed\n");
    return fails;
}
