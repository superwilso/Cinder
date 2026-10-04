// sound_settings_abi.hpp — hand-written declarations of Sony's SECOND sound-settings class,
// pst::services::sound::SoundServiceSettingsDmp (libSoundServiceSettingsDmp.so). Recovered offline
// on 2026-09-30 for the R5 effects-parity pass; before that nothing in Cinder knew it existed.
//
// WHAT IT IS. EffectCtrlDmp (effect_abi.hpp) is the effect CHAIN. This is the other half of what
// HgrmMediaPlayerApp imports for sound: six settings that decide how a stream is RENDERED rather
// than what is done to it —
//
//     DSD → PCM conversion filter   ("Slow Roll-Off" / "Sharp Roll-Off" in the UI catalogue)
//     DSD → PCM conversion gain     ("0 dB" / "-3 dB")
//     built-in output DSD processing mode, and the USB output DSD mode  (native / converted)
//     LPCM playback mode            (meaning unrecovered; RendererDmpMaster::UpdateLpcmPlaybackMode)
//     headphone model               (Sony's noise-cancelling headphones)
//
// ╔══════════════════════════════════════════════════════════════════════════════════════════╗
// ║ EVERY SIGNATURE HERE IS FROM DISASSEMBLY AND UNVERIFIED ON DEVICE (DEVICE_CHECKLIST 26.7).║
// ║ Nothing in cinder-home calls this class; it is linked into cinder-probe only.            ║
// ╚══════════════════════════════════════════════════════════════════════════════════════════╝
//
// HOW EACH FACT WAS READ (arm-linux-gnueabihf-objdump -dC libSoundServiceSettingsDmp.so):
//
//  * SIZE. The ctor @0x6628 does `operator new(1)` and stores the pointer at this+0 — nothing
//    else. The object is ONE POINTER (4 bytes); the dtor @0x6664 deletes it. 16 bytes are reserved
//    below, so the sizing mistake that corrupted the heap for EffectCtrlDmp (0xA8 real, 0x10
//    reserved) cannot repeat here.
//  * SETTERS (e.g. SetBuiltinOutputDsdProcessingMode @0x66b0): r0 = this, r1 = the enum BY VALUE.
//    The body builds the string `<key>=<to_string(unsigned value)>` and hands it to
//    SoundServiceSettings::SetParams(const std::string&); what SetParams returned comes back in r0.
//    So the setters are NOT void — they return SetParams' status, declared `int` here.
//  * GETTERS (e.g. GetBuiltinOutputDsdProcessingMode @0x678c): r0 = this, no arguments. The body
//    calls SoundServiceSettings::GetParams(key, std::string& out); if that returns 0 it returns
//    `std::stoi(out)`, otherwise 0. TWO CONSEQUENCES:
//      - a failed read and a real value of 0 are INDISTINGUISHABLE from the return value;
//      - `std::stoi` THROWS std::invalid_argument on an empty or non-numeric reply, and nothing in
//        the library catches it. The C shim wraps every getter in try/catch for that reason — an
//        exception escaping an `extern "C"` function is std::terminate, i.e. no Home app.
//  * SetHeadphoneModel @0x6ea8 is the exception to "one SetParams call": after it, the body
//    constructs a pst::services::ncasm::NcAsmService and calls ITS SetParams too — it reconfigures
//    the noise-cancelling service. It is declared for completeness and the C shim does NOT wrap
//    it: the feature is inert without Sony's NC headphones and a second service is a second way
//    to hang.
//  * There is NO `…Option const*` argument anywhere in this class, so the "never pass NULL" rule
//    (reference: PrevTrack) has nothing to bite on here — every argument is a plain int by value.
//
// THE ENUM VALUES ARE NOT RECOVERED. The catalogue gives labels, not ordinals, and an echoed
// read-back does not bound an enum on this device (see effect_abi.hpp). They are declared as empty
// `enum class : int` so a raw int can be cast in without inventing enumerator names.
#pragma once
#include <cstddef>

namespace pst { namespace services { namespace sound {

enum class DsdProcessingMode : int { /* unrecovered */ };
enum class DsdOutputMode : int { /* unrecovered */ };
enum class DsdConvFilterType : int { /* catalogue: Slow Roll-Off, Sharp Roll-Off — order unsettled */ };
enum class DsdConvGainMode : int { /* catalogue: 0 dB, -3 dB — order unsettled */ };
enum class LpcmPlaybackMode : int { /* unrecovered */ };
enum class HeadphoneModel : int { /* unrecovered; inert without Sony's NC headphones */ };

// One pointer — see SIZE above.
constexpr std::size_t kSoundServiceSettingsDmpRealSize = 4;

class SoundServiceSettingsDmp {
public:
    SoundServiceSettingsDmp();    // @0x6628
    ~SoundServiceSettingsDmp();   // @0x6664

    int SetBuiltinOutputDsdProcessingMode(DsdProcessingMode m);   // @0x66b0
    DsdProcessingMode GetBuiltinOutputDsdProcessingMode();        // @0x678c
    int SetUacOutputDsdOutputMode(DsdOutputMode m);               // @0x6848
    DsdOutputMode GetUacOutputDsdOutputMode();                    // @0x6924
    int SetDsdConvGainMode(DsdConvGainMode m);                    // @0x69e0
    DsdConvGainMode GetDsdConvGainMode();                         // @0x6abc
    int SetDsdConvFilterType(DsdConvFilterType t);                // @0x6b78
    DsdConvFilterType GetDsdConvFilterType();                     // @0x6c54
    int SetLpcmPlaybackMode(LpcmPlaybackMode m);                  // @0x6d10
    LpcmPlaybackMode GetLpcmPlaybackMode();                       // @0x6dec
    int SetHeadphoneModel(HeadphoneModel m);                      // @0x6ea8
    HeadphoneModel GetHeadphoneModel();                           // @0x6fec

private:
    alignas(8) unsigned char _device_storage[16];
};
static_assert(sizeof(SoundServiceSettingsDmp) >= kSoundServiceSettingsDmpRealSize,
              "SoundServiceSettingsDmp reserved storage smaller than the device object");

} } } // namespace pst::services::sound
