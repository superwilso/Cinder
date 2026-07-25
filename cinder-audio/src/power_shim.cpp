// power_shim.cpp — implements cinder_power.h over Sony's PowerMgrServiceClient
// (libPowerMgrServiceClient.so). Built clang -stdlib=libc++ and linked against the device lib,
// same as effect_shim.cpp.
//
// The client is constructed LAZILY on first use and cached. Construction connects to the power
// service; if that isn't reachable it can crash — the SHELL calls every entry point from behind
// its crash+hang guard (run_guarded), so a failure just means battery care can't be read/changed,
// the UI keeps running, and it can never brick. Object size is RE-confirmed 8 bytes
// (power_abi.hpp reserves 0x10), so `new PowerMgrServiceClient` can't overflow.
#include "power_abi.hpp"
#include "cinder_power.h"

namespace pm = pst::services::funcarch::powermgr;

namespace {
pm::PowerMgrServiceClient* g_pm = nullptr;

// Lazily construct the client. Returns it, or nullptr if unavailable. (If the device ctor faults
// because the power service is down, the caller's guard unwinds out of here before the assignment,
// leaving g_pm null — we retry next time; on a healthy device it succeeds once.)
pm::PowerMgrServiceClient* pmc() {
    if (!g_pm) {
        g_pm = new pm::PowerMgrServiceClient();
    }
    return g_pm;
}
} // namespace

extern "C" {

int cinder_power_get_battery_care(void) {
    pm::PowerMgrServiceClient* p = pmc();
    if (!p) return -1;
    return p->IsItawariChargingEnabled() ? 1 : 0;
}

int cinder_power_set_battery_care(int on) {
    pm::PowerMgrServiceClient* p = pmc();
    if (!p) return -1;
    bool b = on != 0;
    p->EnableItawariCharging(b);   // takes bool const& — pass an lvalue
    return 0;
}

} // extern "C"
