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
/* Push the currently-playing track (progress 0..1, playing 0/1, battery 0..100). */
void cinder_set_now_playing(const char *title, const char *artist, const char *codec,
                            const char *elapsed, const char *remaining,
                            float progress, int playing, int battery);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_H */
