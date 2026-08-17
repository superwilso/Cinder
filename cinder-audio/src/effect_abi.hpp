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
// MEASURED 2026-08-17 (`cinder-probe --tone`). SetEq6BandPreset is the rare setter with a visible
// side effect — a real preset MOVES the six band values, so the curves below are read, not
// guessed. 0..8 are real; 9 and up leave the curve flat, i.e. the enum ends at 8:
//
//     0  0, 0, 0, 0, 0, 0     3  2, 0, 0,-3, 0, 3     6  3,-3, 0, 3, 0, 6
//     1  2, 0,-3, 3, 3, 0     4  6, 0, 3, 0,-3, 6     7 10, 0, 0, 0, 6, 9
//     2  0, 3, 3, 0, 0,-3     5  3, 0, 0, 6, 0, 0     8  0, 1, 2,-1,-2, 0
//
// CONFIRMED 2026-08-17 (`cinder-probe --eq6custom`): the enum runs 0..10, not 0..8. Presets 9 and
// 10 read flat only because nothing has ever written them — they are the two **UserCustom** slots,
// and band writes STICK under those two and are rejected under every other, which the service says
// out loud: `EffectCtrlDmp.cc:534 !!! cannot set value except for UserCustom preset`.
//
// So the 6-band IS editable, via 9/10. The fixed names (the catalogue lists Bright, Excited,
// Mellow, Relaxed, Vocal, Custom 1, Custom 2) are still NOT mapped onto 1..8 — seven names, eight
// curves — so nothing here is labelled with a guess.
enum class Eq6BandPreset : int { /* 0..8 fixed curves above; 9 and 10 are UserCustom 1 and 2 */ };
// Values still unrecovered — but UNLIKE most of these, both of the mode enums below have an
// exported GETTER (EffectCtrlDmp::GetVptMode / ::GetDcPhaseFilterType), so they can be settled by
// experiment rather than by decompiling: write a candidate, read it back, and see which values the
// service keeps. `cinder-probe --vpt` does exactly that. Declared as an empty `enum class : int` so
// a raw int can be static_cast in without inventing enumerator names we have not confirmed.
// Which tone system is actually IN THE PATH. Sony's manual is explicit that the Equalizer and the
// Tone Control are alternatives whose settings are saved separately — so this is a selector, not a
// pair of independent toggles, and having EQ10/EQ6/ToneControl all read "on" at once (as the device
// does) means nothing without it. Values TBD; probe with --eqsel.
// SETTLED ON DEVICE 2026-08-17, and not by ear. The sound service logs
// `<Effect>::UpdateProcCond(bool,bool) … isproc is N`, and isproc is the service saying whether
// that effect is ACTUALLY PROCESSING — the one signal on this device that distinguishes "stored"
// from "in the path". Switching all three tone systems on, selecting each candidate in turn and
// reading which one reported isproc 1 gives:
//
//     0 -> none of them          2 -> Eq10band   (Cinder's own EQ screen)
//     1 -> Eq6band               3 -> EqTone     (Tone Control)
//
// The device was sitting on 1, so the 10-band Cinder has been writing since June was stored and
// NEVER IN THE PATH. `cinder-probe --inpath <t>` reproduces the measurement.
enum class EqType : int { None = 0, Eq6Band = 1, Eq10Band = 2, ToneControl = 3 };

enum class VptMode : int { /* values TBD — probe with --vpt */ };
enum class DseeHxCustomMode : int { /* TBD */ };
enum class DcPhaseFilterType : int { /* values TBD — probe with --vpt */ };
enum class UserPresetNo : int { /* TBD */ };

// Tone Control — the SECOND tone system, mutually exclusive with the EQ via SetSelectUsingEq.
// Three bands, each with its own selectable centre frequency, which is what makes it a different
// control rather than a coarser EQ. Catalogue order (see analysis/RE_dsp_effects_surface.md):
// BASS, MIDDLE, TREBLE. Centre-frequency values are per-band and NOT yet recovered — the
// catalogue lists the frequencies as display strings, not as an enum with known ordinals.
// ORDINALS CONFIRMED 2026-08-17: the service logs `eqtone,type=N` with N in {0,1,2} as these are
// written, so the catalogue order IS the enum order.
enum class ToneType : int { Bass = 0, Middle = 1, Treble = 2 };
enum class ToneCenterFreq : int { /* TBD — per-band frequency list */ };

// RE-CONFIRMED size — CORRECTED 2026-07-02 after an on-device heap corruption. The ctor @0xdd40
// writes this+0 (impl ptr), this+4 (bool), AND THEN `memset(this+8, 0, 0xA0)` (insns @0xdd5e-dd66:
// add r0,this,#8; mov r2,#0xa0; blx memset) — the first RE pass missed the memset and called it
// ~8 bytes. Real object = 0xA8 (168) bytes. The 0x10-byte reservation let the device ctor zero
// 152 bytes of NEIGHBORING heap chunks → `malloc(): memory corruption (fast)` abort on the very
// first on-device construction (2026-07-02, boot-time saved-EQ re-apply). Reserve 0x100.
constexpr std::size_t kEffectCtrlDmpRealSize = 0xA8;

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
    VptMode GetVptMode();          // exported; the read-back that makes the enum probeable
    bool IsVptOn();

    // Read-back for the WHOLE chain. Added 2026-08-17 to answer "the effect is set but I cannot
    // hear it": on this device a setter landing proves nothing (see the high-gain finding), and
    // several of these gate each other — ClearAudioPlus overrides the manual EQ and DSP outright,
    // SourceDirect bypasses the lot, and BtAudioSoundEffect decides whether ANY of it reaches a
    // Bluetooth sink. Without these you cannot tell "not applied" from "applied and inaudible".
    bool IsClearAudioPlusOn();
    bool IsBtAudioSoundEffectOn();
    bool IsSourceDirectOn();
    bool IsDynamicNormalizerOn();
    bool IsDcPhaseLinearizerOn();
    bool IsVinylizerOn();
    bool IsEq10BandOn();
    bool IsEq6BandOn();
    bool IsToneControlOn();
    bool IsClearPhaseHeadphoneOn();
    bool IsDseeAiOn();
    int  GetSelectUsingEq();
    void SetSelectUsingEq(EqType t);
    unsigned GetVinylizerType();

    // Equalizer
    void SetEq10Band(bool on);
    void SetEq10BandValue(Eq10Band band, int gain);
    int  GetEq10BandValue(Eq10Band band);   // so a probe can put the user's curve back
    // MEASURED 2026-08-17: every `...dB` getter returns a FLOAT, not an int. Declaring them
    // int reads r0 while the value is in s0, and yields a constant 0 on armhf — which is
    // exactly how a working getter can be mistaken for a dead one.
    float GetEq10BandValuedB(Eq10Band band); // the CONTROL: its scale is already measured
    void SetEq6Band(bool on);
    void SetEq6BandPreset(Eq6BandPreset preset);
    void SetEq6BandValue(Eq6Band band, int gain);
    // Both getters exported, and the dB one is the useful half: the raw value echoes whatever the
    // service was handed (proven for VptMode and SelectUsingEq), but the dB reading is COMPUTED by
    // the service, so it settles the unit and the clamp without needing ears. Same trick that
    // measured the 10-band's half-decibels.
    int   GetEq6BandValue(Eq6Band band);
    float GetEq6BandValuedB(Eq6Band band);

    // Other DSP
    void SetDynamicNormalizer(bool on);
    void SetDcPhaseLinearizer(bool on);
    void SetDcPhaseFilterType(DcPhaseFilterType type);
    DcPhaseFilterType GetDcPhaseFilterType();
    void SetVinylizer(bool on);
    void SetVinylizerType(unsigned int type);
    void SetClearAudioPlus(bool on);   // overrides EQ+DSP (one-touch tuning)

    // Source Direct — bypasses the whole chain for the shortest path. NOT the same as Cinder's
    // A/B bypass, which uses DisableSoundEffects: this is Sony's own user-facing control, and it
    // silently overrides everything below it (cf. ClearAudioPlus).
    void SetSourceDirect(bool on);

    // Clear Phase. Only the HEADPHONE variant is meaningful on an A55 — Speaker and Wmport
    // describe hardware this unit does not have (same story as `smaster btl`), so they are
    // deliberately NOT exposed here even though the symbols exist.
    void SetClearPhaseHeadphone(bool on);

    // Tone Control — three bands, each with an adjustable centre frequency.
    void SetToneControl(bool on);
    void SetToneValue(ToneType band, int gain);
    int  GetToneValue(ToneType band);
    float GetToneValuedB(ToneType band);  // same reading, converted to dB by the service
    void SetToneCenterFreq(ToneType band, ToneCenterFreq f);
    ToneCenterFreq GetToneCenterFreq(ToneType band);

    // DSEE HX Custom + the 6-band's preset read-back
    bool IsDseeHxCustomOn();
    DseeHxCustomMode GetDseeHxCustomMode();
    Eq6BandPreset GetEq6BandPreset();

    // Sony's own "two saved setups" — the natural backing store for Cinder's A/B, if we ever want
    // the setups to survive being edited from the stock UI too.
    void SaveUserPreset(UserPresetNo no);
    void LoadUserPreset(UserPresetNo no);

    // goal #7 — apply the whole effect chain to Bluetooth output
    void SetBtAudioSoundEffect(bool on);

    void DisableSoundEffects();
    void ReenableSoundEffects();

private:
    // reserve the device object's footprint (real = 0xA8; see kEffectCtrlDmpRealSize).
    alignas(8) unsigned char _device_storage[0x100];
};
static_assert(sizeof(EffectCtrlDmp) >= kEffectCtrlDmpRealSize,
              "EffectCtrlDmp reserved storage smaller than the device object");

} } } // namespace pst::services::sound
