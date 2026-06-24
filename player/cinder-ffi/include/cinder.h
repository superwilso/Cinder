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
/* Unmap + tear down. */
void cinder_render_shutdown(void);
/* 0 = day theme, non-zero = night. */
void cinder_set_theme_night(int night);

/* Logical buttons (the backend maps raw evdev key codes -> these). */
typedef enum {
    CINDER_BTN_UP = 0, CINDER_BTN_DOWN = 1, CINDER_BTN_LEFT = 2, CINDER_BTN_RIGHT = 3,
    CINDER_BTN_SELECT = 4, CINDER_BTN_BACK = 5, CINDER_BTN_OPTION = 6, CINDER_BTN_PLAY = 7,
    CINDER_BTN_HOME = 8, CINDER_BTN_VOLUP = 9, CINDER_BTN_VOLDOWN = 10, CINDER_BTN_POWER = 11
} cinder_button_t;

/* Actions the shell performs (via cinder-audio etc.) in response to cinder_input(). */
typedef enum {
    CINDER_ACT_NONE = 0, CINDER_ACT_PLAYPAUSE = 1, CINDER_ACT_NEXT = 2, CINDER_ACT_PREV = 3,
    CINDER_ACT_NEXT_ALBUM = 4, CINDER_ACT_PREV_ALBUM = 5, CINDER_ACT_VOLUP = 6,
    CINDER_ACT_VOLDOWN = 7, CINDER_ACT_PLAY_INDEX = 8, CINDER_ACT_SLEEP = 10,
    CINDER_ACT_ENTER_USB_MSC = 11
} cinder_action_t;

/* Deliver a button press to the navigator. Theme changes are applied internally; returns a
 * cinder_action_t for the shell to carry out (0 = nothing). */
int  cinder_input(int button);
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

#ifdef __cplusplus
}
#endif
#endif /* CINDER_H */
