/* discover.h — one-shot, READ-ONLY on-device discovery dump. Captures, in a single run, every
 * unknown that blocks the device-gated features (volume mixer control + range, ALSA topology,
 * backlight sysfs, charge nodes, USB config, input device codes, and the live PlayStatus byte
 * layout). Writes a human-readable report to `path` (e.g. /contents/cinder_discovery.txt) that you
 * pull back over adb / USB-MSC.
 *
 * Shared by BOTH:
 *   - cinder-probe --discover  → full run (with_audio + with_input), isolated, zero boot risk.
 *   - the DEV cinder-home      → static + audio at first boot (with_input=0, since the player owns
 *                                the input pump; raw key codes are logged by the pump instead).
 *
 * SAFETY: everything here is read-only — sysfs/proc reads, `amixer`/`getprop` queries, and a
 * PlayStatus hex dump. It writes ONLY the report file. No device state is changed. */
#ifndef CINDER_DISCOVER_H
#define CINDER_DISCOVER_H
#ifdef __cplusplus
extern "C" {
#endif

/* Run the discovery and write the report to `path`.
 *   with_audio: also dump the live PlayStatus bytes (caller must have cinder_audio_init'd first).
 *   with_input: also do a ~12 s evdev key-code capture (press each button) — only when NO other
 *               input reader is running (the probe; NOT cinder-home, whose pump owns the nodes). */
void cinder_run_discovery(const char *path, int with_audio, int with_input);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_DISCOVER_H */
