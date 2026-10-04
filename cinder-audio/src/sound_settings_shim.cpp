// sound_settings_shim.cpp — implements cinder_sound_settings.h over Sony's SoundServiceSettingsDmp
// (libSoundServiceSettingsDmp.so). Built like effect_shim.cpp (clang -stdlib=libc++ against the
// device's libc++ 3.9 headers) and linked into cinder-probe ONLY.
//
// EVERY CALL BELOW IS A SIGNATURE FROM DISASSEMBLY, UNVERIFIED ON DEVICE (DEVICE_CHECKLIST 26.7).
// See sound_settings_abi.hpp for the evidence behind each one.
//
// Two things this file does that effect_shim.cpp does not, both because of what the disassembly
// shows:
//   * try/catch around every call. The library's getters run `std::stoi` on the service's reply
//     with no guard; an empty reply throws std::invalid_argument, and an exception leaving an
//     `extern "C"` function is std::terminate. catch (...) needs no RTTI, which this build has off.
//   * the object is built lazily and never destroyed, exactly like the effects client: its dtor
//     would run during static teardown, after the looper it talks through has gone.
#include "sound_settings_abi.hpp"
#include "cinder_sound_settings.h"

namespace snd = pst::services::sound;

namespace {
snd::SoundServiceSettingsDmp* g_ss = nullptr;

snd::SoundServiceSettingsDmp* ssc() {
    if (!g_ss) {
        try {
            g_ss = new snd::SoundServiceSettingsDmp();
        } catch (...) {
            g_ss = nullptr;
        }
    }
    return g_ss;
}

// Run a getter or setter: -1 with no client, -2 if the library threw.
template <typename F>
int guarded(F f) {
    snd::SoundServiceSettingsDmp* s = ssc();
    if (!s) return -1;
    try {
        return f(s);
    } catch (...) {
        return -2;
    }
}
} // namespace

extern "C" {

int cinder_sound_settings_get_dsd_filter(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) { return static_cast<int>(s->GetDsdConvFilterType()); });
}
int cinder_sound_settings_set_dsd_filter(int type) {
    return guarded([type](snd::SoundServiceSettingsDmp* s) {
        return s->SetDsdConvFilterType(static_cast<snd::DsdConvFilterType>(type));
    });
}
int cinder_sound_settings_get_dsd_gain(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) { return static_cast<int>(s->GetDsdConvGainMode()); });
}
int cinder_sound_settings_set_dsd_gain(int mode) {
    return guarded([mode](snd::SoundServiceSettingsDmp* s) {
        return s->SetDsdConvGainMode(static_cast<snd::DsdConvGainMode>(mode));
    });
}
int cinder_sound_settings_get_dsd_processing(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) {
        return static_cast<int>(s->GetBuiltinOutputDsdProcessingMode());
    });
}
int cinder_sound_settings_set_dsd_processing(int mode) {
    return guarded([mode](snd::SoundServiceSettingsDmp* s) {
        return s->SetBuiltinOutputDsdProcessingMode(static_cast<snd::DsdProcessingMode>(mode));
    });
}
int cinder_sound_settings_get_uac_dsd_output(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) { return static_cast<int>(s->GetUacOutputDsdOutputMode()); });
}
int cinder_sound_settings_set_uac_dsd_output(int mode) {
    return guarded([mode](snd::SoundServiceSettingsDmp* s) {
        return s->SetUacOutputDsdOutputMode(static_cast<snd::DsdOutputMode>(mode));
    });
}
int cinder_sound_settings_get_lpcm_mode(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) { return static_cast<int>(s->GetLpcmPlaybackMode()); });
}
int cinder_sound_settings_set_lpcm_mode(int mode) {
    return guarded([mode](snd::SoundServiceSettingsDmp* s) {
        return s->SetLpcmPlaybackMode(static_cast<snd::LpcmPlaybackMode>(mode));
    });
}
// The setter is NOT wrapped: it also drives ncasm::NcAsmService (see the abi header).
int cinder_sound_settings_get_headphone_model(void) {
    return guarded([](snd::SoundServiceSettingsDmp* s) { return static_cast<int>(s->GetHeadphoneModel()); });
}

} // extern "C"
