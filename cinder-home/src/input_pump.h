// input_pump.h — the buttons and the touch panel: volume rocker auto-repeat, the Power button
// (hold for the power menu, tap for the screen) and input_pump(), which reads every evdev device
// once per frame and hands touches and presses to the navigator.
//
// Moved out of main.cpp (2026-11-24). Not self-contained: included once, inside main.cpp's
// anonymous namespace, after the transport queue and the dev command channel.
#pragma once

// ── Volume rocker auto-repeat ───────────────────────────────────────────────────────────────
// Holding the rocker did nothing past the first step. The `val == 2` path in input_pump only ramps
// when the KERNEL generates key repeats, and these evdev nodes don't set EV_REP for the side keys —
// a held button delivers one press and then silence until the release.
//
// So we synthesize the repeat: remember which volume key is down and re-issue its action on a
// timer. Any kernel repeat that does arrive pushes `g_vol_last_ms` forward, so on a unit/firmware
// where EV_REP is set the two can't stack into a double-speed ramp — whichever ticks first wins.
//
// The dead-man cap matters more than it looks: every other button ignores releases (input_pump
// drops `val == 0`), so if a release event is ever lost the ramp would otherwise run forever with
// no finger on the device.
int g_vol_btn = -1;      // CINDER_BTN_VOLUP / _VOLDOWN while held, else -1
long long g_vol_down_ms = 0;  // when the press landed
long long g_vol_last_ms = 0;  // last volume action emitted (kernel repeat OR synthetic)

const long VOL_REPEAT_DELAY_MS = CINDER_VOL_REPEAT_DELAY_MS;  // hold this long before the ramp starts…
const long VOL_REPEAT_EVERY_MS = CINDER_VOL_REPEAT_EVERY_MS;  // …then a step this often (~8/s)
const long VOL_REPEAT_MAX_MS = 15000;    // dead-man: give up if a release was missed

// ACCELERATION. The tick rate stays at 120 ms and the STEP SIZE grows with how long the rocker has
// been held, so a tap is always exactly one step — the precision the fine scales exist for — while
// crossing the whole range stops taking forever.
//
// It is needed because both scales are deliberately fine: the jack is 0..120 and Bluetooth went to
// 0..127 on 2026-08-18 to match AVRCP 1:1. At a flat one step per tick a full sweep is ~15 s. With
// these buckets it is about 5, and the first second and a half is still single steps, which is the
// part of the range anyone actually aims within.
//
// The curve itself lives in src/vol_ramp.h so tools/volramp_selftest.cpp can check the SAME code
// rather than a copy of it; build.sh runs that self-test on the host and refuses to pack if it
// fails. See that header for why it is geometric.

// Replay the lost press that woke the player as Play (see g_keywake_pending). Written into
// icx_key's own node, so it comes back through input_pump like a real press.
static void keywake_tick() {
    if (!g_keywake_pending || now_ms() < g_keywake_due_ms) return;
    g_keywake_pending = false;
    const int fd = g_sidekey_path[0] ? open(g_sidekey_path, O_WRONLY | O_CLOEXEC) : -1;
    ev_event ev[4] = {};
    ev[0].type = EV_KEY_; ev[0].code = 28; ev[0].value = 1;
    ev[2].type = EV_KEY_; ev[2].code = 28; ev[2].value = 0;   // [1] and [3] stay SYN_REPORT
    const bool ok = fd >= 0 && write(fd, ev, sizeof ev) == (ssize_t)sizeof ev;
    if (fd >= 0) close(fd);
    clog_(ok ? "input: the key that woke the player was dropped -> replayed as Play"
             : "input: the key that woke the player was dropped; replay FAILED (no icx_key node)");
}

void vol_repeat_tick() {
    if (g_vol_btn < 0) return;
    long long now = now_ms();
    if (now - g_vol_down_ms > VOL_REPEAT_MAX_MS) { g_vol_btn = -1; return; }
    if (now - g_vol_down_ms < VOL_REPEAT_DELAY_MS) return;
    if (now - g_vol_last_ms < VOL_REPEAT_EVERY_MS) return;
    g_vol_last_ms = now;

    int steps = cinder_vol_repeat_steps((long)(now - g_vol_down_ms));
    // MULTI-STEP IS ONLY SAFE WHERE THE APPLIED VALUE IS ABSOLUTE. The jack path writes a level and
    // is already coalesced; BT with SetCurrentVolume likewise sends the level, so advancing the UI
    // by N and applying once lands exactly right and costs ONE call instead of N. The BT STEP
    // fallback (SetVolumeUp/Down) is RELATIVE — advancing the UI by N there while sending one step
    // would desync the two — so that path stays at one step per tick, which is also why this cannot
    // simply be a faster timer.
    // Bluetooth is ALWAYS relative now: apply_bt_volume sends one SetVolumeUp/Down per call and no
    // absolute write at all (see its comment). Asking bt_use_absolute_volume() here let a sink that
    // CLAIMS absolute support advance the bar N steps per tick while one step went out — seen
    // 2026-11-18 as the bar running ahead of the headphones on a held key, then snapping back.
    const bool absolute = !cinder_get_bt_route();
    if (!absolute) steps = 1;

    int act = CINDER_ACT_NONE;
    for (int i = 0; i < steps; i++) {
        int a = cinder_input(g_vol_btn);
        if (a != CINDER_ACT_NONE) act = a;   // the UI clamps at both ends, so overshoot is harmless
    }
    if (act != CINDER_ACT_NONE) carry_out(act);
}

// ── Power button: HOLD opens the power menu, TAP toggles the screen ──────────────────────────
// Sony's own gesture, and the one the user asked for: hold Power for about a second and a menu
// comes up. Cinder used to act on the PRESS (screen toggle), so no matter how long you held it the
// only outcomes were "screen off" and — past the PMIC's own ~8 s threshold, which is hardware and
// nothing to do with us — a forced reset.
//
// So the press now only starts a clock. Which action fires is decided later:
//   held >= POWER_HOLD_MS  -> open the menu, and the release does NOTHING
//   released before that   -> the screen toggle, exactly as before
// The menu therefore has to open on a TIMER rather than on an event: mtk-kpd's key repeats are not
// something to depend on for a safety-relevant gesture, so power_hold_tick() is called every frame
// from the end of input_pump(), the same way the volume ramp is driven.
// THE ONE ASSUMPTION HERE IS THAT KEY_POWER REPORTS ITS RELEASE. Everything else in this file
// deliberately ignores releases ("releases never act"), and mtk-kpd has never been observed
// reporting one because nothing ever looked. If it does not, deferring the screen toggle to the
// release would make a short Power press do NOTHING — a core function, silently dead.
//
// So the toggle is only deferred once a release has actually been seen. Until then Power behaves
// exactly as it did before (toggle on the press), and the hold menu is disabled. The first real
// release flips this permanently, on the very first press of a boot, and the dev log records both
// edges so the answer is in cinderhome.log rather than in anyone's assumption.
static bool g_power_hold_ok = false;  // have we ever seen a KEY_POWER release on this unit?
static long long g_power_down_ms = 0;      // when Power went down (0 = not down)
static bool g_power_consumed = false; // has this press already produced the menu?
static const long POWER_HOLD_MS = 1000;

void power_hold_tick() {
    if (!g_power_hold_ok || g_power_down_ms == 0 || g_power_consumed) return;
    if (now_ms() - g_power_down_ms < POWER_HOLD_MS) return;
    g_power_consumed = true;   // set FIRST: whatever happens next, this press is spent
    if (cinder_power_held()) {
        clog_("input: Power held — power menu");
        // Waking here matters: holding Power with the panel already dark must show the menu, not
        // put up a dialog nobody can see. The blank is ours (backlight only), so this is a write.
        if (!g_screen_on) screen_toggle();
        g_last_input_ms = now_ms();
    }
    // If the navigator refused (Hold engaged, or a modal already up) the press stays consumed
    // anyway — a refused hold must not fall through to a screen toggle on release either, or
    // holding Power in a pocket would blank/unblank the panel.
}

// Long-press pick-up for an Up Next queue row. Same shape as power_hold_tick: the gesture is
// decided on ELAPSED TIME, and a finger that is holding still emits no events, so there is nothing
// event-driven to hang it on.
//
// Only ever offered to a contact that has claimed NOTHING yet. Every other gesture on this screen
// takes ownership at its start point or at 12 px of travel, so by the time this fires they have all
// declined — and the slop test means a finger already on its way into a scroll cannot be stolen
// mid-gesture. One shot per contact: a refusal (not on a queue row, not on Up Next, locked) must
// not be retried every frame for as long as the finger stays down.
static void reorder_hold_tick() {
    if (!g_touch_down || g_reorder_hold_tested || g_touch_down_ms == 0) return;
    // WHY IT DID NOT FIRE. Reported on device as "no way to reorder NEXT FROM" while the queue's
    // grab handle worked — i.e. the hold path never ran at all, and the log had no lift line to
    // say which gate stopped it. Every other gesture on this screen claims a contact before this
    // one is offered, so naming the claimant is the whole diagnosis. One line per contact.
    if (g_reorder_active || g_drag_active || g_scrub_active || g_sbar_active ||
        g_hswipe_active  || g_shelfswipe_active || g_quickpull_active) {
        if (!g_reorder_hold_tested) {
            g_reorder_hold_tested = true;
            char m[128];
            std::snprintf(m, sizeof m,
                "hold: contact already claimed — drag=%d scrub=%d sbar=%d hswipe=%d shelf=%d reorder=%d",
                (int)g_drag_active, (int)g_scrub_active, (int)g_sbar_active,
                (int)g_hswipe_active, (int)g_shelfswipe_active, (int)g_reorder_active);
            clog_(m);
        }
        return;
    }
    // Not g_touch_saw_pos: every SYN_REPORT clears it, so between frames — where this tick runs —
    // it was always false and no hold ever fired (found on the player 10-24, hold_play_next).
    if (g_touch_start_x < 0 || g_touch_start_y < 0) return;
    const int dx = touch_ui_x(g_touch_cur_x) - touch_ui_x(g_touch_start_x);
    const int dy = touch_ui_y(g_touch_cur_y) - touch_ui_y(g_touch_start_y);
    if ((dx < 0 ? -dx : dx) > REORDER_HOLD_SLOP || (dy < 0 ? -dy : dy) > REORDER_HOLD_SLOP) {
        g_reorder_hold_tested = true;   // it moved: this contact is a scroll, not a hold
        char m[96];
        std::snprintf(m, sizeof m, "hold: moved %d,%d px before %lld ms — treated as a scroll",
                      dx, dy, now_ms() - g_touch_down_ms);
        clog_(m);
        return;
    }
    if (now_ms() - g_touch_down_ms < REORDER_HOLD_MS) return;
    g_reorder_hold_tested = true;       // set FIRST: whatever the answer, this contact has asked
    const int hx = touch_ui_x(g_touch_start_x), hy = touch_ui_y(g_touch_start_y);
    if (cinder_reorder_begin_hold(hx, hy)) {
        g_reorder_active = true;
        clog_("touch: row lifted by long press");
        // Paint now. The lifted row IS the feedback that the hold registered, and waiting for the
        // next event would leave the screen unchanged until the finger moved — which is precisely
        // the moment the user is deciding whether the gesture worked.
        if (g_screen_on) cinder_render_tick();
        return;
    }
    // A hold on a folder row is "Add to playlist" (#56), always; on a song row it is Play next,
    // opt-in. The UI has already done it and shown the toast; it returns an action code (or NONE)
    // for the shell, and -1 when the row answers no hold. This used to skip cinder_hold entirely
    // without the switch, which is how the folder hold never fired on the owner's player (11-22).
    cinder_set_hold_play_next(adv_on("hold_play_next") ? 1 : 0);
    const int act = cinder_hold(hx, hy);
    if (act >= 0) {
        g_hold_queued = true;
        clog_("touch: long press acted (folder -> playlist, or song -> play next)");
        if (act != CINDER_ACT_NONE) carry_out_input(act);
        if (g_screen_on) cinder_render_tick();
    } else {
        char m[96];
        std::snprintf(m, sizeof m, "hold: UI declined a lift at %d,%d (not a reorderable row?)", hx, hy);
        clog_(m);
    }
}

#ifdef CINDER_DEV
// Consume a dev request-file: true if one was waiting. Handles the two ways /tmp defeats us —
// cinder-home is uid `system`, /tmp is sticky (drwxrwxrwt) and `adb shell echo >` creates files
// root-owned 0600, so an unlink() by a non-owner is EPERM and a request that cannot be removed
// re-fires on every housekeeping tick forever (observed 2026-07-28: one echo, fifty seek probes).
// Falls back to truncating, and treats an empty file as "nothing to do", so a request is consumed
// exactly once whether or not we own it. `out` receives the first line, if the caller wants it.
static bool take_req(const char* path, char* out, size_t cap) {
    struct stat st;
    if (::stat(path, &st) != 0 || st.st_size == 0) return false;
    bool read_ok = false;
    if (out && cap) {
        out[0] = 0;
        if (FILE* f = std::fopen(path, "r")) {
            if (std::fgets(out, (int)cap, f)) read_ok = true;
            std::fclose(f);
        }
    } else {
        read_ok = true;   // caller only cares that the file was there
    }
    if (::unlink(path) != 0) {
        if (FILE* t = std::fopen(path, "w")) std::fclose(t);   // truncate = consumed
    }
    if (!read_ok) {
        char m[160];
        std::snprintf(m, sizeof m, "req: %s exists but is UNREADABLE by us (chmod 666 it) — ignored", path);
        clog_(m);
    }
    return read_ok;
}
#endif  // CINDER_DEV — every caller of take_req is inside one, so the definition is too

void input_pump() {
    ev_event evs[32];
    static long g_ev_total = 0;   // events ever seen (any node) — for the silent-input heartbeat
    // HEARTBEAT: if the input system is silent (foreign grab / dead driver), say so in the log
    // instead of leaving "no events" indistinguishable from "nobody touched it".
    //
    // PACED BY THE WALL CLOCK, AND BACKING OFF. It used to be `++calls % 450`, which is the same
    // assumption the housekeeping block below was explicitly fixed for: the loop is 60 Hz awake and
    // 1 Hz with the panel dark, so "every 450 calls" meant every 7.5 s awake and every 7.5 MINUTES
    // dark — neither of them the ~15 s the comment claimed. And a genuinely dead input system never
    // starts producing events, so it repeated for ever: measured off-device (cinder-home/harness),
    // 499 lines an hour, each one an fflush to /contents. The condition it reports does not change,
    // so after the first few the only useful thing it can do is stop.
    static long long hb_next_ms = 0;
    static long hb_gap_ms  = 15000;
    if (g_ev_total == 0) {
        const long long hb_now = now_ms();
        if (hb_next_ms == 0) hb_next_ms = hb_now + hb_gap_ms;
        else if (hb_now >= hb_next_ms) {
            // Worded for the common case. On a player nobody has touched yet this is the truth, and
            // "foreign grab?" made every quiet boot read as a fault (audits 2026-08-16, 11-11).
            clog_("input: no events yet from any node (untouched so far; if it IS being touched, "
                  "see the node diagnostics above)");
            // 15 s, 1 min, 4 min, 16 min, then hourly. Clamped, or the last step would land at
            // 64 minutes and quietly not be the "hourly" this says — the same clamp retry_log has.
            if (hb_gap_ms < 3600000) hb_gap_ms *= 4;
            if (hb_gap_ms > 3600000) hb_gap_ms = 3600000;
            hb_next_ms = hb_now + hb_gap_ms;
        }
    }
    for (int i = 0; i < g_evn; ++i) {
        int rounds = 0;
        for (;;) {
            ssize_t n = read(g_evfds[i], evs, sizeof evs);
            if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
                static bool logged_err[16] = {};
                if (i < 16 && !logged_err[i]) {
                    logged_err[i] = true;
                    char m[80];
                    std::snprintf(m, sizeof m, "input: read fd%d failed errno=%d%s", i, errno,
                                  (g_evfds[i] == g_touch_fd) ? " [TOUCH]" : "");
                    clog_(m);
                }
            }
            if (n <= 0) break;
            int cnt = (int)(n / (ssize_t)sizeof(ev_event));
            g_ev_total += cnt;
            for (int k = 0; k < cnt; ++k) {
                uint16_t type = evs[k].type, code = evs[k].code;
                int val = evs[k].value;
#ifdef CINDER_DEV
                // DEV: log the first ~200 raw input events (with the touch fd marked) so the exact
                // touchscreen protocol is visible in cinderhome.log if gestures still misbehave.
                {
                    static int g_evlog = 0;
                    if (g_evlog < 200) {
                        char m[80];
                        std::snprintf(m, sizeof m, "input: ev fd%d%s type=0x%x code=0x%x val=%d", i,
                                      (g_evfds[i] == g_touch_fd) ? "*TS" : "", type, code, val);
                        clog_(m); ++g_evlog;
                    }
                }
#endif
                // ── Touchscreen navigation (no d-pad on the NW-A55) — ONLY from the real touch fd,
                // so the sensor-batch node's ABS_X/Y can't masquerade as finger coordinates. Robust
                // to BTN_TOUCH panels AND type-A MT (contact begins on the first position; a lift is
                // BTN_TOUCH=0 or a SYN_REPORT frame that carried no position).
                if (g_evfds[i] == g_touch_fd) {
                    // Panel dark: taps must not navigate invisibly. Stock sleeps the controller;
                    // this fw has no sleep node (see touch_set_sleep), so the events keep coming —
                    // drop them and any in-flight contact until screen-on.
                    //
                    // EXCEPT when the blank was the idle timer's: then this touch is the wake
                    // gesture. It wakes the panel and is CONSUMED (not delivered), so waking can
                    // never also activate whatever happens to be under the finger. A Power-button
                    // blank is left alone — that one stays off until Power is pressed again.
                    if (!g_screen_on) {
                        // Hold engaged = in a pocket: do NOT let stray contact wake the panel, or
                        // an idle blank would be undone by the first thing it brushes against and
                        // the battery saving is lost exactly when it matters most. Keys still wake
                        // (the buttons are usable under Hold by design), and so does the switch
                        // itself. Note this deliberately does not refresh the idle clock either.
                        if (!g_held) {
                            g_last_input_ms = now_ms();
                            if (g_screen_auto_off) screen_auto_wake();
                        }
                        g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1;
                        g_touch_saw_pos = false;
                        g_drag_active = false; g_drag_vel = 0.0f;
                        g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
    g_shelfswipe_active = false; g_shelfswipe_tested = false;
    g_quickpull_active = false; g_quickpull_tested = false;
                            g_touch_down_ms = now_ms(); g_reorder_hold_tested = false; g_hold_queued = false;
                        // AND SWALLOW THE REST OF THIS CONTACT. Clearing the state above is not
                        // enough: the finger is still on the glass and the panel keeps streaming
                        // positions, but `screen_auto_wake` has already set g_screen_on — so the
                        // NEXT position event falls through to the handler below, finds
                        // !g_touch_down, and synthesises a brand-new contact mid-gesture. The lift
                        // then reads as a genuine tap wherever the finger happened to be, which is
                        // exactly the "tap to wake also presses the button underneath" report
                        // (2026-08-17). The latch clears on the real lift, so the NEXT touch works
                        // normally.
                        g_touch_wake_swallow = true;
                        continue;
                    }
                    // Still inside the contact that woke the panel: drop everything until it lifts.
                    if (g_touch_wake_swallow) {
                        if (type == EV_ABS_ && (code == ABS_X_ || code == ABS_MT_POSITION_X_
                                             || code == ABS_Y_ || code == ABS_MT_POSITION_Y_)) {
                            g_touch_saw_pos = true;
                            continue;
                        }
                        // A lift is BTN_TOUCH=0, or (type-A) a SYN frame that carried no position.
                        if (type == EV_KEY_ && code == BTN_TOUCH_ && !val) {
                            g_touch_wake_swallow = false;
                            g_touch_saw_pos = false;
                            continue;
                        }
                        if (type == EV_SYN_ && code == 0) {
                            if (!g_touch_saw_pos) g_touch_wake_swallow = false;
                            g_touch_saw_pos = false;
                            continue;
                        }
                        continue;
                    }
                    // Live touch = activity, so the idle blank holds off — but NOT while Hold is
                    // engaged: those contacts are pocket noise (nav ignores them anyway), and
                    // letting them count would keep the Lock screen lit in a pocket indefinitely.
                    if (!g_held) g_last_input_ms = now_ms();
                    if (type == EV_ABS_ && (code == ABS_X_ || code == ABS_MT_POSITION_X_)) {
                        g_touch_cur_x = val; g_touch_saw_pos = true;
                        if (!g_touch_down) {
                            g_touch_down = true; g_touch_start_x = val; g_touch_start_y = -1;
                            g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
    g_shelfswipe_active = false; g_shelfswipe_tested = false;
    g_quickpull_active = false; g_quickpull_tested = false;
                            g_touch_down_ms = now_ms(); g_reorder_hold_tested = false; g_hold_queued = false;
                            cinder_touch_down();   // finger down stops an in-flight fling
                        } else if (g_touch_start_x < 0) g_touch_start_x = val;
                        // Also drive the classifier from HERE, not only from ABS_Y: a panel that
                        // reports Y before X in a contact's first frame would otherwise never get
                        // classified (at Y time g_touch_down is still false, so touch_drag_motion
                        // returns early, and nothing calls it again for a stationary tap). Safe to
                        // call twice per frame — it no-ops unless the contact has both coordinates,
                        // and the drag branch ignores a zero y-delta.
                        touch_drag_motion();
                        continue;
                    }
                    if (type == EV_ABS_ && (code == ABS_Y_ || code == ABS_MT_POSITION_Y_)) {
                        g_touch_cur_y = val; g_touch_saw_pos = true;
                        if (g_touch_down && g_touch_start_y < 0) g_touch_start_y = val;
                        touch_drag_motion();       // live drag: stream deltas + velocity
                        continue;
                    }
                    if (type == EV_KEY_ && code == BTN_TOUCH_) {
                        if (val) {
                            if (!g_touch_down) {
                                g_touch_down = true; g_touch_start_x = -1; g_touch_start_y = -1;
                                g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
    g_shelfswipe_active = false; g_shelfswipe_tested = false;
    g_quickpull_active = false; g_quickpull_tested = false;
                            g_touch_down_ms = now_ms(); g_reorder_hold_tested = false; g_hold_queued = false;
                                cinder_touch_down();
                            }
                        } else {
                            touch_release();
                        }
                        continue;
                    }
                    if (type == EV_SYN_ && code == 0) {   // SYN_REPORT: empty frame while down = lift (type-A)
                        if (g_touch_down && !g_touch_saw_pos) touch_release();
                        g_touch_saw_pos = false;
                        continue;
                    }
                }

                // ── Hold/lock SWITCH ── sustained state, both edges (val 1 = locked, 0 = off).
                // Reported as EV_KEY or EV_SW depending on the unit; we match by the code the keymap
                // labels CINDER_BTN_HOLD. The switch is the ONLY thing that unlocks (Power just
                // toggles the screen). Default keymap doesn't know the code — drop `<code> 12` into
                // /contents/cinder_keymap.conf from the dev keycode log.
                if ((type == EV_KEY_ || type == EV_SW_) && code < keymap_size()
                        && g_keymap[code] == CINDER_BTN_HOLD) {
                    // ESCAPE from a level-0 backlight. Here rather than with the other buttons
                    // because this branch `continue`s — the switch never reaches them.
                    brightness_wake_on_input();
                    clog_(val ? "hold: switch on" : "hold: switch off");   // 11-09: a boot lock left no trace of its unlock
                    cinder_set_hold(val ? 1 : 0);
                    g_held = (val != 0);     // the auto-wake path needs it (pocket-safety, below)
                    g_last_input_ms = now_ms();   // flicking the switch is deliberate activity
                    g_vol_btn = -1;          // locking mid-hold must not keep ramping
                    // Hold OFF lights the screen (#51): taking the player out of Hold means you are
                    // about to use it, and it used to stay dark until Power.
                    if (!val && !g_screen_on) {
                        clog_("hold: off -> screen on");
                        run_guarded("hold off: screen on", 8, screen_toggle);
                    }
                    continue;
                }

                // ── Buttons ──
                // The volume rocker is the one button whose RELEASE means something: it ends the
                // synthesized ramp (see vol_repeat_tick). Checked before the release filter below.
                if (type == EV_KEY_ && val == 0 && code < keymap_size()
                        && (g_keymap[code] == CINDER_BTN_VOLUP
                            || g_keymap[code] == CINDER_BTN_VOLDOWN)) {
                    if (g_vol_btn == g_keymap[code]) g_vol_btn = -1;
                    continue;
                }
                // POWER RELEASE — the second button whose release means something. A press that
                // was NOT long enough to open the menu becomes the screen toggle here, on the way
                // up. Doing it on the way up is the whole mechanism: on the way down we cannot yet
                // know whether this is a tap or a hold.
                if (type == EV_KEY_ && val == 0 && code < keymap_size()
                        && g_keymap[code] == CINDER_BTN_POWER) {
                    bool was_down = g_power_down_ms != 0, spent = g_power_consumed;
                    // Was the toggle ALREADY deferred to the release when this press went down?
                    // On the very first Power press of a boot it was not — the press itself
                    // toggled the screen (the fallback below), so toggling again here would undo
                    // it and Power would appear dead exactly once per boot.
                    bool deferred = g_power_hold_ok;
                    g_power_hold_ok = true;   // this unit DOES report the release — see below
#ifdef CINDER_DEV
                    {
                        char m[80];
                        std::snprintf(m, sizeof m, "input: POWER release after %lld ms (spent=%d)",
                                      was_down ? now_ms() - g_power_down_ms : -1LL, (int)spent);
                        clog_(m);
                    }
#endif
                    g_power_down_ms = 0; g_power_consumed = false;
                    if (deferred && was_down && !spent) {
                        int act = cinder_input(CINDER_BTN_POWER);   // -> CINDER_ACT_SLEEP
                        if (act != CINDER_ACT_NONE) carry_out_input(act);
                    }
                    continue;
                }
                if (type != EV_KEY_ || val == 0) continue; // releases never act
                soc_suspend_hold_for_input();               // FIRST: see its comment
                int kc = code;
                if (val == 1 && (kc == 201 || kc == 166)) g_peer_pause_ms = now_ms();
                // Key REPEATS (val=2) only ramp the volume rocker; for everything else a held
                // button is ONE action (a held FF must not machine-gun track skips).
                if (val == 2 && kc < keymap_size()
                        && g_keymap[kc] != CINDER_BTN_VOLUP && g_keymap[kc] != CINDER_BTN_VOLDOWN)
                    continue;
#ifdef CINDER_DEV
                // DEV CHANNEL: log every key code (mapped or not) so the physical-button → keycode map
                // can be read straight from cinderhome.log — press each button and watch the log.
                if (val == 1) {
                    char m[64];
                    std::snprintf(m, sizeof m, "input: KEY code=%d (0x%x) -> btn=%d", kc, kc,
                                  (kc >= 0 && kc < keymap_size()) ? g_keymap[kc] : -2);
                    clog_(m);
                }
#endif
                if (kc < 0 || kc >= keymap_size()) continue;
                int btn = g_keymap[kc];
                if (btn < 0) continue;
                // Settings ▸ Hold locks the buttons (#51): under Hold the player's own side keys
                // (play, FF, REW, volume) do nothing and wake nothing. Power still toggles the
                // screen, and headphone buttons arrive as other codes, so they still work.
                if (g_held && (kc == 28 || kc == 105 || kc == 106 || kc == 114 || kc == 115)
                        && cinder_get_hold_buttons())
                    continue;
                // Wampy's "swap prev/next" (`swap_prev_next`), for a hand that holds the player
                // the other way up. The side keys only: headphones' own buttons keep their meaning.
                // Read at the press, not at startup, because /contents can be unreadable then.
                if ((kc == 105 || kc == 106) && (btn == CINDER_BTN_NEXT || btn == CINDER_BTN_PREV)
                        && adv_on("swap_prev_next"))
                    btn = btn == CINDER_BTN_NEXT ? CINDER_BTN_PREV : CINDER_BTN_NEXT;
                // PAUSE and STOP only ever pause. On the play/pause button they would toggle, and a
                // headset that sends PAUSE to a player that is already paused would start it.
                if ((kc == 201 || kc == 166) && btn == CINDER_BTN_PLAY
                        && !(g_playing && cinder_audio_is_playing())) {
                    clog_("input: PAUSE from the headphones while nothing is playing — ignored");
                    continue;
                }
                // Any mapped key press counts as activity for the idle blank, but does NOT wake
                // it (owner 11-19: Play lit the screen). The physical buttons do the same thing
                // whether or not you can see the screen, like a pocket player; Power or a touch
                // wakes it. Power needs no special case: cinder_input maps it to
                // CINDER_ACT_SLEEP -> screen_toggle, which takes ownership of the panel state.
                if (val == 1) {
                    g_last_input_ms = now_ms();
                    g_keywake_pending = false;   // the driver delivered a key itself: nothing to replay
                    // The second escape from a level-0 backlight, and it must be taken BEFORE the
                    // Power block below — that one `continue`s once the hold gesture is armed, so
                    // a call placed after it would never run on a normal press.
                    if (btn == CINDER_BTN_POWER) brightness_wake_on_input();
                }
                if (btn == CINDER_BTN_VOLUP || btn == CINDER_BTN_VOLDOWN) {
                    if (val == 1) { g_vol_btn = btn; g_vol_down_ms = now_ms(); }
                    g_vol_last_ms = now_ms();   // also swallows a kernel repeat's slot
                }
                // POWER PRESS starts the hold clock and does NOTHING ELSE. The screen toggle now
                // happens on the release (above), and the menu on the timer (power_hold_tick), so
                // acting here as well would fire both. Kernel repeats are ignored — the clock is
                // already running and re-arming it would push the hold threshold out forever.
                if (btn == CINDER_BTN_POWER) {
                    if (val == 1 && g_power_down_ms == 0) {
                        g_power_down_ms = now_ms();
                        g_power_consumed = false;
                    }
                    // Until a release has been seen on this unit, fall through and toggle the
                    // screen on the PRESS exactly as before — see g_power_hold_ok. This costs the
                    // hold gesture on the first press of a boot and nothing else.
                    if (g_power_hold_ok) continue;
                }
                int act = cinder_input(btn);
                if (act != CINDER_ACT_NONE) carry_out_input(act);
            }
            if (n < (ssize_t)sizeof evs) break; // drained this node
            // …AND NEVER MORE THAN THIS MANY ROUNDS IN ONE FRAME. The loop above ends when a read
            // comes back short, which assumes the app drains a node faster than the node fills it.
            // A blocking call reached from in here breaks that assumption, and the loop then runs
            // for as long as the user keeps touching the glass rather than for as long as there are
            // events (see the transport queue above for the case that made it visible). Nothing is
            // lost by stopping: the rest stays in the kernel's buffer and is read on the next
            // frame. 8 rounds is 256 events from one node in a single frame, against the handful a
            // 100 Hz panel produces in 16 ms — so a device that is keeping up never reaches it, and
            // one that is not drains 15,000 events a second, which no input device approaches.
            if (++rounds >= 8) break;
        }
    }
    // Phone remote (Settings > Wi-Fi > Phone remote): its presses take the side keys' own path.
    for (int b, k = 0; k < 8 && (b = cinder_remote_take_button()) > 0; k++) {
        int act = cinder_input(b);
        if (act != CINDER_ACT_NONE) carry_out_input(act);
    }
    // ...and what it asked for from the library (play, queue, jump, like): one per frame, so a
    // burst of taps on the phone cannot hold the loop.
    if (int act = cinder_remote_take_action()) carry_out_input(act);
    // Held rocker: the events are all drained, so anything still down is a genuine hold.
    vol_repeat_tick();
    keywake_tick();
    // Banked slider/rail applies and the release seek — deliberate gestures, so ahead of a
    // backlog. Same reason as the line below: a Sony round trip must not be made from the drain.
    scrub_tick();
    // Banked transport steps, one per frame — the same shape as the rocker above, and for the same
    // reason: a Sony round trip must not be made from inside the drain. See queue_transport.
    transport_tick();
    // Same idea for Power: the menu opens on elapsed time, not on an event, so it needs a tick.
    power_hold_tick();
    // Same reason, for the queue's long-press reorder: a still finger emits no events.
    reorder_hold_tick();
    // A level-0 blank is ended by the HOLD switch or POWER only — see brightness_wake_on_input.
    // It used to end on ANY input, which made "backlight off" last exactly until the next touch:
    // the pocket case (screen dark, music playing) was the one it could not do, because a stray
    // touch through fabric lit the panel straight back up.
}
