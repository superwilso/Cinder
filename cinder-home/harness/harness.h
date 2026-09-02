// harness.h — the trace + scripting API the off-device harness is built on.
//
// WHY THIS EXISTS. Every defect this project has shipped so far has been a defect in a CALL
// SEQUENCE, not in a value: refresh_bt_paired never called during boot, SetSelectUsingEq never
// called at all, the DSP reconcile skipped when settings had not loaded, four services polled at
// 2 Hz forever. None of those are visible in a unit test of a pure function, and all of them are
// obvious in a trace. So the harness replaces everything main.cpp calls with a stub that RECORDS
// the call — name, one integer argument, and the virtual time it happened at — and lets a test
// script what that call returns.
//
// VIRTUAL TIME. main.cpp is a real-time frame loop: a 16 ms usleep per frame, housekeeping paced
// off the wall clock, retries paced at 1 Hz, BT reconcile over a 15 s window. Running that in real
// time would make a single boot test take half a minute. The harness overrides usleep/sleep/
// clock_gettime/time (see harness.cpp) so the clock only advances when the frame loop sleeps —
// one virtual minute of device time costs a few milliseconds of CPU.
#pragma once
#ifdef __cplusplus
extern "C" {
#endif

// ── the stub side (called by generated stubs and the fakes) ──────────────────────────────────
void cinder_harness_record(const char* name, long long arg);
// The generated stubs' version: `slot` is a static int initialised to -1 at each call site, holding
// the interned name id after the first call. A long scenario records millions of times, and a map
// lookup per call was most of the cost of one.
void cinder_harness_record_cached(int* slot, const char* name, long long arg);
// Same, for a caller that already holds the harness lock. Only the scheduled-filesystem hook does.
void cinder_harness_record_locked(const char* name, long long arg);
// Delay hook, called by the generated stubs straight after they record. `slot` is the interned name
// id the record call just filled in. Guarded by the flag below so the no-delay path is a load.
extern int cinder_harness_delays_armed;
void cinder_harness_delay(int slot);
// Returns 1 and fills *out if the test scripted a return value for this call, else 0.
int  cinder_harness_scripted(const char* name, long long* out);

// The UI's own state store, faked. cinder-ffi remembers what `cinder_set_bt_on(1)` was told, and a
// stub that forgets it makes the app look broken: the boot reconcile reads the switch back as OFF
// on every poll, flips it ON again, and re-reads the radio's pairing table each time — 20 binder
// round trips a minute that the real build never makes. Generated setter/getter PAIRS route through
// this, so the fake UI remembers like the real one. A test that scripts the getter overrides it.
void      cinder_harness_state_set(const char* key, long long value);
long long cinder_harness_state_get(const char* key, long long fallback);

// ── the test side ────────────────────────────────────────────────────────────────────────────
void cinder_harness_reset(void);
// Every call to `name` returns `value`.
void cinder_harness_script(const char* name, long long value);
// The nth call to `name` returns vals[min(n, count-1)] — the last value sticks. That is how a
// service that is not up yet and then comes up is expressed: {-1,-1,0,2} means three failed reads
// then a radio that is on and stays on.
void cinder_harness_script_seq(const char* name, const long long* vals, int count);

// Make every call to `name` COST `ms` of virtual time before it returns, modelling a blocking
// call the app makes on its own render thread. Sony's client proxies are asynchronous underneath —
// a call marshals a request and the reply is delivered by the Framework pump on another thread — so
// on the device a transport call is a real wait, measured at 360-450 ms for SetTrackSequence and
// ~700 ms for a bracketed seek. The stubs return instantly, which quietly made every scenario a
// test of an infinitely fast device: no amount of tapping could ever back the app up. This is how a
// scenario says "and that call takes as long as it does on hardware".
//
// Costs one global load per stub call while no delay is armed, so the long scenarios are unaffected.
void cinder_harness_script_delay(const char* name, long long ms);

int       cinder_harness_count(const char* name);      // how many times it was called
long long cinder_harness_arg(const char* name, int n); // argument of the nth call (0-based)
long long cinder_harness_first_ms(const char* name);   // virtual ms of the first call, -1 if never
long long cinder_harness_last_ms(const char* name);    // virtual ms of the last call, -1 if never
// Calls to `name` between [from_ms, to_ms) — the rate question ("is this still polled at 2 Hz
// twenty seconds in?") asked directly.
int       cinder_harness_count_between(const char* name, long long from_ms, long long to_ms);
// Was `a` first called before `b`? -1 if either never happened. Ordering defects (the DSP
// reconcile running before the settings load) are ordering questions.
int       cinder_harness_before(const char* a, const char* b);
void      cinder_harness_dump(int max_lines);          // the trace, for eyeballing a new scenario

// The calling thread becomes PASSIVE: it waits for virtual time to arrive and contributes nothing
// to when the clock jumps. The harness's own main thread calls this before waiting out the run
// budget — its wake target is the end of the whole scenario, so letting it count would jump the
// clock straight there and skip everything the scenario is about.
void cinder_harness_clock_passive(void);

// ── virtual clock ────────────────────────────────────────────────────────────────────────────
long long cinder_harness_now_ms(void);
// Run the app's lifecycle until the virtual clock reaches `ms`, then finalize. Set before
// cinder_harness_run().
void cinder_harness_set_budget_ms(long long ms);
// Boot the app (fake easel lifecycle -> render_up -> the real frame loop) and return once the
// budget is spent. Everything main.cpp did is in the trace afterwards.
int  cinder_harness_run(void);

// ── a fake filesystem: the device's sysfs, procfs and /contents ──────────────────────────────
// Nearly everything cinder-home knows about its hardware it reads with fopen from an absolute
// path — the battery percentage, whether a charger is attached, whether the headphones are plugged
// in, the persisted settings, the resume queue. On a build machine those paths are simply absent,
// so every scenario runs against a device with a flat battery reading of -1 and nothing plugged in.
//
// These put files where the app will look. `fs_write` creates the file (and its parent
// directories) inside a private tree, and the fopen override serves reads from there; `fs_mkdir`
// makes a directory exist so that a WRITE the app performs succeeds instead of failing. Anything
// not placed there falls through to the real filesystem, which is what makes an absent file still
// mean absent.
void cinder_harness_fs_write(const char* path, const char* content);
void cinder_harness_fs_mkdir(const char* path);
// What the app currently believes is in the file (after its own writes) — how a test checks that
// something was persisted.
int  cinder_harness_fs_read(const char* path, char* buf, int cap);

// The fake DisplayService's current backlight level (see harness.cpp). 0 = the service was told to
// turn the panel fully off; the sysfs node alone does NOT do that on this hardware.
int  cinder_harness_display_backlight(void);
// Change a file PART WAY THROUGH the run, when the virtual clock reaches `at_ms`. The device's
// world is not static — headphones get unplugged, a charger goes in, the battery falls — and
// almost every interesting rule in the app is an EDGE rather than a level (bt_edge.h, jack_edge.h
// exist for exactly this). A scenario cannot make those edges happen from outside, because it is
// blocked inside cinder_harness_run() for the whole session; so it schedules them first.
void cinder_harness_fs_write_at(long long at_ms, const char* path, const char* content);
// Where a path landed inside the private tree, if it is there at all — 1 and fills `out` on a hit.
// The input fake needs it to turn /dev/input/eventN into a real FIFO it can hold the write end of.
int  cinder_harness_fs_resolve(const char* path, const char* mode, char* out, int cap);

// ── fake input (fakeinput.cpp): a touchscreen and a button block ─────────────────────────────
// cinder-home reads /dev/input/event* directly, so without this the whole gesture and button
// surface — every path through carry_out — has no off-device exercise at all. The nodes are real
// FIFOs in the fake tree; the app opens and reads them exactly as it would the driver.
// Call cinder_harness_input_enable() BEFORE cinder_harness_run(): the app opens the nodes once,
// during bring-up. Coordinates are UI coordinates (480x800) — the panel's reported range makes raw
// and UI the same thing on purpose.
void cinder_harness_input_enable(void);
void cinder_harness_key_at(long long at_ms, int code, int value);   // raw evdev code, 1 press/0 release
void cinder_harness_tap_at(long long at_ms, int x, int y);
void cinder_harness_swipe_at(long long at_ms, int x0, int y0, int x1, int y1, long long dur_ms);

// ── the fake radio (fake_pst.cpp), as a test fixture ─────────────────────────────────────────
// The Bluetooth fake is stateful: SetRfOnOff drives what GetBtStatus reports, and a connect only
// takes if the radio is up and something is paired. These set the starting conditions.
void cinder_harness_bt_reset(void);
void cinder_harness_bt_set_radio(int on);
// The service's connect-retry mode: sticky on the device, and while it is on the transmitter
// REFUSES every connect request (measured 2026-08-26). Set it to model a radio someone else left
// armed; read it to assert the app cleared it.
void cinder_harness_bt_set_retry_mode(int on);
int  cinder_harness_bt_retry_mode(void);
void cinder_harness_bt_add_paired(const char* name, int addr_last);
int  cinder_harness_bt_connected(void);
int  cinder_harness_bt_radio_on(void);

#ifdef __cplusplus
}
#endif
