// volume_shim.cpp — Sony's `pst::services::volume::VolumeService`, reduced to the one thing on it
// Cinder can actually use: the AVLS volume limit.
//
// WHAT THIS SERVICE IS, AND WHY ONLY THIS PART IS HERE. `VolumeService` is the only volume API
// Sony's own player uses — `HgrmMediaPlayerApp` imports no Bluetooth service at all. Behind the
// binder it dispatches by `funcarch::OutputDevice` to three output classes, and only ONE of them
// implements anything: `VolumeAdlerOut` (the CXD3778GF, i.e. the 3.5 mm jack). `VolumeA2dpOut`'s
// `SetVolume` is a stub that touches nothing and its `GetVolume` always returns 0; `VolumeUacOut`
// inherits the same no-ops. Full disassembly in analysis/RE_volume_service.md.
//
// So Cinder does NOT route its volume through this service. Cinder owns the mixer directly
// (`apply_volume` -> amixer `master volume` 0..120), which is the right call on a device where two
// of the three routes are stubs — and the service caches its own level, so the two disagree
// whenever anything moves one behind the other's back.
//
// What IS worth having is the threshold. AVLS is Sony's volume limiter, it is featured on this
// model, and MEASURED on device 2026-09-07 it genuinely enforces:
//
//     SetAvls(true) -> GetAvls()=1
//     asked for 91  -> GetVolume()=63   (cap 63)
//
// But it enforces INSIDE `VolumeAdlerOut::SetVolume`, which Cinder never calls — so switching
// Sony's flag on would clamp nothing of ours. Cinder therefore reads the number and does its own
// clamping. That is the whole reason this file exposes a getter and no setter: turning Sony's flag
// on would be a control that accepts a write and changes nothing, which is exactly the mistake
// "High gain output" made before it was cut (cinder-ui/src/sound.rs).
//
// The threshold is per output device and `adapt=1` on this unit, so it is read live rather than
// cached at boot: it is Sony's safe-listening number for whatever is plugged in now.

#include <cstring>

namespace pst {
namespace services {
namespace volume {

// Layout from `VolumeServiceFwClient::WriteAvlsCondition` (@0x28d90), which marshals exactly
// [+0]=u8, [+4]=u32, [+8]=u8, [+9]=u8 — and Sony's own log line names all four:
// `FireAvlsConditionChanged([on:%u, thrs:%u, adapt:%u, work:%u])`.
struct AvlsCondition {
    unsigned char on;
    unsigned char _pad0[3];
    unsigned int  threshold;
    unsigned char adaptive;
    unsigned char working;
    unsigned char _reserve[26];   // slack: the service writes, we own the buffer
};

// Non-virtual exported members, called by mangled symbol (the playerservice_abi.hpp pattern).
//
// THE RESERVE IS LOAD-BEARING. The real object is 8 bytes — the constructor is `strd r1,r1,[r0]`
// and nothing else — and a class with no data members would be `sizeof` 1, so the constructor
// would write 8 bytes into a 1-byte object. That is the 2026-06-25 sizing bug; over-reserving
// avoids it without having to guess the size exactly.
class VolumeService {
public:
    VolumeService();
    ~VolumeService();
    unsigned GetVolume();
    bool     GetAvls();
    unsigned GetAvlsThresholdValue();
    bool     GetAvlsCondition(AvlsCondition*);
private:
    void* _reserve[8];
};

} // namespace volume
} // namespace services
} // namespace pst

extern "C" {

// Sony's AVLS limit for the CURRENT output device, in the same 0..120 units as the UI level, or
// -1 if the service cannot answer (not featured, or no usable threshold).
//
// Read live on purpose: the condition reports `adapt=1`, so the number belongs to whatever is
// plugged in right now rather than to the boot.
int cinder_volume_avls_threshold(void) {
    try {
        pst::services::volume::VolumeService vs;
        pst::services::volume::AvlsCondition c;
        std::memset(&c, 0, sizeof c);
        if (!vs.GetAvlsCondition(&c)) {
            // The getter's return is not a reliable success flag on this service (it came back 0 on
            // a call that plainly filled the struct), so the FIELDS decide: an unfeatured AVLS
            // cannot report a threshold, and a zero threshold is not a limit anyone wants applied.
        }
        if (c.threshold == 0 || c.threshold > 120) return -1;
        return (int)c.threshold;
    } catch (...) {
        return -1;
    }
}

// Is Sony's own AVLS flag set? Reported for diagnostics only — Cinder does its own clamping, and
// this flag says nothing about whether Cinder's limit is on.
int cinder_volume_avls_enabled(void) {
    try {
        pst::services::volume::VolumeService vs;
        return vs.GetAvls() ? 1 : 0;
    } catch (...) {
        return -1;
    }
}

} // extern "C"
