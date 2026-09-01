/* eq_range.h — the last clamp before a closed DSP.
 *
 * WHY THIS FILE EXISTS. `cinder-audio` is ~2,500 lines driving every Sony service over
 * hand-recovered vtable offsets, and it has had no tests of any kind (docs/SHORTCOMINGS.md §A2 —
 * "being wrong does not produce a compile error; it produces a mis-marshalled call into a closed
 * service"). Almost none of it can be tested off-device, because almost all of it is IPC. This is
 * the part that is not: a range rule, which is pure arithmetic, and which is the difference
 * between a band being set and a band being silently switched off.
 *
 * THE RULE, and it is not a guess. `SetEq10BandValue` takes HALF-dB units, so the ±20 the EQ
 * screen works in is ±10 dB. A value outside that range does NOT clamp inside the service — it
 * ZEROES the band. Measured, and it is the reason this matters: the failure is silent and it looks
 * like the EQ working, with one band mysteriously flat.
 *
 * WHERE THE OUT-OF-RANGE VALUE COMES FROM. Not the UI: every site in the EQ screen that moves a
 * band already clamps (`cinder_ui::eq::BAND_MAX`, and `value_at_y` clamps after snapping). It
 * comes from the SETTINGS FILE, which lives on `/contents` — vfat, world-writable, and shared with
 * any PC the player is plugged into (SECURITY.md treats it as untrusted everywhere else). The
 * loader parses `i8`, which accepts -128..127.
 *
 * That has been fixed on the Rust side too. This is the second layer deliberately: the Rust clamp
 * protects the UI's own model, and this one protects the SERVICE, which is the thing that cannot
 * defend itself and whose failure mode is silent. A shim standing between an untrusted file and a
 * closed DSP should not be forwarding whatever it is handed.
 *
 * Pure, so cinder-home/tools/eqrange_selftest.cpp checks the SHIPPING rule rather than a copy —
 * the same arrangement as vol_ramp.h, bt_poll.h and frame_budget.h.
 */
#ifndef CINDER_EQ_RANGE_H
#define CINDER_EQ_RANGE_H

/* Half-dB units: ±20 = ±10 dB. Mirrors cinder_ui::eq::BAND_MAX. */
#define CINDER_EQ_BAND_MAX 20

/* Clamp one band gain into the range the service will actually honour. */
static inline int cinder_eq_clamp_gain(int gain) {
    if (gain >  CINDER_EQ_BAND_MAX) return  CINDER_EQ_BAND_MAX;
    if (gain < -CINDER_EQ_BAND_MAX) return -CINDER_EQ_BAND_MAX;
    return gain;
}

/* How many bands of a caller-supplied array to actually send. A negative count is a caller bug
 * rather than an attack, but a negative loop bound is worth refusing explicitly rather than
 * relying on `i < n` happening to be false. */
static inline int cinder_eq_clamp_count(int n) {
    if (n < 0) return 0;
    if (n > 10) return 10;
    return n;
}

#endif /* CINDER_EQ_RANGE_H */
