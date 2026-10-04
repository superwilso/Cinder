/* cinder_sound_settings.h — C ABI over Sony's SoundServiceSettingsDmp
 * (libSoundServiceSettingsDmp.so): the DSD-conversion, LPCM-mode and headphone-model settings that
 * sit beside the effect chain. See cinder-audio/src/sound_settings_abi.hpp for how each signature
 * was read.
 *
 * STATUS: SIGNATURES FROM DISASSEMBLY, UNVERIFIED ON DEVICE (docs/DEVICE_CHECKLIST.md 26.7).
 * Linked into cinder-probe ONLY. cinder-home does not link this library and no screen offers
 * these settings: the enum values are not recovered, there is no DSD file on the reference player
 * to hear a difference with, and a row that writes an unknown enum is exactly the kind of control
 * this project removes rather than ships. `cinder-probe --soundsettings` reads all six and writes
 * nothing.
 *
 * Getters return the value (>= 0), -1 if the settings client could not be built, -2 if the library
 * threw while parsing the service's reply (its own code calls std::stoi unguarded). NOTE: Sony's
 * getter returns 0 both for "the value is 0" and for "the read failed" — a 0 here is not proof of
 * either.
 *
 * Setters return Sony's own status (0 appears to mean success: the getters treat 0 from the
 * sibling GetParams as success), or -1 / -2 as above. Raw ints: no enumerator is known. */
#ifndef CINDER_SOUND_SETTINGS_H
#define CINDER_SOUND_SETTINGS_H
#ifdef __cplusplus
extern "C" {
#endif

/* DSD -> PCM conversion. Catalogue labels: filter "Slow Roll-Off" / "Sharp Roll-Off",
 * gain "0 dB" / "-3 dB". Which label is which value is UNSETTLED. */
int cinder_sound_settings_get_dsd_filter(void);
int cinder_sound_settings_set_dsd_filter(int type);
int cinder_sound_settings_get_dsd_gain(void);
int cinder_sound_settings_set_dsd_gain(int mode);

/* How the built-in output handles DSD, and what the USB output sends for it. */
int cinder_sound_settings_get_dsd_processing(void);
int cinder_sound_settings_set_dsd_processing(int mode);
int cinder_sound_settings_get_uac_dsd_output(void);
int cinder_sound_settings_set_uac_dsd_output(int mode);

/* LPCM playback mode. Meaning unrecovered. */
int cinder_sound_settings_get_lpcm_mode(void);
int cinder_sound_settings_set_lpcm_mode(int mode);

/* Headphone model (Sony's NC headphones). READ ONLY: the setter also reconfigures the
 * noise-cancelling service and is deliberately not wrapped. */
int cinder_sound_settings_get_headphone_model(void);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_SOUND_SETTINGS_H */
