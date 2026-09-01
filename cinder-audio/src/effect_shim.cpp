// effect_shim.cpp — implements cinder_effects.h over Sony's EffectCtrlDmp (libEffectCtrlDmp.so).
// Built clang -stdlib=libc++ and linked against libEffectCtrlDmp/libc++, same as player_shim.cpp.
//
// The EffectCtrlDmp object is constructed LAZILY on first use and cached. Construction connects to
// the sound service; if that isn't reachable it can crash — the SHELL calls every entry point from
// behind its crash+hang guard (cinder-home run_guarded), so a failure just means effects don't
// apply, the UI keeps running, and it can never brick. Object size is RE-confirmed ~8 bytes
// (effect_abi.hpp), so `new EffectCtrlDmp` can't overflow.
#include "effect_abi.hpp"
#include "cinder_effects.h"
#include "eq_range.h"

namespace fx = pst::services::sound;

namespace {
fx::EffectCtrlDmp* g_fx = nullptr;

// Lazily construct the controller. Returns it, or nullptr if unavailable. (If the device ctor
// faults because the sound service is down, the caller's guard unwinds out of here before the
// assignment, leaving g_fx null — we retry next time; on a healthy device it succeeds once.)
fx::EffectCtrlDmp* fxc() {
    if (!g_fx) {
        g_fx = new fx::EffectCtrlDmp();
    }
    return g_fx;
}
} // namespace

extern "C" {

int cinder_effects_set_eq(const signed char* bands, int n) {
    fx::EffectCtrlDmp* e = fxc();
    if (!e || !bands) return -1;
    e->SetEq10Band(true);
    // CLAMPED. An out-of-range gain does not clamp inside the service, it ZEROES the band — so
    // forwarding whatever we are handed turns a bad settings line into an EQ with a band silently
    // switched off. See src/eq_range.h for the measurement and for where such a value comes from.
    n = cinder_eq_clamp_count(n);
    for (int i = 0; i < n; ++i) {
        e->SetEq10BandValue(static_cast<fx::Eq10Band>(i),
                            cinder_eq_clamp_gain(static_cast<int>(bands[i])));
    }
    return 0;
}

int cinder_effects_set_dsee_hx(int on)            { auto* e = fxc(); if (!e) return -1; e->SetDseeHx(on != 0); return 0; }
int cinder_effects_set_vpt(int on)                { auto* e = fxc(); if (!e) return -1; e->SetVpt(on != 0); return 0; }
// Mode setters/getters for the two effects that have more than on/off. The enum VALUES are not
// known, so these take and return raw ints: the probe writes candidates and reads them back, and
// whatever the service keeps is the answer.
int cinder_effects_set_vpt_mode(int mode)         { auto* e = fxc(); if (!e) return -1; e->SetVptMode(static_cast<pst::services::sound::VptMode>(mode)); return 0; }
int cinder_effects_get_vpt_mode(void)             { auto* e = fxc(); if (!e) return -1; return static_cast<int>(e->GetVptMode()); }
// Whole-chain read-back — see the note in effect_abi.hpp. -1 = no client.
int cinder_effects_is_vpt_on(void)                { auto* e = fxc(); if (!e) return -1; return e->IsVptOn() ? 1 : 0; }
int cinder_effects_is_dsee_hx_on(void)            { auto* e = fxc(); if (!e) return -1; return e->IsDseeHxOn() ? 1 : 0; }
int cinder_effects_is_dsee_ai_on(void)            { auto* e = fxc(); if (!e) return -1; return e->IsDseeAiOn() ? 1 : 0; }
int cinder_effects_is_clearaudio_on(void)         { auto* e = fxc(); if (!e) return -1; return e->IsClearAudioPlusOn() ? 1 : 0; }
int cinder_effects_is_bt_effect_on(void)          { auto* e = fxc(); if (!e) return -1; return e->IsBtAudioSoundEffectOn() ? 1 : 0; }
int cinder_effects_is_source_direct_on(void)      { auto* e = fxc(); if (!e) return -1; return e->IsSourceDirectOn() ? 1 : 0; }
int cinder_effects_is_normalizer_on(void)         { auto* e = fxc(); if (!e) return -1; return e->IsDynamicNormalizerOn() ? 1 : 0; }
int cinder_effects_is_dc_phase_on(void)           { auto* e = fxc(); if (!e) return -1; return e->IsDcPhaseLinearizerOn() ? 1 : 0; }
int cinder_effects_is_vinylizer_on(void)          { auto* e = fxc(); if (!e) return -1; return e->IsVinylizerOn() ? 1 : 0; }
int cinder_effects_is_eq10_on(void)               { auto* e = fxc(); if (!e) return -1; return e->IsEq10BandOn() ? 1 : 0; }
int cinder_effects_is_eq6_on(void)                { auto* e = fxc(); if (!e) return -1; return e->IsEq6BandOn() ? 1 : 0; }
int cinder_effects_is_tone_on(void)               { auto* e = fxc(); if (!e) return -1; return e->IsToneControlOn() ? 1 : 0; }
int cinder_effects_is_clear_phase_hp_on(void)     { auto* e = fxc(); if (!e) return -1; return e->IsClearPhaseHeadphoneOn() ? 1 : 0; }
int cinder_effects_get_select_using_eq(void)      { auto* e = fxc(); if (!e) return -1; return e->GetSelectUsingEq(); }
int cinder_effects_get_eq_band(int i)             { auto* e = fxc(); if (!e) return 0; return e->GetEq10BandValue(static_cast<pst::services::sound::Eq10Band>(i)); }
float cinder_effects_get_eq_band_db(int i)        { auto* e = fxc(); if (!e) return 0.0f; return e->GetEq10BandValuedB(static_cast<pst::services::sound::Eq10Band>(i)); }
// Same clamp as the whole-curve setter above — this is the per-band path the incremental apply
// uses, and it reaches exactly the same call. The index is bounds-checked for the same reason:
// Eq10Band is an enum with ten values and a cast does not check.
int cinder_effects_set_eq_band(int i, int gain)   { auto* e = fxc(); if (!e) return -1; if (i < 0 || i > 9) return -1; e->SetEq10BandValue(static_cast<pst::services::sound::Eq10Band>(i), cinder_eq_clamp_gain(gain)); return 0; }
int cinder_effects_set_select_using_eq(int t)     { auto* e = fxc(); if (!e) return -1; e->SetSelectUsingEq(static_cast<pst::services::sound::EqType>(t)); return 0; }
// Which tone system is IN THE PATH: 0 none, 1 the 6-band, 2 the 10-band, 3 Tone Control. Same call
// as above, named — the ordinals were settled on device (see effect_abi.hpp EqType) and the raw
// int only ever meant "we do not know yet".
int cinder_effects_set_tone_system(int sys)       { auto* e = fxc(); if (!e) return -1; e->SetSelectUsingEq(static_cast<pst::services::sound::EqType>(sys)); return 0; }
int cinder_effects_get_vinylizer_type(void)       { auto* e = fxc(); if (!e) return -1; return (int)e->GetVinylizerType(); }
int cinder_effects_set_dc_phase_type(int type)    { auto* e = fxc(); if (!e) return -1; e->SetDcPhaseFilterType(static_cast<pst::services::sound::DcPhaseFilterType>(type)); return 0; }
int cinder_effects_get_dc_phase_type(void)        { auto* e = fxc(); if (!e) return -1; return static_cast<int>(e->GetDcPhaseFilterType()); }
int cinder_effects_set_dc_phase(int on)           { auto* e = fxc(); if (!e) return -1; e->SetDcPhaseLinearizer(on != 0); return 0; }
int cinder_effects_set_dynamic_normalizer(int on) { auto* e = fxc(); if (!e) return -1; e->SetDynamicNormalizer(on != 0); return 0; }
int cinder_effects_set_vinylizer(int on)          { auto* e = fxc(); if (!e) return -1; e->SetVinylizer(on != 0); return 0; }
int cinder_effects_set_clearaudio_plus(int on)    { auto* e = fxc(); if (!e) return -1; e->SetClearAudioPlus(on != 0); return 0; }
int cinder_effects_set_bt_audio_effect(int on)    { auto* e = fxc(); if (!e) return -1; e->SetBtAudioSoundEffect(on != 0); return 0; }
int cinder_effects_set_bypass(int bypass)         { auto* e = fxc(); if (!e) return -1; if (bypass) e->DisableSoundEffects(); else e->ReenableSoundEffects(); return 0; }

// ── the rest of Sony's surface (2026-08-17) ───────────────────────────────────────────────────
// Every symbol below was verified present in libEffectCtrlDmp.so's dynamic table before being
// declared; see analysis/RE_dsp_effects_surface.md. Anything Sony exports that describes hardware
// this unit does not have (ClearPhase Speaker/Wmport) is deliberately NOT wired.
namespace { using namespace pst::services::sound; }

// Source Direct — bypasses the whole chain. Overrides everything below it, exactly like
// ClearAudioPlus does, which is why the UI has to grey out what it hides.
int cinder_effects_set_source_direct(int on)      { auto* e = fxc(); if (!e) return -1; e->SetSourceDirect(on != 0); return 0; }
int cinder_effects_is_source_direct(void)         { auto* e = fxc(); if (!e) return -1; return e->IsSourceDirectOn() ? 1 : 0; }

int cinder_effects_set_clear_phase(int on)        { auto* e = fxc(); if (!e) return -1; e->SetClearPhaseHeadphone(on != 0); return 0; }
int cinder_effects_is_clear_phase(void)           { auto* e = fxc(); if (!e) return -1; return e->IsClearPhaseHeadphoneOn() ? 1 : 0; }

int cinder_effects_set_dsee_ai(int on)            { auto* e = fxc(); if (!e) return -1; e->SetDseeAi(on != 0); return 0; }
int cinder_effects_is_dsee_ai(void)               { auto* e = fxc(); if (!e) return -1; return e->IsDseeAiOn() ? 1 : 0; }

int cinder_effects_set_dsee_hx_custom(int on)     { auto* e = fxc(); if (!e) return -1; e->SetDseeHxCustom(on != 0); return 0; }
int cinder_effects_is_dsee_hx_custom(void)        { auto* e = fxc(); if (!e) return -1; return e->IsDseeHxCustomOn() ? 1 : 0; }
int cinder_effects_set_dsee_hx_mode(int mode)     { auto* e = fxc(); if (!e) return -1; e->SetDseeHxCustomMode(static_cast<DseeHxCustomMode>(mode)); return 0; }
int cinder_effects_get_dsee_hx_mode(void)         { auto* e = fxc(); if (!e) return -1; return static_cast<int>(e->GetDseeHxCustomMode()); }

int cinder_effects_set_vinylizer_type(int type)   { auto* e = fxc(); if (!e) return -1; e->SetVinylizerType(static_cast<unsigned>(type)); return 0; }
/* cinder_effects_get_vinylizer_type is defined above with the other read-backs. */

// Tone Control: three bands, each with a selectable centre frequency. Mutually exclusive with the
// 10-band EQ — SetSelectUsingEq decides which one is actually in the path.
int cinder_effects_set_tone_control(int on)       { auto* e = fxc(); if (!e) return -1; e->SetToneControl(on != 0); return 0; }
int cinder_effects_is_tone_control(void)          { auto* e = fxc(); if (!e) return -1; return e->IsToneControlOn() ? 1 : 0; }
int cinder_effects_set_tone_value(int band, int gain) { auto* e = fxc(); if (!e) return -1; e->SetToneValue(static_cast<ToneType>(band), gain); return 0; }
int cinder_effects_get_tone_value(int band)       { auto* e = fxc(); if (!e) return -1; return e->GetToneValue(static_cast<ToneType>(band)); }
float cinder_effects_get_tone_value_db(int band)  { auto* e = fxc(); if (!e) return 0.0f; return e->GetToneValuedB(static_cast<ToneType>(band)); }
int cinder_effects_set_tone_freq(int band, int f) { auto* e = fxc(); if (!e) return -1; e->SetToneCenterFreq(static_cast<ToneType>(band), static_cast<ToneCenterFreq>(f)); return 0; }
int cinder_effects_get_tone_freq(int band)        { auto* e = fxc(); if (!e) return -1; return static_cast<int>(e->GetToneCenterFreq(static_cast<ToneType>(band))); }

// 6-band EQ — where Sony's NAMED presets live (Bright/Excited/Mellow/Relaxed/Vocal/Custom 1/2).
// The ten-band Cinder already drives has no presets of its own.
int cinder_effects_set_eq6(int on)                { auto* e = fxc(); if (!e) return -1; e->SetEq6Band(on != 0); return 0; }
int cinder_effects_is_eq6(void)                   { auto* e = fxc(); if (!e) return -1; return e->IsEq6BandOn() ? 1 : 0; }
int cinder_effects_set_eq6_preset(int p)          { auto* e = fxc(); if (!e) return -1; e->SetEq6BandPreset(static_cast<Eq6BandPreset>(p)); return 0; }
int cinder_effects_get_eq6_preset(void)           { auto* e = fxc(); if (!e) return -1; return static_cast<int>(e->GetEq6BandPreset()); }
int cinder_effects_set_eq6_band(int b, int gain)  { auto* e = fxc(); if (!e) return -1; e->SetEq6BandValue(static_cast<Eq6Band>(b), gain); return 0; }
int cinder_effects_get_eq6_band(int b)            { auto* e = fxc(); if (!e) return -1; return e->GetEq6BandValue(static_cast<Eq6Band>(b)); }
float cinder_effects_get_eq6_band_db(int b)       { auto* e = fxc(); if (!e) return 0.0f; return e->GetEq6BandValuedB(static_cast<Eq6Band>(b)); }

// Sony's own two saved setups.
int cinder_effects_save_user_preset(int no)       { auto* e = fxc(); if (!e) return -1; e->SaveUserPreset(static_cast<UserPresetNo>(no)); return 0; }
int cinder_effects_load_user_preset(int no)       { auto* e = fxc(); if (!e) return -1; e->LoadUserPreset(static_cast<UserPresetNo>(no)); return 0; }

// Read-backs used to grey out rows that something upstream is overriding.
int cinder_effects_is_clearaudio_plus(void)       { auto* e = fxc(); if (!e) return -1; return e->IsClearAudioPlusOn() ? 1 : 0; }

} // extern "C"
