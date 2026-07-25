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
    if (n > 10) n = 10;
    for (int i = 0; i < n; ++i) {
        e->SetEq10BandValue(static_cast<fx::Eq10Band>(i), static_cast<int>(bands[i]));
    }
    return 0;
}

int cinder_effects_set_dsee_hx(int on)            { auto* e = fxc(); if (!e) return -1; e->SetDseeHx(on != 0); return 0; }
int cinder_effects_set_vpt(int on)                { auto* e = fxc(); if (!e) return -1; e->SetVpt(on != 0); return 0; }
int cinder_effects_set_dc_phase(int on)           { auto* e = fxc(); if (!e) return -1; e->SetDcPhaseLinearizer(on != 0); return 0; }
int cinder_effects_set_dynamic_normalizer(int on) { auto* e = fxc(); if (!e) return -1; e->SetDynamicNormalizer(on != 0); return 0; }
int cinder_effects_set_vinylizer(int on)          { auto* e = fxc(); if (!e) return -1; e->SetVinylizer(on != 0); return 0; }
int cinder_effects_set_clearaudio_plus(int on)    { auto* e = fxc(); if (!e) return -1; e->SetClearAudioPlus(on != 0); return 0; }
int cinder_effects_set_bt_audio_effect(int on)    { auto* e = fxc(); if (!e) return -1; e->SetBtAudioSoundEffect(on != 0); return 0; }
int cinder_effects_set_bypass(int bypass)         { auto* e = fxc(); if (!e) return -1; if (bypass) e->DisableSoundEffects(); else e->ReenableSoundEffects(); return 0; }

} // extern "C"
