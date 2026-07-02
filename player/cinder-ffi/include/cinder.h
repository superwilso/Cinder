/* cinder.h — C ABI for the Rust Cinder UI (libcinder_ffi.a, glibc armhf).
 * Include from the C++ easel shell (cinder-home). All strings are copied; NULL = empty. */
#ifndef CINDER_H
#define CINDER_H
#ifdef __cplusplus
extern "C" {
#endif

/* Open /dev/graphics/fb0 + init the renderer. 0 = ok, <0 = error. */
int  cinder_render_init(void);
/* Render the current state to the panel; call once per frame from the pump. */
void cinder_render_tick(void);
/* Force the next cinder_render_tick to repaint + blit even if nothing changed. Used to overwrite
 * anything an EXTERNAL process scribbled on the framebuffer (the boot animation's last frame
 * survives its kill — the dirty-flag renderer would otherwise never paint over it). */
void cinder_force_dirty(void);
/* Unmap + tear down. */
void cinder_render_shutdown(void);
/* 0 = day theme, non-zero = night. */
void cinder_set_theme_night(int night);
/* Load + apply persisted UI preferences (theme + visualiser + EQ + sound effects) from `path`, and
 * remember it so later changes auto-save. Call once at boot after cinder_render_init. Returns 1 if a
 * file was read (shell should then re-apply EQ/sound to the DSP), else 0. Garbage/missing = ignored. */
int cinder_settings_load(const char *path);

/* Logical buttons (the backend maps raw evdev key codes -> these). */
typedef enum {
    CINDER_BTN_UP = 0, CINDER_BTN_DOWN = 1, CINDER_BTN_LEFT = 2, CINDER_BTN_RIGHT = 3,
    CINDER_BTN_SELECT = 4, CINDER_BTN_BACK = 5, CINDER_BTN_OPTION = 6, CINDER_BTN_PLAY = 7,
    CINDER_BTN_HOME = 8, CINDER_BTN_VOLUP = 9, CINDER_BTN_VOLDOWN = 10, CINDER_BTN_POWER = 11,
    /* The Hold/lock SWITCH (sustained state, not a press). The shell routes its code to
     * cinder_set_hold(), NOT cinder_input(); this marker just lets the keymap config identify it. */
    CINDER_BTN_HOLD = 12
} cinder_button_t;

/* Actions the shell performs (via cinder-audio etc.) in response to cinder_input(). */
typedef enum {
    CINDER_ACT_NONE = 0, CINDER_ACT_PLAYPAUSE = 1, CINDER_ACT_NEXT = 2, CINDER_ACT_PREV = 3,
    CINDER_ACT_NEXT_ALBUM = 4, CINDER_ACT_PREV_ALBUM = 5, CINDER_ACT_VOLUP = 6,
    CINDER_ACT_VOLDOWN = 7, CINDER_ACT_PLAY_INDEX = 8, CINDER_ACT_SLEEP = 10,
    CINDER_ACT_ENTER_USB_MSC = 11, CINDER_ACT_EQ_CHANGED = 12,
    CINDER_ACT_BATTERY_CARE_CHANGED = 13, CINDER_ACT_SOUND_CHANGED = 14,
    CINDER_ACT_SOUND_BYPASS = 15, CINDER_ACT_THEME_CHANGED = 16,
    /* Device-wide BT transmit codec / LDAC quality changed: read cinder_get_bt_codec +
     * cinder_get_bt_ldac_quality and apply via BtTransmitterService (feeds the LDAC bridge too). */
    CINDER_ACT_BT_CODEC_CHANGED = 17,
    /* USB-DAC toggled: read cinder_get_usb_dac() and start/stop the LDAC bridge + switch the USB
     * gadget to UAC, WITHOUT disconnecting Bluetooth (the headline USB-DAC→LDAC feature). */
    CINDER_ACT_USBDAC_LDAC = 18,
    /* User left the USB mass-storage modal (Back): remount /contents and restore the USB mode. */
    CINDER_ACT_EXIT_USB_MSC = 19
} cinder_action_t;

/* Deliver a button press to the navigator. Theme changes are applied internally; returns a
 * cinder_action_t for the shell to carry out (0 = nothing). */
int  cinder_input(int button);
/* Touchscreen navigation (the NW-A55 has no d-pad). cinder_tap delivers a tap at UI coordinates
 * (x:0..480, y:0..800) — returns a cinder_action_t for the shell to carry out, same as cinder_input.
 * cinder_touch_scroll drag-scrolls the current list by `dy_rows` rows. (The shell maps raw touch
 * coordinates → UI and classifies tap vs scroll vs the left-edge Back swipe.) */
int  cinder_tap(int x, int y);
void cinder_touch_scroll(int dy_rows);
/* Horizontal swipe (dir < 0 = leftward, else rightward) with the gesture's START point in UI
 * coordinates: onboarding pages through, Now Playing skips track, and a rightward swipe on a
 * Library/Album song row queues that song (start y picks the row). Returns a cinder_action_t for
 * the shell to carry out (0 = nothing). */
int  cinder_swipe(int dir, int x, int y);
/* The Hold/lock SWITCH changed state (held = 1 locked, 0 unlocked). When locked the touchscreen is
 * ignored but the transport/volume buttons still work; ONLY the switch going off (held=0) unlocks.
 * Power is a screen on/off toggle (CINDER_ACT_SLEEP) and does NOT unlock. Call on every edge. */
void cinder_set_hold(int held);
/* Open the library DB read-only (e.g. "/db/MTPDB.dat"). Call after cinder_render_init.
 * 0 = ok, -1 = open failed, -2 = renderer not initialised. */
int  cinder_db_open(const char *path);
/* Set now-playing from the track URI PlayerService reports (PlayStatus.uri): resolves
 * title/artist/codec/duration from the DB and derives elapsed/remaining from progress (0..1).
 * 0 = resolved, -1 = not found (falls back to filename), -2 = renderer not initialised. */
int  cinder_set_now_playing_uri(const char *uri, float progress, int playing, int battery);
/* Push the currently-playing track explicitly (progress 0..1, playing 0/1, battery 0..100). */
void cinder_set_now_playing(const char *title, const char *artist, const char *codec,
                            const char *elapsed, const char *remaining,
                            float progress, int playing, int battery);

/* Enable the built-in scrobbler: appends an Audioscrobbler/1.1 `.scrobbler.log` at `path`
 * (e.g. the storage root). `client` is the #CLIENT id. Call after cinder_db_open.
 * 0 = ok, -2 = renderer not initialised. */
int  cinder_scrobble_open(const char *path, const char *client);
/* Advance the scrobbler's play clock one second (call ~1x/sec from the pump). `playing` is
 * 0 paused / non-zero playing. No-op if the scrobbler isn't enabled. */
void cinder_scrobble_tick(int playing);
/* Refresh the status-bar/lock clock from local time (call ~1x/sec). Repaints only on minute change. */
void cinder_clock_tick(void);
/* Push battery percent (0..100) to the status bar; repaints only on change. */
void cinder_set_battery(int pct);
/* Copy the current 10-band EQ gains (dB) into `out` (>= 10 int8). Call after a
 * CINDER_ACT_EQ_CHANGED action, then apply them to the DSP via the effect shim. */
void cinder_get_eq_bands(signed char *out);
/* Battery care (Itawari charging). Push the device's real state into the UI toggle (1 on / 0 off;
 * <0 ignored) — call once at boot after reading PowerMgrServiceClient. */
void cinder_set_battery_care(int on);
/* Push the real storage usage label (e.g. "12.4 / 58 GB") for the Settings Storage row, formatted
 * from statvfs of the music mount. NULL/empty leaves the neutral placeholder. */
void cinder_set_storage(const char *label);
/* Sleep timer: returns 1 ONCE when the user's sleep timer (set in Settings) has just expired — the
 * shell then pauses playback. Poll ~1x/sec from the pump. The countdown itself is internal. */
int  cinder_sleep_should_pause(void);
/* Read the UI's desired battery-care value (1/0). Call after a CINDER_ACT_BATTERY_CARE_CHANGED
 * action, then apply it via the power shim. */
int  cinder_get_battery_care(void);
/* Read the UI's Sound-effect toggles as a bitmask (bit0 DSEE, bit1 Vinyl, bit2 VPT, bit3 DC-Phase,
 * bit4 Dynamic Normalizer, bit5 ClearAudio+). Call after a CINDER_ACT_SOUND_CHANGED action, then
 * apply each via the effect shim. */
int  cinder_get_sound_flags(void);
/* Read the current UI volume as 0..100%. Call after a CINDER_ACT_VOLUP/VOLDOWN action, then scale to
 * the device mixer range and set the hardware volume. */
int  cinder_get_volume(void);
/* Seed the UI volume from the device's real level (0..100%), no HUD pop. Call once at boot after
 * reading the hardware mixer, so the first Vol± press nudges from the actual level. */
void cinder_set_volume(int pct);
/* Is night theme active? (1/0). Call after a CINDER_ACT_THEME_CHANGED action (and at boot) to set
 * the panel backlight — night = minimal light. */
int  cinder_get_night(void);
/* Device-wide BT transmit codec preference (0 LDAC, 1 aptX HD, 2 aptX, 3 SBC) + LDAC quality tier
 * (0 Auto, 1 990, 2 660, 3 330). Read after a CINDER_ACT_BT_CODEC_CHANGED action (and at boot), then
 * apply via BtTransmitterService. The same values configure the USB-DAC→LDAC bridge. */
int  cinder_get_bt_codec(void);
int  cinder_get_bt_ldac_quality(void);
/* Is USB-DAC mode engaged? (1/0). Read after a CINDER_ACT_USBDAC_LDAC action to start/stop the LDAC
 * bridge + switch the USB gadget to UAC, without disconnecting Bluetooth. */
int  cinder_get_usb_dac(void);
/* Read the Sound A/B compare state (1 = B/bypassed, 0 = A/active). Call after a
 * CINDER_ACT_SOUND_BYPASS action, then apply via cinder_effects_set_bypass. */
int  cinder_get_sound_bypass(void);
/* Visualiser: enable/disable the animation, select the type, and query how many types exist. */
void cinder_set_visualizer(int on);
void cinder_set_visualizer_type(int kind);
int  cinder_visualizer_count(void);
/* Feed a mono PCM window (i16) for a REAL audio-reactive visualiser (FFT'd into the bars).
 * Use only for a raw-PCM tap with no analyzer (e.g. the USB-DAC path). */
void cinder_set_pcm(const short *samples, int n);
/* PREFERRED real-data path: feed Sony's already-FFT'd spectrum bands (the int vector from
 * AudioAnalyzerService::OnSpectrumUpdate) — no FFT cost on our side. `n` source bands are
 * resampled into the visualiser bars and auto-normalised. The analyzer shim calls this. */
void cinder_set_spectrum(const int *bands, int n);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_H */
