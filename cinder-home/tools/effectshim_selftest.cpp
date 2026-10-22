// effectshim_selftest — host test for cinder-audio/src/effect_shim.cpp, the shell's only way into
// Sony's EffectCtrlDmp (docs/SHORTCOMINGS.md §A2).
//
// The shim calls ordinary exported member functions of pst::services::sound::EffectCtrlDmp, not
// vtable slots, so a fake that defines the same members (from the same effect_abi.hpp) receives
// every call the device library would. Each fake member writes one line to a trace, and each
// check below says: this cinder_effects_* entry reaches exactly this Sony method, with these
// arguments, and nothing else. What it cannot check is the device library itself (that the
// declarations match its exports is what the ARM link in build.sh proves).
//
// The shim is included whole, so this tests the shipping file, not a copy.
#include <cstdio>
#include <cstring>
#include <string>

#include "../../cinder-audio/src/effect_shim.cpp"

namespace {
std::string g_trace;
int g_ctors = 0;
bool g_on = false;        // what every Is…On() answers
int g_int = 0;            // what every int getter answers
float g_db = 0.0f;        // what every …dB getter answers

void rec(const char* name, long a = -999, long b = -999) {
    char buf[96];
    if (b != -999) std::snprintf(buf, sizeof buf, "%s(%ld,%ld);", name, a, b);
    else if (a != -999) std::snprintf(buf, sizeof buf, "%s(%ld);", name, a);
    else std::snprintf(buf, sizeof buf, "%s();", name);
    g_trace += buf;
}
template <class E> long n(E e) { return static_cast<long>(e); }
} // namespace

// ── the fake EffectCtrlDmp: every member effect_shim.cpp can reach ───────────────────────────
namespace pst { namespace services { namespace sound {
EffectCtrlDmp::EffectCtrlDmp() { ++g_ctors; }
EffectCtrlDmp::~EffectCtrlDmp() {}
#define SET_BOOL(M) void EffectCtrlDmp::M(bool on) { rec(#M, on); }
#define IS(M) bool EffectCtrlDmp::M() { rec(#M); return g_on; }
SET_BOOL(SetDseeHx) SET_BOOL(SetDseeHxCustom) SET_BOOL(SetDseeAi) SET_BOOL(SetVpt)
SET_BOOL(SetEq10Band) SET_BOOL(SetEq6Band) SET_BOOL(SetDynamicNormalizer)
SET_BOOL(SetDcPhaseLinearizer) SET_BOOL(SetVinylizer) SET_BOOL(SetClearAudioPlus)
SET_BOOL(SetSourceDirect) SET_BOOL(SetClearPhaseHeadphone) SET_BOOL(SetToneControl)
SET_BOOL(SetBtAudioSoundEffect)
IS(IsDseeHxOn) IS(IsVptOn) IS(IsClearAudioPlusOn) IS(IsBtAudioSoundEffectOn) IS(IsSourceDirectOn)
IS(IsDynamicNormalizerOn) IS(IsDcPhaseLinearizerOn) IS(IsVinylizerOn) IS(IsEq10BandOn)
IS(IsEq6BandOn) IS(IsToneControlOn) IS(IsClearPhaseHeadphoneOn) IS(IsDseeAiOn) IS(IsDseeHxCustomOn)
void EffectCtrlDmp::SetDseeHxCustomMode(DseeHxCustomMode m) { rec("SetDseeHxCustomMode", n(m)); }
DseeHxCustomMode EffectCtrlDmp::GetDseeHxCustomMode() { rec("GetDseeHxCustomMode"); return DseeHxCustomMode(g_int); }
void EffectCtrlDmp::SetVptMode(VptMode m) { rec("SetVptMode", n(m)); }
VptMode EffectCtrlDmp::GetVptMode() { rec("GetVptMode"); return VptMode(g_int); }
int EffectCtrlDmp::GetSelectUsingEq() { rec("GetSelectUsingEq"); return g_int; }
void EffectCtrlDmp::SetSelectUsingEq(EqType t) { rec("SetSelectUsingEq", n(t)); }
unsigned EffectCtrlDmp::GetVinylizerType() { rec("GetVinylizerType"); return unsigned(g_int); }
void EffectCtrlDmp::SetEq10BandValue(Eq10Band b, int g) { rec("SetEq10BandValue", n(b), g); }
int EffectCtrlDmp::GetEq10BandValue(Eq10Band b) { rec("GetEq10BandValue", n(b)); return g_int; }
float EffectCtrlDmp::GetEq10BandValuedB(Eq10Band b) { rec("GetEq10BandValuedB", n(b)); return g_db; }
void EffectCtrlDmp::SetEq6BandPreset(Eq6BandPreset p) { rec("SetEq6BandPreset", n(p)); }
void EffectCtrlDmp::SetEq6BandValue(Eq6Band b, int g) { rec("SetEq6BandValue", n(b), g); }
int EffectCtrlDmp::GetEq6BandValue(Eq6Band b) { rec("GetEq6BandValue", n(b)); return g_int; }
float EffectCtrlDmp::GetEq6BandValuedB(Eq6Band b) { rec("GetEq6BandValuedB", n(b)); return g_db; }
Eq6BandPreset EffectCtrlDmp::GetEq6BandPreset() { rec("GetEq6BandPreset"); return Eq6BandPreset(g_int); }
void EffectCtrlDmp::SetDcPhaseFilterType(DcPhaseFilterType t) { rec("SetDcPhaseFilterType", n(t)); }
DcPhaseFilterType EffectCtrlDmp::GetDcPhaseFilterType() { rec("GetDcPhaseFilterType"); return DcPhaseFilterType(g_int); }
void EffectCtrlDmp::SetVinylizerType(unsigned t) { rec("SetVinylizerType", long(t)); }
void EffectCtrlDmp::SetToneValue(ToneType b, int g) { rec("SetToneValue", n(b), g); }
int EffectCtrlDmp::GetToneValue(ToneType b) { rec("GetToneValue", n(b)); return g_int; }
float EffectCtrlDmp::GetToneValuedB(ToneType b) { rec("GetToneValuedB", n(b)); return g_db; }
void EffectCtrlDmp::SetToneCenterFreq(ToneType b, ToneCenterFreq f) { rec("SetToneCenterFreq", n(b), n(f)); }
ToneCenterFreq EffectCtrlDmp::GetToneCenterFreq(ToneType b) { rec("GetToneCenterFreq", n(b)); return ToneCenterFreq(g_int); }
void EffectCtrlDmp::SaveUserPreset(UserPresetNo p) { rec("SaveUserPreset", n(p)); }
void EffectCtrlDmp::LoadUserPreset(UserPresetNo p) { rec("LoadUserPreset", n(p)); }
void EffectCtrlDmp::DisableSoundEffects() { rec("DisableSoundEffects"); }
void EffectCtrlDmp::ReenableSoundEffects() { rec("ReenableSoundEffects"); }
} } } // namespace pst::services::sound

static int fails = 0;
static void expect(const char* what, const std::string& want) {
    const bool ok = g_trace == want;
    std::printf("  %-4s %s\n", ok ? "ok" : "FAIL", what);
    if (!ok) {
        std::printf("       got  %s\n       want %s\n", g_trace.c_str(), want.c_str());
        fails = 1;
    }
    g_trace.clear();
}
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
    g_trace.clear();
}

int main() {
    std::printf("the whole 10-band curve\n");
    const signed char curve[10] = { 0, 4, -4, 20, -20, 127, -128, 21, -21, 1 };
    eq("returns 0", cinder_effects_set_eq(curve, 10), 0);
    cinder_effects_set_eq(curve, 10);
    expect("switches the 10-band on, then writes each band, out-of-range ones CLAMPED to ±20",
           "SetEq10Band(1);SetEq10BandValue(0,0);SetEq10BandValue(1,4);SetEq10BandValue(2,-4);"
           "SetEq10BandValue(3,20);SetEq10BandValue(4,-20);SetEq10BandValue(5,20);"
           "SetEq10BandValue(6,-20);SetEq10BandValue(7,20);SetEq10BandValue(8,-20);"
           "SetEq10BandValue(9,1);");
    const signed char twelve[12] = { 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 9 };
    cinder_effects_set_eq(twelve, 12);
    eq("a count past ten writes ten bands, never an eleventh",
       (long)std::string(g_trace).find("SetEq10BandValue(10"), (long)std::string::npos);
    eq("no curve: -1 and no write", cinder_effects_set_eq(nullptr, 10), -1);
    eq("one construction, however many calls", g_ctors, 1);

    std::printf("one band\n");
    cinder_effects_set_eq_band(3, 99);
    expect("a gain past +20 is clamped on the per-band path too", "SetEq10BandValue(3,20);");
    eq("band 10 is refused", cinder_effects_set_eq_band(10, 1), -1);
    eq("band -1 is refused", cinder_effects_set_eq_band(-1, 1), -1);
    cinder_effects_set_eq_band(10, 1);
    expect("…and reaches no Sony call", "");

    std::printf("which tone system is in the path\n");
    cinder_effects_set_tone_system(2);
    expect("set_tone_system(2) selects the 10-band", "SetSelectUsingEq(2);");
    cinder_effects_set_tone_system(3);
    expect("set_tone_system(3) selects Tone Control", "SetSelectUsingEq(3);");

    std::printf("switches reach their own setter\n");
    struct { int (*fn)(int); const char* want; } sw[] = {
        { cinder_effects_set_eq10_on, "SetEq10Band(1);" },
        { cinder_effects_set_dsee_hx, "SetDseeHx(1);" },
        { cinder_effects_set_vpt, "SetVpt(1);" },
        { cinder_effects_set_dc_phase, "SetDcPhaseLinearizer(1);" },
        { cinder_effects_set_dynamic_normalizer, "SetDynamicNormalizer(1);" },
        { cinder_effects_set_vinylizer, "SetVinylizer(1);" },
        { cinder_effects_set_clearaudio_plus, "SetClearAudioPlus(1);" },
        { cinder_effects_set_bt_audio_effect, "SetBtAudioSoundEffect(1);" },
        { cinder_effects_set_source_direct, "SetSourceDirect(1);" },
        { cinder_effects_set_clear_phase, "SetClearPhaseHeadphone(1);" },
        { cinder_effects_set_dsee_ai, "SetDseeAi(1);" },
        { cinder_effects_set_dsee_hx_custom, "SetDseeHxCustom(1);" },
        { cinder_effects_set_tone_control, "SetToneControl(1);" },
        { cinder_effects_set_eq6, "SetEq6Band(1);" },
    };
    for (auto& s : sw) {
        s.fn(5);   // any non-zero is on
        expect(s.want, s.want);
    }
    cinder_effects_set_vpt(0);
    expect("0 is off", "SetVpt(0);");

    std::printf("modes and values pass through\n");
    cinder_effects_set_vinylizer_type(3);
    cinder_effects_set_dc_phase_type(2);
    cinder_effects_set_vpt_mode(4);
    cinder_effects_set_dsee_hx_mode(1);
    cinder_effects_set_tone_value(0, -6);
    cinder_effects_set_tone_freq(2, 5);
    cinder_effects_set_eq6_preset(9);
    cinder_effects_set_eq6_band(0, 10);
    cinder_effects_save_user_preset(1);
    cinder_effects_load_user_preset(0);
    expect("each to its own method, arguments in order",
           "SetVinylizerType(3);SetDcPhaseFilterType(2);SetVptMode(4);SetDseeHxCustomMode(1);"
           "SetToneValue(0,-6);SetToneCenterFreq(2,5);SetEq6BandPreset(9);SetEq6BandValue(0,10);"
           "SaveUserPreset(1);LoadUserPreset(0);");

    std::printf("bypass is Sony's Disable/Reenable pair\n");
    cinder_effects_set_bypass(1);
    cinder_effects_set_bypass(0);
    expect("1 disables, 0 re-enables", "DisableSoundEffects();ReenableSoundEffects();");

    std::printf("read-backs\n");
    g_on = true;
    eq("is_vpt_on is the service's answer", cinder_effects_is_vpt_on(), 1);
    g_on = false;
    eq("…and 0 when it says off", cinder_effects_is_source_direct_on(), 0);
    g_int = 7;
    eq("get_select_using_eq", cinder_effects_get_select_using_eq(), 7);
    eq("get_vinylizer_type", cinder_effects_get_vinylizer_type(), 7);
    g_db = -3.5f;
    const float db = cinder_effects_get_eq_band_db(4);
    eq("a dB getter comes back as the float it is (x10)", (long)(db * 10), -35);
    cinder_effects_get_tone_value_db(1);
    expect("get_tone_value_db asks GetToneValuedB for that band", "GetToneValuedB(1);");

    std::printf(fails ? "effectshim_selftest: FAILED\n" : "effectshim_selftest: all passed\n");
    return fails;
}
