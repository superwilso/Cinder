// effect_abi.hpp — hand-written declarations of Sony's sound-effect control API
// (libEffectCtrlDmp.so → pst::services::sound::EffectCtrlDmp), reconstructed by offline RE
// (analysis/RE_playerservice_sound.md, 2026-06-26). These cover EVERY effect on the Cinder Sound
// + EQ screens, including applying effects to Bluetooth output (goal #7).
//
// USAGE: build cinder-audio/src/effect_shim.cpp the same way as player_shim.cpp (clang
// -stdlib=libc++, linked against the device libEffectCtrlDmp/libc++), expose a flat C ABI, and
// wire the EQ screen's Action::EqChanged -> SetEq10BandValue and the Sound toggles -> the
// matching setters. All methods below are NON-virtual exported members (called directly by
// mangled symbol — no vtable to reproduce).
//
// ⚠️ TWO REQUIRED CARES (both bit us before):
//  1. SIZING: EffectCtrlDmp has a default ctor; before `new EffectCtrlDmp()` you MUST reserve its
//     real object size (RE the ctor's highest write offset) like easel_abi.hpp's kCuiAppModuleRealSize,
//     or the device ctor overflows the heap (the 2026-06-25 sizing bug). Placeholder below.
//  2. GUARD: every call here is a Sony-service call — invoke behind cinder-home's run_guarded
//     crash+hang guard, and validate with cinder-probe isolation before any boot-path use.
//
// ENUM VALUES marked TBD need confirming on device (short disasm of the setter, or a probe). The
// band index is 0..9 for the 10-band EQ; gain is an int (dB; range to confirm — Sony UIs are
// usually ±10 dB). Cinder's EQ currently uses ±6; map accordingly.
#pragma once
#include <cstddef>

namespace pst { namespace services { namespace sound {

// --- effect enums (names confirmed from symbols; integer values TBD on device) ---
enum class Eq10Band : int { B32=0, B64=1, B125=2, B250=3, B500=4, B1k=5, B2k=6, B4k=7, B8k=8, B16k=9 };
enum class Eq6Band : int { /* TBD 0..5 */ };
enum class Eq6BandPreset : int { /* TBD (FLAT/ROCK/JAZZ/...) */ };
enum class VptMode : int { /* TBD (Studio/Club/ConcertHall/...) */ };
enum class DseeHxCustomMode : int { /* TBD */ };
enum class DcPhaseFilterType : int { /* TBD (Low A/B, Standard A/B...) */ };
enum class UserPresetNo : int { /* TBD */ };

// RE-CONFIRMED size (ctor @0xdd40 disasm): EffectCtrlDmp is a small non-polymorphic PIMPL —
// the ctor writes only this+0 (a heap impl pointer via operator new) and this+4 (a bool); no
// vtable. Real object ≈ 8 bytes. Reserve 0x10 (comfortable margin). (Contrast the 0x100-byte
// CuiAppModule — this one is trivially sizing-safe.)
constexpr std::size_t kEffectCtrlDmpRealSize = 8;

class EffectCtrlDmp {
public:
    EffectCtrlDmp();   // default ctor @libEffectCtrlDmp.so:0xdd40
    ~EffectCtrlDmp();

    // DSEE HX (upscaling)
    void SetDseeHx(bool on);
    void SetDseeHxCustom(bool on);
    void SetDseeHxCustomMode(DseeHxCustomMode mode);
    void SetDseeAi(bool on);
    bool IsDseeHxOn();

    // VPT surround
    void SetVpt(bool on);
    void SetVptMode(VptMode mode);
    bool IsVptOn();

    // Equalizer
    void SetEq10Band(bool on);
    void SetEq10BandValue(Eq10Band band, int gain);
    void SetEq6Band(bool on);
    void SetEq6BandPreset(Eq6BandPreset preset);
    void SetEq6BandValue(Eq6Band band, int gain);

    // Other DSP
    void SetDynamicNormalizer(bool on);
    void SetDcPhaseLinearizer(bool on);
    void SetDcPhaseFilterType(DcPhaseFilterType type);
    void SetVinylizer(bool on);
    void SetVinylizerType(unsigned int type);
    void SetClearAudioPlus(bool on);   // overrides EQ+DSP (one-touch tuning)
    void SetToneControl(bool on);

    // goal #7 — apply the whole effect chain to Bluetooth output
    void SetBtAudioSoundEffect(bool on);

    void DisableSoundEffects();
    void ReenableSoundEffects();

private:
    // reserve the device object's footprint (real ≈ 8 bytes; see kEffectCtrlDmpRealSize).
    alignas(8) unsigned char _device_storage[0x10];
};
static_assert(sizeof(EffectCtrlDmp) >= kEffectCtrlDmpRealSize,
              "EffectCtrlDmp reserved storage smaller than the device object");

} } } // namespace pst::services::sound
