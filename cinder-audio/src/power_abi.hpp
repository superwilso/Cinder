// power_abi.hpp — hand-written declaration of Sony's power-manager client
// (libPowerMgrServiceClient.so → pst::services::funcarch::powermgr::PowerMgrServiceClient),
// reconstructed by offline RE. We use exactly two methods: the battery-care ("Itawari" charging,
// caps charge at ~90% to preserve battery longevity) enable + query — Cinder's Settings "Battery
// care" toggle. The stock app exposes the same as an On/Off toggle (isBatteryCareOn /
// OnBatteryCareOnOffToggled), so it's a plain boolean, not a settable percentage.
//
// USAGE: build power_shim.cpp like effect_shim.cpp (clang -stdlib=libc++, linked against the
// device libPowerMgrServiceClient / libc++), expose a flat C ABI, call from behind run_guarded.
//
// SIZING (the heap-overflow brick care): PowerMgrServiceClient is constructed by us, so its
// `sizeof` must be ≥ the real device object. RE of both the ctor (C1 @0x2818) and the factory
// `GetPowerMgrServiceClient` (@0x3338, `operator new(8)`) shows the object is **8 bytes**: the ctor
// writes only this+0 (a vtable/typeinfo ptr) and this+4 (the framework service-client pointer from
// Framework::GetServiceClient). We reserve 0x10 (2× margin) with a static_assert, like EffectCtrlDmp.
#pragma once
#include <cstddef>

namespace pst { namespace services { namespace funcarch { namespace powermgr {

// RE-CONFIRMED size: ctor write-extent = this+4 (4 bytes) → real object 8 bytes; factory new(8).
constexpr std::size_t kPowerMgrServiceClientRealSize = 8;

class PowerMgrServiceClient {
public:
    PowerMgrServiceClient();    // C1 @libPowerMgrServiceClient.so:0x2818 (connects to the service)
    ~PowerMgrServiceClient();   // D1 @0x2890

    // Battery care = Sony "Itawari" considerate charging (caps at ~90%). Boolean on/off.
    void EnableItawariCharging(bool const& on);   // @0x2e34
    bool IsItawariChargingEnabled();              // @0x2ea0

private:
    // Reserve the device object's footprint (real = 8 bytes; see kPowerMgrServiceClientRealSize).
    alignas(8) unsigned char _device_storage[0x10];
};

static_assert(sizeof(PowerMgrServiceClient) >= kPowerMgrServiceClientRealSize,
              "PowerMgrServiceClient reserved storage smaller than the device object");

} } } } // namespace
